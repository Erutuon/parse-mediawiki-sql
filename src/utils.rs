/*!
Defines [`memory_map`] to read decompressed MediaWiki SQL files,
and [`NamespaceMap`] to display a page title prefixed by its namespace name.
*/

use crate::field_types::{PageNamespace, PageTitle};

use std::{
    collections::HashMap as Map,
    fmt::Write,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use thiserror::Error;
use unicase::UniCase;

pub use memmap2::Mmap;

/**
Memory-maps a file, returning a useful message in case of error.

Pass a borrowed memory map to [`iterate_sql_insertions`](crate::iterate_sql_insertions) so that the [schema](crate::schemas) struct
produced by the iterator can borrow from the file's contents. See the [example](crate#example) in the crate documentation.

# Errors
In case of error, returns [`Error::Io`] containing the action that failed, the path, and the underlying [`std::io::Error`].

# Safety

Inherits unsafe annotation from [`Mmap::map`].
*/
pub unsafe fn memory_map<P: AsRef<Path>>(path: P) -> Result<Mmap, Error> {
    let path = path.as_ref();
    Mmap::map(&File::open(path).map_err(|source| Error::from_io("open file", source, path))?)
        .map_err(|source| Error::from_io("memory map file", source, path))
}

/// The error type used by [`memory_map`] and [`NamespaceMap`].
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to parse JSON at {}", path.canonicalize().as_ref().unwrap_or(path).display())]
    JsonFile {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("Failed to parse JSON")]
    Json { source: serde_json::Error },
    #[error("Failed to {action} at {}", path.canonicalize().as_ref().unwrap_or(path).display())]
    Io {
        action: &'static str,
        source: std::io::Error,
        path: PathBuf,
    },
}

impl Error {
    pub fn from_io<P: Into<PathBuf>>(
        action: &'static str,
        source: std::io::Error,
        path: P,
    ) -> Self {
        Error::Io {
            action,
            source,
            path: path.into(),
        }
    }
}

/// Deserializes `siteinfo-namespaces.json`. See [API:Siteinfo](https://www.mediawiki.org/wiki/API:Siteinfo)
/// on MediaWiki for more information.
#[derive(Debug, Deserialize)]
struct Response {
    query: Query,
}

/// The single field of [`Response`] that [`NamespaceMap`] cares about.
#[derive(Debug, Deserialize)]
struct Query {
    namespaces: Map<String, NamespaceInfo>,
}

/// The single field of [`Query`] that [`NamespaceMap`] cares about.
#[derive(Debug, Deserialize)]
struct NamespaceInfo {
    id: i32,
    #[serde(rename = "*")]
    name: String,
    #[serde(rename = "canonical")]
    canonical_name: Option<String>,
}

/// Provides a way to convert a namespace and title from `page.sql` to the title displayed on the wiki page.
#[derive(Debug)]
pub struct NamespaceMap(Map<PageNamespace, String>);

impl NamespaceMap {
    /// Creates a `NamespaceMap` by parsing `siteinfo-namespaces.json.gz` or `siteinfo-namespaces.json`
    /// and converting it into a map from namespace number to local namespace name.
    /// If the last file extension is `gz`, decompresses from the GZip format before decoding the JSON.
    pub fn from_path(path: &Path) -> Result<Self, Error> {
        let json = if path.extension() == Some("gz".as_ref()) {
            let gz =
                File::open(path).map_err(|source| Error::from_io("open file", source, path))?;
            let mut decoder = GzDecoder::new(gz);
            let mut decoded = String::new();
            decoder
                .read_to_string(&mut decoded)
                .map_err(|source| Error::from_io("parse GZip", source, path))?;
            decoded
        } else {
            std::fs::read_to_string(path)
                .map_err(|source| Error::from_io("read file to string", source, path))?
        };
        Self::from_json_with_path(&json, Some(path))
    }

    pub fn from_json<S: AsRef<str>>(json: S) -> Result<Self, Error> {
        Self::from_json_with_path(json.as_ref(), None)
    }

    fn from_json_with_path(json: &str, path: Option<&Path>) -> Result<Self, Error> {
        Ok(Self(
            serde_json::from_str::<Response>(&json)
                .map_err(|source| {
                    if let Some(path) = path {
                        Error::JsonFile {
                            source,
                            path: path.into(),
                        }
                    } else {
                        Error::Json { source }
                    }
                })?
                .query
                .namespaces
                .into_iter()
                .map(|(_, namespace_info)| (PageNamespace(namespace_info.id), namespace_info.name))
                .collect(),
        ))
    }

    /// Returns a full title, consisting of the [`PageNamespace`] converted to the local namespace name,
    /// a colon if needed, and the [`PageTitle`] with underscores replaced with spaces.
    ///
    /// # Panics
    /// Panics if the `PageNamespace` is not found in the `NamespaceMap`.
    pub fn readable_title(
        &self,
        PageTitle(title): &PageTitle,
        namespace: &PageNamespace,
    ) -> String {
        self.0
            .get(&namespace)
            .map(|n| {
                // This allocates 1 extra byte when `n` is empty.
                let mut readable_title = String::with_capacity(
                    if n.is_empty() { 0 } else { n.len() + ":".len() } + title.len(),
                );
                if !n.is_empty() {
                    write!(readable_title, "{}:", n).unwrap();
                }
                for c in title.chars() {
                    write!(readable_title, "{}", if c == '_' { ' ' } else { c }).unwrap();
                }
                readable_title
            })
            .unwrap()
    }

    /// Get the [`PageNamespace`] corresponding to a candidate namespace name.
    /// Compares names case-insensitively with underscores treated as equal to spaces.
    /// Returns `None` if there is not a match.
    pub fn id<S: AsRef<str>>(&self, name_candidate: S) -> Option<PageNamespace> {
        // The maximum length of a namespace name.
        // Actually the maximum length of a title. Should be more than long enough for a namespace name.
        const MAX_NAMESPACE: usize = 256;

        // Normalizes a namespace name by converting a sequence of one or more spaces and underscores to a single space.
        // Returns `None` if `name` is too long to fit in the buffer.
        pub fn normalize_name<'a>(name: &str, buffer: &'a mut [u8]) -> Option<&'a str> {
            use std::io::Write;
            let mut writer = std::io::Cursor::new(buffer);
            let mut last_was_whitespace = false;
            let mut len = 0;
            for c in name.chars() {
                if c == '_' || c == ' ' {
                    if !last_was_whitespace {
                        write!(writer, " ").ok()?;
                        len += 1;
                    }
                    last_was_whitespace = true;
                } else {
                    write!(writer, "{}", c).ok()?;
                    len += c.len_utf8();
                    last_was_whitespace = false;
                }
            }
            Some(
                std::str::from_utf8(&writer.into_inner()[..len])
                    .expect("the loop can only produce valid UTF-8 because it writes characters"),
            )
        }

        let name_candidate = name_candidate.as_ref();
        let mut buffer = [0u8; MAX_NAMESPACE];
        normalize_name(name_candidate, &mut buffer).and_then(|normalized| {
            let normalized = UniCase::new(normalized);
            self.0.iter().find_map(|(&id, name)| {
                if UniCase::new(name) == normalized {
                    Some(id)
                } else {
                    None
                }
            })
        })
    }
}

#[test]
fn test_namespace_map_id() {
    let namespaces = NamespaceMap::from_json(
        r#"
        {
            "query": {
                "namespaces": {
                    "-2":   {"id": -2, "*": "Media"},
                    "-1":   {"id": -1, "*": "Special"},
                     "0":   {"id":  0, "*": ""},
                     "1":   {"id":  1, "*": "Talk"},
                     "8":   {"id":  8, "*": "MediaWiki"},
                     "9":   {"id":  9, "*": "MediaWiki talk"},
                    "10":   {"id": 10, "*": "Šablona"},
                    "11":   {"id": 11, "*": "Diskuse k šabloně"}
                }
            }
        }
        "#,
    )
    .unwrap();
    assert_eq!(namespaces.id("Media"), Some(PageNamespace(-2)));
    assert_eq!(namespaces.id(""), Some(PageNamespace(0)));
    assert_eq!(namespaces.id("Talk"), Some(PageNamespace(1)));
    assert_eq!(
        namespaces.id("mediawiki".to_string() + &" ".repeat(128) + &"_".repeat(128) + "talk"),
        Some(PageNamespace(9))
    );
    assert_eq!(namespaces.id("šablona"), Some(PageNamespace(10)));
    assert_eq!(namespaces.id("ŠABLONA"), Some(PageNamespace(10)));
    assert_eq!(
        namespaces.id("DISKUSE_ _K _ ŠABLONĚ"),
        Some(PageNamespace(11))
    );
}

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
use memmap::Mmap;
use serde::Deserialize;
use thiserror::Error;

/**
Utility function for memory-mapping a SQL file to make it easier to use [`iterate_sql_insertions`](crate::iterate_sql_insertions).

Pass a borrowed memory map to `iterate_sql_insertions` so that the [schema](crate::schemas) struct
produced by the iterator can borrow from the file's contents.
See the example in the crate documentation.

Returns an [`Error::Io`], which displays the action that failed as well as the path and the underlying `std::io::Error`.

# Safety

Inherits unsafe annotation from [`Mmap::map`].
*/
pub unsafe fn memory_map(path: &Path) -> Result<Mmap, Error> {
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

#[derive(Debug, Deserialize)]
struct Response {
    query: Query,
}

#[derive(Debug, Deserialize)]
struct Query {
    namespaces: Map<String, NamespaceInfo>,
}

#[derive(Debug, Deserialize)]
struct NamespaceInfo {
    id: i32,
    #[serde(rename = "*")]
    name: String,
    #[serde(rename = "canonical")]
    canonical_name: Option<String>,
}

/// Provides a way to convert a namespace and title from `page.sql` to the title displayed on the wiki page.
pub struct NamespaceMap(Map<PageNamespace, String>);

impl NamespaceMap {
    /// Creates a `NamespaceMap` by parsing `siteinfo-namespaces.json.gz` or `siteinfo-namespaces.json`
    /// and converting it into a map from namespace number to local namespace name.
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
        Ok(Self(
            serde_json::from_str::<Response>(&json)
                .map_err(|source| Error::JsonFile {
                    source,
                    path: path.into(),
                })?
                .query
                .namespaces
                .into_iter()
                .map(|(_, namespace_info)| {
                    (PageNamespace(namespace_info.id), namespace_info.name)
                })
                .collect(),
        ))
    }

    /// Returns a full title, consisting of the [`PageNamespace`] converted to the local namespace name,
    /// a colon if needed, and the [`PageTitle`] with underscores replaced with spaces.
    pub fn readable_title(&self, PageTitle(title): &PageTitle, namespace: &PageNamespace) -> String {
        self.0
            .get(&namespace)
            .map(|n| {
                let mut readable_title = String::new();
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

    pub fn iter(&self) -> impl Iterator<Item = (&PageNamespace, &String)> {
        self.0.iter()
    }
}

use std::{
    collections::BTreeMap as Map,
    fmt::Write,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use memmap::Mmap;
use parse_mediawiki_sql::{
    schemas::{Page, PageProps},
    types::{PageNamespace, PageTitle},
};
use pico_args::{Arguments, Error as PicoArgsError};
use serde::Deserialize;
use smartstring::alias::String as SmartString;
use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("Error parsing arguments")]
    PicoArgs(#[from] PicoArgsError),
    #[error("Invalid namespace: {0}")]
    InvalidNamespace(String),
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
    fn from_io<P: Into<PathBuf>>(action: &'static str, source: std::io::Error, path: P) -> Self {
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
    namespaces: Map<SmartString, NamespaceInfo>,
}

#[derive(Debug, Deserialize)]
struct NamespaceInfo {
    id: i32,
    #[serde(rename = "*")]
    name: SmartString,
    #[serde(rename = "canonical")]
    canonical_name: Option<SmartString>,
}

struct NamespaceMap(Map<PageNamespace, SmartString>);

impl NamespaceMap {
    fn from_path(path: &Path) -> Result<Self, Error> {
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
                    (PageNamespace::from(namespace_info.id), namespace_info.name)
                })
                .collect(),
        ))
    }

    fn readable_title(&self, title: &PageTitle, namespace: &PageNamespace) -> SmartString {
        self.0
            .get(&namespace)
            .map(|n| {
                let title: &String = title.into();
                if n == "" {
                    title.into()
                } else {
                    let mut readable_title = SmartString::new();
                    write!(readable_title, "{}:{}", n, title).unwrap();
                    readable_title
                }
            })
            .unwrap()
    }

    fn iter(&self) -> impl Iterator<Item = (&PageNamespace, &SmartString)> {
        self.0.iter()
    }
}

fn opt_path_from_args(
    args: &mut Arguments,
    keys: [&'static str; 2],
) -> Result<Option<PathBuf>, PicoArgsError> {
    args.opt_value_from_os_str(keys, |opt| {
        Result::<_, PicoArgsError>::Ok(PathBuf::from(opt))
    })
}

fn path_from_args_in_dir(
    args: &mut Arguments,
    keys: [&'static str; 2],
    default: &str,
    opt_dir: &Option<PathBuf>,
) -> Result<PathBuf, PicoArgsError> {
    opt_path_from_args(args, keys).map(|path| {
        let file = path.unwrap_or_else(|| default.into());
        opt_dir
            .clone()
            .map(|mut dir| {
                dir.push(&file);
                dir
            })
            .unwrap_or(file)
    })
}

unsafe fn memory_map(path: &Path) -> Result<Mmap, Error> {
    Mmap::map(&File::open(path).map_err(|source| Error::from_io("open file", source, path))?)
        .map_err(|source| Error::from_io("memory map file", source, path))
}

unsafe fn memory_map_from_args_in_dir(
    args: &mut Arguments,
    keys: [&'static str; 2],
    default: &str,
    opt_dir: &Option<PathBuf>,
) -> Result<Mmap, Error> {
    let path = path_from_args_in_dir(args, keys, default, opt_dir)?;
    memory_map(&path)
}

fn main() -> anyhow::Result<()> {
    let mut args = Arguments::from_env();
    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let props_sql = unsafe {
        memory_map_from_args_in_dir(&mut args, ["-P", "--props"], "page_props.sql", &dump_dir)?
    };
    let page_sql =
        unsafe { memory_map_from_args_in_dir(&mut args, ["-p", "--page"], "page.sql", &dump_dir)? };
    let namespace_id_to_name = NamespaceMap::from_path(&path_from_args_in_dir(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
        &dump_dir,
    )?)?;
    let namespaces = args
        .free()?
        .into_iter()
        .map(|n| {
            n.parse()
                .ok()
                .or_else(|| {
                    namespace_id_to_name.iter().find_map(|(id, name)| {
                        if name.as_str() == n.as_str() {
                            Some(id.into_inner())
                        } else {
                            None
                        }
                    })
                })
                .ok_or(Error::InvalidNamespace(n))
        })
        .collect::<Result<Vec<i32>, _>>()?;
    let mut id_to_displaytitle = parse_mediawiki_sql::iterate_sql_insertions(&props_sql)
        .filter_map(
            |PageProps {
                 page, name, value, ..
             }| {
                if name == "displaytitle" {
                    // All displaytitles should be UTF-8.
                    Some((page, String::from_utf8(value).unwrap()))
                } else {
                    None
                }
            },
        )
        .collect::<Map<_, _>>();
    let title_to_displaytitle = parse_mediawiki_sql::iterate_sql_insertions(&page_sql).fold(
        Map::new(),
        |mut map,
         Page {
             namespace,
             title,
             id,
             ..
         }| {
            if let Some(displaytitle) = id_to_displaytitle.remove(&id) {
                if namespaces.is_empty() || namespaces.contains(&namespace.into_inner()) {
                    map.insert(
                        namespace_id_to_name.readable_title(&title, &namespace),
                        displaytitle,
                    );
                }
            }
            map
        },
    );
    serde_json::to_writer(std::io::stdout(), &title_to_displaytitle).unwrap();
    Ok(())
}

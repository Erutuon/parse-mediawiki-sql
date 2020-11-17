#![allow(unused)]
use bstr::ByteSlice;
use flate2::read::GzDecoder;
use memmap::Mmap;
use pico_args::{Arguments, Error as PicoArgsError, Keys};
use serde::Deserialize;
use smartstring::alias::String as SmartString;
use std::{
    collections::BTreeMap as Map,
    fmt::Write,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use thiserror::Error;

use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{CategoryLinks, Page},
    types::{PageNamespace, PageTitle},
};

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

#[derive(Debug, Error)]
enum Error {
    #[error("No namespaces provided in positional arguments")]
    NoNamespaces,
    #[error("Error parsing arguments")]
    PicoArgs(#[from] PicoArgsError),
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

unsafe fn memory_map(path: &Path) -> Result<Mmap, Error> {
    Mmap::map(&File::open(path).map_err(|source| Error::from_io("open file", source, path))?)
        .map_err(|source| Error::from_io("memory map file", source, path))
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
                    write!(readable_title, "{}:{}", n, title);
                    readable_title
                }
            })
            .unwrap()
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

unsafe fn memory_map_from_args_in_dir(
    args: &mut Arguments,
    keys: [&'static str; 2],
    default: &str,
    opt_dir: &Option<PathBuf>,
) -> Result<Mmap, Error> {
    let path = path_from_args_in_dir(args, keys, default, opt_dir)?;
    memory_map(&path)
}

// Expects categorylinks.sql and page.sql in the current directory.
fn main() -> anyhow::Result<()> {
    let mut args = Arguments::from_env();

    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let page_sql =
        unsafe { memory_map_from_args_in_dir(&mut args, ["-p", "--page"], "page.sql", &dump_dir)? };
    let category_links_sql = unsafe {
        memory_map_from_args_in_dir(
            &mut args,
            ["-c", "--category-links"],
            "categorylinks.sql",
            &dump_dir,
        )?
    };
    let namespace_id_to_name = NamespaceMap::from_path(&path_from_args_in_dir(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
        &dump_dir,
    )?)?;
    let prefixes: Vec<String> = args.values_from_str(["-P", "--prefix"])?;

    args.finish()?;

    let mut category_links = iterate_sql_insertions::<CategoryLinks>(&category_links_sql);
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
    let mut id_to_categories: Map<_, _> = category_links
        .filter(|CategoryLinks { to, .. }| {
            let to: &String = to.into();
            prefixes.iter().any(|prefix| {
                to.starts_with(prefix)
            })
        })
        .fold(Map::new(), |mut map, CategoryLinks { from, to, .. }| {
            let entry = map.entry(from).or_insert_with(Vec::new);
            let to: String = to.into_inner();
            entry.push(to);
            map
        });

    let page_to_categories = pages.fold(
        Map::new(),
        |mut map,
         Page {
             id,
             title,
             namespace,
             ..
         }| {
            if let Some(categories) = id_to_categories.remove(&id) {
                map.insert(
                    namespace_id_to_name.readable_title(&title, &namespace),
                    categories,
                );
            }
            map
        },
    );
    serde_json::to_writer(std::io::stdout(), &page_to_categories).unwrap();

    assert_eq!(
        category_links
            .finish()
            .map(|(input, _)| input.chars().take(4).collect::<String>()),
        Ok(";\n/*".into())
    );

    Ok(())
}

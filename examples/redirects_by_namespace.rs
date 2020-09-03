use memmap::Mmap;
use serde::Deserialize;
use std::{
    collections::{BTreeMap as Map, BTreeSet as Set},
    fs::File,
    io::prelude::*,
    path::{Path, PathBuf},
};
use thiserror::Error;

use flate2::read::GzDecoder;
use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{Page, Redirect},
    types::{PageNamespace, PageTitle},
};
use pico_args::{Arguments, Error as PicoArgsError, Keys};

static USAGE: &str = "
redirects-by-namespace [arguments] namespace...
-p, --page                  path to page.sql [default: page.sql]
-r, --redirect              path to redirect.sql [default: redirect.sql]
-s, --siteinfo-namespaces   path to siteinfo-namespaces.json or
                            siteinfo-namespaces.json.gz
                            [default: siteinfo-namespaces.json]

Multiple namespace ids can be provided as positional arguments.
";

enum Args {
    Help,
    PrintRedirects {
        page_path: PathBuf,
        redirect_path: PathBuf,
        namespace_id_to_name: Map<PageNamespace, String>,
        namespaces: Set<PageNamespace>,
    },
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
    IoError {
        action: &'static str,
        source: std::io::Error,
        path: PathBuf,
    },
}

unsafe fn memory_map(path: &Path) -> Result<Mmap, Error> {
    Mmap::map(&File::open(path).map_err(|source| Error::IoError {
        action: "open file",
        source,
        path: path.into(),
    })?)
    .map_err(|source| Error::IoError {
        action: "memory map file",
        source,
        path: path.into(),
    })
}

fn get_namespace_id_to_name(
    path: &Path,
) -> Result<Map<PageNamespace, String>, Error> {
    let json = if path.extension() == Some("gz".as_ref()) {
        let gz = File::open(path).map_err(|source| Error::IoError {
            action: "open file",
            source,
            path: path.into(),
        })?;
        let mut decoder = GzDecoder::new(gz);
        let mut decoded = String::new();
        decoder.read_to_string(&mut decoded).map_err(|source| {
            Error::IoError {
                action: "parse GZip",
                source,
                path: path.into(),
            }
        })?;
        decoded
    } else {
        std::fs::read_to_string(path).map_err(|source| Error::IoError {
            action: "read file to string",
            source,
            path: path.into(),
        })?
    };
    Ok(serde_json::from_str::<Response>(&json)
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
        .collect())
}

fn readable_title(
    namespace_id_to_name: &Map<PageNamespace, String>,
    title: &PageTitle,
    namespace: &PageNamespace,
) -> Option<String> {
    namespace_id_to_name.get(&namespace).map(|n| {
        let title: &String = title.into();
        if *n == "" {
            title.to_string()
        } else {
            format!("{}:{}", n, title)
        }
    })
}

fn get_args() -> Result<Args, Error> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        return Ok(Args::Help);
    }

    fn path_from_args(
        args: &mut Arguments,
        keys: impl Into<Keys>,
        default: impl Into<PathBuf>,
    ) -> PathBuf {
        args.value_from_os_str(keys, |opt| {
            Result::<_, PicoArgsError>::Ok(PathBuf::from(opt))
        })
        .unwrap_or_else(|_| default.into())
    }

    let page_path = path_from_args(&mut args, ["-p", "--page"], "page.sql");
    let redirect_path =
        path_from_args(&mut args, ["-r", "--redirect"], "redirect.sql");
    let siteinfo_namespaces_path = path_from_args(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
    );
    let namespaces = args
        .free()?
        .into_iter()
        .map(|n| {
            n.parse::<i32>().map(PageNamespace::from).map_err(|e| {
                PicoArgsError::ArgumentParsingFailed {
                    cause: e.to_string(),
                }
            })
        })
        .collect::<Result<Set<_>, _>>()?;
    if namespaces.is_empty() {
        return Err(Error::NoNamespaces);
    }
    let namespace_id_to_name =
        get_namespace_id_to_name(&siteinfo_namespaces_path)?;
    Ok(Args::PrintRedirects {
        page_path,
        redirect_path,
        namespace_id_to_name,
        namespaces,
    })
}

// Takes a list of namespaces, which must be parsable as `i32`,
// as arguments. Expects page.sql and redirect.sql and siteinfo-namespaces.json
// in the current directory.
fn main() -> anyhow::Result<()> {
    let (page_path, redirect_path, namespace_id_to_name, namespaces) =
        match get_args()? {
            Args::PrintRedirects {
                page_path,
                redirect_path,
                namespace_id_to_name,
                namespaces,
            } => (page_path, redirect_path, namespace_id_to_name, namespaces),
            Args::Help => {
                eprintln!("{}", USAGE);
                return Ok(());
            }
        };
    let page_sql = unsafe { memory_map(&page_path)? };
    let redirect_sql = unsafe { memory_map(&redirect_path)? };
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
    let id_to_title: Map<_, _> = pages
        .filter(
            |Page {
                 namespace,
                 is_redirect,
                 ..
             }| *is_redirect && namespaces.contains(namespace),
        )
        .map(
            |Page {
                 id,
                 title,
                 namespace,
                 ..
             }| (id, (title, namespace)),
        )
        .collect();
    let mut redirects = iterate_sql_insertions::<Redirect>(&redirect_sql);
    let source_to_target: Map<_, _> = redirects
        .filter_map(
            |Redirect {
                 from,
                 title,
                 namespace,
                 ..
             }| {
                id_to_title
                    .get(&from)
                    .map(|from| (from, (title, namespace)))
            },
        )
        .collect();
    for (k, v) in source_to_target {
        println!(
            "{}\t{}",
            readable_title(&namespace_id_to_name, &k.0, &k.1).unwrap(),
            readable_title(&namespace_id_to_name, &v.0, &v.1).unwrap(),
        );
    }
    Ok(())
}

use anyhow::Result;
use memmap::Mmap;
use parse_mediawiki_sql::schemas::CategoryLinks;
use pico_args::Error as PicoArgsError;
use std::{
    collections::{HashMap as Map, HashSet as Set},
    fs::File,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("Error parsing arguments")]
    PicoArgs(#[from] PicoArgsError),
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

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let category_links = args
        .value_from_os_str(["-c", "--category-links"], |opt| {
            Result::<_, PicoArgsError>::Ok(PathBuf::from(opt))
        })
        .unwrap_or_else(|_| "categorylinks.sql".into());
    let categories = args.free()?.into_iter().collect::<Set<_>>();
    let sql = unsafe { memory_map(&category_links)? };
    let links: Map<_, _> = parse_mediawiki_sql::iterate_sql_insertions(&sql)
        .filter_map(|CategoryLinks { from, to, .. }| {
            let to = to.into_inner();
            if categories.contains(&to) {
                Some((from, to))
            } else {
                None
            }
        })
        .collect();
    Ok(())
}

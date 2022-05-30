/*!
Defines [`memory_map`] to read decompressed MediaWiki SQL files,
and [`NamespaceMap`] to display a page title prefixed by its namespace name.
*/

use std::{
    fs::File,
    path::{Path, PathBuf},
};

use thiserror::Error;

pub use memmap2::Mmap;

/**
Memory-maps a file, returning a useful message in case of error.

Pass a borrowed memory map to [`iterate_sql_insertions`](crate::iterate_sql_insertions) so that the [schema](crate::schemas) struct
produced by the iterator can borrow from the file's contents. See the [example](crate#example) in the crate documentation.

# Errors
In case of error, returns an [`struct@Error`] containing the action that failed, the path, and the underlying [`std::io::Error`].

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
#[error("Failed to {action} at {}", path.canonicalize().as_ref().unwrap_or(path).display())]
pub struct Error {
    action: &'static str,
    source: std::io::Error,
    path: PathBuf,
}

impl Error {
    pub fn from_io<P: Into<PathBuf>>(
        action: &'static str,
        source: std::io::Error,
        path: P,
    ) -> Self {
        Error {
            action,
            source,
            path: path.into(),
        }
    }
}

pub use mwtitle::{NamespaceMap, Title};

use crate::field_types::{PageNamespace, PageTitle};

pub trait NamespaceMapExt {
    /// # Panics
    ///
    /// Will panic if the [`PageNamespace`] isn't found in the [`NamespaceMap`].
    fn pretty_title(&self, namespace: PageNamespace, title: PageTitle) -> String;
}

impl NamespaceMapExt for NamespaceMap {
    fn pretty_title(&self, namespace: PageNamespace, title: PageTitle) -> String {
        // Unsafe because `namespace` is not checked against the `NamespaceMap`.
        // `to_pretty` will panic if namespace is not found in `NamespaceMap`.
        self.to_pretty(unsafe { &Title::new_unchecked(namespace.into_inner(), title.into_inner()) })
            .expect("invalid namespace ID")
    }
}

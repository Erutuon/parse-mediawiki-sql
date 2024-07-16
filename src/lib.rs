/*!
[![crates.io](https://img.shields.io/crates/v/parse-mediawiki-sql.svg)](https://crates.io/crates/parse-mediawiki-sql)
[![docs.rs](https://img.shields.io/docsrs/parse-mediawiki-sql)](https://docs.rs/parse-mediawiki-sql)

`parse_mediawiki_sql` parses SQL dumps of a MediaWiki database.
The SQL dumps are scripts that create a database table and insert rows into it.
The entry point is `iterate_sql_insertions`, which creates an iterable struct
from a byte slice (`&[u8]`). The struct is generic over the type returned by the iterator,
and this type must be one of the structs in the [`schemas`](schemas) module,
which represent rows in the database, such as [`Page`](schemas::Page).

## Usage
This crate is available from [crates.io](https://crates.io/crate/parse-mediawiki-sql) and can be
used by adding `parse-mediawiki-sql` to your dependencies in your project's `Cargo.toml`.

```toml
[dependencies]
parse-mediawiki-sql = "0.10"
```

If you're using Rust 2015, then you’ll also need to add it to your crate root:

```no_run
extern crate parse_mediawiki_sql;
```

## Example
To generate a `Vec` containing the titles of all redirect pages:

```no_run
# #[cfg(feature = "utils")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::Page,
    field_types::{PageNamespace, PageTitle},
    utils::memory_map,
};
use std::fs::File;
let page_sql = unsafe { memory_map("page.sql")? };
let redirects: Vec<(PageNamespace, PageTitle)> =
    iterate_sql_insertions(&page_sql)
        .filter_map(
            |Page { namespace, title, is_redirect, .. }| {
                if is_redirect {
                    Some((namespace, title))
                } else {
                    None
                }
            },
        )
        .collect();
# Ok(())
# }
# #[cfg(not(feature = "utils"))]
# fn main() {}
```

Only a mutable reference to the struct is iterable, so a `for`-loop
must use `&mut` or `.into_iter()` to iterate over the struct:

```no_run
# #[cfg(feature = "utils")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# use parse_mediawiki_sql::{
#     iterate_sql_insertions,
#     schemas::Page,
#     utils::memory_map,
# };
# let page_sql =
#     unsafe { memory_map("page.sql")? };
for Page { namespace, title, is_redirect, .. } in &mut iterate_sql_insertions(&page_sql) {
    if is_redirect {
        dbg!((namespace, title));
    }
}
# Ok(())
# }
# #[cfg(not(feature = "utils"))]
# fn main() {}
```
*/

#![cfg_attr(docsrs, feature(doc_cfg))]

use bstr::{ByteSlice, B};
use nom::{
    branch::alt,
    bytes::streaming::{tag, take_while},
    character::streaming::multispace0,
    combinator::{iterator, opt, recognize, ParserIterator},
    sequence::{preceded, tuple},
};

pub mod error;
pub mod field_types;
pub mod from_sql;
pub mod schemas;

pub use error::Error;
pub use from_sql::IResult;
#[cfg(feature = "utils")]
#[cfg_attr(docsrs, doc(cfg(feature = "utils")))]
pub mod utils;

/**
Trait for converting from a SQL tuple to a Rust type,
which can borrow from the string or not.
Used by [`iterate_sql_insertions`].
*/
pub trait FromSqlTuple<'input>: Sized {
    fn from_sql_tuple(s: &'input [u8]) -> IResult<'input, Self>;
}

/**
The entry point of the crate. Takes a SQL dump of a MediaWiki database table as bytes
and yields an iterator over structs representing rows in the table.

The return value is iterable as a mutable reference,
and when iterated it yields structs representing the database rows (`Row`).
These rows are represented as tuples in the SQL code.
The tuples are parsed using [`FromSqlTuple::from_sql_tuple`]
and the fields in the tuples are parsed by [`FromSql::from_sql`](from_sql::FromSql::from_sql).

See the [example][crate#example] in the documentation, and see [`schemas`] for the full list of possible `Row`s.
*/
#[must_use = "the return type implements `Iterator` as a mutable reference, and does nothing unless consumed"]
pub fn iterate_sql_insertions<'input, Row>(
    sql: &'input [u8],
) -> ParserIterator<&'input [u8], Error<'input>, impl FnMut(&'input [u8]) -> IResult<'input, Row>>
where
    Row: FromSqlTuple<'input> + 'input,
{
    let sql = &sql[sql.find("INSERT INTO").expect("INSERT INTO statement")..];
    iterator(
        sql,
        preceded(
            alt((
                recognize(tuple((
                    opt(multispace0),
                    opt(tag(";")),
                    opt(multispace0),
                    tuple((
                        tag(B("INSERT INTO `")),
                        take_while(|b: u8| b == b'_' || b.is_ascii_lowercase()),
                        tag(B("` VALUES ")),
                    )),
                ))),
                tag(","),
            )),
            FromSqlTuple::from_sql_tuple,
        ),
    )
}

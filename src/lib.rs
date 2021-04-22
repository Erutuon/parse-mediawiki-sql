/*!
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
parse-mediawiki-sql = "0.3"
```

If you're using Rust 2015, then you’ll also need to add it to your crate root:

```no_run
extern crate parse_mediawiki_sql;
```

## Example
To generate a `Vec` containing the titles of all redirect pages:

```no_run
use memmap::Mmap;
use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::Page,
    types::{PageNamespace, PageTitle},
};
use std::fs::File;
let page_sql =
    unsafe { Mmap::map(&File::open("page.sql").unwrap()).unwrap() };
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
```

Only a mutable reference to the struct is iterable, so a `for`-loop
must use `&mut` or `.into_iter()` to iterate over the struct:

```no_run
# use parse_mediawiki_sql::{
#     iterate_sql_insertions,
#     schemas::Page,
# };
# use memmap::Mmap;
# use std::fs::File;
# let page_sql =
#     unsafe { Mmap::map(&File::open("page.sql").unwrap()).unwrap() };
for Page { namespace, title, is_redirect, .. } in &mut iterate_sql_insertions(&page_sql) {
    if is_redirect {
        dbg!((namespace, title));
    }
}
```
*/

use bstr::{ByteSlice, B};
use nom::{
    branch::alt,
    bytes::streaming::{tag, take_while},
    character::streaming::multispace0,
    combinator::{iterator, opt, recognize, ParserIterator},
    sequence::{preceded, tuple},
};
pub use types::{Error, IResult};

pub mod schemas;
pub mod types;

#[cfg(feature = "utils")]
pub mod utils;

/**
Trait for converting from a SQL tuple to a Rust type,
which can borrow from the string or not.
Used by [`iterate_sql_insertions`][crate::iterate_sql_insertions].
*/
pub trait FromSqlTuple<'a>: Sized {
    fn from_sql_tuple(s: &'a [u8]) -> IResult<'a, Self>;
}

/**
Takes a SQL dump of a MediaWiki database table as bytes
and yields a struct that is iterable as a mutable reference,
yielding structs representing the database rows.
*/
#[must_use = "the return type implements `Iterator` as a mutable reference, and does nothing unless consumed"]
pub fn iterate_sql_insertions<'a, T>(
    sql: &'a [u8],
) -> ParserIterator<&'a [u8], Error<'a>, impl FnMut(&'a [u8]) -> IResult<'a, T>>
where
    T: FromSqlTuple<'a> + 'a,
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
                        take_while(|b| (b'a'..=b'z').contains(&b) || b == b'_'),
                        tag(B("` VALUES ")),
                    )),
                ))),
                tag(","),
            )),
            FromSqlTuple::from_sql_tuple,
        ),
    )
}

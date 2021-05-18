# parse-mediawiki-sql
This is a library for quickly parsing the [SQL files](https://meta.wikimedia.org/wiki/Data_dumps/What%27s_available_for_download) from the [Wikimedia dumps](https://dumps.wikimedia.org/). It is regularly used with some of the files from the English Wiktionary dump, but should work for other wikis’ dumps as well.

[![crates.io](https://img.shields.io/crates/v/parse-mediawiki-sql.svg)](https://crates.io/crates/parse-mediawiki-sql)
[![docs.rs](https://img.shields.io/docsrs/parse-mediawiki-sql)](https://docs.rs/parse-mediawiki-sql)

## Background
Wikimedia provides SQL files that can be executed by a database server to create a replica of various MediaWiki [database tables](https://www.mediawiki.org/wiki/Manual:Database_layout). But it is very slow to execute the scripts that create some of the larger tables, and for recurring jobs it is much faster to run a program that extracts information by parsing the scripts. For example, the `template_redirects` example program, which parses all of `page.sql`, takes about 20 seconds, but creating the [`page` table](https://www.mediawiki.org/wiki/Manual:Page_table) by executing `page.sql` with `mariadb` takes much longer, more than an hour the one time I tried it.

This library is sort of a rewriting of my previous Lua library ([`parse_sql_dump`](https://github.com/Erutuon/enwikt-dump-rs/tree/master/lua/parse_sql_dump)), and that in turn was inspired by [WikiUtils](https://github.com/napsternxg/WikiUtils), a library linked from [a Wikipedia help page](https://en.wikipedia.org/wiki/Wikipedia:Database_download#Help_to_import_dumps_into_MySQL) that uses regex to parse the SQL files. My Lua library parses the files with [LPeg](http://www.inf.puc-rio.br/~roberto/lpeg/) (which I am extremely fond of). Like the Rust library, it has an iterator interface, but it often used up all my scant supply of RAM when I used it to parse `page.sql`, making my computer go into swap and malfunction and have to be restarted.

So I finally created a more thrifty Rust library. It is relatively easy to minimize memory usage of a parser with Rust by having the parser's output borrow from the input. With memory mapping, the operating system handles allocating and free the memory of the parser's input.

## Library
The entry point is `iterate_sql_insertions`, which takes the SQL script as a byte slice (`&[u8]`) and generates a struct that functions as iterator over structs representing the rows in the `INSERT` statement. These structs are found in `parse_mediawiki_sql::schemas`, and the types of their fields are found in `parse_mediawiki_sql::types`. The struct from `iterate_sql_insertions` borrows from the byte slice, so in a `for` loop it must be iterated as as a mutable reference: `for _ in &mut parse_mediawiki_sql::iterate_sql_insertions(&sql_script_byte_slice) { /* ... */ }`.

The names of the fields in the structs are based on the names of the fields in the database tables, but with prefixes removed. Fields in one table that relate to a field in another table use the same type, and several fields that are `int` or `binary` types in the database are represented by fields of more specific Rust types.

For example the `Page` struct represents a row in the [`page` table](https://www.mediawiki.org/wiki/Manual:Page_table). Its fields `id`, `namespace`, and `title` (of which the types are `PageId`, `PageNamespace`, and `PageTitle`) represent the `page_id`, `page_namespace`, and `page_title` fields. The `from` field in the `Redirect` struct (representing [`rd_from`](https://www.mediawiki.org/wiki/Manual:Redirect_table#rd_from)) refers to a row in the `page` table identified by its `page_id` field, so it is likewise a `PageId`.

The fields borrow from the input if possible. If a `binary` type contains valid UTF-8, it is represented as a `String` or a `&str`, otherwise a `Vec<u8>`. If a `binary` field is valid UTF-8 and will not, barring errors, contain any escapes (such as `\'`), it is parsed into a `&str` that borrows from the input `&[u8]`.

As some of the SQL dump files, such as `page.sql`, can be very large, I use a convenience function to memory map the file to avoid reading them completely into memory and provide something for the items of the iterator to borrow from. The examples use `utils::memory_map` (enabled by the feature `utils`), which uses the [`memmap`](https://lib.rs/crates/memmap) crate, but with a more helpful error type.

## Example
To generate a `Vec` containing the titles of all redirect pages:

```rust
use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::Page,
    types::{PageNamespace, PageTitle},
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
```

## Current uses

The [`template_redirect`](examples/template_redirects.rs) example, which can be run with `cargo run --release --example template_redirect path/to/page.sql path/to/redirect.sql > template_redirects.json`, generates a JSON object containing all template redirects as of a particular dump version. This program is used by the [Templatehoard](https://templatehoard.toolforge.org/) tool on [Toolforge](https://toolforge.org), which provides dump files of template instances from English Wiktionary, both of a template and its redirects.

## To do

* Allow parsing the `.sql.gz` files offered on the  directly (at the moment, they must be un-gzipped first)
* More helpful errors in the iterator returned by `parse_sql_insertions`.
* Check that the iterator parses the whole set of SQL insertions.
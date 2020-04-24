use bstr::ByteSlice;
use memmap::Mmap;
use std::collections::HashMap;
use std::fs::File;

use parse_mediawiki_sql::{
    iterate_sql_insertions, schemas::Page, types::ContentModel,
};

fn main() {
    let args: Vec<_> = std::env::args().take(1).collect();
    let sql = unsafe {
        Mmap::map(
            &File::open(args.get(0).map(String::as_str).unwrap_or("page.sql"))
                .expect("page.sql not found"),
        )
        .expect("could not memory map file")
    };
    let mut iterator = iterate_sql_insertions::<Page>(&sql);
    let counts: HashMap<Option<ContentModel>, usize> = iterator.fold(
        HashMap::new(),
        |mut counts, Page { content_model, .. }| {
            let entry = counts.entry(content_model).or_insert(0);
            *entry += 1;
            counts
        },
    );
    println!("{:?}", counts);
    assert!(iterator
        .finish()
        .map(|(input, _)| &input.chars().take(4).collect::<String>() == ";\n/*")
        .unwrap_or(false));
}

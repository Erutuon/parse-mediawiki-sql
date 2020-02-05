use memmap::Mmap;
use std::collections::HashMap;
use std::fs::File;

use parse_mediawiki_sql::{
    iterate_sql_insertions, schemas::Page, types::ContentModel,
};

fn main() {
    let sql = unsafe {
        Mmap::map(&File::open("page.sql").expect("page.sql not found"))
            .expect("could not memory map file")
    };
    let mut iterator = iterate_sql_insertions::<Page>(unsafe {
        &std::str::from_utf8_unchecked(&sql)
    });
    let counts: HashMap<Option<ContentModel>, usize> = iterator.fold(
        HashMap::new(),
        |mut counts, Page { content_model, .. }| {
            let entry = counts.entry(content_model).or_insert(0);
            *entry += 1;
            counts
        },
    );
    println!("{:?}", counts);
    match iterator.finish() {
        Ok((remaining_input, _)) => {
            dbg!(remaining_input.chars().take(1000).collect::<String>());
        }
        Err(e) => {
            use nom::Err;
            match e {
                Err::Failure((t, e)) | Err::Error((t, e)) => {
                    dbg!(t.chars().take(1000).collect::<String>(), e);
                }
                _ => {
                    eprintln!("other error");
                }
            }
        }
    };
}

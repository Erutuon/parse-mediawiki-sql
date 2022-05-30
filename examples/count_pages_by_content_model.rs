use bstr::ByteSlice;
use std::{collections::HashMap, path::PathBuf};

use parse_mediawiki_sql::{
    field_types::ContentModel, iterate_sql_insertions, schemas::Page, utils::memory_map,
};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let sql = unsafe {
        memory_map(
            &args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| "page.sql".into()),
        )?
    };
    let mut iterator = iterate_sql_insertions::<Page>(&sql);
    let counts: HashMap<Option<ContentModel>, usize> =
        iterator.fold(HashMap::new(), |mut counts, Page { content_model, .. }| {
            let entry = counts.entry(content_model).or_insert(0);
            *entry += 1;
            counts
        });
    println!("{:?}", counts);
    assert_eq!(
        iterator
            .finish()
            .map(|(input, _)| input.chars().take(4).collect::<String>()),
        Ok(";\n/*".into())
    );
    Ok(())
}

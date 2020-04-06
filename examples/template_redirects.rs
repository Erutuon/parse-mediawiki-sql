use bstr::ByteSlice;
use memmap::Mmap;
use std::collections::BTreeMap as Map;
use std::fs::File;

use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{Page, Redirect},
    types::PageNamespace,
};

unsafe fn memory_map(path: &str) -> Mmap {
    Mmap::map(
        &File::open(path)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", &path, e)),
    )
    .unwrap_or_else(|e| panic!("Failed to memory-map {}: {}", &path, e))
}

// Expects page.sql and redirect.sql in the current directory.
// Generates JSON: { target: [source1, source2, source3, ...], ...}
fn main() {
    let page_sql = unsafe { memory_map("page.sql") };
    let redirect_sql = unsafe { memory_map("redirect.sql") };
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
    let template_namespace = PageNamespace::from(10);
    // This works if every template redirect in redirect.sql is also marked
    // as a redirect in page.sql.
    let id_to_title: Map<_, _> = pages
        .filter(
            |Page {
                 namespace,
                 is_redirect,
                 ..
             }| *is_redirect && *namespace == template_namespace,
        )
        .map(|Page { id, title, .. }| (id, title))
        .collect();
    assert!(pages
        .finish()
        .map(|(input, _)| &input.chars().take(4).collect::<String>() == ";\n/*")
        .unwrap_or(false));
    let mut redirects = iterate_sql_insertions::<Redirect>(&redirect_sql);
    let target_to_sources: Map<_, _> = redirects
        .filter_map(|Redirect { from, title, .. }| {
            id_to_title.get(&from).map(|from| (from, title))
        })
        .fold(Map::new(), |mut map, (from, title)| {
            let entry = map.entry(title.into_inner()).or_insert_with(Vec::new);
            entry.push(from.clone().into_inner());
            map
        });
    assert!(redirects
        .finish()
        .map(|(input, _)| &input.chars().take(4).collect::<String>() == ";\n/*")
        .unwrap_or(false));
    serde_json::to_writer(std::io::stdout(), &target_to_sources).unwrap();
}

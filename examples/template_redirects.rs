use bstr::ByteSlice;
use parse_mediawiki_sql::{
    field_types::{PageNamespace, PageTitle},
    iterate_sql_insertions,
    schemas::{Page, Redirect},
    utils::memory_map,
};
use std::{collections::BTreeMap as Map, path::PathBuf};

// Expects page.sql and redirect.sql in the current directory.
// Generates JSON: { target: [source1, source2, source3, ...], ...}
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let page_sql = unsafe {
        memory_map(
            &args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| "page.sql".into()),
        )?
    };
    let redirect_sql = unsafe {
        memory_map(
            &args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| "redirect.sql".into()),
        )?
    };
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
    let template_namespace = PageNamespace(10);
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
    assert_eq!(
        pages
            .finish()
            .map(|(input, _)| input.chars().take(4).collect::<String>()),
        Ok(";\n/*".into())
    );
    let mut redirects = iterate_sql_insertions::<Redirect>(&redirect_sql);
    let target_to_sources: Map<_, _> = redirects
        .filter_map(
            |Redirect {
                 from: source_id,
                 title: PageTitle(target),
                 ..
             }| {
                id_to_title
                    .get(&source_id)
                    .map(|PageTitle(source)| (source, target))
            },
        )
        .fold(Map::new(), |mut map, (source, target)| {
            let entry = map.entry(target).or_insert_with(Vec::new);
            entry.push(source.as_str());
            map
        });
    serde_json::to_writer(std::io::stdout(), &target_to_sources).unwrap();
    assert_eq!(
        redirects
            .finish()
            .map(|(input, _)| input.chars().take(4).collect::<String>()),
        Ok(";\n/*".into())
    );
    Ok(())
}

use anyhow::Result;
use parse_mediawiki_sql::{
    field_types::PageTitle,
    schemas::{CategoryLink, Page},
    utils::{memory_map, NamespaceMap, NamespaceMapExt as _},
};
use std::{
    collections::{HashMap as Map, HashSet as Set},
    convert::TryFrom,
    path::PathBuf,
};

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();

    #[allow(clippy::redundant_closure)]
    let mut get_arg = |keys: [&'static str; 2], default: &'static str| {
        args.value_from_os_str(keys, |opt| PathBuf::try_from(opt))
            .unwrap_or_else(|_| default.into())
    };

    let category_links_path = get_arg(["-c", "--category-links"], "categorylinks.sql");
    let page_path = get_arg(["-p", "--page"], "page.sql");
    let siteinfo_namespaces_path =
        get_arg(["-S", "--siteinfo-namespaces"], "siteinfo-namespaces.json");
    let category_links_sql = unsafe { memory_map(&category_links_path)? };
    let page_sql = unsafe { memory_map(&page_path)? };

    let namespace_map = NamespaceMap::from_path(&siteinfo_namespaces_path)?;

    let categories = args
        .finish()
        .into_iter()
        .map(|os_str| {
            os_str
                .into_string()
                .map_err(|_| anyhow::Error::new(pico_args::Error::NonUtf8Argument))
        })
        .collect::<Result<Set<_>>>()?;

    let category_members: Map<_, _> =
        parse_mediawiki_sql::iterate_sql_insertions(&category_links_sql)
            .filter_map(
                |CategoryLink {
                     from,
                     to: PageTitle(to),
                     ..
                 }| {
                    if categories.contains(&to) {
                        Some((from, to))
                    } else {
                        None
                    }
                },
            )
            .fold(Map::new(), |mut a, (page, category)| {
                a.entry(page).or_insert_with(Vec::new).push(category);
                a
            });
    let mut pages: Map<_, _> = parse_mediawiki_sql::iterate_sql_insertions(&page_sql)
        .filter_map(
            |Page {
                 id,
                 namespace,
                 title,
                 ..
             }| {
                if category_members.contains_key(&id) {
                    Some((id, (namespace, title)))
                } else {
                    None
                }
            },
        )
        .collect();

    let category_members: Map<_, _> = category_members
        .into_iter()
        .map(|(page_id, categories)| {
            let (namespace, title) = pages.remove(&page_id).expect("page ID should be here!");
            let title = namespace_map.pretty_title(namespace, title);
            (title, categories)
        })
        .collect();

    serde_json::to_writer(std::io::stdout().lock(), &category_members)?;

    Ok(())
}

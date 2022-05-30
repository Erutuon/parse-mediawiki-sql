use anyhow::Result;
use bstr::ByteSlice;
use pico_args::Arguments;
use std::{collections::BTreeMap as Map, convert::TryFrom, path::PathBuf};

use parse_mediawiki_sql::{
    field_types::PageTitle,
    iterate_sql_insertions,
    schemas::{CategoryLink, Page},
    utils::{memory_map, Mmap, NamespaceMap, NamespaceMapExt as _},
};

#[allow(clippy::redundant_closure)]
fn opt_path_from_args(args: &mut Arguments, keys: [&'static str; 2]) -> Result<Option<PathBuf>> {
    Ok(args.opt_value_from_os_str(keys, |opt| PathBuf::try_from(opt))?)
}

fn path_from_args_in_dir(
    args: &mut Arguments,
    keys: [&'static str; 2],
    default: &str,
    opt_dir: &Option<PathBuf>,
) -> Result<PathBuf> {
    opt_path_from_args(args, keys).map(|path| {
        let file = path.unwrap_or_else(|| default.into());
        opt_dir
            .clone()
            .map(|mut dir| {
                dir.push(&file);
                dir
            })
            .unwrap_or(file)
    })
}

unsafe fn memory_map_from_args_in_dir(
    args: &mut Arguments,
    keys: [&'static str; 2],
    default: &str,
    opt_dir: &Option<PathBuf>,
) -> Result<Mmap> {
    let path = path_from_args_in_dir(args, keys, default, opt_dir)?;
    Ok(memory_map(&path)?)
}

// Expects categorylinks.sql and page.sql in the current directory.
fn main() -> Result<()> {
    let mut args = Arguments::from_env();

    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let page_sql =
        unsafe { memory_map_from_args_in_dir(&mut args, ["-p", "--page"], "page.sql", &dump_dir)? };
    let category_links_sql = unsafe {
        memory_map_from_args_in_dir(
            &mut args,
            ["-c", "--category-links"],
            "categorylinks.sql",
            &dump_dir,
        )?
    };
    let namespace_map = NamespaceMap::from_path(&path_from_args_in_dir(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
        &dump_dir,
    )?)?;
    let prefixes: Vec<String> = args.values_from_str(["-P", "--prefix"])?;

    let mut category_links = iterate_sql_insertions::<CategoryLink>(&category_links_sql);
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
    let mut id_to_categories: Map<_, _> = category_links
        .filter(
            |CategoryLink {
                 to: PageTitle(category),
                 ..
             }| { prefixes.iter().any(|prefix| category.starts_with(prefix)) },
        )
        .fold(
            Map::new(),
            |mut map,
             CategoryLink {
                 from,
                 to: PageTitle(category),
                 ..
             }| {
                let entry = map.entry(from).or_insert_with(Vec::new);
                entry.push(category);
                map
            },
        );

    let page_to_categories = pages.fold(
        Map::new(),
        |mut map,
         Page {
             id,
             title,
             namespace,
             ..
         }| {
            if let Some(categories) = id_to_categories.remove(&id) {
                map.insert(namespace_map.pretty_title(namespace, title), categories);
            }
            map
        },
    );
    serde_json::to_writer(std::io::stdout(), &page_to_categories).unwrap();

    assert_eq!(
        category_links
            .finish()
            .map(|(input, _)| input.chars().take(4).collect::<String>()),
        Ok(";\n/*".into())
    );

    Ok(())
}

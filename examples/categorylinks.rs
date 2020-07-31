#![allow(unused)]
use bstr::ByteSlice;
use memmap::Mmap;
use nom;
use serde_json::Value;
use std::collections::BTreeMap as Map;
use std::fs::File;

use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{CategoryLinks, Page},
    types::{PageNamespace, PageTitle},
};

unsafe fn memory_map(path: &str) -> Mmap {
    Mmap::map(
        &File::open(path)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", &path, e)),
    )
    .unwrap_or_else(|e| panic!("Failed to memory-map {}: {}", &path, e))
}

fn get_namespace_id_to_name(filepath: &str) -> Map<PageNamespace, String> {
    let siteinfo_namespaces = unsafe { memory_map(filepath) };
    let json = unsafe { std::str::from_utf8_unchecked(&siteinfo_namespaces) };
    let mut data: Value = serde_json::from_str(json).unwrap();
    match data["query"].take()["namespaces"].take() {
        Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| {
                if let Ok(namespace) = k.parse::<i32>().map(PageNamespace::from)
                {
                    (
                        namespace,
                        v.as_object().unwrap()["*"]
                            .as_str()
                            .unwrap()
                            .to_string(),
                    )
                } else {
                    panic!("{} is not a valid integer", k);
                }
            })
            .collect(),
        _ => panic!("bad json apparently"),
    }
}

fn readable_title(
    namespace_id_to_name: &Map<PageNamespace, String>,
    title: &PageTitle,
    namespace: &PageNamespace,
) -> String {
    namespace_id_to_name
        .get(&namespace)
        .map(|n| {
            let title: &String = title.into();
            if *n == "" {
                title.to_string()
            } else {
                format!("{}:{}", n, title)
            }
        })
        .unwrap()
}

// Expects categorylinks.sql and page.sql in the current directory.
fn main() {
    let args: Vec<_> = std::env::args().skip(1).take(3).collect();
    let page_sql = unsafe {
        memory_map(args.get(0).map(String::as_str).unwrap_or("page.sql"))
    };
    let category_links_sql = unsafe {
        memory_map(
            args.get(1)
                .map(String::as_str)
                .unwrap_or("categorylinks.sql"),
        )
    };
    let namespace_id_to_name = get_namespace_id_to_name(
        args.get(2)
            .map(String::as_str)
            .unwrap_or("siteinfo-namespaces.json"),
    );
    let mut category_links =
        iterate_sql_insertions::<CategoryLinks>(&category_links_sql);
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
    let mut id_to_categories: Map<_, _> = category_links
        .filter(|CategoryLinks { to, .. }| {
            let to: &String = to.into();
            to.starts_with("English_") || to.starts_with("en:")
        })
        .fold(Map::new(), |mut map, CategoryLinks { id, to, .. }| {
            let entry = map.entry(id).or_insert_with(Vec::new);
            let to: String = to.into_inner();
            entry.push(to);
            map
        });

    assert_eq!(
        category_links
            .finish()
            .map(|(input, _)| input.chars().take(4).collect::<String>()),
        Ok(";\n/*".into())
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
                map.insert(
                    readable_title(&namespace_id_to_name, &title, &namespace),
                    categories,
                );
            }
            map
        },
    );
    serde_json::to_writer(std::io::stdout(), &page_to_categories).unwrap();
}

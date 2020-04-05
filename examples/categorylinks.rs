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

unsafe fn memory_map(file: &str) -> Mmap {
    Mmap::map(&File::open(file).expect("file not found"))
        .expect("could not memory map file")
}

fn get_namespace_id_to_name(
    filepath: &str,
) -> Map<PageNamespace, String> {
    let siteinfo_namespaces = unsafe { memory_map(filepath) };
    let json = unsafe { std::str::from_utf8_unchecked(&siteinfo_namespaces) };
    let mut data: Value = serde_json::from_str(json).unwrap();
    match data["query"].take()["namespaces"].take() {
        Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| {
                if let Ok(namespace) = k.parse::<i32>().map(PageNamespace::from) {
                    (namespace, v.as_object().unwrap()["*"].as_str().unwrap().to_string())
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
    let page_sql = unsafe { memory_map("page.sql") };
    let category_links_sql = unsafe { memory_map("categorylinks.sql") };
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

    match category_links.finish() {
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
    let namespace_id_to_name =
        get_namespace_id_to_name("siteinfo-namespaces.json");
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

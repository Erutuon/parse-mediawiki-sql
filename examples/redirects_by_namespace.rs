use memmap::Mmap;
use serde_json::Value;
use std::collections::{BTreeMap as Map, BTreeSet as Set};
use std::borrow::Cow;
use std::fs::File;

use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{Page, Redirect},
    types::{PageNamespace, PageTitle},
};

unsafe fn memory_map(file: &str) -> Mmap {
    Mmap::map(&File::open(file).expect("file not found"))
        .expect("could not memory map file")
}

fn get_namespace_id_to_name<'a>(json: &'a str) -> Map<PageNamespace, String> {
    let mut data: Value = serde_json::from_str(json).unwrap();
    match data["query"].take()["namespaces"].take() {
        Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| {
                let k = match k.parse::<i32>() {
                    Ok(n) => n.into(),
                    Err(e) => panic!("{} is not a valid integer", k),
                };
                (
                    k,
                    v.as_object().unwrap()["*"].as_str().unwrap().to_string(),
                )
            })
            .collect(),
        _ => panic!("bad json apparently"),
    }
}

fn readable_title<'a>(
    namespace_id_to_name: &Map<PageNamespace, String>,
    title: &PageTitle,
    namespace: &PageNamespace,
) -> String {
    namespace_id_to_name
        .get(&namespace)
        .map(|n| {
            let title: &Cow<_> = title.into();
            if *n == "" {
                title.to_string()
            } else {
                format!("{}:{}", n, title)
            }
        })
        .unwrap()
}

fn main() {
    let page_sql = unsafe { memory_map("page.sql") };
    let redirect_sql = unsafe { memory_map("redirect.sql") };
    let siteinfo_namespaces = unsafe { memory_map("siteinfo-namespaces.json") };
    let namespace_id_to_name = get_namespace_id_to_name(unsafe {
        &std::str::from_utf8_unchecked(&siteinfo_namespaces)
    });
    let namespaces: Result<Set<PageNamespace>, _> = std::env::args()
        .skip(1)
        .map(|s| s.parse::<i32>().map(|n| n.into()))
        .collect();
    let namespaces = namespaces.unwrap();
    let mut pages = iterate_sql_insertions::<Page>(unsafe {
        &std::str::from_utf8_unchecked(&page_sql)
    });
    let id_to_title: Map<_, _> = pages
        .filter(
            |Page {
                 namespace,
                 is_redirect,
                 ..
             }| *is_redirect && namespaces.contains(namespace),
        )
        .map(
            |Page {
                 id,
                 title,
                 namespace,
                 ..
             }| (id, (title, namespace)),
        )
        .collect();
    let mut redirects = iterate_sql_insertions::<Redirect>(unsafe {
        &std::str::from_utf8_unchecked(&redirect_sql)
    });
    let source_to_target: Map<_, _> = redirects
        .filter_map(
            |Redirect {
                 from,
                 title,
                 namespace,
                 ..
             }| {
                id_to_title
                    .get(&from)
                    .map(|from| (from, (title, namespace)))
            },
        )
        .collect();
    for (k, v) in source_to_target {
        println!(
            "{}\t{}",
            readable_title(&namespace_id_to_name, &k.0, &k.1),
            readable_title(&namespace_id_to_name, &v.0, &v.1)
        );
    }
}

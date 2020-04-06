use memmap::Mmap;
use serde_json::Value;
use std::collections::{BTreeMap as Map, BTreeSet as Set};
use std::fs::File;

use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{Page, Redirect},
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

// Takes a list of namespaces, which must be parsable as `i32`,
// as arguments. Expects page.sql and redirect.sql and siteinfo-namespaces.json
// in the current directory.
fn main() {
    let page_sql = unsafe { memory_map("page.sql") };
    let redirect_sql = unsafe { memory_map("redirect.sql") };
    let namespace_id_to_name =
        get_namespace_id_to_name("siteinfo-namespaces.json");
    let namespaces = std::env::args()
        .skip(1)
        .map(|s| s.parse::<i32>().map(PageNamespace::from))
        .collect::<Result<Set<PageNamespace>, _>>()
        .unwrap();
    if namespaces.is_empty() {
        eprintln!("No namespaces provided");
        std::process::exit(-1);
    }
    let mut pages = iterate_sql_insertions::<Page>(&page_sql);
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
    let mut redirects = iterate_sql_insertions::<Redirect>(&redirect_sql);
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

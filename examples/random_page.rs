use std::{io::BufRead, path::Path, time::Instant};

use anyhow::Result;
use ordered_float::NotNan;
use parse_mediawiki_sql::{
    field_types::{PageNamespace, PageTitle},
    iterate_sql_insertions,
    schemas::Page,
    utils::{memory_map, NamespaceMap, NamespaceMapExt},
};

fn print_namespaces_and_titles(mut titles: Vec<(PageNamespace, PageTitle)>) {
    titles.sort();
    for (namespace, title) in titles {
        println!("{}\t{}", namespace.into_inner(), title.into_inner());
    }
}

fn print_pretty_titles(mut titles: Vec<(PageNamespace, PageTitle)>, namespace_map: &NamespaceMap) {
    titles.sort();
    use std::io::Write as _;
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    for (namespace, title) in titles {
        write!(stdout, "{}", namespace_map.pretty_title(namespace, &title)).unwrap();
    }
}

fn main() -> Result<()> {
    let start = Instant::now();
    let mut args = std::env::args_os().skip(1);
    let first_arg = args.next();
    if first_arg == Some("print".into()) {
        let namespace_map = NamespaceMap::from_path(Path::new(
            &args
                .next()
                .unwrap_or_else(|| "siteinfo-namespaces.json".into()),
        ))?;
        let titles: Vec<_> = std::io::BufReader::new(std::fs::File::open(
            &args.next().unwrap_or_else(|| "random_pages.txt".into()),
        )?)
        .lines()
        .map(|line| {
            let line = line.unwrap();
            let (namespace, title) = line
                .split_once('\t')
                .expect("namespace and title separated by tab");
            (
                PageNamespace(namespace.parse().expect("integer")),
                PageTitle(title.into()),
            )
        })
        .collect();
        let count = titles.len();
        print_pretty_titles(titles, &namespace_map);
        eprintln!(
            "parsed and printed {} namespaces and titles in {:.6} sec",
            count,
            start.elapsed().as_secs_f64(),
        );
        return Ok(());
    }
    let page_sql = unsafe { memory_map(first_arg.unwrap_or_else(|| "page.sql".into()))? };
    let page_random: NotNan<f64> = rand::random();
    let epsilon = args
        .next()
        .map(|num| {
            num.to_str()
                .expect("valid UTF-8")
                .parse()
                .expect("valid float")
        })
        .unwrap_or(1e5);
    // let mut count = 0;
    let titles = iterate_sql_insertions(&page_sql)
        .filter(|Page { random, .. }| (page_random - random).abs() <= epsilon)
        .fold(
            Vec::new(),
            |mut titles,
             Page {
                 namespace, title, ..
             }| {
                // count += 1;
                // println!("{}", namespace_map.pretty_title(namespace, &title));
                titles.push((namespace, title));
                titles
            },
        );
    let elapsed = start.elapsed().as_secs_f64();
    let count = titles.len();
    print_namespaces_and_titles(titles);
    eprintln!(
        "retrieved and printed {} page{} with random number within {} of {} in {:.6} sec",
        count,
        if count == 1 { "" } else { "s" },
        epsilon,
        page_random,
        elapsed,
    );
    Ok(())
}

use anyhow::Result;
use parse_mediawiki_sql::{schemas::CategoryLink, utils::memory_map};
use std::{
    collections::{HashMap as Map, HashSet as Set},
    convert::TryFrom,
    path::PathBuf,
};

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let category_links = args
        .value_from_os_str(["-c", "--category-links"], |opt| PathBuf::try_from(opt))
        .unwrap_or_else(|_| "categorylinks.sql".into());
    let categories = args.free()?.into_iter().collect::<Set<_>>();
    let sql = unsafe { memory_map(&category_links)? };
    let _: Map<_, _> = parse_mediawiki_sql::iterate_sql_insertions(&sql)
        .filter_map(|CategoryLink { from, to, .. }| {
            let to = to.into_inner();
            if categories.contains(&to) {
                Some((from, to))
            } else {
                None
            }
        })
        .collect();
    Ok(())
}

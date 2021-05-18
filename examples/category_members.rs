use anyhow::Result;
use parse_mediawiki_sql::{field_types::PageTitle, schemas::CategoryLink, utils::memory_map};
use std::{
    collections::{HashMap as Map, HashSet as Set},
    convert::TryFrom,
    path::PathBuf,
};

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();

    #[allow(clippy::clippy::redundant_closure)]
    let category_links = args
        .value_from_os_str(["-c", "--category-links"], |opt| PathBuf::try_from(opt))
        .unwrap_or_else(|_| "categorylinks.sql".into());
    let sql = unsafe { memory_map(&category_links)? };

    let categories = args
        .finish()
        .into_iter()
        .map(|os_str| {
            os_str
                .into_string()
                .map_err(|_| anyhow::Error::new(pico_args::Error::NonUtf8Argument))
        })
        .collect::<Result<Set<_>>>()?;

    let _: Map<_, _> = parse_mediawiki_sql::iterate_sql_insertions(&sql)
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
        .collect();
    Ok(())
}

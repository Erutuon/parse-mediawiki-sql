use std::{collections::BTreeMap as Map, convert::TryFrom, path::PathBuf};

use anyhow::Result;
use parse_mediawiki_sql::{
    field_types::PageNamespace,
    schemas::{Page, PageProperty},
    utils::{memory_map, Mmap, NamespaceMap, NamespaceMapExt as _},
};
use pico_args::Arguments;
use serde::Serialize;
use smartstring::alias::String as SmartString;
use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("Invalid subcommand: choose {}", .0.join(" or "))]
    Subcommand(&'static [&'static str]),
    #[error("Invalid namespace name: {0}")]
    InvalidNamespace(String),
}

#[derive(Serialize)]
#[serde(untagged)]
enum StringOrBytes {
    Bytes(Vec<u8>),
    String(String),
}

impl From<Vec<u8>> for StringOrBytes {
    fn from(vec: Vec<u8>) -> Self {
        String::from_utf8(vec)
            .map(StringOrBytes::String)
            .unwrap_or_else(|e| StringOrBytes::Bytes(e.into_bytes()))
    }
}

unsafe fn memory_map_from_args_in_dir(
    args: &mut Arguments,
    keys: [&'static str; 2],
    default: &str,
    opt_dir: &Option<PathBuf>,
) -> Result<Mmap> {
    let path = path_from_args_in_dir(args, keys, default, opt_dir)?;
    Ok(memory_map(path)?)
}

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

fn count_prop_names(mut args: Arguments) -> Result<()> {
    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let props_sql = unsafe {
        memory_map_from_args_in_dir(&mut args, ["-P", "--props"], "page_props.sql", &dump_dir)?
    };
    let name_counts = parse_mediawiki_sql::iterate_sql_insertions(&props_sql).fold(
        Map::new(),
        |mut map, PageProperty { name, value, .. }| {
            let utf8 = std::str::from_utf8(&value).is_ok();
            let name = SmartString::from(name);
            let entry = map.entry(name).or_insert((0, 0));
            if utf8 {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
            map
        },
    );
    let name_width = name_counts.keys().map(SmartString::len).max().unwrap();
    for (name, (utf8_count, non_utf8_count)) in name_counts {
        println!(
            "{:<name_width$}   {:<6} {}",
            name,
            utf8_count,
            non_utf8_count,
            name_width = name_width
        );
    }
    Ok(())
}

fn get_namespaces(args: Arguments, namespace_map: &NamespaceMap) -> Result<Vec<PageNamespace>> {
    args.finish()
        .into_iter()
        .map(|os_str| -> Result<_> {
            let n = os_str
                .into_string()
                .map_err(|_| pico_args::Error::NonUtf8Argument)?;
            Ok(n.parse()
                .map(PageNamespace)
                .ok()
                .or_else(|| namespace_map.get_id(n.as_str()).map(PageNamespace))
                .ok_or(Error::InvalidNamespace(n))?)
        })
        .collect()
}

fn page_prop_maps(mut args: Arguments) -> Result<()> {
    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let props_sql = unsafe {
        memory_map_from_args_in_dir(&mut args, ["-P", "--props"], "page_props.sql", &dump_dir)?
    };
    let page_sql =
        unsafe { memory_map_from_args_in_dir(&mut args, ["-p", "--page"], "page.sql", &dump_dir)? };
    let namespace_map = NamespaceMap::from_path(&path_from_args_in_dir(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
        &dump_dir,
    )?)?;
    let namespaces = get_namespaces(args, &namespace_map)?;
    let mut id_to_props = parse_mediawiki_sql::iterate_sql_insertions(&props_sql).fold(
        Map::new(),
        |mut map,
         PageProperty {
             page, name, value, ..
         }| {
            let value = StringOrBytes::from(value);
            map.entry(page)
                .or_insert_with(Map::new)
                .insert(SmartString::from(name), value);
            map
        },
    );
    let title_to_props = parse_mediawiki_sql::iterate_sql_insertions(&page_sql).fold(
        Map::new(),
        |mut map,
         Page {
             namespace,
             title,
             id,
             ..
         }| {
            if let Some(props) = id_to_props.remove(&id) {
                if namespaces.is_empty() || namespaces.contains(&namespace) {
                    map.insert(namespace_map.pretty_title(namespace, &title), props);
                }
            }
            map
        },
    );
    serde_json::to_writer(std::io::stdout(), &title_to_props).unwrap();
    Ok(())
}

pub fn serialize_displaytitles(mut args: Arguments) -> Result<()> {
    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let props_sql = unsafe {
        memory_map_from_args_in_dir(&mut args, ["-P", "--props"], "page_props.sql", &dump_dir)?
    };
    let page_sql =
        unsafe { memory_map_from_args_in_dir(&mut args, ["-p", "--page"], "page.sql", &dump_dir)? };
    let namespace_map = NamespaceMap::from_path(&path_from_args_in_dir(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
        &dump_dir,
    )?)?;
    let namespaces = get_namespaces(args, &namespace_map)?;
    let mut id_to_displaytitle = parse_mediawiki_sql::iterate_sql_insertions(&props_sql)
        .filter_map(
            |PageProperty {
                 page, name, value, ..
             }| {
                if name == "displaytitle" {
                    // All displaytitles should be UTF-8.
                    Some((page, String::from_utf8(value).unwrap()))
                } else {
                    None
                }
            },
        )
        .collect::<Map<_, _>>();
    let title_to_displaytitle = parse_mediawiki_sql::iterate_sql_insertions(&page_sql).fold(
        Map::new(),
        |mut map,
         Page {
             namespace,
             title,
             id,
             ..
         }| {
            if let Some(displaytitle) = id_to_displaytitle.remove(&id) {
                if namespaces.is_empty() || namespaces.contains(&namespace) {
                    map.insert(namespace_map.pretty_title(namespace, &title), displaytitle);
                }
            }
            map
        },
    );
    serde_json::to_writer(std::io::stdout(), &title_to_displaytitle).unwrap();
    Ok(())
}

macro_rules! choose_subcommand {
    (
        $arg:expr => {
            $(
                $subcommand:literal => $function:ident,
            )+
        }
    ) => {
        match $arg {
            $(
                Some($subcommand) => $function,
            )+
            _ => return Err(Error::Subcommand(&[ $($subcommand),+ ]).into()),
        }
    };
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let subcommand = args.next().and_then(|s| s.into_string().ok());
    let args = Arguments::from_vec(args.collect());
    choose_subcommand!(subcommand.as_deref() => {
        "display-titles" => serialize_displaytitles,
        "page-prop-maps" => page_prop_maps,
        "count-prop-names" => count_prop_names,
    })(args)?;
    Ok(())
}

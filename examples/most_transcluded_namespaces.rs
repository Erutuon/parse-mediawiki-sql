use anyhow::{Error, Result};
use pico_args::Arguments;
use std::{collections::HashMap as Map, convert::TryFrom, path::PathBuf};

use parse_mediawiki_sql::{
    iterate_sql_insertions,
    schemas::{LinkTarget, TemplateLink},
    utils::{memory_map, Mmap, NamespaceMap, NamespaceMapExt},
};

#[allow(clippy::unnecessary_fallible_conversions)]
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
    Ok(memory_map(path)?)
}

fn main() -> Result<()> {
    let mut args = Arguments::from_env();

    let dump_dir = opt_path_from_args(&mut args, ["-d", "--dump-dir"])?;
    let template_links_sql = unsafe {
        memory_map_from_args_in_dir(
            &mut args,
            ["-t", "--template-links"],
            "templatelinks.sql",
            &dump_dir,
        )?
    };
    let link_target_sql = unsafe {
        memory_map_from_args_in_dir(
            &mut args,
            ["-l", "--link-target"],
            "linktarget.sql",
            &dump_dir,
        )?
    };
    let namespace_map = NamespaceMap::from_path(&path_from_args_in_dir(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo2-namespacesv2.json",
        &dump_dir,
    )?)?;
    let mut get_namespaces = |keys| {
        args.opt_value_from_fn(keys, |value| {
            value.split(' ').try_fold(Vec::new(), |mut vec, item| {
                item.parse().map(|namespace| {
                    vec.push(namespace);
                    vec
                })
            })
        })
    };
    let link_source_namespaces = get_namespaces(["-n", "--namespaces"])?.ok_or(Error::msg(
        "--namespaces (link source namespaces) is required",
    ))?;

    let link_target_namespaces = get_namespaces(["-L", "--link-target-namespaces"])?;

    let invert_link_target_namespaces = args.contains(["-i", "--invert-link-target-namespaces"]);

    // Count how many pages transclude each link target.
    let mut template_links = iterate_sql_insertions::<TemplateLink>(&template_links_sql);
    let link_target_counts = template_links
        .filter(|TemplateLink { from_namespace, .. }| {
            link_source_namespaces.contains(&from_namespace.into_inner())
        })
        .fold(Map::new(), |mut map, TemplateLink { target_id, .. }| {
            *map.entry(target_id).or_insert(0usize) += 1;
            map
        });

    // Determine which namespace the link targets belong to and add up the counts for each namespace.
    let mut link_targets = iterate_sql_insertions::<LinkTarget>(&link_target_sql);

    // Show a list of pages with the number of transclusions if link_target_namespaces is provided.
    if let Some(link_target_namespaces) = link_target_namespaces {
        let page_transclusion_counts = link_targets
            .filter(|LinkTarget { namespace, .. }| {
                link_target_namespaces.contains(&namespace.into_inner())
                    != invert_link_target_namespaces
            })
            .fold(
                Map::new(),
                |mut map,
                 LinkTarget {
                     id,
                     namespace,
                     title,
                     ..
                 }| {
                    if let Some(count) = link_target_counts.get(&id).copied() {
                        *map.entry((namespace, title)).or_insert(0) += count;
                    }
                    map
                },
            );

        // Add namespace names.
        let mut page_transclusion_counts_list: Vec<_> =
            page_transclusion_counts.into_iter().collect();

        // Sort ascending by transclusion count.
        page_transclusion_counts_list
            .sort_by(|(_, count1), (_, count2)| count1.cmp(count2).reverse());

        for ((namespace, title), count) in page_transclusion_counts_list {
            let pretty_title = namespace_map.pretty_title(namespace, &title);
            println!("{pretty_title}\t{count}");
        }
    } else {
        let namespace_transclusion_counts =
            link_targets.fold(Map::new(), |mut map, LinkTarget { id, namespace, .. }| {
                if let Some(count) = link_target_counts.get(&id).copied() {
                    *map.entry(namespace).or_insert(0) += count;
                }
                map
            });

        // Add namespace names.
        let mut namespace_transclusion_counts_list: Vec<_> = namespace_transclusion_counts
            .into_iter()
            .map(|(namespace, count)| {
                (
                    namespace.into_inner(),
                    &*namespace_map
                        .get_by_id(namespace.into_inner())
                        .unwrap()
                        .name,
                    count,
                )
            })
            .collect();

        // Sort ascending by transclusion count.
        namespace_transclusion_counts_list
            .sort_by(|(_, _, count1), (_, _, count2)| count1.cmp(count2).reverse());

        for (namespace_number, namespace_name, count) in namespace_transclusion_counts_list {
            println!("{namespace_name} ({namespace_number}): {count}");
        }
    }

    Ok(())
}

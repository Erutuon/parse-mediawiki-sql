use anyhow::Result;
use mwtitle::Title;
use parse_mediawiki_sql::{
    field_types::PageNamespace,
    iterate_sql_insertions,
    schemas::{Page, Redirect},
    utils::{memory_map, TitleCodec},
};
use pico_args::{Arguments, Keys};
use std::{
    collections::{BTreeMap as Map, BTreeSet as Set},
    convert::TryFrom,
    path::PathBuf,
};

static USAGE: &str = "
redirects-by-namespace [arguments] namespace...
-p, --page                  path to page.sql [default: page.sql]
-r, --redirect              path to redirect.sql [default: redirect.sql]
-s, --siteinfo-namespaces   path to siteinfo-namespaces.json or
                            siteinfo-namespaces.json.gz
                            [default: siteinfo-namespaces.json]

Multiple namespace ids can be provided as positional arguments.
";

#[allow(clippy::large_enum_variant)]
enum Args {
    Help,
    PrintRedirects {
        page_path: PathBuf,
        redirect_path: PathBuf,
        title_codec: TitleCodec,
        namespaces: Set<PageNamespace>,
    },
}

fn get_args() -> Result<Args> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        return Ok(Args::Help);
    }

    #[allow(clippy::redundant_closure)]
    fn path_from_args(
        args: &mut Arguments,
        keys: impl Into<Keys>,
        default: impl Into<PathBuf>,
    ) -> PathBuf {
        args.value_from_os_str(keys, |opt| PathBuf::try_from(opt))
            .unwrap_or_else(|_| default.into())
    }

    let page_path = path_from_args(&mut args, ["-p", "--page"], "page.sql");
    let redirect_path = path_from_args(&mut args, ["-r", "--redirect"], "redirect.sql");
    let siteinfo_namespaces_path = path_from_args(
        &mut args,
        ["-s", "--siteinfo-namespaces"],
        "siteinfo-namespaces.json",
    );
    let namespaces = args
        .finish()
        .into_iter()
        .map(|os_str| -> Result<PageNamespace, anyhow::Error> {
            let n = os_str
                .into_string()
                .map_err(|_| pico_args::Error::NonUtf8Argument)?;
            Ok(PageNamespace(n.parse::<i32>()?))
        })
        .collect::<Result<Set<_>, _>>()?;
    if namespaces.is_empty() {
        return Err(anyhow::Error::msg(
            "No namespaces provided in positional arguments",
        ));
    }
    let title_codec = TitleCodec::from_path(&siteinfo_namespaces_path)?;
    Ok(Args::PrintRedirects {
        page_path,
        redirect_path,
        title_codec,
        namespaces,
    })
}

// Takes a list of namespaces, which must be parsable as `i32`,
// as arguments. Expects page.sql and redirect.sql and siteinfo-namespaces.json
// in the current directory.
fn main() -> anyhow::Result<()> {
    let (page_path, redirect_path, title_codec, namespaces) = match get_args()? {
        Args::PrintRedirects {
            page_path,
            redirect_path,
            title_codec,
            namespaces,
        } => (page_path, redirect_path, title_codec, namespaces),
        Args::Help => {
            eprintln!("{}", USAGE);
            return Ok(());
        }
    };
    let page_sql = unsafe { memory_map(&page_path)? };
    let redirect_sql = unsafe { memory_map(&redirect_path)? };
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
             }| {
                (
                    id,
                    Title::new_unchecked(namespace.into_inner(), &title.into_inner()),
                )
            },
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
                id_to_title.get(&from).map(|from| {
                    (
                        from,
                        Title::new_unchecked(namespace.into_inner(), &title.into_inner()),
                    )
                })
            },
        )
        .collect();
    for (k, v) in source_to_target {
        println!(
            "{}\t{}",
            title_codec.prefixed_text(k),
            title_codec.prefixed_text(&v)
        );
    }
    Ok(())
}

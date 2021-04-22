use joinery::Joinable;
use parse_mediawiki_sql::{iterate_sql_insertions, utils::memory_map, FromSqlTuple};
use std::path::PathBuf;

fn print_row_count<'a, T: FromSqlTuple<'a> + 'a>(sql_script: &'a [u8]) {
    let mut iter = iterate_sql_insertions::<'a, T>(sql_script);
    let count = iter.count();
    match iter.finish() {
        Ok(_) => {
            println!("{} rows parsed", count);
        }
        Err(e) => match e {
            nom::Err::Incomplete(_) => {
                eprintln!("Needed more data");
            }
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                eprintln!("{}", e);
            }
        },
    }
}

macro_rules! do_with_table {
    (
        $function:ident::<
            match $table_name:ident {
                $(
                $table:ident => $type:ident
                ),+
                $(,)?
            }
        >(&$script:ident)

    ) => {
        use parse_mediawiki_sql::schemas::*;
        match $table_name {
            $(
                stringify!($table) => $function::<$type>(&$script),
            )+
            _ => {
                eprintln!(
                    "Expected table name “{}” to be one of {}",
                    $table_name,
                    [
                        $(
                            stringify!($table),
                        )+
                    ].join_with(", "),
                );
                std::process::exit(1);
            }
        }

    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let args = (args.next().map(PathBuf::from), args.next());
    let (sql_path, table) = match &args {
        (Some(sql_path), Some(table)) => {
            if let Some(table) = table.to_str() {
                (sql_path, table)
            } else {
                eprintln!("expected SQL table name (second argument) to be valid UTF-8");
                std::process::exit(1);
            }
        }
        (Some(sql_path), None) => {
            if let Some(table) = sql_path.file_stem().and_then(|s| s.to_str()) {
                (sql_path, table)
            } else {
                return Err(anyhow::Error::msg("no SQL table name (second argument); expected file stem (first argument) to be valid UTF-8"));
            }
        }
        (None, None) => {
            return Err(anyhow::Error::msg(
                "expected path to SQL script as first argument",
            ));
        }
        _ => unreachable!("test"),
    };

    let script = unsafe { memory_map(&sql_path)? };

    do_with_table! {
        print_row_count::<
            match table {
                category => Category,
                categorylinks => CategoryLink,
                change_tag_def => ChangeTagDefinition,
                change_tag => ChangeTag,
                externallinks => ExternalLink,
                image => Image,
                imagelinks => ImageLink,
                iwlinks => InterwikiLink,
                langlinks => LanguageLink,
                page_restrictions => PageRestriction,
                page => Page,
                pagelinks => PageLink,
                page_props => PageProperty,
                protected_titles => ProtectedTitle,
                redirect => Redirect,
                sites => Site,
                site_stats => SiteStats,
                templatelinks => TemplateLink,
                user_former_groups => UserFormerGroupMembership,
                user_groups => UserGroupMembership,
                wbc_entity_usage => WikibaseClientEntityUsage,
            }
        >(&script)
    }

    Ok(())
}

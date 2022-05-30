use joinery::prelude::*;
use parse_mediawiki_sql::{schemas::Page, FromSqlTuple};

fn main() {
    let page_tuple: Vec<&'static str> = concat!(
        "(7,4,'GNU_Free_Documentation_License','',0,0,0.492815242607906,",
        "'20200201151554','20200201151623',28863815,2777,'wikitext',NULL)"
    )
    .strip_prefix('(')
    .unwrap()
    .strip_suffix(')')
    .unwrap()
    .split(',')
    .collect();
    let test = "(7,66.6,'GNU_Free_Documentation_License','',0,0,0.492815242607906,'20200201151554','20200201151623',28863815,2777,'wikitext',NULL)".as_bytes();
    // For some reason the error is reported at `.6` rather than at `66.6` where the parser would see that there is no `'`.
    let res = Page::from_sql_tuple(test);
    match res {
        Ok((_, page)) => {
            println!("{:?}", page);
        }
        Err(e) => match e {
            nom::Err::Incomplete(_) => println!("incomplete"),
            nom::Err::Error(e) | nom::Err::Failure(e) => {
                println!("{}", e);
            }
        },
    }
    for i in 0..page_tuple.len() {
        for random_value in &["666", "'666'", "66.6", "NULL"] {
            let mut bad_page_tuple: String = page_tuple
                .iter()
                .enumerate()
                .map(|(j, segment)| if j == i { random_value } else { segment })
                .join_with(",")
                .to_string();
            bad_page_tuple.insert(0, '(');
            bad_page_tuple.push(')');
            match Page::from_sql_tuple(bad_page_tuple.as_bytes()) {
                Ok((_, page)) => {
                    println!("{:?}", &page);
                    #[cfg(feature = "serialization")]
                    {
                        serde_json::to_writer(std::io::stdout(), &page).unwrap();
                        println!();
                    }
                }
                Err(e) => match e {
                    nom::Err::Incomplete(_) => println!("incomplete"),
                    nom::Err::Error(e) | nom::Err::Failure(e) => {
                        println!("{}", e);
                    }
                },
            }
            println!();
        }
    }
}

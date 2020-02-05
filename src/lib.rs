use crate::types::FromSQL;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while},
    character::complete::multispace0,
    combinator::{iterator, opt, recognize, ParserIterator},
    error::ErrorKind,
    sequence::{preceded, tuple},
    IResult,
};

pub mod schemas;
pub mod types;

/// Returns an iterator over the values represented in the SQL dump of
/// a MediaWiki database.
pub fn iterate_sql_insertions<'a, T>(
    sql: &'a str,
) -> ParserIterator<
    &'a str,
    (&str, ErrorKind),
    impl Fn(&'a str) -> IResult<&'a str, T, (&str, ErrorKind)>,
>
where
    T: FromSQL<'a> + 'a,
{
    let sql = &sql[sql.find("INSERT INTO").unwrap()..];
    iterator(
        sql,
        preceded(
            alt((
                recognize(tuple((
                    opt(multispace0),
                    opt(tag(";")),
                    opt(multispace0),
                    tuple((
                        tag("INSERT INTO `"),
                        take_while(|b| 'a' <= b && b <= 'z' || b == '_'),
                        tag("` VALUES "),
                    )),
                ))),
                tag(","),
            )),
            FromSQL::from_sql,
        ),
    )
}

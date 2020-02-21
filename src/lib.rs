use crate::types::FromSQL;
use bstr::{ByteSlice, B};
use nom::{
    branch::alt,
    bytes::streaming::{tag, take_while},
    character::streaming::multispace0,
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
    sql: &'a [u8],
) -> ParserIterator<
    &'a [u8],
    (&'a [u8], ErrorKind),
    impl Fn(&'a [u8]) -> IResult<&'a [u8], T, (&'a [u8], ErrorKind)>,
>
where
    T: FromSQL<'a> + 'a,
{
    let sql = &sql[sql.find("INSERT INTO").expect("INSERT INTO statement")..];
    iterator(
        sql,
        preceded(
            alt((
                recognize(tuple((
                    opt(multispace0),
                    opt(tag(";")),
                    opt(multispace0),
                    tuple((
                        tag(B("INSERT INTO `")),
                        take_while(|b| b'a' <= b && b <= b'z' || b == b'_'),
                        tag(B("` VALUES ")),
                    )),
                ))),
                tag(","),
            )),
            FromSQL::from_sql,
        ),
    )
}

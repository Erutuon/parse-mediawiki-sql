/*!
Defines the [`FromSql`] trait and implements it for standard Rust types
and for [`NotNan`], so that they can be parsed from SQL syntax.
*/

use bstr::B;
use nom::{
    branch::alt,
    bytes::streaming::{escaped_transform, is_not, tag},
    character::streaming::{char, digit1, one_of},
    combinator::{map, opt, recognize},
    error::context,
    number::streaming::recognize_float,
    sequence::{preceded, terminated, tuple},
};
use ordered_float::NotNan;

pub type IResult<'a, T> = nom::IResult<&'a [u8], T, crate::error::Error<'a>>;

/**
Trait for converting from the SQL syntax for a simple type
(anything other than a tuple) to a Rust type,
which can borrow from the string or not.
Used by [`schemas::FromSqlTuple`][crate::FromSqlTuple].
*/
pub trait FromSql<'a>: Sized {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self>;
}


impl<'a> FromSql<'a> for bool {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("1 or 0", map(one_of("01"), |b| b == '1'))(s)
    }
}

// This won't panic if the SQL file is valid and the parser is using
// the correct numeric types.
macro_rules! number_impl {
    ($type_name:ty $implementation:block ) => {
        impl<'a> FromSql<'a> for $type_name {
            fn from_sql(s: &'a [u8]) -> IResult<'a, $type_name> {
                context(
                    concat!("number (", stringify!($type_name), ")"),
                    map($implementation, |num: &[u8]| {
                        std::str::from_utf8(num)
                            .expect(concat!("valid UTF-8 in ", stringify!($type_name)))
                            .parse()
                            .expect(concat!("valid ", stringify!($type_name)))
                    }),
                )(s)
            }
        }
    };
    ($type_name:ty $implementation:block $further_processing:block ) => {
        impl<'a> FromSql<'a> for $type_name {
            fn from_sql(s: &'a [u8]) -> IResult<'a, $type_name> {
                context(
                    concat!("number (", stringify!($type_name), ")"),
                    map($implementation, $further_processing),
                )(s)
            }
        }
    };
}

macro_rules! unsigned_int {
    ($t:ident) => {
        number_impl! { $t { recognize(digit1) } }
    };
}

unsigned_int!(u8);
unsigned_int!(u16);
unsigned_int!(u32);
unsigned_int!(u64);

macro_rules! signed_int {
    ($t:ident) => {
        number_impl! { $t { recognize(tuple((opt(char('-')), digit1))) } }
    };
}

signed_int!(i8);
signed_int!(i16);
signed_int!(i32);
signed_int!(i64);

macro_rules! float {
    ($t:ident) => {
        number_impl! { $t { recognize_float } }
        number_impl! {
            NotNan<$t> {
                <$t>::from_sql
            } {
                |float| NotNan::new(float).expect("non-NaN")
            }
        }
    };
}

float!(f32);
float!(f64);

/// Use this for byte strings that have no escape sequences.
impl<'a> FromSql<'a> for &'a [u8] {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "byte string with no escape sequences",
            preceded(
                tag("'"),
                terminated(
                    map(opt(is_not(B("'"))), |opt| opt.unwrap_or_else(|| B(""))),
                    tag("'"),
                ),
            ),
        )(s)
    }
}

/// Use this for string-like types that have no escape sequences,
/// like timestamps, which only contain `[0-9: -]`.
impl<'a> FromSql<'a> for &'a str {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "string with no escape sequences",
            map(<&[u8]>::from_sql, |bytes| {
                std::str::from_utf8(bytes).expect("valid UTF-8 in unescaped string")
            }),
        )(s)
    }
}

/// Use this for string types that require unescaping and are guaranteed
/// to be valid UTF-8, like page titles.
impl<'a> FromSql<'a> for String {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "string",
            map(<Vec<u8>>::from_sql, |s| {
                String::from_utf8(s).expect("valid UTF-8 in potentially escaped string")
            }),
        )(s)
    }
}

/// This is used for "strings" that sometimes contain invalid UTF-8, like the
/// `cl_sortkey` field in the `categorylinks` table, which is truncated to 230
// bits, sometimes in the middle of a UTF-8 sequence.
impl<'a> FromSql<'a> for Vec<u8> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "byte string",
            preceded(
                tag("'"),
                terminated(
                    map(
                        opt(escaped_transform(
                            is_not(B("\\\"'")),
                            '\\',
                            map(one_of(B(r#"0btnrZ\'""#)), |b| match b {
                                '0' => B("\0"),
                                'b' => b"\x08",
                                't' => b"\t",
                                'n' => b"\n",
                                'r' => b"\r",
                                'Z' => b"\x1A",
                                '\\' => b"\\",
                                '\'' => b"'",
                                '"' => b"\"",
                                _ => unreachable!(),
                            }),
                        )),
                        |opt| opt.unwrap_or_else(Vec::new),
                    ),
                    tag("'"),
                ),
            ),
        )(s)
    }
}

impl<'a> FromSql<'a> for () {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("unit type", map(tag("NULL"), |_| ()))(s)
    }
}

impl<'a, T> FromSql<'a> for Option<T>
where
    T: FromSql<'a>,
{
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "optional type",
            alt((
                context("“NULL”", map(<()>::from_sql, |_| None)),
                map(T::from_sql, Some),
            )),
        )(s)
    }
}

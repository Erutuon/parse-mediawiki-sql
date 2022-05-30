/*!
Defines the [`FromSql`] trait and implements it for external types.
*/

use bstr::B;
use either::Either;
use nom::{
    branch::alt,
    bytes::streaming::{escaped_transform, is_not, tag},
    character::streaming::{char, digit1, one_of},
    combinator::{map, map_res, opt, recognize},
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

/// Parses a [`bool`] from `1` or `0`.
impl<'a> FromSql<'a> for bool {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("1 or 0", map(one_of("01"), |b| b == '1'))(s)
    }
}

// This won't panic if the SQL file is valid and the parser is using
// the correct numeric types.
macro_rules! number_impl {
    (
        $( #[doc = $com:expr] )*
        $type_name:ty
        $implementation:block
    ) => {
        $( #[doc = $com] )*
        impl<'a> FromSql<'a> for $type_name {
            fn from_sql(s: &'a [u8]) -> IResult<'a, $type_name> {
                context(
                    concat!("number (", stringify!($type_name), ")"),
                    map_res($implementation, |num: &[u8]| {
                        let s = std::str::from_utf8(num).map_err(Either::Right)?;
                        s.parse().map_err(Either::Left)
                    }),
                )(s)
            }
        }
    };
    (
        $( #[doc = $com:expr] )*
        $type_name:ty
        $implementation:block
        $further_processing:block
    ) => {
        $( #[doc = $com] )*
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
        number_impl! {
            #[doc = concat!("Matches a float literal with [`recognize_float`] and parses it as a [`", stringify!($t), "`].")]
            $t { recognize_float }
        }

        number_impl! {
            // Link to `<$t as FromSql>::from_sql` when https://github.com/rust-lang/rust/issues/74563 is resolved.
            #[doc = concat!("Parses an [`", stringify!($t), "`] and wraps it with [`NotNan::new_unchecked`].")]
            ///
            /// # Safety
            /// This will never accidentally wrap a `NaN` because `nom`'s [`recognize_float`] doesn't include a representation of `NaN`.
            NotNan<$t> {
                <$t>::from_sql
            } {
                |float| unsafe { NotNan::new_unchecked(float) }
            }
        }
    };
}

float!(f32);
float!(f64);

/// Used for byte strings that have no escape sequences.
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

/// Used for types represented as strings without escape sequences. For instance,
/// [`Timestamp`](crate::field_types::Timestamp)s matches the regex `^[0-9: -]+$`
/// and thus never has any escape sequences.
impl<'a> FromSql<'a> for &'a str {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "string with no escape sequences",
            map_res(<&[u8]>::from_sql, std::str::from_utf8),
        )(s)
    }
}

/// Use this for string types that require unescaping and are guaranteed
/// to be valid UTF-8, like page titles.
impl<'a> FromSql<'a> for String {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("string", map_res(<Vec<u8>>::from_sql, String::from_utf8))(s)
    }
}

/// Used for "strings" that sometimes contain invalid UTF-8, like the
/// `cl_sortkey` field in the `categorylinks` table, which is truncated to 230
/// bits, sometimes in the middle of a UTF-8 sequence.
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
                        |opt| opt.unwrap_or_default(),
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

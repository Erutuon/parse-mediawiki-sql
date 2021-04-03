/*!
Defines the types used in the [`schemas`](crate::schemas) module
and implements the [`FromSql`] trait for these and other types,
so that they can be parsed from SQL syntax.
Re-exports the [`Datelike`] and [`Timelike`] traits from the [`chrono`] crate,
which are used by [`Timestamp`].
*/

use bstr::{BStr, ByteSlice, B};
use joinery::prelude::*;
use nom::{
    branch::alt,
    bytes::streaming::{escaped_transform, is_not, tag},
    character::streaming::{char, digit1, one_of},
    combinator::{map, map_res, opt, recognize},
    error::{context, ContextError, ErrorKind, FromExternalError, ParseError},
    multi::many1,
    number::streaming::recognize_float,
    sequence::{delimited, pair, preceded, terminated, tuple},
};
use ordered_float::NotNan;
use std::{
    collections::BTreeMap,
    convert::TryFrom,
    fmt::Display,
    iter::FromIterator,
    ops::{Deref, Index},
    str::FromStr,
};

#[cfg(feature = "serialization")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "smartstring")]
use smartstring::alias::String;

#[cfg(feature = "smartstring")]
use std::string::String as StdString;

/// The type that [`Timestamp`] derefs to, from `chrono`.
pub use chrono::NaiveDateTime;

/// Trait for [`Timestamp`], re-exported from `chrono`.
pub use chrono::{Datelike, Timelike};

pub type IResult<'a, T> = nom::IResult<&'a [u8], T, Error<'a>>;

/**
Trait for converting from the SQL syntax for a simple type
(anything other than a tuple) to a Rust type,
which can borrow from the string or not.
Used by [`schemas::FromSqlTuple`][crate::FromSqlTuple].
*/
pub trait FromSql<'a>: Sized {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ParseTypeContext<'a> {
    Single {
        input: &'a BStr,
        label: &'static str,
    },
    Alternatives {
        input: &'a BStr,
        labels: Vec<&'static str>,
    },
}

impl<'a> ParseTypeContext<'a> {
    fn push(&mut self, other: Self) {
        match self {
            ParseTypeContext::Single { label: label1, .. } => match other {
                ParseTypeContext::Single {
                    input,
                    label: label2,
                } => {
                    *self = ParseTypeContext::Alternatives {
                        input,
                        labels: vec![label1, label2],
                    }
                }
                ParseTypeContext::Alternatives {
                    input,
                    labels: mut labels2,
                } => {
                    labels2.insert(0, label1);
                    *self = ParseTypeContext::Alternatives {
                        input,
                        labels: labels2,
                    }
                }
            },
            ParseTypeContext::Alternatives {
                labels: labels1, ..
            } => match other {
                ParseTypeContext::Single { label: label2, .. } => {
                    labels1.push(label2);
                }
                ParseTypeContext::Alternatives {
                    labels: labels2, ..
                } => {
                    labels1.extend(labels2);
                }
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Error<'a> {
    ErrorKind { input: &'a BStr, kind: ErrorKind },
    ErrorWithContexts(Vec<ParseTypeContext<'a>>),
}

impl<'a> ParseError<&'a [u8]> for Error<'a> {
    fn from_error_kind(input: &'a [u8], kind: ErrorKind) -> Self {
        Self::ErrorKind {
            input: input.into(),
            kind,
        }
    }

    // Bubble up ErrorWithContext and skip ErrorKind.
    fn append(input: &'a [u8], kind: ErrorKind, other: Self) -> Self {
        match other {
            Self::ErrorKind { .. } => Self::from_error_kind(input, kind),
            e @ Self::ErrorWithContexts(_) => e,
        }
    }

    fn from_char(input: &'a [u8], _: char) -> Self {
        Self::from_error_kind(input, ErrorKind::Char)
    }

    fn or(self, other: Self) -> Self {
        match self {
            Error::ErrorKind { .. } => match other {
                Error::ErrorKind { input, kind } => Self::from_error_kind(input, kind),
                e @ Error::ErrorWithContexts(_) => e,
            },
            Error::ErrorWithContexts(mut contexts) => match other {
                Error::ErrorKind { .. } => Error::ErrorWithContexts(contexts),
                Error::ErrorWithContexts(mut other_contexts) => {
                    if let (Some(mut old_context), Some(new_context)) =
                        (contexts.pop(), other_contexts.pop())
                    {
                        old_context.push(new_context);
                        other_contexts.push(old_context);
                    };
                    Error::ErrorWithContexts(other_contexts)
                }
            },
        }
    }
}

impl<'a> ContextError<&'a [u8]> for Error<'a> {
    fn add_context(input: &'a [u8], label: &'static str, other: Self) -> Self {
        let context = ParseTypeContext::Single {
            input: input.into(),
            label,
        };
        match other {
            Self::ErrorKind { .. } => Self::ErrorWithContexts(vec![context]),
            Self::ErrorWithContexts(mut contexts) => {
                contexts.push(context);
                Self::ErrorWithContexts(contexts)
            }
        }
    }
}

impl<'a, I: Into<&'a [u8]>, E> FromExternalError<I, E> for Error<'a> {
    fn from_external_error(input: I, kind: ErrorKind, _e: E) -> Self {
        Self::from_error_kind(input.into(), kind)
    }
}

const INPUT_GRAPHEMES_TO_SHOW: usize = 100;

impl<'a> Display for Error<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn show_input(input: &BStr) -> &BStr {
            if input.is_empty() {
                return input;
            }
            // Try to get a whole SQL tuple.
            if input[0] == b'(' {
                if let Ok((_, row)) = recognize(delimited(
                    char('('),
                    many1(pair(
                        alt((
                            tag("NULL"),
                            recognize(f64::from_sql),
                            recognize(i64::from_sql),
                            recognize(<Vec<u8>>::from_sql),
                        )),
                        opt(char(',')),
                    )),
                    char(')'),
                ))(input)
                {
                    return row.into();
                }
            }
            // Try to get one element of the SQL tuple.
            if let Ok((_, result)) = alt((
                tag("NULL"),
                recognize(f64::from_sql),
                recognize(i64::from_sql),
                recognize(<Vec<u8>>::from_sql),
            ))(input)
            {
                result.into()
            // Get up to a maximum number of characters.
            } else {
                let (_, end, _) = input
                    .grapheme_indices()
                    .take(INPUT_GRAPHEMES_TO_SHOW)
                    .last()
                    .expect("we have checked that input is not empty");
                &input[..end]
            }
        }

        match self {
            Error::ErrorKind { input, kind } => write!(
                f,
                "error in {} combinator at\n\t{}",
                kind.description(),
                show_input(input),
            ),
            Error::ErrorWithContexts(contexts) => {
                match contexts.as_slice() {
                    [] => {
                        write!(f, "unknown error")?;
                    }
                    [first, rest @ ..] => {
                        let mut last_input = match first {
                            ParseTypeContext::Single { input, label } => {
                                write!(f, "expected {} at\n\t{}\n", label, show_input(input),)?;
                                input
                            }
                            ParseTypeContext::Alternatives { input, labels } => {
                                write!(
                                    f,
                                    "expected {} at \n\t{}\n",
                                    labels.iter().join_with(" or "),
                                    show_input(input),
                                )?;
                                input
                            }
                        };
                        for context in rest {
                            let labels_joined;
                            let (displayed_label, input): (&dyn Display, _) = match context {
                                ParseTypeContext::Single { input, label } => {
                                    let displayed_input = if last_input == input {
                                        None
                                    } else {
                                        Some(input)
                                    };
                                    last_input = input;
                                    (label, displayed_input)
                                }
                                ParseTypeContext::Alternatives { input, labels } => {
                                    let displayed_input = if last_input == input {
                                        None
                                    } else {
                                        Some(input)
                                    };
                                    labels_joined = labels.iter().join_with(" or ");
                                    last_input = input;
                                    (&labels_joined, displayed_input)
                                }
                            };
                            write!(f, "while parsing {}", displayed_label,)?;
                            if let Some(input) = input {
                                write!(f, " at\n\t{}", show_input(input),)?;
                            }
                            writeln!(f)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }
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

#[cfg(feature = "serialization")]
pub(crate) fn serialize_not_nan<S>(not_nan: &NotNan<f64>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_f64(not_nan.into_inner())
}

#[cfg(feature = "serialization")]
pub(crate) fn deserialize_not_nan<'de, D>(deserializer: D) -> Result<NotNan<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    NotNan::new(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

#[cfg(feature = "serialization")]
pub(crate) fn serialize_option_not_nan<S>(
    not_nan: &Option<NotNan<f64>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    not_nan.map(|n| n.into_inner()).serialize(serializer)
}

#[cfg(feature = "serialization")]
pub(crate) fn deserialize_option_not_nan<'de, D>(
    deserializer: D,
) -> Result<Option<NotNan<f64>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(<Option<f64>>::deserialize(deserializer)?
        .map(|v| NotNan::new(v).map_err(serde::de::Error::custom))
        .transpose()?)
}

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
                #[cfg(not(feature = "smartstring"))]
                {
                    String::from_utf8(s).expect("valid UTF-8 in potentially escaped string")
                }

                #[cfg(feature = "smartstring")]
                {
                    String::from(
                        StdString::from_utf8(s).expect("valid UTF-8 in potentially escaped string"),
                    )
                }
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

macro_rules! impl_wrapper {
    // $l1 and $l2 must be identical.
    (
        $(#[$attrib:meta])*
        $wrapper:ident<$l1:lifetime>: &$l2:lifetime $wrapped_type:ty
    ) => {
        impl_wrapper! {
            @maybe_copy [&$l2 $wrapped_type]
            $(#[$attrib])*
            #[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
            pub struct $wrapper<$l1>(&$l2 $wrapped_type);

            impl<$l1> FromSql<$l1> for $wrapper<$l1> {
                fn from_sql(s: &$l1 [u8]) -> IResult<$l1, Self> {
                    context(
                        stringify!($wrapper),
                        map(<&$l2 $wrapped_type>::from_sql, $wrapper)
                    )(s)
                }
            }

            #[allow(unused)]
            impl<$l1> $wrapper<$l1> {
                pub fn into_inner(self) -> &$l2 $wrapped_type {
                    self.into()
                }
            }

            impl<$l1> From<$wrapper<$l1>> for &$l2 $wrapped_type {
                fn from(val: $wrapper<$l1>) -> Self {
                    val.0
                }
            }

            impl<$l1> From<&$l2 $wrapped_type> for $wrapper<$l1> {
                fn from(val: &$l2 $wrapped_type) -> Self {
                    Self(val)
                }
            }
        }
    };
    (
        $(#[$attrib:meta])*
        $wrapper:ident: $wrapped:ident
    ) => {
        impl_wrapper! {
            @maybe_copy [$wrapped]
            $(#[$attrib])*
            #[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
            pub struct $wrapper($wrapped);

            impl<'input> FromSql<'input> for $wrapper {
                fn from_sql(s: &'input [u8]) -> IResult<'input, Self> {
                    context(
                        stringify!($wrapper),
                        map(<$wrapped>::from_sql, $wrapper)
                    )(s)
                }
            }

            #[allow(unused)]
            impl $wrapper {
                pub fn into_inner(self) -> $wrapped {
                    self.into()
                }
            }

            impl From<$wrapper> for $wrapped {
                fn from(val: $wrapper) -> Self {
                    val.0
                }
            }

            impl<'a> From<&'a $wrapper> for &'a $wrapped {
                fn from(val: &'a $wrapper) -> Self {
                    &val.0
                }
            }

            impl From<$wrapped> for $wrapper {
                fn from(val: $wrapped) -> Self {
                    Self(val)
                }
            }
        }
    };
    (
        @maybe_copy [$(u32)? $(i32)? $(&$l:lifetime $t:ty)?]
        $($rest:item)+
    ) => {
        #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
        $($rest)+
    };
    (
        @maybe_copy [$($anything:tt)?]
        $($rest:item)+
    ) => {
        #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
        $($rest)+
    };
}

impl_wrapper! {
    #[doc = "
Represents
[`page_id`](https://www.mediawiki.org/wiki/Manual:Page_table#page_id),
the primary key of the `page` table, as well as other fields in
other tables that correspond to it.
"]
    PageId: u32
}

impl_wrapper! {
    #[doc = "
Represents the
[`page_namespace`](https://www.mediawiki.org/wiki/Manual:Page_table#page_namespace)
field of the `page` table.
"]
    PageNamespace: i32
}

impl_wrapper! {
    #[doc="
Represents the
[`page_title`](https://www.mediawiki.org/wiki/Manual:Page_table#page_title)
field of the `page` table, a title with underscores.
"]
    PageTitle: String
}

impl_wrapper! {
    #[doc="
Represents a page title with namespace and with spaces rather than underscores,
as in the
[`ll_title`](https://www.mediawiki.org/wiki/Manual:Langlinks_table#ll_title)
field of the `langlinks` table.
"]
    FullPageTitle: String
}

impl_wrapper! {
    #[doc = "
Represents
[`cat_id`](https://www.mediawiki.org/wiki/Manual:Category_table#cat_id),
the primary key of the `category` table.
"]
    CategoryId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`cat_pages`](https://www.mediawiki.org/wiki/Manual:Category_table#cat_id),
[`cat_subcats`](https://www.mediawiki.org/wiki/Manual:Category_table#cat_subcats),
and [`cat_files`](https://www.mediawiki.org/wiki/Manual:Category_table#cat_files)
fields of the `category` table. They should logically be greater than
or equal to 0, but because of errors can be negative.
"]
    PageCount: i32
}

impl_wrapper! {
    #[doc = "
Represents
[`log_id`](https://www.mediawiki.org/wiki/Manual:Logging_table#log_id),
the primary key of the `logging` table.
"]
    LogId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`ct_id`](https://www.mediawiki.org/wiki/Manual:Change_tag_table#ct_id),
the primary key of the `change_tag` table.
"]
    ChangeTagId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`rev_id`](https://www.mediawiki.org/wiki/Manual:Revision_table#rev_id),
the primary key of the `revision` table.
"]
    RevisionId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`ctd_id`](https://www.mediawiki.org/wiki/Manual:Change_tag_def_table#ctd_id),
the primary key of the `change_tag_def` table.
"]
    ChangeTagDefId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`rc_id`](https://www.mediawiki.org/wiki/Manual:Recentchanges_table#rc_id),
the primary key of the `recentchanges` table.
"]
    RecentChangesId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`el_id`](https://www.mediawiki.org/wiki/Manual:Externallinks_table#el_id),
the primary key of the `externallinks` table.
"]
    ExternalLinksId: u32
}

impl_wrapper! {
    #[doc = "
Represents the
[`img_minor_mime`](https://www.mediawiki.org/wiki/Manual:Image_table#img_minor_mime)
field of the `image` table.
"]
    MinorMime<'a>: &'a str
}

impl_wrapper! {
    #[doc = "
Represents
[`comment_id`](https://www.mediawiki.org/wiki/Manual:Comment_table#comment_id),
the primary key of the `comment` table.
"]
    CommentId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`actor_id`](https://www.mediawiki.org/wiki/Manual:Actor_table#actor_id),
the primary key of the `actor` table.
"]
    ActorId: u32
}

impl_wrapper! {
    #[doc = "
Represents a SHA-1 hash in base 36, for instance in the
[`img_sha1`](https://www.mediawiki.org/wiki/Manual:Image_table#img_sha1)
field of the `image` table.
"]
    Sha1<'a>: &'a str
}

impl_wrapper! {
    #[doc = "
Represents
[`pr_id`](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table#pr_id),
the primary key of the `page_restrictions` table.
"]
    PageRestrictionsId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`user_id`](https://www.mediawiki.org/wiki/Manual:User_table#user_id),
the primary key of the `user` table.
"]
    UserId: u32
}

impl_wrapper! {
    #[doc = "
Represents the name of a user group, such as the
[`ug_group`](https://www.mediawiki.org/wiki/Manual:User_groups_table#ug_group)
field of the `user_groups` table.
"]
    UserGroup<'a>: &'a str
}

#[test]
fn test_copy_for_wrappers() {
    use static_assertions::*;
    assert_impl_all!(PageId: Copy);
    assert_not_impl_all!(PageTitle: Copy);
    assert_impl_all!(PageNamespace: Copy);
    assert_impl_all!(UserGroup: Copy);
}

/// Represents a [timestamp](https://www.mediawiki.org/wiki/Manual:Timestamp)
/// given as a string in `yyyymmddhhmmss` format. Provides the methods of
/// [`NaiveDateTime`] through
/// [`Deref`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
pub struct Timestamp(NaiveDateTime);

impl<'input> FromSql<'input> for Timestamp {
    fn from_sql(s: &'input [u8]) -> IResult<'input, Self> {
        context(
            "Timestamp in yyyymmddhhmmss or yyyy-mm-dd hh:mm::ss format",
            map_res(<&str>::from_sql, |s| {
                NaiveDateTime::parse_from_str(
                    s,
                    if s.len() == 14 {
                        "%Y%m%d%H%M%S"
                    } else {
                        "%Y-%m-%d %H:%M:%S"
                    },
                )
                .map(Timestamp)
            }),
        )(s)
    }
}

impl Deref for Timestamp {
    type Target = NaiveDateTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Represents the
/// [`pr_expiry`](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table#pr_expiry)
/// field of the `page_restrictions` table.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(try_from = "&str", into = "String")
)]
pub enum Expiry {
    Timestamp(Timestamp),
    Infinity,
}

impl TryFrom<&str> for Expiry {
    type Error = <NaiveDateTime as FromStr>::Err;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "infinity" => Ok(Expiry::Infinity),
            s => Ok(Expiry::Timestamp(Timestamp(s.parse()?))),
        }
    }
}

impl From<Expiry> for String {
    fn from(e: Expiry) -> Self {
        match e {
            Expiry::Timestamp(t) => t.to_string().into(),
            Expiry::Infinity => String::from("infinity"),
        }
    }
}

impl<'input> FromSql<'input> for Expiry {
    fn from_sql(s: &'input [u8]) -> IResult<'input, Self> {
        context(
            "Expiry",
            alt((
                map(Timestamp::from_sql, Expiry::Timestamp),
                context("“infinity”", map(tag("'infinity'"), |_| Expiry::Infinity)),
            )),
        )(s)
    }
}

// #[cfg(feature = "serialization")]
// impl Serialize for Expiry {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         match self {
//             Expiry::Timestamp(timestamp) => timestamp.serialize(serializer),
//             Expiry::Infinity => timestamp.serialize_str("infinity"),
//         }
//     }
// }

// #[cfg(feature = "serialization")]
// impl<'de> Deserialize<'de> for Expiry {
//     fn deserialize<D>(deserializer: D) -> Result<Expiry, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         match deserializer.deserialize_str(I32Visitor)? {
//             "infinity" => Ok(Expiry::Infinity),
//             s => Ok(Timestamp::from_str(s)?),
//         }
//     }
// }

/// Represents the
/// [`cl_type`](https://www.mediawiki.org/wiki/Manual:Categorylinks_table#cl_type)
/// field of the `categorylinks` table.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(try_from = "&str", into = "&'static str")
)]
pub enum PageType {
    Page,
    Subcat,
    File,
}

/// Returns the unrecognized string as the error value.
impl<'a> TryFrom<&'a str> for PageType {
    type Error = &'a str;

    fn try_from(s: &'a str) -> Result<Self, &'a str> {
        use PageType::*;
        match s {
            "page" => Ok(Page),
            "subcat" => Ok(Subcat),
            "file" => Ok(File),
            other => Err(other),
        }
    }
}

impl From<PageType> for &'static str {
    fn from(s: PageType) -> &'static str {
        use PageType::*;
        match s {
            Page => "page",
            Subcat => "subcat",
            File => "file",
        }
    }
}

impl<'a> FromSql<'a> for PageType {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("PageType", map_res(<&str>::from_sql, PageType::try_from))(s)
    }
}

/// Represents the
/// [`pr_type`](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table#pr_type)
/// field of the `page_restrictions` table, the action that is restricted.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(from = "&'a str", into = "&'a str")
)]
pub enum PageAction<'a> {
    Edit,
    Move,
    Reply,
    Upload,
    All,
    Other(&'a str),
}

impl<'a> From<&'a str> for PageAction<'a> {
    fn from(s: &'a str) -> Self {
        use PageAction::*;
        match s {
            "edit" => Edit,
            "move" => Move,
            "reply" => Reply,
            "upload" => Upload,
            _ => Other(s),
        }
    }
}

impl<'a> From<PageAction<'a>> for &'a str {
    fn from(p: PageAction<'a>) -> Self {
        use PageAction::*;
        match p {
            Edit => "edit",
            Move => "move",
            Reply => "reply",
            Upload => "upload",
            All => "all",
            Other(s) => s,
        }
    }
}

impl<'a> FromSql<'a> for PageAction<'a> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        map(<&str>::from_sql, PageAction::from)(s)
    }
}

/// Represents the
/// [`pr_level`](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table#pr_level)
/// field of the `page_restrictions` table, the group that is allowed
/// to perform the action.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(from = "&'a str", into = "&'a str")
)]
pub enum ProtectionLevel<'a> {
    Autoconfirmed,
    ExtendedConfirmed,
    Sysop,
    TemplateEditor,
    EditProtected,
    EditSemiProtected,
    /// The result of parsing the empty string after the `=` in `'move=:edit='`.
    None,
    Other(&'a str),
}

impl<'a> From<&'a str> for ProtectionLevel<'a> {
    fn from(s: &'a str) -> Self {
        use ProtectionLevel::*;
        match s {
            "autoconfirmed" => Autoconfirmed,
            "extendedconfirmed" => ExtendedConfirmed,
            "templateeditor" => TemplateEditor,
            "sysop" => Sysop,
            "editprotected" => EditProtected,
            "editsemiprotected" => EditSemiProtected,
            "" => None,
            _ => Other(s),
        }
    }
}

impl<'a> From<ProtectionLevel<'a>> for &'a str {
    fn from(p: ProtectionLevel<'a>) -> &'a str {
        use ProtectionLevel::*;
        match p {
            Autoconfirmed => "autoconfirmed",
            ExtendedConfirmed => "extendedconfirmed",
            TemplateEditor => "templateeditor",
            Sysop => "sysop",
            EditProtected => "editprotected",
            EditSemiProtected => "editsemiprotected",
            None => "",
            Other(s) => s,
        }
    }
}

impl<'a> FromSql<'a> for ProtectionLevel<'a> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "ProtectionLevel",
            map(<&str>::from_sql, ProtectionLevel::from),
        )(s)
    }
}

/**
Represents [`page_restrictions`](https://www.mediawiki.org/wiki/Manual:Page_table#page_restrictions),
an outdated field of the `page` table containing a string representing
a map from action to the groups that are allowed to perform that action.

Here the action is represented by [`PageAction`]
and the protection level by [`ProtectionLevel`].
This field was replaced by the
[`page_restrictions` table](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table)
in MediaWiki 1.10, but is still used by the software if a page's restrictions have not
been changed since MediaWiki 1.10 came out.

The string is in the following format, at least on the English Wiktionary:
```txt
# level on its own seems to be shorthand for both "edit" and "move"
"" | level | spec (":" spec)*
spec: action "=" level
# "" means no restrictions
level: "autoconfirmed" | "templateeditor" | "sysop" | ""
action: "edit" | "move" | "upload"
```
However, `spec` is treated as having the following format, because the
documentation for this field gives the example
`edit=autoconfirmed,sysop:move=sysop` with multiple protection levels per
action:
```txt
spec: action "=" level ("," level)*
```

The example given is nonsensical because users in the `sysop` group have
all the rights of users in the `autoconfirmed` group, and neither English
Wikipedia nor English Wiktionary have any `page_restrictions` strings in this
format, but perhaps these types of protection strings have existed in the past
or exist now on other wikis.
*/
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(
        from = "BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>",
        into = "BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>"
    )
)]
pub struct PageRestrictionsOld<'a>(
    #[cfg_attr(feature = "serialization", serde(borrow))]
    BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>,
);

impl<'a> PageRestrictionsOld<'a> {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PageAction<'a>, &Vec<ProtectionLevel<'a>>)> {
        self.0.iter()
    }
}

impl<'a> Index<PageAction<'a>> for PageRestrictionsOld<'a> {
    type Output = Vec<ProtectionLevel<'a>>;

    fn index(&self, idx: PageAction<'a>) -> &Self::Output {
        &self.0[&idx]
    }
}

impl<'a> FromIterator<(PageAction<'a>, Vec<ProtectionLevel<'a>>)> for PageRestrictionsOld<'a> {
    fn from_iter<I: IntoIterator<Item = (PageAction<'a>, Vec<ProtectionLevel<'a>>)>>(
        iter: I,
    ) -> Self {
        PageRestrictionsOld(iter.into_iter().collect())
    }
}

impl<'a> From<BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>> for PageRestrictionsOld<'a> {
    fn from(map: BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>) -> Self {
        Self(map)
    }
}

impl<'a> From<PageRestrictionsOld<'a>> for BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>> {
    fn from(PageRestrictionsOld(map): PageRestrictionsOld<'a>) -> Self {
        map
    }
}

impl<'a> FromSql<'a> for PageRestrictionsOld<'a> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context(
            "PageRestrictionsOld",
            map_res(<&str>::from_sql, |contents| -> Result<_, &'static str> {
                Ok(PageRestrictionsOld(
                    contents
                        .split(':')
                        .filter(|p| !p.is_empty())
                        .map(|restriction| -> Result<_, &'static str> {
                            let mut type_and_levels = restriction.rsplitn(2, '=');
                            let level = type_and_levels
                                .next()
                                .ok_or("expected page restriction level")?
                                .split(',')
                                .map(|l| l.into())
                                .collect();
                            let action = type_and_levels
                                .next()
                                .map(|a| a.into())
                                .unwrap_or(PageAction::All);
                            Ok((action, level))
                        })
                        .collect::<Result<_, _>>()?,
                ))
            }),
        )(s)
    }
}

#[test]
fn test_page_restrictions() {
    use PageAction::*;
    use ProtectionLevel::*;
    assert_eq!(
        PageRestrictionsOld::from_sql(B("'edit=autoconfirmed:move=sysop'")),
        Ok((
            B(""),
            vec![(Edit, vec![Autoconfirmed]), (Move, vec![Sysop])]
                .into_iter()
                .collect()
        )),
    );
    assert_eq!(
        PageRestrictionsOld::from_sql(B("''")),
        Ok((B(""), PageRestrictionsOld(BTreeMap::new()))),
    );
    assert_eq!(
        PageRestrictionsOld::from_sql(B("'sysop'")),
        Ok((B(""), vec![(All, vec![Sysop])].into_iter().collect())),
    );
    assert_eq!(
        PageRestrictionsOld::from_sql(B("'move=:edit='")),
        Ok((
            B(""),
            vec![(Move, vec![None]), (Edit, vec![None])]
                .into_iter()
                .collect()
        )),
    );
}

/// Represents the
/// [`page_content_model`](https://www.mediawiki.org/wiki/Manual:Page_table#page_content_model)
/// field of the `page` table.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(from = "&'a str", into = "&'a str")
)]
pub enum ContentModel<'a> {
    Wikitext,
    Scribunto,
    Text,
    CSS,
    SanitizedCSS,
    JavaScript,
    JSON,
    #[cfg_attr(feature = "serialization", serde(borrow))]
    Other(&'a str),
}

impl<'a> From<&'a str> for ContentModel<'a> {
    fn from(s: &'a str) -> Self {
        use ContentModel::*;
        match s {
            "wikitext" => Wikitext,
            "Scribunto" => Scribunto,
            "text" => Text,
            "css" => CSS,
            "sanitized-css" => SanitizedCSS,
            "javascript" => JavaScript,
            "json" => JSON,
            _ => Other(s),
        }
    }
}

impl<'a> From<ContentModel<'a>> for &'a str {
    fn from(c: ContentModel<'a>) -> Self {
        use ContentModel::*;
        match c {
            Wikitext => "wikitext",
            Scribunto => "Scribunto",
            Text => "text",
            CSS => "css",
            SanitizedCSS => "sanitized-css",
            JavaScript => "javascript",
            JSON => "json",
            Other(s) => s,
        }
    }
}

impl<'a> FromSql<'a> for ContentModel<'a> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("ContentModel", map(<&str>::from_sql, ContentModel::from))(s)
    }
}

/// Represents the
/// [`img_media_type`](https://www.mediawiki.org/wiki/Manual:Image_table#img_media_type)
/// field of the `image` table.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(from = "&'a str", into = "&'a str")
)]
pub enum MediaType<'a> {
    Unknown,
    Bitmap,
    Drawing,
    Audio,
    Video,
    Multimedia,
    Office,
    Text,
    Executable,
    Archive,
    ThreeDimensional,
    #[cfg_attr(feature = "serialization", serde(borrow))]
    Other(&'a str),
}

impl<'a> From<&'a str> for MediaType<'a> {
    fn from(s: &'a str) -> Self {
        use MediaType::*;
        match s {
            "UNKNOWN" => Unknown,
            "BITMAP" => Bitmap,
            "DRAWING" => Drawing,
            "AUDIO" => Audio,
            "VIDEO" => Video,
            "MULTIMEDIA" => Multimedia,
            "OFFICE" => Office,
            "TEXT" => Text,
            "EXECUTABLE" => Executable,
            "ARCHIVE" => Archive,
            "3D" => ThreeDimensional,
            _ => Other(s),
        }
    }
}

impl<'a> From<MediaType<'a>> for &'a str {
    fn from(s: MediaType<'a>) -> Self {
        use MediaType::*;
        match s {
            Unknown => "UNKNOWN",
            Bitmap => "BITMAP",
            Drawing => "DRAWING",
            Audio => "AUDIO",
            Video => "VIDEO",
            Multimedia => "MULTIMEDIA",
            Office => "OFFICE",
            Text => "TEXT",
            Executable => "EXECUTABLE",
            Archive => "ARCHIVE",
            ThreeDimensional => "3D",
            Other(s) => s,
        }
    }
}

impl<'a> FromSql<'a> for MediaType<'a> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("MediaType", map(<&str>::from_sql, MediaType::from))(s)
    }
}

/// Represents the
/// [`img_major_mime`](https://www.mediawiki.org/wiki/Manual:Image_table#img_major_mime)
/// field of the `image` table.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(
    feature = "serialization",
    derive(Serialize, Deserialize),
    serde(from = "&'a str", into = "&'a str")
)]
pub enum MajorMime<'a> {
    Unknown,
    Application,
    Audio,
    Image,
    Text,
    Video,
    Message,
    Model,
    Multipart,
    #[cfg_attr(feature = "serialization", serde(borrow))]
    Other(&'a str),
}

impl<'a> From<&'a str> for MajorMime<'a> {
    fn from(s: &'a str) -> Self {
        use MajorMime::*;
        match s {
            "unknown" => Unknown,
            "application" => Application,
            "audio" => Audio,
            "image" => Image,
            "text" => Text,
            "video" => Video,
            "message" => Message,
            "model" => Model,
            "multipart" => Multipart,
            _ => Other(s),
        }
    }
}

impl<'a> From<MajorMime<'a>> for &'a str {
    fn from(s: MajorMime<'a>) -> Self {
        use MajorMime::*;
        match s {
            Unknown => "unknown",
            Application => "application",
            Audio => "audio",
            Image => "image",
            Text => "text",
            Video => "video",
            Message => "message",
            Model => "model",
            Multipart => "multipart",
            Other(s) => s,
        }
    }
}

impl<'a> FromSql<'a> for MajorMime<'a> {
    fn from_sql(s: &'a [u8]) -> IResult<'a, Self> {
        context("MajorMime", map(<&str>::from_sql, MajorMime::from))(s)
    }
}

#[test]
fn test_bool() {
    for (s, v) in &[(B("0"), false), (B("1"), true)] {
        assert_eq!(bool::from_sql(s), Ok((B(""), *v)));
    }
}

#[test]
fn test_numbers() {
    fn from_utf8(s: &[u8]) -> &str {
        std::str::from_utf8(s).unwrap()
    }

    // Add a space to the end to avoid `nom::Err::Incomplete`.
    let f = B("0.37569 ");
    let res = f64::from_sql(f);
    assert_eq!(res, Ok((B(" "), from_utf8(f).trim_end().parse().unwrap())));

    for i in &[B("1 "), B("-1 ")] {
        assert_eq!(
            i32::from_sql(i),
            Ok((B(" "), from_utf8(i).trim_end().parse().unwrap()))
        );
    }
}

#[test]
fn test_string() {
    let strings = &[
        (B(r"'\''"), r"'"),
        (br"'\\'", r"\"),
        (br"'\n'", "\n"),
        (br"'string'", r"string"),
        (
            br#"'English_words_ending_in_\"-vorous\",_\"-phagous\"_and_similar_endings'"#,
            r#"English_words_ending_in_"-vorous",_"-phagous"_and_similar_endings"#,
        ),
    ];
    for (s, unescaped) in strings {
        assert_eq!(String::from_sql(s), Ok((B(""), (*unescaped).to_string())));
    }
}

/*!
Defines types that represent fields in tables of the
[MediaWiki database](https://www.mediawiki.org/wiki/Manual:Database_layout),
and the `FromSQL` trait to convert them from SQL syntax.
*/

use chrono::NaiveDateTime;
use nom::{
    branch::alt,
    bytes::complete::{escaped, tag},
    character::complete::{char, digit1, none_of, one_of},
    combinator::{cut, map, opt, recognize},
    number::complete::recognize_float,
    sequence::{preceded, terminated, tuple},
    IResult,
};
use std::{
    borrow::Cow,
    collections::HashMap,
    iter::FromIterator,
    ops::{Deref, Index},
};

/// Trait containing a function that infallibly converts from an SQL string
/// to a Rust type, which can borrow from the string or not.
pub trait FromSQL<'a>: Sized {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self>;
}

impl<'a> FromSQL<'a> for bool {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(one_of("01"), |b| b == '1')(s)
    }
}

// This won't panic if the SQL file is valid and the parser is using
// the correct numeric types.
macro_rules! number_impl {
    ($type_name:ident $implementation:block ) => {
        impl<'a> FromSQL<'a> for $type_name {
            fn from_sql(s: &'a str) -> IResult<&'a str, $type_name> {
                map($implementation, |num: &str| num.parse().unwrap())(s)
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
    };
}

float!(f32);
float!(f64);

/// Characters that are escaped in the MediaWiki SQL dumps.
const ESCAPED: &str = r#"\'""#;

/// Use this for string types that have no escape sequences, like timestamps.
impl<'a> FromSQL<'a> for &'a str {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        preceded(
            char('\''),
            cut(terminated(
                map(
                    opt(escaped(none_of(ESCAPED), '\\', one_of(ESCAPED))),
                    |o| o.unwrap_or(""),
                ),
                char('\''),
            )),
        )(s)
    }
}

/// Use this for string types that require unescaping, like page titles.
impl<'a> FromSQL<'a> for Cow<'a, str> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        preceded(
            char('\''),
            cut(terminated(
                map(
                    opt(escaped(none_of(ESCAPED), '\\', one_of(ESCAPED))),
                    |o: Option<&str>| {
                        o.map(|s| {
                            if s.contains('\\') {
                                Cow::Owned(
                                    s.replace(r"\\", r"\")
                                        .replace(r"\'", r"'")
                                        .replace("\\\"", "\""),
                                )
                            } else {
                                Cow::Borrowed(s)
                            }
                        })
                        .unwrap_or(Cow::Borrowed(""))
                    },
                ),
                char('\''),
            )),
        )(s)
    }
}

impl<'a, T> FromSQL<'a> for Option<T>
where
    T: FromSQL<'a>,
{
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        alt((map(T::from_sql, Some), map(tag("NULL"), |_| None)))(s)
    }
}

macro_rules! impl_wrapper {
    (#[$comment:meta] $wrapper:ident: $wrapped:ty) => {
        #[$comment]
        #[derive(Debug, Clone, Eq, PartialEq, Hash)]
        pub struct $wrapper($wrapped);

        impl<'a> FromSQL<'a> for $wrapper {
            fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
                map(<$wrapped>::from_sql, $wrapper)(s)
            }
        }

        impl From<$wrapper> for $wrapped {
            fn from(val: $wrapper) -> Self {
                val.0
            }
        }
    };
    (#[$comment:meta] $wrapper:ident<$l:lifetime>: $wrapped:ty) => {
        #[$comment]
        #[derive(Debug, Clone, Eq, PartialEq, Hash)]
        pub struct $wrapper<$l>($wrapped);

        impl<$l> FromSQL<$l> for $wrapper<$l> {
            fn from_sql(s: &$l str) -> IResult<&'a str, Self> {
                map(<$wrapped>::from_sql, $wrapper)(s)
            }
        }

        impl<$l, 'b: $l> From<&'b $wrapper<$l>> for &'b $wrapped {
            fn from(val: &'b $wrapper) -> Self {
                &val.0
            }
        }
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
    PageTitle<'a>: Cow<'a, str>
}

impl_wrapper! {
    #[doc="
Represents a page title with namespace and with spaces rather than underscores,
as in the
[`ll_title`](https://www.mediawiki.org/wiki/Manual:Langlinks_table#ll_title)
field of the `langlinks` table.
"]
    FullPageTitle<'a>: Cow<'a, str>
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
    MinorMime<'a>: Cow<'a, str>
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
    UserGroup<'a>: Cow<'a, str>
}

/// Represents a [timestamp](https://www.mediawiki.org/wiki/Manual:Timestamp)
/// given as a string in `yyyymmddhhmmss` format. Provides the methods of
/// [`NaiveDateTime`](../../chrono/naive/struct.NaiveDateTime.html) through
/// `Deref`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Timestamp(NaiveDateTime);

impl<'input> FromSQL<'input> for Timestamp {
    fn from_sql(s: &'input str) -> IResult<&'input str, Self> {
        map(<&str>::from_sql, |s| {
            Timestamp(NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M%S").unwrap())
        })(s)
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
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Expiry {
    Timestamp(Timestamp),
    Infinity,
}

impl<'input> FromSQL<'input> for Expiry {
    fn from_sql(s: &'input str) -> IResult<&'input str, Self> {
        alt((
            map(Timestamp::from_sql, Expiry::Timestamp),
            map(tag("infinity"), |_| Expiry::Infinity),
        ))(s)
    }
}

/// Represents the
/// [`cl_type`](https://www.mediawiki.org/wiki/Manual:Categorylinks_table#cl_type)
/// field of the `categorylinks` table.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PageType<'a> {
    Page,
    Subcat,
    File,
    Other(&'a str),
}

impl<'a> From<&'a str> for PageType<'a> {
    fn from(s: &'a str) -> Self {
        use PageType::*;
        match s {
            "page" => Page,
            "subcat" => Subcat,
            "file" => File,
            _ => Other(s),
        }
    }
}

impl<'a> FromSQL<'a> for PageType<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, PageType::from)(s)
    }
}

/// Represents the
/// [`pr_type`](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table#pr_type)
/// field of the `page_restrictions` table, the action that is restricted.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum PageAction<'a> {
    Edit,
    Move,
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
            "upload" => Upload,
            _ => Other(s),
        }
    }
}

impl<'a> FromSQL<'a> for PageAction<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, PageAction::from)(s)
    }
}

/// Represents the
/// [`pr_level`](https://www.mediawiki.org/wiki/Manual:Page_restrictions_table#pr_level)
/// field of the `page_restrictions` table, the group that is allowed
/// to perform the action.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
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

impl<'a> FromSQL<'a> for ProtectionLevel<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, ProtectionLevel::from)(s)
    }
}

/**
Represents [`page_restrictions`](https://www.mediawiki.org/wiki/Manual:Page_table#page_restrictions),
an outdated field of the `page` table containing a string representing
a map from action to the groups that are allowed to perform that action.

Here the action is represented by [`PageAction`](enum.PageAction.html)
and the protection level by [`ProtectionLevel`](enum.ProtectionLevel.html).
This field was replaced by the [`page_restrictions`] table in MediaWiki
1.10, but is still used by the software if a page's restrictions have not
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
MediaWiki documentation for the `page` table
[gives](https://www.mediawiki.org/wiki/Manual:Page_table#page_restrictions)
the example `edit=autoconfirmed,sysop:move=sysop` with multiple protection
levels per action:

`spec`: `action "=" level ("," level)*`

The example given is nonsensical because `autoconfirmed` is a subset of
`sysop`, and neither English Wikipedia nor English Wiktionary have any
`page_restrictions` strings in this format, but perhaps another wiki does.
*/
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PageRestrictionsOld<'a>(
    HashMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>,
);

impl<'a> PageRestrictionsOld<'a> {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&PageAction<'a>, &Vec<ProtectionLevel<'a>>)>
    {
        self.0.iter()
    }
}

impl<'a> Index<PageAction<'a>> for PageRestrictionsOld<'a> {
    type Output = Vec<ProtectionLevel<'a>>;

    fn index(&self, idx: PageAction<'a>) -> &Self::Output {
        &self.0[&idx]
    }
}

impl<'a> FromIterator<(PageAction<'a>, Vec<ProtectionLevel<'a>>)>
    for PageRestrictionsOld<'a>
{
    fn from_iter<
        I: IntoIterator<Item = (PageAction<'a>, Vec<ProtectionLevel<'a>>)>,
    >(
        iter: I,
    ) -> Self {
        PageRestrictionsOld(iter.into_iter().collect())
    }
}

impl<'a> FromSQL<'a> for PageRestrictionsOld<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, |contents| {
            PageRestrictionsOld(
                contents
                    .split(':')
                    .filter(|p| !p.is_empty())
                    .map(|restriction| {
                        let mut type_and_levels = restriction.rsplitn(2, '=');
                        let level = type_and_levels
                            .next()
                            .unwrap()
                            .split(',')
                            .map(|l| l.into())
                            .collect();
                        let action = type_and_levels
                            .next()
                            .map(|a| a.into())
                            .unwrap_or(PageAction::All);
                        (action, level)
                    })
                    .collect(),
            )
        })(s)
    }
}

#[test]
fn test_page_restrictions() {
    use PageAction::*;
    use ProtectionLevel::*;
    assert_eq!(
        PageRestrictionsOld::from_sql("'edit=autoconfirmed:move=sysop'"),
        Ok((
            "",
            vec![(Edit, vec![Autoconfirmed]), (Move, vec![Sysop])]
                .into_iter()
                .collect()
        )),
    );
    assert_eq!(
        PageRestrictionsOld::from_sql("''"),
        Ok(("", PageRestrictionsOld(HashMap::new()))),
    );
    assert_eq!(
        PageRestrictionsOld::from_sql("'sysop'"),
        Ok(("", vec![(All, vec![Sysop])].into_iter().collect())),
    );
    assert_eq!(
        PageRestrictionsOld::from_sql("'move=:edit='"),
        Ok((
            "",
            vec![(Move, vec![None]), (Edit, vec![None])]
                .into_iter()
                .collect()
        )),
    );
}

/// Represents the
/// [`page_content_model`](https://www.mediawiki.org/wiki/Manual:Page_table#page_content_model)
/// field of the `page` table.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum ContentModel<'a> {
    Wikitext,
    Scribunto,
    Text,
    CSS,
    SanitizedCSS,
    JavaScript,
    JSON,
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

impl<'a> FromSQL<'a> for ContentModel<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, ContentModel::from)(s)
    }
}

/// Represents the
/// [`img_media_type`](https://www.mediawiki.org/wiki/Manual:Image_table#img_media_type)
/// field of the `image` table.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
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
    /// Unfortunately a variant name cannot begin with a number.
    ThreeD,
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
            "3D" => ThreeD,
            _ => Other(s),
        }
    }
}

impl<'a> FromSQL<'a> for MediaType<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, MediaType::from)(s)
    }
}

/// Represents the
/// [`img_major_mime`](https://www.mediawiki.org/wiki/Manual:Image_table#img_major_mime)
/// field of the `image` table.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
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

impl<'a> FromSQL<'a> for MajorMime<'a> {
    fn from_sql(s: &'a str) -> IResult<&'a str, Self> {
        map(<&str>::from_sql, MajorMime::from)(s)
    }
}

/*
pub fn rev_id(s: &str) -> IResult<&str, Option<i64>> {
    nullable_integer(s)
}
*/

#[test]
fn test_bool() {
    for (s, v) in &[("0", false), ("1", true)] {
        assert_eq!(bool::from_sql(s), Ok(("", *v)));
    }
}

#[test]
fn test_numbers() {
    let f = "0.37569";
    let res = f64::from_sql(f);
    assert_eq!(res, Ok(("", f.parse().unwrap())));

    for i in &["1", "-1"] {
        assert_eq!(i32::from_sql(i), Ok(("", i.parse().unwrap())));
    }
}

#[test]
fn test_string() {
    let strings = &[
        (r"'\''", r"'"),
        (r"'\\'", r"\"),
        (r"'string'", r"string"),
        (
            r#"'English_words_ending_in_\"-vorous\",_\"-phagous\"_and_similar_endings'"#,
            r#"English_words_ending_in_"-vorous",_"-phagous"_and_similar_endings"#,
        ),
    ];
    for (s, unescaped) in strings {
        let unchanged = &s[1..s.len() - 1];
        assert_eq!(<&str>::from_sql(s), Ok(("", unchanged)));

        let expected = if *unescaped == unchanged {
            Cow::Borrowed(*unescaped)
        } else {
            Cow::Owned(unescaped.to_string())
        };
        println!("{:?} {:?} {:?}", s, Cow::from_sql(s), expected);
        assert_eq!(Cow::from_sql(s), Ok(("", expected)));
    }
}

/*
#[test]
fn test_page_restrictions() {
    let raw = "'edit=autoconfirmed,sysop:move=sysop'";
    let expected = PageRestrictionsOld(vec![
        ("edit", vec!["autoconfirmed", "sysop"]),
        ("move", vec!["sysop"]),
    ]);
    assert_eq!(PageRestrictionsOld::from_sql(raw), Ok(("", expected)));
}
*/

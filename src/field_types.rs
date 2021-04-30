/*!
Defines the types used in the [`schemas`](crate::schemas) module
and implements the [`FromSql`] trait for them.
Re-exports the [`Datelike`] and [`Timelike`] traits from the [`chrono`] crate,
which are used by [`Timestamp`].
 */
use nom::{branch::alt, bytes::streaming::tag, combinator::{map, map_res}, error::context};

use std::{
    collections::BTreeMap,
    convert::TryFrom,
    iter::FromIterator,
    ops::{Deref, Index},
    str::FromStr,
};

#[cfg(feature = "serialization")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(test)]
use bstr::B;

use crate::from_sql::FromSql;
use crate::from_sql::IResult;

/// The type used for float fields that are never NaN.
pub use ordered_float::NotNan;

/// The type that [`Timestamp`] derefs to, from `chrono`.
pub use chrono::NaiveDateTime;

/// Trait for [`Timestamp`], re-exported from `chrono`.
pub use chrono::{Datelike, Timelike};

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
            pub struct $wrapper<$l1>(pub &$l2 $wrapped_type);

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
                pub const fn into_inner(self) -> &$l2 $wrapped_type {
                    self.0
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
            pub struct $wrapper(pub $wrapped);

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
                    self.0
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
    ChangeTagDefinitionId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`rc_id`](https://www.mediawiki.org/wiki/Manual:Recentchanges_table#rc_id),
the primary key of the `recentchanges` table.
"]
    RecentChangeId: u32
}

impl_wrapper! {
    #[doc = "
Represents
[`el_id`](https://www.mediawiki.org/wiki/Manual:Externallinks_table#el_id),
the primary key of the `externallinks` table.
"]
    ExternalLinkId: u32
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
    PageRestrictionId: u32
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

/// A [timestamp](https://www.mediawiki.org/wiki/Manual:Timestamp),
/// represented as a string in the format `'yyyymmddhhmmss'` or `'yyyy-mm-dd hh:mm::ss'`.
/// Provides the methods of [`NaiveDateTime`] through [`Deref`].
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
pub struct Timestamp(pub NaiveDateTime);

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
            Expiry::Timestamp(t) => t.to_string(),
            Expiry::Infinity => "infinity".to_string(),
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
    pub BTreeMap<PageAction<'a>, Vec<ProtectionLevel<'a>>>,
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
    Css,
    SanitizedCss,
    JavaScript,
    Json,
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
            "css" => Css,
            "sanitized-css" => SanitizedCss,
            "javascript" => JavaScript,
            "json" => Json,
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
            Css => "css",
            SanitizedCss => "sanitized-css",
            JavaScript => "javascript",
            Json => "json",
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
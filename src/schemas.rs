/*!
Types that represent rows in tables of the
[MediaWiki database](https://www.mediawiki.org/wiki/Manual:Database_layout).

Implements the [`FromSqlTuple`] trait for them,
so that they can be parsed from SQL tuples by [`iterate_sql_insertions`](crate::iterate_sql_insertions).
*/

use nom::{
    character::streaming::char,
    combinator::{cut, map, opt},
    error::context,
    sequence::{preceded, terminated, tuple},
};

use crate::{
    from_sql::{FromSql, IResult},
    field_types::{
        ActorId, CategoryId, ChangeTagDefinitionId, ChangeTagId, CommentId, ContentModel, Expiry,
        ExternalLinkId, FullPageTitle, LinkTargetId, LogId, MajorMime, MediaType, MinorMime,
        NotNan, PageAction, PageCount, PageId, PageNamespace, PageRestrictionId,
        PageRestrictionsOld, PageTitle, PageType, ProtectionLevel, RecentChangeId, RevisionId,
        Sha1, Timestamp, UserGroup, UserId,
    },
    FromSqlTuple,
};

#[cfg(feature = "serialization")]
use serde::{Serialize, Deserialize};

macro_rules! mediawiki_link {
    (
        $text:expr,
        $page:expr $(,)?
    ) => {
        concat! (
            "[", $text, "](https://www.mediawiki.org/wiki/", $page, ")"
        )
    }
}

macro_rules! with_doc_comment {
    (
        $comment:expr,
        $($item:item)+
    ) => {
        #[doc = $comment]
        $($item)+
    }
}

macro_rules! database_table_doc {
    (
        $table_name:ident
    ) => {
        concat! (
            "Represents a row in the ",
            mediawiki_link!(
                concat!("`", stringify!($table_name), "` table"),
                concat!("Manual:", stringify!($table_name), "_table"),
            ),
            ".",
        )
    };
    (
        $table_name:ident, $page_name:literal
    ) => {
        concat!(
            "Represents a row in the ",
            mediawiki_link!(
                concat!("`", stringify!($table_name), "` table"),
                $page_name,
            ),
            ".",
        )
    };
}

macro_rules! impl_row_from_sql {
    (
        $table_name:ident $(: $page:literal)?
        $output_type:ident {
            $(
                $(#[$field_meta:meta])*
                $field_name:ident: $type_name:ty
            ),+
            $(,)?
        }
    ) => {
        with_doc_comment! {
            database_table_doc!($table_name $(, $page)?),
            #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
            #[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
            pub struct $output_type {
                $(
                    $(#[$field_meta])*
                    pub $field_name: $type_name,
                )+
            }

            impl<'input> FromSqlTuple<'input> for $output_type {
                fn from_sql_tuple(s: &'input [u8]) -> IResult<'input, Self> {
                    let fields = cut(
                        map(
                            tuple((
                                $(
                                    terminated(
                                        context(
                                            concat!(
                                                "the field “",
                                                stringify!($field_name),
                                                "”"
                                            ),
                                            <$type_name>::from_sql,
                                        ),
                                        opt(char(','))
                                    ),
                                )+
                            )),
                            |($($field_name),+)| $output_type {
                                $($field_name,)+
                            }
                        )
                    );
                    context(
                        concat!("row of ", stringify!($table_name), " table"),
                            preceded(
                            char('('),
                            terminated(
                                fields,
                                char(')')
                            )
                        )
                    )(s)
                }
            }
        }
    };
    (
        $table_name:ident $(: $page:literal)?
        $output_type:ident<$life:lifetime> {
            $(
                $(#[$field_meta:meta])*
                $field_name:ident: $type_name:ty,
            )+
        }
    ) => {
        with_doc_comment! {
            database_table_doc!($table_name $(, $page)?),
            #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
            #[cfg_attr(feature = "serialization", derive(Serialize, Deserialize))]
            pub struct $output_type<$life> {
                $(
                    $(#[$field_meta])*
                    pub $field_name: $type_name
                ),+
            }

            impl<$life> FromSqlTuple<$life> for $output_type<$life> {
                fn from_sql_tuple(s: &$life [u8]) -> IResult<$life, Self> {
                    let fields = cut(
                        map(
                            tuple((
                                $(
                                    terminated(
                                        context(
                                            concat!(
                                                "the field “",
                                                stringify!($field_name),
                                                "”"
                                            ),
                                            <$type_name>::from_sql,
                                        ),
                                        opt(char(','))
                                    ),
                                )+
                            )),
                            |($($field_name),+)| $output_type {
                                $($field_name,)+
                            }
                        ),
                    );
                    context(
                        concat!("row in ", stringify!($table_name), " table"),
                        preceded(
                            char('('),
                            terminated(
                                fields,
                                char(')')
                            )
                        )
                    )(s)
                }
            }
        }
    };
}

impl_row_from_sql! {
    babel: "Extension:Babel/babel_table"
    Babel<'input> {
        user: UserId,
        lang: &'input str,
        level: &'input str,
    }
}

impl_row_from_sql! {
    category
    Category {
        id: CategoryId,
        title: PageTitle,
        pages: PageCount,
        subcats: PageCount,
        files: PageCount,
    }
}

impl_row_from_sql! {
    categorylinks
    CategoryLink {
        from: PageId,
        to: PageTitle,
        /// Can be truncated in the middle of a UTF-8 sequence,
        /// so cannot be represented as a `String`.
        sortkey: Vec<u8>,
        timestamp: Timestamp,
        /// Values added after
        /// [this change](https://gerrit.wikimedia.org/r/449280),
        /// should be valid UTF-8, but older values may be invalid if they have
        /// been truncated in the middle of a multi-byte sequence.
        sortkey_prefix: Vec<u8>,
        collation: String,
        r#type: PageType,
    }
}

impl_row_from_sql! {
    change_tag
    ChangeTag {
        id: ChangeTagId,
        recent_changes_id: Option<RecentChangeId>,
        log_id: Option<LogId>,
        revision_id: Option<RevisionId>,
        params: Option<String>,
        tag_id: ChangeTagDefinitionId,
    }
}

impl_row_from_sql! {
    change_tag_def
    ChangeTagDefinition {
        id: ChangeTagDefinitionId,
        name: String,
        user_defined: bool,
        count: u64,
    }
}

impl_row_from_sql! {
    externallinks
    ExternalLink {
        id: ExternalLinkId,
        from: PageId,
        to: String,
        index: Vec<u8>,
        index_60: Vec<u8>,
    }
}

impl_row_from_sql! {
    image
    Image<'input> {
        name: PageTitle,
        size: u32,
        width: i32,
        height: i32,
        metadata: String,
        bits: i32,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        media_type: MediaType<'input>,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        major_mime: MajorMime<'input>,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        minor_mime: MinorMime<'input>,
        description_id: CommentId,
        actor: ActorId,
        timestamp: Timestamp,
        sha1: Sha1<'input>,
    }
}

impl_row_from_sql! {
    imagelinks
    ImageLink {
        from: PageId,
        to: PageTitle,
        from_namespace: PageNamespace,
    }
}

impl_row_from_sql! {
    iwlinks
    InterwikiLink<'input> {
        from: PageId,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        prefix: &'input str,
        title: PageTitle,
    }
}

impl_row_from_sql! {
    langlinks
    LanguageLink<'input> {
        from: PageId,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        lang: &'input str,
        title: FullPageTitle,
    }
}

impl_row_from_sql! {
    page_restrictions
    PageRestriction<'input> {
        id: PageRestrictionId,
        page: PageId,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        r#type: PageAction<'input>,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        level: ProtectionLevel<'input>,
        cascade: bool,
        user: Option<u32>,
        expiry: Option<Expiry>,
    }
}

impl_row_from_sql! {
    page
    Page<'input> {
        id: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        restrictions: Option<PageRestrictionsOld<'input>>,
        is_redirect: bool,
        is_new: bool,
        #[cfg_attr(feature = "serialization", serde(serialize_with = "crate::field_types::serialize_not_nan", deserialize_with = "crate::field_types::deserialize_not_nan"))]
        random: NotNan<f64>,
        touched: Timestamp,
        links_updated: Option<Timestamp>,
        latest: u32,
        len: u32,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        content_model: Option<ContentModel<'input>>,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        lang: Option<&'input str>,
    }
}

impl_row_from_sql! {
    pagelinks
    PageLink {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        from_namespace: PageNamespace,
    }
}

impl_row_from_sql! {
    page_props
    PageProperty<'input> {
        page: PageId,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        name: &'input str,
        value: Vec<u8>,
        #[cfg_attr(feature = "serialization", serde(serialize_with = "crate::field_types::serialize_option_not_nan", deserialize_with = "crate::field_types::deserialize_option_not_nan"))]
        sortkey: Option<NotNan<f64>>,
    }
}

impl_row_from_sql! {
    protected_titles
    ProtectedTitle<'input> {
        namespace: PageNamespace,
        title: PageTitle,
        user: UserId,
        reason_id: CommentId,
        timestamp: Timestamp,
        expiry: Expiry,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        create_perm: ProtectionLevel<'input>,
    }
}

impl_row_from_sql! {
    redirect
    Redirect<'input> {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        interwiki: Option<&'input str>,
        fragment: Option<String>,
    }
}

impl_row_from_sql! {
    sites
    Site<'input> {
        id: u32,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        global_key: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        r#type: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        group: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        source: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        language: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        protocol: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        domain: &'input [u8],
        data: String,
        forward: i8,
        config: String,
    }
}

impl_row_from_sql! {
    site_stats
    SiteStats {
        row_id: u32,
        total_edits: u64,
        good_articles: u64,
        total_pages: u64,
        users: u64,
        images: u64,
        active_users: u64,
    }
}

impl_row_from_sql! {
    wbc_entity_usage: "Wikibase/Schema/wbc_entity_usage"
    WikibaseClientEntityUsage<'input> {
        row_id: u64,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        entity_id: &'input str,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        aspect: &'input str,
        page_id: PageId,
    }
}

#[test]
fn test_redirect() {
    use bstr::B;
    let tuple = r"(605368,1,'разблюто','','Discussion from Stephen G. Brown\'s talk-page')";
    let redirect = Redirect::from_sql_tuple(tuple.as_bytes());
    assert_eq!(
        &redirect,
        &Ok((
            B(""),
            Redirect {
                from: PageId(605368),
                namespace: PageNamespace(1),
                title: PageTitle("разблюто".to_string()),
                interwiki: Some(""),
                fragment: Some(
                    "Discussion from Stephen G. Brown's talk-page".to_string()
                ),
            }
        ))
    );
    #[cfg(feature = "serialization")]
    assert_eq!(
        serde_json::to_string(&redirect.unwrap().1).unwrap(),
        r#"{"from":605368,"namespace":1,"title":"разблюто","interwiki":"","fragment":"Discussion from Stephen G. Brown's talk-page"}"#,
    )
}

impl_row_from_sql! {
    templatelinks
    TemplateLink {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        from_namespace: PageNamespace,
        target_id: LinkTargetId,
    }
}

impl_row_from_sql! {
    user_former_groups
    UserFormerGroupMembership<'input> {
        user: UserId,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        group: UserGroup<'input>,
    }
}

impl_row_from_sql! {
    user_groups
    UserGroupMembership<'input> {
        user: UserId,
        #[cfg_attr(feature = "serialization", serde(borrow))]
        group: UserGroup<'input>,
        expiry: Option<Expiry>,
    }
}

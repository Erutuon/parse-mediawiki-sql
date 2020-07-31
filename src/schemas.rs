/*!
Defines types that represent rows in tables of the
[MediaWiki database](https://www.mediawiki.org/wiki/Manual:Database_layout).
*/

use nom::{
    character::streaming::char,
    combinator::{cut, map, opt},
    sequence::{preceded, terminated, tuple},
};

use ordered_float::NotNan;

use crate::types::{
    ActorId, CategoryId, ChangeTagDefId, ChangeTagId, CommentId, ContentModel,
    Expiry, ExternalLinksId, FromSql, FullPageTitle, LogId, MajorMime,
    MediaType, MinorMime, PageAction, PageId, PageNamespace,
    PageRestrictionsId, PageRestrictionsOld, PageTitle, PageType,
    ProtectionLevel, RecentChangesId, RevisionId, Sha1, Timestamp, UserGroup,
    UserId, IResult,
};

macro_rules! impl_from_sql {
    (
        $table_name:ident
        $output_type:ident {
            $(
                $(#[$field_meta:meta])*
                $field_names:ident: $type_names:ty
            ),+
            $(,)?
        }
    ) => {
        impl_from_sql! {
            @database_table_doc($table_name)
            #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
            pub struct $output_type {
                $(
                    $(#[$field_meta])*
                    pub $field_names: $type_names,
                )+
            }

            impl<'input> FromSql<'input> for $output_type {
                fn from_sql(s: &'input [u8]) -> IResult<'input, Self> {
                    let fields = cut(
                        map(
                            tuple((
                                $(
                                    terminated(
                                        <$type_names>::from_sql,
                                        opt(char(','))
                                    ),
                                )+
                            )),
                            |($($field_names),+)| $output_type {
                                $($field_names,)+
                            }
                        )
                    );
                    preceded(
                        char('('),
                        terminated(
                            fields,
                            char(')')
                        )
                    )(s)
                }
            }
        }
    };
    (
        $table_name:ident
        $output_type:ident<$life:lifetime> {
            $(
                $(#[$field_meta:meta])*
                $field_names:ident: $type_names:ty,
            )+
        }
    ) => {
        impl_from_sql! {
            @database_table_doc($table_name)
            #[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
            pub struct $output_type<$life> {
                $($(#[$field_meta])* pub $field_names: $type_names),+
            }

            impl<$life> FromSql<$life> for $output_type<$life> {
                fn from_sql(s: &$life [u8]) -> IResult<$life, Self> {
                    let fields = cut(
                        map(
                            tuple((
                                $(
                                    terminated(
                                        <$type_names>::from_sql,
                                        opt(char(','))
                                    ),
                                )+
                            )),
                            |($($field_names),+)| $output_type {
                                $($field_names,)+
                            }
                        ),
                    );
                    preceded(
                        char('('),
                        terminated(
                            fields,
                            char(')')
                        )
                    )(s)
                }
            }
        }
    };
    (
        @database_table_doc($table_name:ident)
        $($items:item)+
    ) => {
        impl_from_sql! {
            @with_doc_comment
            #[doc = concat!(
                "Represents the [`",
                stringify!($table_name),
                "` table](https://www.mediawiki.org/wiki/Manual:",
                stringify!($table_name),
                "_table).",
            )]
            $($items)+
        }
    };
    (@with_doc_comment #[doc = $comment:expr] $($metas:meta)* $($items:item)+) => {
        #[doc = $comment]
        $($items)+
    };
}

impl_from_sql! {
    category
    Category {
        id: CategoryId,
        title: PageTitle,
        pages: u32,
        subcats: u32,
        files: u32,
    }
}

impl_from_sql! {
    categorylinks
    CategoryLinks<'input> {
        id: PageId,
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
        r#type: PageType<'input>,
    }
}

impl_from_sql! {
    change_tag_def
    ChangeTagDef {
        id: ChangeTagDefId,
        name: String,
        user_defined: bool,
        count: u32,
    }
}

impl_from_sql! {
    change_tag
    ChangeTag {
        id: ChangeTagId,
        recent_change_id: Option<RecentChangesId>,
        log_id: Option<LogId>,
        revision_id: Option<RevisionId>,
        params: Option<String>,
        tag_id: Option<ChangeTagDefId>,
    }
}

impl_from_sql! {
    externallinks
    ExternalLinks {
        id: ExternalLinksId,
        from: PageId,
        to: String,
        index: String,
        index_60: String,
    }
}

impl_from_sql! {
    image
    Image<'input> {
        name: PageTitle,
        size: u32,
        width: i32,
        height: i32,
        metadata: String,
        bits: i32,
        media_type: MediaType<'input>,
        major_mime: MajorMime<'input>,
        minor_mime: MinorMime<'input>,
        description_id: CommentId,
        actor: ActorId,
        timestamp: Timestamp,
        sha1: Sha1<'input>,
        /// Not mentioned in the
        /// [MediaWiki documentation](https://www.mediawiki.org/wiki/Manual:Image_table),
        /// but present in the dump of the English Wiktionary.
        deleted: i8,
    }
}

impl_from_sql! {
    imagelinks
    ImageLinks {
        from: PageId,
        namespace: PageNamespace,
        to: PageTitle,
    }
}

impl_from_sql! {
    iwlinks
    InterwikiLinks<'input> {
        from: PageId,
        prefix: &'input str,
        title: PageTitle,
    }
}

impl_from_sql! {
    langlinks
    LangLinks<'input> {
        from: PageId,
        lang: &'input str,
        title: FullPageTitle,
    }
}

impl_from_sql! {
    page_restrictions
    PageRestrictions<'input> {
        page: PageId,
        r#type: PageAction<'input>,
        level: ProtectionLevel<'input>,
        cascade: bool,
        user: Option<u32>,
        expiry: Option<Expiry>,
        id: PageRestrictionsId,
    }
}

impl_from_sql! {
    page
    Page<'input> {
        id: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        restrictions: PageRestrictionsOld<'input>,
        is_redirect: bool,
        is_new: bool,
        random: NotNan<f64>,
        touched: Timestamp,
        links_updated: Option<Timestamp>,
        latest: u32,
        len: u32,
        content_model: Option<ContentModel<'input>>,
        lang: Option<&'input str>,
    }
}

impl_from_sql! {
    pagelinks
    PageLinks {
        from: PageId,
        from_namespace: PageNamespace,
        namespace: PageNamespace,
        title: PageTitle,
    }
}

impl_from_sql! {
    protected_titles
    ProtectedTitles<'input> {
        namespace: PageNamespace,
        title: PageTitle,
        user: UserId,
        reason_id: CommentId,
        timestamp: Timestamp,
        expiry: Expiry,
        create_perm: ProtectionLevel<'input>,
    }
}

impl_from_sql! {
    redirect
    Redirect<'input> {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        interwiki: Option<&'input str>,
        fragment: Option<String>,
    }
}

#[test]
fn test_redirect() {
    use bstr::B;
    let item = r"(605368,1,'разблюто','','Discussion from Stephen G. Brown\'s talk-page')";
    assert_eq!(
        Redirect::from_sql(item.as_bytes()),
        Ok((
            B(""),
            Redirect {
                from: PageId::from(605368),
                namespace: PageNamespace::from(1),
                title: PageTitle::from("разблюто".to_string()),
                interwiki: Some(""),
                fragment: Some(
                    "Discussion from Stephen G. Brown's talk-page".to_string()
                ),
            }
        ))
    );
}

impl_from_sql! {
    templatelinks
    TemplateLinks {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle,
        from_namespace: PageNamespace,
    }
}

impl_from_sql! {
    user_former_groups
    UserFormerGroups<'input> {
        user: UserId,
        group: UserGroup<'input>,
    }
}

impl_from_sql! {
    user_groups
    UserGroups<'input> {
        user: UserId,
        group: UserGroup<'input>,
        expiry: Option<Expiry>,
    }
}

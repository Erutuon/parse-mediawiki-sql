/*!
Defines types that represent rows in tables of the
[MediaWiki database](https://www.mediawiki.org/wiki/Manual:Database_layout).
*/

use nom::{
    character::complete::char,
    combinator::{map, opt},
    sequence::{preceded, terminated, tuple},
    IResult,
};
use std::borrow::Cow;

use crate::types::{
    ActorId, CategoryId, ChangeTagDefId, ChangeTagId, CommentId, ContentModel,
    Expiry, ExternalLinksId, FromSQL, FullPageTitle, LogId, MajorMime,
    MediaType, MinorMime, PageAction, PageId, PageNamespace,
    PageRestrictionsId, PageRestrictionsOld, PageTitle, PageType,
    ProtectionLevel, RecentChangesId, RevisionId, Sha1, Timestamp, UserGroup,
    UserId,
};

macro_rules! impl_from_sql {
    (
        $(#[$top_meta:meta])*
        $output_type:ident<$life:lifetime> {
            $($(#[$field_meta:meta])* $field_names:ident: $type_names:ty,)+
        }
    ) => {
        $(#[$top_meta])*
        #[derive(Debug, Clone)]
        pub struct $output_type<$life> {
            $($(#[$field_meta])* pub $field_names: $type_names),+
        }

        impl<$life> FromSQL<$life> for $output_type<$life> {
            fn from_sql(s: &$life str) -> IResult<&$life str, Self> {
                preceded(
                    char('('),
                    terminated(
                        map(
                            tuple(( $( terminated(<$type_names>::from_sql, opt(char(','))) ),+ )),
                            |($($field_names),+)| $output_type { $($field_names),+ }),
                        char(')')))(s)
            }
        }
    };
}

impl_from_sql! {
    /// Represents the [`category` table](https://www.mediawiki.org/wiki/Manual:category_table).
    Category<'input> {
        id: CategoryId,
        title: PageTitle<'input>,
        pages: u32,
        subcats: u32,
        files: u32,
    }
}

impl_from_sql! {
    /// Represents the [`categorylinks` table](https://www.mediawiki.org/wiki/Manual:categorylinks_table).
    CategoryLinks<'input> {
        id: PageId,
        to: PageTitle<'input>,
        sortkey: Cow<'input, str>,
        timestamp: Timestamp,
        sortkey_prefix: Cow<'input, str>,
        collation: Cow<'input, str>,
        r#type: PageType<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`change_tag_def` table](https://www.mediawiki.org/wiki/Manual:change_tag_def_table).
    ChangeTagDef<'input> {
        id: ChangeTagDefId,
        name: Cow<'input, str>,
        user_defined: bool,
        count: u32,
    }
}

impl_from_sql! {
    /// Represents the [`change_tag` table](https://www.mediawiki.org/wiki/Manual:change_tag_table).
    ChangeTag<'input> {
        id: ChangeTagId,
        recent_change_id: Option<RecentChangesId>,
        log_id: Option<LogId>,
        revision_id: Option<RevisionId>,
        params: Option<Cow<'input, str>>,
        tag_id: Option<ChangeTagDefId>,
    }
}

impl_from_sql! {
    /// Represents the [`externallinks` table](https://www.mediawiki.org/wiki/Manual:externallinks_table).
    ExternalLinks<'input> {
        id: ExternalLinksId,
        from: PageId,
        to: Cow<'input, str>,
        index: Cow<'input, str>,
        index_60: Cow<'input, str>,
    }
}

impl_from_sql! {
    /// Represents the [`image` table](https://www.mediawiki.org/wiki/Manual:image_table).
    Image<'input> {
        name: PageTitle<'input>,
        size: u32,
        width: i32,
        height: i32,
        metadata: Cow<'input, str>,
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
    /// Represents the [`imagelinks` table](https://www.mediawiki.org/wiki/Manual:imagelinks_table).
    ImageLinks<'input> {
        from: PageId,
        namespace: PageNamespace,
        to: PageTitle<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`iwlinks` table](https://www.mediawiki.org/wiki/Manual:iwlinks_table).
    InterwikiLinks<'input> {
        from: PageId,
        prefix: &'input str,
        title: PageTitle<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`langlinks` table](https://www.mediawiki.org/wiki/Manual:langlinks_table).
    LangLinks<'input> {
        from: PageId,
        lang: &'input str,
        title: FullPageTitle<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`page_restrictions` table](https://www.mediawiki.org/wiki/Manual:page_restrictions_table).
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
    /// Represents the [`page` table](https://www.mediawiki.org/wiki/Manual:page_table).
    Page<'input> {
        id: PageId,
        namespace: PageNamespace,
        title: PageTitle<'input>,
        restrictions: PageRestrictionsOld<'input>,
        is_redirect: bool,
        is_new: bool,
        random: f64,
        touched: Timestamp,
        links_updated: Option<Timestamp>,
        latest: u32,
        len: u32,
        content_model: Option<ContentModel<'input>>,
        lang: Option<&'input str>,
    }
}

impl_from_sql! {
    /// Represents the [`pagelinks` table](https://www.mediawiki.org/wiki/Manual:pagelinks_table).
    PageLinks<'input> {
        from: PageId,
        from_namespace: PageNamespace,
        namespace: PageNamespace,
        title: PageTitle<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`protected_titles` table](https://www.mediawiki.org/wiki/Manual:protected_titles_table).
    ProtectedTitles<'input> {
        namespace: PageNamespace,
        title: PageTitle<'input>,
        user: UserId,
        reason_id: CommentId,
        timestamp: Timestamp,
        expiry: Expiry,
        create_perm: ProtectionLevel<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`redirect` table](https://www.mediawiki.org/wiki/Manual:redirect_table).
    Redirect<'input> {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle<'input>,
        interwiki: Option<&'input str>,
        fragment: Option<&'input str>,
    }
}

impl_from_sql! {
    /// Represents the [`templatelinks` table](https://www.mediawiki.org/wiki/Manual:templatelinks_table).
    TemplateLinks<'input> {
        from: PageId,
        namespace: PageNamespace,
        title: PageTitle<'input>,
        from_namespace: PageNamespace,
    }
}

impl_from_sql! {
    /// Represents the [`user_former_groups` table](https://www.mediawiki.org/wiki/Manual:user_former_groups_table).
    UserFormerGroups<'input> {
        user: UserId,
        group: UserGroup<'input>,
    }
}

impl_from_sql! {
    /// Represents the [`user_groups` table](https://www.mediawiki.org/wiki/Manual:user_groups_table).
    UserGroups<'input> {
        user: UserId,
        group: UserGroup<'input>,
        expiry: Option<Expiry>,
    }
}

use bstr::{BStr, ByteSlice};
use joinery::prelude::*;
use nom::{
    branch::alt,
    bytes::streaming::tag,
    character::streaming::char,
    combinator::{opt, recognize},
    error::{ContextError, ErrorKind, FromExternalError, ParseError},
    multi::many1,
    sequence::{delimited, pair},
};

use std::fmt::Display;

use crate::from_sql::FromSql;

/// Used inside [`Error`] to store the names of the items that were being parsed.
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

/// Error type used by [`FromSql`].
///
/// Keeps a list of the items that were being parsed when an error was encountered.
/// The [`Display`] implementation prints a backtrace with a snippet of the text that failed to parse.
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

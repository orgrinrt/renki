//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The TOML store's writer: one key set in a document's text, everything else
//! kept byte for byte.
//!
//! A key the document holds has its value's span replaced, so a key inside an
//! inline table is edited in its braces, a multi-line value is replaced whole,
//! a line inside a multi-line string is never taken for a key, and a key
//! under a `[[name]]` header is edited in that element. A key it does not hold
//! is added where a reader would look for it: after the last pair of the
//! nearest table above it that has a header, dotted from there; under a new
//! header at the end where its table is not there at all; and a key of the
//! top level after the top level's last pair, or above the first header where
//! there is none, never inside the last table.

use core::fmt;

use notko::Maybe;
use renki_config::Rendered;
use tomlfuse_parse::{Entry, Holds};

use crate::toml::{Toml, TomlDocument, write_basic, write_segment};

fn value(v: Rendered<'_>, into: &mut impl fmt::Write) -> fmt::Result {
    match v {
        Rendered::Text(t) => write_basic(t, into),
        Rendered::Raw(t) => into.write_str(t),
    }
}

/// `path`, dotted, each segment bare where it can be.
fn dotted(path: &str, into: &mut impl fmt::Write) -> fmt::Result {
    for (n, seg) in path.split('.').enumerate() {
        if n > 0 {
            into.write_char('.')?;
        }
        write_segment(seg, into)?;
    }
    Ok(())
}

fn pair(rest: &str, v: Rendered<'_>, into: &mut impl fmt::Write) -> fmt::Result {
    dotted(rest, into)?;
    into.write_str(" = ")?;
    value(v, into)
}

/// The end of the line `at` stands on, before its newline.
fn line_end(text: &str, at: usize) -> usize {
    text[at ..].find('\n').map_or(text.len(), |n| at + n)
}

/// `pair` added on a line of its own after the line `at` stands on.
fn after_line(
    text: &str,
    at: usize,
    rest: &str,
    v: Rendered<'_>,
    into: &mut impl fmt::Write,
) -> fmt::Result {
    let end = line_end(text, at);
    into.write_str(&text[.. end])?;
    into.write_char('\n')?;
    pair(rest, v, into)?;
    if end == text.len() {
        into.write_char('\n')?;
    }
    into.write_str(&text[end ..])
}

/// Where the first header of the document starts, where it has one.
fn first_header(root: Entry<'_, '_>) -> Maybe<usize> {
    let mut first = Maybe::Isnt;
    for child in root.children() {
        let spans = match child.holds() {
            Holds::ArrayOfTables => child.children().next().and_then(|e| e.value_span()),
            Holds::Table => child.value_span(),
            _ => None,
        };
        if let Some(span) = spans {
            first = Maybe::Is(match first {
                Maybe::Is(f) if f < span.start => f,
                _ => span.start,
            });
        }
    }
    first
}

/// `doc`'s text with `key` set to `v`, written into `into`.
pub(crate) fn set(
    doc: &TomlDocument<'_>,
    key: &str,
    v: Rendered<'_>,
    into: &mut impl fmt::Write,
) -> fmt::Result {
    let parsed = Toml::document(doc);
    let text = parsed.text();
    if let Maybe::Is(e) = Toml::entry(doc, key)
        && let Some(span) = e.value_span()
        && matches!(e.holds(), Holds::Scalar | Holds::Array | Holds::InlineTable)
    {
        into.write_str(&text[.. span.start])?;
        value(v, into)?;
        return into.write_str(&text[span.end ..]);
    }
    // a key whose table is not there at all goes under a new header at the end
    if let Some(dot) = key.rfind('.')
        && Toml::entry(doc, &key[.. dot]).isnt()
    {
        into.write_str(text)?;
        if !text.is_empty() {
            if !text.ends_with('\n') {
                into.write_char('\n')?;
            }
            into.write_char('\n')?;
        }
        into.write_char('[')?;
        dotted(&key[.. dot], into)?;
        into.write_str("]\n")?;
        pair(&key[dot + 1 ..], v, into)?;
        return into.write_char('\n');
    }
    // else the nearest table above it with a header, or the top level
    let mut split = key.len();
    while let Some(dot) = key[.. split].rfind('.') {
        split = dot;
        if let Maybe::Is(t) = Toml::entry(doc, &key[.. dot])
            && t.holds() == Holds::Table
            && let Some(span) = t.value_span()
        {
            return after_line(text, span.end, &key[dot + 1 ..], v, into);
        }
    }
    match (parsed.root().value_span(), first_header(parsed.root())) {
        (Some(span), _) => after_line(text, span.end, key, v, into),
        (None, Maybe::Is(at)) => {
            into.write_str(&text[.. at])?;
            pair(key, v, into)?;
            into.write_str("\n\n")?;
            into.write_str(&text[at ..])
        },
        (None, Maybe::Isnt) => {
            into.write_str(text)?;
            if !text.is_empty() && !text.ends_with('\n') {
                into.write_char('\n')?;
            }
            pair(key, v, into)?;
            into.write_char('\n')
        },
    }
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The backend contract, and where a value came from.
//!
//! Every `u32` and `&str` here is marked for the port to arvo's types and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;

use notko::{Maybe, Outcome};

use crate::Literal;

/// Why text is not a document of some store.
///
/// The store's own message stays with the store, which is the side that has
/// it; the launcher prints that itself. What the contract carries is the line,
/// which is what a person opens the file at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadDocument {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a line number. FIXME: port to arvo.
    line: Maybe<u32>,
}

impl BadDocument {
    /// A refusal at `line`, or with no line known.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a line number. FIXME: port to arvo.
    pub const fn at(line: Maybe<u32>) -> Self {
        BadDocument {
            line,
        }
    }

    /// The line the store refused at, where it knows one.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a line number. FIXME: port to arvo.
    pub const fn line(&self) -> Maybe<u32> {
        self.line
    }
}

impl fmt::Display for BadDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Maybe::Is(line) => write!(f, "not a document, at line {line}"),
            Maybe::Isnt => f.write_str("not a document"),
        }
    }
}

/// A value on its way into a file: text that the store quotes, or a token it
/// writes as it is. A kind renders its value into one of these through the
/// row's renderer, and the store owns the quoting, since a string's quoting
/// is the store's syntax and not the kind's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rendered<'a> {
    /// Text the store quotes in its own way.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    Text(&'a str),
    /// A token written as it is: `true`, `12`, `["a", "b"]`.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    Raw(&'a str),
}

/// A file format a configuration is kept in.
///
/// Implemented by a marker type per format. It parses text into a document
/// that borrows from it, answers one dotted key as a [`Literal`], lists the
/// keys a document holds so a key the schema does not know is refused by
/// name, walks a list literal's items, and writes one key back into the text
/// it was given, keeping everything else in that text where it was, comments
/// included. Nothing else: the precedence, the kinds and the scopes are the
/// resolver's and are the same over every store.
pub trait Store: 'static {
    /// What the schema and the diagnostics call the format.
    // lint:allow(no-bare-static-str) reason: a format's static name. FIXME: port to Str.
    const NAME: &'static str;
    /// The file's extension, without the dot.
    // lint:allow(no-bare-static-str) reason: a format's static extension. FIXME: port to Str.
    const EXTENSION: &'static str;
    /// A parsed document, borrowing the text it was parsed from.
    type Document<'t>;
    /// The keys a document holds, dotted, in the document's order.
    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    type Keys<'d>: Iterator<Item = &'d str>;
    /// The items of a list literal, in order.
    type Items<'d>: Iterator<Item = Literal<'d>>;

    /// Parse, or say where the text stops being a document.
    // lint:allow(no-bare-string) reason: the caller's file text. FIXME: port to Str.
    fn parse<'t>(text: &'t str) -> Outcome<Self::Document<'t>, BadDocument>;
    /// The value under a dotted key, or `Isnt` where the document has none.
    // lint:allow(no-bare-string) reason: a dotted key. FIXME: port to Str.
    fn get<'d>(doc: &'d Self::Document<'_>, key: &str) -> Maybe<Literal<'d>>;
    /// Every dotted key the document holds a scalar or a list under.
    fn keys<'d>(doc: &'d Self::Document<'_>) -> Self::Keys<'d>;
    /// The items of a list literal this store produced.
    // lint:allow(no-bare-string) reason: the list's own text. FIXME: port to Str.
    fn items<'d>(list: &'d str) -> Self::Items<'d>;
    /// `text` with `key` set to `value`, written into `into`, everything else
    /// kept byte for byte. A key not yet in the text is added where a reader
    /// would look for it.
    // lint:allow(no-bare-string) reason: the caller's file text and a dotted key. FIXME: port to Str.
    fn set(text: &str, key: &str, value: Rendered<'_>, into: &mut impl fmt::Write) -> fmt::Result;
}

/// Where a resolved value came from, highest precedence first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// A flag on the command line.
    Flag,
    /// The tool's own environment variable for the key.
    Env,
    /// The repository's file.
    Repo,
    /// The person's file.
    User,
    /// The setting's declared default.
    Default,
}

impl Source {
    /// The word `config get` prints beside the value.
    // lint:allow(no-bare-string, no-bare-static-str) reason: a static name. FIXME: port to Str.
    pub const fn name(self) -> &'static str {
        match self {
            Source::Flag => "flag",
            Source::Env => "env",
            Source::Repo => "repo",
            Source::User => "user",
            Source::Default => "default",
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

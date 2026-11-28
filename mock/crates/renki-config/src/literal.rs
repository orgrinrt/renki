//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The seam between a store and a kind.
//!
//! Every `bool`, `i64` and `&str` here is marked for the port to arvo's types
//! and `hilavitkutin_str::Str`, which is a later unit; the markers sit above
//! the item they cover, which is where rustfmt leaves them alone.

use core::fmt;

use crate::Store;

/// A value in the shape every store speaks and every kind reads.
///
/// A store turns its own node into one of these, a kind turns one of these
/// into its value, and neither knows the other. Closed, because the four
/// shapes are what a configuration file carries; a table of tables is not a
/// setting. Everything borrows from the document, so nothing is allocated to
/// hand a value across.
pub enum Literal<'a, St: Store> {
    /// `true` or `false`.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own boolean. FIXME: port to arvo's Bool.
    Bool(bool),
    /// A whole number.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own integer. FIXME: port to arvo.
    Int(i64),
    /// Text, a path included, borrowed from the document already decoded.
    // lint:allow(no-bare-string) reason: borrowed from the caller's document text. FIXME: port to Str.
    Str(&'a str),
    /// A list, as the store's own handle on it. The store that produced it is
    /// the one that can walk it, through [`Store::items`], so a kind asks the
    /// store rather than knowing the syntax, and a list has no text of its own
    /// to be read again.
    List(St::List<'a>),
}

impl<St: Store> Clone for Literal<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Literal<'_, St> {}

impl<St: Store> PartialEq for Literal<'_, St> {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: `PartialEq::eq` is core's signature.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Literal::Bool(a), Literal::Bool(b)) => a == b,
            (Literal::Int(a), Literal::Int(b)) => a == b,
            (Literal::Str(a), Literal::Str(b)) => a == b,
            (Literal::List(a), Literal::List(b)) => a == b,
            _ => false,
        }
    }
}

impl<St: Store> Eq for Literal<'_, St> {}

impl<St: Store> fmt::Debug for Literal<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal::Bool(b) => f.debug_tuple("Bool").field(b).finish(),
            Literal::Int(i) => f.debug_tuple("Int").field(i).finish(),
            Literal::Str(s) => f.debug_tuple("Str").field(s).finish(),
            Literal::List(l) => f.debug_tuple("List").field(l).finish(),
        }
    }
}

/// What was offered where a kind wanted something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Got<'a> {
    /// A boolean literal.
    Bool,
    /// An integer literal.
    Int,
    /// A list literal.
    List,
    /// Text, from a flag, a variable, or a string literal.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    Text(&'a str),
}

impl fmt::Display for Got<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Got::Bool => f.write_str("a boolean"),
            Got::Int => f.write_str("an integer"),
            Got::List => f.write_str("a list"),
            Got::Text(t) => write!(f, "{t:?}"),
        }
    }
}

/// Why text or a literal is not a value of some kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadValue<'a> {
    // lint:allow(no-bare-string, no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    kind: &'static str,
    got:  Got<'a>,
}

impl<'a> BadValue<'a> {
    /// The refusal of `got` by the kind named `kind`.
    // lint:allow(no-bare-string, no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    pub const fn new(kind: &'static str, got: Got<'a>) -> Self {
        BadValue {
            kind,
            got,
        }
    }

    /// The kind that refused, by its schema name.
    // lint:allow(no-bare-string, no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// What was offered.
    pub const fn got(&self) -> Got<'a> {
        self.got
    }
}

impl fmt::Display for BadValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} is not {}", self.got, self.kind)
    }
}

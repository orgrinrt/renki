//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The merge rule, held once for every reader that merges layers.
//!
//! Every `bool` and `&str` here is marked for the port to arvo's `Bool` and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;

use notko::Outcome;

/// What a document holds at a key. An array of tables is a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    /// A table, whose keys merge one by one.
    Table,
    /// A list, an array of tables included.
    List,
    /// A boolean, a number or text.
    Scalar,
}

impl Shape {
    /// The word a refusal names the shape by.
    // lint:allow(no-bare-static-str) reason: a static name. FIXME: port to Str.
    pub const fn name(self) -> &'static str {
        match self {
            Shape::Table => "a table",
            Shape::List => "a list",
            Shape::Scalar => "a scalar",
        }
    }
}

/// How a higher layer's value at a key meets a lower layer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meet {
    /// Two tables, merged key by key.
    Deep,
    /// Two lists, the higher's items after the lower's.
    Append,
    /// The higher's value in place of the lower's.
    Over,
}

/// Why layers do not merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadMerge<'a> {
    /// One shape over another that nothing replaces, the lower first.
    Shapes(Shape, Shape),
    /// A key `meta.unset` names that no lower layer holds.
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    Unset(&'a str),
}

impl fmt::Display for BadMerge<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadMerge::Shapes(lower, higher) => {
                write!(
                    f,
                    "{} over {} does not merge; name the key in meta.replace to replace it",
                    higher.name(),
                    lower.name()
                )
            },
            BadMerge::Unset(key) => write!(f, "meta.unset names {key}, which no lower layer holds"),
        }
    }
}

/// How the `higher` shape at a key meets the `lower`, where `replaced` says
/// whether the higher layer's `meta.replace` names the key: two tables go
/// deep, two lists append, two scalars and anything replaced go over, and
/// any other pair is refused naming both.
// lint:allow(no-bare-numeric, arvo-types-only) reason: whether `meta.replace` names the key. FIXME: port to arvo's Bool.
pub const fn meet(lower: Shape, higher: Shape, replaced: bool) -> Outcome<Meet, BadMerge<'static>> {
    if replaced {
        return Outcome::Ok(Meet::Over);
    }
    match (lower, higher) {
        (Shape::Table, Shape::Table) => Outcome::Ok(Meet::Deep),
        (Shape::List, Shape::List) => Outcome::Ok(Meet::Append),
        (Shape::Scalar, Shape::Scalar) => Outcome::Ok(Meet::Over),
        _ => Outcome::Err(BadMerge::Shapes(lower, higher)),
    }
}

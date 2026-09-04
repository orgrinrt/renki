//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where a setting may be written: the person's file, the repository's, or
//! either. A type, so a repository file carrying a person's setting is refused
//! by a table the declaration filled in rather than by a check somebody
//! remembered to write.
//!
//! Every `bool` and `&'static str` here is marked for the port to arvo's
//! `Bool` and `hilavitkutin_str::Str`, which is a later unit; the markers sit
//! above the item they cover, which is where rustfmt leaves them alone.

mod sealed {
    pub trait Sealed {}
}

/// A scope, sealed to the three that exist.
pub trait Scope: sealed::Sealed + Copy + Default + 'static {
    /// What the schema calls it.
    // lint:allow(no-bare-static-str) reason: a scope's static name. FIXME: port to Str.
    const NAME: &'static str;
    /// Read from the person's file.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const USER: bool;
    /// Read from the repository's file.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const REPO: bool;
}

/// The person's file only. A theme, a model, a root the machine refuses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct User;
/// The repository's file only. Something every clone of it should agree on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Repo;
/// Either, the repository's winning over the person's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Either;

impl sealed::Sealed for User {}
impl sealed::Sealed for Repo {}
impl sealed::Sealed for Either {}

/// One scope's table row, written out once. A macro rather than three impls
/// typed by hand, so the three cannot drift in shape.
macro_rules! scope {
    ($name:ident, $label:literal, user: $user:literal, repo: $repo:literal) => {
        // lint:allow(no-manual-impl) reason: the scope's own table macro; the stack's define macros describe registrable types, which a sealed marker is not.
        impl Scope for $name {
            // lint:allow(no-bare-static-str) reason: a scope's static name. FIXME: port to Str.
            const NAME: &'static str = $label;
            // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
            const REPO: bool = $repo;
            // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
            const USER: bool = $user;
        }
    };
}

scope!(User, "user", user: true, repo: false);
scope!(Repo, "repo", user: false, repo: true);
scope!(Either, "either", user: true, repo: true);

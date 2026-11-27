//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! One setting, typed, and the erased row a tool's table carries.
//!
//! Every `bool` and `&str` here is marked for the port to arvo's `Bool` and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;
use core::marker::PhantomData;

use notko::Outcome;

use crate::{BadValue, Declared, Kind, ListRun, Literal, Scope, Store};

/// One setting, typed by its kind and its scope.
///
/// The default is text in the kind's own `from_text` form, checked when the
/// table is; a typed default would need a const constructor per kind and a
/// borrowed value has none.
#[derive(Debug, Clone, Copy)]
pub struct Setting<K: Kind, S: Scope> {
    // lint:allow(no-bare-static-str) reason: a setting's static key. FIXME: port to Str.
    key:     &'static str,
    // lint:allow(no-bare-static-str) reason: a setting's static default. FIXME: port to Str.
    default: &'static str,
    // lint:allow(no-bare-static-str) reason: a setting's static doc. FIXME: port to Str.
    doc:     &'static str,
    _kind:   PhantomData<K>,
    _scope:  PhantomData<S>,
}

impl<K: Kind, S: Scope> Setting<K, S> {
    /// Declare one. `key` is dotted, `theme` or `model.base`: dots are
    /// sections in the file and underscores in the environment. `default` is
    /// in the kind's text form. `doc` is one sentence, printed by
    /// `config schema`.
    // lint:allow(no-bare-static-str) reason: the setting's static key, default and doc. FIXME: port to Str.
    pub const fn new(key: &'static str, default: &'static str, doc: &'static str) -> Self {
        Setting {
            key,
            default,
            doc,
            _kind: PhantomData,
            _scope: PhantomData,
        }
    }

    /// The table row for a store: the kind and the scope folded into names
    /// and function pointers, so a tool holds settings of every kind in one
    /// slice. Built here or by [`Declared::named`] and nowhere else, which is
    /// what keeps the pointers and the names in agreement.
    pub const fn row<St: Store>(self) -> Declared<'static, St> {
        Declared::of::<K, S>(self.key, self.default, self.doc)
    }
}

// lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
pub(crate) fn check_text<'a, K: Kind, S: Store>(text: &'a str) -> Outcome<(), BadValue<'a>> {
    K::from_text::<S>(text).map(|_| ())
}

pub(crate) fn check_literal<'a, K: Kind, S: Store>(lit: Literal<'a>) -> Outcome<(), BadValue<'a>> {
    K::from_literal::<S>(lit).map(|_| ())
}

pub(crate) fn check_run<'a, K: Kind, S: Store>(run: ListRun<'a, S>) -> Outcome<(), BadValue<'a>> {
    K::from_run::<S>(run).map(|_| ())
}

// lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
pub(crate) fn render_text<K: Kind, S: Store>(
    text: &str,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match K::from_text::<S>(text) {
        Outcome::Ok(v) => K::write::<S>(v, f),
        // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
        Outcome::Err(_) => Err(fmt::Error),
    }
}

pub(crate) fn render_literal<K: Kind, S: Store>(
    lit: Literal<'_>,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match K::from_literal::<S>(lit) {
        Outcome::Ok(v) => K::write::<S>(v, f),
        // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
        Outcome::Err(_) => Err(fmt::Error),
    }
}

pub(crate) fn render_run<K: Kind, S: Store>(
    run: ListRun<'_, S>,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match K::from_run::<S>(run) {
        Outcome::Ok(v) => K::write::<S>(v, f),
        // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
        Outcome::Err(_) => Err(fmt::Error),
    }
}

/// `a`, `a_b`, `a.b_c`: identifiers joined by single dots, each starting with
/// a letter or an underscore. Becomes `A_B_C` in the environment.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: the caller's text and a predicate over it. FIXME: port to Str and arvo's Bool.
pub const fn key_is_wellformed(key: &str) -> bool {
    let b = key.as_bytes();
    if b.is_empty() {
        return false;
    }
    let mut i = 0;
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a scanner's own flag. FIXME: port to arvo's Bool.
    let mut at_start = true;
    while i < b.len() {
        let c = b[i];
        if at_start {
            if !(c.is_ascii_alphabetic() || c == b'_') {
                return false;
            }
            at_start = false;
        } else if c == b'.' {
            at_start = true;
        } else if !(c.is_ascii_alphanumeric() || c == b'_') {
            return false;
        }
        i += 1;
    }
    !at_start
}

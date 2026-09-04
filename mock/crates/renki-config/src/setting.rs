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

use notko::{Maybe, Outcome};

use crate::{BadValue, Kind, Literal, Scope, Store};

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
    /// slice. Built here and nowhere else, which is what keeps the pointers
    /// and the names in agreement.
    pub const fn row<St: Store>(self) -> Declared<St> {
        Declared {
            key:            self.key,
            kind:           K::NAME,
            scope:          S::NAME,
            reads_user:     S::USER,
            reads_repo:     S::REPO,
            quoted:         K::QUOTED,
            default:        self.default,
            doc:            self.doc,
            check_text:     check_text::<K, St>,
            check_literal:  check_literal::<K, St>,
            render_text:    render_text::<K, St>,
            render_literal: render_literal::<K, St>,
            _store:         PhantomData,
        }
    }
}

// lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
fn check_text<'a, K: Kind, S: Store>(text: &'a str) -> Outcome<(), BadValue<'a>> {
    K::from_text::<S>(text).map(|_| ())
}

fn check_literal<'a, K: Kind, S: Store>(lit: Literal<'a>) -> Outcome<(), BadValue<'a>> {
    K::from_literal::<S>(lit).map(|_| ())
}

// lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
fn render_text<K: Kind, S: Store>(text: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match K::from_text::<S>(text) {
        Outcome::Ok(v) => K::write::<S>(v, f),
        // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
        Outcome::Err(_) => Err(fmt::Error),
    }
}

fn render_literal<K: Kind, S: Store>(lit: Literal<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match K::from_literal::<S>(lit) {
        Outcome::Ok(v) => K::write::<S>(v, f),
        // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
        Outcome::Err(_) => Err(fmt::Error),
    }
}

/// A setting as a tool's table carries it: the kind and the scope erased to
/// names, with the kind's parsers and renderers kept as function pointers so
/// the row still checks and prints a value the way its type would. One type
/// over settings of every kind, per store.
#[derive(Clone, Copy)]
pub struct Declared<St: Store> {
    // lint:allow(no-bare-static-str) reason: a setting's static key. FIXME: port to Str.
    key:            &'static str,
    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    kind:           &'static str,
    // lint:allow(no-bare-static-str) reason: a scope's static name. FIXME: port to Str.
    scope:          &'static str,
    // lint:allow(no-bare-numeric, arvo-types-only, no-public-raw-field) reason: a table entry copied from the scope. FIXME: port to arvo's Bool.
    reads_user:     bool,
    // lint:allow(no-bare-numeric, arvo-types-only, no-public-raw-field) reason: a table entry copied from the scope. FIXME: port to arvo's Bool.
    reads_repo:     bool,
    // lint:allow(no-bare-numeric, arvo-types-only, no-public-raw-field) reason: a table entry copied from the kind. FIXME: port to arvo's Bool.
    quoted:         bool,
    // lint:allow(no-bare-static-str) reason: a setting's static default. FIXME: port to Str.
    default:        &'static str,
    // lint:allow(no-bare-static-str) reason: a setting's static doc. FIXME: port to Str.
    doc:            &'static str,
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    check_text:     for<'a> fn(&'a str) -> Outcome<(), BadValue<'a>>,
    check_literal:  for<'a> fn(Literal<'a>) -> Outcome<(), BadValue<'a>>,
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    render_text:    fn(&str, &mut fmt::Formatter<'_>) -> fmt::Result,
    render_literal: fn(Literal<'_>, &mut fmt::Formatter<'_>) -> fmt::Result,
    _store:         PhantomData<St>,
}

impl<St: Store> Declared<St> {
    /// The dotted key.
    // lint:allow(no-bare-static-str) reason: a setting's static key. FIXME: port to Str.
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// The kind's schema name.
    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// The scope's schema name.
    // lint:allow(no-bare-static-str) reason: a scope's static name. FIXME: port to Str.
    pub const fn scope(&self) -> &'static str {
        self.scope
    }

    /// Whether the person's file is read for it.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a table entry. FIXME: port to arvo's Bool.
    pub const fn reads_user(&self) -> bool {
        self.reads_user
    }

    /// Whether the repository's file is read for it.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a table entry. FIXME: port to arvo's Bool.
    pub const fn reads_repo(&self) -> bool {
        self.reads_repo
    }

    /// Whether a store quotes its canonical text when writing it.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a table entry. FIXME: port to arvo's Bool.
    pub const fn quoted(&self) -> bool {
        self.quoted
    }

    /// The default, in the kind's text form.
    // lint:allow(no-bare-static-str) reason: a setting's static default. FIXME: port to Str.
    pub const fn default(&self) -> &'static str {
        self.default
    }

    /// One sentence.
    // lint:allow(no-bare-static-str) reason: a setting's static doc. FIXME: port to Str.
    pub const fn doc(&self) -> &'static str {
        self.doc
    }

    /// Check text from a flag, a variable or a default.
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    pub fn check_text<'a>(&self, text: &'a str) -> Outcome<(), BadValue<'a>> {
        (self.check_text)(text)
    }

    /// Check a store's literal.
    pub fn check_literal<'a>(&self, lit: Literal<'a>) -> Outcome<(), BadValue<'a>> {
        (self.check_literal)(lit)
    }

    /// The canonical text of a value given as text, written into `f`.
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    pub fn render_text(&self, text: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.render_text)(text, f)
    }

    /// The canonical text of a store's literal, written into `f`.
    pub fn render_literal(&self, lit: Literal<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.render_literal)(lit, f)
    }

    /// The first thing wrong with a table, or `Isnt`.
    pub fn defect(table: &[Declared<St>]) -> Maybe<BadTable> {
        for (i, row) in table.iter().enumerate() {
            if !key_is_wellformed(row.key) {
                return Maybe::Is(BadTable::KeyNotDotted(row.key));
            }
            if table[.. i].iter().any(|r| r.key == row.key) {
                return Maybe::Is(BadTable::KeyTwice(row.key));
            }
            if row.check_text(row.default).is_err() {
                return Maybe::Is(BadTable::DefaultRefused(row.key, row.kind));
            }
        }
        Maybe::Isnt
    }
}

impl<St: Store> fmt::Debug for Declared<St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Declared")
            .field("key", &self.key)
            .field("kind", &self.kind)
            .field("scope", &self.scope)
            .field("default", &self.default)
            .finish_non_exhaustive()
    }
}

/// What is wrong with a table, by row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadTable {
    /// The key is not a dotted identifier, so it names no variable and no
    /// file entry.
    // lint:allow(no-bare-static-str) reason: the row's static key. FIXME: port to Str.
    KeyNotDotted(&'static str),
    /// Declared twice, so a reader would take whichever came last.
    // lint:allow(no-bare-static-str) reason: the row's static key. FIXME: port to Str.
    KeyTwice(&'static str),
    /// The default is refused by the row's own kind, named second.
    // lint:allow(no-bare-static-str) reason: the row's static key and kind. FIXME: port to Str.
    DefaultRefused(&'static str, &'static str),
}

impl fmt::Display for BadTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadTable::KeyNotDotted(k) => {
                write!(
                    f,
                    "setting key {k:?} is not a dotted identifier, so it names no variable and no \
                     file entry"
                )
            },
            BadTable::KeyTwice(k) => {
                write!(
                    f,
                    "setting {k:?} is declared twice, so a reader takes whichever came last"
                )
            },
            BadTable::DefaultRefused(k, kind) => {
                write!(f, "the default of setting {k:?} is not {kind}")
            },
        }
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

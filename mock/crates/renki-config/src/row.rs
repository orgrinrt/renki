//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The erased row a tool's table carries, built from a typed setting at
//! compile time or from a kind's name at run time, and the check of a table.
//!
//! Every `bool` and `&str` here is marked for the port to arvo's `Bool` and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;
use core::marker::PhantomData;

use notko::{Maybe, Outcome};

use crate::layer::same;
use crate::meta::is_meta;
use crate::setting::{
    check_literal,
    check_run,
    check_text,
    render_literal,
    render_run,
    render_text,
};
use crate::{
    BadValue,
    Bool,
    Either,
    Int,
    Kind,
    List,
    ListRun,
    Literal,
    PathText,
    PredicateName,
    Repo,
    Scope,
    Store,
    Text,
    User,
    key_is_wellformed,
};

/// A setting as a tool's table carries it: the kind and the scope erased to
/// names, with the kind's parsers and renderers kept as function pointers so
/// the row still checks and prints a value the way its type would. One type
/// over settings of every kind, per store. Its strings are borrowed for `'a`,
/// so a table read at run time is the same type over the text it was read
/// from; a launcher's static table is `Declared<'static, St>`.
#[derive(Clone, Copy)]
pub struct Declared<'a, St: Store> {
    // lint:allow(no-bare-string) reason: a setting's key, borrowed. FIXME: port to Str.
    key:            &'a str,
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
    // lint:allow(no-bare-string) reason: a setting's default, borrowed. FIXME: port to Str.
    default:        &'a str,
    // lint:allow(no-bare-string) reason: a setting's doc, borrowed. FIXME: port to Str.
    doc:            &'a str,
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    check_text:     for<'t> fn(&'t str) -> Outcome<(), BadValue<'t>>,
    check_literal:  for<'t> fn(Literal<'t, St>) -> Outcome<(), BadValue<'t>>,
    check_run:      for<'t> fn(ListRun<'t, St>) -> Outcome<(), BadValue<'t>>,
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    render_text:    fn(&str, &mut fmt::Formatter<'_>) -> fmt::Result,
    render_literal: fn(Literal<'_, St>, &mut fmt::Formatter<'_>) -> fmt::Result,
    render_run:     fn(ListRun<'_, St>, &mut fmt::Formatter<'_>) -> fmt::Result,
    _store:         PhantomData<St>,
}

impl<'a, St: Store> Declared<'a, St> {
    /// The row of kind `K` and scope `S`, the one place a row's pointers and
    /// names are put together.
    // lint:allow(no-bare-string) reason: the setting's key, default and doc, borrowed. FIXME: port to Str.
    pub(crate) const fn of<K: Kind, S: Scope>(
        key: &'a str,
        default: &'a str,
        doc: &'a str,
    ) -> Self {
        Declared {
            key,
            kind: K::NAME,
            scope: S::NAME,
            reads_user: S::USER,
            reads_repo: S::REPO,
            quoted: K::QUOTED,
            default,
            doc,
            check_text: check_text::<K, St>,
            check_literal: check_literal::<K, St>,
            check_run: check_run::<K, St>,
            render_text: render_text::<K, St>,
            render_literal: render_literal::<K, St>,
            render_run: render_run::<K, St>,
            _store: PhantomData,
        }
    }

    /// A row read at run time, its kind and its scope taken by name: `bool`,
    /// `int`, `text`, `path`, `list` (of text) or `predicate`, and `user`,
    /// `repo` or `either`. A choice takes its options from a type, so no name
    /// builds one; a name nothing ships is refused.
    // lint:allow(no-bare-string) reason: the row's text, borrowed from where it was read. FIXME: port to Str.
    pub const fn named(
        key: &'a str,
        kind: &'a str,
        scope: &'a str,
        default: &'a str,
        doc: &'a str,
    ) -> Outcome<Self, BadTable<'a>> {
        if same(scope, User::NAME) {
            Self::by_kind::<User>(key, kind, default, doc)
        } else if same(scope, Repo::NAME) {
            Self::by_kind::<Repo>(key, kind, default, doc)
        } else if same(scope, Either::NAME) {
            Self::by_kind::<Either>(key, kind, default, doc)
        } else {
            Outcome::Err(BadTable::UnknownScope(key, scope))
        }
    }

    // lint:allow(no-bare-string) reason: the row's text, borrowed from where it was read. FIXME: port to Str.
    const fn by_kind<S: Scope>(
        key: &'a str,
        kind: &'a str,
        default: &'a str,
        doc: &'a str,
    ) -> Outcome<Self, BadTable<'a>> {
        let row = if same(kind, Bool::NAME) {
            Self::of::<Bool, S>(key, default, doc)
        } else if same(kind, Int::NAME) {
            Self::of::<Int, S>(key, default, doc)
        } else if same(kind, Text::NAME) {
            Self::of::<Text, S>(key, default, doc)
        } else if same(kind, PathText::NAME) {
            Self::of::<PathText, S>(key, default, doc)
        } else if same(kind, List::<Text>::NAME) {
            Self::of::<List<Text>, S>(key, default, doc)
        } else if same(kind, PredicateName::NAME) {
            Self::of::<PredicateName, S>(key, default, doc)
        } else {
            return Outcome::Err(BadTable::UnknownKind(key, kind));
        };
        Outcome::Ok(row)
    }

    /// The dotted key.
    // lint:allow(no-bare-string) reason: a setting's key, borrowed. FIXME: port to Str.
    pub const fn key(&self) -> &'a str {
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
    // lint:allow(no-bare-string) reason: a setting's default, borrowed. FIXME: port to Str.
    pub const fn default(&self) -> &'a str {
        self.default
    }

    /// One sentence.
    // lint:allow(no-bare-string) reason: a setting's doc, borrowed. FIXME: port to Str.
    pub const fn doc(&self) -> &'a str {
        self.doc
    }

    /// Check text from a flag, a variable or a default.
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    pub fn check_text<'t>(&self, text: &'t str) -> Outcome<(), BadValue<'t>> {
        (self.check_text)(text)
    }

    /// Check a store's literal.
    pub fn check_literal<'t>(&self, lit: Literal<'t, St>) -> Outcome<(), BadValue<'t>> {
        (self.check_literal)(lit)
    }

    /// Check a list appended across a stack of layers.
    pub fn check_run<'t>(&self, run: ListRun<'t, St>) -> Outcome<(), BadValue<'t>> {
        (self.check_run)(run)
    }

    /// The canonical text of a value given as text, written into `f`.
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    pub fn render_text(&self, text: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.render_text)(text, f)
    }

    /// The canonical text of a store's literal, written into `f`.
    pub fn render_literal(&self, lit: Literal<'_, St>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.render_literal)(lit, f)
    }

    /// The canonical text of a list appended across a stack, written into `f`.
    pub fn render_run(&self, run: ListRun<'_, St>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.render_run)(run, f)
    }

    /// The first thing wrong with a table, or `Isnt`: a key that is not a
    /// dotted identifier, a key under `meta`, which is the reader's and no
    /// tool's, two rows sharing a key, or a default the row's own kind
    /// refuses.
    pub fn defect(table: &[Declared<'a, St>]) -> Maybe<BadTable<'a>> {
        for (i, row) in table.iter().enumerate() {
            if !key_is_wellformed(row.key) {
                return Maybe::Is(BadTable::KeyNotDotted(row.key));
            }
            if is_meta(row.key) {
                return Maybe::Is(BadTable::UnderMeta(row.key));
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

impl<St: Store> fmt::Debug for Declared<'_, St> {
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
pub enum BadTable<'a> {
    /// The key is not a dotted identifier, so it names no variable and no
    /// file entry.
    // lint:allow(no-bare-string) reason: the row's key. FIXME: port to Str.
    KeyNotDotted(&'a str),
    /// The key is `meta` or under it, which is the reader's and no tool's.
    // lint:allow(no-bare-string) reason: the row's key. FIXME: port to Str.
    UnderMeta(&'a str),
    /// Declared twice, so a reader would take whichever came last.
    // lint:allow(no-bare-string) reason: the row's key. FIXME: port to Str.
    KeyTwice(&'a str),
    /// The default is refused by the row's own kind, named second.
    // lint:allow(no-bare-string, no-bare-static-str) reason: the row's key and kind. FIXME: port to Str.
    DefaultRefused(&'a str, &'static str),
    /// The kind, named second, is none a row can be built from by name.
    // lint:allow(no-bare-string) reason: the row's key and kind. FIXME: port to Str.
    UnknownKind(&'a str, &'a str),
    /// The scope, named second, is none of `user`, `repo` and `either`.
    // lint:allow(no-bare-string) reason: the row's key and scope. FIXME: port to Str.
    UnknownScope(&'a str, &'a str),
}

impl fmt::Display for BadTable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadTable::KeyNotDotted(k) => {
                write!(
                    f,
                    "setting key {k:?} is not a dotted identifier, so it names no variable and no \
                     file entry"
                )
            },
            BadTable::UnderMeta(k) => {
                write!(
                    f,
                    "setting key {k:?} is under meta, which is the reader's and no tool's"
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
            BadTable::UnknownKind(k, kind) => {
                write!(
                    f,
                    "setting {k:?} is of kind {kind:?}, which is none of bool, int, text, path, \
                     list and predicate"
                )
            },
            BadTable::UnknownScope(k, scope) => {
                write!(
                    f,
                    "setting {k:?} is of scope {scope:?}, which is none of user, repo and either"
                )
            },
        }
    }
}

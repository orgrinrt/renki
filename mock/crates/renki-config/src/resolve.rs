//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The resolver: every declared setting from five places in a fixed order,
//! each answer carrying which place won.
//!
//! Every `&str` here is marked for the port to `hilavitkutin_str::Str`, which
//! is a later unit; the markers sit above the item they cover, which is where
//! rustfmt leaves them alone.

use core::fmt;

use notko::{Maybe, Outcome};

use crate::meta::is_meta;
use crate::{
    BadMerge,
    BadValue,
    Declared,
    Layer,
    ListRun,
    Literal,
    Origin,
    Part,
    Stack,
    Store,
    stacked,
};

/// The two answers only the caller can give: what a flag said for a key, and
/// what the environment holds under a variable's name. The launcher reads
/// both; an engine reading its own environment answers `env` and nothing for
/// `flag`.
pub trait Lookup {
    /// The value of the tool's flag for `key`, where one was passed.
    // lint:allow(no-bare-string) reason: a dotted key and the caller's own text. FIXME: port to Str.
    fn flag<'s>(&'s self, key: &str) -> Maybe<&'s str>;
    /// The value of the variable `name` renders to, where it is set to text.
    // lint:allow(no-bare-string) reason: the caller's own text. FIXME: port to Str.
    fn env<'s>(&'s self, name: EnvKey<'_>) -> Maybe<&'s str>;
}

/// A setting's environment variable, `<SHORT>_CFG_<KEY>`, uppercased with the
/// key's dots as underscores, rendered through [`fmt::Display`] so the
/// launcher that sets it and an engine that reads it agree on the name
/// without either allocating to compute it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvKey<'a> {
    // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor. FIXME: port to Str.
    short: &'a str,
    // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor. FIXME: port to Str.
    key:   &'a str,
}

impl<'a> EnvKey<'a> {
    /// The variable for `key` under the tool whose short name is `short`.
    // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor. FIXME: port to Str.
    pub const fn of(short: &'a str, key: &'a str) -> Self {
        EnvKey {
            short,
            key,
        }
    }

    /// The variable naming the person's file itself, `<SHORT>_CONFIG_FILE`.
    /// Not `<SHORT>_CONFIG`, which `renki-dirs` gives the config root, the
    /// directory the file sits in.
    // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor. FIXME: port to Str.
    pub const fn file(short: &'a str) -> Self {
        EnvKey {
            short,
            key: "",
        }
    }
}

impl fmt::Display for EnvKey<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write;
        for c in self.short.chars() {
            f.write_char(c.to_ascii_uppercase())?;
        }
        if self.key.is_empty() {
            return f.write_str("_CONFIG_FILE");
        }
        f.write_str("_CFG_")?;
        for c in self.key.chars() {
            f.write_char(if c == '.' { '_' } else { c.to_ascii_uppercase() })?;
        }
        // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
        Ok(())
    }
}

/// A value as it was found: text from a flag, a variable or a default, a
/// store's literal from one layer, or a list appended across a stack.
pub enum Value<'a, St: Store> {
    /// Text, in the kind's own form.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    Text(&'a str),
    /// A literal, from a layer's document.
    Stored(Literal<'a>),
    /// A list, from every layer of a stack that holds it.
    Run(ListRun<'a, St>),
}

impl<St: Store> Clone for Value<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Value<'_, St> {}

impl<St: Store> PartialEq for Value<'_, St> {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: `PartialEq::eq` is core's signature.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Text(a), Value::Text(b)) => a == b,
            (Value::Stored(a), Value::Stored(b)) => a == b,
            (Value::Run(a), Value::Run(b)) => a == b,
            _ => false,
        }
    }
}

impl<St: Store> fmt::Debug for Value<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Text(t) => f.debug_tuple("Text").field(t).finish(),
            Value::Stored(l) => f.debug_tuple("Stored").field(l).finish(),
            Value::Run(r) => f.debug_tuple("Run").field(r).finish(),
        }
    }
}

/// One setting, resolved: the row, the value, where it came from and, for a
/// value from a file, the layer it came from. Displays as the value's
/// canonical text, which is what the engine reads out of its environment.
pub struct ResolvedView<'a, St: Store> {
    row:    &'a Declared<'a, St>,
    source: Origin,
    layer:  Maybe<Part<'a>>,
    value:  Value<'a, St>,
}

impl<St: Store> Clone for ResolvedView<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for ResolvedView<'_, St> {}

impl<St: Store> fmt::Debug for ResolvedView<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedView")
            .field("row", &self.row)
            .field("source", &self.source)
            .field("layer", &self.layer)
            .field("value", &self.value)
            .finish()
    }
}

impl<'a, St: Store> ResolvedView<'a, St> {
    /// The row this resolved.
    pub const fn row(&self) -> &'a Declared<'a, St> {
        self.row
    }

    /// Where the value came from.
    pub const fn source(&self) -> Origin {
        self.source
    }

    /// For [`Origin::Repo`] and [`Origin::User`], the layer of that stack the
    /// value came from; `Isnt` for every other origin.
    pub const fn layer(&self) -> Maybe<Part<'a>> {
        self.layer
    }

    /// The value as it was found.
    pub const fn value(&self) -> Value<'a, St> {
        self.value
    }
}

impl<St: Store> fmt::Display for ResolvedView<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value {
            Value::Text(t) => self.row.render_text(t, f),
            Value::Stored(l) => self.row.render_literal(l, f),
            Value::Run(r) => self.row.render_run(r, f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause<'a> {
    Value(BadValue<'a>),
    Merge(BadMerge<'a>),
}

/// Why a setting did not resolve: a value its kind refuses, or layers of a
/// stack that do not merge at its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadConfig<'a> {
    // lint:allow(no-bare-string) reason: the row's key. FIXME: port to Str.
    key:    &'a str,
    source: Origin,
    layer:  Maybe<Part<'a>>,
    cause:  Cause<'a>,
}

impl<'a> BadConfig<'a> {
    /// The setting.
    // lint:allow(no-bare-string) reason: the row's key. FIXME: port to Str.
    pub const fn key(&self) -> &'a str {
        self.key
    }

    /// The place the refused value came from, so the person knows which of
    /// five files or variables to open.
    pub const fn source(&self) -> Origin {
        self.source
    }

    /// For a file, the layer of its stack the refused value came from.
    pub const fn layer(&self) -> Maybe<Part<'a>> {
        self.layer
    }

    /// The kind's refusal, where the kind refused the value.
    pub const fn why(&self) -> Maybe<BadValue<'a>> {
        match self.cause {
            Cause::Value(v) => Maybe::Is(v),
            Cause::Merge(_) => Maybe::Isnt,
        }
    }

    /// The merge's refusal, where the layers did not merge at the key.
    pub const fn merge(&self) -> Maybe<BadMerge<'a>> {
        match self.cause {
            Cause::Merge(m) => Maybe::Is(m),
            Cause::Value(_) => Maybe::Isnt,
        }
    }
}

impl fmt::Display for BadConfig<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} from {}", self.key, self.source)?;
        if let Maybe::Is(Part::Layer(name)) = self.layer {
            write!(f, ":{}", name.word())?;
        }
        match self.cause {
            Cause::Value(v) => write!(f, ": {v}"),
            Cause::Merge(m) => write!(f, ": {m}"),
        }
    }
}

/// Resolve every row, in the table's order.
///
/// Precedence, highest first: the flag, the variable, the repository's stack
/// where the row's scope reads it, the person's stack where it does, the
/// default. Either stack may be empty, and each was checked whole when it was
/// made. Inside a stack the layers merge at the
/// key by [`stacked`]. The first place holding a value wins and is checked by
/// the row's kind; a refusal names the place and the layer, since a wrong
/// value in the repository's file wants a different repair than one in a
/// variable.
// lint:allow(no-bare-string) reason: the tool's short name, borrowed from its descriptor. FIXME: port to Str.
pub fn resolve<'a, St: Store, L: Lookup>(
    rows: &'a [Declared<'a, St>],
    short: &'a str,
    lookup: &'a L,
    repo: Stack<'a, St>,
    user: Stack<'a, St>,
) -> impl Iterator<Item = Outcome<ResolvedView<'a, St>, BadConfig<'a>>> + 'a {
    rows.iter().map(move |row| {
        let (source, layer, value) = match find(row, short, lookup, repo, user) {
            Outcome::Ok(found) => found,
            Outcome::Err(e) => return Outcome::Err(e),
        };
        let checked = match value {
            Value::Text(t) => row.check_text(t),
            Value::Stored(l) => row.check_literal(l),
            Value::Run(r) => row.check_run(r),
        };
        match checked {
            Outcome::Ok(()) => {
                Outcome::Ok(ResolvedView {
                    row,
                    source,
                    layer,
                    value,
                })
            },
            Outcome::Err(why) => {
                Outcome::Err(BadConfig {
                    key: row.key(),
                    source,
                    layer,
                    cause: Cause::Value(why),
                })
            },
        }
    })
}

type Found<'a, St> = (Origin, Maybe<Part<'a>>, Value<'a, St>);

// lint:allow(no-bare-string) reason: the tool's short name, borrowed from its descriptor. FIXME: port to Str.
fn find<'a, St: Store, L: Lookup>(
    row: &'a Declared<'a, St>,
    short: &'a str,
    lookup: &'a L,
    repo: Stack<'a, St>,
    user: Stack<'a, St>,
) -> Outcome<Found<'a, St>, BadConfig<'a>> {
    if let Maybe::Is(t) = lookup.flag(row.key()) {
        return Outcome::Ok((Origin::Flag, Maybe::Isnt, Value::Text(t)));
    }
    if let Maybe::Is(t) = lookup.env(EnvKey::of(short, row.key())) {
        return Outcome::Ok((Origin::Env, Maybe::Isnt, Value::Text(t)));
    }
    for (reads, source, stack) in
        [(row.reads_repo(), Origin::Repo, repo), (row.reads_user(), Origin::User, user)]
    {
        if !reads {
            continue;
        }
        match stacked(&stack, row.key()) {
            Outcome::Ok(Maybe::Is((layer, value))) => {
                return Outcome::Ok((source, Maybe::Is(layer.part()), value));
            },
            Outcome::Ok(Maybe::Isnt) => {},
            Outcome::Err(e) => {
                return Outcome::Err(BadConfig {
                    key: row.key(),
                    source,
                    layer: Maybe::Isnt,
                    cause: Cause::Merge(e),
                });
            },
        }
    }
    Outcome::Ok((Origin::Default, Maybe::Isnt, Value::Text(row.default())))
}

/// The keys a stack's layers hold that no row declares, each with the layer
/// that holds it, so a typo in a hand edit is refused by name and file
/// rather than silently being a default. Keys under `meta` are the reader's
/// and never among them.
// lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
pub fn unknown_keys<'d, St: Store>(
    rows: &'d [Declared<'_, St>],
    stack: &'d [Layer<'d, St>],
) -> impl Iterator<Item = (&'d str, &'d Layer<'d, St>)> + 'd {
    stack.iter().flat_map(move |layer| {
        St::keys(layer.doc())
            .filter(move |k| !is_meta(k) && !rows.iter().any(|r| r.key() == *k))
            .map(move |k| (k, layer))
    })
}

/// The keys the repository's stack holds that are the person's alone, each
/// with the layer that holds it, so a clone cannot set somebody's theme.
// lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
pub fn misplaced_keys<'d, St: Store>(
    rows: &'d [Declared<'_, St>],
    repo: &'d [Layer<'d, St>],
) -> impl Iterator<Item = (&'d str, &'d Layer<'d, St>)> + 'd {
    repo.iter().flat_map(move |layer| {
        St::keys(layer.doc())
            .filter(move |k| !is_meta(k) && rows.iter().any(|r| r.key() == *k && !r.reads_repo()))
            .map(move |k| (k, layer))
    })
}

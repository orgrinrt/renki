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

use crate::{BadValue, Declared, Literal, Source, Store};

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

/// A value as it was found: text from a flag, a variable or a default, or a
/// store's literal from a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    /// Text, in the kind's own form.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    Text(&'a str),
    /// A literal, from a document.
    Stored(Literal<'a>),
}

/// One setting, resolved: the row, the value, and where it came from.
/// Displays as the value's canonical text, which is what the engine reads
/// out of its environment.
#[derive(Debug, Clone, Copy)]
pub struct Resolved<'a, St: Store> {
    row:    &'a Declared<St>,
    source: Source,
    value:  Value<'a>,
}

impl<'a, St: Store> Resolved<'a, St> {
    /// The row this resolved.
    pub const fn row(&self) -> &'a Declared<St> {
        self.row
    }

    /// Where the value came from.
    pub const fn source(&self) -> Source {
        self.source
    }

    /// The value as it was found.
    pub const fn value(&self) -> Value<'a> {
        self.value
    }
}

impl<St: Store> fmt::Display for Resolved<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value {
            Value::Text(t) => self.row.render_text(t, f),
            Value::Stored(l) => self.row.render_literal(l, f),
        }
    }
}

/// Why a setting did not resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadConfig<'a> {
    // lint:allow(no-bare-static-str) reason: the row's static key. FIXME: port to Str.
    key:    &'static str,
    source: Source,
    why:    BadValue<'a>,
}

impl<'a> BadConfig<'a> {
    /// The setting.
    // lint:allow(no-bare-static-str) reason: the row's static key. FIXME: port to Str.
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// The place the refused value came from, so the person knows which of
    /// five files or variables to open.
    pub const fn source(&self) -> Source {
        self.source
    }

    /// The kind's refusal.
    pub const fn why(&self) -> BadValue<'a> {
        self.why
    }
}

impl fmt::Display for BadConfig<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} from {}: {}", self.key, self.source, self.why)
    }
}

/// Resolve every row, in the table's order.
///
/// Precedence, highest first: the flag, the variable, the repository's
/// document where the row's scope reads it, the person's document where it
/// does, the default. The first place holding a value wins and is checked
/// by the row's kind; a refusal names the place, since a wrong value in the
/// repository's file wants a different repair than one in a variable.
// lint:allow(no-bare-string) reason: the tool's short name, borrowed from its descriptor. FIXME: port to Str.
pub fn resolve<'a, St: Store, L: Lookup>(
    rows: &'a [Declared<St>],
    short: &'a str,
    lookup: &'a L,
    repo: Maybe<&'a St::Document<'a>>,
    user: Maybe<&'a St::Document<'a>>,
) -> impl Iterator<Item = Outcome<Resolved<'a, St>, BadConfig<'a>>> + 'a {
    rows.iter().map(move |row| {
        let (source, value) = find(row, short, lookup, repo, user);
        let checked = match value {
            Value::Text(t) => row.check_text(t),
            Value::Stored(l) => row.check_literal(l),
        };
        match checked {
            Outcome::Ok(()) => {
                Outcome::Ok(Resolved {
                    row,
                    source,
                    value,
                })
            },
            Outcome::Err(why) => {
                Outcome::Err(BadConfig {
                    key: row.key(),
                    source,
                    why,
                })
            },
        }
    })
}

// lint:allow(no-bare-string) reason: the tool's short name, borrowed from its descriptor. FIXME: port to Str.
fn find<'a, St: Store, L: Lookup>(
    row: &'a Declared<St>,
    short: &'a str,
    lookup: &'a L,
    repo: Maybe<&'a St::Document<'a>>,
    user: Maybe<&'a St::Document<'a>>,
) -> (Source, Value<'a>) {
    if let Maybe::Is(t) = lookup.flag(row.key()) {
        return (Source::Flag, Value::Text(t));
    }
    if let Maybe::Is(t) = lookup.env(EnvKey::of(short, row.key())) {
        return (Source::Env, Value::Text(t));
    }
    if row.reads_repo()
        && let Maybe::Is(doc) = repo
        && let Maybe::Is(l) = St::get(doc, row.key())
    {
        return (Source::Repo, Value::Stored(l));
    }
    if row.reads_user()
        && let Maybe::Is(doc) = user
        && let Maybe::Is(l) = St::get(doc, row.key())
    {
        return (Source::User, Value::Stored(l));
    }
    (Source::Default, Value::Text(row.default()))
}

/// The keys a document holds that no row declares, so a typo in a hand edit
/// is refused by name rather than silently being a default.
// lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
pub fn unknown_keys<'d, St: Store>(
    rows: &'d [Declared<St>],
    doc: &'d St::Document<'_>,
) -> impl Iterator<Item = &'d str> + 'd {
    St::keys(doc).filter(move |k| !rows.iter().any(|r| r.key() == *k))
}

/// The keys the repository's document holds that are the person's alone, so
/// a clone cannot set somebody's theme.
// lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
pub fn misplaced_keys<'d, St: Store>(
    rows: &'d [Declared<St>],
    repo: &'d St::Document<'_>,
) -> impl Iterator<Item = &'d str> + 'd {
    St::keys(repo).filter(move |k| rows.iter().any(|r| r.key() == *k && !r.reads_repo()))
}

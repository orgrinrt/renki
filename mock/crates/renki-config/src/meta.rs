//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `meta` table, read before the rest of any layer, and the one glob rule
//! its includes are matched by.
//!
//! Every `bool` and `&str` here is marked for the port to arvo's `Bool` and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;
use core::marker::PhantomData;

use notko::Maybe;

use crate::{BadLayer, Literal, Part, Store};

// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const INCLUDE: &str = "meta.overlays.include";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const ORDER: &str = "meta.overlays.order";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const UNTRACKED: &str = "meta.overlays.untracked";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const WHEN: &str = "meta.when";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const REPLACE: &str = "meta.replace";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const UNSET: &str = "meta.unset";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
pub(crate) const PREDICATES: &str = "meta.predicates.";

/// Whether a dotted key is `meta` or under it, and so the reader's and no
/// tool's.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: a dotted key and a predicate over it. FIXME: port to Str and arvo's Bool.
pub(crate) fn is_meta(key: &str) -> bool {
    key == "meta" || key.starts_with("meta.")
}

/// The `meta` table of one layer, read out of its document through the
/// store: `meta.overlays.include`, `meta.overlays.order`,
/// `meta.overlays.untracked`, `meta.when`, `meta.predicates`, `meta.replace`
/// and `meta.unset`. The definitions under `meta.predicates` are read by
/// [`Predicates`](crate::Predicates); this keeps only whether there are any.
pub struct Meta<'d, St: Store> {
    include:   Maybe<Literal<'d>>,
    order:     Maybe<Literal<'d>>,
    untracked: Maybe<Literal<'d>>,
    when:      Maybe<Literal<'d>>,
    replace:   Maybe<Literal<'d>>,
    unset:     Maybe<Literal<'d>>,
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    stray:     Maybe<&'d str>,
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    defines:   Maybe<&'d str>,
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    unheld:    Maybe<&'d str>,
    mixed:     Maybe<BadLayer<'d>>,
    _store:    PhantomData<St>,
}

impl<St: Store> Clone for Meta<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Meta<'_, St> {}

impl<St: Store> fmt::Debug for Meta<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Meta")
            .field("include", &self.include)
            .field("order", &self.order)
            .field("untracked", &self.untracked)
            .field("when", &self.when)
            .field("replace", &self.replace)
            .field("unset", &self.unset)
            .finish_non_exhaustive()
    }
}

impl<'d, St: Store> Meta<'d, St> {
    /// The layers that must not be tracked by git where no
    /// `meta.overlays.untracked` says otherwise.
    // lint:allow(no-bare-static-str) reason: the default's static words. FIXME: port to Str.
    pub const UNTRACKED: [&'static str; 3] = ["local", "secret", "private"];

    /// The `meta` table of `doc`.
    pub fn read(doc: &'d St::Document<'_>) -> Self {
        let mut stray = Maybe::Isnt;
        let mut defines = Maybe::Isnt;
        for key in St::keys(doc) {
            if !is_meta(key) {
                continue;
            }
            if key.starts_with(PREDICATES) {
                if defines.isnt() {
                    defines = Maybe::Is(key);
                }
                continue;
            }
            let known = [INCLUDE, ORDER, UNTRACKED, WHEN, REPLACE, UNSET].contains(&key);
            if !known && stray.isnt() {
                stray = Maybe::Is(key);
            }
        }
        let replace = St::get(doc, REPLACE);
        // the first list of keys or entries holding an item that is no string
        let mut mixed = Maybe::Isnt;
        for key in [INCLUDE, REPLACE, UNSET] {
            if let Maybe::Is(Literal::List(list)) = St::get(doc, key)
                && St::items(list).any(|item| !matches!(item, Literal::Str(_)))
            {
                mixed = Maybe::Is(BadLayer::MetaShape(key));
                break;
            }
        }
        let mut unheld = Maybe::Isnt;
        if let Maybe::Is(Literal::List(list)) = replace {
            for item in St::items(list) {
                if let Literal::Str(key) = item
                    && St::shape(doc, key).isnt()
                {
                    unheld = Maybe::Is(key);
                    break;
                }
            }
        }
        Meta {
            include: St::get(doc, INCLUDE),
            order: St::get(doc, ORDER),
            untracked: St::get(doc, UNTRACKED),
            when: St::get(doc, WHEN),
            replace,
            unset: St::get(doc, UNSET),
            stray,
            defines,
            unheld,
            mixed,
            _store: PhantomData,
        }
    }

    /// `meta.overlays.include`: entries relative to the file that names them,
    /// each a glob matching files, or a directory where it ends in `/`.
    pub const fn include(&self) -> Maybe<Literal<'d>> {
        self.include
    }

    /// `meta.overlays.order`, read by [`Order`](crate::Order).
    pub const fn order(&self) -> Maybe<Literal<'d>> {
        self.order
    }

    /// `meta.overlays.untracked`, where it is set; [`Meta::UNTRACKED`] where
    /// it is not.
    pub const fn untracked(&self) -> Maybe<Literal<'d>> {
        self.untracked
    }

    /// `meta.when`, the name of the predicate the layer is read under.
    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    pub const fn when(&self) -> Maybe<&'d str> {
        match self.when {
            Maybe::Is(Literal::Str(name)) => Maybe::Is(name),
            _ => Maybe::Isnt,
        }
    }

    /// `meta.replace`: the dotted keys of this layer whose values replace
    /// what the layers below hold.
    pub const fn replace(&self) -> Maybe<Literal<'d>> {
        self.replace
    }

    /// `meta.unset`: the dotted keys taken out of what the layers below hold.
    pub const fn unset(&self) -> Maybe<Literal<'d>> {
        self.unset
    }

    /// The first thing wrong with this table, for a layer that is `at`: a
    /// key that is none of the reader's, a key of the wrong shape, a list of
    /// entries or keys holding an item that is no string, an order,
    /// an untracked list or a predicate in a layer above the base, a `when`
    /// on the base or on `defaults`, and a key `meta.replace` names that this
    /// layer does not hold.
    pub const fn defect(&self, at: Part<'_>) -> Maybe<BadLayer<'d>> {
        if let Maybe::Is(key) = self.stray {
            return Maybe::Is(BadLayer::UnknownMeta(key));
        }
        let lists = [
            (INCLUDE, self.include),
            (ORDER, self.order),
            (UNTRACKED, self.untracked),
            (REPLACE, self.replace),
            (UNSET, self.unset),
        ];
        let mut i = 0;
        while i < lists.len() {
            if let (key, Maybe::Is(l)) = lists[i]
                && !matches!(l, Literal::List(_))
            {
                return Maybe::Is(BadLayer::MetaShape(key));
            }
            i += 1;
        }
        if let Maybe::Is(bad) = self.mixed {
            return Maybe::Is(bad);
        }
        if let Maybe::Is(l) = self.when
            && !matches!(l, Literal::Str(_))
        {
            return Maybe::Is(BadLayer::MetaShape(WHEN));
        }
        if at.always_read() {
            if self.when.is() {
                return Maybe::Is(BadLayer::AlwaysRead(WHEN));
            }
        } else {
            if self.order.is() {
                return Maybe::Is(BadLayer::AboveBase(ORDER));
            }
            if self.untracked.is() {
                return Maybe::Is(BadLayer::AboveBase(UNTRACKED));
            }
            if let Maybe::Is(key) = self.defines {
                return Maybe::Is(BadLayer::AboveBase(key));
            }
        }
        if let Maybe::Is(key) = self.unheld {
            return Maybe::Is(BadLayer::ReplaceNotHeld(key));
        }
        Maybe::Isnt
    }
}

/// Whether `name` matches `pattern` by the POSIX shell's pattern rule: `*`
/// for any run of bytes and `?` for any one, neither crossing a `/`, and
/// `[...]` for one byte of a set, `!` first negating it, `a-z` a range and a
/// `]` first a member. There is no `**`; two stars are one. A `[` with no
/// closing `]` is itself. A `.` that begins the name or follows a `/` is
/// matched by a `.` in the pattern alone, never by `*`, `?` or `[...]`.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: the caller's text and a predicate over it. FIXME: port to Str and arvo's Bool.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.as_bytes(), name.as_bytes());
    let (mut pi, mut ni) = (0, 0);
    // where the last star stood, and the byte of the name it has taken up to
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a scanner's own positions. FIXME: port to arvo.
    let mut back: Maybe<(usize, usize)> = Maybe::Isnt;
    while ni < n.len() {
        // a `.` that begins a name is matched by a `.` in the pattern alone
        let leading = n[ni] == b'.' && (ni == 0 || n[ni - 1] == b'/');
        let step = if pi < p.len() {
            match p[pi] {
                b'*' | b'?' | b'[' if leading => Maybe::Isnt,
                b'*' => {
                    back = Maybe::Is((pi, ni));
                    pi += 1;
                    continue;
                },
                b'?' if n[ni] != b'/' => Maybe::Is(pi + 1),
                b'[' => {
                    match bracket(p, pi, n[ni]) {
                        Bracket::Matches(next) => Maybe::Is(next),
                        Bracket::Misses => Maybe::Isnt,
                        Bracket::Open if n[ni] == b'[' => Maybe::Is(pi + 1),
                        Bracket::Open => Maybe::Isnt,
                    }
                },
                b'?' => Maybe::Isnt,
                c if c == n[ni] => Maybe::Is(pi + 1),
                _ => Maybe::Isnt,
            }
        } else {
            Maybe::Isnt
        };
        match step {
            Maybe::Is(next) => {
                pi = next;
                ni += 1;
            },
            Maybe::Isnt => {
                // give the last star one more byte, unless that byte is a `/`
                match back {
                    Maybe::Is((star, taken)) if n[taken] != b'/' => {
                        back = Maybe::Is((star, taken + 1));
                        pi = star + 1;
                        ni = taken + 1;
                    },
                    _ => return false,
                }
            },
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

enum Bracket {
    /// One byte matched; the pattern goes on past the `]`.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a scanner's own position. FIXME: port to arvo.
    Matches(usize),
    Misses,
    /// No closing `]`: the `[` is a byte like any other.
    Open,
}

/// The bracket expression opening at `p[at]`, against the byte `c`.
// lint:allow(no-bare-numeric, arvo-types-only) reason: a scanner's own positions and bytes. FIXME: port to arvo.
fn bracket(p: &[u8], at: usize, c: u8) -> Bracket {
    let mut i = at + 1;
    let negated = i < p.len() && p[i] == b'!';
    if negated {
        i += 1;
    }
    let first = i;
    let mut hit = false;
    while i < p.len() {
        if p[i] == b']' && i > first {
            let matched = hit != negated && c != b'/';
            return if matched { Bracket::Matches(i + 1) } else { Bracket::Misses };
        }
        if i + 2 < p.len() && p[i + 1] == b'-' && p[i + 2] != b']' {
            hit |= p[i] <= c && c <= p[i + 2];
            i += 3;
        } else {
            hit |= p[i] == c;
            i += 1;
        }
    }
    Bracket::Open
}

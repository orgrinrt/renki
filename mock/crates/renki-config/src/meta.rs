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

use crate::kind::got_of;
use crate::{BadLayer, Literal, Part, Shape, Store};

// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
const INCLUDE: &str = "meta.overlays.include";
// lint:allow(no-bare-string, no-bare-static-str) reason: the reader's static key names. FIXME: port to Str.
pub(crate) const ORDER: &str = "meta.overlays.order";
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

/// One of the lists the `meta` table holds, which a refusal of an item of it
/// names by its dotted key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaList {
    /// `meta.overlays.include`, of entries.
    Include,
    /// `meta.overlays.order`, of words.
    Order,
    /// `meta.overlays.untracked`, of words.
    Untracked,
    /// `meta.replace`, of keys.
    Replace,
    /// `meta.unset`, of keys.
    Unset,
}

impl fmt::Display for MetaList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MetaList::Include => INCLUDE,
            MetaList::Order => ORDER,
            MetaList::Untracked => UNTRACKED,
            MetaList::Replace => REPLACE,
            MetaList::Unset => UNSET,
        })
    }
}

/// The `meta` table of one layer, read out of its document through the
/// store: `meta.overlays.include`, `meta.overlays.order`,
/// `meta.overlays.untracked`, `meta.when`, `meta.predicates`, `meta.replace`
/// and `meta.unset`. The definitions under `meta.predicates` are read by
/// [`Predicates`](crate::Predicates); this keeps only whether there are any.
pub struct Meta<'d, St: Store> {
    include:   Maybe<Literal<'d, St>>,
    order:     Maybe<Literal<'d, St>>,
    untracked: Maybe<Literal<'d, St>>,
    when:      Maybe<Literal<'d, St>>,
    replace:   Maybe<Literal<'d, St>>,
    unset:     Maybe<Literal<'d, St>>,
    /// The first defect where the layer is always read.
    always:    Maybe<BadLayer<'d>>,
    /// The first defect where the layer is an overlay.
    overlay:   Maybe<BadLayer<'d>>,
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

    /// The `meta` table of `doc`, and its first defect for a layer that is
    /// always read and for an overlay: each key in the order the file holds
    /// them, asked for its shape, then its place, then its items.
    pub fn read(doc: &'d St::Document<'_>) -> Self {
        // the list keys, which list each is, and whether its items are words
        let lists = [
            (INCLUDE, MetaList::Include, false),
            (ORDER, MetaList::Order, true),
            (UNTRACKED, MetaList::Untracked, true),
            (REPLACE, MetaList::Replace, false),
            (UNSET, MetaList::Unset, false),
        ];
        let (mut always, mut overlay) = (Maybe::Isnt, Maybe::Isnt);
        for key in St::keys(doc) {
            if !is_meta(key) {
                continue;
            }
            // what is wrong with this key in a layer always read, and in an overlay
            let (a, o) = if ["meta", "meta.overlays", "meta.predicates"].contains(&key) {
                if matches!(St::shape(doc, key), Maybe::Is(Shape::Table)) {
                    (Maybe::Isnt, Maybe::Isnt)
                } else {
                    (
                        Maybe::Is(BadLayer::MetaShape(key)),
                        Maybe::Is(BadLayer::MetaShape(key)),
                    )
                }
            } else if key.starts_with(PREDICATES) {
                // the definitions are `Predicates`' to check, where they may stand
                (Maybe::Isnt, Maybe::Is(BadLayer::AboveBase(key)))
            } else if key == WHEN {
                if matches!(St::get(doc, WHEN), Maybe::Is(Literal::Str(_))) {
                    (Maybe::Is(BadLayer::AlwaysRead(WHEN)), Maybe::Isnt)
                } else {
                    (
                        Maybe::Is(BadLayer::MetaShape(WHEN)),
                        Maybe::Is(BadLayer::MetaShape(WHEN)),
                    )
                }
            } else if let Some(leaf) = [WHEN, INCLUDE, ORDER, UNTRACKED, REPLACE, UNSET]
                .into_iter()
                .find(|leaf| {
                    key.strip_prefix(leaf)
                        .is_some_and(|rest| rest.starts_with('.'))
                })
            {
                // a value under a key the reader holds as a leaf
                (
                    Maybe::Is(BadLayer::MetaShape(leaf)),
                    Maybe::Is(BadLayer::MetaShape(leaf)),
                )
            } else if let Some((name, list, words)) =
                lists.into_iter().find(|(name, ..)| *name == key)
            {
                match St::get(doc, name) {
                    Maybe::Is(Literal::List(items)) => {
                        let wrong_item = St::items(items).find(|i| !matches!(i, Literal::Str(_)));
                        let item = match wrong_item {
                            Some(i) if words => Maybe::Is(BadLayer::NotAWord(list, got_of(i))),
                            Some(i) => Maybe::Is(BadLayer::NotAString(list, got_of(i))),
                            None => Maybe::Isnt,
                        };
                        // a name `meta.replace` lists that this layer does not hold
                        let unheld = if name == REPLACE {
                            match St::items(items)
                                .find(|i| matches!(i, Literal::Str(k) if St::shape(doc, k).isnt()))
                            {
                                Some(Literal::Str(k)) => Maybe::Is(BadLayer::ReplaceNotHeld(k)),
                                _ => Maybe::Isnt,
                            }
                        } else {
                            Maybe::Isnt
                        };
                        let content = if item.is() { item } else { unheld };
                        let place =
                            if words { Maybe::Is(BadLayer::AboveBase(name)) } else { content };
                        (content, place)
                    },
                    _ => {
                        (
                            Maybe::Is(BadLayer::MetaShape(name)),
                            Maybe::Is(BadLayer::MetaShape(name)),
                        )
                    },
                }
            } else {
                (
                    Maybe::Is(BadLayer::UnknownMeta(key)),
                    Maybe::Is(BadLayer::UnknownMeta(key)),
                )
            };
            if always.isnt() {
                always = a;
            }
            if overlay.isnt() {
                overlay = o;
            }
        }
        Meta {
            include: St::get(doc, INCLUDE),
            order: St::get(doc, ORDER),
            untracked: St::get(doc, UNTRACKED),
            when: St::get(doc, WHEN),
            replace: St::get(doc, REPLACE),
            unset: St::get(doc, UNSET),
            always,
            overlay,
            _store: PhantomData,
        }
    }

    /// `meta.overlays.include`: entries relative to the file that names them,
    /// each a glob matching files, or a directory where it ends in `/`.
    pub const fn include(&self) -> Maybe<Literal<'d, St>> {
        self.include
    }

    /// `meta.overlays.order`, read by [`Order`](crate::Order).
    pub const fn order(&self) -> Maybe<Literal<'d, St>> {
        self.order
    }

    /// `meta.overlays.untracked`, where it is set; [`Meta::UNTRACKED`] where
    /// it is not.
    pub const fn untracked(&self) -> Maybe<Literal<'d, St>> {
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
    pub const fn replace(&self) -> Maybe<Literal<'d, St>> {
        self.replace
    }

    /// `meta.unset`: the dotted keys taken out of what the layers below hold.
    pub const fn unset(&self) -> Maybe<Literal<'d, St>> {
        self.unset
    }

    /// The first thing wrong with this table, for a layer that is `at`, in
    /// the order the file holds its keys: a key that is none of the reader's,
    /// a key of the wrong shape, an order, an untracked list or a predicate in
    /// a layer above the base, a `when` on the base or on `defaults`, a list
    /// holding an item of the wrong kind, and a key `meta.replace` names that
    /// this layer does not hold.
    pub const fn defect(&self, at: Part<'_>) -> Maybe<BadLayer<'d>> {
        if at.always_read() { self.always } else { self.overlay }
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

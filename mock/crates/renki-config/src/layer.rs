//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which files are a manifest's layers, and the order the layers above the
//! base stack in.
//!
//! Every `bool`, `u32` and `&str` here is marked for the port to arvo's types
//! and `hilavitkutin_str::Str`, which is a later unit; the markers sit above
//! the item they cover, which is where rustfmt leaves them alone.

use core::fmt;

use notko::{Maybe, Outcome};

use crate::meta::ORDER;
use crate::{Got, Literal, Meta, MetaList, Store};

/// The word of a layer, `local` in `name.local.ext`, borrowed from the file
/// name it was read from. Only [`layer_of`] makes one, so every word is one
/// of `[a-z0-9_-]` and never `base`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayerName<'a> {
    // lint:allow(no-bare-string) reason: borrowed from the caller's file name. FIXME: port to Str.
    word: &'a str,
}

impl<'a> LayerName<'a> {
    /// The word itself.
    // lint:allow(no-bare-string) reason: borrowed from the caller's file name. FIXME: port to Str.
    pub const fn word(&self) -> &'a str {
        self.word
    }

    /// Whether this is `defaults`, the one layer that sits below the base.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a predicate over the word. FIXME: port to arvo's Bool.
    pub const fn is_defaults(&self) -> bool {
        same(self.word, DEFAULTS)
    }
}

/// What a file of a manifest is: its base, or one of its layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part<'a> {
    /// The base file, `name.ext`.
    Base,
    /// A layer, `name.<word>.ext`.
    Layer(LayerName<'a>),
}

impl Part<'_> {
    /// Whether this part is always read, which the base and `defaults` are.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a predicate over the part. FIXME: port to arvo's Bool.
    pub const fn always_read(&self) -> bool {
        match self {
            Part::Base => true,
            Part::Layer(name) => name.is_defaults(),
        }
    }
}

/// Why a file cannot be a layer, or why an order cannot place one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadLayer<'a> {
    /// A file of the manifest's shape with two words, `name.a.b.ext`.
    // lint:allow(no-bare-string) reason: the caller's file name. FIXME: port to Str.
    TwoWords(&'a str),
    /// A file whose word is `base`, which names no layer.
    // lint:allow(no-bare-string) reason: the caller's file name. FIXME: port to Str.
    NamedBase(&'a str),
    /// A file whose word is empty or holds a byte outside `[a-z0-9_-]`.
    // lint:allow(no-bare-string) reason: the caller's file name. FIXME: port to Str.
    BadWord(&'a str),
    /// A word the order neither names nor covers with a `*`.
    // lint:allow(no-bare-string) reason: a layer's word. FIXME: port to Str.
    Unordered(&'a str),
    /// An order naming `defaults` or `base`, which stay below every overlay.
    // lint:allow(no-bare-string) reason: a word of the order. FIXME: port to Str.
    OrderNames(&'a str),
    /// An order naming a word, or `*`, twice.
    // lint:allow(no-bare-string) reason: a word of the order. FIXME: port to Str.
    OrderTwice(&'a str),
    /// An item of `meta.overlays.order` or `meta.overlays.untracked` that is
    /// no word: the list, and what the item holds.
    NotAWord(MetaList, Got<'a>),
    /// An item of `meta.overlays.include`, `meta.replace` or `meta.unset`
    /// that is no string: the list, and what the item holds.
    NotAString(MetaList, Got<'a>),
    /// A key under `meta` that is none of the reader's.
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    UnknownMeta(&'a str),
    /// A key under `meta` holding a value of the wrong shape.
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    MetaShape(&'a str),
    /// A key only the base and `defaults` may hold, held by an overlay.
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    AboveBase(&'a str),
    /// A key no layer that is always read may hold, held by one.
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    AlwaysRead(&'a str),
    /// A key `meta.replace` names that the layer does not hold.
    // lint:allow(no-bare-string) reason: a dotted key of the document. FIXME: port to Str.
    ReplaceNotHeld(&'a str),
}

impl fmt::Display for BadLayer<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadLayer::TwoWords(file) => write!(f, "{file} names two layer words; a layer is one"),
            BadLayer::NamedBase(file) => write!(f, "{file} names the word base, which is no layer"),
            BadLayer::BadWord(file) => {
                write!(f, "{file} has a layer word outside a-z, 0-9, _ and -")
            },
            BadLayer::Unordered(word) => {
                write!(
                    f,
                    "the layer {word} has no place in the order; add it or * to \
                     meta.overlays.order"
                )
            },
            BadLayer::OrderNames(word) => {
                write!(
                    f,
                    "meta.overlays.order names {word}, which stays below every overlay"
                )
            },
            BadLayer::OrderTwice(word) => write!(f, "meta.overlays.order names {word} twice"),
            BadLayer::NotAWord(key, got) => write!(f, "{key} holds {got}, which is no word"),
            BadLayer::NotAString(key, got) => write!(f, "{key} holds {got}, which is no string"),
            BadLayer::UnknownMeta(key) => write!(f, "{key} is no key of the meta table"),
            BadLayer::MetaShape(key) => write!(f, "{key} holds a value of the wrong shape"),
            BadLayer::AboveBase(key) => {
                write!(
                    f,
                    "{key} is read from the base and defaults only, not from an overlay"
                )
            },
            BadLayer::AlwaysRead(key) => {
                write!(
                    f,
                    "{key} is refused on the base and defaults, which are always read"
                )
            },
            BadLayer::ReplaceNotHeld(key) => {
                write!(
                    f,
                    "meta.replace names {key}, which this layer does not hold"
                )
            },
        }
    }
}

// lint:allow(no-bare-string, no-bare-static-str) reason: the layer word below the base. FIXME: port to Str.
const DEFAULTS: &str = "defaults";
// lint:allow(no-bare-string, no-bare-static-str) reason: the word no layer may take. FIXME: port to Str.
const BASE: &str = "base";
// lint:allow(no-bare-string, no-bare-static-str) reason: the order's word for every word it does not name. FIXME: port to Str.
const STAR: &str = "*";

/// Byte equality, usable in a `const fn`.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: the caller's text and a predicate over it. FIXME: port to Str and arvo's Bool.
pub(crate) const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// What `file` is of the manifest whose base is `name.<St::EXTENSION>`: the
/// base, a layer, or `Isnt` for a file of another name or extension. A file
/// of the manifest's shape that cannot be a layer is refused: two words, the
/// word `base`, or a word outside `[a-z0-9_-]`.
// lint:allow(no-bare-string) reason: the manifest's name and the caller's file name. FIXME: port to Str.
pub fn layer_of<'a, St: Store>(
    name: &str,
    file: &'a str,
) -> Outcome<Maybe<Part<'a>>, BadLayer<'a>> {
    let Maybe::Is(rest) = after(file, name) else {
        return Outcome::Ok(Maybe::Isnt);
    };
    let Maybe::Is(rest) = after(rest, ".") else {
        return Outcome::Ok(Maybe::Isnt);
    };
    if same(rest, St::EXTENSION) {
        return Outcome::Ok(Maybe::Is(Part::Base));
    }
    let Maybe::Is(middle) = before(rest, St::EXTENSION) else {
        return Outcome::Ok(Maybe::Isnt);
    };
    let Maybe::Is(word) = before(middle, ".") else {
        return Outcome::Ok(Maybe::Isnt);
    };
    if word.contains('.') {
        return Outcome::Err(BadLayer::TwoWords(file));
    }
    if same(word, BASE) {
        return Outcome::Err(BadLayer::NamedBase(file));
    }
    if word.is_empty()
        || !word
            .bytes()
            .all(|b| matches!(b, b'a' ..= b'z' | b'0' ..= b'9' | b'_' | b'-'))
    {
        return Outcome::Err(BadLayer::BadWord(file));
    }
    Outcome::Ok(Maybe::Is(Part::Layer(LayerName {
        word,
    })))
}

// lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
fn after<'a>(text: &'a str, prefix: &str) -> Maybe<&'a str> {
    match text.strip_prefix(prefix) {
        // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; folded here.
        Some(rest) => Maybe::Is(rest),
        // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; folded here.
        None => Maybe::Isnt,
    }
}

// lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
fn before<'a>(text: &'a str, suffix: &str) -> Maybe<&'a str> {
    match text.strip_suffix(suffix) {
        // lint:allow(no-bare-option) reason: `str::strip_suffix` is core's and answers in `Option`; folded here.
        Some(rest) => Maybe::Is(rest),
        // lint:allow(no-bare-option) reason: `str::strip_suffix` is core's and answers in `Option`; folded here.
        None => Maybe::Isnt,
    }
}

/// The place of every overlay above the base: a list of words, `*` standing
/// for every word the list does not name, in byte order of the word.
///
/// Read from the base's `meta.overlays.order`, else from the `defaults`
/// layer's, else it is [`Order::DEFAULT`]. Where it sits below the base, the
/// `defaults` layer is the caller's to place and no word of the order.
pub struct Order<'a, St: Store> {
    list: Maybe<St::List<'a>>,
}

impl<St: Store> Clone for Order<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Order<'_, St> {}

impl<St: Store> fmt::Debug for Order<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Order").field("list", &self.list).finish()
    }
}

// lint:allow(no-bare-string, no-bare-static-str) reason: the default order's static words. FIXME: port to Str.
const DEFAULT_WORDS: [&str; 4] = ["*", "local", "secret", "private"];

impl<'a, St: Store> Order<'a, St> {
    /// `["*", "local", "secret", "private"]`.
    pub const DEFAULT: Self = Order {
        list: Maybe::Isnt,
    };

    /// The order the base's `meta` sets, else the one `defaults` sets, else
    /// the default. An order naming `defaults` or `base`, naming a word twice,
    /// holding two `*`, or holding an item that is no word, is refused.
    pub fn read(
        base: &Meta<'a, St>,
        defaults: Maybe<&Meta<'a, St>>,
    ) -> Outcome<Self, BadLayer<'a>> {
        let set = match (base.order(), defaults) {
            (Maybe::Is(l), _) => Maybe::Is(l),
            (Maybe::Isnt, Maybe::Is(d)) => d.order(),
            (Maybe::Isnt, Maybe::Isnt) => Maybe::Isnt,
        };
        let list = match set {
            Maybe::Isnt => return Outcome::Ok(Self::DEFAULT),
            Maybe::Is(Literal::List(text)) => text,
            Maybe::Is(_) => return Outcome::Err(BadLayer::MetaShape(ORDER)),
        };
        for (i, item) in St::items(list).enumerate() {
            let word = match item {
                Literal::Str(w) => w,
                Literal::Bool(_) => {
                    return Outcome::Err(BadLayer::NotAWord(MetaList::Order, Got::Bool));
                },
                Literal::Int(_) => {
                    return Outcome::Err(BadLayer::NotAWord(MetaList::Order, Got::Int));
                },
                Literal::List(_) => {
                    return Outcome::Err(BadLayer::NotAWord(MetaList::Order, Got::List));
                },
            };
            if same(word, DEFAULTS) || same(word, BASE) {
                return Outcome::Err(BadLayer::OrderNames(word));
            }
            if St::items(list)
                .take(i)
                .any(|earlier| matches!(earlier, Literal::Str(e) if e == word))
            {
                return Outcome::Err(BadLayer::OrderTwice(word));
            }
        }
        Outcome::Ok(Order {
            list: Maybe::Is(list),
        })
    }

    /// The place of `name` among the overlays, as its position in the list
    /// and then its word, so sorting by it is the order. A word the list does
    /// not name takes the place of the list's `*`, and where the list has
    /// none it is refused, saying to add it or `*` to `meta.overlays.order`.
    // lint:allow(no-bare-numeric, arvo-types-only, no-bare-string) reason: a position and the word it sorts by. FIXME: port to arvo and Str.
    pub fn rank<'w>(&self, name: LayerName<'w>) -> Outcome<(u32, &'w str), BadLayer<'w>> {
        // lint:allow(no-bare-numeric, arvo-types-only) reason: a position in the list. FIXME: port to arvo.
        let mut star: Maybe<u32> = Maybe::Isnt;
        // lint:allow(no-bare-numeric, arvo-types-only) reason: a position in the list. FIXME: port to arvo.
        let mut at: u32 = 0;
        // lint:allow(no-bare-string) reason: a word of the order. FIXME: port to Str.
        let mut seen = |word: &str| {
            if word == name.word {
                return true;
            }
            if word == STAR {
                star = Maybe::Is(at);
            }
            at += 1;
            false
        };
        let found = match self.list {
            Maybe::Isnt => DEFAULT_WORDS.iter().any(|w| seen(w)),
            Maybe::Is(list) => St::items(list).any(|l| matches!(l, Literal::Str(w) if seen(w))),
        };
        if found {
            return Outcome::Ok((at, name.word));
        }
        match star {
            Maybe::Is(p) => Outcome::Ok((p, name.word)),
            Maybe::Isnt => Outcome::Err(BadLayer::Unordered(name.word)),
        }
    }
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The list kind: its value, walked item by item from text, from one
//! document's list, or from a list appended across a stack of layers.
//!
//! Every `bool` and `&str` here is marked for the port to arvo's `Bool` and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;
use core::marker::PhantomData;

use notko::{Maybe, Outcome};

use crate::kind::{bad, got_of};
use crate::{BadValue, Got, Kind, List, ListRun, Literal, Store};

/// A list's value: the items, parsed one at a time by the element kind, from
/// whichever side the list came. Iterating yields each item's parse, so a
/// bad item is refused where it sits rather than the whole list at once.
pub enum ListValue<'a, K: Kind, S: Store> {
    /// From a flag, a variable or a default, in the canonical `[a, b]` form.
    FromText(TextItems<'a>, PhantomData<(K, S)>),
    /// From a document, walked by the store that wrote it.
    FromStore(S::Items<'a>),
    /// From a list appended across a stack of layers: the run, and the items
    /// of the layer being walked.
    FromRun(ListRun<'a, S>, Maybe<S::Items<'a>>),
}

impl<K: Kind, S: Store> fmt::Debug for ListValue<'_, K, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListValue::FromText(items, _) => f.debug_tuple("FromText").field(items).finish(),
            ListValue::FromStore(_) => f.debug_tuple("FromStore").finish_non_exhaustive(),
            ListValue::FromRun(run, _) => {
                f.debug_tuple("FromRun").field(run).finish_non_exhaustive()
            },
        }
    }
}

impl<'a, K: Kind, S: Store> Iterator for ListValue<'a, K, S> {
    type Item = Outcome<K::Value<'a, S>, BadValue<'a>>;

    // lint:allow(no-bare-option) reason: `Iterator::next` is core's signature.
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ListValue::FromText(items, _) => items.next().map(K::from_text::<S>),
            ListValue::FromStore(items) => items.next().map(K::from_literal::<S>),
            ListValue::FromRun(run, walking) => {
                loop {
                    if let Maybe::Is(items) = walking
                        && let Some(item) = items.next()
                    {
                        return Some(K::from_literal::<S>(item));
                    }
                    *walking = Maybe::Is(S::items(run.next()?));
                }
            },
        }
    }
}

impl<K: Kind> Kind for List<K> {
    type Value<'a, S: Store> = ListValue<'a, K, S>;

    // lint:allow(no-bare-static-str) reason: a kind's static name; the element kind is named beside it by the row. FIXME: port to Str.
    const NAME: &'static str = "list";
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = false;

    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<Self::Value<'a, S>, BadValue<'a>> {
        let items = match TextItems::over(text) {
            Outcome::Ok(items) => items,
            Outcome::Err(_) => return Outcome::Err(bad::<Self>(Got::Text(text))),
        };
        // A quoted item holding a quote could not be written back, since the
        // canonical form `["a"]` has no escape for one, so it is refused here
        // where the text arrives rather than at the write, which has no way
        // to say so.
        let mut probe = items;
        if K::QUOTED && probe.any(|i| i.contains('"')) {
            return Outcome::Err(bad::<Self>(Got::Text(text)));
        }
        Outcome::Ok(ListValue::FromText(items, PhantomData))
    }

    fn from_literal<'a, S: Store>(
        lit: Literal<'a, S>,
    ) -> Outcome<Self::Value<'a, S>, BadValue<'a>> {
        match lit {
            Literal::List(text) => Outcome::Ok(ListValue::FromStore(S::items(text))),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    fn from_run<'a, S: Store>(run: ListRun<'a, S>) -> Outcome<Self::Value<'a, S>, BadValue<'a>> {
        Outcome::Ok(ListValue::FromRun(run, Maybe::Isnt))
    }

    fn write<S: Store>(value: Self::Value<'_, S>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, item) in value.enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            let Outcome::Ok(item) = item else {
                // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
                return Err(fmt::Error);
            };
            if K::QUOTED {
                f.write_str("\"")?;
                K::write::<S>(item, f)?;
                f.write_str("\"")?;
            } else {
                K::write::<S>(item, f)?;
            }
        }
        f.write_str("]")
    }
}

/// The items of a list in its canonical text form, `[a, "b, c", d]`: split
/// on the commas outside quotes, each item trimmed and unquoted. The brackets
/// are optional, since a flag typed by a person often omits them and a shell
/// has already eaten one layer of quoting.
#[derive(Debug, Clone, Copy)]
pub struct TextItems<'a> {
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    rest: &'a str,
    // lint:allow(no-bare-numeric, arvo-types-only, no-public-raw-field) reason: an iterator's own flag. FIXME: port to arvo's Bool.
    done: bool,
}

impl<'a> TextItems<'a> {
    /// Over `text`, or a refusal where a quote is left open.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    pub fn over(text: &'a str) -> Outcome<Self, ()> {
        let inner = text.trim();
        let inner = match inner.strip_prefix('[') {
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            Some(s) => {
                match s.strip_suffix(']') {
                    // lint:allow(no-bare-option) reason: `str::strip_suffix` is core's and answers in `Option`; matched here.
                    Some(s) => s,
                    // lint:allow(no-bare-option) reason: `str::strip_suffix` is core's and answers in `Option`; matched here.
                    None => return Outcome::Err(()),
                }
            },
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            None => inner,
        };
        if inner.bytes().filter(|b| *b == b'"').count() % 2 != 0 {
            return Outcome::Err(());
        }
        Outcome::Ok(TextItems {
            rest: inner,
            done: inner.trim().is_empty(),
        })
    }
}

impl<'a> Iterator for TextItems<'a> {
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    type Item = &'a str;

    // lint:allow(no-bare-option, no-bare-string) reason: `Iterator::next` is core's signature over a borrowed item.
    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        // lint:allow(no-bare-numeric, arvo-types-only) reason: a scanner's own flag. FIXME: port to arvo's Bool.
        let mut quoted = false;
        let mut end = self.rest.len();
        for (i, b) in self.rest.bytes().enumerate() {
            match b {
                b'"' => quoted = !quoted,
                b',' if !quoted => {
                    end = i;
                    break;
                },
                _ => {},
            }
        }
        let item = &self.rest[.. end];
        if end == self.rest.len() {
            self.done = true;
        } else {
            self.rest = &self.rest[end + 1 ..];
        }
        let item = item.trim();
        let item = match item.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            Some(s) => s,
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            None => item,
        };
        Some(item)
    }
}

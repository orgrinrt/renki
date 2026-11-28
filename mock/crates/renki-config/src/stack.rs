//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A stack of read layers, and a key answered over it by the merge rule.
//!
//! Every `bool` and `&str` here is marked for the port to arvo's `Bool` and
//! `hilavitkutin_str::Str`, which is a later unit; the markers sit above the
//! item they cover, which is where rustfmt leaves them alone.

use core::fmt;

use notko::{Maybe, Outcome};

use crate::meta::is_meta;
use crate::{BadMerge, Literal, Meet, Meta, Part, Shape, Store, Value, meet};

/// One read layer: which part of the manifest it is, a label for the file it
/// came from, its document, and the `meta` table read out of it. A stack is a
/// slice of these, lowest first, which the caller builds.
pub struct Layer<'a, St: Store> {
    part:  Part<'a>,
    // lint:allow(no-bare-string) reason: the caller's label for the file. FIXME: port to Str.
    label: &'a str,
    doc:   &'a St::Document<'a>,
    meta:  Meta<'a, St>,
}

impl<St: Store> Clone for Layer<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Layer<'_, St> {}

impl<St: Store> fmt::Debug for Layer<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Layer")
            .field("part", &self.part)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl<'a, St: Store> Layer<'a, St> {
    /// The layer `part` of a manifest, read from the file `label` names into
    /// `doc`.
    // lint:allow(no-bare-string) reason: the caller's label for the file. FIXME: port to Str.
    pub fn new(part: Part<'a>, label: &'a str, doc: &'a St::Document<'a>) -> Self {
        Layer {
            part,
            label,
            doc,
            meta: Meta::read(doc),
        }
    }

    /// Which part of the manifest it is.
    pub const fn part(&self) -> Part<'a> {
        self.part
    }

    /// The label of the file it came from.
    // lint:allow(no-bare-string) reason: the caller's label for the file. FIXME: port to Str.
    pub const fn label(&self) -> &'a str {
        self.label
    }

    /// Its document.
    pub const fn doc(&self) -> &'a St::Document<'a> {
        self.doc
    }

    /// Its `meta` table.
    pub const fn meta(&self) -> &Meta<'a, St> {
        &self.meta
    }
}

/// A stack of read layers, lowest first, checked whole: every `meta.unset`
/// names a key some lower layer holds, and at every key a layer holds and
/// every table above one, each layer's shape meets the layers' below it. Only
/// [`Stack::new`] makes one, so [`stacked`] and the resolver read no stack
/// that was not checked.
pub struct Stack<'a, St: Store> {
    layers: &'a [Layer<'a, St>],
}

impl<St: Store> Clone for Stack<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Stack<'_, St> {}

impl<St: Store> fmt::Debug for Stack<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.layers).finish()
    }
}

impl<'a, St: Store> Stack<'a, St> {
    /// Check `layers`, lowest first, by asking [`stacked`] every key an
    /// `unset` names, every key a layer holds and every table above one, and
    /// answer the checked stack or the first refusal, lowest layer first.
    pub fn new(layers: &'a [Layer<'a, St>]) -> Outcome<Self, BadMerge<'a>> {
        let stack = Stack {
            layers,
        };
        for layer in layers {
            if let Maybe::Is(Literal::List(list)) = layer.meta.unset() {
                for item in St::items(list) {
                    if let Literal::Str(named) = item
                        && let Outcome::Err(e) = stacked(&stack, named)
                    {
                        return Outcome::Err(e);
                    }
                }
            }
            for key in St::keys(layer.doc).filter(|k| !is_meta(k)) {
                let tables = key.match_indices('.').map(|(at, _)| &key[.. at]);
                for asked in tables.chain(core::iter::once(key)) {
                    if let Outcome::Err(e) = stacked(&stack, asked) {
                        return Outcome::Err(e);
                    }
                }
            }
        }
        Outcome::Ok(stack)
    }

    /// The layers, lowest first.
    pub const fn layers(&self) -> &'a [Layer<'a, St>] {
        self.layers
    }
}

/// Whether the list literal `list` names `key` itself, or, with `within`, a
/// table `key` sits under.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: dotted keys and a predicate over them. FIXME: port to Str and arvo's Bool.
fn names<'l, St: Store>(list: Maybe<Literal<'l>>, key: &str, within: bool) -> Maybe<&'l str> {
    let Maybe::Is(Literal::List(list)) = list else {
        return Maybe::Isnt;
    };
    for item in St::items(list) {
        if let Literal::Str(named) = item {
            let under = key
                .strip_prefix(named)
                .is_some_and(|rest| rest.starts_with('.'));
            if (!within && named == key) || (within && under) {
                return Maybe::Is(named);
            }
        }
    }
    Maybe::Isnt
}

/// A dotted key answered over a checked stack, lowest layer first, by
/// [`meet`]: the highest layer holding a scalar wins; for a list, the answer
/// is the [`ListRun`] of every holding layer's list from the lowest one that
/// the highest `meta.replace` and `meta.unset` leave, and the layer named is
/// the highest that holds it. A table at the key answers nothing, since no
/// setting is a table. This is the one walk of the merge rule: a `meta.unset`
/// naming the key, or a table it sits under, where no lower layer holds what
/// it names, and one shape over another, are refused here, and
/// [`Stack::new`] checks a stack by asking this every key, so over a stack it
/// answered nothing is refused.
// lint:allow(no-bare-string) reason: a dotted key. FIXME: port to Str.
pub fn stacked<'a, St: Store>(
    stack: &Stack<'a, St>,
    key: &'a str,
) -> Outcome<Maybe<(&'a Layer<'a, St>, Value<'a, St>)>, BadMerge<'a>> {
    let stack = stack.layers;
    // the layer that last set the key, with its shape, and where a list's run starts
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a position in the stack. FIXME: port to arvo.
    let mut held: Maybe<(usize, Shape)> = Maybe::Isnt;
    let mut from = 0;
    for (i, layer) in stack.iter().enumerate() {
        for within in [false, true] {
            if let Maybe::Is(named) = names::<St>(layer.meta.unset(), key, within) {
                if !stack[.. i].iter().any(|l| St::shape(l.doc, named).is()) {
                    return Outcome::Err(BadMerge::Unset(named));
                }
                held = Maybe::Isnt;
            }
        }
        if names::<St>(layer.meta.replace(), key, true).is() {
            held = Maybe::Isnt;
        }
        let Maybe::Is(higher) = St::shape(layer.doc, key) else {
            continue;
        };
        let replaced = names::<St>(layer.meta.replace(), key, false).is();
        let meets = match held {
            Maybe::Isnt => Meet::Over,
            Maybe::Is((_, lower)) => {
                match meet(lower, higher, replaced) {
                    Outcome::Ok(m) => m,
                    Outcome::Err(e) => return Outcome::Err(e),
                }
            },
        };
        if matches!(meets, Meet::Over) {
            from = i;
        }
        held = Maybe::Is((i, higher));
    }
    let Maybe::Is((at, shape)) = held else {
        return Outcome::Ok(Maybe::Isnt);
    };
    let layer = &stack[at];
    let value = match shape {
        Shape::Table => return Outcome::Ok(Maybe::Isnt),
        Shape::List | Shape::Tables | Shape::Empty => {
            Value::Run(ListRun {
                rest: &stack[from ..],
                key,
            })
        },
        Shape::Scalar => {
            match St::get(layer.doc, key) {
                Maybe::Is(l) => Value::Stored(l),
                Maybe::Isnt => return Outcome::Ok(Maybe::Isnt),
            }
        },
    };
    Outcome::Ok(Maybe::Is((layer, value)))
}

/// A list appended across a stack: each holding layer's list in turn, from
/// the lowest the merge leaves, as the list's own text in the store's syntax.
/// A kind walks each through [`Store::items`], so the appended list is
/// neither copied nor allocated.
pub struct ListRun<'a, St: Store> {
    rest: &'a [Layer<'a, St>],
    // lint:allow(no-bare-string) reason: a dotted key. FIXME: port to Str.
    key:  &'a str,
}

impl<St: Store> Clone for ListRun<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for ListRun<'_, St> {}

impl<St: Store> PartialEq for ListRun<'_, St> {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: `PartialEq::eq` is core's signature.
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.rest, other.rest) && self.key == other.key
    }
}

impl<St: Store> fmt::Debug for ListRun<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListRun")
            .field("key", &self.key)
            .field("layers", &self.rest.len())
            .finish()
    }
}

impl<'a, St: Store> Iterator for ListRun<'a, St> {
    // lint:allow(no-bare-string) reason: the list's own text. FIXME: port to Str.
    type Item = &'a str;

    // lint:allow(no-bare-option, no-bare-string) reason: `Iterator::next` is core's signature.
    fn next(&mut self) -> Option<&'a str> {
        while let [first, tail @ ..] = self.rest {
            self.rest = tail;
            if let Maybe::Is(Literal::List(text)) = St::get(first.doc, self.key) {
                return Some(text);
            }
        }
        None
    }
}

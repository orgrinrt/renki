//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A walk over what a store hands out, one at a time.

use notko::Maybe;

/// A walk over a store's keys or a list's items, the next one each step, or
/// `Isnt` once there are no more.
pub trait Walk {
    /// What each step hands out.
    type Item;
    /// The next one, or `Isnt` at the end.
    fn step(&mut self) -> Maybe<Self::Item>;
}

/// Whether some step of `walk` answers `f` true; the walk stops at the first.
pub(crate) fn any<W: Walk>(mut walk: W, mut f: impl FnMut(W::Item) -> bool) -> bool {
    while let Maybe::Is(item) = walk.step() {
        if f(item) {
            return true;
        }
    }
    false
}

/// The first step of `walk` that `f` answers true for.
pub(crate) fn find<W: Walk>(mut walk: W, mut f: impl FnMut(&W::Item) -> bool) -> Maybe<W::Item> {
    while let Maybe::Is(item) = walk.step() {
        if f(&item) {
            return Maybe::Is(item);
        }
    }
    Maybe::Isnt
}

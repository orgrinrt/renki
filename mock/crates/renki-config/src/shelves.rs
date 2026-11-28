//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The memory a store is given, and its refusal when that falls short.
//!
//! This module is the one place the crate names how memory is lent: a store
//! asks `Shelves` for its two shelves, and another lender is another
//! implementation of `Shelves`.

use core::fmt;

use notko::lend::{Exhausted, Lend};

/// The two shelves a store parses a document into: its index, `C` slots, and
/// the spelling shelf, the bytes it writes what it hands out into that is no
/// text of the document as written. One implementation hands out both at
/// once, so a store is never given one without the other.
pub trait Shelves<C> {
    /// The index and the spelling shelf, both for as long as `self` is lent.
    fn shelves(&mut self) -> (&mut [C], &mut [u8]);
}

impl<C, I: Lend<C> + ?Sized, B: Lend<u8> + ?Sized> Shelves<C> for (&mut I, &mut B) {
    fn shelves(&mut self) -> (&mut [C], &mut [u8]) {
        (self.0.lend(), self.1.lend())
    }
}

/// Which of a store's two shelves fell short.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreShelf {
    /// The index of cells.
    Index,
    /// The spelling shelf of bytes.
    Spelling,
}

impl fmt::Display for StoreShelf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StoreShelf::Index => "index",
            StoreShelf::Spelling => "spelling",
        })
    }
}

/// A shelf a store was given that fell short: which one, and what it gave,
/// how much it held and how much the read wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortShelf {
    shelf:     StoreShelf,
    exhausted: Exhausted,
}

impl ShortShelf {
    /// `shelf` fell short as `exhausted` says.
    pub const fn new(shelf: StoreShelf, exhausted: Exhausted) -> Self {
        ShortShelf {
            shelf,
            exhausted,
        }
    }

    /// Which shelf fell short.
    pub const fn shelf(&self) -> StoreShelf {
        self.shelf
    }

    /// What it held and what the read wanted.
    pub const fn exhausted(&self) -> Exhausted {
        self.exhausted
    }
}

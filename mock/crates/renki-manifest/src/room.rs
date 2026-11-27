//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The memory a read uses, the caller's.

use core::ffi::CStr;

use tomlfuse_parse::Node;

use crate::{BadRead, Host};

/// The memory a read uses: `TEXT` bytes for the text of every layer read,
/// back to back, and `NODES` nodes for the index the parser keeps while it
/// reads.
///
/// It is the caller's, on the stack, in a static or in memory the caller
/// allocated, and every string a read hands out borrows from it. A file that
/// does not fit is refused with [`BadRead::Room`], which names the `TEXT` a
/// room needs to read it. [`tomlfuse_parse::nodes_for`] of `TEXT` is a
/// `NODES` that never runs out.
pub struct Room<const TEXT: usize, const NODES: usize> {
    text:  [u8; TEXT],
    nodes: [Node; NODES],
}

impl<const TEXT: usize, const NODES: usize> Room<TEXT, NODES> {
    /// An empty room.
    pub const fn new() -> Self {
        Room {
            text:  [0; TEXT],
            nodes: [Node::EMPTY; NODES],
        }
    }

    /// The room as its two halves: the text, read into from the front, and
    /// the parser's index.
    pub(crate) fn parts(&mut self) -> (Shelf<'_>, &mut [Node]) {
        (
            Shelf {
                rest: &mut self.text,
                used: 0,
            },
            &mut self.nodes,
        )
    }
}

impl<const TEXT: usize, const NODES: usize> Default for Room<TEXT, NODES> {
    fn default() -> Self {
        Self::new()
    }
}

/// The text half of a room: what is not read into yet, and how much is.
pub(crate) struct Shelf<'r> {
    rest: &'r mut [u8],
    used: usize,
}

impl<'r> Shelf<'r> {
    /// Reads the file at `path` after the text already read, and hands it out
    /// for as long as the room lives. A refused read takes nothing of the
    /// room.
    pub(crate) fn read<'p, H: Host>(&mut self, path: &'p CStr) -> Result<&'r str, BadRead<'p>> {
        let rest = core::mem::take(&mut self.rest);
        let n = match H::read(path, rest) {
            Ok(n) => n,
            Err(BadRead::Room {
                file,
                need,
            }) => {
                self.rest = rest;
                // the room holds what is read already, and this file after it
                return Err(BadRead::Room {
                    file,
                    need: self.used + need,
                });
            },
            Err(refused) => {
                self.rest = rest;
                return Err(refused);
            },
        };
        let (got, rest) = rest.split_at_mut(n);
        self.rest = rest;
        self.used += n;
        let got: &'r [u8] = got;
        core::str::from_utf8(got).map_err(|e| {
            BadRead::NotText {
                file: path,
                at:   e.valid_up_to(),
            }
        })
    }
}

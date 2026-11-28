//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The memory a read uses: four shelves the caller lends.

use core::fmt;

use notko::lend::Lend;
use tomlfuse_parse::Node;

/// One of the room's four shelves, as a refusal names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// The text of every layer read, back to back.
    Text,
    /// Where the store writes each string it decodes and each key it spells
    /// whole.
    Spelling,
    /// The nodes of the parser's index, which is each document as it was read.
    Nodes,
    /// The base's path and each joined include path, each ending in a NUL.
    Path,
}

impl fmt::Display for Shelf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Shelf::Text => "text",
            Shelf::Spelling => "spelling",
            Shelf::Nodes => "node",
            Shelf::Path => "path",
        })
    }
}

/// The memory a read uses, four shelves the caller lends: the text, the
/// spelling shelf, the parser's nodes and the paths. Every string a read hands out
/// borrows from it.
#[derive(Debug)]
pub struct Room<'r> {
    pub(crate) text:     &'r mut [u8],
    pub(crate) spelling: &'r mut [u8],
    pub(crate) nodes:    &'r mut [Node],
    pub(crate) path:     &'r mut [u8],
}

impl<'r> Room<'r> {
    /// A room over what each lend gives: an array, a slice or storage of the
    /// caller's own. The nodes are set to [`Node::EMPTY`] here, so a lend of
    /// any nodes will do.
    pub fn new<T, D, N, P>(
        text: &'r mut T,
        spelling: &'r mut D,
        nodes: &'r mut N,
        path: &'r mut P,
    ) -> Self
    where
        T: Lend<u8> + ?Sized,
        D: Lend<u8> + ?Sized,
        N: Lend<Node> + ?Sized,
        P: Lend<u8> + ?Sized,
    {
        let nodes = nodes.lend();
        nodes.fill(Node::EMPTY);
        Room {
            text: text.lend(),
            spelling: spelling.lend(),
            nodes,
            path: path.lend(),
        }
    }
}

/// How long the path shelf of a [`Rooms`] is: room for a base, its includes
/// and the longest path the C library resolves, which is 4096 bytes on linux.
const PATHS: usize = 16 * 1024;

/// The four shelves owned in fixed arrays, on the stack or in a static, for a
/// caller with no allocator: `TEXT` bytes of text, `NODES` nodes, `SPELLING`
/// bytes of spelling, and 16 KiB of paths.
#[derive(Debug)]
pub struct Rooms<const TEXT: usize, const NODES: usize, const SPELLING: usize> {
    text:     [u8; TEXT],
    spelling: [u8; SPELLING],
    nodes:    [Node; NODES],
    path:     [u8; PATHS],
}

impl<const TEXT: usize, const NODES: usize, const SPELLING: usize> Rooms<TEXT, NODES, SPELLING> {
    /// Every shelf empty.
    pub const fn new() -> Self {
        Rooms {
            text:     [0; TEXT],
            spelling: [0; SPELLING],
            nodes:    [Node::EMPTY; NODES],
            path:     [0; PATHS],
        }
    }

    /// The room over the four arrays.
    pub fn room(&mut self) -> Room<'_> {
        Room::new(
            &mut self.text,
            &mut self.spelling,
            &mut self.nodes,
            &mut self.path,
        )
    }
}

impl<const TEXT: usize, const NODES: usize, const SPELLING: usize> Default
    for Rooms<TEXT, NODES, SPELLING>
{
    fn default() -> Self {
        Self::new()
    }
}

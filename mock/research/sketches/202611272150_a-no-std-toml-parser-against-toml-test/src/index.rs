//! The index of what a document has defined so far, in a slice the caller
//! gives: one node per table and per key, linked parent to children.

use crate::key::Key;
use crate::{Error, ErrorKind};

pub(crate) const NONE: u32 = u32::MAX;

/// How a table or key came to be, which decides what may still extend it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Made on the way to a header further down; one header may define it.
    Implicit,
    /// Defined by its own `[header]`.
    Header,
    /// Made by a dotted key; more dotted keys of the same table extend it.
    Dotted,
    /// A key given a value: a scalar, a static array or an inline table.
    /// Nothing extends it.
    Value,
    /// `[[header]]`; a header through it reaches its last element.
    ArrayOfTables,
    /// One table of an array of tables, or an inline table in a static
    /// array. Keyless, and linked from no parent.
    Element,
}

/// One slot of the index. The caller makes the slice, as many as
/// [`crate::nodes_for`] of the text's length, filled with [`Node::EMPTY`].
#[derive(Clone, Copy, Debug)]
pub struct Node {
    first_child:  u32,
    next_sibling: u32,
    last_element: u32,
    key_start:    u32,
    key_end:      u32,
    hash:         u32,
    kind:         Kind,
}

impl Node {
    /// An unused slot.
    pub const EMPTY: Node = Node {
        first_child:  NONE,
        next_sibling: NONE,
        last_element: NONE,
        key_start:    0,
        key_end:      0,
        hash:         0,
        kind:         Kind::Value,
    };
}

pub(crate) struct Index<'s, 'n> {
    src:        &'s str,
    nodes:      &'n mut [Node],
    len:        u32,
    peak:       u32,
    root_first: u32,
}

impl<'s, 'n> Index<'s, 'n> {
    pub(crate) fn new(src: &'s str, nodes: &'n mut [Node]) -> Self {
        Self {
            src,
            nodes,
            len: 0,
            peak: 0,
            root_first: NONE,
        }
    }

    pub(crate) const fn len(&self) -> u32 {
        self.len
    }

    /// The most nodes held at once.
    pub(crate) const fn peak(&self) -> u32 {
        self.peak
    }

    pub(crate) fn kind(&self, n: u32) -> Kind {
        self.nodes[n as usize].kind
    }

    pub(crate) fn set_kind(&mut self, n: u32, kind: Kind) {
        self.nodes[n as usize].kind = kind;
    }

    pub(crate) fn last_element(&self, n: u32) -> u32 {
        self.nodes[n as usize].last_element
    }

    fn first(&self, parent: u32) -> u32 {
        if parent == NONE {
            self.root_first
        } else {
            self.nodes[parent as usize].first_child
        }
    }

    /// The child of `parent` (`NONE` for the root) named `key`.
    pub(crate) fn find(&self, parent: u32, key: &Key<'_>, hash: u32) -> Option<u32> {
        let mut n = self.first(parent);
        while n != NONE {
            let node = &self.nodes[n as usize];
            if node.hash == hash {
                let name = Key::new(&self.src[node.key_start as usize .. node.key_end as usize]);
                if name.same_name(key) {
                    return Some(n);
                }
            }
            n = node.next_sibling;
        }
        None
    }

    fn push(&mut self, node: Node, at: usize) -> Result<u32, Error> {
        let n = self.len;
        let slot = self.nodes.get_mut(n as usize).ok_or(Error {
            kind: ErrorKind::OutOfNodes,
            at,
        })?;
        *slot = node;
        self.len += 1;
        self.peak = self.peak.max(self.len);
        Ok(n)
    }

    /// A new child of `parent` named `key`, where `key` lies in the text.
    pub(crate) fn child(
        &mut self,
        parent: u32,
        key: &Key<'s>,
        hash: u32,
        kind: Kind,
    ) -> Result<u32, Error> {
        let start = key.raw().as_ptr() as usize - self.src.as_ptr() as usize;
        let node = Node {
            next_sibling: self.first(parent),
            key_start: start as u32,
            key_end: (start + key.raw().len()) as u32,
            hash,
            kind,
            ..Node::EMPTY
        };
        let n = self.push(node, start)?;
        if parent == NONE {
            self.root_first = n;
        } else {
            self.nodes[parent as usize].first_child = n;
        }
        Ok(n)
    }

    /// A new keyless element; of the array of tables `array`, when given.
    pub(crate) fn element(&mut self, array: Option<u32>, at: usize) -> Result<u32, Error> {
        let n = self.push(
            Node {
                kind: Kind::Element,
                ..Node::EMPTY
            },
            at,
        )?;
        if let Some(a) = array {
            self.nodes[a as usize].last_element = n;
        }
        Ok(n)
    }

    /// Forgets every node from `mark` on, and the children of `owner`. Only
    /// the inside of a value closed for good is ever forgotten, so nothing
    /// before `mark` but `owner` links past it.
    pub(crate) fn forget(&mut self, mark: u32, owner: Option<u32>) {
        self.len = mark;
        if let Some(o) = owner {
            self.nodes[o as usize].first_child = NONE;
        }
    }
}

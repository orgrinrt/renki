//! Keys and dotted key paths, as spans of the text.

use crate::string::{self, Str, StrKind};
use crate::{ErrorKind, Version};

/// One key of a path: bare, `"basic"` or `'literal'`, as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key<'s> {
    raw: &'s str,
}

impl<'s> Key<'s> {
    pub(crate) const fn new(raw: &'s str) -> Self {
        Self {
            raw,
        }
    }

    /// The key as written, quotes included.
    pub const fn raw(&self) -> &'s str {
        self.raw
    }

    /// The key's name, as a string to decode.
    pub fn name(&self) -> Str<'s> {
        let r = self.raw;
        match r.as_bytes().first() {
            Some(b'"') => Str::new(&r[1 .. r.len() - 1], StrKind::Basic),
            Some(b'\'') => Str::new(&r[1 .. r.len() - 1], StrKind::Literal),
            _ => Str::new(r, StrKind::Literal),
        }
    }

    /// Two keys name the same thing when their names decode the same.
    pub fn same_name(&self, other: &Key<'_>) -> bool {
        self.raw == other.raw || self.name().eq_decoded(&other.name())
    }

    /// A 32-bit FNV-1a hash of the decoded name, for a quick refusal.
    pub fn hash(&self) -> u32 {
        let mut h: u32 = 0x811C_9DC5;
        let mut buf = [0u8; 4];
        for c in self.name().chars() {
            for byte in c.encode_utf8(&mut buf).as_bytes() {
                h ^= u32::from(*byte);
                h = h.wrapping_mul(0x0100_0193);
            }
        }
        h
    }
}

/// A dotted key path as written, already validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPath<'s> {
    raw: &'s str,
}

impl<'s> KeyPath<'s> {
    /// The path as written, dots and spaces included.
    pub const fn raw(&self) -> &'s str {
        self.raw
    }

    /// Its keys, outermost first.
    pub const fn keys(&self) -> Keys<'s> {
        Keys {
            rest: self.raw,
        }
    }
}

/// The keys of a [`KeyPath`], outermost first.
#[derive(Clone, Debug)]
pub struct Keys<'s> {
    rest: &'s str,
}

impl<'s> Iterator for Keys<'s> {
    type Item = Key<'s>;

    fn next(&mut self) -> Option<Key<'s>> {
        let r = self.rest.trim_start_matches([' ', '\t', '.']);
        let b = r.as_bytes();
        let end = match b.first()? {
            q @ (b'"' | b'\'') => {
                let mut i = 1;
                while b[i] != *q {
                    i += if *q == b'"' && b[i] == b'\\' { 2 } else { 1 };
                }
                i + 1
            },
            _ => b.iter().position(|c| !bare(*c)).unwrap_or(b.len()),
        };
        let (key, rest) = r.split_at(end);
        self.rest = rest;
        Some(Key::new(key))
    }
}

pub(crate) fn bare(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

/// Reads a key path starting at `at`; gives the path and the offset after
/// its last key.
pub(crate) fn path(
    s: &str,
    at: usize,
    v: Version,
) -> Result<(KeyPath<'_>, usize), (ErrorKind, usize)> {
    let b = s.as_bytes();
    let mut i = at;
    loop {
        i = match b.get(i) {
            Some(b'"') => string::basic(s, i, v)?.1,
            Some(b'\'') => string::literal(s, i)?.1,
            Some(&c) if bare(c) => {
                let mut j = i;
                while b.get(j).is_some_and(|c| bare(*c)) {
                    j += 1;
                }
                j
            },
            _ => return Err((ErrorKind::Expected("key"), i)),
        };
        let mut j = i;
        while matches!(b.get(j), Some(b' ' | b'\t')) {
            j += 1;
        }
        if b.get(j) != Some(&b'.') {
            return Ok((
                KeyPath {
                    raw: &s[at .. i],
                },
                i,
            ));
        }
        j += 1;
        while matches!(b.get(j), Some(b' ' | b'\t')) {
            j += 1;
        }
        i = j;
    }
}

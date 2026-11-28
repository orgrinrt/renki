//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The TOML store: `renki-config`'s `Store` over `tomlfuse-parse`'s document.
//!
//! A document is the parser's index over the nodes it was given and what the
//! store spelled into its spelling shelf: every string with an escape decoded
//! at the place its source starts, so a string's decoding is found again from
//! its span, and after them every key the document holds, dotted whole, one a
//! line. Nothing is kept anywhere else.

use core::fmt;
use core::ops::Range;

use notko::lend::Exhausted;
use notko::{Maybe, Outcome};
use renki_config::{
    BadDocument,
    Literal,
    Rendered,
    Shape,
    Shelves,
    ShortShelf,
    Store,
    StoreShelf,
    Walk,
};
use tomlfuse_parse::{Document, Entry, ErrorKind, Holds, Node, Options, Scalar, Str, Version};

/// The TOML store, reading TOML 1.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toml;

/// A TOML document as the store read it.
#[derive(Debug, Clone, Copy)]
pub struct TomlDocument<'t> {
    doc:     Document<'t, 't>,
    /// Each escaped string decoded at the place its source starts.
    decoded: &'t [u8],
    /// Every key the document holds, dotted, one a line.
    keys:    &'t str,
}

/// The store's handle on a list a document holds: an array, or an array of
/// tables, by its entry.
#[derive(Debug, Clone, Copy)]
pub struct TomlList<'d> {
    entry:   Entry<'d, 'd>,
    decoded: &'d [u8],
}

impl PartialEq for TomlList<'_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.entry.document().text(), other.entry.document().text())
            && self.entry.value_span() == other.entry.value_span()
            && self.entry.key_span() == other.entry.key_span()
    }
}

impl Eq for TomlList<'_> {}

/// How a document was refused: a shelf that fell short, or the parser's
/// refusal at a byte of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unread {
    /// A shelf fell short.
    Short(ShortShelf),
    /// The parser refused the text: what it said, and the line, from 1.
    Refused(ErrorKind, usize),
}

const fn short(shelf: StoreShelf, wanted: usize, had: usize) -> ShortShelf {
    ShortShelf::new(shelf, Exhausted {
        wanted,
        had,
    })
}

/// The line of the text the byte `at` stands on, from 1.
fn line_of(text: &str, at: usize) -> usize {
    1 + text.as_bytes()[.. at.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

/// Whether a key segment is written bare.
fn bare(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// What `f` is handed for each key the document holds, dotted: every scalar
/// and every list, each array of tables' elements by their index, in the
/// text's order.
fn each_key(doc: &Document<'_, '_>, f: &mut impl FnMut(&[Seg<'_>])) {
    let mut path: [Seg<'_>; DEPTH] = [Seg::Index(0); DEPTH];
    walk_keys(doc.root(), &mut path, 0, f);
}

/// How deep a key a document holds may be, its tables and indexes counted.
const DEPTH: usize = 64;

#[derive(Debug, Clone, Copy)]
enum Seg<'s> {
    Name(Str<'s>),
    Index(usize),
}

fn walk_keys<'s>(
    at: Entry<'s, '_>,
    path: &mut [Seg<'s>; DEPTH],
    depth: usize,
    f: &mut impl FnMut(&[Seg<'_>]),
) {
    if depth >= DEPTH {
        return;
    }
    for (i, child) in at.children().enumerate() {
        path[depth] = match child.key() {
            Some(k) => Seg::Name(k.name()),
            None => Seg::Index(i),
        };
        match child.holds() {
            Holds::Table | Holds::InlineTable => walk_keys(child, path, depth + 1, f),
            Holds::ArrayOfTables => {
                if child.children().next().is_none() {
                    f(&path[..= depth]);
                }
                walk_keys(child, path, depth + 1, f)
            },
            Holds::Array | Holds::Scalar => f(&path[..= depth]),
        }
    }
}

/// The length of a key dotted, its names decoded.
fn dotted_len(path: &[Seg<'_>]) -> usize {
    let mut n = path.len().saturating_sub(1);
    for seg in path {
        n += match seg {
            Seg::Name(s) => s.decoded_len(),
            Seg::Index(i) => digits(*i),
        };
    }
    n
}

fn digits(mut i: usize) -> usize {
    let mut n = 1;
    while i >= 10 {
        i /= 10;
        n += 1;
    }
    n
}

/// A key dotted into `into` from `at`; the offset after it.
fn spell(path: &[Seg<'_>], into: &mut [u8], mut at: usize) -> usize {
    for (n, seg) in path.iter().enumerate() {
        if n > 0 {
            into[at] = b'.';
            at += 1;
        }
        match seg {
            Seg::Name(s) => {
                for c in s.chars() {
                    at += c.encode_utf8(&mut into[at ..]).len();
                }
            },
            Seg::Index(i) => {
                let len = digits(*i);
                let mut v = *i;
                for k in (0 .. len).rev() {
                    into[at + k] = b'0' + (v % 10) as u8;
                    v /= 10;
                }
                at += len;
            },
        }
    }
    at
}

/// The escaped strings of the document, as each one's span of the text.
fn each_escaped(at: Entry<'_, '_>, f: &mut impl FnMut(Str<'_>, Range<usize>)) {
    for child in at.children() {
        match child.holds() {
            Holds::Scalar => {
                if let (Some(Scalar::String(s)), Some(span)) = (child.scalar(), child.value_span())
                    && s.as_borrowed().is_none()
                {
                    f(s, span);
                }
            },
            _ => each_escaped(child, f),
        }
    }
}

impl Toml {
    /// Read `text` into the shelves given, or say why it was not read: a
    /// shelf that fell short with the whole need of the spelling shelf and
    /// the parser's number for the index, or the parser's refusal with its
    /// line. A caller wanting the parser's words, as a refusal writes them,
    /// reads through this; `Store::parse` gives only the line.
    pub fn read<'t, S: Shelves<Node> + ?Sized>(
        text: &'t str,
        shelves: &'t mut S,
    ) -> Outcome<TomlDocument<'t>, Unread> {
        let (nodes, spelling) = shelves.shelves();
        let doc = match tomlfuse_parse::parse(
            text.as_bytes(),
            Options::new(Version::V1_1_0),
            nodes,
            &mut (),
        ) {
            Ok(doc) => doc,
            Err(e) => {
                return Outcome::Err(match e.kind {
                    ErrorKind::Exhausted(x) => Unread::Short(ShortShelf::new(StoreShelf::Index, x)),
                    kind => Unread::Refused(kind, line_of(text, e.at)),
                });
            },
        };
        // the whole need: the decoded strings at their places, then the keys
        let mut region = 0;
        each_escaped(doc.root(), &mut |_, span| region = region.max(span.end));
        let mut keys = 0;
        each_key(&doc, &mut |path| keys += dotted_len(path) + 1);
        let need = region + keys;
        if need > spelling.len() {
            return Outcome::Err(Unread::Short(short(
                StoreShelf::Spelling,
                need,
                spelling.len(),
            )));
        }
        let spelling: &'t mut [u8] = spelling;
        each_escaped(doc.root(), &mut |s, span| {
            let mut at = span.start;
            for c in s.chars() {
                at += c.encode_utf8(&mut spelling[at ..]).len();
            }
        });
        let mut at = region;
        each_key(&doc, &mut |path| {
            at = spell(path, spelling, at);
            spelling[at] = b'\n';
            at += 1;
        });
        let (decoded, keys) = spelling.split_at(region);
        let keys = core::str::from_utf8(&keys[.. need - region]).unwrap_or_default();
        Outcome::Ok(TomlDocument {
            doc,
            decoded,
            keys,
        })
    }

    /// The parser's document under a store document, for a reader that
    /// walks its entries itself, as the dump does.
    pub const fn document<'t>(doc: &TomlDocument<'t>) -> Document<'t, 't> {
        doc.doc
    }

    /// The entry a dotted key names: each segment a name, or an index into
    /// an array of tables or an array.
    pub fn entry<'d>(doc: &'d TomlDocument<'_>, key: &str) -> Maybe<Entry<'d, 'd>> {
        let mut at = doc.doc.root();
        for seg in key.split('.') {
            let next = match at.holds() {
                Holds::ArrayOfTables | Holds::Array => {
                    match seg.parse::<usize>() {
                        Ok(i) => at.children().nth(i),
                        Err(_) => None,
                    }
                },
                _ => at.child(seg),
            };
            match next {
                Some(e) => at = e,
                None => return Maybe::Isnt,
            }
        }
        Maybe::Is(at)
    }

    /// The text a string value lends: its source where decoding changes
    /// nothing, else its decoding in the spelling shelf.
    fn string<'d>(s: Str<'d>, span: Range<usize>, decoded: &'d [u8]) -> &'d str {
        if let Some(b) = s.as_borrowed() {
            return b;
        }
        let len = s.decoded_len();
        match decoded.get(span.start .. span.start + len) {
            Some(bytes) => core::str::from_utf8(bytes).unwrap_or_default(),
            None => "",
        }
    }

    fn literal<'d>(e: Entry<'d, 'd>, decoded: &'d [u8]) -> Maybe<Literal<'d, Toml>> {
        match e.holds() {
            Holds::Array | Holds::ArrayOfTables => {
                Maybe::Is(Literal::List(TomlList {
                    entry: e,
                    decoded,
                }))
            },
            Holds::Scalar => {
                match (e.scalar(), e.value_span()) {
                    (Some(Scalar::String(s)), Some(span)) => {
                        Maybe::Is(Literal::Str(Self::string(s, span, decoded)))
                    },
                    (Some(Scalar::Integer(i)), _) => Maybe::Is(Literal::Int(i)),
                    (Some(Scalar::Boolean(b)), _) => Maybe::Is(Literal::Bool(b)),
                    // a float or a date and time is read by no kind; its text stands for it
                    (Some(_), _) => {
                        match e.value_text() {
                            Some(t) => Maybe::Is(Literal::Str(t)),
                            None => Maybe::Isnt,
                        }
                    },
                    (None, _) => Maybe::Isnt,
                }
            },
            Holds::Table | Holds::InlineTable => Maybe::Isnt,
        }
    }
}

/// The keys a TOML document holds, one a step.
#[derive(Debug, Clone)]
pub struct TomlKeys<'d> {
    rest: &'d str,
}

impl<'d> Walk for TomlKeys<'d> {
    type Item = &'d str;

    fn step(&mut self) -> Maybe<&'d str> {
        let Some((key, rest)) = self.rest.split_once('\n') else {
            return Maybe::Isnt;
        };
        self.rest = rest;
        Maybe::Is(key)
    }
}

/// The items of a TOML list, one a step; an inline table among them, and
/// each table of an array of tables, is no value a kind reads, and is passed
/// over.
#[derive(Debug, Clone)]
pub struct TomlItems<'d> {
    items:   tomlfuse_parse::Children<'d, 'd>,
    decoded: &'d [u8],
}

impl<'d> Walk for TomlItems<'d> {
    type Item = Literal<'d, Toml>;

    fn step(&mut self) -> Maybe<Literal<'d, Toml>> {
        for item in self.items.by_ref() {
            if let Maybe::Is(l) = Toml::literal(item, self.decoded) {
                return Maybe::Is(l);
            }
        }
        Maybe::Isnt
    }
}

impl Store for Toml {
    type Cell = Node;
    type Document<'t> = TomlDocument<'t>;
    type Items<'d> = TomlItems<'d>;
    type Keys<'d> = TomlKeys<'d>;
    type List<'d> = TomlList<'d>;

    fn name() -> &'static str {
        "toml"
    }

    fn extension() -> &'static str {
        "toml"
    }

    fn parse<'t, S: Shelves<Node> + ?Sized>(
        text: &'t str,
        shelves: &'t mut S,
    ) -> Outcome<Outcome<TomlDocument<'t>, BadDocument>, ShortShelf> {
        match Self::read(text, shelves) {
            Outcome::Ok(d) => Outcome::Ok(Outcome::Ok(d)),
            Outcome::Err(Unread::Short(s)) => Outcome::Err(s),
            Outcome::Err(Unread::Refused(_, line)) => {
                let line = u32::try_from(line).unwrap_or(u32::MAX);
                Outcome::Ok(Outcome::Err(BadDocument::at(Maybe::Is(line))))
            },
        }
    }

    fn get<'d>(doc: &'d TomlDocument<'_>, key: &str) -> Maybe<Literal<'d, Toml>> {
        match Self::entry(doc, key) {
            Maybe::Is(e) => Self::literal(e, doc.decoded),
            Maybe::Isnt => Maybe::Isnt,
        }
    }

    fn keys<'d>(doc: &'d TomlDocument<'_>) -> TomlKeys<'d> {
        TomlKeys {
            rest: doc.keys,
        }
    }

    fn items<'d>(list: Self::List<'d>) -> TomlItems<'d> {
        TomlItems {
            items:   list.entry.children(),
            decoded: list.decoded,
        }
    }

    fn shape(doc: &TomlDocument<'_>, key: &str) -> Maybe<Shape> {
        let Maybe::Is(e) = Self::entry(doc, key) else {
            return Maybe::Isnt;
        };
        Maybe::Is(match e.holds() {
            Holds::Table | Holds::InlineTable => Shape::Table,
            Holds::Scalar => Shape::Scalar,
            Holds::ArrayOfTables => Shape::Tables,
            Holds::Array => {
                let mut items = e.children().peekable();
                if items.peek().is_none() {
                    Shape::Empty
                } else if items.all(|i| i.holds() == Holds::InlineTable) {
                    Shape::Tables
                } else {
                    Shape::List
                }
            },
        })
    }

    fn set(
        doc: &TomlDocument<'_>,
        key: &str,
        value: Rendered<'_>,
        into: &mut impl fmt::Write,
    ) -> fmt::Result {
        crate::writer::set(doc, key, value, into)
    }
}

/// `name` as a key segment: bare where it can be, else a basic string.
pub(crate) fn write_segment(name: &str, into: &mut impl fmt::Write) -> fmt::Result {
    if bare(name) {
        return into.write_str(name);
    }
    write_basic(name, into)
}

/// `text` as a TOML basic string: in double quotes, with `\"`, `\\`, `\n`,
/// `\t` and `\r`, and every other byte below 0x20 and 0x7f as `\u00XX` in
/// upper case.
pub(crate) fn write_basic(text: &str, into: &mut impl fmt::Write) -> fmt::Result {
    into.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => into.write_str("\\\"")?,
            '\\' => into.write_str("\\\\")?,
            '\n' => into.write_str("\\n")?,
            '\t' => into.write_str("\\t")?,
            '\r' => into.write_str("\\r")?,
            c if c < ' ' || c == '\u{7f}' => write!(into, "\\u{:04X}", c as u32)?,
            c => into.write_char(c)?,
        }
    }
    into.write_char('"')
}

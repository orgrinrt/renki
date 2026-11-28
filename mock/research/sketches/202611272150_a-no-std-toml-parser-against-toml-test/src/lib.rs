//! A TOML reader that is `no_std`, allocates nothing and builds on stable.
//!
//! It reads one document from bytes the caller holds and hands what it reads
//! to a [`Visitor`] as a stream of events. Keys and strings come out as spans
//! of the caller's text and are decoded only when asked. The checks that need
//! memory of what came before (a key or a table defined twice, a dotted key
//! or an inline table reopened) use an index of [`Node`]s in a slice the
//! caller gives.

#![no_std]
#![forbid(unsafe_code)]

mod datetime;
mod index;
mod key;
mod number;
mod parser;
mod string;

pub use datetime::{Date, Datetime, Offset, Time};
pub use index::Node;
pub use key::{Key, KeyPath, Keys};
pub use string::{Str, StrChars, StrKind};

/// The version of the TOML specification a document is read against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Version {
    /// TOML 1.0.0.
    V1_0_0,
    /// TOML 1.1.0: newlines, comments and a trailing comma in inline tables,
    /// the `\e` and `\xHH` escapes, and times without seconds.
    V1_1_0,
}

/// How a document is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The specification the document is read against.
    pub version:   Version,
    /// How deep arrays and inline tables may nest. Each level is one frame
    /// of the reader's own recursion on the call stack.
    pub max_depth: u32,
}

impl Options {
    /// Reading against `version`, with nesting bounded at 128.
    pub const fn new(version: Version) -> Self {
        Self {
            version,
            max_depth: 128,
        }
    }
}

/// A value that holds no other value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scalar<'s> {
    /// A string, still a span of the text; see [`Str`].
    String(Str<'s>),
    /// An integer, decoded.
    Integer(i64),
    /// A float, decoded.
    Float(f64),
    /// A boolean.
    Boolean(bool),
    /// An offset or local date-time, a local date or a local time, decoded.
    Datetime(Datetime),
}

/// What a document says, in the order it says it.
///
/// A key path is always relative: a header's to the root, a key/value
/// pair's to the table it sits in (the last header, or the inline table
/// being read). The value of a pair follows its [`Visitor::key`] call as one
/// [`Visitor::scalar`], or as a bracketed run of further events.
pub trait Visitor<'s> {
    /// `[path]` was read and is now the table pairs go into.
    fn table_header(&mut self, path: KeyPath<'s>);
    /// `[[path]]` was read; a new table was appended to the array at path.
    fn array_of_tables_header(&mut self, path: KeyPath<'s>);
    /// A key/value pair begins; its value follows.
    fn key(&mut self, path: KeyPath<'s>);
    /// A scalar value.
    fn scalar(&mut self, value: Scalar<'s>);
    /// An array value begins; its elements follow.
    fn begin_array(&mut self);
    /// The innermost open array ends.
    fn end_array(&mut self);
    /// An inline table value begins; its pairs follow.
    fn begin_table(&mut self);
    /// The innermost open inline table ends.
    fn end_table(&mut self);
}

/// Why a document was refused, and the byte offset where it was noticed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    /// What was wrong.
    pub kind: ErrorKind,
    /// The byte offset into the text.
    pub at:   usize,
}

/// What was wrong with a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The text is not UTF-8.
    InvalidUtf8,
    /// The text is longer than the index can address (4 GiB).
    TooLong,
    /// Something else stood where the named thing was expected.
    Expected(&'static str),
    /// A control character where none is allowed.
    ControlCharacter,
    /// A carriage return that does not start a CRLF newline.
    BareCarriageReturn,
    /// A string ran to the end of its line or of the text.
    UnterminatedString,
    /// An escape the specification does not have.
    InvalidEscape,
    /// A `\x`, `\u` or `\U` escape that names no Unicode scalar value.
    InvalidUnicodeEscape,
    /// A number or a bare word that is no TOML value.
    InvalidValue,
    /// An integer outside the signed 64-bit range.
    IntegerOverflow,
    /// A float with more digits than the decoder's stack buffer holds.
    FloatTooLong,
    /// A date or time that is malformed or out of range.
    InvalidDatetime,
    /// A key given a value twice in one table.
    DuplicateKey,
    /// A table header naming a table already defined.
    DuplicateTable,
    /// A key path going through, or naming, something it may not extend:
    /// a value, an inline table, a static array, an array of tables where a
    /// table is meant, or a table another header or another table's dotted
    /// keys defined.
    CannotExtend,
    /// Arrays and inline tables nest deeper than [`Options::max_depth`].
    TooDeep,
    /// The index slice is full. [`nodes_for`] gives a size that always holds.
    OutOfNodes,
}

/// How many index nodes always suffice for a text of `len` bytes.
///
/// Every node the reader keeps is paid for by at least two bytes of text: a
/// key and the separator after it, a `{}` in an array, a `[[k]]` header
/// that makes two nodes from at least five bytes.
pub const fn nodes_for(len: usize) -> usize {
    len / 2 + 1
}

/// Reads `text` against `options`, telling `visitor` what it says.
///
/// `nodes` is the index for the duplicate-definition checks. Its size bounds
/// the document: [`nodes_for`] of the text's length always suffices, and a
/// smaller slice suffices for most documents; when it does not, the read
/// stops with [`ErrorKind::OutOfNodes`]. On success it gives the most nodes
/// the read held at once.
pub fn parse<'s, V: Visitor<'s>>(
    text: &'s [u8],
    options: Options,
    nodes: &mut [Node],
    visitor: &mut V,
) -> Result<usize, Error> {
    let src = match core::str::from_utf8(text) {
        Ok(s) => s,
        Err(e) => {
            return Err(Error {
                kind: ErrorKind::InvalidUtf8,
                at:   e.valid_up_to(),
            });
        },
    };
    if src.len() >= u32::MAX as usize {
        return Err(Error {
            kind: ErrorKind::TooLong,
            at:   0,
        });
    }
    parser::Parser::new(src, options, nodes, visitor).document()
}

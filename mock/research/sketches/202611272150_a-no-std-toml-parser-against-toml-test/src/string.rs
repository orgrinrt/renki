//! Strings: validated while read, decoded only when asked.

use crate::{ErrorKind, Version};

/// Which of the four string forms a [`Str`] was written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrKind {
    /// `"..."`, with escapes.
    Basic,
    /// `'...'`, taken as written. A bare key reads as this kind too.
    Literal,
    /// `"""..."""`, with escapes and line-ending backslashes.
    MlBasic,
    /// `'''...'''`, taken as written.
    MlLiteral,
}

/// A string value or key: a span of the caller's text, already validated.
///
/// The span is the content between the delimiters, with the newline right
/// after an opening `"""` or `'''` left out. Nothing is decoded until
/// [`Str::chars`], [`Str::as_borrowed`] or [`Str::decode_into`] is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Str<'s> {
    raw:  &'s str,
    kind: StrKind,
}

impl<'s> Str<'s> {
    pub(crate) const fn new(raw: &'s str, kind: StrKind) -> Self {
        Self {
            raw,
            kind,
        }
    }

    /// The content as written, escapes and all.
    pub const fn raw(&self) -> &'s str {
        self.raw
    }

    /// The form it was written in.
    pub const fn kind(&self) -> StrKind {
        self.kind
    }

    /// The decoded characters, one at a time, with no buffer.
    pub fn chars(&self) -> StrChars<'s> {
        StrChars {
            rest:    self.raw,
            escapes: matches!(self.kind, StrKind::Basic | StrKind::MlBasic),
        }
    }

    /// The decoded string as a borrow of the text, where decoding changes
    /// nothing: no escape and no CRLF in it.
    pub fn as_borrowed(&self) -> Option<&'s str> {
        let escapes = matches!(self.kind, StrKind::Basic | StrKind::MlBasic);
        let ml = matches!(self.kind, StrKind::MlBasic | StrKind::MlLiteral);
        let b = self.raw.as_bytes();
        if (escapes && b.contains(&b'\\')) || (ml && b.contains(&b'\r')) {
            None
        } else {
            Some(self.raw)
        }
    }

    /// The decoded length in bytes.
    pub fn decoded_len(&self) -> usize {
        self.chars().map(char::len_utf8).sum()
    }

    /// Decodes into `buf`; refused with the size needed when it is short.
    pub fn decode_into<'b>(&self, buf: &'b mut [u8]) -> Result<&'b str, usize> {
        let need = self.decoded_len();
        if need > buf.len() {
            return Err(need);
        }
        let mut at = 0;
        for c in self.chars() {
            at += c.encode_utf8(&mut buf[at ..]).len();
        }
        core::str::from_utf8(&buf[.. at]).map_err(|_| need)
    }

    /// Equal as decoded strings, whatever forms the two were written in.
    pub fn eq_decoded(&self, other: &Str<'_>) -> bool {
        self.chars().eq(other.chars())
    }
}

/// The decoded characters of a [`Str`].
#[derive(Clone, Debug)]
pub struct StrChars<'s> {
    rest:    &'s str,
    escapes: bool,
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap_or(0xFFFD)
}

impl Iterator for StrChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<char> {
        loop {
            let mut it = self.rest.chars();
            let c = it.next()?;
            self.rest = it.as_str();
            if c == '\r' && self.rest.starts_with('\n') {
                continue;
            }
            if !(self.escapes && c == '\\') {
                return Some(c);
            }
            let mut it = self.rest.chars();
            let e = it.next()?;
            self.rest = it.as_str();
            let width = match e {
                'b' => return Some('\u{8}'),
                't' => return Some('\t'),
                'n' => return Some('\n'),
                'f' => return Some('\u{c}'),
                'r' => return Some('\r'),
                'e' => return Some('\u{1b}'),
                '"' => return Some('"'),
                '\\' => return Some('\\'),
                'x' => 2,
                'u' => 4,
                'U' => 8,
                _ => {
                    // A line-ending backslash: drop every space, tab and
                    // newline up to the next other character.
                    self.rest = self.rest.trim_start_matches([' ', '\t', '\r', '\n']);
                    continue;
                },
            };
            let (digits, rest) = self.rest.split_at(width);
            self.rest = rest;
            return Some(char::from_u32(hex(digits)).unwrap_or('\u{FFFD}'));
        }
    }
}

type Scan<'s> = Result<(Str<'s>, usize), (ErrorKind, usize)>;

fn control(b: u8) -> bool {
    (b < 0x20 && b != b'\t') || b == 0x7F
}

/// Validates an escape whose backslash is at `at`; gives the offset after.
fn escape(b: &[u8], at: usize, v: Version) -> Result<usize, (ErrorKind, usize)> {
    let width = match b.get(at + 1) {
        Some(b'b' | b't' | b'n' | b'f' | b'r' | b'"' | b'\\') => return Ok(at + 2),
        Some(b'e') if v == Version::V1_1_0 => return Ok(at + 2),
        Some(b'x') if v == Version::V1_1_0 => 2,
        Some(b'u') => 4,
        Some(b'U') => 8,
        _ => return Err((ErrorKind::InvalidEscape, at)),
    };
    let start = at + 2;
    let end = start + width;
    let digits = b
        .get(start .. end)
        .ok_or((ErrorKind::InvalidUnicodeEscape, at))?;
    if !digits.iter().all(u8::is_ascii_hexdigit) {
        return Err((ErrorKind::InvalidUnicodeEscape, at));
    }
    let mut n: u32 = 0;
    for d in digits {
        n = n * 16 + (*d as char).to_digit(16).unwrap_or(0);
    }
    if char::from_u32(n).is_none() {
        return Err((ErrorKind::InvalidUnicodeEscape, at));
    }
    Ok(end)
}

/// A `"..."` string opening at `at`.
pub(crate) fn basic(s: &str, at: usize, v: Version) -> Scan<'_> {
    let b = s.as_bytes();
    let start = at + 1;
    let mut i = start;
    loop {
        match b.get(i) {
            None | Some(b'\n') => return Err((ErrorKind::UnterminatedString, i)),
            Some(b'"') => return Ok((Str::new(&s[start .. i], StrKind::Basic), i + 1)),
            Some(b'\\') => i = escape(b, i, v)?,
            Some(&c) if control(c) => return Err((ErrorKind::ControlCharacter, i)),
            Some(_) => i += 1,
        }
    }
}

/// A `'...'` string opening at `at`.
pub(crate) fn literal(s: &str, at: usize) -> Scan<'_> {
    let b = s.as_bytes();
    let start = at + 1;
    let mut i = start;
    loop {
        match b.get(i) {
            None | Some(b'\n') => return Err((ErrorKind::UnterminatedString, i)),
            Some(b'\'') => return Ok((Str::new(&s[start .. i], StrKind::Literal), i + 1)),
            Some(&c) if control(c) => return Err((ErrorKind::ControlCharacter, i)),
            Some(_) => i += 1,
        }
    }
}

/// The offset after a newline at `i`, if one is there.
fn newline(b: &[u8], i: usize) -> Option<usize> {
    match b.get(i) {
        Some(b'\n') => Some(i + 1),
        Some(b'\r') if b.get(i + 1) == Some(&b'\n') => Some(i + 2),
        _ => None,
    }
}

/// A `"""..."""` or `'''...'''` string opening at `at`.
pub(crate) fn multiline(s: &str, at: usize, quote: u8, v: Version) -> Scan<'_> {
    let b = s.as_bytes();
    let escapes = quote == b'"';
    let mut i = at + 3;
    if let Some(after) = newline(b, i) {
        i = after;
    }
    let start = i;
    loop {
        match b.get(i) {
            None => return Err((ErrorKind::UnterminatedString, i)),
            Some(&q) if q == quote => {
                let mut run = 0;
                while b.get(i + run) == Some(&quote) {
                    run += 1;
                }
                if run < 3 {
                    i += run;
                    continue;
                }
                if run > 5 {
                    return Err((ErrorKind::Expected("end of string"), i + 5));
                }
                let end = i + run - 3;
                let kind = if escapes { StrKind::MlBasic } else { StrKind::MlLiteral };
                return Ok((Str::new(&s[start .. end], kind), i + run));
            },
            Some(b'\\') if escapes => {
                let mut j = i + 1;
                while matches!(b.get(j), Some(b' ' | b'\t')) {
                    j += 1;
                }
                if let Some(after) = newline(b, j) {
                    i = after;
                } else if j > i + 1 {
                    return Err((ErrorKind::InvalidEscape, i));
                } else {
                    i = escape(b, i, v)?;
                }
            },
            Some(b'\n') => i += 1,
            Some(b'\r') => {
                match newline(b, i) {
                    Some(after) => i = after,
                    None => return Err((ErrorKind::BareCarriageReturn, i)),
                }
            },
            Some(&c) if control(c) => return Err((ErrorKind::ControlCharacter, i)),
            Some(_) => i += 1,
        }
    }
}

//! Bare values: booleans, integers in every base, floats, and the token
//! scan that hands dates and times on.

use crate::{ErrorKind, Scalar, Version, datetime};

fn token_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b':' | b'+' | b'-')
}

/// The end of the bare value token starting at `at`. A date followed by a
/// space and a time is one token.
pub(crate) fn token_end(b: &[u8], at: usize) -> usize {
    let mut i = at;
    while b.get(i).is_some_and(|c| token_byte(*c)) {
        i += 1;
    }
    let t = &b[at .. i];
    let digit = |j: usize| b.get(j).is_some_and(u8::is_ascii_digit);
    if t.len() == 10
        && datetime::looks_like(t)
        && b.get(i) == Some(&b' ')
        && digit(i + 1)
        && digit(i + 2)
        && b.get(i + 3) == Some(&b':')
    {
        i += 1;
        while b.get(i).is_some_and(|c| token_byte(*c)) {
            i += 1;
        }
    }
    i
}

/// Digits of `radix` with single underscores between them, from `at`;
/// gives the offset after them.
fn digits(t: &[u8], at: usize, radix: u32) -> Option<usize> {
    let is = |j: usize| t.get(j).is_some_and(|c| (*c as char).is_digit(radix));
    if !is(at) {
        return None;
    }
    let mut i = at + 1;
    loop {
        if is(i) {
            i += 1;
        } else if t.get(i) == Some(&b'_') && is(i + 1) {
            i += 2;
        } else if t.get(i) == Some(&b'_') {
            return None;
        } else {
            return Some(i);
        }
    }
}

fn radix_integer(t: &[u8], radix: u32) -> Result<i64, ErrorKind> {
    if digits(t, 2, radix) != Some(t.len()) {
        return Err(ErrorKind::InvalidValue);
    }
    let mut n: i64 = 0;
    for c in &t[2 ..] {
        if let Some(d) = (*c as char).to_digit(radix) {
            n = n
                .checked_mul(i64::from(radix))
                .and_then(|n| n.checked_add(i64::from(d)))
                .ok_or(ErrorKind::IntegerOverflow)?;
        }
    }
    Ok(n)
}

fn decimal_integer(t: &[u8]) -> Result<i64, ErrorKind> {
    let negative = t[0] == b'-';
    let mut n: i64 = 0;
    for c in t {
        if c.is_ascii_digit() {
            let d = i64::from(c - b'0');
            n = n
                .checked_mul(10)
                .and_then(|n| if negative { n.checked_sub(d) } else { n.checked_add(d) })
                .ok_or(ErrorKind::IntegerOverflow)?;
        }
    }
    Ok(n)
}

/// The largest float, in bytes once underscores are gone, the stack buffer
/// decodes. 800 significant digits decide every `f64` correctly.
const FLOAT_BUF: usize = 1024;

fn float(t: &[u8]) -> Result<f64, ErrorKind> {
    let mut buf = [0u8; FLOAT_BUF];
    let mut n = 0;
    for c in t.iter().filter(|c| **c != b'_') {
        *buf.get_mut(n).ok_or(ErrorKind::FloatTooLong)? = *c;
        n += 1;
    }
    core::str::from_utf8(&buf[.. n])
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(ErrorKind::InvalidValue)
}

/// Decodes a whole bare value token.
pub(crate) fn scalar<'s>(t: &[u8], v: Version) -> Result<Scalar<'s>, ErrorKind> {
    match t {
        b"true" => return Ok(Scalar::Boolean(true)),
        b"false" => return Ok(Scalar::Boolean(false)),
        b"inf" | b"+inf" => return Ok(Scalar::Float(f64::INFINITY)),
        b"-inf" => return Ok(Scalar::Float(f64::NEG_INFINITY)),
        b"nan" | b"+nan" => return Ok(Scalar::Float(f64::NAN)),
        b"-nan" => return Ok(Scalar::Float(-f64::NAN)),
        [b'0', b'x', ..] => return radix_integer(t, 16).map(Scalar::Integer),
        [b'0', b'o', ..] => return radix_integer(t, 8).map(Scalar::Integer),
        [b'0', b'b', ..] => return radix_integer(t, 2).map(Scalar::Integer),
        _ => {},
    }
    if datetime::looks_like(t) {
        return datetime::parse(t, v)
            .map(Scalar::Datetime)
            .ok_or(ErrorKind::InvalidDatetime);
    }
    let start = usize::from(matches!(t.first(), Some(b'+' | b'-')));
    let int_end = digits(t, start, 10).ok_or(ErrorKind::InvalidValue)?;
    if t[start] == b'0' && int_end - start > 1 {
        return Err(ErrorKind::InvalidValue);
    }
    if int_end == t.len() {
        return decimal_integer(t).map(Scalar::Integer);
    }
    let mut i = int_end;
    if t[i] == b'.' {
        i = digits(t, i + 1, 10).ok_or(ErrorKind::InvalidValue)?;
    }
    if matches!(t.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(t.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = digits(t, i, 10).ok_or(ErrorKind::InvalidValue)?;
    }
    if i != t.len() {
        return Err(ErrorKind::InvalidValue);
    }
    float(t).map(Scalar::Float)
}

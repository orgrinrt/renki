//! Dates and times, decoded into fields while read.

use crate::Version;

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    /// 0 to 9999.
    pub year:  u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to the month's last day.
    pub day:   u8,
}

/// A time of day. Digits past nanoseconds are cut off, never rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    /// 0 to 23.
    pub hour:       u8,
    /// 0 to 59.
    pub minute:     u8,
    /// 0 to 60, for a leap second. 0 when the text gave none (TOML 1.1).
    pub second:     u8,
    /// The fraction of the second, in nanoseconds.
    pub nanosecond: u32,
}

/// The offset of an offset date-time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offset {
    /// `Z`.
    Utc,
    /// `+HH:MM` or `-HH:MM`, in minutes east of UTC.
    Minutes(i16),
}

/// What TOML calls a date-time: one of four shapes by which parts it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Datetime {
    /// Absent for a local time.
    pub date:   Option<Date>,
    /// Absent for a local date.
    pub time:   Option<Time>,
    /// Present only for an offset date-time.
    pub offset: Option<Offset>,
}

fn two(t: &[u8], i: usize) -> Option<u8> {
    match (t.get(i), t.get(i + 1)) {
        (Some(a @ b'0' ..= b'9'), Some(b @ b'0' ..= b'9')) => Some((a - b'0') * 10 + (b - b'0')),
        _ => None,
    }
}

const fn days_in(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        },
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Whether a value token starts like a date or a time, and so must be one.
pub(crate) fn looks_like(t: &[u8]) -> bool {
    let digits =
        |r: core::ops::Range<usize>| t.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    (digits(0 .. 2) && t.get(2) == Some(&b':')) || (digits(0 .. 4) && t.get(4) == Some(&b'-'))
}

fn time(t: &[u8], i: &mut usize, v: Version) -> Option<Time> {
    let hour = two(t, *i)?;
    if t.get(*i + 2) != Some(&b':') {
        return None;
    }
    let minute = two(t, *i + 3)?;
    *i += 5;
    let mut second = 0;
    let mut nanosecond = 0;
    if t.get(*i) == Some(&b':') {
        second = two(t, *i + 1)?;
        *i += 3;
        if t.get(*i) == Some(&b'.') {
            *i += 1;
            let start = *i;
            while t.get(*i).is_some_and(u8::is_ascii_digit) {
                if *i - start < 9 {
                    nanosecond = nanosecond * 10 + u32::from(t[*i] - b'0');
                }
                *i += 1;
            }
            if *i == start {
                return None;
            }
            for _ in (*i - start) .. 9 {
                nanosecond *= 10;
            }
        }
    } else if v == Version::V1_0_0 {
        return None;
    }
    (hour <= 23 && minute <= 59 && second <= 60).then_some(Time {
        hour,
        minute,
        second,
        nanosecond,
    })
}

/// Decodes a whole value token as a date-time.
pub(crate) fn parse(t: &[u8], v: Version) -> Option<Datetime> {
    let mut i = 0;
    if t.get(2) == Some(&b':') {
        let time = time(t, &mut i, v)?;
        return (i == t.len()).then_some(Datetime {
            date:   None,
            time:   Some(time),
            offset: None,
        });
    }
    let year = u16::from(two(t, 0)?) * 100 + u16::from(two(t, 2)?);
    let (month, day) = (two(t, 5)?, two(t, 8)?);
    if t[4] != b'-'
        || t.get(7) != Some(&b'-')
        || !(1 ..= 12).contains(&month)
        || day == 0
        || day > days_in(year, month)
    {
        return None;
    }
    let date = Some(Date {
        year,
        month,
        day,
    });
    if t.len() == 10 {
        return Some(Datetime {
            date,
            time: None,
            offset: None,
        });
    }
    if !matches!(t[10], b'T' | b't' | b' ') {
        return None;
    }
    i = 11;
    let time = Some(time(t, &mut i, v)?);
    let offset = match t.get(i) {
        None => None,
        Some(b'Z' | b'z') => {
            i += 1;
            Some(Offset::Utc)
        },
        Some(sign @ (b'+' | b'-')) => {
            let (h, m) = (two(t, i + 1)?, two(t, i + 4)?);
            if t.get(i + 3) != Some(&b':') || h > 23 || m > 59 {
                return None;
            }
            i += 6;
            let minutes = i16::from(h) * 60 + i16::from(m);
            Some(Offset::Minutes(if *sign == b'-' {
                -minutes
            } else {
                minutes
            }))
        },
        Some(_) => return None,
    };
    (i == t.len()).then_some(Datetime {
        date,
        time,
        offset,
    })
}

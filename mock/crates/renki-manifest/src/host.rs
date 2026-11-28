//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The i/o a read needs, as one contract over the operating system, and why a
//! read is refused.

use core::ffi::{CStr, c_int};
use core::fmt::{self, Write};

/// Why a read is refused.
///
/// Every refusal names the file it is about, so a caller reading several
/// layers knows which one to look at, and its `Display` is the line the
/// design's table of refusals gives it, the same bytes the POSIX reader
/// writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadRead<'p> {
    /// `file` does not fit in the memory given, or for a variable, its value.
    /// `need` is the size that memory has to be for it to fit.
    Room {
        /// The file that does not fit.
        file: &'p CStr,
        /// The size the memory needs.
        need: usize,
    },
    /// The C library refused `call` on `file` with `errno`. Both are kept for
    /// a caller that asks, and the `Display` writes neither, as the POSIX
    /// reader cannot know them.
    Os {
        /// The file, or for a shell the directory it was to run in.
        file:  &'p CStr,
        /// The C function that refused.
        call:  &'static str,
        /// What it set `errno` to, or for `posix_spawn` what it returned.
        errno: c_int,
    },
    /// `file` is no regular file: a directory, a FIFO, a device or a socket
    /// named where a file was to be read.
    NotAFile {
        /// What was named.
        file: &'p CStr,
    },
}

/// `file` as a refusal names it: its bytes, each that is no UTF-8 as U+FFFD.
fn named(file: &CStr, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for chunk in file.to_bytes().utf8_chunks() {
        f.write_str(chunk.valid())?;
        if !chunk.invalid().is_empty() {
            f.write_char(char::REPLACEMENT_CHARACTER)?;
        }
    }
    Ok(())
}

impl fmt::Display for BadRead<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadRead::Room {
                file,
                need,
            } => {
                named(file, f)?;
                write!(f, " needs a room of {need} bytes")
            },
            BadRead::Os {
                file,
                ..
            } => {
                named(file, f)?;
                f.write_str(" cannot be read")
            },
            BadRead::NotAFile {
                file,
            } => {
                named(file, f)?;
                f.write_str(" is not a regular file")
            },
        }
    }
}

/// The i/o reading a manifest needs, over one operating system.
///
/// Implemented by a marker type per target family; nothing on it is `self`. A
/// path is a NUL-terminated string the caller holds, so no call allocates.
pub trait Host {
    /// Reads the whole file at `path` into `into` and answers how many bytes
    /// it holds. The size is known before a byte is read, so a file that does
    /// not fit is refused with [`BadRead::Room`] and nothing is read; a file
    /// that grows past what it said while it is read is refused the same. It
    /// is opened without blocking, and what is no regular file is refused
    /// with [`BadRead::NotAFile`], so a FIFO is never waited on.
    fn read<'p>(path: &'p CStr, into: &mut [u8]) -> Result<usize, BadRead<'p>>;

    /// Calls `each` with the name of every entry of the directory at `dir`,
    /// `.` and `..` left out, in the order the directory gives them. A
    /// listing the C library ends with an error is refused, after the entries
    /// it gave, so no entry goes missing in silence.
    fn list<'p>(dir: &'p CStr, each: impl FnMut(&CStr)) -> Result<(), BadRead<'p>>;

    /// The canonical form of `path`, every link and every `.` and `..`
    /// resolved, written into `into`. `into` is at least as long as the
    /// longest path the system has, or it is refused with the size it needs.
    fn canonical<'p, 'b>(path: &'p CStr, into: &'b mut [u8]) -> Result<&'b CStr, BadRead<'p>>;

    /// The value of the environment variable `name`, where it is set, copied
    /// into `into`, or refused with [`BadRead::Room`] and the length it needs.
    /// Nothing borrows the process's environment past the call. The copy does
    /// not guard the read itself: the C library guards `getenv` against no
    /// `setenv` on another thread, so a program that sets variables from one
    /// thread while another reads a manifest has that race.
    fn var<'n, 'b>(name: &'n CStr, into: &'b mut [u8]) -> Result<Option<&'b [u8]>, BadRead<'n>>;

    /// Runs `sh -c <command>` in the directory `dir` and answers its exit, or
    /// `None` where it ended with no exit, by a signal.
    #[cfg(feature = "processes")]
    fn shell<'p>(dir: &'p CStr, command: &CStr) -> Result<Option<c_int>, BadRead<'p>>;
}

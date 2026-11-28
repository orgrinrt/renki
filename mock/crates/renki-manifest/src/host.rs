//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The i/o a read needs, as one contract over the operating system, and why a
//! read is refused.

use core::ffi::{CStr, c_int};
use core::fmt;

/// Why a read is refused.
///
/// Every refusal names the file it is about, so a caller reading several
/// layers knows which one to look at.
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
    /// The C library refused `call` on `file` with `errno`.
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
    /// `file` is not UTF-8 text from byte `at` on.
    NotText {
        /// The file.
        file: &'p CStr,
        /// The offset of the first byte that is not.
        at:   usize,
    },
}

impl fmt::Display for BadRead<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadRead::Room {
                file,
                need,
            } => write!(f, "{file:?} needs a room of {need} bytes"),
            BadRead::Os {
                file,
                call,
                errno,
            } => write!(f, "{file:?}: {call} failed with errno {errno}"),
            BadRead::NotText {
                file,
                at,
            } => write!(f, "{file:?} is not UTF-8 from byte {at}"),
            BadRead::NotAFile {
                file,
            } => write!(f, "{file:?} is not a regular file"),
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
    /// Nothing borrows the process's environment past the call, since a
    /// thread of the program may set a variable while a manifest is read.
    fn var<'n, 'b>(name: &'n CStr, into: &'b mut [u8]) -> Result<Option<&'b [u8]>, BadRead<'n>>;

    /// Runs `sh -c <command>` in the directory `dir` and answers its exit, or
    /// `None` where it ended with no exit, by a signal.
    #[cfg(feature = "processes")]
    fn shell<'p>(dir: &'p CStr, command: &CStr) -> Result<Option<c_int>, BadRead<'p>>;
}

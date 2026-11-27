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
    /// `file` does not fit in the memory given. `need` is the size that memory
    /// has to be for it to fit: for a [`Room`](crate::Room), its whole text,
    /// the layers already read in it included, so a room of `need` bytes reads
    /// it.
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
    /// that grows past what it said while it is read is refused the same.
    fn read<'p>(path: &'p CStr, into: &mut [u8]) -> Result<usize, BadRead<'p>>;

    /// Calls `each` with the name of every entry of the directory at `dir`,
    /// `.` and `..` left out, in the order the directory gives them.
    fn list<'p>(dir: &'p CStr, each: impl FnMut(&CStr)) -> Result<(), BadRead<'p>>;

    /// The canonical form of `path`, every link and every `.` and `..`
    /// resolved, written into `into`. `into` is at least as long as the
    /// longest path the system has, or it is refused with the size it needs.
    fn canonical<'p, 'b>(path: &'p CStr, into: &'b mut [u8]) -> Result<&'b CStr, BadRead<'p>>;

    /// The value of the environment variable `name`, where it is set. It
    /// borrows from the process's environment, which holds while nothing
    /// sets a variable; reading a manifest sets none.
    fn var(name: &CStr) -> Option<&'static [u8]>;

    /// Runs `sh -c <command>` in the directory `dir` and answers its exit, or
    /// `None` where it ended with no exit, by a signal.
    #[cfg(feature = "shell-predicate")]
    fn shell<'p>(dir: &'p CStr, command: &CStr) -> Result<Option<c_int>, BadRead<'p>>;
}

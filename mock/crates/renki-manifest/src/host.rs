//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The i/o a read needs, as one contract over the operating system, and why a
//! read is refused.

use core::ffi::{CStr, c_int};
use core::fmt::{self, Write};

use notko::lend::Exhausted;
use notko::{Maybe, Outcome};

use crate::Shelf;

/// Why a read is refused.
///
/// Every refusal names the file it is about, so a caller reading several
/// layers knows which one to look at, and its `Display` is the line the
/// design's table of refusals gives it, the same bytes the POSIX reader
/// writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadRead<'p> {
    /// A shelf of the room falls short for `file`: which shelf, and what it
    /// gave, how much it held and how much the read wanted at the least.
    Room {
        /// The file the read was at.
        file:      &'p CStr,
        /// The shelf that fell short.
        shelf:     Shelf,
        /// What it held, and what the read wanted.
        exhausted: Exhausted,
    },
    /// The C library refused `call` on `file` with `errno`. Both are kept for
    /// a caller that asks, and the `Display` writes neither, as the POSIX
    /// reader cannot know them.
    Os {
        /// The file.
        file:  &'p CStr,
        /// The C function that refused.
        call:  &'static str,
        /// What it set `errno` to.
        errno: c_int,
    },
    /// `file` is no regular file: a directory, a FIFO, a device or a socket
    /// named where a file was to be read.
    NotAFile {
        /// What was named.
        file: &'p CStr,
    },
    /// A shell could not be started in `dir`, or not waited on: `call` is
    /// `posix_spawn` or `waitpid`, and `errno` what it answered.
    Shell {
        /// The directory the shell was to run in.
        dir:   &'p CStr,
        /// The C function that refused.
        call:  &'static str,
        /// What it answered.
        errno: c_int,
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
                shelf,
                exhausted,
            } => {
                named(file, f)?;
                write!(
                    f,
                    ": the room's {shelf} shelf holds {}, and the read wants at least {}",
                    exhausted.had, exhausted.wanted
                )
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
            BadRead::Shell {
                dir,
                ..
            } => {
                named(dir, f)?;
                f.write_str(": no shell could be run there")
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
    fn read<'p>(path: &'p CStr, into: &mut [u8]) -> Outcome<usize, BadRead<'p>>;

    /// Calls `each` with the name of every entry of the directory at `dir`,
    /// `.` and `..` left out, in the order the directory gives them. A
    /// listing the C library ends with an error is refused, after the entries
    /// it gave, so no entry goes missing in silence.
    fn list<'p>(dir: &'p CStr, each: impl FnMut(&CStr)) -> Outcome<(), BadRead<'p>>;

    /// The canonical form of `path`, every link and every `.` and `..`
    /// resolved, written into `into`. `into` is at least as long as the
    /// longest path the system has, or it is refused with the size it needs.
    fn canonical<'p, 'b>(path: &'p CStr, into: &'b mut [u8]) -> Outcome<&'b CStr, BadRead<'p>>;

    /// The value of the environment variable `name`, where it is set, copied
    /// into `into`, or refused with [`BadRead::Room`] and the length it needs.
    /// Nothing borrows the process's environment past the call. The copy does
    /// not guard the read itself: the C library guards `getenv` against no
    /// `setenv` on another thread, so a program that sets variables from one
    /// thread while another reads a manifest has that race.
    fn var<'n, 'b>(name: &'n CStr, into: &'b mut [u8]) -> Outcome<Maybe<&'b [u8]>, BadRead<'n>>;

    /// Runs `/bin/sh -c <command>` in the directory `dir` and answers its
    /// exit, or `Isnt` where it ended with no exit, by a signal, or
    /// [`BadRead::Shell`] where no shell could be started or waited on.
    #[cfg(feature = "processes")]
    fn shell<'p>(dir: &'p CStr, command: &CStr) -> Outcome<Maybe<c_int>, BadRead<'p>>;
}

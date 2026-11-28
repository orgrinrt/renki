//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `Host` over the C library, through `libc` with its default features off.
//!
//! The one module of the crate that holds `unsafe`: every block here is a C
//! call, its arguments NUL-terminated strings and buffers this module checked
//! the length of. Only where `errno` lives differs per C library, so it has a
//! row per target, and a target with no row does not build.

use core::ffi::{CStr, c_int};

use notko::lend::Exhausted;
use notko::{Maybe, Outcome};

use crate::{BadRead, Host, Shelf};

/// The unix targets: linux, macOS, freebsd, netbsd, openbsd and illumos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unix;

/// Where the C library keeps this thread's `errno`.
fn errno_slot() -> *mut c_int {
    #[cfg(target_os = "linux")]
    // SAFETY: the C library's own thread-local errno, always there.
    unsafe {
        libc::__errno_location()
    }
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    // SAFETY: as above.
    unsafe {
        libc::__error()
    }
    #[cfg(any(target_os = "openbsd", target_os = "netbsd"))]
    // SAFETY: as above.
    unsafe {
        libc::__errno()
    }
    #[cfg(target_os = "illumos")]
    // SAFETY: as above.
    unsafe {
        libc::___errno()
    }
}

fn errno() -> c_int {
    // SAFETY: the slot is this thread's and always readable.
    unsafe { *errno_slot() }
}

/// Sets `errno` to 0, for a call such as `readdir` that answers an error and
/// the end alike and tells them apart only by `errno`.
fn clear_errno() {
    // SAFETY: the slot is this thread's and always writable.
    unsafe { *errno_slot() = 0 }
}

fn os<'p>(file: &'p CStr, call: &'static str) -> BadRead<'p> {
    BadRead::Os {
        file,
        call,
        errno: errno(),
    }
}

/// What `fstat` says of the open file `fd`: its size, where it is a regular
/// file, and `Isnt` where it is anything else.
fn regular_size(fd: c_int) -> Outcome<Maybe<usize>, c_int> {
    // SAFETY: `stat` is plain data, and all zeroes is a value of it.
    let mut st: libc::stat = unsafe { core::mem::zeroed() };
    // SAFETY: `fd` is open, and `st` is a `stat` this frame owns.
    if unsafe { libc::fstat(fd, &mut st) } < 0 {
        return Outcome::Err(errno());
    }
    if st.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Outcome::Ok(Maybe::Isnt);
    }
    Outcome::Ok(Maybe::Is(usize::try_from(st.st_size).unwrap_or(usize::MAX)))
}

/// One `read` into `into`, retried on `EINTR`.
fn read_some(fd: c_int, into: &mut [u8]) -> Outcome<usize, c_int> {
    loop {
        // SAFETY: `into` is writable for its whole length, which is what is
        // passed.
        let n = unsafe { libc::read(fd, into.as_mut_ptr().cast(), into.len()) };
        if n >= 0 {
            return Outcome::Ok(n.unsigned_abs());
        }
        let e = errno();
        if e != libc::EINTR {
            return Outcome::Err(e);
        }
    }
}

fn read_open<'p>(path: &'p CStr, fd: c_int, into: &mut [u8]) -> Outcome<usize, BadRead<'p>> {
    let refused = |call, errno| {
        BadRead::Os {
            file: path,
            call,
            errno,
        }
    };
    let size = match regular_size(fd) {
        Outcome::Ok(Maybe::Is(size)) => size,
        Outcome::Ok(Maybe::Isnt) => {
            return Outcome::Err(BadRead::NotAFile {
                file: path,
            });
        },
        Outcome::Err(e) => return Outcome::Err(refused("fstat", e)),
    };
    if size > into.len() {
        return Outcome::Err(BadRead::Room {
            file:      path,
            shelf:     Shelf::Text,
            exhausted: Exhausted {
                wanted: size,
                had:    into.len(),
            },
        });
    }
    let mut at = 0;
    loop {
        if at == into.len() {
            // full: one more byte means it grew past what `fstat` said
            let mut probe = [0u8; 1];
            match read_some(fd, &mut probe) {
                Outcome::Ok(0) => return Outcome::Ok(at),
                Outcome::Ok(_) => {},
                Outcome::Err(e) => return Outcome::Err(refused("read", e)),
            }
            let now = match regular_size(fd) {
                Outcome::Ok(now) => now.unwrap_or(0),
                Outcome::Err(e) => return Outcome::Err(refused("fstat", e)),
            };
            return Outcome::Err(BadRead::Room {
                file:      path,
                shelf:     Shelf::Text,
                exhausted: Exhausted {
                    wanted: now.max(at + 1),
                    had:    at,
                },
            });
        }
        let n = match read_some(fd, &mut into[at ..]) {
            Outcome::Ok(n) => n,
            Outcome::Err(e) => return Outcome::Err(refused("read", e)),
        };
        if n == 0 {
            return Outcome::Ok(at);
        }
        at += n;
    }
}

impl Host for Unix {
    fn read<'p>(path: &'p CStr, into: &mut [u8]) -> Outcome<usize, BadRead<'p>> {
        // without blocking, so a FIFO named like a layer answers at once and is
        // refused by `fstat` below instead of waiting for a writer
        // SAFETY: `path` is NUL-terminated.
        let fd = unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            return Outcome::Err(os(path, "open"));
        }
        let read = read_open(path, fd, into);
        // SAFETY: `fd` was opened above and is closed once, here.
        unsafe { libc::close(fd) };
        read
    }

    fn list<'p>(dir: &'p CStr, mut each: impl FnMut(&CStr)) -> Outcome<(), BadRead<'p>> {
        // SAFETY: `dir` is NUL-terminated.
        let handle = unsafe { libc::opendir(dir.as_ptr()) };
        if handle.is_null() {
            return Outcome::Err(os(dir, "opendir"));
        }
        let mut ended = Outcome::Ok(());
        loop {
            // `readdir` answers null at the end and on an error alike, and
            // sets `errno` only on the error
            clear_errno();
            // SAFETY: `handle` is an open directory stream.
            let entry = unsafe { libc::readdir(handle) };
            if entry.is_null() {
                if errno() != 0 {
                    ended = Outcome::Err(os(dir, "readdir"));
                }
                break;
            }
            // SAFETY: a non-null entry holds a NUL-terminated name that stays
            // valid until the next `readdir` on the same stream.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if !matches!(name.to_bytes(), b"." | b"..") {
                each(name);
            }
        }
        // SAFETY: `handle` was opened above and is closed once, here.
        unsafe { libc::closedir(handle) };
        ended
    }

    fn canonical<'p, 'b>(path: &'p CStr, into: &'b mut [u8]) -> Outcome<&'b CStr, BadRead<'p>> {
        let longest = usize::try_from(libc::PATH_MAX).unwrap_or(usize::MAX);
        if into.len() < longest {
            return Outcome::Err(BadRead::Room {
                file:      path,
                shelf:     Shelf::Path,
                exhausted: Exhausted {
                    wanted: longest,
                    had:    into.len(),
                },
            });
        }
        // SAFETY: `path` is NUL-terminated and `into` holds `PATH_MAX` bytes,
        // the most `realpath` writes.
        let got = unsafe { libc::realpath(path.as_ptr(), into.as_mut_ptr().cast()) };
        if got.is_null() {
            return Outcome::Err(os(path, "realpath"));
        }
        match CStr::from_bytes_until_nul(into) {
            Ok(c) => Outcome::Ok(c),
            Err(_) => Outcome::Err(os(path, "realpath")),
        }
    }

    fn var<'n, 'b>(name: &'n CStr, into: &'b mut [u8]) -> Outcome<Maybe<&'b [u8]>, BadRead<'n>> {
        // SAFETY: `name` is NUL-terminated.
        let value = unsafe { libc::getenv(name.as_ptr()) };
        if value.is_null() {
            return Outcome::Ok(Maybe::Isnt);
        }
        // SAFETY: a non-null answer is a NUL-terminated string in the
        // environment block. It is copied out at once and nothing of it is
        // kept, so no borrow of the block outlives this call; the block is
        // read unguarded, so a `setenv` on another thread meanwhile is a race
        // this does not close, as no reader of the environment can.
        let value = unsafe { CStr::from_ptr(value) }.to_bytes();
        let had = into.len();
        let Some(into) = into.get_mut(.. value.len()) else {
            return Outcome::Err(BadRead::Room {
                file:      name,
                shelf:     Shelf::Path,
                exhausted: Exhausted {
                    wanted: value.len(),
                    had,
                },
            });
        };
        into.copy_from_slice(value);
        Outcome::Ok(Maybe::Is(into))
    }

    #[cfg(feature = "processes")]
    fn shell<'p>(dir: &'p CStr, command: &CStr) -> Outcome<Maybe<c_int>, BadRead<'p>> {
        spawn::shell(dir, command)
    }
}

#[cfg(feature = "processes")]
mod spawn {
    use core::ffi::{CStr, c_char, c_int};

    use notko::{Maybe, Outcome};

    use super::errno;
    use crate::BadRead;

    const SH: &CStr = c"/bin/sh";
    const DASH_C: &CStr = c"-c";
    // the directory is set by the shell itself, so no target needs a spawn
    // call to change it, which not every C library has
    // a directory it cannot enter exits 125, which no predicate holds or fails
    // on, and what the command prints goes to `/dev/null`, so no dump holds it
    const WRAP: &CStr = c"cd -- \"$1\" || exit 125; exec /bin/sh -c \"$2\" >/dev/null";
    const ZERO: &CStr = c"sh";

    unsafe extern "C" {
        static environ: *const *mut c_char;
    }

    pub(super) fn shell<'p>(dir: &'p CStr, command: &CStr) -> Outcome<Maybe<c_int>, BadRead<'p>> {
        let argv: [*mut c_char; 7] = [
            SH.as_ptr().cast_mut(),
            DASH_C.as_ptr().cast_mut(),
            WRAP.as_ptr().cast_mut(),
            ZERO.as_ptr().cast_mut(),
            dir.as_ptr().cast_mut(),
            command.as_ptr().cast_mut(),
            core::ptr::null_mut(),
        ];
        let mut pid: libc::pid_t = 0;
        // SAFETY: every argument is NUL-terminated, `argv` ends in a null, and
        // `environ` is the process's own environment block.
        let spawned = unsafe {
            libc::posix_spawn(
                &mut pid,
                SH.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                argv.as_ptr(),
                environ,
            )
        };
        if spawned != 0 {
            return Outcome::Err(BadRead::Shell {
                dir,
                call: "posix_spawn",
                errno: spawned,
            });
        }
        let mut status: c_int = 0;
        loop {
            // SAFETY: `pid` is the child spawned above, and `status` is this
            // frame's.
            if unsafe { libc::waitpid(pid, &mut status, 0) } == pid {
                break;
            }
            let e = errno();
            if e != libc::EINTR {
                return Outcome::Err(BadRead::Shell {
                    dir,
                    call: "waitpid",
                    errno: e,
                });
            }
        }
        Outcome::Ok(if libc::WIFEXITED(status) {
            Maybe::Is(libc::WEXITSTATUS(status))
        } else {
            Maybe::Isnt
        })
    }
}

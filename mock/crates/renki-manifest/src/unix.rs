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

use crate::{BadRead, Host};

/// The unix targets: linux, macOS, freebsd, netbsd, openbsd and illumos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unix;

fn errno() -> c_int {
    #[cfg(target_os = "linux")]
    // SAFETY: the C library's own thread-local errno, always readable.
    unsafe {
        *libc::__errno_location()
    }
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    // SAFETY: as above.
    unsafe {
        *libc::__error()
    }
    #[cfg(any(target_os = "openbsd", target_os = "netbsd"))]
    // SAFETY: as above.
    unsafe {
        *libc::__errno()
    }
    #[cfg(target_os = "illumos")]
    // SAFETY: as above.
    unsafe {
        *libc::___errno()
    }
}

fn os<'p>(file: &'p CStr, call: &'static str) -> BadRead<'p> {
    BadRead::Os {
        file,
        call,
        errno: errno(),
    }
}

/// The size `fstat` gives the open file `fd`.
fn size_of(fd: c_int) -> Result<usize, c_int> {
    // SAFETY: `stat` is plain data, and all zeroes is a value of it.
    let mut st: libc::stat = unsafe { core::mem::zeroed() };
    // SAFETY: `fd` is open, and `st` is a `stat` this frame owns.
    if unsafe { libc::fstat(fd, &mut st) } < 0 {
        return Err(errno());
    }
    Ok(usize::try_from(st.st_size).unwrap_or(usize::MAX))
}

/// One `read` into `into`, retried on `EINTR`.
fn read_some(fd: c_int, into: &mut [u8]) -> Result<usize, c_int> {
    loop {
        // SAFETY: `into` is writable for its whole length, which is what is
        // passed.
        let n = unsafe { libc::read(fd, into.as_mut_ptr().cast(), into.len()) };
        if n >= 0 {
            return Ok(n.unsigned_abs());
        }
        let e = errno();
        if e != libc::EINTR {
            return Err(e);
        }
    }
}

fn read_open<'p>(path: &'p CStr, fd: c_int, into: &mut [u8]) -> Result<usize, BadRead<'p>> {
    let refused = |call, errno| {
        BadRead::Os {
            file: path,
            call,
            errno,
        }
    };
    let size = size_of(fd).map_err(|e| refused("fstat", e))?;
    if size > into.len() {
        return Err(BadRead::Room {
            file: path,
            need: size,
        });
    }
    let mut at = 0;
    loop {
        if at == into.len() {
            // full: one more byte means it grew past what `fstat` said
            let mut probe = [0u8; 1];
            if read_some(fd, &mut probe).map_err(|e| refused("read", e))? > 0 {
                let now = size_of(fd).map_err(|e| refused("fstat", e))?;
                return Err(BadRead::Room {
                    file: path,
                    need: now.max(at + 1),
                });
            }
            return Ok(at);
        }
        let n = read_some(fd, &mut into[at ..]).map_err(|e| refused("read", e))?;
        if n == 0 {
            return Ok(at);
        }
        at += n;
    }
}

impl Host for Unix {
    fn read<'p>(path: &'p CStr, into: &mut [u8]) -> Result<usize, BadRead<'p>> {
        // SAFETY: `path` is NUL-terminated.
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(os(path, "open"));
        }
        let read = read_open(path, fd, into);
        // SAFETY: `fd` was opened above and is closed once, here.
        unsafe { libc::close(fd) };
        read
    }

    fn list<'p>(dir: &'p CStr, mut each: impl FnMut(&CStr)) -> Result<(), BadRead<'p>> {
        // SAFETY: `dir` is NUL-terminated.
        let handle = unsafe { libc::opendir(dir.as_ptr()) };
        if handle.is_null() {
            return Err(os(dir, "opendir"));
        }
        loop {
            // SAFETY: `handle` is an open directory stream.
            let entry = unsafe { libc::readdir(handle) };
            if entry.is_null() {
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
        Ok(())
    }

    fn canonical<'p, 'b>(path: &'p CStr, into: &'b mut [u8]) -> Result<&'b CStr, BadRead<'p>> {
        let longest = usize::try_from(libc::PATH_MAX).unwrap_or(usize::MAX);
        if into.len() < longest {
            return Err(BadRead::Room {
                file: path,
                need: longest,
            });
        }
        // SAFETY: `path` is NUL-terminated and `into` holds `PATH_MAX` bytes,
        // the most `realpath` writes.
        let got = unsafe { libc::realpath(path.as_ptr(), into.as_mut_ptr().cast()) };
        if got.is_null() {
            return Err(os(path, "realpath"));
        }
        CStr::from_bytes_until_nul(into).map_err(|_| os(path, "realpath"))
    }

    fn var(name: &CStr) -> Option<&'static [u8]> {
        // SAFETY: `name` is NUL-terminated.
        let value = unsafe { libc::getenv(name.as_ptr()) };
        if value.is_null() {
            return None;
        }
        // SAFETY: a non-null answer is a NUL-terminated string in the
        // environment block, which holds while nothing sets a variable.
        Some(unsafe { CStr::from_ptr(value) }.to_bytes())
    }

    #[cfg(feature = "processes")]
    fn shell<'p>(dir: &'p CStr, command: &CStr) -> Result<Option<c_int>, BadRead<'p>> {
        spawn::shell(dir, command)
    }
}

#[cfg(feature = "processes")]
mod spawn {
    use core::ffi::{CStr, c_char, c_int};

    use super::errno;
    use crate::BadRead;

    const SH: &CStr = c"/bin/sh";
    const DASH_C: &CStr = c"-c";
    // the directory is set by the shell itself, so no target needs a spawn
    // call to change it, which not every C library has
    const WRAP: &CStr = c"cd -- \"$1\" && exec /bin/sh -c \"$2\"";
    const ZERO: &CStr = c"sh";

    unsafe extern "C" {
        static environ: *const *mut c_char;
    }

    pub(super) fn shell<'p>(dir: &'p CStr, command: &CStr) -> Result<Option<c_int>, BadRead<'p>> {
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
            return Err(BadRead::Os {
                file:  dir,
                call:  "posix_spawn",
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
                return Err(BadRead::Os {
                    file:  dir,
                    call:  "waitpid",
                    errno: e,
                });
            }
        }
        Ok(libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status)))
    }
}

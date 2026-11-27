//! Can a `no_std`, no-alloc crate do every piece of i/o a manifest reader
//! needs on the unix targets renki builds for: read a file of unknown size
//! into a caller's buffer, list a directory, read one environment variable,
//! and run `sh -c <command>` in a directory and answer its exit?
//!
//! Every call is the C library's, through the `libc` crate with its default
//! features off, which is `no_std`. Paths go in as NUL-terminated bytes the
//! caller holds, so nothing here allocates.

#![no_std]

use core::ffi::{CStr, c_char, c_int};

/// Why a read could not be done. `TooBig` carries the size the file has, so a
/// caller with a bigger buffer or an arena knows how much to give.
#[derive(Debug, PartialEq, Eq)]
pub enum Fail {
    Open(c_int),
    Stat(c_int),
    Read(c_int),
    TooBig {
        need: usize,
    },
    Spawn(c_int),
    Wait(c_int),
}

fn errno() -> c_int {
    // Where errno lives differs per C library: one call per target, and a
    // target with no row here does not build.
    #[cfg(target_os = "linux")]
    unsafe {
        *libc::__errno_location()
    }
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    unsafe {
        *libc::__error()
    }
    #[cfg(any(target_os = "openbsd", target_os = "netbsd"))]
    unsafe {
        *libc::__errno()
    }
    #[cfg(target_os = "illumos")]
    unsafe {
        *libc::___errno()
    }
}

/// Reads the whole file at `path` into `buf`, answering the bytes read. The
/// size comes from `fstat` first, so a file bigger than the buffer is refused
/// with the size it needs before a byte is read, and a file that grows while
/// being read is caught by the read that does not reach end of file.
pub fn read_into<'b>(path: &CStr, buf: &'b mut [u8]) -> Result<&'b [u8], Fail> {
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(Fail::Open(errno()));
    }
    let out = read_fd(fd, buf);
    unsafe { libc::close(fd) };
    out
}

fn read_fd(fd: c_int, buf: &mut [u8]) -> Result<&[u8], Fail> {
    let mut st: libc::stat = unsafe { core::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } < 0 {
        return Err(Fail::Stat(errno()));
    }
    let size = st.st_size as usize;
    if size > buf.len() {
        return Err(Fail::TooBig {
            need: size,
        });
    }
    let mut at = 0;
    loop {
        if at == buf.len() {
            // Full: one more byte means it grew past what fstat said.
            let mut probe = [0u8; 1];
            let n = unsafe { libc::read(fd, probe.as_mut_ptr().cast(), 1) };
            if n > 0 {
                return Err(Fail::TooBig {
                    need: at + 1,
                });
            }
            break;
        }
        let n = unsafe { libc::read(fd, buf[at ..].as_mut_ptr().cast(), buf.len() - at) };
        if n < 0 {
            let e = errno();
            if e == libc::EINTR {
                continue;
            }
            return Err(Fail::Read(e));
        }
        if n == 0 {
            break;
        }
        at += n as usize;
    }
    Ok(&buf[.. at])
}

/// Calls `each` with every entry name of the directory at `path`, `.` and
/// `..` left out, borrowing the name from the C library's own buffer.
pub fn list(path: &CStr, mut each: impl FnMut(&CStr)) -> Result<(), Fail> {
    let dir = unsafe { libc::opendir(path.as_ptr()) };
    if dir.is_null() {
        return Err(Fail::Open(errno()));
    }
    loop {
        let ent = unsafe { libc::readdir(dir) };
        if ent.is_null() {
            break;
        }
        let name = unsafe { CStr::from_ptr((*ent).d_name.as_ptr()) };
        let b = name.to_bytes();
        if b == b"." || b == b".." {
            continue;
        }
        each(name);
    }
    unsafe { libc::closedir(dir) };
    Ok(())
}

/// One environment variable's value, borrowed from the environment block.
/// The borrow is as long as nothing sets the environment, which a reader does
/// not do.
pub fn var(name: &CStr) -> Option<&'static [u8]> {
    let p = unsafe { libc::getenv(name.as_ptr()) };
    if p.is_null() { None } else { Some(unsafe { CStr::from_ptr(p) }.to_bytes()) }
}

/// Runs `sh -c <command>` in `dir` and answers its exit status. The child
/// changes directory before the exec, through `posix_spawn_file_actions_
/// addchdir_np` where the C library has it; the portable fallback is
/// `sh -c 'cd -- "$1" && exec sh -c "$2"' sh <dir> <command>`, which needs no
/// such call, and is the one used here so every target takes the same path.
pub fn shell(dir: &CStr, command: &CStr) -> Result<c_int, Fail> {
    const SH: &CStr = c"/bin/sh";
    const DASH_C: &CStr = c"-c";
    const WRAP: &CStr = c"cd -- \"$1\" && exec /bin/sh -c \"$2\"";
    const ZERO: &CStr = c"sh";
    let argv: [*mut c_char; 6] = [
        SH.as_ptr().cast_mut(),
        DASH_C.as_ptr().cast_mut(),
        WRAP.as_ptr().cast_mut(),
        ZERO.as_ptr().cast_mut(),
        dir.as_ptr().cast_mut(),
        command.as_ptr().cast_mut(),
    ];
    let argv = [argv[0], argv[1], argv[2], argv[3], argv[4], argv[5], core::ptr::null_mut()];
    unsafe extern "C" {
        static environ: *const *mut c_char;
    }
    let mut pid: libc::pid_t = 0;
    let rc = unsafe {
        libc::posix_spawn(
            &mut pid,
            SH.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            argv.as_ptr(),
            environ,
        )
    };
    if rc != 0 {
        return Err(Fail::Spawn(rc));
    }
    let mut status: c_int = 0;
    loop {
        let w = unsafe { libc::waitpid(pid, &mut status, 0) };
        if w == pid {
            break;
        }
        let e = errno();
        if e != libc::EINTR {
            return Err(Fail::Wait(e));
        }
    }
    if libc::WIFEXITED(status) { Ok(libc::WEXITSTATUS(status)) } else { Ok(-1) }
}

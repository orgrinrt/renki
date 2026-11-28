//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Each C library call `Unix` makes, run on this host against the fixtures
//! under `tests/host/`, which are committed and never written to, and in a
//! directory of its own under the system's temporary one for what no committed
//! fixture can be, a FIFO and a directory of many entries.

use core::ffi::CStr;

use notko::lend::Exhausted;
use notko::{Maybe, Outcome};
use renki_manifest::{BadRead, Host, Shelf, Unix};

const ELEVEN: &CStr = c"tests/host/eleven_bytes";
const LISTED: &CStr = c"tests/host/listed";
const MISSING: &CStr = c"tests/host/no_such_file";

/// More reads than the process may hold descriptors open at once: the soft
/// limit is brought down to 256 where it is higher, so a call that never
/// gives its descriptor back runs out within the loop on any host.
fn past_the_descriptor_limit() -> usize {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `limit` is this frame's, and the call only fills it.
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
        0
    );
    if limit.rlim_cur > 256 {
        limit.rlim_cur = 256;
        // SAFETY: as above; lowering the soft limit is always allowed.
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) }, 0);
    }
    usize::try_from(limit.rlim_cur).unwrap() * 2
}

#[test]
fn open_fstat_and_read_take_a_whole_file_into_memory_that_fits() {
    let mut into = [0u8; 64];
    let n = Unix::read(ELEVEN, &mut into).unwrap();
    assert_eq!(&into[.. n], b"name = \"x\"\n");
}

#[test]
fn fstat_answers_the_size_before_a_byte_is_read_so_a_short_buffer_is_told_what_it_needs() {
    let mut into = [0u8; 4];
    assert_eq!(
        Unix::read(ELEVEN, &mut into),
        Outcome::Err(BadRead::Room {
            file:      ELEVEN,
            shelf:     Shelf::Text,
            exhausted: Exhausted {
                wanted: 11,
                had:    4,
            },
        })
    );
    assert_eq!(
        into, [0u8; 4],
        "nothing is read into a buffer that is refused"
    );
    let mut exact = [0u8; 11];
    assert_eq!(Unix::read(ELEVEN, &mut exact), Outcome::Ok(11));
}

#[test]
fn open_refuses_a_missing_file_with_its_errno() {
    let mut into = [0u8; 8];
    assert_eq!(
        Unix::read(MISSING, &mut into),
        Outcome::Err(BadRead::Os {
            file:  MISSING,
            call:  "open",
            errno: libc::ENOENT,
        })
    );
}

#[test]
fn close_gives_every_descriptor_back_so_reads_outlast_the_descriptor_limit() {
    let mut into = [0u8; 16];
    for _ in 0 .. past_the_descriptor_limit() {
        assert_eq!(Unix::read(ELEVEN, &mut into), Outcome::Ok(11));
    }
}

#[test]
fn opendir_and_readdir_list_every_entry_but_dot_and_dot_dot() {
    let mut seen = [false; 3];
    let mut others = 0;
    Unix::list(LISTED, |name| {
        match name.to_bytes() {
            b"one" => seen[0] = true,
            b"two" => seen[1] = true,
            b"three" => seen[2] = true,
            _ => others += 1,
        }
    })
    .unwrap();
    assert_eq!(seen, [true; 3]);
    assert_eq!(others, 0);
}

#[test]
fn opendir_refuses_a_missing_directory_and_closedir_gives_every_handle_back() {
    assert_eq!(
        Unix::list(MISSING, |_| {}),
        Outcome::Err(BadRead::Os {
            file:  MISSING,
            call:  "opendir",
            errno: libc::ENOENT,
        })
    );
    for _ in 0 .. past_the_descriptor_limit() {
        Unix::list(LISTED, |_| {}).unwrap();
    }
}

#[test]
fn realpath_resolves_dots_into_the_buffer_given() {
    let mut into = [0u8; 4096];
    let got = Unix::canonical(c"tests/host/listed/../listed/./one", &mut into).unwrap();
    let want = std::fs::canonicalize("tests/host/listed/one").unwrap();
    assert_eq!(got.to_bytes(), want.as_os_str().as_encoded_bytes());
    assert_eq!(
        Unix::canonical(MISSING, &mut into),
        Outcome::Err(BadRead::Os {
            file:  MISSING,
            call:  "realpath",
            errno: libc::ENOENT,
        })
    );
}

#[test]
fn realpath_refuses_a_buffer_shorter_than_the_longest_path_with_the_size_it_needs() {
    let mut into = [0u8; 8];
    assert_eq!(
        Unix::canonical(ELEVEN, &mut into),
        Outcome::Err(BadRead::Room {
            file:      ELEVEN,
            shelf:     Shelf::Path,
            exhausted: Exhausted {
                wanted: libc::PATH_MAX as usize,
                had:    8,
            },
        })
    );
}

#[test]
fn getenv_copies_a_set_variable_and_answers_nothing_for_an_unset_one() {
    let path = std::env::var_os("PATH").expect("a test runs with a PATH");
    let path = path.as_encoded_bytes();
    let mut into = vec![0u8; path.len()];
    assert_eq!(Unix::var(c"PATH", &mut into), Outcome::Ok(Maybe::Is(path)));
    let mut none = [0u8; 0];
    assert_eq!(
        Unix::var(c"RENKI_MANIFEST_NO_SUCH_VARIABLE", &mut none),
        Outcome::Ok(Maybe::Isnt)
    );
}

#[test]
fn getenv_refuses_a_buffer_too_short_for_the_value_with_the_size_it_needs() {
    let path = std::env::var_os("PATH").expect("a test runs with a PATH");
    let mut short = vec![0u8; path.len() - 1];
    assert_eq!(
        Unix::var(c"PATH", &mut short),
        Outcome::Err(BadRead::Room {
            file:      c"PATH",
            shelf:     Shelf::Path,
            exhausted: Exhausted {
                wanted: path.len(),
                had:    path.len() - 1,
            },
        })
    );
}

/// A directory of this test binary's own under the system's temporary one,
/// made afresh, for files the committed fixtures cannot hold.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("renki-manifest-host-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn c_path(path: &std::path::Path) -> std::ffi::CString {
    std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap()
}

#[test]
fn a_directory_given_as_a_file_is_refused_as_no_regular_file() {
    let mut into = [0u8; 8192];
    assert_eq!(
        Unix::read(LISTED, &mut into),
        Outcome::Err(BadRead::NotAFile {
            file: LISTED,
        })
    );
    // and so with a buffer too short for what `fstat` says a directory holds
    let mut small = [0u8; 4];
    assert_eq!(
        Unix::read(LISTED, &mut small),
        Outcome::Err(BadRead::NotAFile {
            file: LISTED,
        })
    );
}

#[test]
fn a_fifo_named_like_a_layer_is_refused_and_never_waited_on() {
    let dir = scratch("fifo");
    let fifo = dir.join("app.local.toml");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success(), "mkfifo made the fifo");
    let path = c_path(&fifo);
    let (send, got) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut into = [0u8; 64];
        let refused = matches!(
            Unix::read(&path, &mut into),
            Outcome::Err(BadRead::NotAFile { .. })
        );
        send.send(refused).unwrap();
    });
    let answer = got.recv_timeout(std::time::Duration::from_secs(5));
    assert_eq!(
        answer,
        Ok(true),
        "the read answered at once, refusing the fifo"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_file_that_holds_more_than_fstat_said_is_refused_with_more_room() {
    // `/proc` files say a size of 0 and hold more: the grow-past branch
    let status = c"/proc/self/status";
    let mut none = [0u8; 0];
    assert_eq!(
        Unix::read(status, &mut none),
        Outcome::Err(BadRead::Room {
            file:      status,
            shelf:     Shelf::Text,
            exhausted: Exhausted {
                wanted: 1,
                had:    0,
            },
        })
    );
}

#[test]
fn readdir_lists_every_entry_of_a_large_directory_then_ends() {
    let dir = scratch("many");
    for i in 0 .. 2000 {
        std::fs::write(dir.join(format!("f{i:04}")), b"").unwrap();
    }
    let path = c_path(&dir);
    let mut seen = 0;
    Unix::list(&path, |_| seen += 1).unwrap();
    assert_eq!(seen, 2000);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_listing_readdir_ends_with_an_error_is_refused_after_the_entries_it_gave() {
    // the listing's own descriptor is turned into `/dev/null` while it is
    // read, so the next read of entries fails with `ENOTDIR`, an error
    // `readdir` ends a listing on; nothing else's descriptor is touched
    let dir = scratch("unreadable-listing");
    for i in 0 .. 3 {
        std::fs::write(dir.join(format!("f{i}")), b"").unwrap();
    }
    let path = c_path(&dir);
    let canonical = std::fs::canonicalize(&dir).unwrap();
    let mut seen = 0;
    let got = Unix::list(&path, |_| {
        if seen == 0 {
            let fd: libc::c_int = std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .flatten()
                .find(|e| std::fs::read_link(e.path()).ok().as_deref() == Some(canonical.as_path()))
                .and_then(|e| e.file_name().to_str()?.parse().ok())
                .expect("the listing's descriptor is found");
            // SAFETY: `null` is opened here and closed once; `dup2` puts it
            // over the listing's descriptor, which `closedir` closes after.
            unsafe {
                let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
                assert!(null >= 0);
                assert_eq!(libc::dup2(null, fd), fd);
                libc::close(null);
            }
        }
        seen += 1;
    });
    assert!(seen >= 1, "the entries before the error were given");
    assert_eq!(
        got,
        Outcome::Err(BadRead::Os {
            file:  &path,
            call:  "readdir",
            errno: libc::ENOTDIR,
        })
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(feature = "processes")]
mod spawned {
    use super::*;

    #[test]
    fn posix_spawn_and_waitpid_answer_the_exit_of_the_command() {
        assert_eq!(Unix::shell(LISTED, c"exit 0"), Outcome::Ok(Maybe::Is(0)));
        assert_eq!(Unix::shell(LISTED, c"exit 1"), Outcome::Ok(Maybe::Is(1)));
        assert_eq!(Unix::shell(LISTED, c"exit 2"), Outcome::Ok(Maybe::Is(2)));
    }

    #[test]
    fn the_command_runs_in_the_directory_given() {
        assert_eq!(
            Unix::shell(LISTED, c"test -f one && test -f three"),
            Outcome::Ok(Maybe::Is(0))
        );
        assert_eq!(
            Unix::shell(c"tests/host", c"test -f one"),
            Outcome::Ok(Maybe::Is(1))
        );
    }

    #[test]
    fn a_command_ended_by_a_signal_has_no_exit() {
        assert_eq!(Unix::shell(LISTED, c"kill -9 $$"), Outcome::Ok(Maybe::Isnt));
    }

    #[test]
    fn a_directory_that_is_not_there_is_the_wrapper_s_failure_and_not_a_hang() {
        let Outcome::Ok(Maybe::Is(exit)) = Unix::shell(MISSING, c"exit 0") else {
            panic!("the wrapper ran and answered");
        };
        assert_ne!(exit, 0);
    }

    #[test]
    fn a_directory_the_shell_cannot_enter_exits_125() {
        assert_eq!(Unix::shell(MISSING, c"exit 0"), Outcome::Ok(Maybe::Is(125)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn what_a_command_prints_goes_to_dev_null() {
        assert_eq!(
            Unix::shell(LISTED, c"test \"$(readlink /proc/$$/fd/1)\" = /dev/null"),
            Outcome::Ok(Maybe::Is(0))
        );
        // the control: its standard error is still the reader's
        assert_eq!(
            Unix::shell(LISTED, c"test \"$(readlink /proc/$$/fd/2)\" != /dev/null"),
            Outcome::Ok(Maybe::Is(0))
        );
    }
}

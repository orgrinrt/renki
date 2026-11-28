//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Each C library call `Unix` makes, run on this host against the fixtures
//! under `tests/host/`, which are committed and never written to.

use core::ffi::CStr;

use renki_manifest::{BadRead, Host, Unix};

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
        Err(BadRead::Room {
            file: ELEVEN,
            need: 11,
        })
    );
    assert_eq!(
        into, [0u8; 4],
        "nothing is read into a buffer that is refused"
    );
    let mut exact = [0u8; 11];
    assert_eq!(Unix::read(ELEVEN, &mut exact), Ok(11));
}

#[test]
fn open_refuses_a_missing_file_with_its_errno() {
    let mut into = [0u8; 8];
    assert_eq!(
        Unix::read(MISSING, &mut into),
        Err(BadRead::Os {
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
        assert_eq!(Unix::read(ELEVEN, &mut into), Ok(11));
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
        Err(BadRead::Os {
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
        Err(BadRead::Os {
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
        Err(BadRead::Room {
            file: ELEVEN,
            need: libc::PATH_MAX as usize,
        })
    );
}

#[test]
fn getenv_answers_a_set_variable_and_nothing_for_an_unset_one() {
    let path = std::env::var_os("PATH").expect("a test runs with a PATH");
    assert_eq!(Unix::var(c"PATH"), Some(path.as_encoded_bytes()));
    assert_eq!(Unix::var(c"RENKI_MANIFEST_NO_SUCH_VARIABLE"), None);
}

#[cfg(feature = "processes")]
mod spawned {
    use super::*;

    #[test]
    fn posix_spawn_and_waitpid_answer_the_exit_of_the_command() {
        assert_eq!(Unix::shell(LISTED, c"exit 0"), Ok(Some(0)));
        assert_eq!(Unix::shell(LISTED, c"exit 1"), Ok(Some(1)));
        assert_eq!(Unix::shell(LISTED, c"exit 2"), Ok(Some(2)));
    }

    #[test]
    fn the_command_runs_in_the_directory_given() {
        assert_eq!(
            Unix::shell(LISTED, c"test -f one && test -f three"),
            Ok(Some(0))
        );
        assert_eq!(Unix::shell(c"tests/host", c"test -f one"), Ok(Some(1)));
    }

    #[test]
    fn a_command_ended_by_a_signal_has_no_exit() {
        assert_eq!(Unix::shell(LISTED, c"kill -9 $$"), Ok(None));
    }

    #[test]
    fn a_directory_that_is_not_there_is_the_wrapper_s_failure_and_not_a_hang() {
        let Ok(Some(exit)) = Unix::shell(MISSING, c"exit 0") else {
            panic!("the wrapper ran and answered");
        };
        assert_ne!(exit, 0);
    }
}

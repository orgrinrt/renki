//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Building this crate for a target that is not unix says why, rather than
//! failing on a missing import.
//!
//! The launcher ends in `CommandExt::exec`, which is unix-only. Two files
//! import it unconditionally, so a non-unix build was failing with `could not
//! find 'unix' in 'os'` twice and nothing at all about the design decision
//! behind it. The `compile_error!` in `lib.rs` is that explanation, and this is
//! the test that it actually reaches a reader.
//!
//! It compiles for real rather than asserting on the source text, because a
//! guard behind the wrong `cfg` reads correctly and fires never.

use std::process::Command;

/// A target in the installed set that is not unix. Rebuilding the standard
/// library is not something a test should do, so this asks rustup rather than
/// naming a target and hoping.
const NON_UNIX: &str = "wasm32-unknown-unknown";

fn target_is_installed() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim() == NON_UNIX)
        })
        .unwrap_or(false)
}

#[test]
fn a_non_unix_build_explains_itself() {
    assert!(
        target_is_installed(),
        "the non-unix guard was not exercised, because {NON_UNIX} is not installed.\n\
         `rustup target add {NON_UNIX}` and run this again.\n\
         \n\
         This used to print the same sentence and return, which the harness \
         reported as `ok`. cargo swallows the output of a passing test, so on \
         every machine without that target the line read green having checked \
         nothing, and the machine most likely to have broken this guard is a \
         fresh one. A check that did not run is not a check that passed."
    );

    let out = Command::new(env!("CARGO"))
        .args(["check", "--target", NON_UNIX])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo did not run");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a non-unix target must not build, and it did:\n{stderr}"
    );
    // `renki-dirs` refuses first, since a dependency is built before its
    // consumer and the build stops there, so what a porter reads today is its
    // message. The launcher's own guard stands behind it for the day the
    // table gains a column and the exec is what is left; each crate's suite
    // pins its own sentence, and this one pins that the reader gets one of
    // them rather than a missing import.
    let dirs_refused = stderr.contains("A port adds a `Platform` for it");
    let launcher_refused = stderr.contains("which is a unix operation")
        && stderr.contains("a port needs a different handover");
    assert!(
        dirs_refused || launcher_refused,
        "the build failed without saying why it cannot work here:\n{stderr}"
    );
    assert!(
        !stderr.contains("could not find `unix` in `os`"),
        "the missing import reached the reader ahead of the explanation:\n{stderr}"
    );
}

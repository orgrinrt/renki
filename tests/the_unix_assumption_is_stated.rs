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
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == NON_UNIX))
        .unwrap_or(false)
}

#[test]
fn a_non_unix_build_explains_itself() {
    if !target_is_installed() {
        // Not a pass. There is no skip state to report, so this is the record:
        // the check did not run on this machine, and `rustup target add
        // wasm32-unknown-unknown` is what makes it run.
        eprintln!(
            "NOT CHECKED: {NON_UNIX} is not installed, so the non-unix guard was not exercised. \
             `rustup target add {NON_UNIX}` to run it."
        );
        return;
    }

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
    assert!(
        stderr.contains("which is a unix operation"),
        "the build failed without saying why it cannot work here:\n{stderr}"
    );
    assert!(
        stderr.contains("a port needs a different handover"),
        "the message stopped short of the part a porter needs:\n{stderr}"
    );
}

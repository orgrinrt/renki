//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A target the table has no column for does not build, and says so.
//!
//! Without the guard a Windows build took the XDG column and printed
//! `%HOME%/.cache/<ns>`, which is a wrong answer rather than a refusal. It
//! compiles for real rather than reading the source, because a guard behind
//! the wrong `cfg` reads correctly and fires never.

use std::process::Command;

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
fn a_target_with_no_column_is_refused_by_name() {
    assert!(
        target_is_installed(),
        "the guard was not exercised, because {NON_UNIX} is not installed; \
         `rustup target add {NON_UNIX}` and run this again. A check that did \
         not run is not a check that passed."
    );
    let out = Command::new(env!("CARGO"))
        .args(["check", "--target", NON_UNIX])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo did not run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "a target with no column built:\n{stderr}"
    );
    assert!(
        stderr.contains("this target is neither") && stderr.contains("A port adds a `Platform`"),
        "the build failed without naming the missing column:\n{stderr}"
    );
}

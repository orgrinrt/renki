//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The library checked for every target `Unix` has a row for, with the shell
//! predicate and without it, and refused for a target it has none for.
//!
//! The crate needs `core` alone, so each target is checked against a `core`
//! built from source, which needs no installed target. A check of its own
//! directory, so it never waits on the build running this test.

use std::process::Command;

const TARGETS: [&str; 9] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-freebsd",
    "x86_64-unknown-netbsd",
    "x86_64-unknown-openbsd",
    "x86_64-unknown-illumos",
];

const TARGET_DIR: &str = "../../../target/renki-manifest-every-target";

fn check(target: &str, features: &[&str]) -> std::process::Output {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    Command::new(cargo)
        .args(["check", "--lib", "--quiet", "-Zbuild-std=core", "--target", target])
        .args(["--target-dir", TARGET_DIR])
        .args(features)
        .output()
        .expect("cargo runs")
}

#[test]
fn the_library_builds_for_every_target_unix_names() {
    let mut failed = Vec::new();
    for target in TARGETS {
        for features in [&[][..], &["--features", "processes"][..]] {
            let out = check(target, features);
            if !out.status.success() {
                failed.push(format!(
                    "{target} {features:?}:\n{}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn a_target_with_no_row_does_not_build_and_says_why() {
    let out = check("x86_64-unknown-dragonfly", &[]);
    assert!(!out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("has no `Host` for this target"), "{said}");
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Every Rust file in the crate is one the header tool maintains.
//!
//! The headers at the top of these files are generated and kept current from
//! `ante.toml`, and a file outside that config's `include` still gets a header
//! the first time somebody writes one by hand. Nothing then keeps it current,
//! and it goes stale silently: the year rolls over, an address changes, and the
//! one file nobody's tool touches keeps the old text.
//!
//! That is not hypothetical here. `tests/` held a header and the config named
//! only `src/`, so exactly one file in the crate was hand-maintained and
//! nothing said so. The tool reported everything it looked at as passing,
//! which it was, and it had never looked at that file.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

/// The directory prefixes `ante.toml` names, read out of the config rather
/// than restated here. A copy of the answer in the test would agree with
/// itself forever and with the config never.
fn configured_prefixes() -> BTreeSet<String> {
    let toml = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("ante.toml"))
        .expect("ante.toml is not readable");

    let line = toml
        .lines()
        .find(|l| l.trim_start().starts_with("include"))
        .expect("ante.toml names no `include`, so nothing at all is covered");

    line.split('"')
        .filter(|s| s.ends_with(".rs"))
        .filter_map(|glob| glob.split_once("/**/").map(|(dir, _)| dir.to_owned()))
        .collect()
}

/// Tracked Rust files, from git rather than from a walk, so an ignored build
/// directory cannot make the answer look better than it is.
fn tracked_rust_files() -> Vec<String> {
    let out = Command::new("git")
        .args(["ls-files", "*.rs"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("git did not run");
    assert!(out.status.success(), "git ls-files failed");

    let files: Vec<String> = String::from_utf8(out.stdout)
        .expect("git printed a path that is not utf-8")
        .lines()
        .map(str::to_owned)
        .collect();

    assert!(
        !files.is_empty(),
        "git listed no Rust files at all, so every assertion below would hold \
         over an empty set and say nothing. Either this is not a checkout or \
         the pathspec stopped matching."
    );
    files
}

fn covered_by(prefixes: &BTreeSet<String>, file: &str) -> bool {
    prefixes.iter().any(|p| file.starts_with(&format!("{p}/")))
}

#[test]
fn no_rust_file_sits_outside_the_header_config() {
    let prefixes = configured_prefixes();
    assert!(
        !prefixes.is_empty(),
        "no directory prefix came out of ante.toml's include, so the check \
         below would find nothing covered and fail for the wrong reason"
    );

    let uncovered: Vec<String> = tracked_rust_files()
        .into_iter()
        .filter(|f| !covered_by(&prefixes, f))
        .collect();

    assert!(
        uncovered.is_empty(),
        "these Rust files are outside every include glob in ante.toml, so their \
         headers are hand-maintained and will go stale:\n  {}\n\
         The fix is a glob in ante.toml, not a header written by hand.",
        uncovered.join("\n  ")
    );
}

/// The control. Without it the assertion above passes on any config that
/// happens to name a prefix every file starts with, including a config that
/// named `""` and covered everything vacuously.
#[test]
fn a_directory_the_config_omits_is_reported() {
    let only_src: BTreeSet<String> = ["src".to_owned()].into_iter().collect();

    assert!(
        covered_by(&only_src, "src/lib.rs"),
        "the matcher failed to cover a file that is plainly under the prefix"
    );
    assert!(
        !covered_by(&only_src, "tests/a_test.rs"),
        "the matcher called a file covered by a prefix it does not sit under, \
         so it would call any config complete and the check above is vacuous"
    );
    assert!(
        !covered_by(&only_src, "src_generated/thing.rs"),
        "the matcher matched on a bare string prefix rather than a path \
         component, so a sibling directory whose name merely starts the same \
         way would read as covered"
    );
}

/// The tests this crate publishes have to run from an unpacked tarball, where
/// the repository is not there.
///
/// Two kinds of test live under `tests/`. One checks the crate and belongs in
/// the package. The other checks this repository, reads files `include`
/// deliberately leaves out, and asks git what is tracked. Shipping the second
/// kind produces a crate whose own suite fails on a file that was never meant
/// to be in it, and `cargo package` will not catch it, because packaging
/// compiles the tests and never runs them.
#[test]
fn a_shipped_test_does_not_reach_for_the_repository() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let toml = std::fs::read_to_string(manifest.join("Cargo.toml")).expect("no manifest");

    let include = toml
        .split_once("include = [")
        .expect("the manifest names no `include`, so everything ships and this check is moot")
        .1
        .split_once(']')
        .expect("`include` is not closed")
        .0;

    let shipped: Vec<&str> = include
        .split('"')
        .filter(|s| s.starts_with("tests/") && s.ends_with(".rs"))
        .collect();

    assert!(
        !shipped.is_empty(),
        "no test file is named in `include`, so this check would hold over an \
         empty set. If that is deliberate, delete this test rather than \
         leaving it to pass on nothing."
    );

    // What a tarball does not have. `ante.toml` is excluded on purpose and the
    // repository is not shipped at all, so a test touching either cannot run
    // where the crate is consumed.
    const ABSENT: [&str; 2] = ["ante.toml", "\"git\""];

    for file in &shipped {
        let body = std::fs::read_to_string(manifest.join(file))
            .unwrap_or_else(|e| panic!("`include` names {file}, which is not readable: {e}"));

        for needle in ABSENT {
            assert!(
                !body.contains(needle),
                "{file} is published and reaches for {needle}, which an unpacked \
                 tarball does not have. Either drop it from `include` because it \
                 is a check about this repository, or stop it depending on the \
                 repository because it is a check about the crate."
            );
        }
    }

    // The control. Without it the loop passes on any needle nothing contains,
    // including a typo, and this file itself is the proof that the needles are
    // findable in a real test body.
    let own = std::fs::read_to_string(file!()).expect("this file is not readable");
    for needle in ABSENT {
        assert!(
            own.contains(needle),
            "the needle {needle} was not found even in a file that plainly uses \
             it, so the loop above would clear every file it looked at"
        );
    }
}

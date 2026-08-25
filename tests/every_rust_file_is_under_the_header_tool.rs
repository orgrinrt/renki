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

/// `include` ships exactly the tests that can run without the repository.
///
/// Two kinds of test live under `tests/`, and they want opposite answers. One
/// checks the crate and belongs in the package. The other checks this
/// repository: it reads a config the package leaves out, or asks git or rustup
/// something only a checkout can answer. Shipping one of those produces a
/// crate whose own suite fails on something that was never meant to be there,
/// and `cargo package` will not catch it, because packaging compiles the tests
/// and never runs them.
///
/// Checked in both directions, so neither a repository check sneaking into
/// `include` nor a crate test quietly dropped from it goes unreported.
#[test]
fn include_ships_the_crate_tests_and_none_of_the_repository_ones() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let toml = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("no manifest");

    let include = toml
        .split_once("include = [")
        .expect("the manifest names no `include`, so everything ships and this check is moot")
        .1
        .split_once(']')
        .expect("`include` is not closed")
        .0;

    let entries: Vec<&str> = include
        .split('"')
        .filter(|s| s.starts_with("tests/") && s.ends_with(".rs"))
        .collect();

    // A glob under `tests/` defeats the whole check: it matches every file
    // including the ones that must not ship, and it reads as a set of names to
    // anything comparing strings, so the comparison below silently finds
    // nothing wrong. It is also the thing being forbidden on its own merits,
    // since deciding per file is the point.
    let globbed: Vec<&&str> = entries.iter().filter(|e| e.contains('*')).collect();
    assert!(
        globbed.is_empty(),
        "`include` reaches into `tests/` with a glob: {globbed:?}\n\
         A glob cannot tell a crate test from a repository check, so it ships \
         both. Name the files that belong in the package instead."
    );

    let shipped: BTreeSet<String> = entries.into_iter().map(str::to_owned).collect();

    // What an unpacked tarball does not have: `ante.toml` is excluded on
    // purpose, and neither the repository nor a rustup toolchain list is in
    // there at all.
    const ABSENT: [&str; 3] = ["ante.toml", "\"git\"", "\"rustup\""];

    let mut every = Vec::new();
    let mut repository_only = BTreeSet::new();
    for entry in std::fs::read_dir(manifest_dir.join("tests")).expect("no tests directory") {
        let path = entry.expect("unreadable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = format!("tests/{}", path.file_name().unwrap().to_string_lossy());
        let body = std::fs::read_to_string(&path).expect("unreadable test");
        if ABSENT.iter().any(|n| body.contains(n)) {
            repository_only.insert(name.clone());
        }
        every.push(name);
    }

    assert!(
        !every.is_empty(),
        "no test files were found at all, so both assertions below would hold \
         over an empty set and say nothing"
    );

    let wrongly_shipped: Vec<&String> = shipped.intersection(&repository_only).collect();
    assert!(
        wrongly_shipped.is_empty(),
        "these are published and reach for the repository, which an unpacked \
         tarball does not have:\n  {}\n\
         Either drop each from `include` because it is a check about this \
         repository, or stop it depending on the repository because it is a \
         check about the crate.",
        wrongly_shipped
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    let wrongly_left_out: Vec<&String> = every
        .iter()
        .filter(|n| !repository_only.contains(*n) && !shipped.contains(*n))
        .collect();
    assert!(
        wrongly_left_out.is_empty(),
        "these test the crate rather than the repository and are not in \
         `include`, so the package ships the code without the check on \
         it:\n  {}",
        wrongly_left_out
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    // The control. Without it both assertions pass on needles nothing
    // contains, including a typo, and every test would classify as a crate
    // test. Each needle has to be live in at least one real test body.
    for needle in ABSENT {
        let found = every.iter().any(|n| {
            std::fs::read_to_string(manifest_dir.join(n)).is_ok_and(|b| b.contains(needle))
        });
        assert!(
            found,
            "the needle {needle} matches no test body here, so it classifies \
             nothing and the two assertions above are that much weaker"
        );
    }

    assert!(
        !repository_only.is_empty(),
        "nothing classified as a repository check, so the first assertion held \
         over an empty intersection and checked nothing"
    );
}

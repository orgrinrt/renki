//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What the readme says about installing this crate is what the manifest ships.
//!
//! A readme is the one surface a stranger reads before anything else, and the
//! install block is the one part of it they copy rather than read. A version in
//! it that the crate does not ship sends them to a release that either does not
//! exist or is not the one the rest of the page describes, and nothing catches
//! that: the doctest compiles the usage example and has nothing to say about a
//! fenced `toml` block, and `cargo package` does not read prose.
//!
//! It ships, because it can. Both files it reads are in the package: cargo puts
//! `Cargo.toml` in every tarball and copies in whatever `readme` names. So the
//! check travels with the thing it checks, and a consumer running the suite gets
//! the same answer we do.

use std::path::Path;

/// The `version = "..."` of the `[package]` table, which is the first one.
fn package_version(manifest: &str) -> &str {
    let (_, rest) = manifest
        .split_once("\nversion = \"")
        .expect("the manifest has no version");
    rest.split_once('"').expect("the version is not closed").0
}

/// Every `renki = "..."` a readme names, in a dependency block or in prose.
fn versions_the_readme_names(readme: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = readme;
    while let Some(at) = rest.find("renki = \"") {
        let tail = &rest[at + "renki = \"".len()..];
        let (v, after) = tail.split_once('"').expect("an unclosed version");
        out.push(v);
        rest = after;
    }
    out
}

#[test]
fn every_version_the_readme_names_is_the_one_the_manifest_ships() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("no readme");

    let shipped = package_version(&manifest);
    let named = versions_the_readme_names(&readme);

    assert!(
        !named.is_empty(),
        "the readme names no version to install, so every assertion below held \
         over an empty set. It carried an `Installation` section when this was \
         written and losing it is the thing to notice, not to pass over."
    );
    for v in &named {
        assert_eq!(
            *v, shipped,
            "the readme tells a reader to install renki {v} and the manifest \
             ships {shipped}"
        );
    }
}

#[test]
fn the_readme_does_not_promise_a_binary_this_crate_has_none_of() {
    // The tagline said "renki is the launcher half" and the body corrected it a
    // hundred lines later with "you don't install renki itself as a command".
    // There is no `[[bin]]` and no `src/main.rs`, so the correction was the true
    // half and a reader who stopped at the tagline was misled.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");
    let readme = std::fs::read_to_string(root.join("README.md")).expect("no readme");

    let ships_a_binary = declares_a_bin(&manifest) || root.join("src/main.rs").exists();
    assert!(
        !ships_a_binary,
        "this crate ships a binary now, so the readme may say so and this check \
         is the thing that is out of date"
    );
    assert!(
        !readme.contains("cargo install renki"),
        "the readme tells a reader to `cargo install renki`, which installs \
         nothing: there is no binary target"
    );
}

/// Whether the manifest declares a binary target.
///
/// The table header at the start of a line, not the string anywhere. A comment
/// explaining that there is no binary here contains the spelling, and a
/// `contains` reads that comment as the declaration it denies. That is not
/// hypothetical: adding the comment is what turned this red.
fn declares_a_bin(manifest: &str) -> bool {
    manifest
        .lines()
        .any(|l| l.trim_start().starts_with("[[bin]]"))
}

/// The value of a top-level `key = "..."` in the `[package]` table.
fn package_str<'a>(manifest: &'a str, key: &str) -> &'a str {
    let needle = format!("\n{key} = \"");
    let (_, rest) = manifest
        .split_once(&needle)
        .unwrap_or_else(|| panic!("the manifest has no {key}"));
    rest.split_once('"').expect("the value is not closed").0
}

#[test]
fn the_manifest_does_not_sell_a_binary_either() {
    // The readme is not the only place a stranger reads before installing. The
    // description is the line crates.io puts under the name in a search result,
    // and the categories are what they arrive through. Both said utility, which
    // is what `cargo install renki` failing with `no packages found with
    // binaries or examples` was the consequence of.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");

    let description = package_str(&manifest, "description");
    assert!(
        description.starts_with("A library"),
        "the description does not say this is a library, and a reader who takes \
         it for a tool reaches for `cargo install`: {description}"
    );

    let (_, categories) = manifest
        .split_once("\ncategories = [")
        .expect("the manifest names no categories");
    let categories = categories.split_once(']').expect("unclosed categories").0;
    assert!(
        !categories.contains("command-line-utilities"),
        "`command-line-utilities` is where a reader looks for something to \
         install, and there is no binary here: {categories}"
    );
}

#[test]
fn the_checks_above_can_fail() {
    // The control on both. Each reads a document, and a parse that quietly finds
    // nothing passes every assertion it feeds.
    assert_eq!(
        package_version("\nversion = \"9.9.9\"\nname = \"x\"\n"),
        "9.9.9"
    );
    assert_eq!(
        versions_the_readme_names("a\nrenki = \"1.2\"\nb\nrenki = \"3.4\"\n"),
        vec!["1.2", "3.4"]
    );
    assert!(versions_the_readme_names("nothing here").is_empty());
    assert_eq!(
        package_str("\nname = \"x\"\ndescription = \"a thing\"\n", "description"),
        "a thing"
    );
    assert!(declares_a_bin(
        "[package]\nname = \"x\"\n\n[[bin]]\nname = \"x\"\n"
    ));
    assert!(
        !declares_a_bin("# no [[bin]] here, and that is the point\nname = \"x\"\n"),
        "a comment naming the table read as the table"
    );
}

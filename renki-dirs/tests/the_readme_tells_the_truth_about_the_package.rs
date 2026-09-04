//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What the readme says about this crate is what the manifest ships. The same
//! check renki carries for itself, since the prose rots for the same reason:
//! no change is ever about it. Both files are in the package, so a consumer
//! running the suite gets the same answer.

use std::path::Path;

fn read(name: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("no {name}: {e}"))
}

/// The value of one `key = "..."` line in the `[package]` table.
fn package_field<'a>(manifest: &'a str, key: &str) -> &'a str {
    let needle = format!("\n{key} = \"");
    let (_, rest) = manifest
        .split_once(&needle)
        .unwrap_or_else(|| panic!("the manifest has no {key}"));
    rest.split_once('"').expect("the value is not closed").0
}

/// The readme's tagline, the one `> ` line under the badges.
fn tagline(readme: &str) -> &str {
    readme
        .lines()
        .find_map(|l| l.strip_prefix("> "))
        .expect("the readme has no tagline")
        .trim()
}

#[test]
fn the_description_is_the_readme_s_tagline() {
    // crates.io shows the description and the readme on the same page, so the
    // two disagreeing is visible to everybody but us.
    assert_eq!(
        package_field(&read("Cargo.toml"), "description"),
        tagline(&read("README.md"))
    );
}

#[test]
fn every_version_the_readme_names_is_the_one_the_manifest_ships() {
    let manifest = read("Cargo.toml");
    let readme = read("README.md");
    let shipped = package_field(&manifest, "version");
    let mut named = Vec::new();
    let mut rest = readme.as_str();
    while let Some(at) = rest.find("renki-dirs = \"") {
        let tail = &rest[at + "renki-dirs = \"".len() ..];
        let (v, after) = tail.split_once('"').expect("an unclosed version");
        named.push(v);
        rest = after;
    }
    assert!(!named.is_empty(), "the readme names no version to install");
    for v in named {
        assert_eq!(
            v, shipped,
            "the readme installs renki-dirs {v}, the manifest ships {shipped}"
        );
    }
}

#[test]
fn the_readme_s_usage_block_names_only_items_the_crate_exports() {
    // the example is not compiled by anything, so at least its import line is
    // checked against the crate's surface
    let readme = read("README.md");
    let line = readme
        .lines()
        .find(|l| l.starts_with("use renki_dirs::{"))
        .expect("the usage block imports nothing from the crate");
    let items = line
        .trim_start_matches("use renki_dirs::{")
        .trim_end_matches("};");
    let lib = read("src/lib.rs");
    for item in items.split(',').map(str::trim) {
        let exported = lib.contains(&format!("pub struct {item}"))
            || lib.contains(&format!("pub type {item}"))
            || lib.contains(&format!("pub enum {item}"))
            || lib.contains(&format!("{item}, \""));
        assert!(
            exported,
            "the readme imports `{item}`, which the crate does not export"
        );
    }
}

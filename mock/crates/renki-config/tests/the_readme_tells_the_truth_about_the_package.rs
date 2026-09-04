//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What the readme says about this crate is what the manifest ships. The same
//! check the two crates beside this carry, since the prose rots for the same
//! reason in all three: no change is ever about it. Both files are in the
//! package, so a consumer running the suite gets the same answer.

use std::path::Path;

fn read(name: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(root.join(name)).unwrap_or_else(|e| panic!("no {name}: {e}"))
}

/// Every source file of the crate, joined, for the export check below.
fn every_source() -> String {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = String::new();
    for entry in std::fs::read_dir(&src).expect("the crate has a src directory") {
        let path = entry.expect("a readable entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            out.push_str(&std::fs::read_to_string(&path).expect("a readable source file"));
            out.push('\n');
        }
    }
    out
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
    while let Some(at) = rest.find("renki-config = \"") {
        let tail = &rest[at + "renki-config = \"".len() ..];
        let (v, after) = tail.split_once('"').expect("an unclosed version");
        named.push(v);
        rest = after;
    }
    assert!(!named.is_empty(), "the readme names no version to install");
    for v in named {
        assert_eq!(
            v, shipped,
            "the readme installs renki-config {v}, the manifest ships {shipped}"
        );
    }
}

#[test]
fn the_readme_s_usage_block_names_only_items_the_crate_exports() {
    // The block itself compiles as a doctest through `lib.rs`; this is the
    // cheaper half, run without a doctest harness, and it reads the import
    // line against what the sources declare.
    let readme = read("README.md");
    let line = readme
        .lines()
        .find(|l| l.starts_with("use renki_config::{"))
        .expect("the usage block imports nothing from the crate");
    let items = line
        .trim_start_matches("use renki_config::{")
        .trim_end_matches("};");
    let sources = every_source();
    let lib = read("src/lib.rs");
    for item in items.split(',').map(str::trim) {
        let declared = ["pub struct ", "pub trait ", "pub enum ", "pub type ", "macro_rules! "]
            .iter()
            .any(|kw| sources.contains(&format!("{kw}{item}")));
        let exported = lib
            .lines()
            .filter(|l| l.starts_with("pub use ") || l.trim_start().starts_with(item))
            .any(|l| l.contains(item))
            || sources.contains(&format!("macro_rules! {item}"));
        assert!(
            declared && exported,
            "the readme imports `{item}`, which the crate does not declare and export"
        );
    }
    // The control: a name this crate has never had is not read as exported.
    assert!(!every_source().contains("pub struct Widget"));
}

#[test]
fn the_package_ships_its_licence_and_the_files_the_readme_points_at() {
    let manifest = read("Cargo.toml");
    let licence = read("LICENSE");
    assert!(
        licence.contains("Mozilla Public License"),
        "the licence beside the manifest is not the MPL the manifest declares"
    );
    assert_eq!(package_field(&manifest, "license"), "MPL-2.0");
    // and `include` is an allowlist, so the two prose files ship only because
    // it names them
    let include = manifest
        .split_once("\ninclude = [")
        .expect("the manifest has an include list")
        .1;
    let include = include
        .split_once(']')
        .expect("the include list is closed")
        .0;
    for name in ["\"README.md\"", "\"LICENSE\""] {
        assert!(include.contains(name), "include does not name {name}");
    }
}

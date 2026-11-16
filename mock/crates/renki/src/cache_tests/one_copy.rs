//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A finished install leaves the binary in the cache and nothing else: the intermediates go
//! where the repository's cargo builds, and the key's own target is gone.
//!
//! These run a real `cargo install` of a fixture crate with no dependencies, offline.

use std::path::{Path, PathBuf};

use super::*;
use crate::test_fixtures::{fixture_engine, targets_under};

const KEY: &str = "0f0f0f0f0f0f0f0f";

fn path_pin(src: &Path) -> Resolved {
    Resolved {
        pin:         Pin {
            url:       "u".into(),
            reference: Reference::Rev("r".into()),
        },
        key_rev:     "r".into(),
        attempts:    vec![vec!["--path".into(), src.display().to_string(), "--offline".into()]],
        version_tag: String::new(),
    }
}

#[test]
fn an_install_with_a_build_dir_compiles_there_and_leaves_no_target_under_the_cache_root() {
    let tmp = tempfile::tempdir().unwrap();
    let src = fixture_engine(tmp.path());
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".cargo")).unwrap();
    std::fs::write(
        repo.join(".cargo/config.toml"),
        "[build]\nbuild-dir = \"shared\"\n",
    )
    .unwrap();
    let cache = tmp.path().join("cache");

    let bin = ensure_built(&T, &cache, KEY, &path_pin(&src), &repo).unwrap();

    assert!(bin.is_file(), "no binary at {}", bin.display());
    assert_eq!(
        targets_under(&cache).collect::<Vec<_>>(),
        Vec::<PathBuf>::new()
    );
    let shared = repo.join("shared");
    assert!(
        std::fs::read_dir(&shared).is_ok_and(|mut d| d.next().is_some()),
        "nothing was compiled into the repository's build directory {}",
        shared.display()
    );
}

#[test]
fn an_install_with_no_build_dir_removes_its_private_target() {
    let tmp = tempfile::tempdir().unwrap();
    let src = fixture_engine(tmp.path());
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let cache = tmp.path().join("cache");

    let bin = ensure_built(&T, &cache, KEY, &path_pin(&src), &repo).unwrap();

    assert!(bin.is_file(), "no binary at {}", bin.display());
    assert_eq!(
        targets_under(&cache).collect::<Vec<_>>(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn a_built_key_loses_the_target_an_earlier_launcher_left() {
    // The binary is there, so nothing builds; the target beside it is a second
    // copy of everything the build compiled and goes.
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cache");
    let key_dir = builds_dir(&cache).join(KEY);
    std::fs::create_dir_all(key_dir.join("bin")).unwrap();
    std::fs::write(key_dir.join("bin/engine"), b"#!/bin/sh\n").unwrap();
    std::fs::create_dir_all(key_dir.join("target/release/deps")).unwrap();
    let none = Resolved {
        attempts: vec![],
        ..path_pin(tmp.path())
    };

    let bin = ensure_built(&T, &cache, KEY, &none, tmp.path()).unwrap();

    assert!(bin.is_file());
    assert!(
        !key_dir.join("target").exists(),
        "the leftover target stayed"
    );
}

/// An install of the fixture through the backend, its `cargo` carrying a cargo home of its own
/// and `var` where one is given, with no build directory read for the repository.
fn install_with(tmp: &Path, var: Option<(&str, String)>) {
    let src = fixture_engine(tmp);
    let repo = tmp.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let into = tmp.join("cache/builds").join(KEY);
    let home = tmp.join("home");
    let plan = crate::extension::CargoPlan {
        attempts:   path_pin(&src).attempts,
        bin:        "engine".into(),
        crate_name: "engine".into(),
        target_dir: into.join("target"),
        build_dir:  None,
        dir:        Some(repo),
    };
    crate::extension::Cargo::build_with(&plan, &into, || {
        let mut cmd = std::process::Command::new("cargo");
        cmd.env("CARGO_HOME", &home);
        if let Some((name, value)) = &var {
            cmd.env(name, value);
        }
        cmd
    })
    .unwrap();
    assert!(into.join("bin/engine").is_file());
    assert_eq!(
        targets_under(&tmp.join("cache")).collect::<Vec<_>>(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn a_path_hash_in_the_cargo_homes_config_does_not_reach_the_install() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let hashed = tmp.path().join("hashed");
    std::fs::write(
        home.join("config.toml"),
        format!(
            "[build]\nbuild-dir = \"{}/{{workspace-path-hash}}\"\n",
            hashed.display()
        ),
    )
    .unwrap();
    install_with(tmp.path(), None);
    assert!(!hashed.exists(), "cargo built into {}", hashed.display());
}

#[test]
fn a_path_hash_in_an_inherited_variable_does_not_reach_the_install() {
    let tmp = tempfile::tempdir().unwrap();
    let hashed = tmp.path().join("hashed");
    let value = format!("{}/{{workspace-path-hash}}", hashed.display());
    install_with(tmp.path(), Some(("CARGO_BUILD_BUILD_DIR", value)));
    assert!(!hashed.exists(), "cargo built into {}", hashed.display());
}

/// A toolchain no rustup has, standing in for the one a caller's cargo pinned.
const CALLERS_TOOLCHAIN: &str = "renki-no-such-toolchain";

/// Two launchers of one pin, one started under a cargo pinned to another toolchain and one
/// from a plain shell, key it the same, so the pin is built once.
#[test]
fn the_key_reads_the_same_toolchain_whatever_toolchain_the_caller_picked() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = fingerprint_of(std::process::Command::new("rustc"), Some(tmp.path()));
    let mut pinned = std::process::Command::new("rustc");
    pinned.env("RUSTUP_TOOLCHAIN", CALLERS_TOOLCHAIN);
    assert!(
        !plain.is_empty(),
        "no rustc answered in {}",
        tmp.path().display()
    );
    assert_eq!(fingerprint_of(pinned, Some(tmp.path())), plain);
}

/// The build follows the same toolchain the key read, not the one the caller's cargo pinned.
#[test]
fn an_install_under_a_callers_toolchain_builds_with_the_one_the_key_read() {
    let tmp = tempfile::tempdir().unwrap();
    install_with(
        tmp.path(),
        Some(("RUSTUP_TOOLCHAIN", CALLERS_TOOLCHAIN.into())),
    );
}

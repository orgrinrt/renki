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

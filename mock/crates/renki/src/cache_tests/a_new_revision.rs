//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A revision installed from git over a build directory an earlier revision compiled in runs
//! its own sources, not the earlier one's units.
//!
//! These run real `cargo install --git` of a fixture repository on the local disk: two path
//! packages, `engine` printing `word::WORD`, and only `word` changing between revisions, so a
//! stale unit of either package shows in what the binary prints.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::extension::{Cargo, CargoPlan};

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Commit `word` as the word of the fixture repository and answer the revision.
fn commit_word(repo: &Path, word: &str) -> String {
    std::fs::write(
        repo.join("word/src/lib.rs"),
        format!("pub const WORD: &str = \"{word}\";\n"),
    )
    .unwrap();
    git(repo, &["add", "Cargo.toml", "Cargo.lock", "engine", "word"]);
    git(repo, &["commit", "-q", "-m", word]);
    git(repo, &["rev-parse", "HEAD"])
}

/// A repository of the two packages, its lockfile committed for `--locked`.
fn fixture_repo(at: &Path) -> PathBuf {
    let repo = at.join("origin");
    std::fs::create_dir_all(repo.join("engine/src")).unwrap();
    std::fs::create_dir_all(repo.join("word/src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[workspace]\nmembers = [\"engine\", \"word\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("engine/Cargo.toml"),
        "[package]\nname = \"engine\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [dependencies]\nword = { path = \"../word\" }\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("engine/src/main.rs"),
        "fn main() { println!(\"{}\", word::WORD); }\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("word/Cargo.toml"),
        "[package]\nname = \"word\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"engine\"\nversion = \"0.0.0\"\n\
         dependencies = [\n \"word\",\n]\n\n[[package]]\nname = \"word\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    git(&repo, &["init", "-q"]);
    repo
}

/// Install `rev` of `repo` into its own key over the shared `build`, with a cargo home of the
/// test's own, and answer what the installed engine prints.
fn install(tmp: &Path, repo: &Path, rev: &str, build: &Path) -> String {
    let into = tmp.join("cache/builds").join(&rev[.. 12]);
    let plan = CargoPlan {
        attempts:   vec![vec![
            "--git".into(),
            format!("file://{}", repo.display()),
            "--rev".into(),
            rev.into(),
            "engine".into(),
        ]],
        bin:        "engine".into(),
        crate_name: "engine".into(),
        target_dir: into.join("target"),
        build_dir:  Some(build.to_path_buf()),
        dir:        Some(tmp.to_path_buf()),
    };
    let home = tmp.join("home");
    Cargo::build_with(
        &plan,
        &into,
        || {
            let mut cmd = Command::new("cargo");
            cmd.env("CARGO_HOME", &home);
            cmd
        },
        |_| None,
    )
    .unwrap();
    let out = Command::new(into.join("bin/engine")).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn a_new_revision_over_a_shared_build_dir_runs_its_own_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture_repo(tmp.path());
    let one = commit_word(&repo, "one");
    let two = commit_word(&repo, "two");
    let build = tmp.path().join("shared");

    assert_eq!(install(tmp.path(), &repo, &one, &build), "one");
    assert_eq!(
        install(tmp.path(), &repo, &two, &build),
        "two",
        "the second revision ran the first one's units out of the shared build directory"
    );
}

#[test]
fn going_back_to_an_earlier_revision_runs_that_revisions_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = fixture_repo(tmp.path());
    let one = commit_word(&repo, "one");
    let two = commit_word(&repo, "two");
    let build = tmp.path().join("shared");

    assert_eq!(install(tmp.path(), &repo, &two, &build), "two");
    assert_eq!(install(tmp.path(), &repo, &one, &build), "one");
}

#[test]
fn the_path_units_of_an_install_are_read_by_name_and_freshness() {
    let messages = concat!(
        r#"{"reason":"compiler-artifact","package_id":"path+file:///h/git/checkouts/r-1/abc/word#0.0.0","fresh":true}"#,
        "\n",
        r#"{"reason":"compiler-artifact","package_id":"path+file:///h/git/checkouts/r-1/abc/crates/x#homma-api@0.0.1","fresh":false}"#,
        "\n",
        r#"{"reason":"compiler-artifact","package_id":"registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0","fresh":true}"#,
        "\n",
        r#"{"reason":"build-script-executed","package_id":"path+file:///h/git/checkouts/r-1/abc/y#y@0.0.1"}"#,
        "\n",
        r#"{"reason":"build-finished","success":true}"#,
    );
    assert_eq!(crate::extension::path_units(messages), vec![
        ("word".to_owned(), true),
        ("homma-api".to_owned(), false),
    ]);
}

#[test]
fn forgetting_units_removes_the_named_packages_release_fingerprints_and_nothing_else() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let kept = [
        "release/.fingerprint/word-extra-0a1b",
        "release/.fingerprint/serde-0a1b",
        "debug/.fingerprint/word-0a1b",
    ];
    let gone = [
        "release/.fingerprint/word-0a1b2c3d",
        "release/.fingerprint/engine-ffee",
        "x86_64-unknown-linux-gnu/release/.fingerprint/word-0a1b",
    ];
    for unit in kept.iter().chain(&gone) {
        std::fs::create_dir_all(dir.join(unit)).unwrap();
    }
    crate::extension::forget_units(dir, &["word".to_owned(), "engine".to_owned()]);
    for unit in kept {
        assert!(dir.join(unit).is_dir(), "{unit} was removed");
    }
    for unit in gone {
        assert!(!dir.join(unit).exists(), "{unit} stayed");
    }
}

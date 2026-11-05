//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The resolved pin and the engine built for it: what a build that fails does to the
//! branch resolution it was built from. Split from the crate's tests by subject; the
//! fixture is the parent module's.

use std::path::{Path, PathBuf};

use super::*;
use crate::pin::{Pin, Reference, branch_resolution_path};
use crate::retry::BuildError;

fn pin(reference: Reference) -> Pin {
    Pin {
        url: "u".into(),
        reference,
    }
}

fn dev() -> Pin {
    pin(Reference::Branch("dev".into()))
}

/// A resolution of `branch` on `u` recorded a moment ago, as a run an hour earlier would have
/// left, naming `sha`.
fn recorded(cache: &Path, branch: &str, sha: &str) -> PathBuf {
    let path = branch_resolution_path(cache, "u", branch);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\n{sha}\n", now_secs())).unwrap();
    path
}

fn failing(
    error: BuildError,
) -> impl FnOnce(&Tool, &Path, &str, &Resolved, &Path) -> Result<PathBuf, BuildError> {
    move |_, _, _, _, _| Err(error)
}

#[test]
fn a_refused_build_drops_its_branch_resolution_and_no_other_so_the_next_run_asks_the_remote() {
    // The branch was fixed after the broken revision was resolved, but the resolution lasted
    // its hour and every run built the same broken revision.
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path(), "dev", "feedface99c0ffee");
    let other = recorded(cache.path(), "main", "badc0ffee0ddf00d");

    let err = resolved_and_built(
        &T,
        &dev(),
        cache.path(),
        root.path(),
        failing(BuildError::Refused("the build broke".into())),
    )
    .err()
    .unwrap();
    assert_eq!(err, "the build broke");
    assert!(
        !path.exists(),
        "the resolution of the refused revision was kept"
    );
    assert!(other.exists(), "another branch's resolution went with it");

    // `u` is no remote, so what the next run does is ask one and fail, which it could not do
    // while the recorded revision was still fresh.
    let next = crate::pin::resolve(&T, &dev(), cache.path()).err().unwrap();
    assert!(next.contains("ls-remote"), "{next}");
}

#[test]
fn a_refused_build_keeps_a_resolution_another_launcher_recorded_while_it_built() {
    // The record names the tip this launcher resolved when the build starts, and another
    // launcher resolves the branch again before the refusal comes back. What is forgotten is
    // the refused revision, so the newer record stays.
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path(), "dev", "feedface99c0ffee");

    let err = resolved_and_built(
        &T,
        &dev(),
        cache.path(),
        root.path(),
        |_, cache_root, _, _, _| {
            recorded(cache_root, "dev", "0123456789abcdef");
            Err(BuildError::Refused("the build broke".into()))
        },
    )
    .err()
    .unwrap();
    assert_eq!(err, "the build broke");
    let kept = std::fs::read_to_string(&path).unwrap();
    assert!(
        kept.contains("0123456789abcdef"),
        "the newer record went with the refused one: {kept}"
    );
}

#[test]
fn the_message_of_a_refused_rebuild_forgets_the_resolution_like_the_first_build_does() {
    // `dispatch` rebuilds after an eviction through this, so it has no exception of its own.
    let cache = tempfile::tempdir().unwrap();
    let path = recorded(cache.path(), "dev", "feedface99c0ffee");
    let resolved = Resolved {
        pin:         dev(),
        key_rev:     "feedface99c0ffee".into(),
        attempts:    vec![],
        version_tag: String::new(),
    };

    let kept = message_forgetting_a_refusal(
        BuildError::NotStarted("cargo is not on PATH".into()),
        &dev(),
        cache.path(),
        &resolved,
    );
    assert_eq!(kept, "cargo is not on PATH");
    assert!(
        path.exists(),
        "a rebuild that never started cost the resolution"
    );

    let forgot = message_forgetting_a_refusal(
        BuildError::Refused("the build broke".into()),
        &dev(),
        cache.path(),
        &resolved,
    );
    assert_eq!(forgot, "the build broke");
    assert!(!path.exists(), "a refused rebuild kept the resolution");
}

#[test]
fn a_build_that_could_not_start_keeps_the_resolution_it_says_nothing_about() {
    // The control of the test above: it passes against a launcher that forgot on any error.
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path(), "dev", "feedface99c0ffee");

    let err = resolved_and_built(
        &T,
        &dev(),
        cache.path(),
        root.path(),
        failing(BuildError::NotStarted("cargo is not on PATH".into())),
    )
    .err()
    .unwrap();
    assert_eq!(err, "cargo is not on PATH");
    assert!(
        path.exists(),
        "a build that never started cost the resolution"
    );
}

#[test]
fn a_build_that_succeeds_keeps_the_resolution_and_hands_back_what_it_built() {
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path(), "dev", "feedface99c0ffee");

    let built = resolved_and_built(&T, &dev(), cache.path(), root.path(), |_, _, key, _, _| {
        assert!(!key.is_empty());
        Ok(PathBuf::from("bin"))
    })
    .ok()
    .unwrap();
    assert_eq!(built.resolved.key_rev, "feedface99c0ffee");
    assert_eq!(built.bin, PathBuf::from("bin"));
    // Only that the resolution and the toolchain reached the key; what the key is made of is
    // `cache_tests`' to say.
    assert_eq!(
        built.key,
        crate::cache::compute_key("u", "feedface99c0ffee", &built.toolchain),
        "the key is the one the resolution and the toolchain make"
    );
    assert!(path.exists(), "a build that worked cost the resolution");
}

#[test]
fn a_refused_build_of_a_pin_that_names_no_branch_leaves_a_branchs_resolution_alone() {
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path(), "dev", "feedface99c0ffee");

    let err = resolved_and_built(
        &T,
        &pin(Reference::Rev("abc1234".into())),
        cache.path(),
        root.path(),
        failing(BuildError::Refused("the build broke".into())),
    )
    .err()
    .unwrap();
    assert_eq!(err, "the build broke");
    assert!(
        path.exists(),
        "a rev pin's refused build dropped a branch's resolution"
    );
}

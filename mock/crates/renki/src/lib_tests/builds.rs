//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The resolved pin and the engine built for it: what a build that fails does to the
//! branch resolution it was built from. Split from the crate's tests by subject; the
//! fixture is the parent module's.

use std::path::PathBuf;

use super::*;
use crate::pin::{Pin, Reference, branch_resolution_path};

fn pin(reference: Reference) -> Pin {
    Pin {
        url: "u".into(),
        reference,
    }
}

/// A resolution of `dev` on `u` recorded a moment ago, as a run an hour earlier would have left.
fn recorded(cache: &std::path::Path) -> PathBuf {
    let path = branch_resolution_path(cache, "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\nfeedface99c0ffee\n", now_secs())).unwrap();
    path
}

#[test]
fn a_failed_build_drops_the_branch_resolution_so_the_next_run_asks_the_remote_again() {
    // The branch moved on and the broken revision was fixed, but the run kept the
    // resolution for its whole hour and rebuilt the same broken revision.
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path());
    // another branch of the same remote, whose resolution is none of this build's business
    let other = branch_resolution_path(cache.path(), "u", "main");
    std::fs::write(&other, format!("{}\nbadc0ffee0ddf00d\n", now_secs())).unwrap();

    let err = resolved_and_built(
        &T,
        &pin(Reference::Branch("dev".into())),
        cache.path(),
        root.path(),
        |_, _, _, _, _| Err("the build broke".to_string()),
    )
    .err()
    .unwrap();
    assert_eq!(err, "the build broke");
    assert!(!path.exists(), "the resolution of the broken revision was kept");
    assert!(other.exists(), "another branch's resolution went with it");

    // `u` is no remote, so what the next run does is ask one and fail, which it
    // could not do while the recorded revision was still fresh.
    let next = crate::pin::resolve(&T, &pin(Reference::Branch("dev".into())), cache.path())
        .err()
        .unwrap();
    assert!(next.contains("ls-remote"), "{next}");
}

#[test]
fn a_build_that_succeeds_keeps_the_resolution_and_a_pin_that_is_no_branch_leaves_it_alone() {
    // The controls of the test above: it passes against a launcher that dropped the
    // resolution after every run, or after a failed build of any pin at all.
    let cache = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let path = recorded(cache.path());

    let built = resolved_and_built(
        &T,
        &pin(Reference::Branch("dev".into())),
        cache.path(),
        root.path(),
        |_, _, _, _, _| Ok(PathBuf::from("bin")),
    )
    .ok()
    .unwrap();
    assert_eq!(built.0.key_rev, "feedface99c0ffee");
    assert_eq!(built.3, PathBuf::from("bin"));
    assert!(path.exists(), "a build that worked cost the resolution");

    let err = resolved_and_built(
        &T,
        &pin(Reference::Rev("abc1234".into())),
        cache.path(),
        root.path(),
        |_, _, _, _, _| Err("the build broke".to_string()),
    )
    .err()
    .unwrap();
    assert_eq!(err, "the build broke");
    assert!(
        path.exists(),
        "a rev pin's failed build dropped a branch's resolution"
    );
}

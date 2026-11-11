//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What `ensure_built` answers when a build fails: a build that was refused and one that never
//! started, the two a launcher reads to decide whether to forget a branch resolution. Split from
//! the cache tests by subject; the fixture is the parent module's.

use super::*;

fn resolved_with(attempts: Vec<Vec<String>>) -> Resolved {
    Resolved {
        pin: Pin {
            url:       "u".into(),
            reference: Reference::Rev("r".into()),
        },
        key_rev: "r".into(),
        attempts,
        version_tag: String::new(),
    }
}

#[test]
fn a_build_with_nothing_to_try_never_started_and_is_not_a_refusal() {
    // What `resolved_and_built` reads to decide whether to forget a branch resolution: nothing
    // ran, so the revision was refused by nothing.
    let dir = tempfile::tempdir().unwrap();
    let err = ensure_built(
        &T,
        dir.path(),
        "feedfacefeedface",
        &resolved_with(vec![]),
        dir.path(),
    )
    .unwrap_err();
    assert!(!err.is_refused(), "{err:?}");
}

#[test]
fn a_cache_directory_that_cannot_be_made_is_a_build_that_never_started() {
    // The cache root is a file, so the key's directory cannot be made under it. The attempt
    // list is not empty, so the only thing that can answer `NotStarted` here is the directory.
    let dir = tempfile::tempdir().unwrap();
    let a_file = dir.path().join("a-file");
    std::fs::write(&a_file, "").unwrap();
    let attempt = vec!["--path".to_string(), dir.path().display().to_string()];
    let err = ensure_built(
        &T,
        &a_file,
        "feedfacefeedface",
        &resolved_with(vec![attempt]),
        dir.path(),
    )
    .unwrap_err();
    assert!(!err.is_refused(), "{err:?}");
}

#[test]
fn a_build_lock_that_cannot_be_taken_is_a_build_that_never_started() {
    // The lock's path is a directory, so it cannot be opened for writing, and nothing was
    // built. Again with an attempt to run, so that the empty list is not what answers.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("builds").join("feedfacefeedface.lock")).unwrap();
    let attempt = vec!["--path".to_string(), dir.path().display().to_string()];
    let err = ensure_built(
        &T,
        dir.path(),
        "feedfacefeedface",
        &resolved_with(vec![attempt]),
        dir.path(),
    )
    .unwrap_err();
    assert!(!err.is_refused(), "{err:?}");
}

#[test]
fn a_build_cargo_refuses_is_a_refusal_through_the_whole_way_out() {
    // The control of the test above, and the one place the typed failure is read off a real
    // cargo: an install from a path with no package in it is refused by cargo at once, on both
    // attempts, and no network is touched.
    let dir = tempfile::tempdir().unwrap();
    let nowhere = dir.path().join("no-package-here");
    let attempt = vec!["--path".to_string(), nowhere.display().to_string()];
    let err = ensure_built(
        &T,
        dir.path(),
        "feedfacefeedface",
        &resolved_with(vec![attempt]),
        dir.path(),
    )
    .unwrap_err();
    assert!(err.is_refused(), "{err:?}");
}

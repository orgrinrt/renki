//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A build superseded by the run that re-resolved its repository goes at once, not on the
//! daily pass.

use super::*;

const T: Tool = Tool {
    short: "t",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "tns",
    default_url: "u",
    launcher_crate: "t-launcher",
    ..Tool::CONVENTIONS
};

const FIRST: &str = "1111111111111111";
const SECOND: &str = "2222222222222222";

fn installed(cache_root: &Path, key: &str) -> PathBuf {
    let bin = cache_root.join("builds").join(key).join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("engine"), b"#!/bin/sh\n").unwrap();
    bin.join("engine")
}

fn run(cache_root: &Path, state_root: &Path, repo: &Path, rev: &str, key: &str) {
    let pin = Pin {
        url:       "u".into(),
        reference: Reference::Branch("dev".into()),
    };
    let resolved = Resolved {
        pin:         pin.clone(),
        key_rev:     rev.into(),
        attempts:    vec![],
        version_tag: String::new(),
    };
    record_and_collect(
        &T,
        cache_root,
        state_root,
        repo,
        repo,
        &pin,
        PinSource::Config,
        &resolved,
        "tc",
        key,
    );
}

#[test]
fn installing_two_pins_in_turn_leaves_one_binary() {
    // A branch pin that moved within the day: the daily pass has run already, so
    // only the recording itself can remove the revision the repository left.
    let cache = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let path = registry_path(state.path());
    Registry {
        last_gc: crate::now_secs(),
        ..Registry::default()
    }
    .save(&path);

    let first = installed(cache.path(), FIRST);
    run(cache.path(), state.path(), repo.path(), "r1", FIRST);
    assert!(first.is_file(), "the build in use stays");

    let second = installed(cache.path(), SECOND);
    run(cache.path(), state.path(), repo.path(), "r2", SECOND);
    assert!(second.is_file(), "the build in use stays");
    assert!(
        !cache.path().join("builds").join(FIRST).exists(),
        "the superseded build is still on disk"
    );
    let reg = Registry::load(&path);
    let keys: Vec<&str> = reg.builds.iter().map(|b| b.key.as_str()).collect();
    assert_eq!(keys, [SECOND]);
}

#[test]
fn a_build_another_repository_still_resolves_to_is_kept() {
    // The other half: one binary per pin in use, so a key another live repository
    // still resolves to is not superseded by this one moving on.
    let cache = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let path = registry_path(state.path());
    Registry {
        last_gc: crate::now_secs(),
        ..Registry::default()
    }
    .save(&path);

    let first = installed(cache.path(), FIRST);
    run(cache.path(), state.path(), one.path(), "r1", FIRST);
    run(cache.path(), state.path(), two.path(), "r1", FIRST);
    let second = installed(cache.path(), SECOND);
    run(cache.path(), state.path(), one.path(), "r2", SECOND);
    assert!(first.is_file(), "a key another repository resolves to went");
    assert!(second.is_file());
}

#[test]
fn a_superseded_key_whose_build_lock_is_held_stays_with_its_row() {
    let cache = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    Registry {
        last_gc: crate::now_secs(),
        ..Registry::default()
    }
    .save(&registry_path(state.path()));

    let first = installed(cache.path(), FIRST);
    run(cache.path(), state.path(), repo.path(), "r1", FIRST);
    let building = crate::cache::BuildLock::try_take(&cache.path().join("builds"), FIRST).unwrap();
    assert!(building.is_some());
    installed(cache.path(), SECOND);
    run(cache.path(), state.path(), repo.path(), "r2", SECOND);

    assert!(first.is_file(), "a key being built was removed");
    let reg = Registry::load(&registry_path(state.path()));
    assert!(reg.builds.iter().any(|b| b.key == FIRST), "its row went");
}

#[test]
fn a_recording_waits_for_the_registry_lock() {
    // Another launcher between its load and its save holds the lock; this one
    // does not load, record or remove anything until it is let go.
    let cache = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    let other = crate::cache::BuildLock::hold(state.path(), "registry").unwrap();

    installed(cache.path(), FIRST);
    let (c, s, r) = (
        cache.path().to_path_buf(),
        state.path().to_path_buf(),
        repo.path().to_path_buf(),
    );
    let waiting = std::thread::spawn(move || run(&c, &s, &r, "r1", FIRST)); // lint:allow(no-runtime-spawn) reason: the second launcher the lock test holds, in-process because the call under test is.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        !waiting.is_finished(),
        "the recording ran under another's lock"
    );
    assert!(!registry_path(state.path()).exists());

    drop(other);
    waiting.join().unwrap();
    let reg = Registry::load(&registry_path(state.path()));
    assert_eq!(reg.consumers.len(), 1);
}

#[test]
fn recordings_from_many_launchers_at_once_keep_every_row_and_every_build() {
    // Each launcher its own repository and key; a lost update would drop a row,
    // and a removal working from a stale load would take a build just recorded.
    const N: usize = 16;
    let cache = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let repos: Vec<_> = (0 .. N).map(|_| tempfile::tempdir().unwrap()).collect();
    let keys: Vec<String> = (0 .. N).map(|i| format!("{i:016x}")).collect();
    let bins: Vec<_> = keys.iter().map(|k| installed(cache.path(), k)).collect();

    std::thread::scope(|s| {
        for (repo, key) in repos.iter().zip(&keys) {
            let (c, st) = (cache.path(), state.path());
            s.spawn(move || run(c, st, repo.path(), "r", key));
        }
    });

    let reg = Registry::load(&registry_path(state.path()));
    assert_eq!(reg.consumers.len(), N);
    assert_eq!(reg.builds.len(), N);
    for bin in &bins {
        assert!(bin.is_file(), "{} went", bin.display());
    }
}

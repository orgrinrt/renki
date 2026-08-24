//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::fs;

use super::*;

fn touch_build_dir(cache_root: &Path, key: &str) {
    let d = cache_root.join("builds").join(key).join("bin");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("engine"), b"#!/bin/sh\n").unwrap();
}

#[test]
fn record_upserts_consumer_and_build() {
    let mut r = Registry::default();
    r.record(
        "/r",
        "r",
        "/r/mock",
        "u",
        PinForm::Branch,
        "dev",
        "k1",
        "rev1",
        "tc",
        100,
    );
    r.record(
        "/r",
        "r",
        "/r/mock",
        "u",
        PinForm::Branch,
        "dev",
        "k1",
        "rev1",
        "tc",
        200,
    );
    assert_eq!(r.consumers.len(), 1);
    assert_eq!(r.consumers[0].last_seen, 200);
    assert_eq!(r.builds.len(), 1);
    assert_eq!(r.builds[0].last_used, 200);
    assert_eq!(r.consumers[0].pin_form, "branch");
}

#[test]
fn record_repin_leaves_old_build_orphaned() {
    let mut r = Registry::default();
    r.record(
        "/r",
        "r",
        "/r/mock",
        "u",
        PinForm::Branch,
        "dev",
        "old",
        "r1",
        "tc",
        100,
    );
    // same repo re-pins to a new key: consumer moves, old build stays.
    r.record(
        "/r",
        "r",
        "/r/mock",
        "u",
        PinForm::Version,
        "0.0.1",
        "new",
        "r2",
        "tc",
        200,
    );
    assert_eq!(r.consumers.len(), 1);
    assert_eq!(r.consumers[0].key, "new");
    assert_eq!(r.builds.len(), 2); // old is now orphaned, GC removes it
}

#[test]
fn roundtrips_through_toml() {
    let mut r = Registry::default();
    r.record(
        "/r",
        "r",
        "/r/mock",
        "u",
        PinForm::Rev,
        "abc",
        "k",
        "abc",
        "tc",
        100,
    );
    let dir = tempfile::tempdir().unwrap();
    let path = registry_path(dir.path());
    r.save(&path);
    let back = Registry::load(&path);
    assert_eq!(back.consumers.len(), 1);
    assert_eq!(back.builds.len(), 1);
    assert_eq!(back.consumers[0].root, "/r");
}

#[test]
fn load_missing_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let r = Registry::load(&registry_path(dir.path()));
    assert!(r.consumers.is_empty() && r.builds.is_empty());
}

#[test]
fn gc_removes_orphan_build_but_keeps_pinned() {
    // real key shapes: the delete is guarded on them, so a made-up
    // name would be spared and this would measure the guard instead.
    const PINNED: &str = "1111111111111111";
    const ORPHAN: &str = "2222222222222222";
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // a real repo dir so its consumer is not dropped.
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    touch_build_dir(root, PINNED);
    touch_build_dir(root, ORPHAN);

    let mut r = Registry::default();
    r.record(
        repo.to_str().unwrap(),
        "repo",
        "/m",
        "u",
        PinForm::Branch,
        "dev",
        PINNED,
        "r",
        "tc",
        1000,
    );
    // an orphan build with no consumer at all.
    r.builds.push(Build {
        key: ORPHAN.into(),
        engine_url: "u".into(),
        key_rev: "r".into(),
        toolchain: "tc".into(),
        built_at: 1,
        last_used: 1,
    });

    let removed = r.gc(root, PINNED, 2000);
    assert_eq!(removed, vec![ORPHAN.to_string()]);
    assert!(root.join("builds").join(PINNED).is_dir());
    assert!(!root.join("builds").join(ORPHAN).exists());
}

#[test]
fn gc_evicts_build_whose_consumers_are_all_stale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    touch_build_dir(root, "stalekey");

    let mut r = Registry::default();
    // last_seen far in the past relative to `now`.
    r.record(
        repo.to_str().unwrap(),
        "repo",
        "/m",
        "u",
        PinForm::Branch,
        "dev",
        "stalekey",
        "r",
        "tc",
        1,
    );
    let now = LRU_STALE_SECS + 1000;
    let removed = r.gc(root, "somethingelse", now);
    assert_eq!(removed, vec!["stalekey".to_string()]);
}

#[test]
fn gc_protects_current_key_even_if_stale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    touch_build_dir(root, "current");
    let mut r = Registry::default();
    r.record(
        repo.to_str().unwrap(),
        "repo",
        "/m",
        "u",
        PinForm::Branch,
        "dev",
        "current",
        "r",
        "tc",
        1,
    );
    let now = LRU_STALE_SECS + 1000;
    let removed = r.gc(root, "current", now);
    assert!(removed.is_empty());
    assert!(root.join("builds").join("current").is_dir());
}

#[test]
fn gc_drops_consumer_whose_root_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch_build_dir(root, "k");
    let mut r = Registry::default();
    // consumer root does not exist on disk.
    r.record(
        "/no/such/repo",
        "gone",
        "/m",
        "u",
        PinForm::Branch,
        "dev",
        "k",
        "r",
        "tc",
        1000,
    );
    let removed = r.gc(root, "protect-nothing", 2000);
    assert!(r.consumers.is_empty());
    assert_eq!(removed, vec!["k".to_string()]); // its build is now orphaned
}

#[test]
fn gc_due_throttles() {
    let r = Registry {
        last_gc: 1000,
        ..Default::default()
    };
    assert!(!r.gc_due(1000 + GC_INTERVAL_SECS - 1));
    assert!(r.gc_due(1000 + GC_INTERVAL_SECS));
}

#[test]
fn a_key_that_is_not_ours_never_reaches_the_delete() {
    // the shape `compute_key` writes, and nothing else. Anything that fails
    // this could make `builds_dir.join(key)` denote a directory outside the
    // cache, and what happens there is a recursive delete.
    assert!(is_build_key("0123456789abcdef"));
    assert!(is_build_key(&crate::cache::compute_key("u", "r", "tc")));

    for bad in [
        "../../../etc",
        "a/b",
        "/absolute",
        "..",
        "",
        "0123456789ABCDEF",  // uppercase is not what we write
        "0123456789abcde",   // one short
        "0123456789abcdef0", // one long
        "0123456789abcdeg",  // not hex
        "0123456789abcde/",
    ] {
        assert!(!is_build_key(bad), "{bad:?} passed as a build key");
    }
}

#[test]
fn the_guard_spares_a_directory_outside_the_cache_and_still_evicts_a_real_one() {
    // the property the predicate exists for, exercised through `gc` rather
    // than asserted about the predicate, so a `gc` that stopped calling it
    // is what fails.
    let d = tempfile::tempdir().unwrap();
    let builds = d.path().join("builds");
    let outside = d.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("keepme"), b"x").unwrap();

    let real = crate::cache::compute_key("u", "r", "tc");
    fs::create_dir_all(builds.join(&real)).unwrap();

    let build = |key: String| Build {
        key,
        engine_url: "u".into(),
        key_rev: "r".into(),
        toolchain: "tc".into(),
        built_at: 0,
        last_used: 0,
    };
    let mut reg = Registry {
        builds: vec![build("../outside".into()), build(real.clone())],
        ..Default::default()
    };
    // no consumers, so both rows are orphans and both are evicted.
    let removed = reg.gc(d.path(), "", 10_000_000);

    assert!(
        outside.join("keepme").exists(),
        "a hand-edited key walked out of the cache and deleted a directory"
    );
    assert!(
        !builds.join(&real).exists(),
        "control: a key we actually wrote must still be evicted"
    );
    assert_eq!(removed.len(), 2, "both rows leave the registry either way");
}

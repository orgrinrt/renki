//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::fs;

use super::*;

/// The retention the conventions carry, which is what these cases are written
/// against. Passed explicitly now that it is the tool's rather than the
/// launcher's.
const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const RETENTION_SECS: u64 = 30 * 24 * 60 * 60;

/// A key of the shape this crate actually writes: sixteen lowercase hex
/// characters. `gc` refuses to delete anything else, by design, so a made-up
/// key makes every assertion about a deletion vacuous.
const KEY: &str = "00112233445566aa";

/// Build keys as `is_build_key` wants them: sixteen hex characters. A word
/// here is rejected before any directory is touched, so a test using one
/// asserts about the disk and measures nothing.
const STALE_KEY: &str = "0123456789abcdef";
const CURRENT_KEY: &str = "fedcba9876543210";
/// The key a sweep is running under when the row being swept is somebody
/// else's. It carried [`KEY`]'s value, which was harmless and said nothing:
/// the whole of what the name claims is that it is not [`STALE_KEY`], and a
/// constant equal to the one it is contrasted with makes the sweep that reads
/// it prove nothing. `the_test_keys_are_distinct` is what holds it apart now.
const OTHER_KEY: &str = "aabbccdd00112233";

#[test]
fn the_test_keys_are_distinct() {
    // The control on every sweep below. `gc` decides by comparing the key it
    // is running under against the key on each row, so two of these being the
    // same string turns a test about what survives into a test about nothing,
    // and it does it silently: the assertions still pass.
    let keys = [KEY, STALE_KEY, CURRENT_KEY, OTHER_KEY];
    for (i, a) in keys.iter().enumerate() {
        for b in &keys[i + 1..] {
            assert_ne!(a, b, "two of the test keys are the same string");
        }
    }
}

#[test]
fn a_registry_carrying_a_key_this_version_does_not_know_still_loads() {
    // The claim the schema tag's removal rests on. A future version that needs
    // to tell two shapes apart writes one, and a file it wrote must still be
    // readable here rather than being silently discarded as unparseable, which
    // is what `load` does with an error.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("registry.toml");
    fs::write(&path, "schema = 7\nlast_gc = 41\nsomething_else = \"x\"\n").unwrap();
    let r = Registry::load(&path);
    assert_eq!(
        r.last_gc, 41,
        "a key this version does not know threw the file away"
    );
}

fn touch_build_dir(cache_root: &Path, key: &str) {
    let d = cache_root.join("builds").join(key).join("bin");
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("engine"), b"#!/bin/sh\n").unwrap();
}

#[test]
fn record_upserts_consumer_and_build() {
    let mut r = Registry::default();
    r.record(&Recording {
        root: "/r",
        root_exact: true,
        name: "r",
        workdir: "/r/widget",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: "k1",
        key_rev: "rev1",
        toolchain: "tc",
        now: 100,
    });
    r.record(&Recording {
        root: "/r",
        root_exact: true,
        name: "r",
        workdir: "/r/widget",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: "k1",
        key_rev: "rev1",
        toolchain: "tc",
        now: 200,
    });
    assert_eq!(r.consumers.len(), 1);
    assert_eq!(r.consumers[0].last_seen, 200);
    assert_eq!(r.builds.len(), 1);
    assert_eq!(r.builds[0].last_used, 200);
    assert_eq!(r.consumers[0].pin_form, "branch");
}

#[test]
fn record_repin_leaves_old_build_orphaned() {
    let mut r = Registry::default();
    r.record(&Recording {
        root: "/r",
        root_exact: true,
        name: "r",
        workdir: "/r/widget",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: "old",
        key_rev: "r1",
        toolchain: "tc",
        now: 100,
    });
    // same repo re-pins to a new key: consumer moves, old build stays.
    r.record(&Recording {
        root: "/r",
        root_exact: true,
        name: "r",
        workdir: "/r/widget",
        engine_url: "u",
        form: PinForm::Version,
        pin_value: "0.0.1",
        key: "new",
        key_rev: "r2",
        toolchain: "tc",
        now: 200,
    });
    assert_eq!(r.consumers.len(), 1);
    assert_eq!(r.consumers[0].key, "new");
    assert_eq!(r.builds.len(), 2); // old is now orphaned, GC removes it
}

#[test]
fn roundtrips_through_toml() {
    let mut r = Registry::default();
    r.record(&Recording {
        root: "/r",
        root_exact: true,
        name: "r",
        workdir: "/r/widget",
        engine_url: "u",
        form: PinForm::Rev,
        pin_value: "abc",
        key: "k",
        key_rev: "abc",
        toolchain: "tc",
        now: 100,
    });
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
    r.record(&Recording {
        root: repo.to_str().unwrap(),
        root_exact: true,
        name: "repo",
        workdir: "/m",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: PINNED,
        key_rev: "r",
        toolchain: "tc",
        now: 1000,
    });
    // an orphan build with no consumer at all.
    r.builds.push(Build {
        key: ORPHAN.into(),
        engine_url: "u".into(),
        key_rev: "r".into(),
        toolchain: "tc".into(),
        built_at: 1,
        last_used: 1,
    });

    let removed = r.gc(root, PINNED, RETENTION, 2000);
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
    touch_build_dir(root, STALE_KEY);

    let mut r = Registry::default();
    // last_seen far in the past relative to `now`.
    r.record(&Recording {
        root: repo.to_str().unwrap(),
        root_exact: true,
        name: "repo",
        workdir: "/m",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: STALE_KEY,
        key_rev: "r",
        toolchain: "tc",
        now: 1,
    });
    let now = RETENTION_SECS + 1000;
    let removed = r.gc(root, OTHER_KEY, RETENTION, now);
    assert_eq!(removed, vec![STALE_KEY.to_string()]);
    assert!(
        !root.join("builds").join(STALE_KEY).is_dir(),
        "the row went and the build it names stayed on disk"
    );
}

#[test]
fn gc_protects_current_key_even_if_stale() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let repo = root.join("repo");
    fs::create_dir_all(&repo).unwrap();
    touch_build_dir(root, CURRENT_KEY);
    let mut r = Registry::default();
    r.record(&Recording {
        root: repo.to_str().unwrap(),
        root_exact: true,
        name: "repo",
        workdir: "/m",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: CURRENT_KEY,
        key_rev: "r",
        toolchain: "tc",
        now: 1,
    });
    let now = RETENTION_SECS + 1000;
    let removed = r.gc(root, CURRENT_KEY, RETENTION, now);
    assert!(removed.is_empty());
    assert!(
        root.join("builds").join(CURRENT_KEY).is_dir(),
        "the protected build was collected"
    );
}

#[test]
fn gc_drops_consumer_whose_root_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch_build_dir(root, "k");
    let mut r = Registry::default();
    // consumer root does not exist on disk.
    r.record(&Recording {
        root: "/no/such/repo",
        root_exact: true,
        name: "gone",
        workdir: "/m",
        engine_url: "u",
        form: PinForm::Branch,
        pin_value: "dev",
        key: "k",
        key_rev: "r",
        toolchain: "tc",
        now: 1000,
    });
    let removed = r.gc(root, "protect-nothing", RETENTION, 2000);
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
    let removed = reg.gc(d.path(), "", RETENTION, 10_000_000);

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

#[test]
fn a_root_that_could_not_be_written_exactly_is_not_dropped_for_being_absent() {
    // A path is bytes and this file is TOML. A root that is not valid UTF-8
    // can only be written with the bytes replaced, and the replacement names
    // no file, so the not-a-directory rule dropped the row on every pass.
    // The repo's build then had no pinner, so any other repo's collection
    // deleted it while it was still pinned, and the repo paid a full cold
    // rebuild. Every time, silently, under a message that says "once per
    // version".
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    touch_build_dir(root, KEY);

    let mut r = Registry::default();
    r.record(&Recording {
        root: "/r/\u{fffd}\u{fffd}",
        root_exact: false,
        name: "r",
        workdir: "/r/w",
        engine_url: "u",
        form: PinForm::Version,
        pin_value: "0.1.0",
        key: KEY,
        key_rev: "rev1",
        toolchain: "tc",
        now: 1000,
    });

    // Somebody else's run is what collects, so nothing here is protected.
    let removed = r.gc(root, "another-key", RETENTION, 2000);
    assert!(
        removed.is_empty(),
        "a pinned build was evicted because its repo's path is not text: {removed:?}"
    );
    assert_eq!(r.consumers.len(), 1, "the row was dropped");
    assert!(
        root.join("builds").join(KEY).is_dir(),
        "the build was deleted"
    );

    // and the control, one field apart: a row claiming to be exact, holding
    // the same absent path, is dropped and its build collected. Without this
    // the assertions above pass against a collector that stopped dropping
    // anything at all.
    let mut r = Registry::default();
    r.record(&Recording {
        root: "/r/\u{fffd}\u{fffd}",
        root_exact: true,
        name: "r",
        workdir: "/r/w",
        engine_url: "u",
        form: PinForm::Version,
        pin_value: "0.1.0",
        key: KEY,
        key_rev: "rev1",
        toolchain: "tc",
        now: 1000,
    });
    let removed = r.gc(root, "another-key", RETENTION, 2000);
    assert_eq!(removed, vec![KEY.to_string()]);
    assert!(r.consumers.is_empty());
}

#[test]
fn an_exempt_row_still_ages_out_through_the_retention_window() {
    // The exemption is from one rule, not from collection. A repo that is
    // genuinely gone stops moving `last_seen`, and that is what eventually
    // frees its build, which is the same thing that happens to a repo whose
    // path is ordinary and which nobody has opened in a month.
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    touch_build_dir(root, KEY);

    let mut r = Registry::default();
    r.record(&Recording {
        root: "/r/\u{fffd}\u{fffd}",
        root_exact: false,
        name: "r",
        workdir: "/r/w",
        engine_url: "u",
        form: PinForm::Version,
        pin_value: "0.1.0",
        key: KEY,
        key_rev: "rev1",
        toolchain: "tc",
        now: 1000,
    });

    let removed = r.gc(root, "another-key", RETENTION, 1000 + RETENTION_SECS + 1);
    assert_eq!(removed, vec![KEY.to_string()]);
    assert!(!root.join("builds").join(KEY).exists());
}

#[test]
fn a_legacy_pin_registers_as_legacy_whatever_its_reference_is() {
    let p = Pin {
        url: "u".into(),
        reference: Reference::Rev("abc".into()),
    };
    assert_eq!(
        pin_form_and_value(&p, PinSource::Config),
        (PinForm::Rev, "abc".to_string())
    );
    assert_eq!(
        pin_form_and_value(&p, PinSource::Legacy),
        (PinForm::Legacy, "abc".to_string())
    );
}

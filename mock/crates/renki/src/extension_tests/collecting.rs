//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Collecting materialised tools, and what the reviews of the extension
//! model found. Split from the extension tests by size; the fixtures are the
//! parent module's.

use super::*;

/// A tool tree, marked used now.
fn marked_tool(dir: &Path, name: &str) -> std::path::PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("payload"), "x").unwrap();
    std::fs::write(root.join(".last-used"), b"").unwrap();
    root
}

/// `now`, moved forward by `secs`.
///
/// The clock moves rather than the files. Ageing a marker backwards means
/// setting an mtime, which needs a dependency this crate does not have and
/// behaves differently on a directory; `collect` already takes the time it
/// should judge against, so there is nothing to reach for.
fn later(secs: u64) -> SystemTime {
    SystemTime::now() + std::time::Duration::from_secs(secs)
}

const DAY: u64 = 86400;

#[test]
fn a_tool_nothing_has_used_is_collected_and_a_fresh_one_is_not() {
    // Nothing else reaches `<cache>/tools`: a tool has no registry row to evict
    // on, because it is named by a workspace's configuration rather than
    // resolved through a pin. Without this every superseded revision stayed on
    // disk forever.
    let cache = scratch("collect");
    let tools = cache.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let a = marked_tool(&tools, "aaaa");

    // Nothing goes while the window holds, which is the control: a collector
    // that took everything would pass the second half alone.
    assert!(collect(&cache, std::time::Duration::from_secs(30 * DAY), later(DAY)).is_empty());
    assert!(a.exists());

    let removed = collect(
        &cache,
        std::time::Duration::from_secs(30 * DAY),
        later(90 * DAY),
    );
    assert_eq!(removed, vec!["aaaa".to_string()]);
    assert!(!a.exists(), "the stale tool is still on disk");
}

#[test]
fn a_scratch_from_a_dead_fetch_goes_on_a_much_shorter_rule() {
    // A scratch is a partial tree by definition and is never read, so one that
    // outlived its fetch is one whose process is gone. An hour is far longer
    // than any fetch and short enough that a crashed run does not leave a copy
    // of a repository sitting until the retention window.
    let cache = scratch("collect-scratch");
    let tools = cache.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let tool = marked_tool(&tools, "aaaa");
    let scratch_dir = tools.join(".aaaa.999.0");
    std::fs::create_dir_all(&scratch_dir).unwrap();

    // A retention window far longer than the scratch rule, so a scratch judged
    // on the tool rule would survive this and the assertion would fail.
    let removed = collect(
        &cache,
        std::time::Duration::from_secs(365 * DAY),
        later(4 * 3600),
    );

    assert_eq!(removed, vec![".aaaa.999.0".to_string()]);
    assert!(!scratch_dir.exists());
    assert!(tool.exists(), "the tool went with the scratch");
}

#[test]
fn a_tool_with_no_marker_is_stamped_rather_than_evicted() {
    // Everything already on disk predates this mechanism and carries no marker.
    // Reading that as "never used" would evict every tool on the machine the
    // first time a launcher with this code runs.
    let cache = scratch("collect-unmarked");
    let tools = cache.join("tools");
    std::fs::create_dir_all(tools.join("cccc")).unwrap();
    std::fs::write(tools.join("cccc/payload"), "x").unwrap();

    let removed = collect(&cache, std::time::Duration::from_secs(0), later(365 * DAY));

    assert!(
        removed.is_empty(),
        "it evicted an unmarked tool: {removed:?}"
    );
    assert!(tools.join("cccc/payload").is_file());
    assert!(
        tools.join("cccc/.last-used").is_file(),
        "it left the tool unmarked, so the next pass makes the same decision"
    );
}

#[test]
fn locating_a_cached_tool_marks_it_used() {
    // The marker moves on the hit, not only on the fetch. Written once at fetch
    // time it says a tool used every day has not been touched since the day it
    // arrived, and the collector then takes it.
    let cache = scratch("collect-touch");
    let mut d = desc();
    d.backend = "marker".into();

    let at = locate(&d, &registry(), &cache, &cache).unwrap();
    let marker = at.root.join(".last-used");
    assert!(marker.is_file(), "a fresh fetch left no marker");
    let fetched_at = marker.metadata().unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    // The second call is a cache hit, and the hit is what has to move the mark.
    locate(&d, &registry(), &cache, &cache).unwrap();

    assert!(
        marker.metadata().unwrap().modified().unwrap() > fetched_at,
        "a cache hit left the mark where the fetch put it"
    );
}

// --- what the review of the first round found ----------------------------

#[test]
fn the_key_refuses_a_path_source_the_way_locate_does() {
    // `cache_key` is public, and its path arm used to hash a workspace-relative
    // string with an empty rev, which is exactly the collision `locate` refuses:
    // two workspaces each holding a `tools/x` land on one entry. A host reaching
    // the key directly got the collision the refusal exists to prevent.
    let r = registry();
    let b = r.get("marker").unwrap();
    let mut d = desc();
    d.source = Source::Path {
        path: "tools/x".into(),
    };

    let err = cache_key(&d, b).unwrap_err();
    assert!(err.contains("shared by all of them"), "{err}");

    // The control: a git source still keys.
    assert!(cache_key(&desc(), b).is_ok());
}

#[test]
fn a_scratch_this_process_owns_is_never_collected() {
    // A launcher collects on the same run that fetches, so a fetch slow enough
    // to cross the scratch bound would be collected by its own process. The pid
    // is in the name, which is the one case liveness settles cheaply.
    let cache = scratch("collect-own");
    let tools = cache.join("tools");
    std::fs::create_dir_all(&tools).unwrap();

    let mine = tools.join(format!(".aaaa.{}.0", std::process::id()));
    let theirs = tools.join(".aaaa.999999.0");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::create_dir_all(&theirs).unwrap();

    // Far past the bound, so age is not what spares it.
    let removed = collect(
        &cache,
        std::time::Duration::from_secs(365 * DAY),
        later(365 * DAY),
    );

    assert_eq!(removed, vec![".aaaa.999999.0".to_string()]);
    assert!(mine.exists(), "it collected its own in-flight scratch");
    assert!(
        !theirs.exists(),
        "the control survived, so nothing was collected"
    );
}

#[test]
fn a_sha256_object_name_is_accepted() {
    // A repository may use sha-256, whose object names are sixty-four hex. The
    // length check was written for sha-1 alone and refused them.
    let sha256 = "a".repeat(64);
    let ok = with_source(&format!(
        r#"git = {{ url = "https://e.invalid/x.git", rev = "{sha256}" }}"#
    ));
    assert!(ok.is_ok(), "{ok:?}");

    // The controls, on either side: still not a short prefix, and still not an
    // arbitrary length between the two.
    for bad in [40 - 1, 41, 63, 65] {
        let r = with_source(&format!(
            r#"git = {{ url = "https://e.invalid/x.git", rev = "{}" }}"#,
            "a".repeat(bad)
        ));
        assert!(r.is_err(), "{bad} hex was accepted: {r:?}");
    }
}

#[test]
fn locate_and_the_launcher_place_through_one_body() {
    // They used to carry the same `places_itself` branch twice, and the doc on
    // `materialise_once` claimed `locate` called it, which it never did. A
    // precondition added to one would silently not reach the other.
    //
    // Checked by adding one here rather than by reading: `place` is the only
    // thing that refuses a root with no parent, so if either route stopped going
    // through it, that route would stop refusing.
    let places_itself = |_: &Path| Ok(());
    assert!(place(Path::new("/"), false, places_itself).is_err());

    // And the two routes agree on what a backend that places itself gets.
    let base = scratch("one-body");
    let via_generic = base.join("generic");
    materialise_once::<PlacesItself>(&desc(), &via_generic).unwrap();
    let generic_saw = PLACED_IN_PLACE.lock().unwrap().clone();

    let via_registry = base.join("tools").join("registry");
    std::fs::create_dir_all(base.join("tools")).unwrap();
    place(&via_registry, true, |into| {
        PlacesItself::materialise(&desc(), into)
    })
    .unwrap();
    let registry_saw = PLACED_IN_PLACE.lock().unwrap().clone();

    assert_eq!(generic_saw.as_deref(), Some(via_generic.as_path()));
    assert_eq!(registry_saw.as_deref(), Some(via_registry.as_path()));
    assert!(via_generic.join("in-place").is_file());
    assert!(via_registry.join("in-place").is_file());
}

#[test]
fn nothing_but_place_decides_how_material_is_placed() {
    // Finding 1 of the second review, and it is invisible to every runtime test
    // here: `locate` open-coded the same `places_itself` branch that
    // `materialise_once` had, so the two dispatches were separate bodies that
    // happened to agree. The doc three lines above `materialise_once` said
    // `locate` called it, which it never did, and the two had already drifted:
    // one refused a root with no parent and the other derived its parent itself.
    //
    // A source read rather than a behaviour check, because the defect is that
    // two bodies exist rather than that either is wrong. The repository already
    // tests prose this way; this is the same instrument pointed at a branch.
    let src = include_str!("../extension.rs");
    let deciders: Vec<&str> = src
        .lines()
        .filter(|l| l.contains("places_itself") && l.trim_start().starts_with("if "))
        .collect();
    assert_eq!(
        deciders.len(),
        1,
        "the placement branch exists in more than one body, so a precondition \
         added to one does not reach the other: {deciders:?}"
    );

    // And the one that exists is inside `place`. Without this the assertion
    // above passes just as well when the single copy is the one in `locate`.
    //
    // Located rather than matched: an exact match on the signature's text would
    // go red on a rustfmt bump or a renamed parameter, for a reason that is not
    // this defect, and the natural repair then is to paste the new text in, at
    // which point it asserts the file matches itself.
    let opens = src
        .find("fn place(")
        .expect("`place` is gone, so this test is measuring nothing");
    let closes = src[opens ..]
        .find("\n}\n")
        .map(|n| opens + n)
        .expect("`place` does not close");
    let at = src
        .find(deciders[0])
        .expect("the decider line is not in the file it came from");
    assert!(
        (opens .. closes).contains(&at),
        "the placement branch sits outside `place`, at byte {at} against \
         {opens}..{closes}: {}",
        deciders[0].trim()
    );
}

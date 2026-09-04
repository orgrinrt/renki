//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Locating a tool and building its command. Split from the extension tests
//! by size; the fixtures are the parent module's.

use super::*;

// --- locating ------------------------------------------------------------

#[test]
fn an_unknown_backend_names_what_was_available() {
    // A typo in a descriptor is the common case, and a bare "unknown backend"
    // leaves the reader guessing at the spelling.
    let root = scratch("unknown");
    let mut d = desc();
    d.backend = "gti".into();
    let err = locate(&d, &registry(), &root, &root.join("cache")).unwrap_err();
    assert!(err.contains("gti"), "{err}");
    assert!(err.contains("git"), "should list what it has: {err}");
}

#[test]
fn a_path_source_resolves_against_the_workspace_and_is_not_cached() {
    let root = scratch("local");
    std::fs::create_dir_all(root.join("tools/x")).unwrap();
    let mut d = desc();
    d.backend = "local".into();
    d.source = Source::Path {
        path: "tools/x".into(),
    };

    let cache = root.join("cache");
    let at = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(at.root, root.join("tools/x"));
    assert!(
        !cache.exists(),
        "a local tool was copied into the cache, so an edit to it would be invisible"
    );
}

#[test]
fn a_missing_local_directory_is_reported_against_the_path() {
    let root = scratch("absent");
    let mut d = desc();
    d.backend = "local".into();
    d.source = Source::Path {
        path: "tools/absent".into(),
    };
    let err = locate(&d, &registry(), &root, &root.join("cache")).unwrap_err();
    assert!(err.contains("absent"), "{err}");
}

#[test]
fn a_non_caching_backend_refuses_a_git_source() {
    let root = scratch("mismatch");
    let mut d = desc();
    d.backend = "local".into(); // the source is still git
    let err = locate(&d, &registry(), &root, &root.join("cache")).unwrap_err();
    assert!(err.contains("path source"), "{err}");
}

#[test]
fn materialising_happens_once_and_the_second_call_is_a_cache_hit() {
    let root = scratch("cachehit");
    let cache = root.join("cache");
    let mut d = desc();
    d.backend = "marker".into();

    // Counted as a delta rather than against an absolute. The counter is a
    // global and the tests run in parallel threads, so an absolute reading is a
    // claim about every other test that touches this backend, and it broke each
    // time one was added.
    let count = || MATERIALISED.load(std::sync::atomic::Ordering::SeqCst);
    let before = count();

    let first = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(
        std::fs::read_to_string(first.root.join("who")).unwrap(),
        "rules"
    );
    let after_first = count();
    assert!(
        after_first > before,
        "the first call did not fetch at all: {before} -> {after_first}"
    );

    let second = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(second.root, first.root);
    assert_eq!(
        count(),
        after_first,
        "the second call fetched again instead of hitting the cache"
    );
}

#[test]
fn a_failed_materialise_leaves_nothing_behind() {
    // Half a tool in the cache is worse than none: the next run finds the
    // directory, treats it as a hit, and executes an incomplete checkout.
    let root = scratch("failed");
    let cache = root.join("cache");
    let mut d = desc();
    d.backend = "broken".into();

    assert!(locate(&d, &registry(), &root, &cache).is_err());
    let tools = cache.join("tools");
    let left: Vec<_> = std::fs::read_dir(&tools)
        .map(|it| it.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

// --- building the command ------------------------------------------------

#[test]
fn a_command_runs_the_file_the_descriptor_names() {
    let root = scratch("run");
    runnable(&root);
    let at = Located {
        root: root.clone(),
    };
    let cmd = command(&desc(), &at, "list", "renki", &root, &[]).unwrap();
    assert_eq!(cmd.get_program(), root.join("commands/list").as_os_str());
}

#[test]
fn a_command_is_told_which_workspace_it_is_acting_on() {
    // The load bearing part. A tool's code sits in a cache shared by every
    // workspace on the machine and its data does not, so being told is the
    // only way it can know which one it is acting on.
    let root = scratch("ws");
    runnable(&root);
    let ws = root.join("somewhere-else");
    std::fs::create_dir_all(&ws).unwrap();

    let at = Located {
        root: root.clone(),
    };

    // Both variables, under two different hosts, because the names are derived
    // from the host's short name rather than fixed. Asserting one name under one
    // host passes just as well when the derivation is a hardcoded constant, and
    // a constant here is one host's policy in a published crate's contract with
    // every child process anybody spawns.
    for (short, ws_var, root_var) in [
        ("renki", "RENKI_WORKSPACE", "RENKI_TOOL_ROOT"),
        ("mock", "MOCK_WORKSPACE", "MOCK_TOOL_ROOT"),
    ] {
        let cmd = command(&desc(), &at, "list", short, &ws, &[]).unwrap();
        let envs: Vec<(String, Option<std::ffi::OsString>)> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_owned())))
            .collect();
        let got = |want: &str| {
            envs.iter()
                .find(|(k, _)| k == want)
                .and_then(|(_, v)| v.clone())
        };
        assert_eq!(
            got(ws_var).as_deref(),
            Some(ws.as_os_str()),
            "the workspace did not reach the child under {short}: {envs:?}"
        );
        assert_eq!(
            got(root_var).as_deref(),
            Some(root.as_os_str()),
            "the tool root did not reach the child under {short}: {envs:?}"
        );
        // And the other host's names are absent, so a derivation that set both
        // spellings would fail here rather than passing twice.
        for absent in ["HOMMA_WORKSPACE", "RENKI_WORKSPACE", "MOCK_WORKSPACE"] {
            if absent != ws_var {
                assert!(
                    got(absent).is_none(),
                    "{absent} set under {short}: {envs:?}"
                );
            }
        }
        assert_eq!(cmd.get_current_dir(), Some(ws.as_path()));
    }
}

#[test]
fn arguments_are_forwarded() {
    let root = scratch("args");
    runnable(&root);
    let at = Located {
        root: root.clone(),
    };
    let args = vec!["--load".to_string(), "always".to_string()];
    let cmd = command(&desc(), &at, "list", "renki", &root, &args).unwrap();
    let got: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(got, args);
}

#[test]
fn an_unknown_command_lists_the_ones_that_exist() {
    let root = scratch("typo");
    let at = Located {
        root: root.clone(),
    };
    let err = command(&desc(), &at, "shwo", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("shwo"), "{err}");
    assert!(err.contains("list") && err.contains("show"), "{err}");
}

#[test]
fn a_command_whose_file_is_missing_says_so_before_running_anything() {
    let root = scratch("missing");
    let at = Located {
        root: root.clone(),
    };
    let err = command(&desc(), &at, "list", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("commands/list"), "{err}");
}

#[test]
fn a_tool_with_no_commands_says_that_rather_than_listing_nothing() {
    let root = scratch("nocmds");
    let at = Located {
        root: root.clone(),
    };
    let mut d = desc();
    d.commands.clear();
    let err = command(&d, &at, "anything", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("no commands"), "{err}");
}

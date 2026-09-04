//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What a descriptor cannot reach, and one race on one key. Split from the
//! extension tests by size; the fixtures are the parent module's.

use super::*;

/// A descriptor with one command, whose `run` is whatever is under test.
///
/// Built as a struct literal rather than parsed, deliberately: the fields are
/// public and `Deserialize` is derived, so this is a shape a host can hand
/// `command` without ever passing through `parse`, and a check that only runs
/// at parse time guards nothing about it.
fn with_run(run: &str) -> Descriptor {
    let mut d = desc();
    d.commands = vec![CommandDef {
        name:    "go".into(),
        summary: "go".into(),
        run:     run.into(),
    }];
    d
}

#[test]
fn an_absolute_run_cannot_name_an_executable_outside_the_tool() {
    // `Path::join` throws its left side away when the right is absolute, so an
    // unchecked `run` of `/bin/sh` spawns `/bin/sh` and the tool root the
    // descriptor was materialised into is never consulted.
    let root = scratch("run-absolute");
    runnable(&root);
    let at = Located {
        root: root.clone(),
    };

    let err = command(&with_run("/bin/sh"), &at, "go", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("/bin/sh"), "{err}");
    assert!(err.contains("not inside it"), "{err}");
}

#[test]
fn a_run_climbing_out_of_the_tool_is_refused() {
    // The other half of the class. Relative, so `join` keeps the root, and the
    // result is still outside it. `is_file` accepts what `..` resolves to and
    // `Command` runs it.
    let root = scratch("run-climb");
    runnable(&root);
    std::fs::write(root.join("sh"), "#!/bin/sh\n").unwrap();
    let deep = root.join("a/b/c");
    std::fs::create_dir_all(deep.join("commands")).unwrap();
    std::fs::write(deep.join("commands/list"), "#!/bin/sh\n").unwrap();
    let at = Located {
        root: deep.clone(),
    };

    let err = command(&with_run("../../../sh"), &at, "go", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("not inside it"), "{err}");

    // The control: from the same root, a command that stays inside runs.
    let ok = command(&desc(), &at, "list", "renki", &root, &[]);
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn a_run_that_is_a_symlink_out_of_the_tool_is_refused() {
    // A string check cannot see this one: `commands/list` stays inside by every
    // component test and resolves anywhere the link points. The string check
    // and the resolved check are different claims and neither implies the
    // other, so both are made.
    let root = scratch("run-link");
    let outside = scratch("run-link-target");
    std::fs::write(outside.join("sh"), "#!/bin/sh\n").unwrap();
    std::fs::create_dir_all(root.join("commands")).unwrap();
    std::os::unix::fs::symlink(outside.join("sh"), root.join("commands/list")).unwrap();
    let at = Located {
        root: root.clone(),
    };

    let err = command(&desc(), &at, "list", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("outside the tool"), "{err}");
}

#[test]
fn locate_checks_a_descriptor_it_was_handed_rather_than_trusting_it() {
    // Same premise as the run tests. `locate` takes a `&Descriptor`, and the
    // one it gets need never have been parsed.
    let root = scratch("locate-check");
    let mut d = desc();
    d.source = Source::Git {
        url: "--config=core.sshCommand=id".into(),
        rev: "0123456789abcdef0123456789abcdef01234567".into(),
    };
    let err = locate(&d, &registry(), &root, &root).unwrap_err();
    assert!(err.contains("url"), "{err}");
    assert!(
        !root.join("tools").exists(),
        "it got as far as creating the cache before refusing"
    );
}

#[test]
fn an_unknown_field_in_a_descriptor_is_refused_rather_than_ignored() {
    // A typo in a `tool.toml` used to parse to the field's default, so a
    // `promoted = true` silently meant `promote = false` and nothing said so.
    let base = "[tool]\nname=\"x\"\nsummary=\"y\"\nbackend=\"git\"\n\
                [tool.source]\ngit = { url = \"https://e.invalid/x.git\", \
                rev = \"0123456789abcdef0123456789abcdef01234567\" }\n";
    assert!(
        Descriptor::parse(base).is_ok(),
        "the control does not parse"
    );
    assert!(Descriptor::parse(&format!("{base}promoted = true\n")).is_err());
    assert!(Descriptor::parse(&format!("{base}tag = [\"a\"]\n")).is_err());
    assert!(
        Descriptor::parse(&format!(
            "{base}[[tool.commands]]\nname=\"a\"\nsummary=\"b\"\nrun=\"c\"\ndescriptions=\"d\"\n"
        ))
        .is_err()
    );
}

#[test]
fn an_empty_path_is_refused() {
    // It resolves `Located.root` to the workspace root itself, which makes
    // every command's `run` relative to the whole repository.
    assert!(with_source(r#"path = { path = "" }"#).is_err());
}

#[test]
fn a_caching_backend_refuses_a_path_source() {
    // A path is relative to one workspace; the cache is shared by all of them.
    // The key would be a workspace-relative string, so two workspaces each
    // holding a `tools/x` would collide on one entry.
    let root = scratch("cache-path");
    let mut d = desc();
    d.backend = "marker".into();
    d.source = Source::Path {
        path: "tools/x".into(),
    };
    let err = locate(&d, &registry(), &root, &root).unwrap_err();
    assert!(err.contains("shared by all of them"), "{err}");
}

#[test]
fn two_threads_racing_on_one_key_publish_one_fetch_whole() {
    // The scratch name used to be derived from the process id alone, which
    // every thread in it shares. `locate` is `pub` and takes shared references,
    // so two threads on one key wrote into one scratch and the published tree
    // was spliced from both fetches, with both callers returning `Ok`.
    //
    // The backend below makes a splice visible: each thread writes a file named
    // for itself and a shared file holding its own id, after a stagger long
    // enough that an interleave is certain rather than lucky.
    static RACED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    struct Slow;

    impl Backend for Slow {
        type Plan = Descriptor;

        const NAME: &'static str = "slow";

        fn fingerprint() -> String {
            String::new()
        }

        fn materialise(_: &Descriptor, into: &Path) -> Result<(), String> {
            let n = RACED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
            std::fs::write(into.join("manifest"), n.to_string()).map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::fs::write(into.join(format!("payload-{n}")), "x").map_err(|e| e.to_string())
        }
    }

    static SLOW: &[Registered] = &[Registered::of::<Slow>()];
    let root = scratch("race");

    let mut d = desc();
    d.backend = "slow".into();

    std::thread::scope(|s| {
        for _ in 0 .. 2 {
            let (d, root) = (d.clone(), root.clone());
            s.spawn(move || {
                let r = Registry::new(SLOW);
                locate(&d, &r, &root, &root).expect("both callers should succeed");
            });
        }
    });

    let published = locate(&d, &Registry::new(SLOW), &root, &root).unwrap();
    // The last-used marker is the launcher's, not the fetch's, so it is not part
    // of what this counts.
    let mut names: Vec<String> = std::fs::read_dir(&published.root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();

    // Exactly one fetch's output, whichever won. A spliced tree carries the
    // manifest of one and the payload of the other, which is what this saw.
    assert_eq!(names.len(), 2, "the published tree is spliced: {names:?}");
    let manifest = std::fs::read_to_string(published.root.join("manifest")).unwrap();
    assert_eq!(
        names[1],
        format!("payload-{manifest}"),
        "manifest is from one fetch and the payload from another: {names:?}"
    );

    // And nothing is left behind: the loser removes its own scratch and only
    // its own.
    let leftovers: Vec<String> = std::fs::read_dir(root.join("tools"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "scratch left behind: {leftovers:?}");
}

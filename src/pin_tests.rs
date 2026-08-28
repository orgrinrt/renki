//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::tool::Hooks;

const T: Tool = Tool {
    short: "widget",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "t",
    default_url: "u",
    launcher_crate: "t-launcher",
    ..Tool::CONVENTIONS
};

fn pin(r: Reference) -> Pin {
    Pin {
        url:       "u".into(),
        reference: r,
    }
}

#[test]
fn a_version_resolves_to_the_pinned_repository_and_nowhere_else() {
    // The whole of what a version pin means by default. `cargo install engine
    // --version x` resolves `engine` by name against crates.io, and nothing
    // ties that name to the url the config pinned, so a name somebody else
    // holds would install their code and run it as the engine. The url is the
    // only source the repository actually named.
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&T, &pin(Reference::Version("0.0.0-d05".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "v:0.0.0-d05");
    assert_eq!(r.attempts, vec![vec![
        "--git",
        "u",
        "--tag",
        "0.0.0-d05",
        "engine"
    ]]);
    // and the dep a hook would build points at the tag, not the version
    assert_eq!(r.git_ref(), ("tag", "0.0.0-d05"));

    // Stated as a property too, because the assertion above passes for any
    // reordering that still happens to put the git attempt first.
    for a in &r.attempts {
        assert!(
            a.contains(&"--git".to_string()),
            "an attempt with no source selector resolves by name: {a:?}"
        );
    }
}

#[test]
fn a_tool_that_owns_its_crate_name_can_take_the_registry_first() {
    // The opt in, and the only shape in which the registry attempt is
    // correct: the tool has said the name on crates.io is its own.
    const PUBLISHED: Tool = Tool {
        version_source: VersionSource::RegistryThenGitTag,
        ..T
    };
    let d = tempfile::tempdir().unwrap();
    let r = resolve(
        &PUBLISHED,
        &pin(Reference::Version("0.0.0-d05".into())),
        d.path(),
    )
    .unwrap();
    assert_eq!(r.attempts, vec![
        vec!["engine", "--version", "0.0.0-d05"],
        vec!["--git", "u", "--tag", "0.0.0-d05", "engine"],
    ]);
    // the fallback is the point of the ordering: a version the registry has
    // not got is still buildable from the tag.
    assert_eq!(r.git_ref(), ("tag", "0.0.0-d05"));
}

#[test]
fn the_source_choice_reaches_only_the_version_form() {
    // A rev, a tag and a branch all name something inside the pinned
    // repository already, so there was never a second source for them to
    // resolve from and the flag must not invent one. Branch is in here rather
    // than named and skipped: it is the one of the three that goes through a
    // second function to get its rev, so it is the one that could pick the
    // setting up by accident.
    const PUBLISHED: Tool = Tool {
        version_source: VersionSource::RegistryThenGitTag,
        ..T
    };
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\nfeedface99c0ffee\n", unix_now())).unwrap();

    for r in [
        Reference::Rev("abc123".into()),
        Reference::Tag("v1.2.3".into()),
        Reference::Branch("dev".into()),
    ] {
        for t in [&T, &PUBLISHED] {
            let got = resolve(t, &pin(r.clone()), d.path()).unwrap();
            assert_eq!(got.attempts.len(), 1, "{r:?}");
            assert!(got.attempts[0].contains(&"--git".to_string()), "{r:?}");
        }
    }
}

#[test]
fn the_base_answers_the_source_with_the_one_that_needs_no_promise() {
    // The security property of the whole field, stated rather than left to be
    // inferred from every fixture happening to spread the base. Flipping
    // `CONVENTIONS` would otherwise fail a pile of tests for reasons none of
    // them names.
    assert_eq!(Tool::CONVENTIONS.version_source, VersionSource::GitTag);
}

#[test]
fn a_hook_that_names_no_tag_is_refused_rather_than_left_with_nothing_to_try() {
    // Without the registry attempt there is nothing else in the list, so an
    // empty tag list is zero attempts. The build loop then runs zero times and
    // reports a failure that names nothing it tried and blames the pin, which
    // is the one thing that is not wrong.
    const NO_TAGS: Tool = Tool {
        hooks: Hooks {
            version_tags: Some(|_| Vec::new()),
            ..Hooks::NONE
        },
        ..T
    };
    // A url nothing else in the message could supply. The one the rest of this
    // file uses is a single character, and every sentence here contains it, so
    // asserting on it was asserting on the alphabet.
    const URL: &str = "https://forge.example/aardvark-quorum.git";
    let where_it_would_look = Pin {
        url:       URL.into(),
        reference: Reference::Version("0.1.0".into()),
    };
    let d = tempfile::tempdir().unwrap();
    let err = resolve(&NO_TAGS, &where_it_would_look, d.path()).unwrap_err();
    assert!(err.contains("version_tags"), "{err}");
    assert!(err.contains("0.1.0"), "{err}");
    assert!(err.contains(URL), "the url it would have looked in: {err}");

    // and the opt in is refused too, rather than limping on the registry
    // attempt alone. What makes the registry safe to try is the tool asserting
    // it owns the name, which `RegistryThenGitTag` is; the tags behind it are
    // what make the mode useful before the engine has ever been published, and
    // a tool naming none of them has asked for a pin that resolves through
    // tags and then supplied no tag. That is a broken descriptor either way.
    const PUBLISHED_NO_TAGS: Tool = Tool {
        version_source: VersionSource::RegistryThenGitTag,
        ..NO_TAGS
    };
    assert!(resolve(&PUBLISHED_NO_TAGS, &where_it_would_look, d.path()).is_err());

    // the control: the same tool with one tag resolves, so the refusal is
    // about the empty list and not about the hook being set at all.
    const ONE_TAG: Tool = Tool {
        hooks: Hooks {
            version_tags: Some(|v| vec![format!("v{v}")]),
            ..Hooks::NONE
        },
        ..T
    };
    let ok = resolve(&ONE_TAG, &pin(Reference::Version("0.1.0".into())), d.path()).unwrap();
    assert_eq!(ok.attempts.len(), 1);
}

#[test]
fn a_repository_that_prefixes_its_tags_can_say_so() {
    // `v0.1.0` is at least as common a spelling as `0.1.0`, and a tool
    // whose engine repository uses it could not be built from a version pin
    // until that engine published, with the failure blaming the pin.
    const PREFIXED: Tool = Tool {
        hooks: Hooks {
            version_tags: Some(|v| vec![format!("v{v}")]),
            ..Hooks::NONE
        },
        ..T
    };
    let d = tempfile::tempdir().unwrap();
    let r = resolve(
        &PREFIXED,
        &pin(Reference::Version("0.1.0".into())),
        d.path(),
    )
    .unwrap();
    assert_eq!(r.attempts, vec![vec![
        "--git", "u", "--tag", "v0.1.0", "engine"
    ]]);
    assert_eq!(r.git_ref(), ("tag", "v0.1.0"));
}

#[test]
fn every_tag_a_tool_names_is_tried_in_order() {
    // A repository that changed convention partway has both spellings in
    // its history, and which one a given version is under is a fact about
    // that version rather than about the repository.
    const BOTH: Tool = Tool {
        hooks: Hooks {
            version_tags: Some(|v| vec![format!("v{v}"), v.to_string()]),
            ..Hooks::NONE
        },
        ..T
    };
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&BOTH, &pin(Reference::Version("0.1.0".into())), d.path()).unwrap();
    assert_eq!(r.attempts.len(), 2);
    assert_eq!(r.attempts[0], vec![
        "--git", "u", "--tag", "v0.1.0", "engine"
    ]);
    assert_eq!(r.attempts[1], vec![
        "--git", "u", "--tag", "0.1.0", "engine"
    ]);
}

#[test]
fn the_engine_crate_named_in_the_attempts_is_the_tools_own() {
    // the control that makes the assertions above mean anything: nothing
    // is baked into the argument lists that did not come from the tool.
    let d = tempfile::tempdir().unwrap();
    const OTHER: Tool = Tool {
        engine_crate: "somethingelse",
        engine_bin: None,
        ..T
    };
    let r = resolve(&OTHER, &pin(Reference::Tag("v1".into())), d.path()).unwrap();
    assert_eq!(r.attempts, vec![vec![
        "--git",
        "u",
        "--tag",
        "v1",
        "somethingelse"
    ]]);
}

#[test]
fn a_rev_resolves_to_one_git_attempt() {
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&T, &pin(Reference::Rev("sha1".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "sha1");
    assert_eq!(r.attempts, vec![vec![
        "--git", "u", "--rev", "sha1", "engine"
    ]]);
    assert_eq!(r.git_ref(), ("rev", "sha1"));
}

#[test]
fn a_tag_resolves_to_the_tag_only() {
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&T, &pin(Reference::Tag("nightly".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "tag:nightly");
    assert_eq!(r.attempts, vec![vec![
        "--git", "u", "--tag", "nightly", "engine"
    ]]);
    assert_eq!(r.git_ref(), ("tag", "nightly"));
}

#[test]
fn a_branch_resolves_to_its_rev_and_the_git_ref_carries_the_rev_not_the_name() {
    // the case a hook actually depends on. A dependency pinned to `dev`
    // resolves again an hour later to a different head, and then links a
    // different revision than the engine it loads into. So `git_ref` must
    // hand back the concrete rev, and a fixture whose rev equals the branch
    // name cannot tell the two apart.
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\nfeedface99c0ffee\n", unix_now())).unwrap();

    let r = resolve(&T, &pin(Reference::Branch("dev".into())), d.path()).unwrap();
    assert_eq!(
        r.key_rev, "feedface99c0ffee",
        "the cached resolution was ignored"
    );
    assert_eq!(r.git_ref(), ("rev", "feedface99c0ffee"));
    assert_eq!(r.attempts, vec![vec![
        "--git",
        "u",
        "--rev",
        "feedface99c0ffee",
        "engine"
    ]]);
    // and the branch name is nowhere in what a hook would build from
    assert!(!r.git_ref().1.contains("dev"));
}

#[test]
fn a_branch_falls_back_to_the_last_known_revision_when_the_remote_cannot_be_reached() {
    // On a train, or with the forge down. The recorded revision was the
    // branch's tip an hour ago and is very probably still built and sitting
    // in the cache, so running from it beats refusing to run at all. `u` is
    // not a remote, so git fails immediately and without a network.
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let stale = unix_now() - BRANCH_TTL.as_secs() - 1;
    std::fs::write(&path, format!("{stale}\nfeedface99c0ffee\n")).unwrap();

    let r = resolve(&T, &pin(Reference::Branch("dev".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "feedface99c0ffee");

    // The control: with nothing recorded there is nothing to fall back to,
    // and the failure is reported rather than invented. Without this the
    // test above would pass against a resolver that had stopped consulting
    // the remote at all.
    let empty = tempfile::tempdir().unwrap();
    let err = resolve(&T, &pin(Reference::Branch("dev".into())), empty.path()).unwrap_err();
    assert!(
        err.contains("ls-remote"),
        "the failure did not name what could not be reached: {err}"
    );
}

#[test]
fn the_full_ref_is_asked_for_so_a_tag_of_the_same_name_cannot_answer() {
    // A bare name matches `refs/tags/<name>` as well as `refs/heads/<name>`,
    // and which one comes back is then down to the order the remote lists
    // them. A repository carrying both is not exotic: a release tagged
    // after the branch it was cut from is exactly that.
    let empty = tempfile::tempdir().unwrap();
    let err = resolve(&T, &pin(Reference::Branch("dev".into())), empty.path()).unwrap_err();
    // The message is built from the same string the argument is, so this
    // reports what git was actually asked rather than a description of it.
    assert!(
        err.contains("refs/heads/dev"),
        "the branch was asked for by bare name: {err}"
    );
}

#[test]
fn the_branch_resolution_is_reused_inside_the_ttl_and_not_outside_it() {
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let now = unix_now();
    std::fs::write(&path, format!("{now}\nfeedface99\n")).unwrap();
    assert_eq!(fresh_resolution(&path), Some("feedface99".into()));

    std::fs::write(
        &path,
        format!("{}\nfeedface99\n", now - BRANCH_TTL.as_secs() - 1),
    )
    .unwrap();
    assert_eq!(fresh_resolution(&path), None);
}

#[test]
fn a_malformed_or_empty_resolution_is_not_reused() {
    // the control on the reader: a truncated write must not resolve to an
    // empty rev, which would key the cache on nothing and build the default
    // branch instead of the pin.
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("m");
    for bad in ["", "notanumber\nabc\n", &format!("{}\n\n", unix_now()), "123\n"] {
        std::fs::write(&path, bad).unwrap();
        assert_eq!(fresh_resolution(&path), None, "{bad:?}");
    }
}

#[test]
fn two_branches_on_one_url_do_not_share_a_resolution() {
    let d = tempfile::tempdir().unwrap();
    assert_ne!(
        branch_resolution_path(d.path(), "u", "dev"),
        branch_resolution_path(d.path(), "u", "main")
    );
    assert_ne!(
        branch_resolution_path(d.path(), "u", "dev"),
        branch_resolution_path(d.path(), "v", "dev")
    );
}

#[test]
fn the_sweep_leaves_the_resolution_the_offline_fallback_reads() {
    // The composition, which neither half was tested against. The fallback at
    // `resolve_branch` reads a resolution whatever its age; the sweep used to
    // delete anything past `BRANCH_TTL`, which is one hour. So the fallback
    // worked in the gap between going offline and the next daily collection
    // pass, and stopped afterwards, with a built engine for that very revision
    // still sitting in the cache.
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let stale = unix_now() - BRANCH_TTL.as_secs() - 1;
    std::fs::write(&path, format!("{stale}\nfeedface99c0ffee\n")).unwrap();
    filetime_back(&path, BRANCH_TTL.as_secs() + 1);

    sweep_branch_resolutions(d.path(), Duration::from_secs(60 * 60 * 24 * 30));
    assert!(
        path.exists(),
        "the sweep removed the resolution the offline fallback exists to read"
    );

    // And it still resolves from it, since a surviving file that no longer
    // parses would pass the assertion above and fail the user.
    let r = resolve(&T, &pin(Reference::Branch("dev".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "feedface99c0ffee");
}

#[test]
fn the_sweep_still_removes_a_resolution_nothing_could_want() {
    // The control on the test above, and the reason the sweep exists at all. A
    // resolution older than the retention window names a build the collector
    // has already taken, so falling back to it would name a revision that has
    // to be rebuilt anyway, which is what going to the remote does better.
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\nfeedface99c0ffee\n", unix_now())).unwrap();

    let retention = Duration::from_secs(60 * 60 * 24 * 30);
    filetime_back(&path, retention.as_secs() + 1);
    sweep_branch_resolutions(d.path(), retention);
    assert!(!path.exists(), "a resolution past retention was kept");
}

#[test]
fn a_zero_retention_sweeps_everything_and_a_huge_one_sweeps_nothing() {
    // The two ends, so the comparison is known to be a comparison. Without
    // these the two tests above pass against a sweep that always keeps, or one
    // that reads a constant.
    let d = tempfile::tempdir().unwrap();
    let path = branch_resolution_path(d.path(), "u", "dev");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\nfeedface99c0ffee\n", unix_now())).unwrap();
    filetime_back(&path, 60);

    sweep_branch_resolutions(d.path(), Duration::from_secs(60 * 60 * 24 * 3650));
    assert!(path.exists(), "a fresh resolution went under a huge window");

    sweep_branch_resolutions(d.path(), Duration::ZERO);
    assert!(!path.exists(), "nothing went under a zero window");
}

/// Push a file's modification time back by `secs`, since the sweep reads mtime
/// and a file written now is new whatever its contents say.
fn filetime_back(path: &Path, secs: u64) {
    let t = std::time::SystemTime::now() - Duration::from_secs(secs);
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(t).unwrap();
}

#[test]
fn a_pin_that_needs_no_remote_resolves_without_one() {
    // What this exists for: `engine_args` takes a `&Resolved`, and the two
    // fields deciding what `git_ref` answers are crate-only, so a consumer had
    // no way to build one and no way to run its own hook against it.
    let d = tempfile::tempdir().unwrap();

    let v = Resolved::without_network(&T, &pin(Reference::Version("1.2.3".into()))).unwrap();
    assert_eq!(v.git_ref(), ("tag", "1.2.3"));
    assert_eq!(v.key_rev, "v:1.2.3");

    let t = Resolved::without_network(&T, &pin(Reference::Tag("v9".into()))).unwrap();
    assert_eq!(t.git_ref(), ("tag", "v9"));

    let r = Resolved::without_network(&T, &pin(Reference::Rev("a".repeat(40)))).unwrap();
    assert_eq!(r.git_ref(), ("rev", "a".repeat(40).as_str()));

    // And it agrees with what a run derives, which is the property that makes
    // it worth handing to a consumer at all: one code path, so `key_rev`,
    // `attempts` and `version_tag` cannot be made to disagree here and not
    // there.
    for reference in [
        Reference::Version("1.2.3".into()),
        Reference::Tag("v9".into()),
        Reference::Rev("a".repeat(40)),
    ] {
        let p = pin(reference);
        assert_eq!(
            Resolved::without_network(&T, &p).unwrap(),
            resolve(&T, &p, d.path()).unwrap(),
            "{p:?}"
        );
    }
}

#[test]
fn a_branch_pin_is_refused_rather_than_reaching_the_network() {
    // The control, and the one arm that genuinely cannot be answered offline: a
    // branch is whatever the remote says it is now. Without this the function
    // would either block on `git ls-remote` inside somebody's test suite or
    // quietly answer from a cache that may be empty.
    let err = Resolved::without_network(&T, &pin(Reference::Branch("dev".into()))).unwrap_err();
    assert!(err.contains("dev"), "the branch is not named: {err}");
    assert!(err.contains("network"), "{err}");
}

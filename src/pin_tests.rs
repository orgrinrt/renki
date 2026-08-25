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
        url: "u".into(),
        reference: r,
    }
}

#[test]
fn a_version_tries_the_registry_then_the_matching_tag() {
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&T, &pin(Reference::Version("0.0.0-d05".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "v:0.0.0-d05");
    assert_eq!(
        r.attempts,
        vec![
            vec!["engine", "--version", "0.0.0-d05"],
            vec!["--git", "u", "--tag", "0.0.0-d05", "engine"],
        ]
    );
    // and the dep a hook would build points at the tag, not the version
    assert_eq!(r.git_ref(), ("tag", "0.0.0-d05"));
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
    assert_eq!(
        r.attempts,
        vec![
            vec!["engine", "--version", "0.1.0"],
            vec!["--git", "u", "--tag", "v0.1.0", "engine"],
        ]
    );
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
    assert_eq!(r.attempts.len(), 3);
    assert_eq!(
        r.attempts[1],
        vec!["--git", "u", "--tag", "v0.1.0", "engine"]
    );
    assert_eq!(
        r.attempts[2],
        vec!["--git", "u", "--tag", "0.1.0", "engine"]
    );
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
    assert_eq!(
        r.attempts,
        vec![vec!["--git", "u", "--tag", "v1", "somethingelse"]]
    );
}

#[test]
fn a_rev_resolves_to_one_git_attempt() {
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&T, &pin(Reference::Rev("sha1".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "sha1");
    assert_eq!(
        r.attempts,
        vec![vec!["--git", "u", "--rev", "sha1", "engine"]]
    );
    assert_eq!(r.git_ref(), ("rev", "sha1"));
}

#[test]
fn a_tag_resolves_to_the_tag_only() {
    let d = tempfile::tempdir().unwrap();
    let r = resolve(&T, &pin(Reference::Tag("nightly".into())), d.path()).unwrap();
    assert_eq!(r.key_rev, "tag:nightly");
    assert_eq!(
        r.attempts,
        vec![vec!["--git", "u", "--tag", "nightly", "engine"]]
    );
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
    assert_eq!(
        r.attempts,
        vec![vec!["--git", "u", "--rev", "feedface99c0ffee", "engine"]]
    );
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
    for bad in [
        "",
        "notanumber\nabc\n",
        &format!("{}\n\n", unix_now()),
        "123\n",
    ] {
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

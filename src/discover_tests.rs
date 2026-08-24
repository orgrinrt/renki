//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::fs;

use super::*;
use crate::tool::{Cli, Hooks, Locate, Workdir};

/// A tool whose config lives inside a repository.
const REPO: Tool = Tool {
    anchor: Anchor::Marker(".git"),
    short: "mock",
    config_file: "t.toml",
    pin_prefix: "t",
    engine_crate: "engine",
    engine_bin: None,
    cache_namespace: "t",
    default_url: "u",
    launcher_crate: "t-launcher",
    workdir: Some(Workdir {
        key: "work_dir",
        root_default: "mock",
    }),
    dir_flag: Cli::DIR_FLAG,
    engine_flag: Cli::ENGINE_FLAG,
    locate: Locate::DEFAULT,
    hooks: Hooks::NONE,
};

/// A tool whose config sits above a pile of repositories.
const SPAN: Tool = Tool {
    anchor: Anchor::ConfigFile,
    workdir: None,
    ..REPO
};

#[test]
fn the_env_override_wins_when_it_is_a_directory() {
    let d = tempfile::tempdir().unwrap();
    let got = repo_root_with(
        &REPO,
        Some(d.path().as_os_str().to_os_string()),
        Path::new("/"),
    );
    assert_eq!(got.as_deref(), Some(d.path()));
}

#[test]
fn the_env_override_is_ignored_when_it_is_not() {
    // and falls through to the walk rather than failing, so a stale export
    // in a shell does not make the tool unusable.
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join(".git")).unwrap();
    let got = repo_root_with(&REPO, Some("/definitely/not/a/dir/xyzzy".into()), d.path());
    assert_eq!(got.as_deref(), Some(d.path()));
}

#[test]
fn a_marker_anchor_stops_at_the_nearest_repo() {
    let d = tempfile::tempdir().unwrap();
    let inner = d.path().join("member");
    fs::create_dir_all(inner.join(".git")).unwrap();
    fs::create_dir(d.path().join(".git")).unwrap();
    fs::create_dir(inner.join("deep")).unwrap();
    // from inside the member, the member is the root, not the outer repo.
    assert_eq!(
        repo_root_with(&REPO, None, &inner.join("deep")).as_deref(),
        Some(inner.as_path())
    );
}

#[test]
fn a_config_anchor_walks_past_a_nested_repo_to_reach_the_config() {
    // the case a marker anchor cannot serve, and the reason the anchor is a
    // parameter: the config sits above the repos, and running the tool from
    // inside one of them is the normal way it is used.
    let d = tempfile::tempdir().unwrap();
    let member = d.path().join("member");
    fs::create_dir_all(member.join(".git")).unwrap();
    fs::create_dir(d.path().join(".git")).unwrap();
    fs::write(d.path().join("t.toml"), "").unwrap();

    assert_eq!(
        repo_root_with(&SPAN, None, &member).as_deref(),
        Some(d.path()),
        "the walk stopped at the member repo instead of reaching the config"
    );
    // the control: the same tree under a marker anchor stops at the member,
    // which is exactly the failure this variant exists to avoid.
    assert_eq!(
        repo_root_with(&REPO, None, &member).as_deref(),
        Some(member.as_path())
    );
}

#[test]
fn no_anchor_anywhere_resolves_to_nothing() {
    let d = tempfile::tempdir().unwrap();
    assert!(repo_root_with(&SPAN, None, d.path()).is_none());
}

#[test]
fn a_root_config_maps_the_conventional_subdirectory() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("t.toml"), "project_name = \"x\"\n").unwrap();
    let loc = locate(&REPO, d.path()).unwrap().unwrap();
    assert_eq!(loc.config_path, d.path().join("t.toml"));
    assert_eq!(loc.workdir, d.path().join("mock"));
}

#[test]
fn a_root_config_may_name_another() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("t.toml"), "work_dir = \"design\"\n").unwrap();
    assert_eq!(
        locate(&REPO, d.path()).unwrap().unwrap().workdir,
        d.path().join("design")
    );
}

#[test]
fn a_subdir_config_maps_its_own_directory() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("mock")).unwrap();
    fs::write(d.path().join("mock/t.toml"), "project_name = \"x\"\n").unwrap();
    let loc = locate(&REPO, d.path()).unwrap().unwrap();
    assert_eq!(loc.config_path, d.path().join("mock/t.toml"));
    assert_eq!(loc.workdir, d.path().join("mock"));
}

#[test]
fn two_configs_in_scope_is_an_error_naming_both() {
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("t.toml"), "project_name = \"root\"\n").unwrap();
    fs::create_dir(d.path().join("mock")).unwrap();
    fs::write(d.path().join("mock/t.toml"), "project_name = \"sub\"\n").unwrap();
    let err = locate(&REPO, d.path()).unwrap_err();
    assert!(err.contains("t.toml"), "{err}");
    assert!(err.contains("mock"), "{err}");

    // and two subdirs, neither of which is the root
    let d = tempfile::tempdir().unwrap();
    for sub in ["mock", ".config"] {
        fs::create_dir(d.path().join(sub)).unwrap();
        fs::write(d.path().join(sub).join("t.toml"), "x = 1\n").unwrap();
    }
    assert!(locate(&REPO, d.path()).is_err());
}

#[test]
fn a_config_anchored_tool_does_not_scan_subdirectories() {
    // the whole difference. Under a marker anchor the second config below
    // would be a hard error; here it is a nested workspace and invisible.
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("t.toml"), "").unwrap();
    fs::create_dir(d.path().join("member")).unwrap();
    fs::write(d.path().join("member/t.toml"), "").unwrap();

    let loc = locate(&SPAN, d.path()).unwrap().unwrap();
    assert_eq!(loc.config_path, d.path().join("t.toml"));
    // no workdir, so the engine runs against the root itself
    assert_eq!(loc.workdir, d.path());
    // the control, on the identical tree
    assert!(locate(&REPO, d.path()).is_err());
}

#[test]
fn a_config_anchored_tool_with_a_workdir_maps_it_under_the_root() {
    // the cell nothing named: `ConfigFile` and `Some(Workdir)` together.
    // The config's directory is the root under this anchor, so the
    // root-level default applies and the in-subdirectory `.` branch is
    // unreachable here by construction rather than by omission.
    const SPAN_WD: Tool = Tool {
        anchor: Anchor::ConfigFile,
        ..REPO
    };
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("t.toml"), "").unwrap();
    assert_eq!(
        locate(&SPAN_WD, d.path()).unwrap().unwrap().workdir,
        d.path().join("mock")
    );
    // and the config may still name another
    fs::write(d.path().join("t.toml"), "work_dir = \"design\"\n").unwrap();
    assert_eq!(
        locate(&SPAN_WD, d.path()).unwrap().unwrap().workdir,
        d.path().join("design")
    );
}

#[test]
fn a_directory_named_like_the_config_does_not_anchor_a_config_anchored_tool() {
    // it would return a root whose config is then absent, which reads as
    // "no config here" rather than as "that is a directory". The walk
    // continues instead, and finds the real one above.
    let d = tempfile::tempdir().unwrap();
    let real = d.path().join("real");
    let decoy = real.join("member");
    fs::create_dir_all(decoy.join("t.toml")).unwrap();
    fs::write(real.join("t.toml"), "").unwrap();

    assert_eq!(
        repo_root_with(&SPAN, None, &decoy).as_deref(),
        Some(real.as_path()),
        "a directory of that name anchored the walk"
    );
    // the control: a marker anchor is not a file, so `.git` as a directory
    // must go on anchoring, which is the ordinary case.
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join(".git")).unwrap();
    assert_eq!(
        repo_root_with(&REPO, None, d.path()).as_deref(),
        Some(d.path())
    );
}

#[test]
fn no_config_is_not_an_error() {
    let d = tempfile::tempdir().unwrap();
    assert!(locate(&REPO, d.path()).unwrap().is_none());
    assert!(locate(&SPAN, d.path()).unwrap().is_none());
}

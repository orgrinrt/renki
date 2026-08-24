//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Locating the repo, the config, and the working directory from any working
//! directory. All discovery uses absolute paths so cwd never matters, and it is
//! strictly read-only: nothing is moved or written during resolution.
//!
//! The launcher is deliberately schema-agnostic about whatever workflow the
//! engine implements. That is a function of the pinned version, not a separate
//! axis to detect, so the launcher resolves the version, builds that engine and
//! runs it. The engine binary inherently knows its own workflow.

use std::path::{Path, PathBuf};

use crate::manifest::Header;
use crate::tool::{Anchor, Tool};

/// A located config and the working directory it maps.
#[derive(Debug, PartialEq, Eq)]
pub struct Located {
    /// The config to read the pin from.
    pub config_path: PathBuf,
    /// The absolute directory the engine runs against.
    pub workdir: PathBuf,
}

/// The repo root.
///
/// The tool's root environment variable wins when set and pointing at a
/// directory. Otherwise the anchor decides. `None` when neither resolves.
pub fn repo_root(tool: &Tool) -> Option<PathBuf> {
    let from_env = std::env::var_os(tool.root_env());
    let cwd = std::env::current_dir().ok()?;
    repo_root_with(tool, from_env, &cwd)
}

/// Pure core of [`repo_root`]: the override and the starting directory are
/// passed in so resolution is testable without mutating process env, where
/// cargo's parallel test threads make `set_var` a data race.
fn repo_root_with(
    tool: &Tool,
    from_env: Option<std::ffi::OsString>,
    cwd: &Path,
) -> Option<PathBuf> {
    if let Some(r) = from_env {
        let p = PathBuf::from(r);
        if p.is_dir() {
            return Some(p);
        }
    }
    // A marker is whatever the tool named and may be a directory, `.git`
    // being one. A config file is a file, and it is checked as one here for
    // the same reason `locate` checks it below: anchoring on a *directory* of
    // that name would return a root that then reports no config, which reads
    // as the config being absent rather than as the directory being in the
    // way.
    let found: fn(&Path) -> bool = match tool.anchor {
        Anchor::Marker(_) => |p| p.exists(),
        Anchor::ConfigFile => |p| p.is_file(),
    };
    let wanted = match tool.anchor {
        Anchor::Marker(name) => name,
        Anchor::ConfigFile => tool.config_file,
    };
    let mut d = cwd.to_path_buf();
    loop {
        if found(&d.join(wanted)) {
            return Some(d);
        }
        if !d.pop() {
            return None;
        }
    }
}

/// Resolve the config and the working directory it maps.
///
/// # Under a marker anchor: exactly one config per repo
///
/// The config sits either at the repo root or in a single immediate
/// subdirectory. There is no merging, no precedence and no nearest-wins: those
/// semantics were never specified, so rather than pick one silently, **more
/// than one config anywhere in scope is a hard error** that blocks every run
/// until exactly one remains. Zero is `Ok(None)`, which lets the caller fall
/// back to whatever legacy pin the tool still honours.
///
/// The scan covers the repo root and its immediate subdirectories, hidden ones
/// first, skipping `.git`, `target` and `node_modules`. Deeper nesting is out
/// of scope.
///
/// # Under a config-file anchor
///
/// Finding the root found the config, so there is no scan and no two-config
/// error. A subdirectory carrying a config of its own is a nested workspace
/// rather than an ambiguity here.
pub fn locate(tool: &Tool, root: &Path) -> Result<Option<Located>, String> {
    if tool.anchor == Anchor::ConfigFile {
        let cfg = root.join(tool.config_file);
        return Ok(cfg.is_file().then(|| located(tool, root, root, cfg)));
    }

    // Collect every (config, its-dir) in scope, so a second is caught rather
    // than silently shadowed by a precedence order.
    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    let root_cfg = root.join(tool.config_file);
    if root_cfg.is_file() {
        found.push((root_cfg, root.to_path_buf()));
    }
    for sub in ordered_subdirs(root) {
        let cfg = sub.join(tool.config_file);
        if cfg.is_file() {
            found.push((cfg, sub));
        }
    }
    match found.len() {
        0 => Ok(None),
        1 => {
            let (config_path, dir) = found.into_iter().next().unwrap();
            Ok(Some(located(tool, root, &dir, config_path)))
        }
        _ => {
            let list = found
                .iter()
                .map(|(c, _)| format!("  {}", c.display()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(format!(
                "found more than one {} in this repo; a repo must have exactly one (at the \
                 repo root, or in a single subdir). Remove the extras, keep one:\n{list}",
                tool.config_file
            ))
        }
    }
}

fn located(tool: &Tool, root: &Path, config_dir: &Path, config_path: PathBuf) -> Located {
    let declared = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|t| Header::parse(tool, &t).workdir);
    Located {
        workdir: tool.workdir_for(root, config_dir, declared),
        config_path,
    }
}

/// Immediate subdirs of `root`, hidden ones first, each group sorted, skipping
/// dirs that never hold a workspace.
///
/// Only ever reached under [`Anchor::Marker`], since a config-anchored tool
/// found its config by finding its root. The three skipped names are a fixed
/// convention rather than a tool parameter: none of them holds a config under
/// any anchor, so nothing is gained by letting a tool restate them.
fn ordered_subdirs(root: &Path) -> Vec<PathBuf> {
    let mut hidden = Vec::new();
    let mut plain = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    for e in rd.flatten() {
        if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if matches!(name.as_str(), ".git" | "target" | "node_modules") {
            continue;
        }
        if name.starts_with('.') {
            hidden.push((name, e.path()));
        } else {
            plain.push((name, e.path()));
        }
    }
    hidden.sort_by(|a, b| a.0.cmp(&b.0));
    plain.sort_by(|a, b| a.0.cmp(&b.0));
    hidden.into_iter().chain(plain).map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::tool::{Hooks, Workdir};

    /// A tool whose config lives in a repo, mockspace's shape.
    const REPO: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "mock",
        config_file: "t.toml",
        pin_prefix: "t",
        engine_crate: "engine",
        cache_namespace: "t",
        default_url: "u",
        launcher_crate: "t-launcher",
        workdir: Some(Workdir {
            key: "work_dir",
            root_default: "mock",
        }),
        hooks: Hooks::NONE,
    };

    /// A tool whose config sits above a pile of repos, homma's shape.
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
}

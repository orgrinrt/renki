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
pub(crate) struct Located {
    /// The config to read the pin from.
    pub config_path: PathBuf,
    /// The absolute directory the engine runs against.
    pub workdir:     PathBuf,
}

/// The repo root.
///
/// The tool's root environment variable wins when set and pointing at a
/// directory. Otherwise the anchor decides. `None` when neither resolves.
pub(crate) fn repo_root(tool: &Tool) -> Option<PathBuf> {
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
pub(crate) fn locate(tool: &Tool, root: &Path) -> Result<Option<Located>, String> {
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
    for sub in ordered_subdirs(root, tool.scan_skip) {
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
        },
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
        },
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
/// the names in `skip`.
///
/// Only ever reached under [`Anchor::Marker`], since a config-anchored tool
/// found its config by finding its root. The skip list is the tool's, because
/// the cost of a name missing from it is not a slower scan: a file with this
/// tool's config name anywhere in scope is a hard error that blocks every run,
/// and a vendored tree or a build output directory can hold one without
/// anybody putting it there.
fn ordered_subdirs(root: &Path, skip: &[&str]) -> Vec<PathBuf> {
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
        if skip.contains(&name.as_str()) {
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

/// Why the root walk found nothing, as a message an operator can act on.
pub(crate) fn no_root(tool: &Tool) -> String {
    no_root_with(tool, std::env::var_os(tool.root_env()))
}

/// Pure core of [`no_root`]. The override is passed in so both arms are
/// testable without mutating process env.
///
/// The distinction is the whole of it. A set-but-wrong override is the case an
/// operator can actually fix, and telling them the variable is unset when they
/// just exported it sends them looking in the wrong place. The walk falls
/// through rather than failing on a bad override, deliberately, so a stale
/// export in a shell does not make the tool unusable; that is what leaves this
/// message the only place the operator hears about it.
fn no_root_with(tool: &Tool, from_env: Option<std::ffi::OsString>) -> String {
    let what = match tool.anchor {
        Anchor::Marker(m) => m.to_string(),
        Anchor::ConfigFile => tool.config_file.to_string(),
    };
    let env = tool.root_env();
    match from_env {
        Some(v) => {
            format!(
                "no {what} found in this directory or any above it. {env} is set to {}, which is \
             not a directory, so it was ignored",
                Path::new(&v).display()
            )
        },
        None => format!("no {what} found in this directory or any above it, and {env} is unset"),
    }
}

#[cfg(test)]
#[path = "discover_tests.rs"]
mod tests;

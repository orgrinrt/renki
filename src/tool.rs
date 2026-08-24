//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What distinguishes one launcher from another.
//!
//! Everything else in this crate is the same for every tool: find the repo,
//! read the pin, resolve it to build attempts, build once into a keyed cache,
//! exec. A [`Tool`] is the handful of names that differ, plus the few places a
//! tool needs to do something of its own.
//!
//! It is a plain const-constructible struct rather than a trait, so a consuming
//! binary is a `static` and a `main` that hands it over.

use std::path::{Path, PathBuf};

use crate::pin::{Pin, Resolved};

/// How the repo root is found, walking up from the working directory.
///
/// A parameter rather than the `.git` constant it was in the code this crate
/// came from, because the two shapes do not generalise to each other: one
/// stops at the first repository and the other must walk past it. `mock` is
/// the [`Marker`](Anchor::Marker) consumer and is the only consumer today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The nearest ancestor holding an entry of this name, `.git` in practice.
    ///
    /// Right for a tool whose config lives inside a repository, and whose
    /// config may sit either at the root or one directory below it. The search
    /// for the config then covers the root and its immediate subdirectories,
    /// and more than one config found there is an error rather than a
    /// precedence question.
    Marker(&'static str),
    /// The nearest ancestor holding the config file itself.
    ///
    /// Right for a tool whose config sits above a pile of repositories rather
    /// than inside one. Anchoring on `.git` would stop at the first repository
    /// walking up and never reach the config, which is the common case rather
    /// than an edge: running such a tool from inside a member repository is
    /// how it is normally used.
    ///
    /// Finding the anchor finds the config, so there is no second search and no
    /// two-configs error. A member repository carrying a config of its own is a
    /// nested workspace, not an ambiguity at this level.
    ConfigFile,
}

/// The identity of one launcher.
pub struct Tool {
    /// How the repo root is found. See [`Anchor`].
    pub anchor: Anchor,
    /// The short name this launcher answers to, used in its own diagnostics
    /// (`mock: ...`) and as the prefix of the environment variables it reads
    /// (`MOCK_ROOT`, `MOCK_NO_SELF_UPDATE`, uppercased).
    pub short: &'static str,
    /// The one config file a repo carries, e.g. `mockspace.toml`.
    pub config_file: &'static str,
    /// The prefix of the pin keys inside that config: `mockspace` reads
    /// `mockspace_version`, `mockspace_rev`, `mockspace_branch`,
    /// `mockspace_tag` and `mockspace_git`.
    pub pin_prefix: &'static str,
    /// The engine's package name on crates.io, which is also the name of the
    /// binary its build produces.
    pub engine_crate: &'static str,
    /// The directory under `$XDG_CACHE_HOME` (or `~/.cache`) this launcher
    /// owns. Distinct per tool so two tools never share a build cache.
    pub cache_namespace: &'static str,
    /// The engine's source when the config names none.
    pub default_url: &'static str,
    /// This launcher's own package name, which is how it recognises its own
    /// entry in cargo's install ledger when checking for an update.
    pub launcher_crate: &'static str,
    /// The working subdirectory the engine is pointed at, when the tool has
    /// one. `None` runs the engine against the repo root.
    pub workdir: Option<Workdir>,
    /// The tool-specific parts, all optional.
    pub hooks: Hooks,
}

/// A check a tool runs against a directory, refusing with a message a reader
/// can act on. Named because two hooks have this shape and a bare
/// `Option<fn(&Path) -> Result<(), String>>` reads as noise at both.
pub type Check = fn(&Path) -> Result<(), String>;

/// A working subdirectory the config maps.
///
/// The key is read from the config; the default depends on where the config
/// sits. At the repo root it is `root_default` (a subdirectory beside it); in
/// a subdirectory the default is that subdirectory itself, since a config
/// living inside the working directory is already pointing at it.
pub struct Workdir {
    /// The config key naming it, e.g. `mock_dir`.
    pub key: &'static str,
    /// What a root-level config means when it does not set the key.
    pub root_default: &'static str,
}

/// The places a launcher does something only its own tool needs.
///
/// Every field is optional and defaults to doing nothing, so a tool that needs
/// none of this writes `Hooks::NONE`.
pub struct Hooks {
    /// Run once the repo and its config are located, before the engine is
    /// built. Whatever a tool must keep planted in a repo goes here, and it is
    /// best-effort by contract: it cannot fail the command the user ran.
    pub prepare_repo: Option<fn(&Path)>,
    /// Extra arguments passed to the engine ahead of the user's, derived from
    /// the resolved pin. This is how a tool hands the engine something that
    /// must match the exact revision the engine was built from.
    pub engine_args: Option<fn(&Resolved) -> Vec<String>>,
    /// The same, for the `--engine <path>` override, where there is no pin and
    /// the source is a working tree.
    pub engine_args_local: Option<fn(&Path) -> Vec<String>>,
    /// Refuse an `--engine <path>` that is not a checkout of this engine.
    /// Reported against the flag the user passed, rather than surfacing later
    /// as a build failure about something else.
    pub verify_engine_dir: Option<Check>,
    /// A last-resort pin for a repo that has not adopted an explicit one,
    /// given the working directory. Keeps a repo mid-migration running.
    pub legacy_pin: Option<fn(&Path) -> Option<Pin>>,
    /// Refuse a repo state that would silently route the user somewhere else,
    /// given the repo root. A retired cargo alias shadowing the launcher is
    /// the case this exists for.
    pub verify_repo_state: Option<Check>,
}

impl Hooks {
    /// A tool that needs none of the extension points.
    pub const NONE: Hooks = Hooks {
        prepare_repo: None,
        engine_args: None,
        engine_args_local: None,
        verify_engine_dir: None,
        legacy_pin: None,
        verify_repo_state: None,
    };
}

impl Tool {
    /// The environment variable naming the repo root, overriding the `.git`
    /// walk: the short name uppercased, plus `_ROOT`.
    pub fn root_env(&self) -> String {
        format!("{}_ROOT", self.short.to_uppercase())
    }

    /// The environment variable that opts out of launcher self-update.
    pub fn no_self_update_env(&self) -> String {
        format!("{}_NO_SELF_UPDATE", self.short.to_uppercase())
    }

    /// The working directory to run the engine against, for a config found at
    /// `config_dir` inside `root`, with `declared` read from the config.
    ///
    /// A tool with no [`Workdir`] runs against the repo root, whatever the
    /// config says.
    pub fn workdir_for(&self, root: &Path, config_dir: &Path, declared: Option<String>) -> PathBuf {
        let Some(wd) = &self.workdir else {
            return root.to_path_buf();
        };
        let default = if config_dir == root {
            wd.root_default
        } else {
            "."
        };
        normalize(config_dir.join(declared.unwrap_or_else(|| default.to_string())))
    }

    /// The working directory when no config was found at all: the conventional
    /// one under the repo root, or the root itself for a tool with none.
    pub fn workdir_default(&self, root: &Path) -> PathBuf {
        match &self.workdir {
            Some(wd) => normalize(root.join(wd.root_default)),
            None => root.to_path_buf(),
        }
    }
}

/// Collapse a trailing `/.`, which the in-subdirectory default produces.
fn normalize(p: PathBuf) -> PathBuf {
    if p.file_name().map(|n| n == ".").unwrap_or(false) {
        return p.parent().map(Path::to_path_buf).unwrap_or(p);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH: Tool = Tool {
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

    const WITHOUT: Tool = Tool {
        workdir: None,
        ..WITH
    };

    #[test]
    fn env_names_come_from_the_short_name() {
        assert_eq!(WITH.root_env(), "MOCK_ROOT");
        assert_eq!(WITH.no_self_update_env(), "MOCK_NO_SELF_UPDATE");
    }

    #[test]
    fn a_root_config_defaults_to_the_subdirectory_beside_it() {
        let root = Path::new("/r");
        assert_eq!(WITH.workdir_for(root, root, None), Path::new("/r/mock"));
    }

    #[test]
    fn a_root_config_may_name_another() {
        let root = Path::new("/r");
        let got = WITH.workdir_for(root, root, Some("design".into()));
        assert_eq!(got, Path::new("/r/design"));
    }

    #[test]
    fn a_config_inside_the_workdir_defaults_to_its_own_directory() {
        // and the trailing `/.` that default produces is collapsed, or every
        // path derived from it carries it.
        let got = WITH.workdir_for(Path::new("/r"), Path::new("/r/mock"), None);
        assert_eq!(got, Path::new("/r/mock"));
    }

    #[test]
    fn a_tool_without_a_workdir_runs_against_the_root() {
        let root = Path::new("/r");
        // the control: the same inputs that give a subdirectory above give the
        // root here, including when the config declares something.
        assert_eq!(WITHOUT.workdir_for(root, root, None), root);
        assert_eq!(WITHOUT.workdir_for(root, root, Some("design".into())), root);
        assert_eq!(WITHOUT.workdir_default(root), root);
        assert_eq!(WITH.workdir_default(root), Path::new("/r/mock"));
    }
}

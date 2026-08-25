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
use std::time::Duration;

use crate::pin::{Pin, Resolved};

/// How the repo root is found, walking up from the working directory.
///
/// A parameter rather than a `.git` constant, because the two shapes do not
/// generalise to each other: one stops at the first repository and the other
/// must walk past it.
///
/// That argument says the two are irreducible. It does not say there is no
/// third, and a launcher of this kind plausibly wants one: an anchor that is
/// only the environment override, for a tool that refuses to guess. So the
/// enum is left open. Nothing in the public surface hands a consumer an
/// [`Anchor`] to match on, so the marker costs a consumer nothing today and
/// makes a third shape a minor release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Anchor {
    /// The nearest ancestor holding an entry of this name, `.git` in practice.
    ///
    /// Right for a tool whose config lives inside a repository, and whose
    /// config may sit either at the root or one directory below it. The search
    /// for the config then covers the root and its immediate subdirectories,
    /// and more than one config found there is an error rather than a
    /// precedence question.
    ///
    /// One directory below is the whole depth. A config at
    /// `tools/widget/widget.toml` is not found, and [`Anchor::ConfigFile`] is
    /// the anchor for that shape. Which subdirectories are looked in at all is
    /// [`Tool::scan_skip`].
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
///
/// Every field is a name the launcher has to know. Anything the launcher needs
/// the tool to *decide* rather than to *know* is a [`Hooks`] entry instead, and
/// that line is the whole rule for where a new member belongs.
pub struct Tool {
    /// How the repo root is found. See [`Anchor`].
    pub anchor: Anchor,
    /// The short name this launcher answers to, used in its own diagnostics
    /// (`widget: ...`) and, uppercased, as the prefix of the environment
    /// variables it reads: `WIDGET_ROOT` overrides discovery, `WIDGET_CACHE`
    /// overrides where the built engines go, and `WIDGET_NO_SELF_UPDATE` turns
    /// the update check off.
    pub short: &'static str,
    /// The one config file a repo carries, e.g. `widget.toml`.
    pub config_file: &'static str,
    /// What the pin keys inside that config are called. See [`PinKeys`].
    pub pin_keys: PinKeys,
    /// The engine's package name, which is what `cargo install` is asked for.
    pub engine_crate: &'static str,
    /// The binary that package installs, when it is not named after the
    /// package. A crate `widget-engine` whose `[[bin]]` is `widget` names
    /// `Some("widget")` here.
    ///
    /// `None` is the ordinary case and means the two are the same.
    pub engine_bin: Option<&'static str>,
    /// The directory under `$XDG_CACHE_HOME` (or `~/.cache`) this launcher
    /// owns. Distinct per tool so two tools never share a build cache.
    ///
    /// `<SHORT>_CACHE` overrides the whole path for a user who needs the
    /// builds somewhere else.
    pub cache_namespace: &'static str,
    /// How long a cached engine survives after the last run that wanted it.
    ///
    /// A build nothing has asked for in this long is collected. The number
    /// decides how much of a user's disk the tool holds, and it is a very
    /// different number for a tool used daily than for one used twice a year,
    /// which is why it is the tool's rather than the launcher's.
    pub cache_retention: Duration,
    /// Directory names the config scan never descends into, under
    /// [`Anchor::Marker`].
    ///
    /// [`Tool::CONVENTIONS`] carries the three that never hold a config under
    /// any tool. A repository with a vendored tree, a virtual environment or a
    /// build output directory adds its own, which is worth doing: a stray file
    /// with this tool's config name anywhere in scope is a hard error that
    /// blocks every run, not a scan result.
    pub scan_skip: &'static [&'static str],
    /// The engine's source when the config names none.
    pub default_url: &'static str,
    /// This launcher's own package name, which is how it recognises its own
    /// entry in cargo's install ledger when checking for an update.
    pub launcher_crate: &'static str,
    /// The working subdirectory the engine is pointed at, when the tool has
    /// one. `None` runs the engine against the repo root.
    pub workdir: Option<Workdir>,
    /// The flag the engine takes its absolute working directory on. The
    /// launcher always passes it, and strips any copy the user wrote, so the
    /// engine never has to decide which of two answers is right.
    ///
    /// [`Cli::DIR_FLAG`] is the conventional `--dir`.
    pub dir_flag: &'static str,
    /// The flag that points the launcher at a checkout on disk instead of the
    /// pinned engine. Consumed by the launcher and never forwarded, so an
    /// engine wanting a flag of this name needs a different one here.
    ///
    /// [`Cli::ENGINE_FLAG`] is the conventional `--engine`.
    pub engine_flag: &'static str,
    /// The query the launcher answers itself, and what it calls the parts of
    /// its answer. `None` for a tool that answers no such query and forwards
    /// every subcommand to the engine. See [`Locate`].
    pub locate: Option<Locate>,
    /// Whether this launcher keeps itself current. See [`SelfUpdate`].
    pub self_update: SelfUpdate,
    /// The tool-specific parts, all optional.
    pub hooks: Hooks,
}

/// Conventional spellings for the two launcher flags.
///
/// Named constants rather than defaults, since [`Tool`] is a plain struct with
/// no `Default`: a tool with no opinion writes `Cli::DIR_FLAG` and reads as
/// having chosen it.
pub struct Cli;

impl Cli {
    /// The conventional [`Tool::dir_flag`].
    pub const DIR_FLAG: &'static str = "--dir";
    /// The conventional [`Tool::engine_flag`].
    pub const ENGINE_FLAG: &'static str = "--engine";
}

/// The query the launcher answers without building or running the engine, and
/// the names it answers under.
///
/// A tool's git hooks and shell helpers typically ask this instead of walking
/// the tree themselves, so the key names are a contract with those callers
/// rather than an internal detail. They are fields for that reason: a tool
/// migrating onto this crate keeps whatever names its existing readers already
/// parse, and nothing downstream has to change on the same day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locate {
    /// The subcommand that triggers the query.
    pub subcommand: &'static str,
    /// What the answer calls the repo root.
    pub root_key: &'static str,
    /// What the answer calls the config file's path. Empty in the answer when
    /// the repo has a working directory and no config, which is a real shape
    /// rather than a broken one.
    pub config_key: &'static str,
    /// What the answer calls the working directory. Empty in the answer when
    /// there is no such directory on disk.
    pub workdir_key: &'static str,
}

impl Locate {
    /// The conventional spelling: a `locate` subcommand answering under `root`,
    /// `config` and `workdir`.
    pub const DEFAULT: Locate = Locate {
        subcommand: "locate",
        root_key: "root",
        config_key: "config",
        workdir_key: "workdir",
    };
}

/// What the pin keys inside a repo's config are called.
///
/// Full names rather than a prefix, on the same argument [`Locate`] is
/// parameterised by: a tool moving onto this crate keeps whatever its existing
/// configs already spell, and no repository has to be edited on the day it
/// migrates. [`PinKeys::prefixed`] is the conventional shape and is what a new
/// tool writes.
///
/// Only top-level keys are read, so a name here is a top-level name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinKeys {
    /// A released version, resolved against the registry first and a tag
    /// second.
    pub version: &'static str,
    /// An exact commit.
    pub rev: &'static str,
    /// A tag.
    pub tag: &'static str,
    /// A moving branch head.
    pub branch: &'static str,
    /// Where the engine's source is, overriding
    /// [`Tool::default_url`](crate::Tool::default_url).
    pub git: &'static str,
}

impl PinKeys {
    /// The first empty name, or `None`.
    const fn defect(&self) -> Option<&'static str> {
        if self.version.is_empty()
            || self.rev.is_empty()
            || self.tag.is_empty()
            || self.branch.is_empty()
            || self.git.is_empty()
        {
            return Some("a pin key is empty, so that pin form can never be read");
        }
        None
    }
}

/// The conventional [`PinKeys`]: `<prefix>_version`, `_rev`, `_tag`, `_branch`
/// and `_git`.
///
/// ```
/// # use renki::{pin_keys, PinKeys};
/// const KEYS: PinKeys = pin_keys!("widget");
/// assert_eq!(KEYS.version, "widget_version");
/// assert_eq!(KEYS.git, "widget_git");
/// ```
///
/// A macro rather than a `const fn`, because joining two strings into a
/// `&'static str` is something only the expansion can do: a const function
/// would have to return a value it has nowhere to store. The prefix is
/// therefore a literal, which is what a descriptor writes anyway.
#[macro_export]
macro_rules! pin_keys {
    ($prefix:literal) => {
        $crate::PinKeys {
            version: ::core::concat!($prefix, "_version"),
            rev: ::core::concat!($prefix, "_rev"),
            tag: ::core::concat!($prefix, "_tag"),
            branch: ::core::concat!($prefix, "_branch"),
            git: ::core::concat!($prefix, "_git"),
        }
    };
}

/// Whether a launcher keeps itself current.
///
/// A launcher installed from a moving branch goes stale silently, and chasing
/// it is what a tool distributed that way wants. A tool packaged by somebody
/// else, installed into a prefix it does not own, or run where a program
/// reinstalling itself is unwelcome, wants the opposite, and that is the tool
/// author's call rather than something every one of its users has to turn off
/// by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelfUpdate {
    /// Never. The launcher is whatever was installed.
    Never,
    /// Reinstall when the installed launcher came from a branch and that
    /// branch has moved, at most once an hour. A user turns it off for
    /// themselves with `<SHORT>_NO_SELF_UPDATE`.
    ///
    /// A version or tag install is left alone either way: it names an
    /// immutable revision, so there is nothing to chase.
    ChaseTheBranch,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Workdir {
    /// The config key naming it, e.g. `widget_dir`.
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
    /// Refuse a [`Tool::engine_flag`] path that is not a checkout of this
    /// engine. Reported against the flag the user passed, rather than
    /// surfacing later as a build failure about something else.
    pub verify_engine_dir: Option<Check>,
    /// A last-resort pin for a repo that has not adopted an explicit one,
    /// given the working directory. Keeps a repo mid-migration running.
    pub legacy_pin: Option<fn(&Path) -> Option<Pin>>,
    /// The tag a released version is under, where the repository does not name
    /// its tags after the bare version.
    ///
    /// The version pin tries the registry first and a git tag second, and the
    /// second attempt needs a name. `v0.1.0` is at least as common as `0.1.0`,
    /// and a tool whose engine repository uses the prefix could not be built
    /// from a version pin before publishing, with the failure reported against
    /// the pin rather than against the spelling.
    ///
    /// Every string this returns is tried in order, so a repository that
    /// changed convention partway can name both.
    pub version_tags: Option<fn(&str) -> Vec<String>>,
    /// Refuse a repo state that would silently route the user somewhere else,
    /// given the repo root.
    ///
    /// Runs on every invocation that found a root, config or no config. That
    /// matters: a repo with no config is a repo that has not adopted the
    /// launcher, which is exactly where a stale route left over from whatever
    /// came before is most likely to still be in place.
    pub verify_repo_state: Option<Check>,
}

impl Hooks {
    /// A tool that needs none of the extension points.
    pub const NONE: Hooks = Hooks {
        prepare_repo: None,
        engine_args: None,
        engine_args_local: None,
        verify_engine_dir: None,
        version_tags: None,
        legacy_pin: None,
        verify_repo_state: None,
    };
}

/// Byte equality for two `&str` in a const context, which `==` is not.
const fn const_str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

impl Tool {
    /// Everything a launcher can be given a conventional answer for, and an
    /// empty string everywhere it cannot.
    ///
    /// Written as `..Tool::CONVENTIONS`, which reads as taking the
    /// conventions rather than as forgetting the fields. The empty names are
    /// safe to ship as a base because [`Tool::defect`] refuses every one of
    /// them, and it is const, so a descriptor that leaves one unset does not
    /// build:
    ///
    /// ```compile_fail
    /// # use renki::Tool;
    /// const TOOL: Tool = Tool { short: "widget", ..Tool::CONVENTIONS };
    /// const _: () = assert!(TOOL.defect().is_none());
    /// ```
    ///
    /// The point of the base is what it does to a release. Adding a field to
    /// a struct every consumer writes as a literal breaks every consumer;
    /// adding one that this const already answers does not.
    pub const CONVENTIONS: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "",
        config_file: "",
        pin_keys: PinKeys {
            version: "",
            rev: "",
            tag: "",
            branch: "",
            git: "",
        },
        engine_crate: "",
        engine_bin: None,
        cache_namespace: "",
        cache_retention: Duration::from_secs(30 * 24 * 60 * 60),
        scan_skip: &[".git", "target", "node_modules"],
        default_url: "",
        launcher_crate: "",
        workdir: None,
        dir_flag: Cli::DIR_FLAG,
        engine_flag: Cli::ENGINE_FLAG,
        locate: Some(Locate::DEFAULT),
        self_update: SelfUpdate::ChaseTheBranch,
        hooks: Hooks::NONE,
    };

    /// The file name of the binary the engine's build produces.
    #[must_use]
    pub const fn engine_bin_name(&self) -> &'static str {
        match self.engine_bin {
            Some(name) => name,
            None => self.engine_crate,
        }
    }

    /// The first thing wrong with this descriptor, or `None`.
    ///
    /// Every name here ends up in a path, a command line, an environment
    /// variable or a config key, and an empty one produces a launcher that runs
    /// and does the wrong thing quietly rather than one that fails. The worst is
    /// an empty [`Tool::engine_bin`]: the built binary is then looked for at the
    /// `bin/` directory itself, which is never a file, so the engine is rebuilt
    /// on every single run and nothing says why.
    ///
    /// Const, so a tool can settle it at build time:
    ///
    /// ```
    /// # use renki::{pin_keys, Tool};
    /// # const TOOL: Tool = Tool {
    /// #     short: "widget", config_file: "w.toml", pin_keys: pin_keys!("w"),
    /// #     engine_crate: "w-engine", cache_namespace: "w",
    /// #     default_url: "u", launcher_crate: "w", ..Tool::CONVENTIONS
    /// # };
    /// const _: () = assert!(TOOL.defect().is_none());
    /// ```
    ///
    /// [`run`](crate::run) reports this and exits rather than starting.
    #[must_use]
    pub const fn defect(&self) -> Option<&'static str> {
        if !self.short_is_usable() {
            return Some(
                "short is not usable as an environment variable name, so the tool's \
                 own overrides could never be set",
            );
        }
        // Every remaining name, in the order a run reaches them. The message
        // names the field rather than describing the symptom, because the
        // symptom is usually somewhere else entirely by the time it shows.
        if self.config_file.is_empty() {
            return Some("config_file is empty, so discovery has nothing to look for");
        }
        if let Some(bad) = self.pin_keys.defect() {
            return Some(bad);
        }
        if self.engine_crate.is_empty() {
            return Some("engine_crate is empty, so there is nothing to build");
        }
        if let Some(bin) = self.engine_bin
            && bin.is_empty()
        {
            return Some(
                "engine_bin is empty, so the built binary is looked for at the bin \
                 directory itself and the engine rebuilds on every run",
            );
        }
        if self.cache_namespace.is_empty() {
            return Some(
                "cache_namespace is empty, so this tool would share a cache with every other",
            );
        }
        if self.launcher_crate.is_empty() {
            return Some(
                "launcher_crate is empty, so the update check can never find its own install",
            );
        }
        if self.default_url.is_empty() {
            return Some(
                "default_url is empty, so the git attempts ask cargo to install from \
                 nowhere and it fails naming nothing the user wrote",
            );
        }
        if self.dir_flag.is_empty() || self.engine_flag.is_empty() {
            return Some(
                "dir_flag or engine_flag is empty, which puts a bare empty argument on the command line",
            );
        }
        if const_str_eq(self.dir_flag, self.engine_flag) {
            return Some(
                "dir_flag and engine_flag are the same string, so the override is \
                 unreachable: the directory is stripped first and nothing is left \
                 for the engine flag to find",
            );
        }
        if let Anchor::Marker(m) = self.anchor
            && m.is_empty()
        {
            return Some("the anchor marker is empty, so the root walk matches every directory");
        }
        None
    }

    /// Whether [`Tool::short`] can survive being made into an environment
    /// variable name.
    ///
    /// The short name is uppercased and suffixed to build `WIDGET_ROOT` and
    /// `WIDGET_NO_SELF_UPDATE`, and a shell will not set a variable holding a
    /// hyphen or a dot. A name like `my-tool` therefore produces overrides
    /// nobody can use, and nothing about the run says so.
    ///
    /// Const, so a tool can settle it at compile time rather than finding out:
    ///
    /// ```
    /// # use renki::{pin_keys, Tool};
    /// # const TOOL: Tool = Tool {
    /// #     short: "widget", config_file: "w.toml", pin_keys: pin_keys!("w"),
    /// #     engine_crate: "w-engine", cache_namespace: "w",
    /// #     default_url: "u", launcher_crate: "w", ..Tool::CONVENTIONS
    /// # };
    /// const _: () = assert!(TOOL.short_is_usable());
    /// ```
    ///
    /// [`run`](crate::run) refuses a tool that fails this, through
    /// [`Tool::defect`], rather than running with two overrides that silently
    /// do nothing.
    #[must_use]
    pub const fn short_is_usable(&self) -> bool {
        let b = self.short.as_bytes();
        if b.is_empty() || b[0].is_ascii_digit() {
            return false;
        }
        let mut i = 0;
        while i < b.len() {
            if !(b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                return false;
            }
            i += 1;
        }
        true
    }

    /// The environment variable naming the repo root, overriding the `.git`
    /// walk: the short name uppercased, plus `_ROOT`.
    pub fn root_env(&self) -> String {
        format!("{}_ROOT", self.short.to_uppercase())
    }

    /// The environment variable naming this launcher's cache directory,
    /// overriding `$XDG_CACHE_HOME` and the namespace both: the short name
    /// uppercased, plus `_CACHE`.
    pub fn cache_env(&self) -> String {
        format!("{}_CACHE", self.short.to_uppercase())
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
#[path = "tool_tests.rs"]
mod tests;

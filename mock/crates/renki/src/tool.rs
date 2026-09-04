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

use crate::command::Command;
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
/// enum is left open. A consumer writes an [`Anchor`] into its descriptor and
/// is not asked to match one, so a third variant costs nothing to anyone who
/// only constructs, and the marker makes adding it a minor release.
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
/// the tool to decide rather than to know is a [`Hooks`] entry instead, and
/// that line is the whole rule for where a new member belongs.
pub struct Tool {
    /// How the repo root is found. See [`Anchor`].
    pub anchor:          Anchor,
    /// The short name this launcher answers to, used in its own diagnostics
    /// (`widget: ...`) and, uppercased, as the prefix of the environment
    /// variables it reads: `WIDGET_ROOT` overrides discovery, `WIDGET_CACHE`
    /// overrides where the built engines go, and `WIDGET_NO_SELF_UPDATE` turns
    /// the update check off.
    pub short:           &'static str,
    /// The one config file a repo carries, e.g. `widget.toml`.
    pub config_file:     &'static str,
    /// What the pin keys inside that config are called. See [`PinKeys`].
    pub pin_keys:        PinKeys,
    /// The engine's package name, which is what `cargo install` is asked for.
    pub engine_crate:    &'static str,
    /// The binary that package installs, when it is not named after the
    /// package. A crate `widget-engine` whose `[[bin]]` is `widget` names
    /// `Some("widget")` here.
    ///
    /// `None` is the ordinary case and means the two are the same.
    pub engine_bin:      Option<&'static str>,
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
    pub scan_skip:       &'static [&'static str],
    /// The engine's source when the config names none.
    pub default_url:     &'static str,
    /// Where a `version` pin is allowed to look. See [`VersionSource`].
    pub version_source:  VersionSource,
    /// This launcher's own package name, which is how it recognises its own
    /// entry in cargo's install ledger when checking for an update.
    pub launcher_crate:  &'static str,
    /// The working subdirectory the engine is pointed at, when the tool has
    /// one. `None` runs the engine against the repo root.
    pub workdir:         Option<Workdir>,
    /// The flag the engine takes its absolute working directory on. The
    /// launcher always passes it, and strips any copy the user wrote, so the
    /// engine never has to decide which of two answers is right.
    ///
    /// [`Cli::DIR_FLAG`] is the conventional `--dir`.
    pub dir_flag:        &'static str,
    /// The flag that points the launcher at a checkout on disk instead of the
    /// pinned engine. Consumed by the launcher and never forwarded, so an
    /// engine wanting a flag of this name needs a different one here.
    ///
    /// [`Cli::ENGINE_FLAG`] is the conventional `--engine`.
    pub engine_flag:     &'static str,
    /// The query the launcher answers itself, and what it calls the parts of
    /// its answer. `None` for a tool that answers no such query and forwards
    /// every subcommand to the engine. See [`Locate`].
    pub locate:          Option<Locate>,
    /// Whether this launcher keeps itself current. See [`SelfUpdate`].
    pub self_update:     SelfUpdate,
    /// The settings this tool has, as `renki-config` rows over the TOML store.
    /// Resolved once per run from a `--cfg` flag, the tool's `<SHORT>_CFG_<KEY>`
    /// variable, the repository's file, the person's file and the default, in
    /// that order, and handed to the engine as `<SHORT>_CFG_<KEY>` variables.
    /// Empty for a tool with nothing to configure, which also leaves the
    /// `config` subcommand to the engine.
    pub settings:        &'static [renki_config::Declared<crate::config::Toml>],
    /// The subcommands this launcher answers itself, beside `locate` and
    /// `config`, for the questions the engine cannot be asked. Tried where
    /// `config` is, before a root is required, so one runs where there is no
    /// repository at all. Empty for a tool with none. See [`Command`].
    pub commands:        &'static [Command],
    /// The tool-specific parts, all optional.
    pub hooks:           Hooks,
}

/// Where a `version` pin is allowed to resolve the engine from.
///
/// A version is the one pin form with two possible sources. A rev, a tag and a
/// branch all name something inside the repository the pin's url points at; a
/// version could mean that repository's tag of the same name, or it could mean
/// a release of [`Tool::engine_crate`] on crates.io, and those are not the same
/// artifact unless somebody has made them so.
///
/// The registry resolves by name, with nothing tying that name to the url.
/// So a tool whose engine crate name is unclaimed, or claimed by somebody else,
/// resolves a version pin to a stranger's code and runs it as the engine. Every
/// tool starts out with an unclaimed name, which is why the base answers this
/// with [`GitTag`](VersionSource::GitTag) rather than with the faster one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VersionSource {
    /// The matching tag in the pinned repository, and nowhere else.
    ///
    /// The only source the config actually named, so the engine that runs is
    /// the engine the repository asked for whatever the registry holds. Costs
    /// a git fetch on a cold build, once per version.
    GitTag,
    /// The registry release first, falling back to the matching tag.
    ///
    /// Faster on a cold build, and correct **only when the tool owns
    /// [`Tool::engine_crate`] on crates.io**. Choosing this is a statement that
    /// the name is yours and will stay yours. It is not checked and cannot be:
    /// a name is claimed or not at the moment somebody runs the launcher, not
    /// at the moment the descriptor is written.
    RegistryThenGitTag,
}

/// Conventional spellings for the two launcher flags.
///
/// Named constants rather than defaults, since [`Tool`] is a plain struct with
/// no `Default`: a tool with no opinion writes `Cli::DIR_FLAG` and reads as
/// having chosen it.
pub struct Cli;

impl Cli {
    /// The settings flag, `--cfg key=value`, which is not a tool's to rename:
    /// it is the launcher's on every tool with settings and the engine's on
    /// every tool without.
    pub const CFG_FLAG: &'static str = "--cfg";
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
    pub subcommand:  &'static str,
    /// What the answer calls the repo root.
    pub root_key:    &'static str,
    /// What the answer calls the config file's path. Empty in the answer when
    /// the repo has a working directory and no config, which is a real shape
    /// rather than a broken one.
    pub config_key:  &'static str,
    /// What the answer calls the working directory. Empty in the answer when
    /// there is no such directory on disk.
    pub workdir_key: &'static str,
}

impl Locate {
    /// The conventional spelling: a `locate` subcommand answering under `root`,
    /// `config` and `workdir`.
    pub const DEFAULT: Locate = Locate {
        subcommand:  "locate",
        root_key:    "root",
        config_key:  "config",
        workdir_key: "workdir",
    };

    /// The first thing wrong with the query, or `None`.
    ///
    /// An empty subcommand is matched by every bare argument, so the launcher
    /// answers the query instead of passing the argument to the engine. An
    /// empty answer key prints a line that is only a separator. Two keys the
    /// same prints the name twice with different values behind it, which a
    /// reader of the answer parses into whichever of the two comes last.
    const fn defect(&self) -> Option<&'static str> {
        if self.subcommand.is_empty() {
            return Some(
                "locate.subcommand is empty, so the query answers an empty argument \
                 and the engine never sees it",
            );
        }
        if self.root_key.is_empty() {
            return Some("locate.root_key is empty, so the root is answered under no name");
        }
        if self.config_key.is_empty() {
            return Some("locate.config_key is empty, so the config is answered under no name");
        }
        if self.workdir_key.is_empty() {
            return Some(
                "locate.workdir_key is empty, so the working directory is answered under no name",
            );
        }
        if const_str_eq(self.root_key, self.config_key)
            || const_str_eq(self.root_key, self.workdir_key)
            || const_str_eq(self.config_key, self.workdir_key)
        {
            return Some(
                "two of locate's answer keys are the same string, so the answer \
                 names one of them twice and a reader takes whichever came last",
            );
        }
        None
    }
}

/// What the pin keys inside a repo's config are called.
///
/// Full names rather than a prefix, on the same argument [`Locate`] is
/// parameterised by: a tool moving onto this crate keeps whatever its existing
/// configs already spell, and no repository has to be edited on the day it
/// migrates. The [`pin_keys!`](crate::pin_keys) macro writes the conventional shape and is
/// what a new tool reaches for.
///
/// Only top-level keys are read, so a name here is a top-level name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinKeys {
    /// A released version, resolved against the registry first and a tag
    /// second.
    pub version: &'static str,
    /// An exact commit.
    pub rev:     &'static str,
    /// A tag.
    pub tag:     &'static str,
    /// A moving branch head.
    pub branch:  &'static str,
    /// Where the engine's source is, overriding
    /// [`Tool::default_url`](crate::Tool::default_url).
    pub git:     &'static str,
}

impl PinKeys {
    /// The first empty key, named, or `None`.
    ///
    /// Named rather than described, because an empty key makes exactly one pin
    /// form unreadable and a message saying only that one of five is empty
    /// leaves the reader to find out which.
    const fn defect(&self) -> Option<&'static str> {
        if self.version.is_empty() {
            return Some("pin_keys.version is empty, so a version pin can never be read");
        }
        if self.rev.is_empty() {
            return Some("pin_keys.rev is empty, so a rev pin can never be read");
        }
        if self.tag.is_empty() {
            return Some("pin_keys.tag is empty, so a tag pin can never be read");
        }
        if self.branch.is_empty() {
            return Some("pin_keys.branch is empty, so a branch pin can never be read");
        }
        if self.git.is_empty() {
            return Some("pin_keys.git is empty, so a repo url can never be read");
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
            rev:     ::core::concat!($prefix, "_rev"),
            tag:     ::core::concat!($prefix, "_tag"),
            branch:  ::core::concat!($prefix, "_branch"),
            git:     ::core::concat!($prefix, "_git"),
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
    pub key:          &'static str,
    /// What a root-level config means when it does not set the key.
    pub root_default: &'static str,
}

impl Workdir {
    /// The first thing wrong with this, or `None`.
    const fn defect(&self) -> Option<&'static str> {
        if self.key.is_empty() {
            return Some(
                "workdir.key is empty, so a repo can never declare where its \
                 working directory is and every one of them falls back to the \
                 default",
            );
        }
        if self.root_default.is_empty() {
            return Some(
                "workdir.root_default is empty, so a root-level config that does \
                 not set the key puts the working directory at the root itself, \
                 which is the repo rather than a directory inside it",
            );
        }
        None
    }
}

/// The places a launcher does something only its own tool needs.
///
/// Every field is optional and defaults to doing nothing, so a tool that needs
/// none of this writes `Hooks::NONE`.
///
/// **Spread [`Hooks::NONE`] rather than naming every field.** This is a struct
/// literal, so a hook added later breaks a tool that wrote all seven out, and
/// one is expected: the cache key hashes the engine url, the resolved rev and
/// the toolchain, and a tool whose engine build depends on an input of its own
/// arrives here as an eighth field. The spread is what makes that a minor
/// release instead of a breaking one.
///
/// ```
/// # use renki::Hooks;
/// const HOOKS: Hooks = Hooks {
///     prepare_repo: Some(
///         |_root| { /* plant whatever this tool keeps in a repo */ },
///     ),
///     ..Hooks::NONE
/// };
/// ```
pub struct Hooks {
    /// Run once the repo and its config are located, before the engine is
    /// built. Whatever a tool must keep planted in a repo goes here, and it is
    /// best-effort by contract: it cannot fail the command the user ran.
    pub prepare_repo:      Option<fn(&Path)>,
    /// Extra arguments passed to the engine ahead of the user's, derived from
    /// the resolved pin. This is how a tool hands the engine something that
    /// must match the exact revision the engine was built from.
    pub engine_args:       Option<fn(&Resolved) -> Vec<String>>,
    /// The same, for the `--engine <path>` override, where there is no pin and
    /// the source is a working tree.
    pub engine_args_local: Option<fn(&Path) -> Vec<String>>,
    /// Refuse a [`Tool::engine_flag`] path that is not a checkout of this
    /// engine. Reported against the flag the user passed, rather than
    /// surfacing later as a build failure about something else.
    pub verify_engine_dir: Option<Check>,
    /// A last-resort pin for a repo that has not adopted an explicit one,
    /// given the working directory. Keeps a repo mid-migration running.
    pub legacy_pin:        Option<fn(&Path) -> Option<Pin>>,
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
    pub version_tags:      Option<fn(&str) -> Vec<String>>,
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
    /// A tool that needs none of the extension points, and the base every other
    /// tool spreads.
    pub const NONE: Hooks = Hooks {
        prepare_repo:      None,
        engine_args:       None,
        engine_args_local: None,
        verify_engine_dir: None,
        version_tags:      None,
        legacy_pin:        None,
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
    /// adding one this const already answers breaks only the consumers who
    /// wrote every field out instead of spreading the base. Spreading it is
    /// what buys the compatibility, so the guarantee is theirs and nobody
    /// else's.
    pub const CONVENTIONS: Tool = Tool {
        anchor:          Anchor::Marker(".git"),
        short:           "",
        config_file:     "",
        pin_keys:        PinKeys {
            version: "",
            rev:     "",
            tag:     "",
            branch:  "",
            git:     "",
        },
        engine_crate:    "",
        engine_bin:      None,
        cache_namespace: "",
        cache_retention: Duration::from_secs(30 * 24 * 60 * 60),
        scan_skip:       &[".git", "target", "node_modules"],
        default_url:     "",
        version_source:  VersionSource::GitTag,
        launcher_crate:  "",
        workdir:         None,
        dir_flag:        Cli::DIR_FLAG,
        engine_flag:     Cli::ENGINE_FLAG,
        locate:          Some(Locate::DEFAULT),
        self_update:     SelfUpdate::ChaseTheBranch,
        settings:        &[],
        commands:        &[],
        hooks:           Hooks::NONE,
    };

    /// The command `args` names, where the first argument is one of the
    /// tool's own.
    #[must_use]
    pub fn command_named(&self, args: &[std::ffi::OsString]) -> Option<&'static Command> {
        let first = args.first()?.to_str()?;
        self.commands.iter().find(|c| c.name == first)
    }

    /// The first thing wrong with the command table, or `None`.
    ///
    /// An empty name is matched by every bare argument, so the launcher
    /// answers the command instead of passing the argument to the engine. A
    /// name the crate's own queries take is unreachable, since those are
    /// tried first: `locate`'s subcommand, and `config` where the tool has
    /// settings. Two commands of one name is the same defect between
    /// themselves, decided by table order rather than by anything declared.
    const fn commands_defect(&self) -> Option<&'static str> {
        let mut i = 0;
        while i < self.commands.len() {
            let name = self.commands[i].name;
            if name.is_empty() {
                return Some(
                    "a command's name is empty, so the launcher answers an empty argument \
                     and the engine never sees it",
                );
            }
            if let Some(l) = self.locate
                && const_str_eq(name, l.subcommand)
            {
                return Some(
                    "a command is named the same as locate's subcommand, which is tried first, \
                     so the command never runs",
                );
            }
            if !self.settings.is_empty() && const_str_eq(name, crate::config::query::SUBCOMMAND) {
                return Some(
                    "a command is named `config`, which a tool with settings answers first, \
                     so the command never runs",
                );
            }
            // The flags come off the arguments before the table is looked
            // at, wherever they sit, so a command named after one is taken
            // as the flag: the directory flag vanishes with no message, the
            // engine flag is refused as a flag with nothing after it, and
            // `--cfg` swallows what follows as a value.
            if const_str_eq(name, self.dir_flag) {
                return Some(
                    "a command is named the same as dir_flag, which is stripped before the \
                     table is read, so the command vanishes",
                );
            }
            if const_str_eq(name, self.engine_flag) {
                return Some(
                    "a command is named the same as engine_flag, which is taken off before the \
                     table is read, so the command is refused as a flag with no path after it",
                );
            }
            if !self.settings.is_empty() && const_str_eq(name, Cli::CFG_FLAG) {
                return Some(
                    "a command is named `--cfg`, which a tool with settings takes off before \
                     the table is read, so the command is swallowed as a flag",
                );
            }
            let mut j = 0;
            while j < i {
                if const_str_eq(name, self.commands[j].name) {
                    return Some(
                        "two commands share a name, so the second is unreachable and which \
                         one runs is decided by table order",
                    );
                }
                j += 1;
            }
            i += 1;
        }
        None
    }

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
        if self.cache_retention.is_zero() {
            return Some(
                "cache_retention is zero, so every build is older than the window \
                 the moment it lands and the collector removes it on the next \
                 pass. The result is a full rebuild every run, under a message \
                 saying it happens once per version",
            );
        }
        // `as_secs`, because `Duration`'s comparison is not const and this is.
        if self.cache_retention.as_secs() < crate::pin::BRANCH_TTL.as_secs() {
            return Some(
                "cache_retention is shorter than the hour a branch resolution \
                 counts as the branch's current tip. The collector sweeps \
                 resolutions on this window, so one would be deleted while it \
                 is still live, and every run of a branch-pinned repo would ask \
                 the remote again",
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
        // The two optional descriptors. Absent is a shape rather than a defect:
        // a tool with no working directory and no locate query is ordinary.
        // Present and empty is not.
        if let Some(w) = self.workdir
            && let Some(bad) = w.defect()
        {
            return Some(bad);
        }
        if let Some(l) = self.locate
            && let Some(bad) = l.defect()
        {
            return Some(bad);
        }
        if let Some(bad) = self.config_keys_collide() {
            return Some(bad);
        }
        if let Some(bad) = self.commands_defect() {
            return Some(bad);
        }
        None
    }

    /// Whether two of the names read out of the repo's config are the same
    /// string.
    ///
    /// Six names come out of one `toml::Table`: the five pin keys and the
    /// working directory's. A collision between two of them is not a
    /// duplicated line in a config, it is one line answering two questions,
    /// and which answer wins is decided by the order the reader happens to try
    /// them in rather than by anything a descriptor said.
    ///
    /// Both halves do real damage. Two pin keys the same and the more specific
    /// form wins, so `version` spelled the same as `tag` is read as a tag,
    /// which skips the registry attempt and the `version_tags` rewrite and
    /// fetches a different artifact under the same config. The working
    /// directory sharing a pin key means one string is both a path and a
    /// revision.
    ///
    /// Separate from the emptiness checks above because it is a different
    /// question: those ask whether a name can be used at all, this asks
    /// whether two of them mean the same thing.
    const fn config_keys_collide(&self) -> Option<&'static str> {
        let k = &self.pin_keys;
        // Written out rather than looped, because a `const fn` cannot build the
        // array of `&str` this would iterate over, and the pairs are few.
        let names: [&'static str; 6] =
            [k.version, k.rev, k.tag, k.branch, k.git, match self.workdir {
                // A tool with no working directory has five names to compare,
                // and repeating one of them against itself is the cheapest way
                // to say so in a const context: it collides with nothing new.
                Some(w) => w.key,
                None => k.version,
            }];
        let mut i = 0;
        while i < names.len() {
            let mut j = i + 1;
            while j < names.len() {
                // The workdir slot standing in as `version` is the one pair
                // that must not count, and it is exactly `i == 0, j == 5`.
                let stand_in = i == 0 && j == 5 && self.workdir.is_none();
                if !stand_in && const_str_eq(names[i], names[j]) {
                    return Some(
                        "two of the names read out of a repo's config are the same string, \
                         so one line answers two questions and which one wins is decided by \
                         the order they are tried in",
                    );
                }
                j += 1;
            }
            i += 1;
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
        renki_dirs::Short::new(self.short).is_ok()
    }

    /// The short name as `renki-dirs` types it. A tool that fails
    /// [`short_is_usable`](Self::short_is_usable) is refused by [`run`](crate::run)
    /// before anything here is reached, so the refusal below names that check
    /// rather than repeating it.
    pub(crate) fn short_typed(&self) -> renki_dirs::Short<'_> {
        match renki_dirs::Short::new(self.short) {
            notko::Outcome::Ok(s) => s,
            notko::Outcome::Err(e) => {
                panic!(
                    "the tool's short name {:?} is not a variable name ({e:?}); \
                 `Tool::short_is_usable` is the check `run` refuses on",
                    self.short
                )
            },
        }
    }

    /// The environment variable naming the repo root, overriding the `.git`
    /// walk: the short name uppercased, plus `_ROOT`.
    pub fn root_env(&self) -> String {
        format!("{}_ROOT", self.short.to_uppercase())
    }

    /// The environment variable naming this launcher's cache directory,
    /// overriding `$XDG_CACHE_HOME` and the namespace both: the short name
    /// uppercased, plus `_CACHE`. Named by `renki-dirs`, so the launcher and
    /// a tool that reads the table without the launcher agree.
    pub fn cache_env(&self) -> String {
        renki_dirs::EnvName::<renki_dirs::Cache>::of(self.short_typed()).to_string()
    }

    /// The environment variable naming this launcher's state directory, where
    /// the registry and the self-update marker live: the short name
    /// uppercased, plus `_STATE`. Overrides `$XDG_STATE_HOME` and the
    /// namespace both, the way `cache_env` does for the cache.
    pub fn state_env(&self) -> String {
        renki_dirs::EnvName::<renki_dirs::State>::of(self.short_typed()).to_string()
    }

    /// The environment variable naming this launcher's config directory,
    /// where the person's settings file lives: the short name uppercased,
    /// plus `_CONFIG`. Overrides `$XDG_CONFIG_HOME` and the namespace both,
    /// the way `cache_env` does for the cache.
    pub fn config_env(&self) -> String {
        renki_dirs::EnvName::<renki_dirs::Config>::of(self.short_typed()).to_string()
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
        let default = if config_dir == root { wd.root_default } else { "." };
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

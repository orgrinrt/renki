//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The launcher half of a command-line tool whose engine each repo pins.
//!
//! A tool built this way has two pieces. The **engine** does the work and is
//! version-pinned by each repo that uses it. The **launcher** is what sits on
//! `PATH`: it finds the repo, reads the pin, builds that exact engine once into
//! a shared per-version cache, and execs it with an absolute working directory
//! so the shell's cwd never matters.
//!
//! The point is that a repo's tooling cannot drift from what the repo asked
//! for. A launcher installed from a git branch also keeps itself current; one
//! installed from a registry, a tag or a revision is pinned to what was asked
//! for and stays there.
//!
//! # Using it
//!
//! Declare a [`Tool`] as a `const`, and hand it over:
//!
//! ```no_run
//! use renki::{pin_keys, Tool};
//!
//! const TOOL: Tool = Tool {
//!     short:           "widget",
//!     config_file:     "widget.toml",
//!     pin_keys:        pin_keys!("widget"),
//!     engine_crate:    "widget-engine",
//!     cache_namespace: "widget",
//!     default_url:     "https://github.com/o/widget.git",
//!     launcher_crate:  "widget",
//!     ..Tool::CONVENTIONS
//! };
//!
//! fn main() -> std::process::ExitCode {
//!     // SAFETY: the first statement of main, before any thread exists.
//!     unsafe { renki::run(&TOOL) }
//! }
//! ```
//!
//! Everything that is one tool's and no other's goes through [`Hooks`] rather
//! than into this crate.

// The crate's whole selling point is a small honest surface, and both of these
// guard exactly that claim. `unreachable_pub` caught thirty-one items marked
// public inside private modules, and one genuinely public type hiding in that
// noise that the crate root had forgotten to re-export.
#![warn(unreachable_pub, missing_docs)]

// The README's `rust` block is compiled as a doctest. Only that one: the shell
// and toml blocks are prose as far as this is concerned, and changing the fence
// would drop the check with nothing saying so.
//
// It went out of date once already, when `Tool` grew fields and the block kept
// the old shape, which read as correct and did not build.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct Readme;

// The launcher's whole job ends in `CommandExt::exec`, which replaces the
// process rather than spawning a child, and unix is the only place that call
// exists. Without this, a non-unix build fails on `use
// std::os::unix::process::CommandExt` in two files and says nothing about why,
// leaving the reader to work out that the design assumes a handover rather
// than that an import went missing.
#[cfg(not(unix))]
compile_error!(
    "renki execs the engine in place of itself, which is a unix operation. \
     There is no portable equivalent, so a port needs a different handover \
     rather than a different import."
);

mod args;
mod cache;
mod discover;
mod engine;
mod env;
mod hash;
mod manifest;
mod pin;
mod registry;
mod selfupdate;
mod tool;

use std::io::Write as _;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::args::{is_the_locate_query, normalize_args};

pub use crate::env::{GIT_REPO_ENV, sanitize_git_env};
pub use crate::manifest::{Header, Pin, Reference, package_name};
pub use crate::pin::Resolved;
pub use crate::tool::{Anchor, Check, Cli, Hooks, Locate, PinKeys, SelfUpdate, Tool, Workdir};

/// Where a resolved pin came from, so the registry can tell a repo that has
/// adopted an explicit pin from one still on whatever legacy fallback the tool
/// honours. That difference is the migration-detection signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinSource {
    Config,
    Legacy,
}

/// The launcher entry. A tool's `main` is this and nothing else.
///
/// A launcher usually runs as a child of a git hook, whose exported
/// repo-location `GIT_*` variables would poison every `git` this process and
/// the engine it spawns invoke from a different working directory. This drops
/// them, which is what makes it unsafe: the environment is process-global and
/// unsynchronised, so the removal is sound only in a process that has not
/// started a thread yet. That is a fact about the caller, which is why the
/// caller states it.
///
/// # Safety
///
/// Call as the first statement of `main`, before anything spawns a thread and
/// before anything reads the environment. See [`sanitize_git_env`], whose
/// contract this inherits whole.
///
/// ```no_run
/// # const TOOL: renki::Tool = renki::Tool {
/// #     anchor: renki::Anchor::ConfigFile, short: "w", config_file: "w.toml",
/// #     pin_keys: renki::pin_keys!("w"), engine_crate: "w-engine",
/// #     cache_namespace: "w", default_url: "https://example.invalid/w.git",
/// #     launcher_crate: "w", ..renki::Tool::CONVENTIONS
/// # };
/// fn main() -> std::process::ExitCode {
///     // SAFETY: the first statement of main, before any thread exists.
///     unsafe { renki::run(&TOOL) }
/// }
/// ```
///
/// [`run_without_sanitizing`] is the safe door, for a caller that has already
/// scrubbed or that is not a hook descendant and wants its environment left
/// alone.
pub unsafe fn run(tool: &Tool) -> ExitCode {
    // SAFETY: forwarded to this function's own caller, unchanged.
    unsafe { sanitize_git_env() };
    run_without_sanitizing(tool)
}

/// The launcher entry, without touching the environment.
///
/// Safe, and correspondingly narrower: a launcher reached from a git hook needs
/// the scrub [`run`] performs, or every `git` it and its engine run reads the
/// hook's repository rather than the one under the working directory. Reach for
/// this when the caller has already sanitised, or when it knows it is not a
/// hook descendant.
pub fn run_without_sanitizing(tool: &Tool) -> ExitCode {
    let raw: Vec<String> = std::env::args().collect();
    match outcome(tool, &raw) {
        Ok(()) => ExitCode::SUCCESS, // unreachable when the exec succeeds
        Err(e) => {
            eprintln!("{}: {e}", tool.short);
            ExitCode::FAILURE
        }
    }
}

/// What the launcher did, as a value rather than as an exit code and a line on
/// stderr.
///
/// Split out because an exit code cannot say *why*, and both failures below
/// produce the same one. A test asserting that a broken descriptor is refused
/// passes just as well against a launcher that got as far as looking for a
/// repository and did not find one, which is not the same thing at all and is
/// how the check ends up unwired without anything reporting it.
fn outcome(tool: &Tool, raw: &[String]) -> Result<(), String> {
    // Refused before anything else, because what a bad descriptor produces is
    // silence rather than an error: a short name no shell can spell leaves both
    // overrides unsettable, and an empty engine binary name rebuilds the engine
    // on every run forever. `Tool::defect` is const, so a tool can settle this
    // at build time instead of shipping and finding out.
    if let Some(defect) = tool.defect() {
        return Err(format!(
            "this launcher's descriptor is not usable: {defect}"
        ));
    }
    dispatch(tool, &normalize_args(tool, raw))
}

/// Print where this checkout keeps its config and working directory, as
/// shell-assignable lines.
///
/// [`discover::locate`] is the authority on the search. Every other consumer of
/// that answer should ask here rather than walking the tree itself: a tool's
/// git hooks typically carry a shell reimplementation that has to be kept in
/// step, and a third copy is how the three drift apart.
///
/// ```text
/// root=/path/to/repo
/// config=/path/to/repo/widget.toml
/// workdir=/path/to/repo/work
/// ```
///
/// `config` is empty when the repo has a working directory but no config, which
/// is a real shape rather than a broken one.
///
/// The three names come from [`Locate`], which is what lets a tool keep the
/// spelling its existing readers already parse.
///
/// One value per line, in the tool's own bytes, with no quoting and no
/// escaping. That is the whole format, and it is injective over every path it
/// accepts: the first `=` separates, so a later one in a directory name is
/// unambiguous, and a reader splitting on the first `=` is correct. A newline
/// is the one legal path byte the format cannot carry, since a reader would
/// see two records and the second would have no `=`, so a path containing one
/// is refused by name rather than answered wrongly.
fn locate_query(tool: &Tool, locate: &Locate) -> Result<(), String> {
    let root = discover::repo_root(tool).ok_or_else(|| discover::no_root(tool))?;
    let located = discover::locate(tool, &root)?;
    let (config, workdir) = match located {
        Some(l) => (l.config_path, l.workdir),
        // No config, so the conventional directory if it is there at all. The
        // caller distinguishes by the empty `config`.
        None => (PathBuf::new(), tool.workdir_default(&root)),
    };
    let answer = locate_answer(locate, &root, &config, &workdir)?;
    // Written as bytes rather than printed, because a path is bytes and
    // `Display` replaces whatever is not text with a character that names no
    // file. A reader `cd`ing into the answer has to get the directory that is
    // there.
    std::io::stdout()
        .write_all(&answer)
        .map_err(|e| format!("could not write the answer: {e}"))
}

/// The locate answer as text, so the keys can be checked without a subprocess.
///
/// Separate from the printing because the keys were fields nothing read: all
/// three were hardcoded here while [`Locate`] documented them as a contract
/// with a tool's shell helpers, so any tool that set them got the conventional
/// spellings and its own readers found nothing.
fn locate_answer(
    locate: &Locate,
    root: &Path,
    config: &Path,
    workdir: &Path,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut line = |key: &str, value: &Path| -> Result<(), String> {
        let bytes = value.as_os_str().as_bytes();
        if bytes.contains(&b'\n') {
            return Err(format!(
                "{key} is a path containing a newline, which this answer has no way to \
                 carry: a reader would see it as two records. {}",
                value.display()
            ));
        }
        out.extend_from_slice(key.as_bytes());
        out.push(b'=');
        out.extend_from_slice(bytes);
        out.push(b'\n');
        Ok(())
    };
    line(locate.root_key, root)?;
    line(locate.config_key, config)?;
    // An absent working directory answers with the key and no value rather than
    // omitting the line, so a reader can tell "no such directory" from "this
    // launcher is too old to answer".
    if workdir.is_dir() {
        line(locate.workdir_key, workdir)?;
    } else {
        line(locate.workdir_key, Path::new(""))?;
    }
    Ok(out)
}

/// How many times a build evicted out from under us is re-materialised before
/// the run gives up.
///
/// Each attempt is a full `cargo install` when it fires at all, so this is not
/// a busy loop; it is the number of collection passes that would have to land
/// in the same narrow gap for a run to fail.
const EVICTION_RETRIES: usize = 3;

fn dispatch(tool: &Tool, args: &[String]) -> Result<(), String> {
    // `--engine <path>` is the launcher's, so it comes off before anything
    // reads the arguments, the same as `--dir`.
    let (engine_override, args) = engine::take_flag(args.to_vec(), tool.engine_flag);
    // Passed with nothing after it. Running the pinned engine here is the exact
    // opposite of what was asked, and it is the silent kind of wrong: the
    // override path announces itself on stderr and this path would not.
    if engine_override == engine::Flag::Missing {
        return Err(format!(
            "{} needs a path to an engine checkout, and none followed it",
            tool.engine_flag
        ));
    }
    let engine_override = engine_override.value();
    let args = args.as_slice();

    // Answered before anything else: it is a question about this checkout
    // rather than a run of the engine, so it skips the self-update, the pin
    // resolution and the build.
    if let Some(locate) = tool.locate.as_ref()
        && is_the_locate_query(Some(locate), args)
    {
        return locate_query(tool, locate);
    }

    let root = discover::repo_root(tool).ok_or_else(|| discover::no_root(tool))?;

    // Keep the launcher itself current: branch installs only, hourly, opt-out.
    // May reinstall and re-exec into the new binary, never returning.
    //
    // After the root, deliberately. Typing the tool's name somewhere that is
    // not a repository is the fastest-failing invocation it has, and running
    // this first made it one of the slowest: a network round trip, and
    // possibly a full reinstall and re-exec, before failing with "no .git
    // found in this directory or any above it".
    //
    // Skipped under `--engine`, which says which engine to run: replacing the
    // launcher underneath a deliberate override is the one moment an automatic
    // update is unwelcome.
    if engine_override.is_none()
        && let Ok(cache_root) = cache::cache_root(tool)
    {
        selfupdate::maybe_self_update(tool, &cache_root);
    }

    // `locate` hard-errors, blocking the run, when a marker-anchored repo has
    // more than one config. `None` means none, `Some` exactly one.
    let located = discover::locate(tool, &root)?;
    let workdir = located
        .as_ref()
        .map(|l| l.workdir.clone())
        .unwrap_or_else(|| tool.workdir_default(&root));

    // A repo state that would silently route the user somewhere else is refused
    // rather than tolerated. A retired cargo alias shadowing the launcher is the
    // case this exists for: the two spellings must be the same tool.
    // Every run that found a root, config or none. A repo with no config is a
    // repo that has not adopted the launcher, which is where a stale route left
    // over from whatever it used before is most likely to still be in place, so
    // gating this on a config found would skip the case it exists for.
    if let Some(verify) = tool.hooks.verify_repo_state {
        verify(&root)?;
    }

    // Whatever the tool keeps planted in a repo goes in BEFORE the engine is
    // built.
    //
    // Leaving it to the engine leaves a window nothing covers: every way the
    // engine can fail to run also leaves the repo unprepared, silently. Its
    // build can fail on a bad pin, on no network, or on a compile error in the
    // pinned revision. The launcher cannot fail for any of those reasons.
    //
    // Best-effort by contract: it must not stop the command the user ran.
    if located.is_some()
        && let Some(prepare) = tool.hooks.prepare_repo
    {
        prepare(&root);
    }

    let cache_root = cache::cache_root(tool)?;

    // The scratch path: build the engine from a checkout on disk and run this
    // repo against it. No pin is resolved, nothing is recorded in the registry
    // and nothing is keyed by revision, because the source is a working tree
    // and the question is what it does right now.
    if let Some(raw) = engine_override {
        let source = engine::locate(tool, &raw)?;
        eprintln!(
            "{}: ENGINE OVERRIDE: running the engine at {} instead of this repo's pinned engine",
            tool.short,
            source.display()
        );
        let bin = engine::build(tool, &cache_root, &source)?;
        let extra = tool
            .hooks
            .engine_args_local
            .map(|f| f(&source))
            .unwrap_or_default();
        return cache::exec_engine(tool, &bin, &workdir, &extra, args).map(|_never| ());
    }

    let (pin, source) = resolve_pin(tool, located.as_ref(), &root, &workdir)?;
    let resolved = pin::resolve(tool, &pin, &cache_root)?;
    let toolchain = cache::rustc_fingerprint();
    let key = cache::compute_key(&pin.url, &resolved.key_rev, &toolchain);
    let bin = cache::ensure_built(tool, &cache_root, &key, &resolved)?;

    // Record this repo and build in the registry, then at most once a day
    // collect engine builds nothing pins anymore. Best-effort throughout: the
    // registry is a cache, never a reason to fail a run.
    registry::record_and_collect(
        tool,
        &cache_root,
        &root,
        &workdir,
        &pin,
        source,
        &resolved,
        &toolchain,
        &key,
    );

    // A concurrent launcher's collection pass protects only its own resolved
    // key, so it could have evicted this build between our build and this exec.
    // Re-materialise if so.
    //
    // Bounded rather than unbounded, and bounded rather than the single retry
    // this used to be. One retry narrows the window and the comment above it
    // claimed the window was closed: the same collection can evict the rebuilt
    // binary between the retry and the exec, and then the run fails. A few
    // attempts make that need a collection pass landing in the same gap
    // repeatedly, which is not something to keep trying forever either, since
    // a build that keeps vanishing is a fault rather than a race.
    let mut bin = bin;
    for _ in 0..EVICTION_RETRIES {
        if bin.is_file() {
            break;
        }
        bin = cache::ensure_built(tool, &cache_root, &key, &resolved)?;
    }

    let extra = tool
        .hooks
        .engine_args
        .map(|f| f(&resolved))
        .unwrap_or_default();
    cache::exec_engine(tool, &bin, &workdir, &extra, args).map(|_never| ())
}

/// Unix seconds now, or 0 if the clock is before the epoch, which is impossible
/// in practice and which the registry reads as very old.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The pin: the config's own key, then whatever legacy fallback the tool
/// honours, which keeps a repo mid-migration running until it adopts one.
fn resolve_pin(
    tool: &Tool,
    located: Option<&discover::Located>,
    root: &Path,
    workdir: &Path,
) -> Result<(Pin, PinSource), String> {
    if let Some(l) = located
        && let Ok(s) = std::fs::read_to_string(&l.config_path)
        && let Some(p) = Header::parse(tool, &s).to_pin(tool)
    {
        return Ok((p, PinSource::Config));
    }
    if let Some(legacy) = tool.hooks.legacy_pin
        && let Some(p) = legacy(workdir)
    {
        return Ok((p, PinSource::Legacy));
    }
    let where_to = located
        .map(|l| l.config_path.clone())
        .unwrap_or_else(|| root.join(tool.config_file));
    Err(format!(
        "no {} pin found. add one to {}:\n\n    {} = \"0.1.0\"   # the released engine \
         version\n",
        tool.engine_crate,
        where_to.display(),
        tool.pin_keys.version
    ))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

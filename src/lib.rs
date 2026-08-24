//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The launcher half of a pinned-engine command-line tool.
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
//! use renki::{Anchor, Cli, Hooks, Locate, Tool};
//!
//! const TOOL: Tool = Tool {
//!     anchor:          Anchor::Marker(".git"),
//!     short:           "widget",
//!     config_file:     "widget.toml",
//!     pin_prefix:      "widget",
//!     engine_crate:    "widget-engine",
//!     engine_bin:      None,
//!     cache_namespace: "widget",
//!     default_url:     "https://github.com/o/widget.git",
//!     launcher_crate:  "widget",
//!     workdir:         None,
//!     dir_flag:        Cli::DIR_FLAG,
//!     engine_flag:     Cli::ENGINE_FLAG,
//!     locate:          Locate::DEFAULT,
//!     hooks:           Hooks::NONE,
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

use std::path::Path;
use std::process::ExitCode;

pub use crate::env::{GIT_REPO_ENV, sanitize_git_env};
pub use crate::manifest::{Header, Pin, Reference};
pub use crate::pin::Resolved;
pub use crate::tool::{Anchor, Check, Cli, Hooks, Locate, Tool, Workdir};

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
/// #     pin_prefix: "w", engine_crate: "w-engine", engine_bin: None, cache_namespace: "w",
/// #     default_url: "https://example.invalid/w.git", launcher_crate: "w",
/// #     workdir: None, dir_flag: renki::Cli::DIR_FLAG,
/// #     engine_flag: renki::Cli::ENGINE_FLAG, locate: renki::Locate::DEFAULT,
/// #     hooks: renki::Hooks::NONE,
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
/// workdir=/path/to/repo/mock
/// ```
///
/// `config` is empty when the repo has a working directory but no config, which
/// is a real shape rather than a broken one.
///
/// The three names come from [`Locate`], which is what lets a tool keep the
/// spelling its existing readers already parse.
fn locate_query(tool: &Tool) -> Result<(), String> {
    let root = discover::repo_root(tool).ok_or_else(|| no_root(tool))?;
    let located = discover::locate(tool, &root)?;
    let (config, workdir) = match located {
        Some(l) => (l.config_path.display().to_string(), l.workdir),
        // No config, so the conventional directory if it is there at all. The
        // caller distinguishes by the empty `config`.
        None => (String::new(), tool.workdir_default(&root)),
    };
    print!("{}", locate_answer(&tool.locate, &root, &config, &workdir));
    Ok(())
}

/// The locate answer as text, so the keys can be checked without a subprocess.
///
/// Separate from the printing because the keys were fields nothing read: all
/// three were hardcoded here while [`Locate`] documented them as a contract
/// with a tool's shell helpers, so any tool that set them got the conventional
/// spellings and its own readers found nothing.
fn locate_answer(locate: &Locate, root: &Path, config: &str, workdir: &Path) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}={}\n", locate.root_key, root.display()));
    out.push_str(&format!("{}={config}\n", locate.config_key));
    // An absent working directory answers with the key and no value rather than
    // omitting the line, so a reader can tell "no such directory" from "this
    // launcher is too old to answer".
    if workdir.is_dir() {
        out.push_str(&format!("{}={}\n", locate.workdir_key, workdir.display()));
    } else {
        out.push_str(&format!("{}=\n", locate.workdir_key));
    }
    out
}

fn no_root(tool: &Tool) -> String {
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
        Some(v) => format!(
            "no {what} found in this directory or any above it. {env} is set to {}, which is \
             not a directory, so it was ignored",
            Path::new(&v).display()
        ),
        None => format!("no {what} found in this directory or any above it, and {env} is unset"),
    }
}

/// The user-facing arguments to forward to the engine.
///
/// Two invocation shapes collapse to one. Invoked directly, every argument is
/// forwarded. Invoked as a cargo external subcommand, cargo executes
/// `cargo-<x> <x> <args...>`, so a leading `<x>` is dropped when the program
/// name is `cargo-<x>`. That is cargo's convention rather than any one tool's,
/// which is why it lives here.
///
/// A user-supplied [`Tool::dir_flag`] is stripped, in either spelling: the
/// launcher owns it and always passes the absolute working directory.
fn normalize_args(tool: &Tool, raw: &[String]) -> Vec<String> {
    let prog = raw
        .first()
        .map(|p| {
            Path::new(p)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();
    let mut rest: Vec<String> = raw.iter().skip(1).cloned().collect();
    if let Some(sub) = prog.strip_prefix("cargo-")
        && rest.first().map(String::as_str) == Some(sub)
    {
        rest.remove(0);
    }
    strip_dir_flag(rest, tool.dir_flag)
}

/// Drop a user-supplied `<dir_flag> <value>` pair anywhere in the args, in
/// either spelling. The launcher passes its own, so a second one is an
/// ambiguity the engine should never have to resolve.
///
/// A `<dir_flag>` with no value after it is dropped too, and that is not the
/// oversight it looks like beside the engine flag's refusal below. The user's
/// directory is discarded whether they named one or not, so naming nothing
/// changes nothing about the run.
fn strip_dir_flag(args: Vec<String>, dir_flag: &str) -> Vec<String> {
    engine::take_flag(args, dir_flag).1
}

/// Whether these arguments ask the launcher the locate question rather than
/// asking the engine anything.
///
/// Its own function because the guard on the left is load-bearing and easy to
/// lose: without it, a tool that wants no locate query at all has
/// `subcommand: None`, an invocation with no arguments compares `None` against
/// `None`, and every bare run answers the query instead of running the engine.
fn is_the_locate_query(locate: &Locate, args: &[String]) -> bool {
    locate.subcommand.is_some() && args.first().map(String::as_str) == locate.subcommand
}

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
    if is_the_locate_query(&tool.locate, args) {
        return locate_query(tool);
    }

    // Keep the launcher itself current: branch installs only, hourly, opt-out.
    // May reinstall and re-exec into the new binary, never returning.
    //
    // Skipped under `--engine`, which says which engine to run: replacing the
    // launcher underneath a deliberate override is the one moment an automatic
    // update is unwelcome.
    if engine_override.is_none()
        && let Ok(cache_root) = cache::cache_root(tool)
    {
        selfupdate::maybe_self_update(tool, &cache_root);
    }

    let root = discover::repo_root(tool).ok_or_else(|| no_root(tool))?;
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
    record_and_gc(
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
    // Re-materialise if so, so a background collection never fails a run.
    let bin = if bin.is_file() {
        bin
    } else {
        cache::ensure_built(tool, &cache_root, &key, &resolved)?
    };

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

/// The registry pin form and value for a resolved pin. Legacy overrides the
/// reference variant: a legacy pin is always a rev, but must register as legacy
/// for migration detection.
fn pin_form_and_value(pin: &Pin, source: PinSource) -> (registry::PinForm, String) {
    let value = match &pin.reference {
        Reference::Version(v) | Reference::Branch(v) | Reference::Rev(v) | Reference::Tag(v) => {
            v.clone()
        }
    };
    let form = match source {
        PinSource::Legacy => registry::PinForm::Legacy,
        PinSource::Config => match &pin.reference {
            Reference::Version(_) => registry::PinForm::Version,
            Reference::Branch(_) => registry::PinForm::Branch,
            Reference::Rev(_) => registry::PinForm::Rev,
            Reference::Tag(_) => registry::PinForm::Tag,
        },
    };
    (form, value)
}

/// Record this repo and its resolved build, then run a throttled collection
/// pass protecting the just-resolved key. Every step is best-effort; a registry
/// failure never blocks the exec.
#[allow(clippy::too_many_arguments)]
fn record_and_gc(
    tool: &Tool,
    cache_root: &Path,
    root: &Path,
    workdir: &Path,
    pin: &Pin,
    source: PinSource,
    resolved: &Resolved,
    toolchain: &str,
    key: &str,
) {
    let path = registry::registry_path(cache_root);
    let mut reg = registry::Registry::load(&path);
    let now = now_secs();
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let (form, value) = pin_form_and_value(pin, source);
    reg.record(
        &root.display().to_string(),
        &name,
        &workdir.display().to_string(),
        &pin.url,
        form,
        &value,
        key,
        &resolved.key_rev,
        toolchain,
        now,
    );
    if reg.gc_due(now) {
        let removed = reg.gc(cache_root, key, now);
        if !removed.is_empty() {
            eprintln!(
                "{}: cache gc removed {} unused engine build(s)",
                tool.short,
                removed.len()
            );
        }
    }
    reg.save(&path);
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
        "no {} pin found. add one to {}:\n\n    {}_version = \"0.1.0\"   # the released engine \
         version\n",
        tool.engine_crate,
        where_to.display(),
        tool.pin_prefix
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "widget",
        config_file: "t.toml",
        pin_prefix: "t",
        engine_crate: "engine",
        engine_bin: None,
        cache_namespace: "t",
        default_url: "u",
        launcher_crate: "cargo-widget",
        workdir: None,
        dir_flag: Cli::DIR_FLAG,
        engine_flag: Cli::ENGINE_FLAG,
        locate: Locate::DEFAULT,
        hooks: Hooks::NONE,
    };

    #[test]
    fn a_launcher_with_a_broken_descriptor_refuses_to_start() {
        // The point of the check is that it runs, and a predicate tested only
        // as a predicate stays green when nothing calls it. Every arm below is
        // a descriptor that would otherwise run and misbehave quietly.
        const BAD: [Tool; 11] = [
            Tool {
                short: "my-tool",
                ..T
            },
            Tool {
                config_file: "",
                ..T
            },
            Tool {
                pin_prefix: "",
                ..T
            },
            Tool {
                engine_crate: "",
                ..T
            },
            Tool {
                engine_bin: Some(""),
                ..T
            },
            Tool {
                cache_namespace: "",
                ..T
            },
            Tool {
                launcher_crate: "",
                ..T
            },
            Tool {
                anchor: Anchor::Marker(""),
                ..T
            },
            // Empty, so both git attempts ask cargo to install from nowhere and
            // it fails naming a url the user never wrote.
            Tool {
                default_url: "",
                ..T
            },
            Tool {
                dir_flag: "",
                ..T
            },
            // The same string for both, so `normalize_args` strips the user's
            // copy as the directory flag and `dispatch` then finds no override
            // to act on. The launcher runs and quietly ignores what was asked.
            Tool {
                engine_flag: Cli::DIR_FLAG,
                ..T
            },
        ];
        for bad in &BAD {
            assert!(
                bad.defect().is_some(),
                "no defect reported for {:?}",
                bad.short
            );
            let err = outcome(bad, &s(&["widget"])).expect_err("a broken launcher ran");
            assert!(
                err.contains("descriptor is not usable"),
                "it failed for some other reason, so nothing checked the descriptor: {err}"
            );
        }
    }

    #[test]
    fn a_sound_descriptor_is_not_refused() {
        // The control. Without it the test above passes for a `defect` that
        // returns `Some` unconditionally, which would refuse every launcher
        // ever built on this.
        assert!(T.defect().is_none(), "the fixture itself is not usable");
        const NAMED_BIN: Tool = Tool {
            engine_bin: Some("engine"),
            ..T
        };
        assert!(NAMED_BIN.defect().is_none());
    }

    #[test]
    fn an_empty_engine_bin_would_have_looked_for_the_directory_itself() {
        // Why that arm is in the list, computed rather than asserted from
        // memory: the join produces the bin directory, and a directory is never
        // the file the cache short-circuits on, so the engine rebuilds forever.
        const EMPTY: Tool = Tool {
            engine_bin: Some(""),
            ..T
        };
        let looked_for = Path::new("/cache/builds/k/bin").join(EMPTY.engine_bin_name());
        assert_eq!(looked_for, Path::new("/cache/builds/k/bin/"));
        assert_eq!(looked_for, Path::new("/cache/builds/k/bin"));
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_direct_invocation_forwards_everything() {
        assert_eq!(
            normalize_args(&T, &s(&["/usr/bin/mock", "lock", "--foo"])),
            s(&["lock", "--foo"])
        );
    }

    #[test]
    fn a_cargo_subcommand_drops_the_repeated_name() {
        // cargo runs `cargo widget x` as `cargo-widget widget x`, so the engine
        // would otherwise be handed a subcommand it does not have.
        assert_eq!(
            normalize_args(
                &T,
                &s(&["/root/.cargo/bin/cargo-widget", "widget", "lock", "--foo"])
            ),
            s(&["lock", "--foo"])
        );
        assert_eq!(
            normalize_args(&T, &s(&["cargo-widget", "widget"])),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_repeated_name_is_dropped_only_when_it_is_the_cargo_shape() {
        // the control, and the reason the rule is written against the program
        // name rather than the first argument. `mock mock` is a user asking the
        // engine for a subcommand called `mock`, and eating it would be wrong.
        assert_eq!(
            normalize_args(&T, &s(&["/usr/bin/mock", "mock"])),
            s(&["mock"])
        );
        // and a cargo-shaped launcher whose first argument is something else
        assert_eq!(
            normalize_args(&T, &s(&["cargo-mock", "lock"])),
            s(&["lock"])
        );
        // and the name has to match the program's own suffix
        assert_eq!(
            normalize_args(&T, &s(&["cargo-mock", "other", "lock"])),
            s(&["other", "lock"])
        );
    }

    #[test]
    fn a_user_supplied_dir_flag_is_stripped() {
        // the launcher owns `--dir`, and two of them would leave the engine
        // reading whichever it parsed last.
        assert_eq!(
            normalize_args(
                &T,
                &s(&["widget", "check", "--dir", "/somewhere", "--scope", "x"])
            ),
            s(&["check", "--scope", "x"])
        );
    }

    #[test]
    fn the_joined_dir_flag_is_stripped_too() {
        // The defect the single `take_flag` exists to prevent, and the one the
        // two copies before it had: the separated spelling was stripped and the
        // joined one was forwarded, so the engine saw a flag the launcher owns
        // and the launcher's own `--dir` fought it.
        assert_eq!(
            normalize_args(
                &T,
                &s(&["widget", "check", "--dir=/somewhere", "--scope", "x"])
            ),
            s(&["check", "--scope", "x"])
        );
        // the same for the flag this tool actually spells, in case a tool picks
        // another and only one of the two spellings is wired to it
        const OTHER: Tool = Tool {
            dir_flag: "--at",
            ..T
        };
        assert_eq!(
            normalize_args(&OTHER, &s(&["widget", "check", "--at=/somewhere"])),
            s(&["check"])
        );
        assert_eq!(
            normalize_args(&OTHER, &s(&["widget", "check", "--dir=/somewhere"])),
            s(&["check", "--dir=/somewhere"]),
            "a flag the tool did not choose was stripped anyway"
        );
    }

    #[test]
    fn a_dir_flag_with_no_value_is_dropped_and_takes_nothing_with_it() {
        // Deliberate, and the counterpart to the engine flag's refusal: the
        // user's directory is discarded whether they named one or not, so
        // naming nothing changes nothing. What must not happen is the next
        // argument being eaten as the value.
        assert_eq!(
            normalize_args(&T, &s(&["widget", "check", "--dir", "--scope", "x"])),
            s(&["check", "--scope", "x"])
        );
    }

    #[test]
    fn the_locate_answer_uses_the_tools_own_key_names() {
        // All three were hardcoded here while `Locate` documented them as "a
        // contract with those callers", so a tool that set them got the
        // conventional spellings anyway and its own shell helpers, parsing the
        // names it had chosen, parsed nothing at all.
        const OWN: Locate = Locate {
            subcommand: Some("locate"),
            root_key: "repo",
            config_key: "manifest",
            workdir_key: "mock_dir",
        };
        let d = tempfile::tempdir().unwrap();
        let wd = d.path().join("mock");
        std::fs::create_dir_all(&wd).unwrap();

        let got = locate_answer(&OWN, d.path(), "/c/x.toml", &wd);
        assert_eq!(
            got,
            format!(
                "repo={}\nmanifest=/c/x.toml\nmock_dir={}\n",
                d.path().display(),
                wd.display()
            )
        );
        // and the control: the conventional names are not what came out, so
        // this cannot be passing against a formatter that ignores its argument
        assert!(!got.contains("root="), "{got}");
        assert!(!got.contains("config="), "{got}");
        assert!(!got.contains("workdir="), "{got}");

        // the default still answers conventionally
        let d2 = locate_answer(&Locate::DEFAULT, d.path(), "/c/x.toml", &wd);
        assert!(d2.starts_with("root="), "{d2}");
        assert!(d2.contains("\nconfig=/c/x.toml\n"), "{d2}");
        assert!(
            d2.contains(&format!("\nworkdir={}\n", wd.display())),
            "{d2}"
        );
    }

    #[test]
    fn a_missing_workdir_answers_with_the_key_and_no_value() {
        // The line stays, so a reader can tell an absent directory from a
        // launcher too old to answer at all.
        let d = tempfile::tempdir().unwrap();
        let absent = d.path().join("nothing-here");
        let got = locate_answer(&Locate::DEFAULT, d.path(), "", &absent);
        assert!(got.ends_with("workdir=\n"), "{got}");
        assert!(got.contains("\nconfig=\n"), "{got}");
    }

    #[test]
    fn an_engine_flag_with_no_path_is_refused_rather_than_running_the_pinned_engine() {
        // Before this, the flag came back as a bare `None`, which is what an
        // absent flag also came back as, so the run fell through to the pinned
        // engine. That is the opposite of what was asked and it is silent: the
        // override path prints `ENGINE OVERRIDE` and this path printed nothing.
        //
        // The refusal is the first thing `dispatch` does, so this reaches it
        // without any discovery, build or exec.
        let e = dispatch(&T, &s(&["--engine"])).unwrap_err();
        assert!(
            e.contains("--engine") && e.contains("none followed it"),
            "the refusal does not name the flag or say what was missing: {e}"
        );

        let e = dispatch(&T, &s(&["--engine", "--verbose"])).unwrap_err();
        assert!(e.contains("--engine"), "{e}");

        // and it names the tool's own flag, not the conventional spelling
        const OTHER: Tool = Tool {
            engine_flag: "--with",
            ..T
        };
        let e = dispatch(&OTHER, &s(&["--with"])).unwrap_err();
        assert!(e.contains("--with"), "{e}");
        assert!(!e.contains("--engine"), "{e}");
    }

    #[test]
    fn the_locate_query_needs_a_subcommand_to_ask_it_with() {
        // The `is_some()` half of the guard. A tool that wants no locate query
        // has `subcommand: None`, and a bare invocation has no first argument,
        // so without it `None == None` and every plain run answers the query
        // instead of running the engine.
        const NO_QUERY: Locate = Locate {
            subcommand: None,
            ..Locate::DEFAULT
        };
        assert!(
            !is_the_locate_query(&NO_QUERY, &s(&[])),
            "a tool with no locate subcommand answered the query on a bare run"
        );
        assert!(!is_the_locate_query(&NO_QUERY, &s(&["locate"])));
        assert!(!is_the_locate_query(&NO_QUERY, &s(&["lock"])));

        // and the control, so the assertions above are not passing because the
        // predicate is a constant `false`
        assert!(is_the_locate_query(&Locate::DEFAULT, &s(&["locate"])));
        assert!(!is_the_locate_query(&Locate::DEFAULT, &s(&[])));
        assert!(!is_the_locate_query(&Locate::DEFAULT, &s(&["lock"])));

        // a tool that spells it differently is asked by its own name and not by
        // the conventional one
        const RENAMED: Locate = Locate {
            subcommand: Some("where"),
            ..Locate::DEFAULT
        };
        assert!(is_the_locate_query(&RENAMED, &s(&["where"])));
        assert!(!is_the_locate_query(&RENAMED, &s(&["locate"])));
    }

    #[test]
    fn the_missing_root_message_names_what_was_looked_for() {
        assert!(no_root(&T).contains(".git"), "{}", no_root(&T));
        assert!(no_root(&T).contains("WIDGET_ROOT"), "{}", no_root(&T));

        const SPAN: Tool = Tool {
            anchor: Anchor::ConfigFile,
            short: "widget",
            ..T
        };
        // a config-anchored tool has no marker, so naming one would send the
        // reader looking for a file that has nothing to do with it.
        assert!(no_root(&SPAN).contains("t.toml"), "{}", no_root(&SPAN));
        assert!(!no_root(&SPAN).contains(".git"), "{}", no_root(&SPAN));
        assert!(no_root(&SPAN).contains("WIDGET_ROOT"), "{}", no_root(&SPAN));
    }

    #[test]
    fn a_legacy_pin_registers_as_legacy_whatever_its_reference_is() {
        let p = Pin {
            url: "u".into(),
            reference: Reference::Rev("abc".into()),
        };
        assert_eq!(
            pin_form_and_value(&p, PinSource::Config),
            (registry::PinForm::Rev, "abc".to_string())
        );
        assert_eq!(
            pin_form_and_value(&p, PinSource::Legacy),
            (registry::PinForm::Legacy, "abc".to_string())
        );
    }

    #[test]
    fn the_missing_pin_message_names_the_tools_own_key() {
        let d = tempfile::tempdir().unwrap();
        let err = resolve_pin(&T, None, d.path(), d.path()).unwrap_err();
        assert!(err.contains("t_version"), "{err}");
        assert!(err.contains("t.toml"), "{err}");
    }

    #[test]
    fn the_refusal_says_whether_the_override_was_set() {
        // an operator who has just exported the variable and got it wrong is
        // the one person this message has to serve, and telling them it is
        // unset sends them looking somewhere else entirely.
        let unset = no_root_with(&T, None);
        assert!(unset.contains("WIDGET_ROOT is unset"), "{unset}");
        assert!(unset.contains(".git"), "{unset}");

        let set = no_root_with(&T, Some("/nope/xyzzy".into()));
        assert!(set.contains("/nope/xyzzy"), "{set}");
        assert!(set.contains("not a directory"), "{set}");
        assert!(
            !set.contains("is unset"),
            "the set case still claims unset: {set}"
        );
    }

    #[test]
    fn the_refusal_names_the_anchor_the_tool_actually_looks_for() {
        // a config-anchored tool never looked for `.git`, so naming it would
        // send the reader to create one.
        const SPAN: Tool = Tool {
            anchor: Anchor::ConfigFile,
            ..T
        };
        let m = no_root_with(&SPAN, None);
        assert!(m.contains(T.config_file), "{m}");
        assert!(!m.contains(".git"), "{m}");
    }
}

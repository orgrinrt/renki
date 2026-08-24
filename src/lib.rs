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
//! for, and that the launcher keeps itself current so nobody has to remember
//! to. A hand-installed binary goes stale silently and nothing reports it.
//!
//! # Using it
//!
//! Declare a [`Tool`] as a `const`, and hand it over:
//!
//! ```no_run
//! use renki::{Anchor, Hooks, Tool};
//!
//! const TOOL: Tool = Tool {
//!     anchor:          Anchor::Marker(".git"),
//!     short:           "widget",
//!     config_file:     "widget.toml",
//!     pin_prefix:      "widget",
//!     engine_crate:    "widget-engine",
//!     cache_namespace: "widget",
//!     default_url:     "ssh://git@github.com/o/widget.git",
//!     launcher_crate:  "widget",
//!     workdir:         None,
//!     hooks:           Hooks::NONE,
//! };
//!
//! fn main() -> std::process::ExitCode {
//!     renki::run(&TOOL)
//! }
//! ```
//!
//! Everything that is one tool's and no other's goes through [`Hooks`] rather
//! than into this crate.

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
pub use crate::tool::{Anchor, Hooks, Tool, Workdir};

/// Where a resolved pin came from, so the registry can tell a repo that has
/// adopted an explicit pin from one still on whatever legacy fallback the tool
/// honours. That difference is the migration-detection signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinSource {
    Config,
    Legacy,
}

/// The launcher entry. A tool's `main` is this and nothing else.
pub fn run(tool: &Tool) -> ExitCode {
    // The launcher runs as a child of git hooks, whose exported repo-location
    // GIT_* variables poison every `git` this process, and the engine it
    // spawns, invokes from a different working directory. Drop them first.
    //
    // SAFETY: this is the first statement of the installed binary's main; no
    // thread has been spawned yet.
    unsafe { sanitize_git_env() };

    let raw: Vec<String> = std::env::args().collect();
    let forwarded = normalize_args(tool, &raw);
    match dispatch(tool, &forwarded) {
        Ok(()) => ExitCode::SUCCESS, // unreachable when the exec succeeds
        Err(e) => {
            eprintln!("{}: {e}", tool.short);
            ExitCode::FAILURE
        }
    }
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
fn locate_query(tool: &Tool) -> Result<(), String> {
    let root = discover::repo_root(tool).ok_or_else(|| no_root(tool))?;
    let located = discover::locate(tool, &root)?;
    let (config, workdir) = match located {
        Some(l) => (l.config_path.display().to_string(), l.workdir),
        // No config, so the conventional directory if it is there at all. The
        // caller distinguishes by the empty `config`.
        None => (String::new(), tool.workdir_default(&root)),
    };
    println!("root={}", root.display());
    println!("config={config}");
    if workdir.is_dir() {
        println!("workdir={}", workdir.display());
    } else {
        println!("workdir=");
    }
    Ok(())
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
/// A user-supplied `--dir <x>` is stripped: the launcher owns `--dir` and
/// always passes the absolute working directory.
fn normalize_args(_tool: &Tool, raw: &[String]) -> Vec<String> {
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
    strip_dir_flag(rest)
}

/// Drop a `--dir <value>` pair anywhere in the args.
fn strip_dir_flag(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a == "--dir" {
            skip = true;
            continue;
        }
        out.push(a);
    }
    out
}

fn dispatch(tool: &Tool, args: &[String]) -> Result<(), String> {
    // `--engine <path>` is the launcher's, so it comes off before anything
    // reads the arguments, the same as `--dir`.
    let (engine_override, args) = engine::take_flag(args.to_vec());
    let args = args.as_slice();

    // Answered before anything else: it is a question about this checkout
    // rather than a run of the engine, so it skips the self-update, the pin
    // resolution and the build.
    if args.first().map(String::as_str) == Some("locate") {
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
    if located.is_some()
        && let Some(verify) = tool.hooks.verify_repo_state
    {
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
        return cache::exec_engine(&bin, &workdir, &extra, args).map(|_never| ());
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
    cache::exec_engine(&bin, &workdir, &extra, args).map(|_never| ())
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
        short: "mock",
        config_file: "t.toml",
        pin_prefix: "t",
        engine_crate: "engine",
        cache_namespace: "t",
        default_url: "u",
        launcher_crate: "cargo-mock",
        workdir: None,
        hooks: Hooks::NONE,
    };

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
        // cargo runs `cargo mock x` as `cargo-mock mock x`, so the engine would
        // otherwise be handed a subcommand it does not have.
        assert_eq!(
            normalize_args(
                &T,
                &s(&["/root/.cargo/bin/cargo-mock", "mock", "lock", "--foo"])
            ),
            s(&["lock", "--foo"])
        );
        assert_eq!(
            normalize_args(&T, &s(&["cargo-mock", "mock"])),
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
                &s(&["mock", "check", "--dir", "/somewhere", "--scope", "x"])
            ),
            s(&["check", "--scope", "x"])
        );
    }

    #[test]
    fn the_missing_root_message_names_what_was_looked_for() {
        assert!(no_root(&T).contains(".git"), "{}", no_root(&T));
        assert!(no_root(&T).contains("MOCK_ROOT"), "{}", no_root(&T));

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
        assert!(unset.contains("MOCK_ROOT is unset"), "{unset}");
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

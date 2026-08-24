//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `--engine <path>`: run a repo against an engine checkout on disk.
//!
//! The launcher normally builds the engine a repo pins and caches it per
//! version, which is right for every real run and useless for the one case that
//! matters while working on the engine itself: seeing what an uncommitted change
//! does to a real repository. The pin resolves to a branch head on the
//! remote, so local edits are invisible until they are pushed, and pushing to
//! find out whether a change took effect is a slow way to ask a fast question.
//!
//! So this path deliberately breaks the caching contract the rest of the
//! launcher keeps. It rebuilds every time, because the whole point is that the
//! source just changed and no key derived from a revision would notice. It stays
//! out of the registry, because a scratch build is not something a repo pins and
//! recording it would make the garbage collector reason about builds nothing
//! points at. And it sweeps its own leftovers, because these are one-offs by
//! nature and a directory per engine checkout would otherwise accumulate
//! silently.
//!
//! Not a way to pin an engine. A repo pins through its config; this is a flag
//! passed by hand, and it is read from configuration nowhere.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hash::Fnv;
use crate::tool::Tool;

/// How long a scratch build survives without being used.
///
/// Short because the flag is for one-off questions. Long enough that iterating
/// on one change through an afternoon does not pay a full rebuild each time, since
/// the target directory is what makes the rebuild cheap and it lives here too.
const SCRATCH_TTL_SECS: u64 = 24 * 60 * 60;

/// Where scratch engine builds live, beside the keyed ones but never mixed with
/// them: the keyed cache is content-addressed and shared, this is neither.
fn scratch_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("engines")
}

/// Pull `--engine <path>` (or `--engine=<path>`) out of the forwarded arguments.
///
/// Returns the path and the arguments with the flag removed, so the engine never
/// sees a flag the launcher owns, the same treatment `--dir` gets.
pub fn take_flag(args: Vec<String>) -> (Option<String>, Vec<String>) {
    let mut path = None;
    let mut rest = Vec::with_capacity(args.len());
    let mut want_value = false;
    for arg in args {
        if want_value {
            want_value = false;
            path = Some(arg);
            continue;
        }
        if arg == "--engine" {
            want_value = true;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--engine=") {
            path = Some(value.to_string());
            continue;
        }
        rest.push(arg);
    }
    (path, rest)
}

/// Check that `raw` looks like an engine checkout, and make it absolute.
///
/// Absolute because the build runs from elsewhere and a relative path would
/// resolve against the wrong directory. Checked because `cargo install --path`
/// on a directory that is not a crate fails with a message about manifests that
/// says nothing about the flag the user actually passed.
///
/// The tool's own `verify_engine_dir` hook runs after the manifest check, for
/// whatever else that engine's checkout has to carry.
pub fn locate(tool: &Tool, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    let abs = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|e| format!("could not read the working directory: {e}"))?
            .join(path)
    };
    let abs = abs
        .canonicalize()
        .map_err(|e| format!("--engine {}: {e}", abs.display()))?;

    if !abs.join("Cargo.toml").is_file() {
        return Err(format!(
            "--engine {} has no Cargo.toml, so it is not a {} checkout",
            abs.display(),
            tool.engine_crate
        ));
    }
    if let Some(verify) = tool.hooks.verify_engine_dir {
        verify(&abs)?;
    }
    Ok(abs)
}

/// Build the engine from `source`, always, and return the binary.
///
/// Always, rather than when something looks stale: the reason to pass this flag
/// is that the source changed a moment ago, and any staleness check cheap enough
/// to run here would be a worse version of what cargo already does. cargo skips
/// what genuinely did not change, and the target directory is kept between runs
/// so it can.
pub fn build(tool: &Tool, cache_root: &Path, source: &Path) -> Result<PathBuf, String> {
    let scratch = scratch_dir(cache_root);
    sweep(&scratch);

    let mut h = Fnv::new();
    h.write_field(&source.to_string_lossy());
    let root = scratch.join(h.hex());
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("could not create {}: {e}", root.display()))?;

    eprintln!(
        "{}: building the engine from {} (scratch, not cached by revision)",
        tool.short,
        source.display()
    );
    let status = Command::new("cargo")
        .arg("install")
        .arg("--path")
        .arg(source)
        .arg("--root")
        .arg(&root)
        .arg("--target-dir")
        .arg(root.join("target"))
        .arg("--force")
        .status()
        .map_err(|e| format!("could not run cargo install: {e}"))?;
    if !status.success() {
        return Err(format!(
            "the engine at {} did not build; nothing was installed",
            source.display()
        ));
    }

    let bin = root.join("bin").join(tool.engine_crate);
    if !bin.is_file() {
        return Err(format!(
            "cargo install reported success but produced no binary under {}",
            root.display()
        ));
    }
    Ok(bin)
}

/// Delete scratch builds nothing has used for [`SCRATCH_TTL_SECS`].
///
/// Best-effort in every direction: a scratch build is disposable, so a sweep
/// that cannot read a directory, cannot stat an entry, or cannot remove one has
/// nothing worth reporting and must never fail a run.
fn sweep(scratch: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age.as_secs() > SCRATCH_TTL_SECS);
        if stale {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{Anchor, Hooks};

    /// A tool that demands nothing of a checkout beyond a manifest.
    const PLAIN: Tool = Tool {
        anchor:          Anchor::Marker(".git"),
        short:           "t",
        config_file:     "t.toml",
        pin_prefix:      "t",
        engine_crate:    "engine",
        cache_namespace: "t",
        default_url:     "u",
        launcher_crate:  "t-launcher",
        workdir:         None,
        hooks:           Hooks::NONE,
    };

    /// A tool that demands more, the way mockspace demands a lint-rules crate
    /// its custom-lint cdylibs can link against.
    const FUSSY: Tool = Tool {
        hooks: Hooks {
            verify_engine_dir: Some(|abs| {
                abs.join("extra")
                    .is_dir()
                    .then_some(())
                    .ok_or_else(|| format!("--engine {} has no extra/", abs.display()))
            }),
            ..Hooks::NONE
        },
        ..PLAIN
    };

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_flag_is_taken_out_of_the_forwarded_arguments() {
        // The engine must never see it. It is the launcher's, like `--dir`, and
        // an engine given an argument it does not know reports a usage error
        // against a flag the user passed correctly.
        let (path, rest) = take_flag(strings(&["lock", "--engine", "/tmp/e", "--verbose"]));
        assert_eq!(path.as_deref(), Some("/tmp/e"));
        assert_eq!(rest, strings(&["lock", "--verbose"]));
    }

    #[test]
    fn the_joined_form_is_the_same_flag() {
        let (path, rest) = take_flag(strings(&["--engine=/tmp/e", "close"]));
        assert_eq!(path.as_deref(), Some("/tmp/e"));
        assert_eq!(rest, strings(&["close"]));
    }

    #[test]
    fn a_run_without_the_flag_is_untouched() {
        // The control. Every assertion above would hold for a parser that
        // dropped arguments it did not recognise.
        let (path, rest) = take_flag(strings(&["lock", "--verbose"]));
        assert!(path.is_none());
        assert_eq!(rest, strings(&["lock", "--verbose"]));
    }

    #[test]
    fn a_trailing_flag_with_no_value_takes_nothing() {
        let (path, rest) = take_flag(strings(&["lock", "--engine"]));
        assert!(path.is_none(), "a value was invented for a flag that had none");
        assert_eq!(rest, strings(&["lock"]));
    }

    #[test]
    fn a_directory_that_is_not_a_crate_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let err = locate(&PLAIN, &dir.path().to_string_lossy()).unwrap_err();
        assert!(err.contains("no Cargo.toml"), "{err}");
        // and the message names the engine the tool actually builds
        assert!(err.contains("engine"), "{err}");
    }

    #[test]
    fn the_tools_own_demand_is_checked_after_the_manifest() {
        // and only the tool that makes it: the same tree passes for one and
        // fails for the other, which is what makes the hook a hook rather than
        // a rule the crate hardcodes for everyone.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), b"[package]\n").unwrap();
        let raw = dir.path().to_string_lossy().to_string();

        assert!(locate(&PLAIN, &raw).is_ok());
        let err = locate(&FUSSY, &raw).unwrap_err();
        assert!(err.contains("no extra/"), "{err}");

        std::fs::create_dir(dir.path().join("extra")).unwrap();
        assert!(locate(&FUSSY, &raw).is_ok());
    }

    #[test]
    fn a_real_checkout_resolves_to_an_absolute_path() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), b"[package]\n").unwrap();
        let got = locate(&PLAIN, &dir.path().to_string_lossy()).unwrap();
        assert!(got.is_absolute());
    }

    #[test]
    fn the_sweep_keeps_fresh_builds_and_survives_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let scratch = dir.path().join("engines");
        let fresh = scratch.join("aaaa");
        std::fs::create_dir_all(&fresh).unwrap();
        sweep(&scratch);
        assert!(fresh.is_dir(), "a build made a moment ago was swept");

        // A sweep of somewhere that does not exist is a normal first run.
        sweep(&dir.path().join("nothing-here"));
    }
}

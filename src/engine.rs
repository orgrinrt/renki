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

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::hash::Fnv;
use crate::tool::Tool;

/// How long a scratch build survives without being used.
///
/// Short because the flag is for one-off questions. Long enough that iterating
/// on one change through an afternoon does not pay a full rebuild each time, since
/// the target directory is what makes the rebuild cheap and it lives here too.
const SCRATCH_TTL_SECS: u64 = 24 * 60 * 60;

/// The file whose timestamp says when a scratch build was last used.
///
/// A directory's own modification time changes when an entry is added to it or
/// removed from it, and not when something inside an entry is written. cargo
/// writes into `bin` and `target`, both of which already exist after the first
/// run, so the root's timestamp is when it was created and never moves again.
/// Reading it as last use meant a checkout somebody worked on daily was swept
/// by its own process every day and rebuilt cold, which is the opposite of what
/// keeping the target directory is for.
const SCRATCH_MARKER: &str = ".last-used";

/// Where scratch engine builds live, beside the keyed ones but never mixed with
/// them: the keyed cache is content-addressed and shared, this is neither.
fn scratch_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("engines")
}

/// Pull `--engine <path>` (or `--engine=<path>`) out of the forwarded arguments.
///
/// Take a `<flag> <value>` or `<flag>=<value>` pair out of the arguments.
///
/// Returns the value and the arguments with both halves removed, so the engine
/// never sees a flag the launcher owns. One function rather than two, because
/// the launcher does this to [`Tool::engine_flag`] keeping the value and to
/// [`Tool::dir_flag`] discarding it, and two copies is how one of them came to
/// handle the joined spelling while the other silently forwarded it.
///
/// A value beginning with `-` is not taken. `--engine --verbose` is a flag with
/// its value missing, and reading `--verbose` as a path both loses the flag and
/// produces a diagnostic about a file nobody named.
///
/// The joined spelling with nothing after the `=` is also missing rather than
/// empty. An empty path resolves to the working directory, so `--engine=` and
/// an unset variable expanding to it would have built whatever repository the
/// command was run in.
///
/// Scanning stops at a bare `--`. Everything after one belongs to the engine
/// verbatim, by the convention every command line shares, so a launcher that
/// kept reading would take an argument the user had already said was not its.
///
/// A flag passed more than once takes the last one, and every occurrence is
/// still removed, so nothing the launcher owns reaches the engine. Last wins is
/// what a shell user expects from a repeated option, and it is the spelling
/// that lets a wrapper script append an override after whatever it was handed.
pub(crate) fn take_flag(args: Vec<OsString>, flag: &str) -> (Flag, Vec<OsString>) {
    let joined = format!("{flag}=");
    let mut found = Flag::Absent;
    let mut rest = Vec::with_capacity(args.len());
    let mut want_value = false;
    let mut users_from_here = false;
    for arg in args {
        if users_from_here {
            rest.push(arg);
            continue;
        }
        // An argument that is not valid UTF-8 cannot equal any of the ASCII
        // flags below, so `None` here means "the user's", which is the right
        // answer and the one that keeps the bytes intact all the way to `exec`.
        let text = arg.to_str();
        if text == Some("--") {
            users_from_here = true;
            // A pending value stays missing: `--engine --` is the flag with
            // nothing after it, not the flag taking a separator.
            want_value = false;
            rest.push(arg);
            continue;
        }
        if want_value {
            want_value = false;
            if !text.is_some_and(|t| t.starts_with('-')) {
                found = Flag::Value(arg);
                continue;
            }
            // The flag was passed and the next argument is another flag, so it
            // stays `Missing` and this argument is the user's.
        }
        if text == Some(flag) {
            want_value = true;
            found = Flag::Missing;
            continue;
        }
        if let Some(value) = text.and_then(|t| t.strip_prefix(&joined)) {
            found = if value.is_empty() {
                Flag::Missing
            } else {
                Flag::Value(OsString::from(value))
            };
            continue;
        }
        rest.push(arg);
    }
    (found, rest)
}

/// What [`take_flag`] found, which is three things rather than two.
///
/// A flag nobody passed and a flag passed with nothing after it are different
/// facts, and collapsing them to `None` is how `--engine` with no value came to
/// run the pinned engine silently: the caller saw an absent override and did
/// what it does when the user asked for nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Flag {
    /// Not passed.
    Absent,
    /// Passed, with the value.
    ///
    /// An `OsString` because a value is a path as often as not, and a path is
    /// bytes. Lossy conversion here would hand the engine a filename that does
    /// not exist.
    Value(OsString),
    /// Passed, with no value after it. A usage error for a flag whose value is
    /// read, and harmless for one whose value is discarded.
    Missing,
}

impl Flag {
    /// The value, for a caller that treats a missing one the same as an absent
    /// flag. Only correct where the flag's value is discarded anyway.
    pub(crate) fn value(self) -> Option<OsString> {
        match self {
            Self::Value(v) => Some(v),
            Self::Absent | Self::Missing => None,
        }
    }
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
pub(crate) fn locate(tool: &Tool, raw: &OsStr) -> Result<PathBuf, String> {
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

/// The scratch directory name for one `--engine` source path.
///
/// The path's bytes, not a rendering of them. On unix a path is arbitrary bytes
/// and `to_string_lossy` maps every invalid sequence onto one replacement
/// character, so two unrelated sources collided into one directory and built
/// over each other. Named rather than written inline in [`build`], because the
/// test that pins the collision has to hash the same way the build does and a
/// second copy of two lines is a test that guards a copy.
pub(crate) fn scratch_key(source: &Path) -> String {
    let mut h = Fnv::new();
    h.write_bytes(source.as_os_str().as_encoded_bytes());
    h.hex()
}

/// Build the engine from `source`, always, and return the binary.
///
/// Always, rather than when something looks stale: the reason to pass this flag
/// is that the source changed a moment ago, and any staleness check cheap enough
/// to run here would be a worse version of what cargo already does. cargo skips
/// what genuinely did not change, and the target directory is kept between runs
/// so it can.
pub(crate) fn build(tool: &Tool, cache_root: &Path, source: &Path) -> Result<PathBuf, String> {
    let scratch = scratch_dir(cache_root);
    sweep(cache_root);

    let root = scratch.join(scratch_key(source));
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("could not create {}: {e}", root.display()))?;
    touch(&root);

    eprintln!(
        "{}: building the engine from {} (scratch, not cached by revision)",
        tool.short,
        source.display()
    );
    // The same backend the pinned build uses, with a one-attempt plan. The
    // target directory is named so it lands inside the scratch root and is kept
    // between runs, which is what makes an unchanged rebuild cheap.
    crate::extension::materialise_once::<crate::extension::Cargo>(
        &crate::extension::CargoPlan {
            attempts:   vec![vec![
                "--path".into(),
                source.display().to_string(),
                "--target-dir".into(),
                root.join("target").display().to_string(),
            ]],
            bin:        tool.engine_bin_name().into(),
            crate_name: tool.engine_crate.into(),
        },
        &root,
    )
    .map_err(|e| format!("the engine at {} did not build.\n{e}", source.display()))?;

    Ok(root.join("bin").join(tool.engine_bin_name()))
}

/// Record that this scratch build is in use, for the sweep to read later.
///
/// Best-effort. A marker that cannot be written costs a rebuild a day later and
/// nothing else, so it is not worth failing a run over.
fn touch(root: &Path) {
    let _ = std::fs::write(root.join(SCRATCH_MARKER), b"");
}

/// Delete scratch builds nothing has used for [`SCRATCH_TTL_SECS`].
///
/// Called from the ordinary run as well as from [`build`], because a user who
/// passes the engine flag once and never again would otherwise keep that
/// checkout and its target directory forever: the only caller used to be the
/// path that creates them, which is a sweep that runs exactly when there is
/// nothing yet to sweep.
///
/// Best-effort in every direction: a scratch build is disposable, so a sweep
/// that cannot read a directory, cannot stat an entry, or cannot remove one has
/// nothing worth reporting and must never fail a run.
pub(crate) fn sweep(cache_root: &Path) {
    let Ok(entries) = std::fs::read_dir(scratch_dir(cache_root)) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // The marker where there is one, and the directory itself where there
        // is not, which is a root from before the marker existed.
        let stamp = std::fs::metadata(path.join(SCRATCH_MARKER)).or_else(|_| entry.metadata());
        let stale = stamp
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
#[path = "engine_tests.rs"]
mod tests;

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The per-version build cache.
//!
//! Each distinct compilation input (the engine's url and rev) is built once into
//! `<cache>/builds/<key>/bin/<engine>` and shared by every repo pinned to it.
//! This crate holds no lock of its own. What it relies on is narrower than
//! that sounds and is worth stating exactly: `cargo install --root` takes
//! cargo's own lock on the install root and moves the finished binary into
//! place, so two launchers resolving the *same* key either serialise on that
//! lock or find the binary already there. Keys differ per url, revision and
//! toolchain, and two launchers on different keys share no install root, so
//! they do not contend at all.
//!
//! What has not been established is that no interleaving anywhere fails, and
//! nothing here tests two launchers running at once. The one interleaving that
//! is known and handled is a collection pass evicting a build between the
//! build and the exec, which the run re-materialises a bounded number of
//! times.
//!
//! The cache lives under `~/.cache`, honouring `XDG_CACHE_HOME`. Never under
//! `~/.config`, which is per-developer configuration rather than machine
//! content that can be deleted and rebuilt.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hash::Fnv;
use crate::pin::Resolved;
use crate::tool::Tool;

/// `<SHORT>_CACHE`, else `$XDG_CACHE_HOME/<namespace>`, else
/// `~/.cache/<namespace>`.
pub(crate) fn cache_root(tool: &Tool) -> Result<PathBuf, String> {
    cache_root_from(
        tool,
        std::env::var_os(tool.cache_env()),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Pure core of [`cache_root`]: env values passed in so it is testable without
/// mutating process env (cargo runs tests in parallel threads, where `set_var`
/// is a data race).
fn cache_root_from(
    tool: &Tool,
    own: Option<std::ffi::OsString>,
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
    // The tool's own variable is the whole path rather than a parent to join
    // the namespace onto, because the request it answers is "put this tool's
    // builds here" and a namespace appended is the launcher arguing with it.
    // The root discovery override reads the same way, which is the asymmetry
    // this closes: a user could say which repository to work on and not where
    // several hundred megabytes of engines were going to land. Setting
    // `XDG_CACHE_HOME` works and moves every other program's cache too, which
    // is a different request.
    if let Some(o) = own
        && !o.is_empty()
    {
        return Ok(PathBuf::from(o));
    }
    if let Some(x) = xdg
        && !x.is_empty()
    {
        return Ok(PathBuf::from(x).join(tool.cache_namespace));
    }
    let home = home
        .filter(|h| !h.is_empty())
        .ok_or_else(|| "neither XDG_CACHE_HOME nor HOME is set".to_string())?;
    Ok(PathBuf::from(home)
        .join(".cache")
        .join(tool.cache_namespace))
}

fn builds_dir(root: &Path) -> PathBuf {
    root.join("builds")
}

/// The toolchain identity to fold into the cache key: `rustc -vV`, which
/// carries the version, the commit hash, the host triple and the LLVM version.
///
/// rustc is part of the real compilation input, so a toolchain change must
/// re-key the cached engine. A frozen engine binary paired with anything
/// compiled later by a different rustc is at best a rebuild nobody asked for,
/// and at worst unsound where the two share a type across a dynamic library
/// boundary, since neither the layout nor the vtable of a trait object is
/// stable between compilers.
///
/// The empty string when rustc cannot be run at all. The key then omits it, and
/// the build that follows would fail for the same reason anyway.
pub(crate) fn rustc_fingerprint() -> String {
    Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// The cache key: a hash of the full compilation input. The engine's url, the
/// resolved rev, and the toolchain identity (see [`rustc_fingerprint`]). A
/// change in any of the three re-keys and forces a coherent rebuild.
///
/// Nothing else goes in. A tool whose engine build depends on inputs of its own
/// would need them here, and when one exists it arrives as a hook and as a
/// fourth field. It is not anticipated: an unreachable parameter reads as a
/// feature the crate has, and this one carried two tests no caller could
/// exercise.
pub(crate) fn compute_key(url: &str, key_rev: &str, toolchain: &str) -> String {
    let mut h = Fnv::new();
    h.write_field(url);
    h.write_field(key_rev);
    h.write_field(toolchain);
    h.hex()
}

/// The cached engine binary for `key`, building it once if missing.
///
/// The resolved pin carries one or more install attempts (a `version` pin
/// tries crates.io first, then the git tag); the first that succeeds wins.
/// `cargo install --root` locks the install root and moves the finished binary
/// into place, so a second launcher on the same key either blocks on that lock
/// or finds the binary already there.
pub(crate) fn ensure_built(
    tool: &Tool,
    cache_root: &Path,
    key: &str,
    resolved: &Resolved,
) -> Result<PathBuf, String> {
    let root = builds_dir(cache_root).join(key);
    let bin = root.join("bin").join(tool.engine_bin_name());
    if bin.is_file() {
        return Ok(bin);
    }
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("could not create cache dir {}: {e}", root.display()))?;

    eprintln!(
        "{}: building the engine for this pin (once per version) ...",
        tool.short
    );
    let mut failures = Vec::new();
    for attempt in &resolved.attempts {
        let status = Command::new("cargo")
            .arg("install")
            .args(attempt)
            .arg("--root")
            .arg(&root)
            .arg("--force")
            .status()
            .map_err(|e| format!("could not run cargo install: {e}"))?;
        if status.success() {
            if bin.is_file() {
                return Ok(bin);
            }
            failures.push(format!(
                "{attempt:?} reported success but produced no binary"
            ));
        } else {
            failures.push(format!("{attempt:?} failed"));
        }
    }
    Err(build_failure(tool, &failures))
}

/// What the operator reads when no attempt produced a binary.
///
/// The toolchain is named because it is a real cause the message otherwise
/// hides. `cargo install` resolves the engine's dependencies fresh rather than
/// from its committed lockfile, so a transitive crate can float to a version
/// whose minimum rustc is above the one in effect, and the build then fails on
/// a crate nobody in this repo named. The toolchain in effect is the launcher's
/// own, since `cargo install` inherits this process's working directory: the
/// consuming repo's `rust-toolchain.toml` governs, not the engine's.
///
/// Measured rather than reasoned: installing one engine over `--git` failed
/// under rustc 1.94 on a transitive crate requiring 1.96, and succeeded
/// unchanged from a directory pinning a newer toolchain.
fn build_failure(tool: &Tool, failures: &[String]) -> String {
    format!(
        "could not build the {} engine for this pin; nothing was cached.\n  \
         tried, in order:\n    - {}\n  \
         the pin may be wrong, the release may not exist yet, the build may have broken, \
         or the toolchain in effect here ({}) may be older than one of the engine's \
         dependencies requires.",
        tool.engine_crate,
        failures.join("\n    - "),
        rustc_version_line()
    )
}

/// The `rustc --version` line, for a diagnostic. Falls back to a phrase that
/// reads correctly in the sentence above rather than to an empty string, which
/// would leave the operator with an empty pair of brackets.
fn rustc_version_line() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "no rustc on PATH".to_string())
}

/// The arguments the engine is run with: the tool's own directory flag and the
/// absolute working directory so cwd is irrelevant, then whatever the tool's
/// hooks add, then the caller's forwarded arguments.
///
/// The whole command line, argv[0] first, because an `exec` cannot be observed
/// from a test and this is the half worth observing.
///
/// argv[0] is the launcher's own short name rather than the engine binary's
/// path, so an engine that prints its own usage prints the name the user
/// typed. `Command::new` alone puts the path there, and the engine then tells
/// the user about a binary they have never heard of and cannot invoke.
pub(crate) fn engine_command_line(
    tool: &Tool,
    workdir: &Path,
    extra: &[String],
    args: &[String],
) -> Vec<std::ffi::OsString> {
    let mut argv = Vec::with_capacity(3 + extra.len() + args.len());
    argv.push(tool.short.into());
    argv.push(tool.dir_flag.into());
    argv.push(workdir.as_os_str().to_os_string());
    argv.extend(extra.iter().map(Into::into));
    argv.extend(args.iter().map(Into::into));
    argv
}

/// Replace this process with the engine. On unix `exec` never returns on
/// success; it returns only if the exec itself fails.
pub(crate) fn exec_engine(
    tool: &Tool,
    bin: &Path,
    workdir: &Path,
    extra: &[String],
    args: &[String],
) -> Result<std::convert::Infallible, String> {
    use std::os::unix::process::CommandExt;
    let argv = engine_command_line(tool, workdir, extra, args);
    let err = Command::new(bin).arg0(&argv[0]).args(&argv[1..]).exec();
    Err(format!("failed to exec {}: {err}", bin.display()))
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

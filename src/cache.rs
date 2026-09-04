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
//! Where the cache is, and the state beside it, is `renki-dirs`'s table: the
//! platform's own directory for each, with the tool's `<SHORT>_CACHE` and
//! `<SHORT>_STATE` naming a whole path over it and the XDG variable for the
//! kind over the default. The cache holds what a cleanup may take at any time
//! and the launcher rebuilds. The state holds the registry and the self-update
//! marker, which the launcher would behave differently without, so they do
//! not sit where a cleanup takes them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use notko::{Maybe, Outcome};
use renki_dirs::{EnvName, Host, Kind, Namespace, Root, Short, Sources};

use crate::hash::Fnv;
use crate::pin::Resolved;
use crate::tool::Tool;

/// The cache root on this machine: `<SHORT>_CACHE`, else the XDG cache
/// directory with the namespace under it, else the platform's default.
pub(crate) fn cache_root(tool: &Tool) -> Result<PathBuf, String> {
    root_of::<renki_dirs::Cache>(tool)
}

/// The state root on this machine, by the same precedence over `<SHORT>_STATE`
/// and `XDG_STATE_HOME`.
pub(crate) fn state_root(tool: &Tool) -> Result<PathBuf, String> {
    root_of::<renki_dirs::State>(tool)
}

/// One kind's root, read off the environment. The table and the precedence are
/// `renki-dirs`'s; what is this crate's is the reading and the one refusal the
/// table cannot make, a value that is not text.
fn root_of<K: Kind>(tool: &Tool) -> Result<PathBuf, String> {
    let own = std::env::var_os(EnvName::<K>::of(Short(tool.short)).to_string());
    let xdg = std::env::var_os(K::XDG_VAR);
    let home = std::env::var_os("HOME");
    root_from::<K>(tool, own.as_deref(), xdg.as_deref(), home.as_deref())
}

/// Pure core of [`root_of`]: the values passed in so it is testable without
/// mutating process env (cargo runs tests in parallel threads, where `set_var`
/// is a data race).
///
/// A value that is not text is refused by name rather than replaced. The table
/// prints a path as text, and a directory whose bytes do not decode would come
/// back as a different directory, one that does not exist, reported under a
/// name the operator cannot find on disk.
fn root_from<K: Kind>(
    tool: &Tool,
    own: Option<&std::ffi::OsStr>,
    xdg: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Result<PathBuf, String> {
    fn text<'a>(name: &str, v: Option<&'a std::ffi::OsStr>) -> Result<Maybe<&'a str>, String> {
        match v {
            None => Ok(Maybe::Isnt),
            Some(s) => {
                s.to_str()
                    .map(Maybe::Is)
                    .ok_or_else(|| format!("{name} is set to something that is not text"))
            },
        }
    }
    let sources = Sources {
        own:  text(&EnvName::<K>::of(Short(tool.short)).to_string(), own)?,
        xdg:  text(K::XDG_VAR, xdg)?,
        home: text("HOME", home)?,
    };
    let ns = match Namespace::new(tool.cache_namespace) {
        Outcome::Ok(ns) => ns,
        Outcome::Err(e) => {
            return Err(format!(
                "the tool's cache namespace {:?} is not a directory name: {e:?}",
                tool.cache_namespace
            ));
        },
    };
    match Root::<K, Host>::resolve(ns, sources) {
        Outcome::Ok(root) => Ok(PathBuf::from(root.to_string())),
        Outcome::Err(e) => Err(e.to_string()),
    }
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
    crate::extension::materialise_once::<crate::extension::Cargo>(
        &crate::extension::CargoPlan {
            attempts:   resolved.attempts.clone(),
            bin:        tool.engine_bin_name().into(),
            crate_name: tool.engine_crate.into(),
        },
        &root,
    )?;
    Ok(bin)
}

/// What the operator reads when no attempt produced a binary.
///
/// The toolchain is named because it is a real cause the message otherwise
/// hides. The install is locked to the engine's committed lockfile, and a
/// dependency that lockfile names can still want a rustc above the one in
/// effect, so the build fails on a crate nobody in this repo named. The
/// toolchain in effect is the launcher's own, since `cargo install` inherits
/// this process's working directory: the consuming repo's
/// `rust-toolchain.toml` governs, not the engine's.
///
/// The lockfile is named for the same reason. An engine that commits none
/// gets a warning from cargo and a fresh resolution, which takes whatever the
/// registry published since, and the operator cannot tell that from the
/// output of a build that merely failed.
///
/// Measured rather than reasoned: installing one engine over `--git` failed
/// under rustc 1.94 on a transitive crate requiring 1.96, and succeeded
/// unchanged from a directory pinning a newer toolchain.
pub(crate) fn build_failure(engine_crate: &str, failures: &[String]) -> String {
    format!(
        "could not build the {} engine for this pin; nothing was cached.\n  \
         tried, in order:\n    - {}\n  \
         the pin may be wrong, the release may not exist yet, the build may have broken, \
         the engine may commit no lockfile and so have resolved fresh, \
         or the toolchain in effect here ({}) may be older than one of the engine's \
         dependencies requires.",
        engine_crate,
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
    args: &[OsString],
) -> Vec<OsString> {
    let mut argv = Vec::with_capacity(3 + extra.len() + args.len());
    argv.push(tool.short.into());
    argv.push(tool.dir_flag.into());
    argv.push(workdir.as_os_str().to_os_string());
    argv.extend(extra.iter().map(Into::into));
    argv.extend(args.iter().cloned());
    argv
}

/// Replace this process with the engine. On unix `exec` never returns on
/// success; it returns only if the exec itself fails.
pub(crate) fn exec_engine(
    tool: &Tool,
    bin: &Path,
    workdir: &Path,
    extra: &[String],
    args: &[OsString],
) -> Result<std::convert::Infallible, String> {
    use std::os::unix::process::CommandExt;
    let argv = engine_command_line(tool, workdir, extra, args);
    let err = Command::new(bin).arg0(&argv[0]).args(&argv[1 ..]).exec();
    Err(format!("failed to exec {}: {err}", bin.display()))
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The per-version build cache.
//!
//! Each distinct compilation input (the engine's url and rev) is built once into
//! `<cache>/builds/<key>/bin/<engine>` and shared by every repo pinned to it.
//! `cargo install` takes its own lock on the install root and installs the
//! binary atomically, so a racing second launcher either blocks on that lock or
//! finds the finished binary; the launcher needs no lock of its own.
//!
//! The cache lives under `~/.cache`, honouring `XDG_CACHE_HOME`. Never under
//! `~/.config`, which is per-developer configuration rather than machine
//! content that can be deleted and rebuilt.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::hash::Fnv;
use crate::pin::Resolved;
use crate::tool::Tool;

/// `$XDG_CACHE_HOME/<namespace>` or `~/.cache/<namespace>`.
pub(crate) fn cache_root(tool: &Tool) -> Result<PathBuf, String> {
    cache_root_from(
        tool,
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Pure core of [`cache_root`]: env values passed in so it is testable without
/// mutating process env (cargo runs tests in parallel threads, where `set_var`
/// is a data race).
fn cache_root_from(
    tool: &Tool,
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf, String> {
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
/// `cargo install --root` locks the install root and installs the binary
/// atomically, so a concurrent launcher either blocks on that lock or finds
/// the finished binary.
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
mod tests {
    use super::*;
    use crate::pin::{Pin, Reference};
    use crate::tool::{Anchor, Cli, Hooks, Locate};

    const T: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "t",
        config_file: "t.toml",
        pin_prefix: "t",
        engine_crate: "engine",
        engine_bin: None,
        cache_namespace: "tns",
        default_url: "u",
        launcher_crate: "t-launcher",
        workdir: None,
        dir_flag: Cli::DIR_FLAG,
        engine_flag: Cli::ENGINE_FLAG,
        locate: Locate::DEFAULT,
        hooks: Hooks::NONE,
    };

    #[test]
    fn the_build_failure_names_every_cause_including_the_toolchain() {
        let msg = build_failure(&T, &["a failed".to_string(), "b failed".to_string()]);
        // the engine it could not build, and each attempt in order
        assert!(msg.contains("engine"), "{msg}");
        assert!(
            msg.contains("a failed") && msg.contains("b failed"),
            "{msg}"
        );
        // the four causes, the last of which is the one an operator cannot see
        // from the cargo output: `cargo install` resolves fresh rather than
        // from the engine's committed lockfile, so a transitive crate floats to
        // a version whose minimum rustc is above the one in effect, and the
        // failure then names a crate nobody in the repo chose.
        for cause in [
            "pin may be wrong",
            "release may not exist",
            "build may have broken",
        ] {
            assert!(msg.contains(cause), "missing `{cause}`: {msg}");
        }
        assert!(msg.contains("toolchain in effect"), "{msg}");
        // and it says WHICH. "check your toolchain" sends someone to read a
        // file when the answer is what this process actually resolved.
        assert!(
            msg.contains("rustc ") || msg.contains("no rustc on PATH"),
            "the toolchain is named as a cause but never identified: {msg}"
        );
        assert!(
            !msg.contains("()"),
            "an empty version left empty brackets: {msg}"
        );
    }

    #[test]
    fn the_version_line_is_never_empty_and_never_multiline() {
        // It lands inside a parenthesised clause, so an empty or wrapped value
        // breaks the sentence around it rather than merely being unhelpful.
        let v = rustc_version_line();
        assert!(!v.is_empty());
        assert_eq!(v.lines().count(), 1, "{v:?}");
        assert_eq!(v, v.trim(), "{v:?}");
    }
    #[test]
    fn the_engine_is_handed_the_tools_own_directory_flag() {
        // The control that makes this mean anything: a tool whose flag is NOT
        // the conventional one. With `--dir` hardcoded at the exec site, the
        // assertion below reads `--dir` for a tool that never named it, and the
        // engine is handed a flag it does not take while never seeing the one
        // it declared. A fixture using `Cli::DIR_FLAG` cannot tell the two
        // apart, which is why the existing strip-side test could pass
        // throughout.
        const AT: Tool = Tool {
            dir_flag: "--at",
            ..T
        };
        let argv = engine_command_line(&AT, Path::new("/w"), &[], &[]);
        assert_eq!(argv, ["t", "--at", "/w"]);
        assert!(
            !argv.iter().any(|a| a == "--dir"),
            "the conventional flag reached a tool that named its own: {argv:?}"
        );

        // and the conventional spelling still arrives for a tool that chose it
        assert_eq!(
            engine_command_line(&T, Path::new("/w"), &[], &[]),
            ["t", "--dir", "/w"]
        );
    }

    #[test]
    fn the_engine_is_told_it_is_the_launcher_and_not_the_binary_on_disk() {
        // An engine that prints its own usage prints argv[0], so if the exec
        // leaves that as the cached binary's path the user is told to run a
        // `widget-engine` they have never installed and cannot find. The name
        // handed over is the launcher's, which is the one they typed.
        let argv = engine_command_line(&T, Path::new("/w"), &[], &[]);
        assert_eq!(argv[0], T.short);
        assert_ne!(
            argv[0], T.engine_crate,
            "the engine's own package name reached argv[0]"
        );

        // and it tracks the tool rather than being a constant that happens to
        // match this fixture
        const OTHER: Tool = Tool {
            short: "other",
            ..T
        };
        assert_eq!(
            engine_command_line(&OTHER, Path::new("/w"), &[], &[])[0],
            "other"
        );
    }

    #[test]
    fn the_directory_leads_and_the_hooks_arguments_precede_the_users() {
        // The order is a contract: the engine reads its directory before
        // anything, a hook's argument must not be shadowed by a user's copy of
        // the same flag, and a `--` the user wrote has to stay last or every
        // argument after it changes meaning.
        let extra = vec!["--dep".to_string(), "{ path = \"x\" }".to_string()];
        let args = vec!["lock".to_string(), "--".to_string(), "-v".to_string()];
        assert_eq!(
            engine_command_line(&T, Path::new("/w"), &extra, &args),
            [
                "t",
                "--dir",
                "/w",
                "--dep",
                "{ path = \"x\" }",
                "lock",
                "--",
                "-v"
            ]
        );
    }

    #[test]
    fn a_working_directory_that_is_not_utf8_survives_the_handover() {
        // A path is bytes, not text. Building the argument list as `String`
        // would replace whatever does not decode, and the engine would then be
        // pointed at a directory that does not exist, reporting it under a name
        // the operator cannot find on disk.
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let raw = OsStr::from_bytes(b"/w/\xff\xfe");
        let argv = engine_command_line(&T, Path::new(raw), &[], &[]);
        assert_eq!(argv[2], raw, "the path was lossily re-encoded");
    }

    #[test]
    fn the_cache_root_prefers_xdg_and_falls_back_to_home() {
        let r = cache_root_from(&T, Some("/x/cache".into()), Some("/home/u".into())).unwrap();
        assert_eq!(r, Path::new("/x/cache/tns"));

        let r = cache_root_from(&T, None, Some("/home/u".into())).unwrap();
        assert_eq!(r, Path::new("/home/u/.cache/tns"));
        // an empty XDG is not a setting
        let r = cache_root_from(&T, Some("".into()), Some("/home/u".into())).unwrap();
        assert_eq!(r, Path::new("/home/u/.cache/tns"));
    }

    #[test]
    fn two_tools_never_share_a_cache_root() {
        // the control on the namespace being a parameter at all: without it
        // every tool builds into the same directory and one evicts the other's
        // engines on its own collection pass.
        const OTHER: Tool = Tool {
            cache_namespace: "another",
            ..T
        };
        let a = cache_root_from(&T, Some("/x".into()), None).unwrap();
        let b = cache_root_from(&OTHER, Some("/x".into()), None).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn no_home_and_no_xdg_is_an_error_rather_than_a_guess() {
        assert!(cache_root_from(&T, None, None).is_err());
    }

    #[test]
    fn the_key_is_deterministic_and_sensitive_to_every_input() {
        let a = compute_key("u", "r1", "tc");
        assert_eq!(a, compute_key("u", "r1", "tc"));
        assert_ne!(a, compute_key("u", "r2", "tc"));
        assert_ne!(a, compute_key("v", "r1", "tc"));
        // a toolchain change re-keys, or a frozen engine binary gets paired
        // with something built by a different rustc
        assert_ne!(a, compute_key("u", "r1", "tc2"));
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn a_present_binary_short_circuits_without_invoking_cargo() {
        let dir = tempfile::tempdir().unwrap();
        let key = "deadbeefdeadbeef";
        let binpath = builds_dir(dir.path()).join(key).join("bin");
        std::fs::create_dir_all(&binpath).unwrap();
        // named for the tool's engine, which is what a second tool would miss
        std::fs::write(binpath.join("engine"), b"#!/bin/sh\n").unwrap();

        let resolved = crate::pin::resolve(
            &T,
            &Pin {
                url: "u".into(),
                reference: Reference::Rev("r".into()),
            },
            dir.path(),
        )
        .unwrap();
        let got = ensure_built(&T, dir.path(), key, &resolved).unwrap();
        assert_eq!(got, binpath.join("engine"));
    }

    #[test]
    fn a_package_whose_binary_is_named_differently_is_still_found() {
        // `cargo install widget-engine` on a package whose `[[bin]]` is
        // `widget` writes `bin/widget`, so a lookup keyed on the package name
        // finds nothing and rebuilds on every run, forever, silently.
        let dir = tempfile::tempdir().unwrap();
        let key = "deadbeefdeadbeef";
        let binpath = builds_dir(dir.path()).join(key).join("bin");
        std::fs::create_dir_all(&binpath).unwrap();
        std::fs::write(binpath.join("shortname"), b"#!/bin/sh\n").unwrap();

        const RENAMED: Tool = Tool {
            engine_bin: Some("shortname"),
            ..T
        };
        assert_ne!(
            RENAMED.engine_crate, "shortname",
            "the fixture proves nothing"
        );

        // No attempts, so a miss cannot fall through to a build: returning the
        // path is the only way this succeeds.
        let no_attempts = Resolved {
            pin: Pin {
                url: "u".into(),
                reference: Reference::Rev("r".into()),
            },
            key_rev: "r".into(),
            attempts: vec![],
            version_tag: String::new(),
        };
        let got = ensure_built(&RENAMED, dir.path(), key, &no_attempts).unwrap();
        assert_eq!(got, binpath.join("shortname"));

        // and the control: the same cache is a miss for the tool that did not
        // rename, which is what makes the hit above about `engine_bin` rather
        // than about the file merely existing
        assert!(ensure_built(&T, dir.path(), key, &no_attempts).is_err());
    }

    #[test]
    fn a_binary_under_another_tools_name_is_not_this_tools_build() {
        // the control on the one above: the short-circuit is keyed on the
        // engine's own name, so a cache populated by a different tool at the
        // same key must not read as a hit.
        //
        // Proved by handing it no attempts at all: if the short-circuit fired
        // it would return the path, and it cannot fall through to a build. That
        // keeps the control off the network, which the first version of this
        // test was not.
        let dir = tempfile::tempdir().unwrap();
        let key = "deadbeefdeadbeef";
        let binpath = builds_dir(dir.path()).join(key).join("bin");
        std::fs::create_dir_all(&binpath).unwrap();
        std::fs::write(binpath.join("somethingelse"), b"#!/bin/sh\n").unwrap();

        let no_attempts = Resolved {
            pin: Pin {
                url: "u".into(),
                reference: Reference::Rev("r".into()),
            },
            key_rev: "r".into(),
            attempts: vec![],
            version_tag: String::new(),
        };
        assert!(ensure_built(&T, dir.path(), key, &no_attempts).is_err());

        // and the positive control on the same input: the right name hits.
        std::fs::write(binpath.join("engine"), b"#!/bin/sh\n").unwrap();
        assert_eq!(
            ensure_built(&T, dir.path(), key, &no_attempts).unwrap(),
            binpath.join("engine")
        );
    }
}

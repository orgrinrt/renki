//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::pin::{Pin, Reference};
use crate::tool::Tool;

const T: Tool = Tool {
    short: "t",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "tns",
    default_url: "u",
    launcher_crate: "t-launcher",
    ..Tool::CONVENTIONS
};

#[test]
fn the_build_failure_names_every_cause_including_the_toolchain() {
    let msg = build_failure(T.engine_crate, &[
        "a failed".to_string(),
        "b failed".to_string(),
    ]);
    // the engine it could not build, and each attempt in order
    assert!(msg.contains("engine"), "{msg}");
    assert!(
        msg.contains("a failed") && msg.contains("b failed"),
        "{msg}"
    );
    // the five causes, the last two of which an operator cannot see from the
    // cargo output: an engine committing no lockfile resolves fresh under a
    // warning, and a locked dependency can still want a rustc above the one
    // in effect, so the failure names a crate nobody in the repo chose.
    for cause in [
        "pin may be wrong",
        "release may not exist",
        "build may have broken",
        "commit no lockfile",
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
    assert_eq!(engine_command_line(&T, Path::new("/w"), &[], &[]), [
        "t", "--dir", "/w"
    ]);
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
    let args: Vec<std::ffi::OsString> = ["lock", "--", "-v"].iter().map(Into::into).collect();
    assert_eq!(engine_command_line(&T, Path::new("/w"), &extra, &args), [
        "t",
        "--dir",
        "/w",
        "--dep",
        "{ path = \"x\" }",
        "lock",
        "--",
        "-v"
    ]);
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
    let r = cache_root_from(&T, None, Some("/x/cache".into()), Some("/home/u".into())).unwrap();
    assert_eq!(r, Path::new("/x/cache/tns"));

    let r = cache_root_from(&T, None, None, Some("/home/u".into())).unwrap();
    assert_eq!(r, Path::new("/home/u/.cache/tns"));
    // an empty XDG is not a setting
    let r = cache_root_from(&T, None, Some("".into()), Some("/home/u".into())).unwrap();
    assert_eq!(r, Path::new("/home/u/.cache/tns"));
}

#[test]
fn the_tools_own_cache_variable_wins_and_is_the_whole_path() {
    // The asymmetry this closes: a user could say which repository to work
    // on, through `<SHORT>_ROOT`, and could not say where several hundred
    // megabytes of built engines were going to land. `XDG_CACHE_HOME` moves
    // every other program's cache with it, which is a different request.
    let r = cache_root_from(
        &T,
        Some("/mnt/big".into()),
        Some("/x/cache".into()),
        Some("/home/u".into()),
    )
    .unwrap();
    assert_eq!(
        r,
        Path::new("/mnt/big"),
        "the namespace was appended to a path that already names this tool's cache"
    );

    // it wins over the fallback too, not only over XDG
    let r = cache_root_from(&T, Some("/mnt/big".into()), None, Some("/home/u".into())).unwrap();
    assert_eq!(r, Path::new("/mnt/big"));

    // an empty value is not a setting, the same as XDG's
    let r = cache_root_from(&T, Some("".into()), None, Some("/home/u".into())).unwrap();
    assert_eq!(r, Path::new("/home/u/.cache/tns"));

    // and the name is the tool's own, so two launchers do not read each
    // other's variable
    assert_eq!(T.cache_env(), "T_CACHE");
    const OTHER: Tool = Tool {
        short: "widget",
        ..T
    };
    assert_eq!(OTHER.cache_env(), "WIDGET_CACHE");
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
    let a = cache_root_from(&T, None, Some("/x".into()), None).unwrap();
    let b = cache_root_from(&OTHER, None, Some("/x".into()), None).unwrap();
    assert_ne!(a, b);
}

#[test]
fn no_home_and_no_xdg_is_an_error_rather_than_a_guess() {
    assert!(cache_root_from(&T, None, None, None).is_err());
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
            url:       "u".into(),
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
        pin:         Pin {
            url:       "u".into(),
            reference: Reference::Rev("r".into()),
        },
        key_rev:     "r".into(),
        attempts:    vec![],
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
        pin:         Pin {
            url:       "u".into(),
            reference: Reference::Rev("r".into()),
        },
        key_rev:     "r".into(),
        attempts:    vec![],
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

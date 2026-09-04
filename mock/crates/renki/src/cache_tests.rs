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
fn the_roots_are_the_table_renki_dirs_answers_for_this_host() {
    // The table itself is `renki-dirs`'s and tested there over every platform.
    // What this crate adds is the reading: the three values arrive from the
    // environment as `OsStr`, the tool's own variable wins as the whole path,
    // the XDG one takes the namespace under it, and the platform default is
    // whatever the host's column says.
    use std::ffi::OsStr;
    let os = |s: &'static str| Some(OsStr::new(s));
    let r = root_from::<renki_dirs::Cache>(&T, None, os("/x/cache"), os("/home/u")).unwrap();
    assert_eq!(r, Path::new("/x/cache/tns"));
    let r =
        root_from::<renki_dirs::Cache>(&T, os("/mnt/big"), os("/x/cache"), os("/home/u")).unwrap();
    assert_eq!(r, Path::new("/mnt/big"));
    let r = root_from::<renki_dirs::State>(&T, os("/mnt/state"), None, os("/home/u")).unwrap();
    assert_eq!(r, Path::new("/mnt/state"));
    // an empty value is not a setting
    let r = root_from::<renki_dirs::Cache>(&T, os(""), os(""), os("/home/u")).unwrap();
    let want = if cfg!(target_os = "macos") {
        "/home/u/Library/Caches/tns"
    } else {
        "/home/u/.cache/tns"
    };
    assert_eq!(r, Path::new(want));
    // and the state never lands beside the cache, on this host or any
    let c = root_from::<renki_dirs::Cache>(&T, None, None, os("/home/u")).unwrap();
    let s = root_from::<renki_dirs::State>(&T, None, None, os("/home/u")).unwrap();
    assert_ne!(c, s);
    assert!(!s.starts_with(&c));
}

#[test]
fn the_variables_are_the_tools_own_so_two_launchers_do_not_read_each_others() {
    assert_eq!(T.cache_env(), "T_CACHE");
    assert_eq!(T.state_env(), "T_STATE");
    const OTHER: Tool = Tool {
        short: "widget",
        ..T
    };
    assert_eq!(OTHER.cache_env(), "WIDGET_CACHE");
    assert_eq!(OTHER.state_env(), "WIDGET_STATE");
}

#[test]
fn a_value_that_is_not_text_is_refused_by_name_rather_than_replaced() {
    // A directory whose bytes do not decode would print as a different
    // directory, one that does not exist, under a name the operator cannot
    // find on disk. The refusal says which variable.
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let raw = OsStr::from_bytes(b"/home/\xff\xfe");
    let e = root_from::<renki_dirs::Cache>(&T, None, None, Some(raw)).unwrap_err();
    assert!(e.contains("HOME"), "{e}");
    let e = root_from::<renki_dirs::Cache>(&T, Some(raw), None, Some(OsStr::new("/home/u")))
        .unwrap_err();
    assert!(e.contains("T_CACHE"), "{e}");
    let e = root_from::<renki_dirs::State>(&T, None, Some(raw), Some(OsStr::new("/home/u")))
        .unwrap_err();
    assert!(e.contains("XDG_STATE_HOME"), "{e}");
}

#[test]
fn no_home_and_no_xdg_is_an_error_naming_the_kind_rather_than_a_guess() {
    let e = root_from::<renki_dirs::Cache>(&T, None, None, None).unwrap_err();
    assert!(e.contains("XDG_CACHE_HOME"), "{e}");
    let e = root_from::<renki_dirs::State>(&T, None, None, None).unwrap_err();
    assert!(e.contains("XDG_STATE_HOME"), "{e}");
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

/// An old-layout tree: registry, marker, one build and one tool under the one
/// directory an earlier launcher used for everything.
fn old_layout(old: &std::path::Path) {
    std::fs::create_dir_all(old.join("builds/k1/bin")).unwrap();
    std::fs::write(old.join("builds/k1/bin/engine"), b"e").unwrap();
    std::fs::create_dir_all(old.join("tools/t1")).unwrap();
    std::fs::write(old.join("registry.toml"), b"[[build]]\nkey = \"k1\"\n").unwrap();
    std::fs::write(old.join("launcher-selfupdate"), b"1").unwrap();
}

#[test]
fn the_old_layout_moves_whole_into_the_two_new_roots_when_the_cache_root_moved() {
    // the mac case: the cache root itself moved, so everything goes
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let cache = dir.path().join("Caches/tns");
    let state = dir.path().join("Support/tns/state");
    old_layout(&old);

    move_old_layout(&old, &cache, &state);

    assert_eq!(
        std::fs::read(state.join("registry.toml")).unwrap(),
        b"[[build]]\nkey = \"k1\"\n"
    );
    assert_eq!(
        std::fs::read(state.join("launcher-selfupdate")).unwrap(),
        b"1"
    );
    assert!(
        cache.join("builds/k1/bin/engine").is_file(),
        "the build moved"
    );
    assert!(cache.join("tools/t1").is_dir(), "the tool moved");
    assert!(
        !old.exists(),
        "the old directory is gone, since nothing was left in it"
    );
}

#[test]
fn the_old_layout_moves_only_the_state_when_the_cache_root_stayed() {
    // linux: the cache root is the same directory, so the builds stay put and
    // only the two state files leave
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("cache/tns");
    let state = dir.path().join("state/tns");
    old_layout(&old);

    move_old_layout(&old, &old, &state);

    assert!(state.join("registry.toml").is_file());
    assert!(state.join("launcher-selfupdate").is_file());
    assert!(!old.join("registry.toml").exists());
    assert!(!old.join("launcher-selfupdate").exists());
    assert!(
        old.join("builds/k1/bin/engine").is_file(),
        "the builds did not move"
    );
    assert!(old.join("tools/t1").is_dir());
}

#[test]
fn a_new_root_that_already_has_the_thing_wins_and_the_old_copy_is_deleted() {
    // the run after the first: whatever is left under the old layout is a
    // leftover, and a leftover build is deleted rather than kept beside the
    // registered one, since the collector never lists the directory
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let cache = dir.path().join("Caches/tns");
    let state = dir.path().join("Support/tns/state");
    old_layout(&old);
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("registry.toml"), b"new").unwrap();
    std::fs::create_dir_all(cache.join("builds/k2")).unwrap();

    move_old_layout(&old, &cache, &state);

    assert_eq!(
        std::fs::read(state.join("registry.toml")).unwrap(),
        b"new",
        "the new registry stayed"
    );
    assert!(
        state.join("launcher-selfupdate").is_file(),
        "the marker had no rival and moved"
    );
    assert!(cache.join("builds/k2").is_dir());
    assert!(
        !cache.join("builds/k1").exists(),
        "the old build was not merged in"
    );
    assert!(
        cache.join("tools/t1").is_dir(),
        "tools had no rival and moved"
    );
    assert!(!old.exists());
}

#[test]
fn nothing_under_the_old_layout_is_a_no_op_that_creates_nothing() {
    // the ordinary run, every time after the first: no old tree, no writes
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old");
    let cache = dir.path().join("cache");
    let state = dir.path().join("state");

    move_old_layout(&old, &cache, &state);

    assert!(!cache.exists());
    assert!(!state.exists());
}

#[test]
fn the_old_root_is_the_xdg_column_whatever_the_host_and_the_own_variable_wins_there_too() {
    // the old launcher read `<SHORT>_CACHE`, then `XDG_CACHE_HOME/<ns>`, then
    // `~/.cache/<ns>`, on a mac as much as anywhere; so the old root is the
    // XDG table over the same sources, and the two agree with the current root
    // exactly where the host is an XDG platform
    use renki_dirs::{Cache, Namespace, Root, Sources, Xdg};
    let ns = Namespace::new("tns").unwrap();
    let s = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: Maybe::Is("/home/u"),
    };
    assert_eq!(
        Root::<Cache, Xdg>::resolve(ns, s).unwrap().to_string(),
        "/home/u/.cache/tns"
    );
    let own = Sources {
        own: Maybe::Is("/mnt/big"),
        ..s
    };
    assert_eq!(
        Root::<Cache, Xdg>::resolve(ns, own).unwrap().to_string(),
        "/mnt/big"
    );
}

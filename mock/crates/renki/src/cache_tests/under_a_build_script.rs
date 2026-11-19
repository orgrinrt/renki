//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A launcher a build script started: it runs an engine already built, and where there is none
//! it returns at once rather than wait on anything the build that started it holds. Each wait
//! is bounded in the test itself, so a launcher that does wait fails the test and does not hang
//! the suite. Split from the cache tests by subject; the fixture is the parent module's.

use std::sync::mpsc;
use std::time::Duration;

use super::*;

/// How long a launcher under a build script may take to answer. Seconds, where the hang this
/// guards against lasted from twenty minutes to well over an hour.
const BOUND: Duration = Duration::from_secs(20);

const KEY: &str = "feedfacefeedface";

/// A build script's environment.
fn build_script(name: &str) -> Option<OsString> {
    crate::nested::BUILD_SCRIPT_VARS
        .contains(&name)
        .then(|| OsString::from("x"))
}

fn no_build_script(_: &str) -> Option<OsString> {
    None
}

fn resolved_with(attempts: Vec<Vec<String>>) -> Resolved {
    Resolved {
        pin: Pin {
            url:       "u".into(),
            reference: Reference::Rev("r".into()),
        },
        key_rev: "r".into(),
        attempts,
        version_tag: String::new(),
    }
}

/// `ensure_built_in` on a thread, its answer on the channel.
fn ensure_built_on_a_thread(
    cache: PathBuf,
    var: fn(&str) -> Option<OsString>,
) -> mpsc::Receiver<Result<PathBuf, BuildError>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // lint:allow(no-runtime-spawn) reason: the launcher under test on a thread of its own, so a wait it should not make fails the bound and does not hang the suite.
        let answer = ensure_built_in(&T, &cache, KEY, &resolved_with(vec![]), &cache, var);
        let _ = tx.send(answer);
    });
    rx
}

#[test]
fn a_launcher_under_a_build_script_returns_at_once_while_another_holds_the_keys_lock() {
    // Another launcher is building this key, and in the case met it was itself waiting on the
    // build directory's lock, which the build running the build script holds. Waiting on the
    // key's lock is then waiting on that build.
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().to_path_buf();
    let _another = BuildLock::take(&cache.join("builds"), KEY, "t").unwrap();
    let answer = ensure_built_on_a_thread(cache, build_script)
        .recv_timeout(BOUND)
        .expect("the launcher under a build script waited on the key's lock");
    let err = answer.unwrap_err();
    assert!(!err.is_refused(), "{err:?}");
    let message = err.into_message();
    assert!(message.contains("build script"), "{message}");
    assert!(message.contains("Run `t` once from a shell"), "{message}");
}

#[test]
fn outside_a_build_script_the_launcher_waits_for_the_keys_lock() {
    // The control of the test above: the same launcher with no build script around it waits
    // for the holder, which is what makes the second launcher on a key return the first one's
    // engine. Released here, it goes on and answers.
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().to_path_buf();
    let another = BuildLock::take(&cache.join("builds"), KEY, "t").unwrap();
    let rx = ensure_built_on_a_thread(cache, no_build_script);
    assert!(
        rx.recv_timeout(Duration::from_secs(2)).is_err(),
        "the launcher answered while another held the key's lock"
    );
    drop(another);
    assert!(rx.recv_timeout(BOUND).is_ok(), "released, it still waited");
}

#[test]
fn an_engine_already_built_runs_under_a_build_script() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp
        .path()
        .join("builds")
        .join(KEY)
        .join("bin")
        .join("engine");
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(&bin, "").unwrap();
    let found = ensure_built_in(
        &T,
        tmp.path(),
        KEY,
        &resolved_with(vec![]),
        tmp.path(),
        build_script,
    )
    .unwrap();
    assert_eq!(found, bin);
}

#[test]
fn the_install_under_a_build_script_starts_no_cargo_that_would_wait_on_the_build() {
    // The cargo here waits until the test lets it go, standing in for a `cargo install` waiting
    // on the lock of the build that started the build script. `Cargo::build_with` must answer
    // without starting it; the file it would leave says whether it started.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let waiting = dir.join("cargo");
    std::fs::write(
        &waiting,
        "#!/bin/sh\ntouch \"$(dirname \"$0\")/started\"\n\
         while [ ! -e \"$(dirname \"$0\")/released\" ]; do sleep 0.1; done\nexit 1\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&waiting, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let plan = crate::extension::CargoPlan {
        attempts:   vec![vec!["--path".into(), dir.display().to_string()]],
        bin:        "engine".into(),
        crate_name: "engine".into(),
        target_dir: dir.join("target"),
        build_dir:  None,
        dir:        Some(dir.clone()),
    };
    let (tx, rx) = mpsc::channel();
    let into = dir.join("into");
    std::thread::spawn(move || {
        // lint:allow(no-runtime-spawn) reason: the launcher under test on a thread of its own, so a wait it should not make fails the bound and does not hang the suite.
        let answer = crate::extension::Cargo::build_with(
            &plan,
            &into,
            || std::process::Command::new(&waiting),
            build_script,
        );
        let _ = tx.send(answer);
    });
    let answer = rx.recv_timeout(BOUND);
    std::fs::write(dir.join("released"), "").unwrap();
    let err = answer
        .expect("the install under a build script waited on its cargo")
        .unwrap_err();
    assert!(!err.is_refused(), "{err:?}");
    assert!(!dir.join("started").exists(), "a cargo was started");
}

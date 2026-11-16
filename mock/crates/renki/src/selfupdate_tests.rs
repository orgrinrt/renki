//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

const CRATES_TOML: &str = "\
[v1]
\"lch 0.1.0 (git+ssh://git@github.com/o/r.git?branch=dev#8dd3b750abc)\" = [\"lch\", \"t\"]
\"ripgrep 14.0.0 (registry+https://github.com/rust-lang/crates.io-index)\" = [\"rg\"]
";

#[test]
fn a_reinstall_is_locked_to_the_launcher_s_own_lockfile() {
    assert_eq!(
        reinstall_args(
            "ssh://git@github.com/o/r.git",
            "dev",
            "r-launcher",
            Path::new("/cache/launcher/target"),
        ),
        vec![
            "install",
            "--git",
            "ssh://git@github.com/o/r.git",
            "--branch",
            "dev",
            "r-launcher",
            "--locked",
            "--force",
            "--target-dir",
            "/cache/launcher/target",
        ]
    );
}

#[test]
fn a_reinstall_builds_in_the_cache_and_not_in_a_temporary_directory() {
    let cache = Path::new("/cache");
    let target = crate::cache::launcher_target_dir(cache);
    assert!(target.starts_with(cache), "{}", target.display());
    assert_eq!(target, cache.join("launcher").join("target"));
    // Stable across calls, so a repeat update compiles incrementally in one place.
    assert_eq!(target, crate::cache::launcher_target_dir(cache));
    // Not under the engine builds, which the collector removes key by key.
    assert!(!target.starts_with(cache.join("builds")));
}

#[test]
fn a_git_branch_install_is_parsed() {
    let src = installed_source_from("lch", CRATES_TOML).unwrap();
    assert_eq!(src.url, "ssh://git@github.com/o/r.git");
    assert_eq!(src.branch, "dev");
    assert_eq!(src.rev, "8dd3b750abc");
}

#[test]
fn only_this_launchers_entry_is_read() {
    // the control on the prefix match. Another tool's launcher in the same
    // ledger must not be chased, and a name this one is a prefix of must
    // not match either, which is why the space is part of the prefix.
    assert!(installed_source_from("ripgrep", CRATES_TOML).is_none());
    assert!(installed_source_from("l", CRATES_TOML).is_none());
    assert!(installed_source_from("lch-extra", CRATES_TOML).is_none());
}

#[test]
fn a_non_branch_install_is_not_chased() {
    // each of these is immutable or unknowable, so there is no newer head
    // to find and reinstalling would be churn.
    for toml in [
        // a tag install
        "[v1]\n\"lch 0.1.0 (git+ssh://x/y.git?tag=v1#abc)\" = [\"lch\"]\n",
        // a plain rev or default-branch install, carrying no query
        "[v1]\n\"lch 0.1.0 (git+ssh://x/y.git#abc)\" = [\"lch\"]\n",
        // a registry install: not a git source at all
        "[v1]\n\"lch 0.1.0 (registry+https://x)\" = [\"lch\"]\n",
        // a branch pin with an empty branch, which names nothing
        "[v1]\n\"lch 0.1.0 (git+ssh://x/y.git?branch=#abc)\" = [\"lch\"]\n",
        // `cargo install --path`, which is what somebody working on the
        // launcher itself runs, and what a repo's own readme is likely to
        // recommend. There is no remote to compare against, so the check
        // has nothing to chase, and a launcher installed this way stays
        // exactly as stale as its checkout.
        "[v1]\n\"lch 0.1.0 (path+file:///home/u/launcher)\" = [\"lch\"]\n",
    ] {
        assert!(installed_source_from("lch", toml).is_none(), "{toml}");
    }
}

#[test]
fn an_absent_or_unreadable_ledger_is_none() {
    assert!(installed_source_from("lch", "[v1]\n").is_none());
    assert!(installed_source_from("lch", "this is not [ toml").is_none());
    assert!(installed_source_from("lch", "").is_none());
}

#[test]
fn the_ttl_marker_gates_the_check_and_creates_its_own_directory() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("sub").join("launcher-selfupdate");
    assert!(!recently_checked(&marker, 10_000));
    mark_checked(&marker, 10_000);
    assert!(recently_checked(&marker, 10_000 + SELF_UPDATE_TTL_SECS - 1));
    assert!(!recently_checked(&marker, 10_000 + SELF_UPDATE_TTL_SECS));
}

#[test]
fn a_garbled_marker_does_not_suppress_the_check() {
    // the failure direction that matters: a marker that cannot be read must
    // mean "check now", never "checked recently", or a corrupt byte freezes
    // the launcher at its installed version forever.
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("m");
    for bad in ["", "not-a-number", "\n"] {
        std::fs::write(&marker, bad).unwrap();
        assert!(!recently_checked(&marker, 10_000), "{bad:?}");
    }
}

/// The fixture crate as a git repository with a `main` branch, as a reinstall fetches it.
fn fixture_repository(at: &std::path::Path) -> String {
    let src = crate::test_fixtures::fixture_engine(at);
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(&src)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "fixture"]);
    format!("file://{}", src.display())
}

/// `cargo`, installing into `root` rather than the person's own bin directory.
fn cargo_into(root: &std::path::Path) -> impl Fn() -> std::process::Command + '_ {
    move || {
        let mut cmd = std::process::Command::new("cargo");
        cmd.env("CARGO_INSTALL_ROOT", root);
        cmd.env("CARGO_NET_GIT_FETCH_WITH_CLI", "true");
        cmd
    }
}

#[test]
fn a_reinstall_leaves_the_launcher_and_no_target() {
    let tmp = tempfile::tempdir().unwrap();
    let url = fixture_repository(tmp.path());
    let cache = tmp.path().join("cache");
    let root = tmp.path().join("installed");
    let reinstall = Reinstall {
        url:            &url,
        branch:         "main",
        launcher_crate: "engine",
        cache:          &cache,
        build_dir:      None,
    };

    reinstall.run(cargo_into(&root)).unwrap();

    assert!(root.join("bin/engine").is_file());
    assert_eq!(
        crate::test_fixtures::targets_under(&cache).collect::<Vec<_>>(),
        Vec::<std::path::PathBuf>::new()
    );
}

#[test]
fn a_reinstall_waits_for_another_to_finish_with_the_shared_target() {
    // The target is one directory for every reinstall on the machine. One held by
    // another launcher keeps this one from starting, so neither removes the
    // directory the other is compiling into.
    let tmp = tempfile::tempdir().unwrap();
    let url = fixture_repository(tmp.path());
    let cache = tmp.path().join("cache");
    let root = tmp.path().join("installed");
    let other = crate::cache::BuildLock::hold(&cache, "launcher").unwrap();

    let (cache_t, root_t, url_t) = (cache.clone(), root.clone(), url.clone());
    let waiting = std::thread::spawn(move || {
        // lint:allow(no-runtime-spawn) reason: the second launcher the lock test holds, in-process because the call under test is.
        Reinstall {
            url:            &url_t,
            branch:         "main",
            launcher_crate: "engine",
            cache:          &cache_t,
            build_dir:      None,
        }
        .run(cargo_into(&root_t))
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(
        !waiting.is_finished(),
        "the reinstall ran under another's lock"
    );
    assert!(!root.join("bin/engine").exists());

    drop(other);
    waiting.join().unwrap().unwrap();
    assert!(root.join("bin/engine").is_file());
}

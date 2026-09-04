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
        reinstall_args("ssh://git@github.com/o/r.git", "dev", "r-launcher"),
        vec![
            "install",
            "--git",
            "ssh://git@github.com/o/r.git",
            "--branch",
            "dev",
            "r-launcher",
            "--locked",
            "--force",
        ]
    );
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

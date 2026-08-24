//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Keeping the launcher itself current.
//!
//! When the launcher was installed from a git branch (`cargo install --git …
//! --branch dev <launcher>`), that branch keeps moving. On invocation, at most
//! once an hour, it checks whether its branch has a newer head and,
//! if so, reinstalls itself and re-execs into the new binary. This is the
//! launcher-side twin of the engine's short branch-pin TTL: a repo tracking a
//! branch picks up new engine heads within the hour, and the launcher that
//! drives it stays current on the same cadence.
//!
//! It is deliberately conservative:
//!
//! - Best-effort. Any failure (offline, no cargo, a build break on the branch)
//!   leaves the current launcher running; an invocation never fails because
//!   self-update could not run.
//! - Opt-out with the tool's `<SHORT>_NO_SELF_UPDATE` (a CI job or a pinned
//!   developer sets it).
//! - Only for a git-**branch** install under the cargo home bin. A version, tag,
//!   or rev install is immutable and never chased; a binary built elsewhere
//!   (`cargo run`, a dev checkout) is left alone.
//! - Throttled by a one-hour TTL marker written *before* the reinstall, so the
//!   re-exec'd new binary finds a fresh marker and does not re-check in a loop.

use std::path::{Path, PathBuf};

use crate::tool::Tool;

/// Re-check the launcher's branch at most this often.
const SELF_UPDATE_TTL_SECS: u64 = 60 * 60;

/// The git-branch source a `cargo install` recorded for the launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledSource {
    url:    String,
    branch: String,
    rev:    String,
}

/// Check for and apply a launcher update. May reinstall and re-exec, in which
/// case it never returns; otherwise returns having done nothing user-visible.
pub fn maybe_self_update(tool: &Tool, cache_root: &Path) {
    if std::env::var_os(tool.no_self_update_env()).is_some() {
        return;
    }
    // Only an actually-installed binary self-updates; a dev build is left alone.
    let Some(exe) = current_exe_if_installed() else {
        return;
    };
    let now = crate::now_secs();
    let marker = cache_root.join("launcher-selfupdate");
    if recently_checked(&marker, now) {
        return;
    }
    // Write the marker BEFORE anything else, so a re-exec'd new binary sees a
    // fresh check and does not loop, and so a failed attempt still backs off.
    mark_checked(&marker, now);

    let Some(src) = installed_source(tool) else {
        return; // not a git-branch install: nothing to chase.
    };
    let Ok(head) = crate::pin::ls_remote_head(&src.url, &src.branch) else {
        return; // offline or branch gone: keep the current launcher.
    };
    if head == src.rev {
        return; // already current.
    }
    eprintln!(
        "{}: newer launcher on {}, updating ...",
        tool.short, src.branch
    );
    if reinstall(tool, &src.url, &src.branch).is_ok() {
        // Replace this process with the freshly-installed binary, same argv. On
        // success this never returns; on failure it falls through and the
        // current launcher carries on (the new binary is installed for next time).
        reexec(tool, &exe);
    }
}

/// `current_exe()` when it lives under the cargo home bin, else `None`.
fn current_exe_if_installed() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bin = cargo_home_bin()?;
    exe.starts_with(&bin).then_some(exe)
}

/// `$CARGO_HOME/bin` or `~/.cargo/bin`.
fn cargo_home_bin() -> Option<PathBuf> {
    cargo_home().map(|h| h.join("bin"))
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
}

/// The launcher's install source from cargo's `.crates.toml`, if it is a
/// git-branch install.
fn installed_source(tool: &Tool) -> Option<InstalledSource> {
    let content = std::fs::read_to_string(cargo_home()?.join(".crates.toml")).ok()?;
    installed_source_from(tool.launcher_crate, &content)
}

/// Parse the launcher's own install entry out of a `.crates.toml`, returning
/// its source only when it is a git-branch install. Pure, for testing.
fn installed_source_from(launcher: &str, crates_toml: &str) -> Option<InstalledSource> {
    let parsed: CratesToml = toml::from_str(crates_toml).ok()?;
    let prefix = format!("{launcher} ");
    let spec = parsed.v1.keys().find(|k| k.starts_with(&prefix))?;
    // `<launcher> 0.1.0 (git+<url>?branch=<b>#<rev>)`
    let inner = spec.split_once('(')?.1;
    let inner = inner.strip_suffix(')').unwrap_or(inner);
    parse_git_branch_spec(inner)
}

/// Parse a cargo git source spec, returning `Some` only for a `?branch=` pin.
fn parse_git_branch_spec(spec: &str) -> Option<InstalledSource> {
    let git = spec.strip_prefix("git+")?;
    let (loc, rev) = git.rsplit_once('#')?;
    let (url, query) = loc.split_once('?')?; // a branch pin always carries a query
    let branch = query.split('&').find_map(|kv| kv.strip_prefix("branch="))?;
    if branch.is_empty() || rev.is_empty() {
        return None;
    }
    Some(InstalledSource {
        url:    url.to_string(),
        branch: branch.to_string(),
        rev:    rev.to_string(),
    })
}

#[derive(serde::Deserialize)]
struct CratesToml {
    #[serde(default)]
    v1: std::collections::BTreeMap<String, Vec<String>>,
}

/// Whether the launcher checked its branch within the TTL.
fn recently_checked(marker: &Path, now: u64) -> bool {
    std::fs::read_to_string(marker)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|ts| now.saturating_sub(ts) < SELF_UPDATE_TTL_SECS)
        .unwrap_or(false)
}

fn mark_checked(marker: &Path, now: u64) {
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, now.to_string());
}

fn reinstall(tool: &Tool, url: &str, branch: &str) -> Result<(), String> {
    let status = std::process::Command::new("cargo")
        .args(["install", "--git", url, "--branch", branch, tool.launcher_crate, "--force"])
        .status()
        .map_err(|e| format!("could not run cargo install: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("cargo install of the updated launcher failed".to_string())
    }
}

/// Replace this process with `exe`, forwarding the original argv (minus the
/// program name; the new process re-derives its own). On success `exec` never
/// returns; on failure it returns and the caller continues with this process.
fn reexec(tool: &Tool, exe: &Path) {
    use std::os::unix::process::CommandExt;
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let err = std::process::Command::new(exe).args(&args).exec();
    eprintln!(
        "{}: could not re-exec updated launcher ({err}); continuing",
        tool.short
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRATES_TOML: &str = "\
[v1]
\"lch 0.1.0 (git+ssh://git@github.com/o/r.git?branch=dev#8dd3b750abc)\" = [\"lch\", \"t\"]
\"ripgrep 14.0.0 (registry+https://github.com/rust-lang/crates.io-index)\" = [\"rg\"]
";

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
}

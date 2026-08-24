//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Resolving a pinned engine to concrete build attempts.
//!
//! The pin schema lives in [`crate::manifest`]. This is the half that turns a
//! [`Pin`] into `cargo install` attempts, which means resolving a branch pin to
//! a concrete rev via `git ls-remote`, cached with a TTL.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::hash::Fnv;
pub use crate::manifest::{Pin, Reference};
use crate::tool::Tool;

/// A branch pin re-resolves to a concrete rev at most this often; a fresh
/// resolution within the window is reused without a network round-trip. Kept
/// short so a repo tracking a branch picks up new heads within the hour,
/// matching the launcher's own self-update cadence.
const BRANCH_TTL: Duration = Duration::from_secs(60 * 60);

/// A pin resolved to concrete build attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The pin this came from, kept so a tool's hooks can derive whatever they
    /// need to hand the engine without re-reading the config. A dependency that
    /// must match the exact revision the engine was built from is the case.
    pub pin: Pin,
    /// The stable component of the cache key: `v:<version>` for a release, the
    /// concrete rev for a rev or branch pin, `tag:<t>` for a tag.
    pub key_rev: String,
    /// One or more `cargo install` argument lists, source selectors and package
    /// name included, tried in order until one succeeds. `--root` and `--force`
    /// are added by the cache. A version pin tries the registry first, then the
    /// matching git tag.
    pub attempts: Vec<Vec<String>>,
}

impl Resolved {
    /// The git ref kind and value this resolved to, for a hook building a
    /// dependency pinned to the same source. Always a concrete immutable ref:
    /// a branch has already become the rev it pointed at.
    pub fn git_ref(&self) -> (&'static str, &str) {
        match &self.pin.reference {
            Reference::Version(v) | Reference::Tag(v) => ("tag", v.as_str()),
            Reference::Rev(_) | Reference::Branch(_) => ("rev", self.key_rev.as_str()),
        }
    }
}

/// Resolve a pin to concrete build attempts. A branch resolves to its current
/// head via `git ls-remote`, cached with a TTL; a rev, tag or version is
/// already immutable.
pub fn resolve(tool: &Tool, pin: &Pin, cache_root: &Path) -> Result<Resolved, String> {
    let git = |sel: &[&str]| -> Vec<String> {
        let mut a = vec!["--git".to_string(), pin.url.clone()];
        a.extend(sel.iter().map(|s| s.to_string()));
        a.push(tool.engine_crate.to_string());
        a
    };
    let (key_rev, attempts) = match &pin.reference {
        Reference::Version(v) => {
            (
                format!("v:{v}"),
                vec![
                    // the registry release first, which is the fast path once the
                    // engine publishes. Before that it fails on a cold build and
                    // falls through to the tag below, silently, since a failure is
                    // only reported when every attempt fails.
                    vec![tool.engine_crate.to_string(), "--version".into(), v.clone()],
                    // the matching git tag: works before the engine is published,
                    // and for git-only consumers after.
                    git(&["--tag", v]),
                ],
            )
        }
        Reference::Rev(r) => (r.clone(), vec![git(&["--rev", r])]),
        Reference::Tag(t) => (format!("tag:{t}"), vec![git(&["--tag", t])]),
        Reference::Branch(b) => {
            let sha = resolve_branch(pin, b, cache_root)?;
            let attempts = vec![git(&["--rev", &sha])];
            (sha, attempts)
        }
    };
    Ok(Resolved {
        pin: pin.clone(),
        key_rev,
        attempts,
    })
}

fn resolve_branch(pin: &Pin, branch: &str, cache_root: &Path) -> Result<String, String> {
    let cache = branch_resolution_path(cache_root, &pin.url, branch);
    if let Some(sha) = fresh_resolution(&cache) {
        return Ok(sha);
    }
    let sha = ls_remote_head(&pin.url, branch)?;
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let now = unix_now();
    let _ = std::fs::write(&cache, format!("{now}\n{sha}\n"));
    Ok(sha)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The recorded resolution if present and younger than the TTL.
fn fresh_resolution(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let ts: u64 = lines.next()?.trim().parse().ok()?;
    let sha = lines.next()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }
    (unix_now().saturating_sub(ts) <= BRANCH_TTL.as_secs()).then_some(sha)
}

pub(crate) fn ls_remote_head(url: &str, branch: &str) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["ls-remote", url, branch])
        .output()
        .map_err(|e| format!("could not run git ls-remote: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-remote {url} {branch} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let sha = text
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("")
        .to_string();
    if sha.len() < 7 {
        return Err(format!(
            "branch '{branch}' not found on {url} (ls-remote returned no rev)"
        ));
    }
    Ok(sha)
}

fn branch_resolution_path(cache_root: &Path, url: &str, branch: &str) -> std::path::PathBuf {
    let mut h = Fnv::new();
    h.write_field(url);
    h.write_field(branch);
    cache_root.join("branch-resolutions").join(h.hex())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::{Anchor, Hooks};

    const T: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "mock",
        config_file: "t.toml",
        pin_prefix: "t",
        engine_crate: "engine",
        cache_namespace: "t",
        default_url: "u",
        launcher_crate: "t-launcher",
        workdir: None,
        hooks: Hooks::NONE,
    };

    fn pin(r: Reference) -> Pin {
        Pin {
            url: "u".into(),
            reference: r,
        }
    }

    #[test]
    fn a_version_tries_the_registry_then_the_matching_tag() {
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&T, &pin(Reference::Version("0.0.0-d05".into())), d.path()).unwrap();
        assert_eq!(r.key_rev, "v:0.0.0-d05");
        assert_eq!(
            r.attempts,
            vec![
                vec!["engine", "--version", "0.0.0-d05"],
                vec!["--git", "u", "--tag", "0.0.0-d05", "engine"],
            ]
        );
        // and the dep a hook would build points at the tag, not the version
        assert_eq!(r.git_ref(), ("tag", "0.0.0-d05"));
    }

    #[test]
    fn the_engine_crate_named_in_the_attempts_is_the_tools_own() {
        // the control that makes the assertions above mean anything: nothing
        // mockspace-shaped is baked into the argument lists.
        let d = tempfile::tempdir().unwrap();
        const OTHER: Tool = Tool {
            engine_crate: "somethingelse",
            ..T
        };
        let r = resolve(&OTHER, &pin(Reference::Tag("v1".into())), d.path()).unwrap();
        assert_eq!(
            r.attempts,
            vec![vec!["--git", "u", "--tag", "v1", "somethingelse"]]
        );
    }

    #[test]
    fn a_rev_resolves_to_one_git_attempt() {
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&T, &pin(Reference::Rev("sha1".into())), d.path()).unwrap();
        assert_eq!(r.key_rev, "sha1");
        assert_eq!(
            r.attempts,
            vec![vec!["--git", "u", "--rev", "sha1", "engine"]]
        );
        assert_eq!(r.git_ref(), ("rev", "sha1"));
    }

    #[test]
    fn a_tag_resolves_to_the_tag_only() {
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&T, &pin(Reference::Tag("nightly".into())), d.path()).unwrap();
        assert_eq!(r.key_rev, "tag:nightly");
        assert_eq!(
            r.attempts,
            vec![vec!["--git", "u", "--tag", "nightly", "engine"]]
        );
        assert_eq!(r.git_ref(), ("tag", "nightly"));
    }

    #[test]
    fn a_branch_resolves_to_its_rev_and_the_git_ref_carries_the_rev_not_the_name() {
        // the case a hook actually depends on. A dependency pinned to `dev`
        // resolves again an hour later to a different head, and then links a
        // different revision than the engine it loads into. So `git_ref` must
        // hand back the concrete rev, and a fixture whose rev equals the branch
        // name cannot tell the two apart.
        let d = tempfile::tempdir().unwrap();
        let path = branch_resolution_path(d.path(), "u", "dev");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{}\nfeedface99c0ffee\n", unix_now())).unwrap();

        let r = resolve(&T, &pin(Reference::Branch("dev".into())), d.path()).unwrap();
        assert_eq!(
            r.key_rev, "feedface99c0ffee",
            "the cached resolution was ignored"
        );
        assert_eq!(r.git_ref(), ("rev", "feedface99c0ffee"));
        assert_eq!(
            r.attempts,
            vec![vec!["--git", "u", "--rev", "feedface99c0ffee", "engine"]]
        );
        // and the branch name is nowhere in what a hook would build from
        assert!(!r.git_ref().1.contains("dev"));
    }

    #[test]
    fn the_branch_resolution_is_reused_inside_the_ttl_and_not_outside_it() {
        let d = tempfile::tempdir().unwrap();
        let path = branch_resolution_path(d.path(), "u", "dev");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let now = unix_now();
        std::fs::write(&path, format!("{now}\nfeedface99\n")).unwrap();
        assert_eq!(fresh_resolution(&path), Some("feedface99".into()));

        std::fs::write(
            &path,
            format!("{}\nfeedface99\n", now - BRANCH_TTL.as_secs() - 1),
        )
        .unwrap();
        assert_eq!(fresh_resolution(&path), None);
    }

    #[test]
    fn a_malformed_or_empty_resolution_is_not_reused() {
        // the control on the reader: a truncated write must not resolve to an
        // empty rev, which would key the cache on nothing and build the default
        // branch instead of the pin.
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("m");
        for bad in [
            "",
            "notanumber\nabc\n",
            &format!("{}\n\n", unix_now()),
            "123\n",
        ] {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(fresh_resolution(&path), None, "{bad:?}");
        }
    }

    #[test]
    fn two_branches_on_one_url_do_not_share_a_resolution() {
        let d = tempfile::tempdir().unwrap();
        assert_ne!(
            branch_resolution_path(d.path(), "u", "dev"),
            branch_resolution_path(d.path(), "u", "main")
        );
        assert_ne!(
            branch_resolution_path(d.path(), "u", "dev"),
            branch_resolution_path(d.path(), "v", "dev")
        );
    }
}

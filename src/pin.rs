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
pub(crate) use crate::manifest::{Pin, Reference};
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
    /// The tag a version pin resolved to, which is the bare version unless the
    /// tool said its repository spells tags differently. Empty for every other
    /// reference kind, which carries its own ref already.
    ///
    /// Not public: it has to agree with `attempts`, and nothing outside the
    /// crate can keep that true. Read it through [`Resolved::git_ref`].
    pub(crate) version_tag: String,
}

impl Resolved {
    /// The git ref kind and value this resolved to, for a hook building a
    /// dependency pinned to the same source. Always a concrete immutable ref:
    /// a branch has already become the rev it pointed at.
    pub fn git_ref(&self) -> (&'static str, &str) {
        match &self.pin.reference {
            Reference::Version(_) => ("tag", self.version_tag.as_str()),
            Reference::Tag(t) => ("tag", t.as_str()),
            Reference::Rev(_) | Reference::Branch(_) => ("rev", self.key_rev.as_str()),
        }
    }
}

/// Resolve a pin to concrete build attempts. A branch resolves to its current
/// head via `git ls-remote`, cached with a TTL; a rev, tag or version is
/// already immutable.
pub(crate) fn resolve(tool: &Tool, pin: &Pin, cache_root: &Path) -> Result<Resolved, String> {
    let git = |sel: &[&str]| -> Vec<String> {
        let mut a = vec!["--git".to_string(), pin.url.clone()];
        a.extend(sel.iter().map(|s| s.to_string()));
        a.push(tool.engine_crate.to_string());
        a
    };
    let mut version_tag = String::new();
    let (key_rev, attempts) = match &pin.reference {
        Reference::Version(v) => {
            let key = format!("v:{v}");
            // the registry release first, which is the fast path once the
            // engine publishes. Before that it fails on a cold build and falls
            // through to the tags below, silently, since a failure is only
            // reported when every attempt fails.
            let mut attempts = vec![vec![
                tool.engine_crate.to_string(),
                "--version".into(),
                v.clone(),
            ]];
            // the matching git tags: work before the engine is published, and
            // for git-only consumers after. The bare version unless the tool
            // says its repository spells them differently.
            let tags = match tool.hooks.version_tags {
                Some(f) => f(v),
                None => vec![v.clone()],
            };
            attempts.extend(tags.iter().map(|t| git(&["--tag", t])));
            // The first is what a hook building a dependency points at: the
            // attempts are tried in order, so it is the one most likely to be
            // the tag that exists.
            version_tag = tags.first().cloned().unwrap_or_else(|| v.clone());
            (key, attempts)
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
        version_tag,
    })
}

fn resolve_branch(pin: &Pin, branch: &str, cache_root: &Path) -> Result<String, String> {
    let cache = branch_resolution_path(cache_root, &pin.url, branch);
    if let Some(sha) = fresh_resolution(&cache) {
        return Ok(sha);
    }
    let sha = match ls_remote_head(&pin.url, branch) {
        Ok(sha) => sha,
        Err(e) => {
            // Offline, or the remote is down. A stale resolution names a
            // revision that was the branch's tip an hour ago and is very
            // probably still built and sitting in the cache, so running from it
            // beats refusing to run at all. The self-update path already takes
            // this posture; the engine path used to be the one that stopped.
            let Some(sha) = any_resolution(&cache) else {
                return Err(e);
            };
            eprintln!("{e}");
            eprintln!("using the last known revision for {branch}: {sha}");
            return Ok(sha);
        }
    };
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
    let (ts, sha) = read_resolution(path)?;
    (unix_now().saturating_sub(ts) <= BRANCH_TTL.as_secs()).then_some(sha)
}

/// The recorded resolution whatever its age, for the case where asking the
/// remote is not possible.
fn any_resolution(path: &Path) -> Option<String> {
    read_resolution(path).map(|(_, sha)| sha)
}

fn read_resolution(path: &Path) -> Option<(u64, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let ts: u64 = lines.next()?.trim().parse().ok()?;
    let sha = lines.next()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }
    Some((ts, sha))
}

pub(crate) fn ls_remote_head(url: &str, branch: &str) -> Result<String, String> {
    // The full ref, because a bare name also matches `refs/tags/<name>` and
    // which one wins is then down to the order the remote lists them. Built
    // once and used for both the argument and the message, so the message
    // cannot describe a different question than the one that was asked.
    let refspec = format!("refs/heads/{branch}");
    let out = std::process::Command::new("git")
        .args(["ls-remote", url, &refspec])
        .output()
        .map_err(|e| format!("could not run git ls-remote: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git ls-remote {url} {refspec} failed: {}",
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
    use crate::tool::{Anchor, Cli, Hooks, Locate};

    const T: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "mock",
        config_file: "t.toml",
        pin_prefix: "t",
        engine_crate: "engine",
        engine_bin: None,
        cache_namespace: "t",
        default_url: "u",
        launcher_crate: "t-launcher",
        workdir: None,
        dir_flag: Cli::DIR_FLAG,
        engine_flag: Cli::ENGINE_FLAG,
        locate: Locate::DEFAULT,
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
    fn a_repository_that_prefixes_its_tags_can_say_so() {
        // `v0.1.0` is at least as common a spelling as `0.1.0`, and a tool
        // whose engine repository uses it could not be built from a version pin
        // until that engine published, with the failure blaming the pin.
        const PREFIXED: Tool = Tool {
            hooks: Hooks {
                version_tags: Some(|v| vec![format!("v{v}")]),
                ..Hooks::NONE
            },
            ..T
        };
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&PREFIXED, &pin(Reference::Version("0.1.0".into())), d.path()).unwrap();
        assert_eq!(
            r.attempts,
            vec![
                vec!["engine", "--version", "0.1.0"],
                vec!["--git", "u", "--tag", "v0.1.0", "engine"],
            ]
        );
        assert_eq!(r.git_ref(), ("tag", "v0.1.0"));
    }

    #[test]
    fn every_tag_a_tool_names_is_tried_in_order() {
        // A repository that changed convention partway has both spellings in
        // its history, and which one a given version is under is a fact about
        // that version rather than about the repository.
        const BOTH: Tool = Tool {
            hooks: Hooks {
                version_tags: Some(|v| vec![format!("v{v}"), v.to_string()]),
                ..Hooks::NONE
            },
            ..T
        };
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&BOTH, &pin(Reference::Version("0.1.0".into())), d.path()).unwrap();
        assert_eq!(r.attempts.len(), 3);
        assert_eq!(r.attempts[1], vec!["--git", "u", "--tag", "v0.1.0", "engine"]);
        assert_eq!(r.attempts[2], vec!["--git", "u", "--tag", "0.1.0", "engine"]);
    }

    #[test]
    fn the_engine_crate_named_in_the_attempts_is_the_tools_own() {
        // the control that makes the assertions above mean anything: nothing
        // is baked into the argument lists that did not come from the tool.
        let d = tempfile::tempdir().unwrap();
        const OTHER: Tool = Tool {
            engine_crate: "somethingelse",
            engine_bin: None,
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
    fn a_branch_falls_back_to_the_last_known_revision_when_the_remote_cannot_be_reached() {
        // On a train, or with the forge down. The recorded revision was the
        // branch's tip an hour ago and is very probably still built and sitting
        // in the cache, so running from it beats refusing to run at all. `u` is
        // not a remote, so git fails immediately and without a network.
        let d = tempfile::tempdir().unwrap();
        let path = branch_resolution_path(d.path(), "u", "dev");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let stale = unix_now() - BRANCH_TTL.as_secs() - 1;
        std::fs::write(&path, format!("{stale}\nfeedface99c0ffee\n")).unwrap();

        let r = resolve(&T, &pin(Reference::Branch("dev".into())), d.path()).unwrap();
        assert_eq!(r.key_rev, "feedface99c0ffee");

        // The control: with nothing recorded there is nothing to fall back to,
        // and the failure is reported rather than invented. Without this the
        // test above would pass against a resolver that had stopped consulting
        // the remote at all.
        let empty = tempfile::tempdir().unwrap();
        let err = resolve(&T, &pin(Reference::Branch("dev".into())), empty.path()).unwrap_err();
        assert!(
            err.contains("ls-remote"),
            "the failure did not name what could not be reached: {err}"
        );
    }

    #[test]
    fn the_full_ref_is_asked_for_so_a_tag_of_the_same_name_cannot_answer() {
        // A bare name matches `refs/tags/<name>` as well as `refs/heads/<name>`,
        // and which one comes back is then down to the order the remote lists
        // them. A repository carrying both is not exotic: a release tagged
        // after the branch it was cut from is exactly that.
        let empty = tempfile::tempdir().unwrap();
        let err = resolve(&T, &pin(Reference::Branch("dev".into())), empty.path()).unwrap_err();
        // The message is built from the same string the argument is, so this
        // reports what git was actually asked rather than a description of it.
        assert!(
            err.contains("refs/heads/dev"),
            "the branch was asked for by bare name: {err}"
        );
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

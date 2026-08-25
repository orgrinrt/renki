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
use crate::tool::{Tool, VersionSource};

/// A branch pin re-resolves to a concrete rev at most this often; a fresh
/// resolution within the window is reused without a network round-trip. Kept
/// short so a repo tracking a branch picks up new heads within the hour,
/// matching the launcher's own self-update cadence.
pub(crate) const BRANCH_TTL: Duration = Duration::from_secs(60 * 60);

/// A pin resolved to concrete build attempts.
///
/// Produced by parsing, never by a consumer, so it is closed for the same
/// reason [`crate::Reference`] and [`crate::Header`] are: a field added later
/// would otherwise be a breaking change.
#[non_exhaustive]
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
    ///
    /// Not public. It is the build plan rather than anything a hook needs, and
    /// leaving it reachable would mean a caller could rewrite the plan out from
    /// under the rest of the struct.
    pub(crate) attempts: Vec<Vec<String>>,
    /// The tag a version pin resolved to, which is the bare version unless the
    /// tool said its repository spells tags differently. Empty for every other
    /// reference kind, which carries its own ref already.
    ///
    /// Not public: it has to agree with `attempts`, which is also crate-only, so
    /// the pair moves together and no caller can leave one describing a source
    /// the other does not. Read it through [`Resolved::git_ref`].
    pub(crate) version_tag: String,
}

impl Resolved {
    /// The git ref kind and value this resolved to, for a hook building a
    /// dependency pinned to the same source. Always a concrete immutable ref:
    /// a branch has already become the rev it pointed at.
    ///
    /// Exact for a rev, a tag and a branch. For a **version** it is the first
    /// tag the pin would try, which is the one that built whenever the tool
    /// names a single tag, and that is the ordinary case. It is not exact for a
    /// tool whose `version_tags` hook names several and whose first one does not
    /// exist, nor under [`VersionSource::RegistryThenGitTag`] when the registry
    /// answered, because the build reports back only a path and not which
    /// attempt won.
    ///
    /// [`VersionSource::RegistryThenGitTag`]: crate::VersionSource::RegistryThenGitTag
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
            // The registry release first, when the tool has said its engine
            // crate name is one it owns. `cargo install` resolves by name, and
            // nothing here ties that name to `pin.url`, so for a tool that has
            // not said so the registry is a different artifact wearing the same
            // name and this list must not contain it. See `VersionSource`.
            let mut attempts = Vec::new();
            if matches!(tool.version_source, VersionSource::RegistryThenGitTag) {
                attempts.push(vec![
                    tool.engine_crate.to_string(),
                    "--version".into(),
                    v.clone(),
                ]);
            }
            // the matching git tags: work before the engine is published, and
            // for git-only consumers after. The bare version unless the tool
            // says its repository spells them differently.
            let tags = match tool.hooks.version_tags {
                Some(f) => f(v),
                None => vec![v.clone()],
            };
            // A hook naming no tag leaves nothing to try, and under the default
            // source there is no registry attempt to carry the pin either. The
            // loop that builds then runs zero times and reports a failure with
            // an empty list of what it tried, which names nothing and blames
            // the pin. Refuse here instead, where the hook can be named.
            if tags.is_empty() {
                return Err(format!(
                    "the {} version pin {v} has no tag to build from: this tool's `version_tags`                      hook returned an empty list. A version pin resolves through the tags on                      {}, so the hook has to name at least one.",
                    tool.short, pin.url,
                ));
            }
            attempts.extend(tags.iter().map(|t| git(&["--tag", t])));
            // The first tag, which is the one tried first. Not necessarily the
            // one that built: `ensure_built` walks the list and does not report
            // back which attempt won, so a tool naming several tags gets the
            // first here whichever one existed. See `Resolved::git_ref`.
            version_tag = tags[0].clone();
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

/// Delete branch resolutions no build could still want.
///
/// One file per url and branch, seventy bytes each, and nothing removed them:
/// a repo that once tracked a branch left one behind for good.
///
/// **The window is the build cache's, not [`BRANCH_TTL`].** Those are two
/// different questions and this swept on the wrong one. `BRANCH_TTL` says when
/// a resolution stops being taken as the branch's current tip, which is an
/// hour. It does not say when the file stops being read, because
/// [`resolve_branch`]'s offline arm reads it at any age on purpose: a revision
/// that was the tip yesterday is very probably still built and sitting in the
/// cache, and running from it beats refusing to run.
///
/// So sweeping on `BRANCH_TTL` deleted the fallback an hour after it was
/// written, leaving it to work only in the gap before the next collection pass.
/// `retention` is the honest bound: past it the collector has taken the build
/// the resolution names, so falling back to it would name a revision that has
/// to be fetched and rebuilt anyway, which is what asking the remote already
/// does, and does better.
///
/// Best-effort, like every other sweep here: failing to remove one costs a
/// seventy-byte file and must never fail a run.
pub(crate) fn sweep_branch_resolutions(cache_root: &Path, retention: Duration) {
    let Ok(entries) = std::fs::read_dir(cache_root.join("branch-resolutions")) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > retention);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn branch_resolution_path(cache_root: &Path, url: &str, branch: &str) -> std::path::PathBuf {
    let mut h = Fnv::new();
    h.write_field(url);
    h.write_field(branch);
    cache_root.join("branch-resolutions").join(h.hex())
}

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;

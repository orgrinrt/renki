//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The global launcher registry: a machine-wide record of which repos consume
//! which built engine, used to garbage-collect engine builds nothing pins
//! anymore and, forward-looking, to index every repo the launcher has seen.
//!
//! It lives at `<cache>/registry.toml`, beside `builds/`. Everything in it is
//! recomputable: re-read each repo's config, re-resolve, and the same rows
//! reappear. That is why it is cache rather than config, and why a wipe costs
//! only the first re-run of each repo. Durable per-developer state belongs
//! somewhere a wipe does not reach, and this file does not hold any.
//!
//! Two tables:
//!
//! - `[[consumer]]`, one per repo the launcher has run: where it is, what it
//!   pins, and which build key that resolved to. The pin `form` (`version` /
//!   `branch` / `rev` / `tag` / `legacy`) is recorded, so a `legacy` row is a
//!   repo that has not adopted an explicit pin yet. Nothing outside this crate
//!   can read that today; the file is on disk and parseable, and a query for it
//!   is a decision nobody has made.
//! - `[[build]]`, one per cached engine build: its key and when it was last
//!   used. GC removes a build no live consumer resolves to.
//!
//! The `name`, `workdir` and `engine_url` fields are recorded although nothing
//! reads them yet. They are what an index across repositories would need, and
//! recording them from the start costs a few bytes per repo, where adding them
//! later would leave every row written before that day incomplete.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::PinSource;
use crate::manifest::{Pin, Reference};
use crate::pin::Resolved;
use crate::tool::Tool;

/// GC runs at most this often; a `last_gc` within the window skips the pass.
const GC_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Pin form as recorded for a consumer. `Legacy` means the pin came from the
/// tool's [`legacy_pin`](crate::Hooks::legacy_pin) hook rather than from a pin
/// key in the config, so the repo has not adopted an explicit pin yet. Counting
/// those rows would say whether a migration is finished, and nothing exposes
/// them to a tool yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinForm {
    Version,
    Branch,
    Rev,
    Tag,
    Legacy,
}

impl PinForm {
    fn as_str(self) -> &'static str {
        match self {
            PinForm::Version => "version",
            PinForm::Branch => "branch",
            PinForm::Rev => "rev",
            PinForm::Tag => "tag",
            PinForm::Legacy => "legacy",
        }
    }
}

/// The whole registry file.
///
/// No schema tag. One was declared here and never written, so every file on
/// disk carried the same absent value and a migration reading it would have
/// learned nothing it did not already know from the field being missing. A
/// version that needs to tell two shapes apart adds the tag then and reads its
/// absence as the shape that came before, which is what it would have had to do
/// anyway. Unknown keys are ignored on load, so a file written by such a
/// version still parses here.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Registry {
    /// Unix seconds of the last GC pass, throttling the next one.
    #[serde(default)]
    pub last_gc: u64,
    #[serde(default, rename = "consumer")]
    pub consumers: Vec<Consumer>,
    #[serde(default, rename = "build")]
    pub builds: Vec<Build>,
}

/// One repo the launcher has run in.
///
/// Six of these fields the collector never reads: `name`, `workdir`,
/// `engine_url`, `pin_form`, `pin_value`, and `key` outside the sweep. They are
/// here for the person who opens the file, which is a real reader and the only
/// one there is: the registry sits at a known path under the cache root and
/// answers "why did this rebuild" in a way no log line does. Dropping them
/// would shrink the file and leave that question unanswerable.
///
/// So this is a record, not internal state, and a field is added here when it
/// answers something a reader would otherwise have to guess. A field nothing
/// reads and nobody would look for does not belong.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Consumer {
    /// Absolute repo root.
    pub root: String,
    /// Repo name (the root dir basename today; a real project name later).
    pub name: String,
    /// Absolute working directory the engine is run from.
    pub workdir: String,
    /// The engine source url this repo builds from.
    pub engine_url: String,
    /// `version` / `branch` / `rev` / `tag` / `legacy`.
    pub pin_form: String,
    /// The pin value (version string, branch name, rev, tag), empty for legacy.
    #[serde(default)]
    pub pin_value: String,
    /// The build key this consumer last resolved to.
    pub key: String,
    /// Whether [`Consumer::root`] is the repo root exactly, rather than its
    /// lossy rendering.
    ///
    /// A path is bytes and this file is TOML, which is text, so a root that is
    /// not valid UTF-8 can only be written here with the bytes replaced. The
    /// replacement names no file, so `is_dir` on it is false, so the row was
    /// dropped on every collection pass, so that repo's build became an orphan
    /// and was deleted while still pinned, and the repo paid a full cold
    /// rebuild every time some other repo happened to collect. Forever, with
    /// the launcher printing "once per version" each time.
    ///
    /// So the flag is read as "the not-a-directory rule may be applied to this
    /// row". Where it cannot, the row still ages out through the retention
    /// window like every other, which is the right answer anyway: a repo that
    /// is still there keeps moving `last_seen`.
    ///
    /// Defaults true, because every row written before the flag existed was
    /// written from a path that round-tripped.
    #[serde(default = "yes")]
    pub root_exact: bool,
    /// Unix seconds of the last run in this repo.
    pub last_seen: u64,
}

/// The default for [`Consumer::root_exact`] on a row that predates it.
fn yes() -> bool {
    true
}

/// One cached engine build.
///
/// `last_used` is what the collector decides on. The rest, like [`Consumer`]'s,
/// is for whoever opens the file: which engine and revision a directory of
/// build output came from, under which toolchain, and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Build {
    pub key: String,
    pub engine_url: String,
    pub key_rev: String,
    #[serde(default)]
    pub toolchain: String,
    pub built_at: u64,
    pub last_used: u64,
}

/// The registry file path under the cache root.
pub(crate) fn registry_path(cache_root: &Path) -> PathBuf {
    cache_root.join("registry.toml")
}

impl Registry {
    /// Load the registry, tolerating absence or corruption by returning an empty
    /// one (it is a rebuildable cache, never a hard dependency).
    pub(crate) fn load(path: &Path) -> Registry {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Serialize and write. Best-effort: a write failure is not worth failing a
    /// run over, since the registry is only an optimisation. The
    /// write is atomic (temp beside the target, then rename) so a concurrent
    /// launcher reading the registry never sees a half-written file.
    pub(crate) fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = toml::to_string_pretty(self) {
            // The pid in the name, because two launchers finishing at the same
            // moment would otherwise write the same temporary file and rename
            // whichever half won into place. The registry survives a torn read
            // by wiping itself, so this is a lost history rather than a
            // corruption, and it is one call to avoid.
            let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
            if std::fs::write(&tmp, &text).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    /// Upsert the current repo as a consumer (keyed by `root`) and the build it
    /// resolved to (keyed by `key`), stamping `now`.
    pub(crate) fn record(&mut self, r: &Recording<'_>) {
        let Recording {
            root,
            root_exact,
            name,
            workdir,
            engine_url,
            form,
            pin_value,
            key,
            key_rev,
            toolchain,
            now,
        } = *r;
        match self.consumers.iter_mut().find(|c| c.root == root) {
            Some(c) => {
                c.name = name.to_string();
                c.workdir = workdir.to_string();
                c.engine_url = engine_url.to_string();
                c.pin_form = form.as_str().to_string();
                c.pin_value = pin_value.to_string();
                c.key = key.to_string();
                c.root_exact = root_exact;
                c.last_seen = now;
            }
            None => self.consumers.push(Consumer {
                root: root.to_string(),
                name: name.to_string(),
                workdir: workdir.to_string(),
                engine_url: engine_url.to_string(),
                pin_form: form.as_str().to_string(),
                pin_value: pin_value.to_string(),
                key: key.to_string(),
                root_exact,
                last_seen: now,
            }),
        }
        match self.builds.iter_mut().find(|b| b.key == key) {
            Some(b) => b.last_used = now,
            None => self.builds.push(Build {
                key: key.to_string(),
                engine_url: engine_url.to_string(),
                key_rev: key_rev.to_string(),
                toolchain: toolchain.to_string(),
                built_at: now,
                last_used: now,
            }),
        }
    }

    /// Whether a GC pass is due (throttled by `last_gc`).
    pub(crate) fn gc_due(&self, now: u64) -> bool {
        now.saturating_sub(self.last_gc) >= GC_INTERVAL_SECS
    }

    /// Garbage-collect the build cache. A build is removed when no live consumer
    /// resolves to its key: either no consumer pins it at all (orphan, e.g. a
    /// repo re-pinned to a newer engine), or every consumer that pins it has
    /// been idle past `retention`. The `protect` key (the build resolving
    /// this very invocation) is never removed. Consumers whose repo root no
    /// longer exists on disk are dropped first, so a deleted repo orphans its
    /// build; a consumer whose root could not be written exactly is exempt from
    /// that rule and ages out through `retention` instead. See
    /// [`Consumer::root_exact`]. Removes the on-disk build dirs and the pruned `[[build]]` rows;
    /// returns the removed keys for logging.
    pub(crate) fn gc(
        &mut self,
        cache_root: &Path,
        protect: &str,
        retention: Duration,
        now: u64,
    ) -> Vec<String> {
        // A row whose root did not survive being written as text cannot be
        // checked against the disk: the rendering names no file. Keeping it is
        // the safe direction, since the retention window still ages it out.
        self.consumers
            .retain(|c| !c.root_exact || Path::new(&c.root).is_dir());
        self.last_gc = now;

        let builds_dir = cache_root.join("builds");
        let mut removed = Vec::new();
        let consumers = self.consumers.clone();
        self.builds.retain(|b| {
            if b.key == protect {
                return true;
            }
            let pinners: Vec<&Consumer> = consumers.iter().filter(|c| c.key == b.key).collect();
            let live = pinners
                .iter()
                .any(|c| now.saturating_sub(c.last_seen) < retention.as_secs());
            if live {
                return true;
            }
            // orphan (no pinners) or all pinners stale: evict.
            //
            // The key is read verbatim out of a file on disk, and what it
            // reaches here is a recursive delete. A key carrying a separator or
            // a `..` would put that delete outside the cache entirely, so it is
            // checked against the only shape this crate ever writes rather than
            // trusted for having come from our own file. A registry somebody
            // hand-edited is exactly the case, and it is the cheap check.
            if is_build_key(&b.key) {
                let _ = std::fs::remove_dir_all(builds_dir.join(&b.key));
            }
            removed.push(b.key.clone());
            false
        });
        removed
    }
}

/// One run, as the registry records it.
///
/// A struct rather than eleven parameters, ten of which are strings. Nothing
/// about `record(root, name, workdir, url, ...)` tells a caller which of them
/// it is looking at, so a transposition compiles and produces a registry whose
/// rows are quietly wrong; named fields make the same mistake a build error.
pub(crate) struct Recording<'a> {
    /// The absolute repo root, as text.
    pub root: &'a str,
    /// Whether that text is the root exactly. See [`Consumer::root_exact`].
    pub root_exact: bool,
    /// The repo name.
    pub name: &'a str,
    /// The absolute working directory the engine runs against, as text.
    pub workdir: &'a str,
    /// The engine source this repo builds from.
    pub engine_url: &'a str,
    /// Which of the pin forms the repo used.
    pub form: PinForm,
    /// The pin value, empty for a legacy pin.
    pub pin_value: &'a str,
    /// The build key the run resolved to.
    pub key: &'a str,
    /// The revision that key was computed from.
    pub key_rev: &'a str,
    /// The toolchain identity folded into the key.
    pub toolchain: &'a str,
    /// Unix seconds of this run.
    pub now: u64,
}

/// Whether `key` is a key this crate wrote: exactly the 16 lowercase hex
/// characters [`crate::cache::compute_key`] produces, and nothing else.
///
/// The point is not the length. It is that a value failing this cannot contain
/// a path separator, a `..`, a leading `/`, or anything else that makes
/// `builds_dir.join(key)` denote a directory outside the cache.
fn is_build_key(key: &str) -> bool {
    key.len() == 16
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The registry pin form and value for a resolved pin. Legacy overrides the
/// reference variant: a legacy pin is always a rev, but must register as legacy
/// for migration detection.
pub(crate) fn pin_form_and_value(pin: &Pin, source: PinSource) -> (PinForm, String) {
    let value = match &pin.reference {
        Reference::Version(v) | Reference::Branch(v) | Reference::Rev(v) | Reference::Tag(v) => {
            v.clone()
        }
    };
    let form = match source {
        PinSource::Legacy => PinForm::Legacy,
        PinSource::Config => match &pin.reference {
            Reference::Version(_) => PinForm::Version,
            Reference::Branch(_) => PinForm::Branch,
            Reference::Rev(_) => PinForm::Rev,
            Reference::Tag(_) => PinForm::Tag,
        },
    };
    (form, value)
}

/// Record this repo and its resolved build, then run a throttled collection
/// pass protecting the just-resolved key. Every step is best-effort; a registry
/// failure never blocks the exec.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_and_collect(
    tool: &Tool,
    cache_root: &Path,
    root: &Path,
    workdir: &Path,
    pin: &Pin,
    source: PinSource,
    resolved: &Resolved,
    toolchain: &str,
    key: &str,
) {
    let path = registry_path(cache_root);
    let mut reg = Registry::load(&path);
    let now = crate::now_secs();
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let (form, value) = pin_form_and_value(pin, source);
    reg.record(&Recording {
        root: &root.display().to_string(),
        root_exact: root.to_str().is_some(),
        name: &name,
        workdir: &workdir.display().to_string(),
        engine_url: &pin.url,
        form,
        pin_value: &value,
        key,
        key_rev: &resolved.key_rev,
        toolchain,
        now,
    });
    if reg.gc_due(now) {
        let removed = reg.gc(cache_root, key, tool.cache_retention, now);
        if !removed.is_empty() {
            eprintln!(
                "{}: cache gc removed {} unused engine build(s)",
                tool.short,
                removed.len()
            );
        }
        // The two leftovers nothing else collects, on the same schedule and
        // for the same reason. Both used to be swept only by the path that
        // creates them, which for `--engine` meant a user who passed the flag
        // once and never again kept the checkout and its target directory
        // forever, and for branch resolutions meant never.
        crate::engine::sweep(cache_root);
        crate::pin::sweep_branch_resolutions(cache_root, tool.cache_retention);
    }
    reg.save(&path);
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;

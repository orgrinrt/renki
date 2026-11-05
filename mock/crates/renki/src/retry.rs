//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A cargo install tried twice, after one prerequisite check.
//!
//! The failure that prompted this was a registry deleted under the build,
//! where the second attempt succeeded on its own once the redownload had
//! started. Two attempts and not a loop: a build that fails twice is a fault
//! rather than a race, and the existing failure message carries the retry.

use std::process::Command;

/// How a build of an engine came to nothing, told apart by whether the revision it was
/// building can be blamed. The launcher forgets a branch pin's recorded resolution on one
/// of the two and keeps it on the other (`pin::forget_resolution`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuildError {
    /// The build could not start: cargo is not on `PATH` or does not run, cargo could not be
    /// spawned, the key's lock was not taken, a directory was not made. This says nothing
    /// about the revision. Anything that is only a message becomes this one, through `From`.
    NotStarted(String),
    /// The build started and every attempt came to nothing: cargo failed each, or
    /// succeeded and left no binary. This is what the revision is blamed for, though cargo
    /// fails the same way when the network drops under a download and from here the two
    /// cannot be told apart.
    Refused(String),
}

impl BuildError {
    /// Whether the build ran and was refused, which is the one failure that forgets the
    /// branch resolution it was built from.
    pub(crate) const fn is_refused(&self) -> bool {
        matches!(self, Self::Refused(_))
    }

    /// What the operator reads, the same text whichever it is.
    pub(crate) fn into_message(self) -> String {
        match self {
            Self::NotStarted(message) | Self::Refused(message) => message,
        }
    }
}

impl From<String> for BuildError {
    fn from(message: String) -> Self {
        Self::NotStarted(message)
    }
}

/// The text alone, for the one route that carries text whatever failed: the scratch placement of a
/// backend that does not place itself.
impl From<BuildError> for String {
    fn from(error: BuildError) -> Self {
        error.into_message()
    }
}

/// How long the second attempt waits for the first's failure to clear. A
/// moment is what a registry's redownload needs to have started.
pub(crate) const RETRY_PAUSE: std::time::Duration = std::time::Duration::from_secs(2);

/// The one prerequisite checked ahead of the first attempt, so the refusal
/// names it rather than failing inside cargo's own output.
// FIXME: the design names two more prerequisites before the first attempt, the toolchain a `rust-toolchain.toml` in the checkout pins and the network where the pin is a git source, each refused naming the missing one. Neither is checked; the two ignored tests below are the catalogue, and this check gains the checkout and the source as arguments when they land.
pub(crate) fn cargo_is_on_path() -> Result<(), String> {
    match Command::new("cargo").arg("--version").output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            Err(format!(
                "cargo is on PATH and does not run: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ))
        },
        Err(e) => {
            Err(format!(
                "cargo is not on PATH ({e}); the engine is built with it, so install a rust toolchain first"
            ))
        },
    }
}

/// Every attempt in order, each tried twice: a second time after `pause`
/// when the first failed. The first success wins; the failure message names
/// the retry, or the binary that never appeared. Every attempt failing is
/// [`BuildError::Refused`]; a run that could not start cargo, or an empty list of
/// attempts, is [`BuildError::NotStarted`].
pub(crate) fn install_with_retry(
    attempts: &[Vec<String>],
    crate_name: &str,
    mut run: impl FnMut(&[String]) -> Result<Attempt, String>,
    mut pause: impl FnMut(),
) -> Result<(), BuildError> {
    let mut failures = Vec::new();
    for attempt in attempts {
        let first = run(attempt)?;
        if first == Attempt::Built {
            return Ok(());
        }
        pause();
        let second = run(attempt)?;
        if second == Attempt::Built {
            return Ok(());
        }
        failures.push(match (first, second) {
            (_, Attempt::NoBinary) | (Attempt::NoBinary, _) => {
                format!("{attempt:?} reported success but produced no binary")
            },
            _ => format!("{attempt:?} failed, and failed again when retried"),
        });
    }
    let message = crate::cache::build_failure(crate_name, &failures);
    // With nothing to try nothing ran, and the revision was not refused by anything.
    Err(if failures.is_empty() {
        BuildError::NotStarted(message)
    } else {
        BuildError::Refused(message)
    })
}

/// What one install attempt came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attempt {
    /// The binary is where it was asked for.
    Built,
    /// cargo said no.
    Failed,
    /// cargo said yes and the binary is not there, which is the wrong package
    /// or the wrong binary name rather than anything a retry mends.
    NoBinary,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempts() -> Vec<Vec<String>> {
        vec![vec!["a".into()], vec!["b".into()]]
    }

    #[test]
    fn a_failure_is_tried_once_more_and_the_second_success_wins() {
        let mut calls = Vec::new();
        let mut pauses = 0;
        let ok = install_with_retry(
            &attempts(),
            "x",
            |a| {
                calls.push(a[0].clone());
                Ok(if calls.len() == 2 { Attempt::Built } else { Attempt::Failed })
            },
            || pauses += 1,
        );
        assert!(ok.is_ok());
        assert_eq!(
            calls,
            ["a", "a"],
            "the same attempt again, never the next one first"
        );
        assert_eq!(pauses, 1);
    }

    #[test]
    fn two_failures_move_to_the_next_attempt_and_the_message_carries_the_retry() {
        let mut calls = Vec::new();
        let err = install_with_retry(
            &attempts(),
            "x",
            |a| {
                calls.push(a[0].clone());
                Ok(Attempt::Failed)
            },
            || {},
        )
        .unwrap_err();
        assert!(
            err.is_refused(),
            "every attempt failing is a refusal: {err:?}"
        );
        let err = err.into_message();
        assert_eq!(calls, ["a", "a", "b", "b"]);
        assert!(err.contains("failed again when retried"), "{err}");
        assert!(err.contains("x"), "{err}");
    }

    #[test]
    fn a_missing_binary_is_named_as_that_rather_than_as_a_failure() {
        let err = install_with_retry(&attempts()[.. 1], "x", |_| Ok(Attempt::NoBinary), || {})
            .unwrap_err();
        assert!(err.is_refused(), "{err:?}");
        let err = err.into_message();
        assert!(err.contains("produced no binary"), "{err}");
        assert!(!err.contains("retried"), "{err}");
    }

    #[test]
    fn a_first_success_needs_no_pause_and_a_run_error_stops_everything() {
        let mut pauses = 0;
        assert!(
            install_with_retry(&attempts(), "x", |_| Ok(Attempt::Built), || pauses += 1).is_ok()
        );
        assert_eq!(pauses, 0);
        let err = install_with_retry(&attempts(), "x", |_| Err("no cargo".to_string()), || {})
            .unwrap_err();
        assert_eq!(
            err,
            BuildError::NotStarted("no cargo".to_string()),
            "a run that could not start cargo stops everything and is not a refusal"
        );
    }

    #[test]
    fn nothing_to_try_is_a_build_that_never_started_and_not_a_refusal() {
        let mut ran = 0;
        let err = install_with_retry(
            &[],
            "x",
            |_| {
                ran += 1;
                Ok(Attempt::Built)
            },
            || {},
        )
        .unwrap_err();
        assert_eq!(ran, 0);
        assert!(!err.is_refused(), "{err:?}");
    }

    #[test]
    fn a_message_alone_is_a_build_that_never_started_and_reads_the_same_either_way() {
        let from_text = BuildError::from("no lock".to_string());
        assert_eq!(from_text, BuildError::NotStarted("no lock".to_string()));
        assert!(!from_text.is_refused());
        assert!(BuildError::Refused("no".to_string()).is_refused());
        assert_eq!(from_text.into_message(), "no lock");
        assert_eq!(BuildError::Refused("no".to_string()).into_message(), "no");
    }

    #[test]
    fn the_prerequisite_is_the_cargo_on_this_machine() {
        // The control on the check itself: this suite runs under cargo, so the
        // answer here is yes, and a refusal would be the check lying.
        assert!(cargo_is_on_path().is_ok());
    }

    // The two prerequisites the design names and this module does not check.
    // Each asserts the intended answer against the check that exists, and
    // fails, since the check cannot yet be asked about a checkout or a source:
    // that is the gap, and the FIXME on `cargo_is_on_path` names it.

    #[test]
    #[ignore = "catalogue: the toolchain a checkout pins is not checked before cargo runs; the FIXME on cargo_is_on_path; tracked #30"]
    fn a_pinned_toolchain_rustup_lacks_is_refused_by_name_before_cargo_runs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"nightly-1999-01-01\"\n",
        )
        .unwrap();
        let err =
            cargo_is_on_path().expect_err("a toolchain rustup lacks is refused before cargo runs");
        assert!(err.contains("nightly-1999-01-01"), "{err}");
    }

    #[test]
    #[ignore = "catalogue: the network is not checked where the pin is a git source; the FIXME on cargo_is_on_path; tracked #31"]
    fn an_unreachable_git_source_is_refused_by_name_before_cargo_runs() {
        let source = "ssh://git@nowhere.invalid/nobody/nothing.git";
        let err =
            cargo_is_on_path().expect_err("an unreachable git source is refused before cargo runs");
        assert!(err.contains(source), "{err}");
    }
}

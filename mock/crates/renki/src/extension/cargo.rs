//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `Cargo` backend: `cargo install` into a root, per attempt, with the plan it is told.
//! Split from the backend contract by subject; its paths are re-exported from `extension`.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::Backend;
use crate::retry::{Attempt, BuildError, RETRY_PAUSE, cargo_is_on_path, install_with_retry};

/// What a [`Cargo`] build is told to install.
///
/// `attempts` are argument lists tried in order until one succeeds, source
/// selector and package name included; `--root` and `--force` are added here.
/// A version pin has two, the registry and then the matching git tag.
#[derive(Debug, Clone)]
pub struct CargoPlan {
    /// The argument lists, tried in order.
    pub attempts:   Vec<Vec<String>>,
    /// The binary the install must produce under `<root>/bin`, checked because
    /// cargo reporting success is not the same as cargo having built the thing
    /// that was wanted.
    pub bin:        String,
    /// Named in the failure message, so an operator reads the crate that did
    /// not build rather than the launcher that asked for it.
    pub crate_name: String,
    /// The install's own target directory, passed as `--target-dir`. Cargo's own
    /// install otherwise compiles in a temporary directory that is gone with the
    /// process, so a killed build starts over; named, it resumes. One directory
    /// belongs to one build: two builds that compile into the same one race on
    /// cargo's copy of the finished binary, and nothing here keeps them apart, so
    /// a plan names a directory no other plan uses at the same time.
    ///
    /// Removed once the install has put the binary in place, so what stays is the
    /// binary and not a second copy of it and of everything it was built from.
    pub target_dir: PathBuf,
    /// Where the intermediates go, the consuming repository's build directory,
    /// given to cargo as `CARGO_BUILD_BUILD_DIR`. With one, `target_dir` holds
    /// only the final artifacts, and the install reuses every unit that comes out
    /// the same as the repository's own. The install does not read the
    /// repository's `[patch]`, its flags or its profile overrides, so a unit those
    /// change is compiled again. `None` compiles everything in `target_dir`.
    pub build_dir:  Option<PathBuf>,
    /// The directory the build runs in, when it is not the process's own:
    /// cargo is started with it as its working directory, so a relative path in
    /// the install arguments, `--root` and `--target-dir` among them, resolves
    /// there and not against the launcher. A toolchain file decides which rustc
    /// answers in a directory, so the build runs where the cache key's
    /// fingerprint was taken, the repository's root, or one revision is built
    /// under two toolchains and keyed twice.
    pub dir:        Option<PathBuf>,
}

/// Building something with cargo.
///
/// The launcher half's backend, and the reason [`Backend`] is a contract rather
/// than the one thing this crate happens to do. It is picked at compile time by
/// the launcher, so it goes through [`materialise_once`] directly and is not in
/// any [`Registry`]: its [`Plan`](Backend::Plan) is a [`CargoPlan`] rather than
/// a [`Descriptor`], and a table dispatched by a descriptor's `backend =` field
/// has nothing to hand it.
pub struct Cargo;

/// The whole argument list of one install: the attempt's own source selector
/// and package, then `--locked`, the root and `--force`.
///
/// `--locked` because the source's lockfile is what its author built and
/// tested, and an install that resolves afresh takes whatever the registry
/// published since. One such crate, released the same morning, did not build
/// on the toolchain the source pins, and every launcher on the machine stopped
/// until the engine was installed by hand. A pin names a revision so the build
/// is the same one every time; a fresh resolution is a different build wearing
/// the revision's name.
///
/// Holds where the source commits a lockfile. A source without one gets a
/// warning from cargo and the fresh resolution anyway, which is cargo's rule
/// and not this crate's to change; `build_failure` names it as a cause.
// lint:allow(trait-first-signatures) reason: an argument list handed to `Command::args`, at the launcher's std boundary. FIXME: an iterator once the callers take one.
pub(crate) fn install_args(attempt: &[String], into: &Path, target_dir: &Path) -> Vec<String> {
    let mut args = Vec::with_capacity(attempt.len() + 7);
    args.push("install".into());
    args.extend(attempt.iter().cloned());
    args.push("--locked".into());
    args.push("--root".into());
    args.push(into.display().to_string());
    args.push("--force".into());
    args.push("--target-dir".into());
    args.push(target_dir.display().to_string());
    args
}

impl Cargo {
    /// `cargo install <attempt> --locked --root <into> --force`, per attempt, first success
    /// wins, answering whether a failure is a build that was refused or one that never started.
    /// [`Backend::materialise`] is this with the answer read as its message.
    pub(crate) fn build(plan: &CargoPlan, into: &Path) -> Result<(), BuildError> {
        Self::build_with(
            plan,
            into,
            || Command::new("cargo"),
            crate::nested::process_var,
        )
    }

    /// [`Cargo::build`] with `cargo` the command each attempt starts from, so a test can hand
    /// the install an environment of its own, and `var` the environment read for whether this
    /// run is a build script's, where no attempt starts.
    pub(crate) fn build_with(
        plan: &CargoPlan,
        into: &Path,
        cargo: impl Fn() -> Command,
        var: impl Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Result<(), BuildError> {
        let root = plan.dir.as_deref().unwrap_or(into);
        crate::nested::refuse_inside_a_build_script(crate::nested::THE_SAME_COMMAND, root, var)?;
        cargo_is_on_path()?;
        let run = |attempt: &[String]| -> Result<Attempt, String> {
            let mut cmd = cargo();
            cmd.args(install_args(attempt, into, &plan.target_dir));
            crate::cache::in_selected_toolchain(&mut cmd, plan.dir.as_deref());
            set_build_dir(&mut cmd, plan.build_dir.as_deref(), &plan.target_dir);
            let status = cmd
                .status()
                .map_err(|e| format!("could not run cargo install: {e}"))?;
            Ok(if !status.success() {
                Attempt::Failed
            } else if into.join("bin").join(&plan.bin).is_file() {
                Attempt::Built
            } else {
                Attempt::NoBinary
            })
        };
        install_with_retry(&plan.attempts, &plan.crate_name, run, || {
            std::thread::sleep(RETRY_PAUSE);
        })?;
        drop_target(&plan.target_dir);
        Ok(())
    }
}

/// Give an install its build directory, always: `build_dir` where one was read for the
/// repository, and the install's own `target_dir` where none was.
///
/// Never left to cargo, because `cargo install` still reads `$CARGO_HOME`'s configuration and
/// an inherited `CARGO_BUILD_BUILD_DIR`, and a `{workspace-path-hash}` in either is expanded
/// against the installed package's checkout, a build directory per revision kept for good.
/// The target directory is removed with the install's other leavings once the binary is in.
pub(crate) fn set_build_dir(cmd: &mut Command, build_dir: Option<&Path>, target_dir: &Path) {
    cmd.env("CARGO_BUILD_BUILD_DIR", build_dir.unwrap_or(target_dir));
}

/// Remove an install's own target directory once the binary is in place.
///
/// Best-effort: a target that cannot be removed costs disk and not the run, and the next run
/// that finds the binary tries again.
pub(crate) fn drop_target(target_dir: &Path) {
    let _ = std::fs::remove_dir_all(target_dir);
}

impl Backend for Cargo {
    type Plan = CargoPlan;

    const NAME: &'static str = "cargo";

    /// `rustc -vV`, which carries the version, the commit hash, the host triple
    /// and the LLVM version.
    ///
    /// rustc is part of the compilation input, so a toolchain change re-keys and
    /// forces a coherent rebuild. A frozen binary paired with a moved toolchain
    /// is the failure this prevents.
    ///
    /// Read in the launcher's own directory, which is not where the build runs:
    /// the build runs in [`CargoPlan::dir`], where a `rust-toolchain.toml` can
    /// select another rustc. [`Backend::fingerprint`] takes no plan, and it is
    /// held as a function pointer by [`Registered`], so the directory cannot
    /// reach it without changing the contract every backend implements. The
    /// launcher's own key does not use this method; it reads rustc in the
    /// repository's root directly, the directory the build runs in, so the key
    /// and the build agree. A host that keys a [`Cargo`] build by this method
    /// gets the launcher's directory's toolchain and not the build's.
    fn fingerprint() -> String {
        crate::cache::rustc_fingerprint(None)
    }

    /// `cargo install <attempt> --locked --root <into> --force`, per attempt,
    /// first success wins.
    fn materialise(plan: &Self::Plan, into: &Path) -> Result<(), String> {
        Self::build(plan, into).map_err(BuildError::into_message)
    }

    /// `cargo install --root` takes cargo's own lock on the install root and
    /// moves the binary in itself, so a scratch and a rename would stack a
    /// second, weaker mechanism on a working one. The compiled state is in the
    /// plan's `build_dir` or `target_dir`, which the plan names and this backend
    /// does not move.
    fn places_itself() -> bool {
        true
    }
}

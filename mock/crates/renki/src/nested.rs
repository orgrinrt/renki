//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Telling a launcher that a build script started from one any other caller started.
//!
//! Cargo holds the build directory's lock for the whole of a build, its build scripts
//! included, and an engine install compiles in the repository's build directory. A
//! `cargo install` a build script starts therefore waits on the build that started it,
//! which waits on the build script, and neither ever returns. The program of a `cargo run`
//! and a `cargo test` binary run with the lock free. A build script alone has `TARGET` and
//! `HOST` set beside `CARGO` and `OUT_DIR`, which the other two also have; the sketch
//! `202611191450_cargo-holds-its-build-lock` measured both.

use std::ffi::OsString;
use std::path::Path;

/// The variables that together say a process is a build script's, or a descendant of one.
pub(crate) const BUILD_SCRIPT_VARS: [&str; 4] = ["CARGO", "OUT_DIR", "TARGET", "HOST"];

/// Whether `var` reads a build script's environment: every one of [`BUILD_SCRIPT_VARS`] set
/// and not empty.
///
/// A reader rather than the process environment, so a test hands in the variables without
/// mutating the environment the other tests in the binary read.
pub(crate) fn inside_a_build_script(var: impl Fn(&str) -> Option<OsString>) -> bool {
    BUILD_SCRIPT_VARS
        .iter()
        .all(|name| var(name).is_some_and(|value| !value.is_empty()))
}

/// The process environment, as the reader [`inside_a_build_script`] takes.
pub(crate) fn process_var(name: &str) -> Option<OsString> {
    std::env::var_os(name)
}

/// The tool's own name as the command to run, for [`refuse_inside_a_build_script`].
pub(crate) fn the_tool(short: &str) -> String {
    format!("`{short}`")
}

/// What [`refuse_inside_a_build_script`] says to run where the tool's name is not known: the
/// backend's install, which has a plan and no tool.
pub(crate) const THE_SAME_COMMAND: &str = "the same command";

/// Refuse a build where `var` reads a build script's environment, naming what to run instead.
///
/// `run` is the command that builds the engine from a shell, already quoted, and `root` the
/// repository the build would have run in, so the line says what to run and where.
pub(crate) fn refuse_inside_a_build_script(
    run: &str,
    root: &Path,
    var: impl Fn(&str) -> Option<OsString>,
) -> Result<(), String> {
    if !inside_a_build_script(var) {
        return Ok(());
    }
    Err(format!(
        "the engine for this pin is not built, and this run is a cargo build script's \
         ({} are set). A cargo started here would wait on the lock the build running this \
         script holds, so nothing is built. Run {run} once from a shell in {} to build \
         it, then start the build again.",
        BUILD_SCRIPT_VARS.join(", "),
        root.display()
    ))
}

#[cfg(test)]
#[path = "nested_tests.rs"]
mod tests;

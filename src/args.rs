//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Turning the argument list a shell handed the launcher into the one the
//! engine is invoked with.
//!
//! Three things happen between the two, and each is the launcher taking
//! something out rather than putting something in: the name the launcher was
//! invoked by, a repeated subcommand name where cargo invoked it, and the flags
//! the launcher owns.

use std::path::Path;

use crate::engine;
use crate::tool::{Locate, Tool};

/// The user-facing arguments to forward to the engine.
///
/// Two invocation shapes collapse to one. Invoked directly, every argument is
/// forwarded. Invoked as a cargo external subcommand, cargo executes
/// `cargo-<x> <x> <args...>`, so a leading `<x>` is dropped when the program
/// name is `cargo-<x>`. That is cargo's convention rather than any one tool's,
/// which is why it lives here.
///
/// A user-supplied [`Tool::dir_flag`] is stripped, in either spelling: the
/// launcher owns it and always passes the absolute working directory.
pub(crate) fn normalize_args(tool: &Tool, raw: &[String]) -> Vec<String> {
    let prog = raw
        .first()
        .map(|p| {
            Path::new(p)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();
    let mut rest: Vec<String> = raw.iter().skip(1).cloned().collect();
    if let Some(sub) = prog.strip_prefix("cargo-")
        && rest.first().map(String::as_str) == Some(sub)
    {
        rest.remove(0);
    }
    strip_dir_flag(rest, tool.dir_flag)
}

/// Drop a user-supplied `<dir_flag> <value>` pair anywhere in the args, in
/// either spelling. The launcher passes its own, so a second one is an
/// ambiguity the engine should never have to resolve.
///
/// A `<dir_flag>` with no value after it is dropped too, and that is not the
/// oversight it looks like beside the engine flag's refusal below. The user's
/// directory is discarded whether they named one or not, so naming nothing
/// changes nothing about the run.
pub(crate) fn strip_dir_flag(args: Vec<String>, dir_flag: &str) -> Vec<String> {
    engine::take_flag(args, dir_flag).1
}

/// Whether these arguments ask the launcher the locate question rather than
/// asking the engine anything.
///
/// Its own function because the guard on the left is load-bearing and easy to
/// lose: without it, a tool that wants no locate query at all has
/// `subcommand: None`, an invocation with no arguments compares `None` against
/// `None`, and every bare run answers the query instead of running the engine.
pub(crate) fn is_the_locate_query(locate: &Locate, args: &[String]) -> bool {
    locate.subcommand.is_some() && args.first().map(String::as_str) == locate.subcommand
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;

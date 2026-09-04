//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A subcommand the launcher answers itself, declared by the tool.
//!
//! `locate` and `config` are the crate's, fixed in name and in output. This
//! is the tool's: a name, a sentence and a function, tried where `config`
//! is and before a root is required. `Tool::commands` holds the table and
//! `Tool::defect` refuses the names the crate takes first.

use std::path::Path;

use crate::tool::Tool;

/// A subcommand the launcher answers without the engine, declared by the
/// tool.
///
/// `locate` and `config` are the crate's, fixed in name and in output. This
/// is the tool's: a name, a sentence and a function, tried right where
/// `config` is, after the launcher's own flags are off the arguments and
/// before it goes looking for a root. The case it exists for is a command
/// that makes a repository where there is none yet, which is exactly where
/// the engine cannot run.
///
/// A command is not a hook. It runs only when named, sees no pin, and cannot
/// reach the engine, since the engine may not be buildable where it runs. A
/// subcommand that needs the engine belongs in the engine.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    /// The subcommand, as typed after the launcher's name.
    pub name: &'static str,
    /// One sentence, for whoever prints a listing.
    pub doc:  &'static str,
    /// What answers it. A refusal is printed under the tool's name and exits
    /// nonzero, the way `config`'s is.
    pub run:  fn(&Invocation<'_>) -> Result<(), String>,
}

/// What a [`Command`] is handed: everything the launcher knows at the point
/// it stops looking for a root.
///
/// Built by the launcher and read through the accessors, since nothing
/// outside the crate has a reason to construct one and a field added later
/// should be a minor release, the way `ResolvedSetting` is shaped.
pub struct Invocation<'a> {
    tool:     &'a Tool,
    cwd:      &'a Path,
    root:     Option<&'a Path>,
    settings: &'a [crate::config::ResolvedSetting],
    args:     &'a [std::ffi::OsString],
}

impl<'a> Invocation<'a> {
    pub(crate) const fn new(
        tool: &'a Tool,
        cwd: &'a Path,
        root: Option<&'a Path>,
        settings: &'a [crate::config::ResolvedSetting],
        args: &'a [std::ffi::OsString],
    ) -> Self {
        Invocation {
            tool,
            cwd,
            root,
            settings,
            args,
        }
    }

    /// The descriptor the command was declared on.
    #[must_use]
    pub const fn tool(&self) -> &'a Tool {
        self.tool
    }

    /// Where the launcher was run from.
    #[must_use]
    pub const fn cwd(&self) -> &'a Path {
        self.cwd
    }

    /// The repository root, where the walk up from the cwd found one. `None`
    /// is a real answer here rather than a refusal: a command may be the
    /// thing that makes the repository.
    #[must_use]
    pub const fn root(&self) -> Option<&'a Path> {
        self.root
    }

    /// Every setting the tool declares, resolved from the flag, the variable,
    /// the repository's file where one was found, the person's file and the
    /// default, in that order. The text is the kind's canonical form, the
    /// same bytes the engine reads out of its environment.
    #[must_use]
    pub const fn settings(&self) -> &'a [crate::config::ResolvedSetting] {
        self.settings
    }

    /// What followed the command's name on the command line, with the
    /// launcher's own flags taken out wherever they sat: `--dir`, `--engine`
    /// and, on a tool with settings, `--cfg` are the launcher's before and
    /// after the name alike, so `widget spawn a --cfg k=v` hands the command
    /// `["a"]` and the setting.
    #[must_use]
    pub const fn args(&self) -> &'a [std::ffi::OsString] {
        self.args
    }

    /// The resolved text of one setting, by its dotted key.
    ///
    /// `None` only for a key the tool never declared, since every declared
    /// key resolves to something, the default at least. A command asking for
    /// its own tool's key gets `Some` every time, so the `None` arm is the
    /// typo's, and a command that would rather not write that arm reads
    /// through [`Invocation::setting`] once and keeps the text.
    #[must_use]
    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings
            .iter()
            .find(|s| s.key() == key)
            .map(|s| s.text())
    }
}

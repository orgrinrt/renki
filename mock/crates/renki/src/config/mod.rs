//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A tool's configuration, resolved once per run.
//!
//! The contracts are `renki-config`'s: the schema on the [`Tool`], the store,
//! the precedence and the provenance. What is this crate's is the reading
//! nothing `no_std` can do: the two files off the disk, the flag off the
//! command line, the variable off the environment, and the `String`
//! diagnostics the operator reads. The engine receives the result as
//! `<SHORT>_CFG_<KEY>` per setting plus `<SHORT>_CONFIG_FILE` naming the
//! person's file, the way it receives `--dir`, and never has two answers to
//! choose from. `<SHORT>_CONFIG` is a different variable, the config
//! directory's override, which is why the file's carries the suffix.

pub mod query;
pub mod toml;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use notko::{Maybe, Outcome};
use renki_config::{Declared, EnvKey, Lookup, Source, Store};

pub use self::toml::Toml;
use crate::tool::Tool;

/// The person's file: `<config root>/<config_file>`, so `~/.config/homma/homma.toml`
/// on linux. The name reuses [`Tool::config_file`], which names the file a
/// repository carries, on purpose: the same tool's configuration at two
/// scopes, and a reader who knows one knows where to look for the other.
pub(crate) fn user_file(tool: &Tool) -> Result<PathBuf, String> {
    let root = crate::cache::config_root(tool)?;
    Ok(root.join(tool.config_file))
}

/// The two documents' texts, read once. A file that is not there is `Isnt`;
/// one that is there and cannot be read is an error, since a settings file
/// the operator wrote and cannot be read is not the same as none.
pub(crate) struct Texts {
    pub user: Maybe<String>,
    pub repo: Maybe<String>,
}

pub(crate) fn read_texts(user: &Path, repo: Option<&Path>) -> Result<Texts, String> {
    let read = |path: &Path| -> Result<Maybe<String>, String> {
        if !path.is_file() {
            return Ok(Maybe::Isnt);
        }
        std::fs::read_to_string(path)
            .map(Maybe::Is)
            .map_err(|e| format!("could not read {}: {e}", path.display()))
    };
    Ok(Texts {
        user: read(user)?,
        repo: match repo {
            Some(p) => read(p)?,
            None => Maybe::Isnt,
        },
    })
}

/// What the command line and the environment answer for a key: the `--cfg
/// key=value` flags taken off the arguments, and the process environment.
#[derive(Debug)]
pub(crate) struct Cli {
    flags: Vec<(String, String)>,
    /// The tool's `<SHORT>_CFG_<KEY>` variables that are set to text, read
    /// once here so the lookup lends them. A non-text value reads as unset,
    /// since every kind wants text.
    env:   Vec<(String, String)>,
}

impl Cli {
    /// Take every `--cfg key=value` off `args`, in either spelling, and read
    /// the tool's own variables for every setting it declares.
    // lint:allow(trait-first-signatures) reason: the arguments with the flag taken off, an argument list at the launcher's std boundary. FIXME: an iterator once the callers take one.
    pub(crate) fn take(tool: &Tool, args: Vec<OsString>) -> Result<(Cli, Vec<OsString>), String> {
        if tool.settings.is_empty() {
            // Nothing to resolve, so `--cfg` is the engine's flag if it is
            // anybody's, and the arguments go through untouched.
            return Ok((
                Cli {
                    flags: Vec::new(),
                    env:   Vec::new(),
                },
                args,
            ));
        }
        let env = tool
            .settings
            .iter()
            .map(|r| EnvKey::of(tool.short, r.key()).to_string())
            .filter_map(|name| std::env::var(&name).ok().map(|v| (name, v)))
            .collect();
        Self::take_with(env, args)
    }

    /// [`Cli::take`] over a given environment, for the tests.
    // lint:allow(trait-first-signatures) reason: the arguments with the flag taken off, an argument list at the launcher's std boundary. FIXME: an iterator once the callers take one.
    pub(crate) fn take_with(
        env: Vec<(String, String)>,
        args: Vec<OsString>,
    ) -> Result<(Cli, Vec<OsString>), String> {
        let mut flags = Vec::new();
        let mut rest = Vec::with_capacity(args.len());
        let mut want_value = false;
        let mut users_from_here = false;
        for arg in args {
            if users_from_here {
                rest.push(arg);
                continue;
            }
            let text = arg.to_str();
            if want_value {
                want_value = false;
                let Some(text) = text else {
                    return Err("--cfg takes key=value, and what followed it is not text".into());
                };
                flags.push(split_flag(text)?);
                continue;
            }
            match text {
                Some("--") => {
                    users_from_here = true;
                    rest.push(arg);
                },
                Some(t) if t == crate::tool::Cli::CFG_FLAG => want_value = true,
                Some(t)
                    if t.starts_with(crate::tool::Cli::CFG_FLAG)
                        && t.as_bytes().get(crate::tool::Cli::CFG_FLAG.len()) == Some(&b'=') =>
                {
                    flags.push(split_flag(&t[crate::tool::Cli::CFG_FLAG.len() + 1 ..])?)
                },
                _ => rest.push(arg),
            }
        }
        if want_value {
            return Err("--cfg takes key=value, and nothing followed it".into());
        }
        Ok((
            Cli {
                flags,
                env,
            },
            rest,
        ))
    }
}

fn split_flag(text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
        _ => Err(format!("--cfg takes key=value, not {text:?}")),
    }
}

impl Lookup for Cli {
    fn flag<'s>(&'s self, key: &str) -> Maybe<&'s str> {
        match self.flags.iter().rev().find(|(k, _)| k == key) {
            Some((_, v)) => Maybe::Is(v),
            None => Maybe::Isnt,
        }
    }

    fn env<'s>(&'s self, name: EnvKey<'_>) -> Maybe<&'s str> {
        let name = name.to_string();
        match self.env.iter().find(|(k, _)| *k == name) {
            Some((_, v)) => Maybe::Is(v),
            None => Maybe::Isnt,
        }
    }
}

/// One resolved setting as the launcher carries it: the key, the canonical
/// text, and where it came from.
///
/// Public because a tool's own [`Command`](crate::Command) reads the table
/// the engine environment is built from. The text is the kind's canonical
/// form, the same bytes `<SHORT>_CFG_<KEY>` would hold, so a command parses
/// it through the kind the way an engine does: a list is `["a", "b"]` and
/// `renki_config::TextItems` walks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSetting {
    // lint:allow(no-bare-static-str) reason: the row's static key. FIXME: port to Str.
    key:    &'static str,
    // lint:allow(no-bare-string) reason: the canonical text, owned once per run at the launcher's std boundary. FIXME: port to Str.
    text:   String,
    source: Source,
}

impl ResolvedSetting {
    /// The dotted key.
    // lint:allow(no-bare-static-str) reason: the row's static key. FIXME: port to Str.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.key
    }

    /// The value in the kind's canonical text form.
    // lint:allow(no-bare-string) reason: the canonical text, borrowed from the row. FIXME: port to Str.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Which of the five places it came from.
    #[must_use]
    pub const fn source(&self) -> Source {
        self.source
    }
}

/// Every setting the tool declares, resolved from the two texts and the
/// command line, or the first refusal, by name and place.
// lint:allow(trait-first-signatures) reason: the resolved table, owned once per run at the launcher's std boundary. FIXME: an iterator once the callers take one.
pub(crate) fn resolve_all(
    tool: &Tool,
    cli: &Cli,
    texts: &Texts,
) -> Result<Vec<ResolvedSetting>, String> {
    fn parse<'t>(which: &str, text: &'t str) -> Result<<Toml as Store>::Document<'t>, String> {
        match Toml::parse(text) {
            Outcome::Ok(doc) => Ok(doc),
            Outcome::Err(e) => Err(format!("the {which} configuration is {e}")),
        }
    }
    let user = match &texts.user {
        Maybe::Is(t) => Maybe::Is(parse("user", t)?),
        Maybe::Isnt => Maybe::Isnt,
    };
    let repo = match &texts.repo {
        Maybe::Is(t) => Maybe::Is(parse("repository", t)?),
        Maybe::Isnt => Maybe::Isnt,
    };
    if let Maybe::Is(doc) = &user
        && let Some(key) = renki_config::unknown_keys(tool.settings, doc).next()
    {
        return Err(format!(
            "the user configuration names {key:?}, which {} has no setting called; `{} config schema` \
             lists them",
            tool.short, tool.short
        ));
    }
    if let Maybe::Is(doc) = &repo
        && let Some(key) = renki_config::misplaced_keys(tool.settings, doc).next()
    {
        return Err(format!(
            "the repository's {} sets {key:?}, which is a user setting and does not belong in a \
             repository file",
            tool.config_file
        ));
    }
    let mut out = Vec::with_capacity(tool.settings.len());
    for r in renki_config::resolve(tool.settings, tool.short, cli, repo.as_ref(), user.as_ref()) {
        match r {
            Outcome::Ok(r) => {
                out.push(ResolvedSetting {
                    key:    r.row().key(),
                    text:   r.to_string(),
                    source: r.source(),
                })
            },
            Outcome::Err(e) => return Err(format!("setting {e}")),
        }
    }
    Ok(out)
}

/// The variables the engine receives: one per setting, plus the person's file.
// lint:allow(trait-first-signatures) reason: the variables handed to `Command::envs`, at the launcher's std boundary. FIXME: an iterator once the callers take one.
pub(crate) fn engine_env(
    tool: &Tool,
    user_file: &Path,
    settings: &[ResolvedSetting],
) -> Vec<(String, OsString)> {
    let mut out: Vec<(String, OsString)> = settings
        .iter()
        .map(|s| {
            (
                EnvKey::of(tool.short, s.key).to_string(),
                OsString::from(&s.text),
            )
        })
        .collect();
    out.push((
        EnvKey::file(tool.short).to_string(),
        user_file.as_os_str().to_os_string(),
    ));
    out
}

/// The row for a key, or why there is none.
pub(crate) fn row_of<'t>(tool: &'t Tool, key: &str) -> Result<&'t Declared<Toml>, String> {
    tool.settings
        .iter()
        .find(|r| r.key() == key)
        .ok_or_else(|| {
            format!(
                "{} has no setting called {key:?}; `{} config schema` lists them",
                tool.short, tool.short
            )
        })
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

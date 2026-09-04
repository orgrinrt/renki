//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! `widget config`, the query the launcher answers without the engine.
//!
//! The engine is the thing being configured and may not build, so this runs
//! before the pin is resolved, the way `locate` does. Five verbs: `path`,
//! `schema`, `get <key>`, `set <key> <value>` and `edit`. The output is
//! `key=value` lines, like `locate`, and `get` prints the source beside the
//! value, since the question behind most configuration bugs is not what the
//! value is but which of five places it came from.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;

use notko::{Maybe, Outcome};
use renki_config::{Rendered, Store};

use super::{Cli, Texts, Toml, read_texts, resolve_all, row_of, user_file};
use crate::tool::Tool;

/// The subcommand, fixed: a tool with settings answers it, one without leaves
/// it to the engine.
pub(crate) const SUBCOMMAND: &str = "config";

/// Whether `args` is this query for `tool`.
pub(crate) fn is_the_config_query(tool: &Tool, args: &[OsString]) -> bool {
    !tool.settings.is_empty() && args.first().and_then(|a| a.to_str()) == Some(SUBCOMMAND)
}

/// Answer the query. `root` is the repository root where one was found, for
/// the repository's file.
pub(crate) fn answer(
    tool: &Tool,
    root: Option<&Path>,
    cli: &Cli,
    args: &[OsString],
) -> Result<(), String> {
    let verb = args.get(1).and_then(|a| a.to_str());
    let user = user_file(tool)?;
    let repo = root.map(|r| r.join(tool.config_file));
    let out = std::io::stdout();
    let mut out = out.lock();
    match verb {
        Some("path") => {
            writeln!(out, "user={}", user.display()).map_err(|e| e.to_string())?;
            match &repo {
                Some(r) if r.is_file() => writeln!(out, "repo={}", r.display()),
                _ => writeln!(out, "repo="),
            }
            .map_err(|e| e.to_string())?;
            Ok(())
        },
        Some("schema") => {
            for row in tool.settings {
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}",
                    row.key(),
                    row.kind(),
                    row.scope(),
                    row.default(),
                    row.doc()
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        },
        Some("get") => {
            let key = args
                .get(2)
                .and_then(|a| a.to_str())
                .ok_or("config get takes a key")?;
            row_of(tool, key)?;
            let texts = read_texts(&user, repo.as_deref())?;
            let all = resolve_all(tool, cli, &texts)?;
            let one = all
                .iter()
                .find(|s| s.key == key)
                .ok_or("resolved nothing for the key")?;
            writeln!(out, "{}={}", one.key, one.text).map_err(|e| e.to_string())?;
            writeln!(out, "source={}", one.source).map_err(|e| e.to_string())?;
            Ok(())
        },
        Some("set") => {
            let key = args
                .get(2)
                .and_then(|a| a.to_str())
                .ok_or("config set takes a key and a value")?;
            let value = args
                .get(3)
                .and_then(|a| a.to_str())
                .ok_or("config set takes a key and a value")?;
            let text = set(tool, &user, key, value)?;
            write_user(&user, &text)?;
            writeln!(out, "{key}={value}").map_err(|e| e.to_string())?;
            writeln!(out, "file={}", user.display()).map_err(|e| e.to_string())?;
            Ok(())
        },
        Some("edit") => {
            if let Some(parent) = user.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
            }
            let editor = std::env::var_os("VISUAL")
                .or_else(|| std::env::var_os("EDITOR"))
                .unwrap_or_else(|| "vi".into());
            let status = std::process::Command::new(&editor)
                .arg(&user)
                .status()
                .map_err(|e| format!("could not run {}: {e}", editor.to_string_lossy()))?;
            if !status.success() {
                return Err(format!("{} exited with {status}", editor.to_string_lossy()));
            }
            // The schema check, so a typo in the edit is refused now rather than
            // on the next run of something else.
            let texts = read_texts(&user, repo.as_deref())?;
            resolve_all(tool, cli, &texts).map(|_| ())
        },
        _ => {
            Err(format!(
                "{} config takes one of: path, schema, get <key>, set <key> <value>, edit",
                tool.short
            ))
        },
    }
}

/// Write the person's file, creating its directory first: the first `config
/// set` on a machine is what makes `~/.config/<ns>/` exist, the same way
/// `edit` creates it before opening the editor.
pub(crate) fn write_user(user: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = user.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    std::fs::write(user, text).map_err(|e| format!("could not write {}: {e}", user.display()))
}

/// The person's file with `key` set to `value`, as text: the value checked by
/// the row's kind, rendered canonically, and written into the existing text
/// with everything else kept, comments included.
pub(crate) fn set(tool: &Tool, user: &Path, key: &str, value: &str) -> Result<String, String> {
    let row = row_of(tool, key)?;
    if !row.reads_user() {
        return Err(format!(
            "{key:?} is a repository setting and is set in the repository's {}, not the user file",
            tool.config_file
        ));
    }
    if let Outcome::Err(e) = row.check_text(value) {
        return Err(format!("{key}: {e}"));
    }
    let canonical = Canonical(row, value).to_string();
    let rendered = if row.quoted() {
        Rendered::Text(&canonical)
    } else {
        Rendered::Raw(&canonical)
    };
    let existing = match read_texts(user, None)?.user {
        Maybe::Is(t) => t,
        Maybe::Isnt => String::new(),
    };
    let mut out = String::with_capacity(existing.len() + 64);
    Toml::set(&existing, key, rendered, &mut out)
        .map_err(|_| "could not render the file".to_string())?;
    // and the file has to parse back with the schema still holding, which is
    // what stops a value carrying a newline from splitting a line in two
    let texts = Texts {
        user: Maybe::Is(out.clone()),
        repo: Maybe::Isnt,
    };
    let cli = Cli::take_with(Vec::new(), Vec::new())?.0;
    resolve_all(tool, &cli, &texts)?;
    Ok(out)
}

/// A value's canonical text through its row, so a list typed as `a, b`
/// lands in the file as `["a", "b"]`.
struct Canonical<'a>(&'a renki_config::Declared<Toml>, &'a str);

impl core::fmt::Display for Canonical<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.render_text(self.1, f)
    }
}

/// The lines `config get` would print for every setting, for the tests.
#[cfg(test)]
pub(crate) fn lines(settings: &[super::ResolvedSetting]) -> String {
    let mut out = String::new();
    for s in settings {
        out.push_str(s.key);
        out.push('=');
        out.push_str(&s.text);
        out.push('\n');
    }
    out
}

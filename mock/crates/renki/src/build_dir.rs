//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where the consuming repository's cargo keeps its intermediates, so an install compiles there.
//!
//! `cargo install` reads no project configuration, only `$CARGO_HOME`'s, so an engine built
//! for a repository would otherwise compile every dependency again in a target of its own. This
//! reads cargo's configuration the way a `cargo build` in the repository's root would: the
//! variable first, then each `.cargo/config` or `.cargo/config.toml` from the root upward, the
//! extensionless one where both are there, then `$CARGO_HOME`'s. `build.build-dir` wins at any
//! level; where no level sets it, the target directory is the build directory, as it is for
//! cargo. A file that does not parse is an error, as it is for cargo.
//!
//! A value from a file is relative to the directory holding its `.cargo`, and one from a
//! variable to the root, which is where the install runs. `{workspace-root}` is the root, the
//! workspace the value was written for: cargo would read it as the installed package's
//! checkout, a directory per revision. A value with `{workspace-path-hash}` names no build
//! directory, since the hash cargo would compute for the install is of that checkout and not of
//! the repository. Any other template, `{cargo-cache-home}`, does not depend on the workspace
//! and is handed on for cargo to expand.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The build directory cargo would use in `root`, `None` where nothing is configured, and
/// an error naming the file where a configuration file does not parse.
pub(crate) fn build_dir(root: &Path) -> Result<Option<PathBuf>, String> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")));
    resolve(root, home.as_deref(), |name| std::env::var_os(name))
}

/// [`build_dir`] on its inputs, so a test hands in the variables and the cargo home.
pub(crate) fn resolve(
    root: &Path,
    cargo_home: Option<&Path>,
    var: impl Fn(&str) -> Option<OsString>,
) -> Result<Option<PathBuf>, String> {
    let files = config_files(root, cargo_home)?;
    let from_var = |name: &str| {
        var(name)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string_lossy().into_owned())
            .map(|v| (root.to_path_buf(), v))
    };
    let from_files = |key: &str| {
        files
            .iter()
            .find_map(|(base, table)| setting(table, key).map(|v| (base.clone(), v)))
    };
    let found = from_var("CARGO_BUILD_BUILD_DIR")
        .or_else(|| from_files("build-dir"))
        .or_else(|| from_var("CARGO_TARGET_DIR"))
        .or_else(|| from_var("CARGO_BUILD_TARGET_DIR"))
        .or_else(|| from_files("target-dir"));
    Ok(found.and_then(|(base, value)| place(&base, root, &value)))
}

/// Each configuration file cargo reads in `root`, nearest first, with the directory its
/// relative paths are taken from. `$CARGO_HOME`'s comes last and only once.
fn config_files(
    root: &Path,
    cargo_home: Option<&Path>,
) -> Result<Vec<(PathBuf, toml::Table)>, String> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    let homes = cargo_home.map(|h| (h.to_path_buf(), h.parent().map(Path::to_path_buf)));
    let walked = root
        .ancestors()
        .map(|dir| (dir.join(".cargo"), Some(dir.to_path_buf())));
    for (dot_cargo, base) in walked.chain(homes) {
        if seen.contains(&dot_cargo) {
            continue;
        }
        seen.push(dot_cargo.clone());
        let Some(base) = base else { continue };
        // Cargo reads the extensionless file where both are there.
        let Some(file) = ["config", "config.toml"]
            .iter()
            .map(|name| dot_cargo.join(name))
            .find(|f| f.is_file())
        else {
            continue;
        };
        let text = std::fs::read_to_string(&file)
            .map_err(|e| format!("could not read {}: {e}", file.display()))?;
        let table = text
            .parse::<toml::Table>()
            .map_err(|e| format!("could not parse {}: {e}", file.display()))?;
        out.push((base, table));
    }
    Ok(out)
}

/// `[build] <key>` as text, where the table sets it.
fn setting(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get("build")?
        .get(key)?
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// `value` as a directory: `{workspace-root}` read as `root`, made absolute against `base`
/// where it is relative, and `None` where it carries `{workspace-path-hash}`.
fn place(base: &Path, root: &Path, value: &str) -> Option<PathBuf> {
    if value.contains("{workspace-path-hash}") {
        return None;
    }
    let value = value.replace("{workspace-root}", &root.to_string_lossy());
    let path = PathBuf::from(&value);
    Some(if path.is_absolute() || value.starts_with('{') {
        path
    } else {
        base.join(path)
    })
}

#[cfg(test)]
#[path = "build_dir_tests.rs"]
mod tests;

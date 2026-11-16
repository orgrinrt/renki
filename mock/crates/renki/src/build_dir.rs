//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where the consuming repository's cargo keeps its intermediates, so an install compiles there.
//!
//! `cargo install` reads no project configuration, only `$CARGO_HOME`'s, so an engine built
//! for a repository would otherwise compile every dependency again in a target of its own. This
//! reads cargo's configuration the way a `cargo build` in the repository's root would: the
//! variable first, then each `.cargo/config.toml` from the root upward, then `$CARGO_HOME`'s.
//! `build.build-dir` wins at any level; where no level sets it, the target directory is the
//! build directory, as it is for cargo.
//!
//! A value from a file is relative to the directory holding its `.cargo`, and one from a
//! variable to the root, which is where the install runs. A value with a template in it
//! (`{workspace-root}` and the rest) is handed on verbatim for cargo to expand.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The build directory cargo would use in `root`, or `None` where nothing is configured.
pub(crate) fn build_dir(root: &Path) -> Option<PathBuf> {
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
) -> Option<PathBuf> {
    let files = config_files(root, cargo_home);
    let from_var = |name: &str| {
        var(name)
            .filter(|v| !v.is_empty())
            .map(|v| place(root, &v.to_string_lossy()))
    };
    let from_files = |key: &str| {
        files
            .iter()
            .find_map(|(base, table)| setting(table, key).map(|v| place(base, &v)))
    };
    from_var("CARGO_BUILD_BUILD_DIR")
        .or_else(|| from_files("build-dir"))
        .or_else(|| from_var("CARGO_TARGET_DIR"))
        .or_else(|| from_var("CARGO_BUILD_TARGET_DIR"))
        .or_else(|| from_files("target-dir"))
}

/// Each configuration file cargo reads in `root`, nearest first, with the directory its
/// relative paths are taken from. `$CARGO_HOME`'s comes last and only once.
fn config_files(root: &Path, cargo_home: Option<&Path>) -> Vec<(PathBuf, toml::Table)> {
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
        for name in ["config.toml", "config"] {
            let text = std::fs::read_to_string(dot_cargo.join(name));
            if let Some(table) = text.ok().and_then(|t| t.parse::<toml::Table>().ok()) {
                out.push((base.clone(), table));
                break;
            }
        }
    }
    out
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

/// `value` made absolute against `base`, unless it is already or carries a template.
fn place(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() || value.contains('{') { path } else { base.join(path) }
}

#[cfg(test)]
#[path = "build_dir_tests.rs"]
mod tests;

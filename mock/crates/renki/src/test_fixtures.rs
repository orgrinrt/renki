//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What the tests of more than one module install: a fixture crate with nothing to fetch, and
//! the walk that finds a `target` left behind.

use std::path::{Path, PathBuf};

/// A crate called `engine` with one binary and nothing to fetch, outside any workspace, at
/// `<at>/engine-src`.
pub(crate) fn fixture_engine(at: &Path) -> PathBuf {
    let src = at.join("engine-src");
    std::fs::create_dir_all(src.join("src")).unwrap();
    std::fs::write(
        src.join("Cargo.toml"),
        "[package]\nname = \"engine\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(src.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        src.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"engine\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    src
}

/// Every directory called `target` at or under `root`.
pub(crate) fn targets_under(root: &Path) -> impl Iterator<Item = PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if entry.file_name() == "target" {
                    found.push(path.clone());
                }
                stack.push(path);
            }
        }
    }
    found.into_iter()
}

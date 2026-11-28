//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The conformance suite under `posix/conformance/` as the tests read it: one
//! directory per fixture, its manifest's files, what reading it answers, and
//! the `context` it needs around it, one line each:
//!
//! - `env NAME=VALUE`: the variable is set, to the empty text where nothing
//!   follows the `=`. No other variable is set but `PATH`.
//! - `git [PATH...]`: the fixture's directory is a git work tree, with the
//!   paths named tracked.
//! - `no-git`: the reader runs with no `git` on its path.
//! - `git-fails`: the `git` on the reader's path fails to answer anything.
//! - `sh-on-path-fails`: the `sh` on the reader's path fails whatever it is
//!   asked, so only a reader running `/bin/sh` by its path answers.
//! - `base FILE`: the base is `FILE`, where it is not `app.toml`.
//! - `unlistable DIR`: the directory, `.` for the fixture's own, is one the
//!   reader may enter and not list.
//! - a line starting `#`, or a blank one, says nothing.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub const SUITE: &str = "../../../posix/conformance";

/// The files of a fixture that are about it and not part of its manifest.
pub const ABOUT: [&str; 3] = ["dump", "refused", "context"];

/// The suffix a file that is not TOML is kept under.
pub const NOT_TOML: &str = ".toml.in";

/// The base where a fixture's context names none.
pub const BASE: &str = "app.toml";

/// One fixture as the suite holds it.
pub struct Fixture {
    pub name:       String,
    pub dir:        PathBuf,
    pub dump:       Option<String>,
    pub refused:    Option<String>,
    pub env:        Vec<(String, String)>,
    pub tracked:    Option<Vec<String>>,
    pub no_git:     bool,
    pub git_fails:  bool,
    pub sh_fails:   bool,
    pub base:       String,
    pub unlistable: Vec<String>,
}

/// Whether the fixture at `dir` holds `file`, kept under its own name or, as
/// a file that is not TOML, under `<file>.in`; a link counts, whatever it
/// points at.
pub fn holds(dir: &Path, file: &str) -> bool {
    [file.to_owned(), format!("{file}.in")]
        .iter()
        .any(|f| dir.join(f).symlink_metadata().is_ok())
}

impl Fixture {
    pub fn read(dir: PathBuf) -> Self {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let text = |file: &str| fs::read_to_string(dir.join(file)).ok();
        let mut f = Fixture {
            dump:       text("dump"),
            refused:    text("refused"),
            name:       name.clone(),
            dir:        dir.clone(),
            env:        Vec::new(),
            tracked:    None,
            no_git:     false,
            git_fails:  false,
            sh_fails:   false,
            base:       BASE.to_owned(),
            unlistable: Vec::new(),
        };
        for line in text("context").unwrap_or_default().lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
            match word {
                "env" => {
                    let (var, value) = rest
                        .split_once('=')
                        .unwrap_or_else(|| panic!("{name}: `{line}` has no `=`"));
                    f.env.push((var.to_owned(), value.to_owned()));
                },
                "git" => f.tracked = Some(rest.split_whitespace().map(str::to_owned).collect()),
                "no-git" => f.no_git = true,
                "git-fails" => f.git_fails = true,
                "sh-on-path-fails" => f.sh_fails = true,
                "base" => rest.clone_into(&mut f.base),
                "unlistable" => f.unlistable.push(rest.to_owned()),
                _ => panic!("{name}: the context line `{line}` is none the suite knows"),
            }
        }
        assert!(
            !(f.no_git && f.git_fails),
            "{name}: no git and a git that fails cannot both be on the path"
        );
        f
    }

    /// The manifest's files, each by its name in the fixture once copied out,
    /// with its path in the suite; links and directories left out.
    pub fn files(&self) -> Vec<(String, PathBuf)> {
        let mut out = Vec::new();
        walk(&self.dir, "", true, &mut out);
        out.sort();
        out
    }
}

fn walk(dir: &Path, rel: &str, top: bool, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(dir).expect("a fixture's directory is listed") {
        let entry = entry.expect("a readable entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if top && ABOUT.contains(&name.as_str()) {
            continue;
        }
        let kind = entry.file_type().expect("a fixture's entry has a type");
        let copied = match name.strip_suffix(NOT_TOML) {
            Some(stem) => format!("{stem}.toml"),
            None => name,
        };
        let joined = if rel.is_empty() { copied } else { format!("{rel}/{copied}") };
        if kind.is_dir() {
            walk(&entry.path(), &joined, false, out);
        } else if kind.is_file() {
            out.push((joined, entry.path()));
        }
    }
}

/// Every fixture of the suite, in byte order of its name, each checked to pin
/// something.
pub fn suite() -> Vec<Fixture> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(SUITE)
        .expect("the suite is at posix/conformance")
        .map(|e| e.expect("a readable entry").path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let fixtures: Vec<Fixture> = dirs.into_iter().map(Fixture::read).collect();
    assert!(
        !fixtures.is_empty(),
        "the suite holds no fixture, so every reader passes it"
    );
    for f in &fixtures {
        assert!(
            f.dump.is_some() || f.refused.is_some(),
            "{} holds neither a dump nor a refused file, so it pins nothing",
            f.name
        );
        assert!(
            holds(&f.dir, &f.base),
            "{} holds no base {}",
            f.name,
            f.base
        );
        if let Some(refused) = &f.refused {
            assert!(
                refused.ends_with('\n') && refused.lines().count() == 1,
                "{}: a refusal is one line",
                f.name
            );
        }
        for path in f.tracked.iter().flatten() {
            assert!(
                holds(&f.dir, path),
                "{}: the context tracks {path}, which the fixture does not hold",
                f.name
            );
        }
        for dir in &f.unlistable {
            assert!(
                dir == "." || f.dir.join(dir).is_dir(),
                "{}: the context makes {dir} unlistable, which the fixture does not hold",
                f.name
            );
        }
    }
    fixtures
}

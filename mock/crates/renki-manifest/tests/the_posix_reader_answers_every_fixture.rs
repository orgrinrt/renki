//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The conformance suite under `posix/conformance/`, run through every reader
//! of manifests and compared with what each fixture holds, byte for byte.
//!
//! A fixture is a directory holding a manifest whose base is `app.toml`, its
//! layers and the files they include, and what reading it answers: a `dump`
//! file with the dump, a `refused` file with the refusal's line, or both where
//! the manifest is outside the POSIX subset, the dump for a reader of all of
//! TOML and the refusal for the POSIX reader. A `context` file says what the
//! fixture needs around it, one line each:
//!
//! - `env NAME=VALUE`: the variable is set, to the empty text where nothing
//!   follows the `=`. No other variable is set but `PATH`.
//! - `git [PATH...]`: the fixture's directory is a git work tree, with the
//!   paths named tracked.
//! - `no-git`: the reader runs with no `git` on its path.
//! - a line starting `#`, or a blank one, says nothing.
//!
//! Every fixture is copied into a directory of its own outside any work tree,
//! where the reader is run, so the repository's own git never answers for it.
//! Each reader is a [`Reader`] and the walk is the same for all of them; the
//! reading in Rust joins it as a second one once it exists.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SUITE: &str = "../../../posix/conformance";
const POSIX_READER: &str = "../../../posix/manifest.sh";

/// The files of a fixture that are about it and not part of its manifest.
const ABOUT: [&str; 3] = ["dump", "refused", "context"];

/// What a reader answered.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    /// A dump, or for `get` the dump's line, and nothing on the error stream.
    Read(String),
    /// A refusal on the error stream and nothing on the output.
    Refused(String),
    /// Anything else: the reader is not there, broke, or wrote to both.
    Broke(String),
}

/// A reader of manifests, run over every fixture by the same walk.
trait Reader {
    /// The name a failure is reported under.
    const NAME: &'static str;
    /// Whether this reader reads only the POSIX subset of TOML, and so answers
    /// a fixture holding both a dump and a refusal with the refusal.
    const SUBSET: bool;
    /// `dump` of the base `app.toml` of the fixture at `run`.
    fn dump(&self, run: &Run) -> Answer;
    /// `get` of `key` from the same manifest.
    fn get(&self, run: &Run, key: &str) -> Answer;
}

/// A fixture copied out and ready to read: its directory, the directory it
/// sits in, and the environment the reader gets.
struct Run {
    root: PathBuf,
    name: String,
    env:  Vec<(String, String)>,
}

/// One fixture as the suite holds it.
struct Fixture {
    name:    String,
    dir:     PathBuf,
    dump:    Option<String>,
    refused: Option<String>,
    env:     Vec<(String, String)>,
    tracked: Option<Vec<String>>,
    no_git:  bool,
}

impl Fixture {
    fn read(dir: PathBuf) -> Self {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let text = |file: &str| fs::read_to_string(dir.join(file)).ok();
        let (mut env, mut tracked, mut no_git) = (Vec::new(), None, false);
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
                    env.push((var.to_owned(), value.to_owned()));
                },
                "git" => tracked = Some(rest.split_whitespace().map(str::to_owned).collect()),
                "no-git" => no_git = true,
                _ => panic!("{name}: the context line `{line}` is none the suite knows"),
            }
        }
        Fixture {
            dump: text("dump"),
            refused: text("refused"),
            name,
            dir,
            env,
            tracked,
            no_git,
        }
    }

    /// What a reader is to answer: the refusal where it reads the subset
    /// and the fixture holds one, else the dump, else the refusal.
    fn expected<R: Reader>(&self) -> Answer {
        match (&self.dump, &self.refused) {
            (_, Some(refused)) if R::SUBSET => Answer::Refused(refused.clone()),
            (Some(dump), _) => Answer::Read(dump.clone()),
            (None, Some(refused)) => Answer::Refused(refused.clone()),
            (None, None) => unreachable!("checked when the suite was walked"),
        }
    }
}

/// Every fixture of the suite, in byte order of its name.
fn suite() -> Vec<Fixture> {
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
            f.dir.join("app.toml").is_file(),
            "{} holds no base app.toml",
            f.name
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
                f.dir.join(path).is_file(),
                "{}: the context tracks {path}, which the fixture does not hold",
                f.name
            );
        }
    }
    fixtures
}

/// The directory every fixture is copied into for one reader, emptied first.
fn scratch(reader: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "renki-manifest-conformance-{reader}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a scratch directory");
    root
}

fn copy_tree(from: &Path, to: &Path, top: bool) {
    fs::create_dir_all(to).expect("a fixture's directory is made");
    for entry in fs::read_dir(from).expect("a fixture's directory is listed") {
        let entry = entry.expect("a readable entry");
        let name = entry.file_name();
        if top && ABOUT.iter().any(|a| name == *a) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy_tree(&path, &to.join(&name), false);
        } else {
            fs::copy(&path, to.join(&name)).expect("a fixture's file is copied");
        }
    }
}

/// A directory of links to every program on this process's path but `git`,
/// for the fixtures that run with none.
fn path_without_git(root: &Path) -> String {
    let bin = root.join("path-without-git");
    if !bin.is_dir() {
        fs::create_dir_all(&bin).expect("a directory for the path");
        let path = std::env::var_os("PATH").expect("this process has a path");
        for dir in std::env::split_paths(&path) {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let link = bin.join(&name);
                if name == "git" || link.symlink_metadata().is_ok() {
                    continue;
                }
                let _ = std::os::unix::fs::symlink(entry.path(), link);
            }
        }
    }
    bin.to_string_lossy().into_owned()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs for a fixture that needs a work tree");
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The fixture copied under `root` with what its context asks for around it.
fn stand_up(fixture: &Fixture, root: &Path) -> Run {
    let dir = root.join(&fixture.name);
    copy_tree(&fixture.dir, &dir, true);
    if let Some(tracked) = &fixture.tracked {
        git(&dir, &["init", "--quiet"]);
        let mut add = vec!["add", "--"];
        add.extend(tracked.iter().map(String::as_str));
        if !tracked.is_empty() {
            git(&dir, &add);
        }
    }
    let path = if fixture.no_git {
        path_without_git(root)
    } else {
        std::env::var("PATH").expect("this process has a path")
    };
    let mut env = vec![("PATH".to_owned(), path)];
    env.extend(fixture.env.iter().cloned());
    Run {
        root: root.to_owned(),
        name: fixture.name.clone(),
        env,
    }
}

/// The scratch root lies in no work tree, or a fixture with no `git` line
/// would be read inside one and show nothing about reading outside one.
fn outside_every_work_tree(root: &Path) {
    if let Ok(out) = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output()
    {
        assert!(
            !out.status.success(),
            "{} is inside a git work tree, so the fixtures read outside one show nothing",
            root.display()
        );
    }
}

/// Every fixture through `reader`: its dump or its refusal, and for a dump,
/// `get` of each of its keys and of one it does not hold.
fn conforms<R: Reader>(reader: &R) {
    let fixtures = suite();
    let root = scratch(R::NAME);
    outside_every_work_tree(&root);
    let mut red: BTreeMap<String, String> = BTreeMap::new();
    for fixture in &fixtures {
        let run = stand_up(fixture, &root);
        let expected = fixture.expected::<R>();
        let got = reader.dump(&run);
        if got != expected {
            red.insert(
                fixture.name.clone(),
                format!("dump\n  expected {expected:?}\n  got      {got:?}"),
            );
            continue;
        }
        let Answer::Read(dump) = expected else {
            continue;
        };
        for line in dump.lines() {
            let key = line.split_once(" = ").map_or(line, |(k, _)| k);
            let got = reader.get(&run, key);
            let want = Answer::Read(format!("{line}\n"));
            if got != want {
                red.insert(
                    fixture.name.clone(),
                    format!("get {key}\n  expected {want:?}\n  got      {got:?}"),
                );
            }
        }
        let got = reader.get(&run, "no.key.of.the.suite");
        if got != Answer::Read(String::new()) {
            red.insert(
                fixture.name.clone(),
                format!("get of a key no layer holds answers nothing, and got {got:?}"),
            );
        }
    }
    let _ = fs::remove_dir_all(&root);
    assert!(
        red.is_empty(),
        "{} of {} fixtures are red for the {} reader:\n\n{}",
        red.len(),
        fixtures.len(),
        R::NAME,
        red.iter()
            .map(|(name, why)| format!("{name}: {why}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

/// `posix/manifest.sh`, sourced into `sh` and asked `manifest dump` and
/// `manifest get` from the directory above the fixture's, so nothing it
/// answers can lean on where it is run from.
struct Posix {
    library: PathBuf,
}

impl Posix {
    fn new() -> Self {
        let library = fs::canonicalize(".")
            .expect("the crate's directory")
            .join(POSIX_READER);
        Posix {
            library,
        }
    }

    fn ask(&self, run: &Run, args: &[&str]) -> Answer {
        let base = format!("{}/app.toml", run.name);
        let out = Command::new("sh")
            .arg("-c")
            .arg(r#". "$1" && shift && manifest "$@""#)
            .arg("sh")
            .arg(&self.library)
            .arg(args[0])
            .arg(&base)
            .args(&args[1 ..])
            .current_dir(&run.root)
            .env_clear()
            .envs(run.env.iter().map(|(k, v)| (k, v)))
            .output()
            .expect("sh runs");
        let (stdout, stderr) = (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        );
        match out.status.code() {
            Some(0) if stderr.is_empty() => Answer::Read(stdout),
            Some(1) if stdout.is_empty() && !stderr.is_empty() => Answer::Refused(stderr),
            Some(1) if stdout.is_empty() && stderr.is_empty() && args[0] == "get" => {
                Answer::Read(String::new())
            },
            code => {
                Answer::Broke(format!(
                    "exit {code:?}, output {stdout:?}, error {stderr:?}"
                ))
            },
        }
    }
}

impl Reader for Posix {
    const NAME: &'static str = "posix";
    const SUBSET: bool = true;

    fn dump(&self, run: &Run) -> Answer {
        self.ask(run, &["dump"])
    }

    fn get(&self, run: &Run, key: &str) -> Answer {
        self.ask(run, &["get", key])
    }
}

#[test]
fn the_posix_reader_answers_every_fixture() {
    conforms(&Posix::new());
}

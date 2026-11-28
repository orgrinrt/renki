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
//! - `git-fails`: the `git` on the reader's path fails to answer anything.
//! - a line starting `#`, or a blank one, says nothing.
//!
//! A file of a fixture that is not TOML is kept as `<name>.toml.in` and
//! copied to `<name>.toml`, so the repository's TOML lint never reads it; so
//! is a link that leads nowhere and a directory named like a layer. A link in
//! a fixture is copied as a link.
//!
//! Every fixture is copied into a directory of its own outside any work tree,
//! where the reader is run, so the repository's own git never answers for it.
//! The suite runs twice, once from a directory whose name holds a space and a
//! quote, which no answer may notice. Each reader is a [`Reader`] and the walk
//! is the same for all of them; the reading in Rust joins it as a second one
//! once it exists. A copy of the POSIX reader broken on purpose in one line is
//! run over the suite too, and must turn exactly the fixtures that pin that
//! line red, which shows the suite can fail.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SUITE: &str = "../../../posix/conformance";
const POSIX_READER: &str = "../../../posix/manifest.sh";
const BROKEN_ON_PURPOSE: &str = "tests/posix/manifest.broken-on-purpose.sh";

/// The fixtures the copy broken on purpose turns red: it no longer takes
/// `yes` for true.
const RED_WHEN_BROKEN: [&str; 2] = ["env_truthy_in_any_case", "env_truthy_yes"];

/// The files of a fixture that are about it and not part of its manifest.
const ABOUT: [&str; 3] = ["dump", "refused", "context"];

/// The suffix a file that is not TOML is kept under.
const NOT_TOML: &str = ".toml.in";

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
    name:      String,
    dir:       PathBuf,
    dump:      Option<String>,
    refused:   Option<String>,
    env:       Vec<(String, String)>,
    tracked:   Option<Vec<String>>,
    no_git:    bool,
    git_fails: bool,
}

/// Whether the fixture at `dir` holds `file`, kept under its own name or, as
/// a file that is not TOML, under `<file>.in`; a link counts, whatever it
/// points at.
fn holds(dir: &Path, file: &str) -> bool {
    [file.to_owned(), format!("{file}.in")]
        .iter()
        .any(|f| dir.join(f).symlink_metadata().is_ok())
}

impl Fixture {
    fn read(dir: PathBuf) -> Self {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let text = |file: &str| fs::read_to_string(dir.join(file)).ok();
        let (mut env, mut tracked, mut no_git, mut git_fails) = (Vec::new(), None, false, false);
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
                "git-fails" => git_fails = true,
                _ => panic!("{name}: the context line `{line}` is none the suite knows"),
            }
        }
        assert!(
            !(no_git && git_fails),
            "{name}: no git and a git that fails cannot both be on the path"
        );
        Fixture {
            dump: text("dump"),
            refused: text("refused"),
            name,
            dir,
            env,
            tracked,
            no_git,
            git_fails,
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
            holds(&f.dir, "app.toml"),
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
                holds(&f.dir, path),
                "{}: the context tracks {path}, which the fixture does not hold",
                f.name
            );
        }
    }
    fixtures
}

/// The directory every fixture is copied into for one run, emptied first.
fn scratch(run: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "renki-manifest-conformance-{run}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a scratch directory");
    root
}

/// A fixture's tree copied, each link as a link and each `<name>.toml.in` to
/// `<name>.toml`.
fn copy_tree(from: &Path, to: &Path, top: bool) {
    fs::create_dir_all(to).expect("a fixture's directory is made");
    for entry in fs::read_dir(from).expect("a fixture's directory is listed") {
        let entry = entry.expect("a readable entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if top && ABOUT.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().expect("a fixture's entry has a type");
        let copied = to.join(match name.strip_suffix(NOT_TOML) {
            Some(stem) => format!("{stem}.toml"),
            None => name,
        });
        if kind.is_symlink() {
            let target = fs::read_link(&path).expect("a fixture's link is read");
            std::os::unix::fs::symlink(target, copied).expect("a fixture's link is made");
        } else if kind.is_dir() {
            copy_tree(&path, &copied, false);
        } else {
            fs::copy(&path, copied).expect("a fixture's file is copied");
        }
    }
}

/// A directory holding a `git` that fails to answer anything, for the
/// fixtures that run with one.
fn failing_git(root: &Path) -> PathBuf {
    let bin = root.join("path-with-a-failing-git");
    if !bin.is_dir() {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(&bin).expect("a directory for the path");
        let git = bin.join("git");
        fs::write(&git, "#!/bin/sh\necho 'git: cannot answer' >&2\nexit 128\n")
            .expect("a git that fails is written");
        fs::set_permissions(&git, fs::Permissions::from_mode(0o755))
            .expect("the git that fails runs");
    }
    bin
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

/// The fixture copied under `root` as `name` with what its context asks for
/// around it.
fn stand_up(fixture: &Fixture, root: &Path, name: &str) -> Run {
    let dir = root.join(name);
    copy_tree(&fixture.dir, &dir, true);
    if let Some(tracked) = &fixture.tracked {
        git(&dir, &["init", "--quiet"]);
        let mut add = vec!["add", "--"];
        add.extend(tracked.iter().map(String::as_str));
        if !tracked.is_empty() {
            git(&dir, &add);
        }
    }
    let here = std::env::var("PATH").expect("this process has a path");
    let path = if fixture.no_git {
        path_without_git(root)
    } else if fixture.git_fails {
        format!("{}:{here}", failing_git(root).to_string_lossy())
    } else {
        here
    };
    let mut env = vec![("PATH".to_owned(), path)];
    env.extend(fixture.env.iter().cloned());
    Run {
        root: root.to_owned(),
        name: name.to_owned(),
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

/// Every fixture through `reader`, each copied out as its name followed by
/// `suffix`: its dump or its refusal, and for a dump, `get` of each of its
/// keys and of one it does not hold. Answers why each red fixture is red, and
/// how many fixtures there are.
fn red<R: Reader>(reader: &R, tag: &str, suffix: &str) -> (BTreeMap<String, String>, usize) {
    let fixtures = suite();
    let root = scratch(&format!("{}-{tag}", R::NAME));
    outside_every_work_tree(&root);
    let mut red: BTreeMap<String, String> = BTreeMap::new();
    for fixture in &fixtures {
        let run = stand_up(fixture, &root, &format!("{}{suffix}", fixture.name));
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
    (red, fixtures.len())
}

/// Every fixture is green for `reader`, read from a directory named as the
/// fixture followed by `suffix`.
fn conforms<R: Reader>(reader: &R, tag: &str, suffix: &str) {
    let (red, of) = red(reader, tag, suffix);
    assert!(
        red.is_empty(),
        "{} of {of} fixtures are red for the {} reader:\n\n{}",
        red.len(),
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
    /// The reader at `path`, relative to the crate's directory.
    fn at(path: &str) -> Self {
        let library = fs::canonicalize(".")
            .expect("the crate's directory")
            .join(path);
        Posix {
            library,
        }
    }

    fn new() -> Self {
        Self::at(POSIX_READER)
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

/// A manifest written under `root` at `dir`, a directory name that may hold
/// any byte, as `(path, text)` pairs relative to it, ready to read.
fn written(root: &Path, dir: &str, files: &[(&str, &str)]) -> Run {
    for (path, text) in files {
        let at = root.join(dir).join(path);
        fs::create_dir_all(at.parent().unwrap()).expect("a directory for the file");
        fs::write(at, text).expect("a file of the manifest is written");
    }
    Run {
        root: root.to_owned(),
        name: dir.to_owned(),
        env:  vec![(
            "PATH".to_owned(),
            std::env::var("PATH").expect("this process has a path"),
        )],
    }
}

/// A path holding a newline is refused wherever it comes from, since no line
/// of a refusal or a dump can name it: the base's directory, and a name an
/// include's glob matches. An include entry holding one is a fixture.
fn refuses_a_newline_in_a_path<R: Reader>(reader: &R) {
    let root = scratch(&format!("{}-newline", R::NAME));
    outside_every_work_tree(&root);
    let cases = [
        (
            written(&root, "in\nside", &[
                ("app.toml", "a = 1\n"),
                ("app.local.toml", "a = 2\n"),
            ]),
            "app.toml: its path holds a newline, which no line can name\n",
        ),
        (
            written(&root, "a_glob", &[
                ("app.toml", "meta.overlays.include = [\"conf/*\"]\n"),
                ("conf/odd\nname", "a = 2\n"),
            ]),
            "app.toml:1: meta.overlays.include matches a path holding a newline, which no line \
             can name\n",
        ),
        (
            written(&root, "a_link", &[
                ("app.toml", "a = 1\n"),
                ("odd\nplace/app.local.toml", "a = 2\n"),
            ]),
            "app.local.toml: its path holds a newline, which no line can name\n",
        ),
        (
            written(&root, "a_name", &[
                ("app.toml", "a = 1\n"),
                ("app.odd\nword.toml", "a = 2\n"),
            ]),
            "app.toml: its directory holds a layer whose name holds a newline, which no line can \
             name\n",
        ),
    ];
    std::os::unix::fs::symlink(
        "odd\nplace/app.local.toml",
        root.join("a_link/app.local.toml"),
    )
    .expect("a link whose target holds a newline");
    for (run, refusal) in &cases {
        assert_eq!(
            reader.dump(run),
            Answer::Refused((*refusal).to_owned()),
            "{} reading {:?}",
            R::NAME,
            run.name
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_posix_reader_answers_every_fixture() {
    conforms(&Posix::new(), "plain", "");
}

#[test]
fn the_posix_reader_answers_every_fixture_from_a_directory_with_a_space_and_a_quote() {
    conforms(&Posix::new(), "spaced", " it's here");
}

#[test]
fn the_posix_reader_refuses_a_newline_in_a_path() {
    refuses_a_newline_in_a_path(&Posix::new());
}

/// The copy broken on purpose differs from the reader in one line, so it is
/// the reader with one fault and not a reader of its own, and the suite turns
/// red over it for exactly the fixtures that pin that line.
#[test]
fn the_suite_turns_red_over_a_reader_broken_on_purpose() {
    let reader = fs::read_to_string(POSIX_READER).expect("the reader is read");
    let broken = fs::read_to_string(BROKEN_ON_PURPOSE).expect("the broken copy is read");
    let (a, b): (Vec<&str>, Vec<&str>) = (reader.lines().collect(), broken.lines().collect());
    let differ = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert!(
        a.len() == b.len() && differ == 1,
        "{BROKEN_ON_PURPOSE} is to be {POSIX_READER} with one line changed; it has {} lines \
         against {} and {differ} differ",
        b.len(),
        a.len()
    );
    let (red, _) = red(&Posix::at(BROKEN_ON_PURPOSE), "broken", "");
    let names: Vec<&str> = red.keys().map(String::as_str).collect();
    assert_eq!(
        names, RED_WHEN_BROKEN,
        "the suite over the copy broken on purpose"
    );
}

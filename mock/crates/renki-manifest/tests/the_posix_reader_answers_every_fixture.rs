//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The conformance suite under `posix/conformance/`, run through every reader
//! of manifests and compared with what each fixture holds, byte for byte.
//!
//! A fixture is a directory holding a manifest, its layers and the files they
//! include, and what reading it answers: a `dump` file with the dump, a
//! `refused` file with the refusal's line, or both where the manifest is
//! outside the POSIX subset, the dump for a reader of all of TOML and the
//! refusal for the POSIX reader. Its `context` file says what it needs around
//! it; `tests/common/mod.rs` names the lines.
//!
//! A file of a fixture that is not TOML is kept as `<name>.toml.in` and
//! copied to `<name>.toml`, so the repository's TOML lint never reads it; so
//! is a link that leads nowhere and a directory named like a layer. A link in
//! a fixture is copied as a link.
//!
//! Every fixture is copied into a directory of its own outside any work tree,
//! where the reader is run, so the repository's own git never answers for it.
//! The suite runs twice, once from a directory whose name holds a space and a
//! quote, which no answer may notice. The POSIX reader is run under every
//! shell of `dash` and `bash` and every `awk` of `mawk`, `gawk`, the one true
//! `awk` and `busybox awk` that this host has. Each reader is a [`Reader`]
//! and the walk is the same for all of them; the reading in Rust joins it as
//! a second one once it exists. A copy of the POSIX reader broken on purpose
//! in one line is run over the suite too, and must turn exactly the fixtures
//! that pin that line red, which shows the suite can fail.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{ABOUT, Fixture, NOT_TOML, suite};

const POSIX_READER: &str = "../../../posix/manifest.sh";
const BROKEN_ON_PURPOSE: &str = "tests/posix/manifest.broken-on-purpose.sh";

/// The fixtures the copy broken on purpose turns red: it no longer takes
/// `yes` for true.
const RED_WHEN_BROKEN: [&str; 2] = ["env_truthy_in_any_case", "env_truthy_yes"];

/// The shells the reader is sourced into, each where this host has it.
const SHELLS: [&str; 2] = ["dash", "bash"];

/// The `awk`s the reader runs under, each where this host has it: `busybox`
/// answers as its `awk` when called by that name, and `awk` and `nawk` are
/// whatever the path names, as macOS and the BSDs ship theirs; a program met
/// under two names is run once.
const AWKS: [&str; 6] = ["mawk", "gawk", "original-awk", "busybox", "awk", "nawk"];

/// Variables a shell reads itself, which no `env` predicate of a fixture may
/// name: the shell would answer one before the reader does.
const READ_BY_A_SHELL: [&str; 30] = [
    "LC_ALL",
    "LC_CTYPE",
    "LC_COLLATE",
    "LC_MESSAGES",
    "LC_NUMERIC",
    "LC_TIME",
    "LANG",
    "LANGUAGE",
    "NLSPATH",
    "PATH",
    "IFS",
    "ENV",
    "BASH_ENV",
    "HOME",
    "CDPATH",
    "PWD",
    "OLDPWD",
    "SHELLOPTS",
    "BASHOPTS",
    "POSIXLY_CORRECT",
    "PS1",
    "PS2",
    "PS4",
    "OPTIND",
    "OPTARG",
    "MAIL",
    "MAILPATH",
    "HISTFILE",
    "TMOUT",
    "SHELL",
];

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
    /// Whether this reader reads only the POSIX subset of TOML, and so answers
    /// a fixture holding both a dump and a refusal with the refusal.
    const SUBSET: bool;
    /// The name a failure is reported under.
    fn label(&self) -> String;
    /// `dump` of the base of the fixture at `run`.
    fn dump(&self, run: &Run) -> Answer;
    /// `get` of `key` from the same manifest.
    fn get(&self, run: &Run, key: &str) -> Answer;
}

/// A fixture copied out and ready to read: the directory it sits in, its own
/// directory's name there, its base, the environment the reader gets, and
/// the directories it may enter and not list.
struct Run {
    root:   PathBuf,
    name:   String,
    base:   String,
    env:    Vec<(String, String)>,
    locked: Vec<(PathBuf, u32)>,
}

impl Fixture {
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

/// A directory under `root` named `name` holding one program, `program`,
/// which writes `say` on its error stream and exits 128 whatever it is asked.
fn failing(root: &Path, name: &str, program: &str) -> PathBuf {
    let bin = root.join(name);
    if !bin.is_dir() {
        fs::create_dir_all(&bin).expect("a directory for the path");
        let at = bin.join(program);
        fs::write(
            &at,
            format!("#!/bin/sh\necho '{program}: cannot answer' >&2\nexit 128\n"),
        )
        .expect("a program that fails is written");
        fs::set_permissions(&at, fs::Permissions::from_mode(0o755))
            .expect("the program that fails runs");
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
    let mut path = if fixture.no_git {
        path_without_git(root)
    } else if fixture.git_fails {
        format!(
            "{}:{here}",
            failing(root, "path-with-a-failing-git", "git").to_string_lossy()
        )
    } else {
        here
    };
    if fixture.sh_fails {
        path = format!(
            "{}:{path}",
            failing(root, "path-with-a-failing-sh", "sh").to_string_lossy()
        );
    }
    // a directory the reader may enter and not list, and a file it may find
    // and not read, each with the permission it is given back
    let mut locked: Vec<(PathBuf, u32)> = Vec::new();
    for (at, mode, back) in fixture
        .unlistable
        .iter()
        .map(|d| (dir.join(d), 0o300, 0o755))
        .chain(
            fixture
                .unreadable
                .iter()
                .map(|f| (dir.join(f), 0o000, 0o644)),
        )
    {
        fs::set_permissions(&at, fs::Permissions::from_mode(mode))
            .expect("a permission the test takes away");
        locked.push((at, back));
    }
    let mut env = vec![("PATH".to_owned(), path)];
    env.extend(fixture.env.iter().cloned());
    Run {
        root: root.to_owned(),
        name: name.to_owned(),
        base: fixture.base.clone(),
        env,
        locked,
    }
}

/// What a run took away, given back.
fn unlock(run: &Run) {
    for (path, back) in &run.locked {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(*back));
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
    let root = scratch(tag);
    outside_every_work_tree(&root);
    let mut red: BTreeMap<String, String> = BTreeMap::new();
    for fixture in &fixtures {
        let run = stand_up(fixture, &root, &format!("{}{suffix}", fixture.name));
        let why = check(reader, fixture, &run);
        unlock(&run);
        if let Some(why) = why {
            red.insert(fixture.name.clone(), why);
        }
    }
    let _ = fs::remove_dir_all(&root);
    (red, fixtures.len())
}

/// Why `fixture` is red for `reader`, where it is.
fn check<R: Reader>(reader: &R, fixture: &Fixture, run: &Run) -> Option<String> {
    let expected = fixture.expected::<R>();
    let got = reader.dump(run);
    if got != expected {
        return Some(format!("dump\n  expected {expected:?}\n  got      {got:?}"));
    }
    let Answer::Read(dump) = expected else {
        return None;
    };
    for line in dump.lines() {
        let key = line.split_once(" = ").map_or(line, |(k, _)| k);
        let got = reader.get(run, key);
        let want = Answer::Read(format!("{line}\n"));
        if got != want {
            return Some(format!(
                "get {key}\n  expected {want:?}\n  got      {got:?}"
            ));
        }
    }
    let got = reader.get(run, "no.key.of.the.suite");
    (got != Answer::Read(String::new()))
        .then(|| format!("get of a key no layer holds answers nothing, and got {got:?}"))
}

/// Every fixture is green for every reader of `readers`, read from a
/// directory named as the fixture followed by `suffix`.
fn conforms(readers: &[Posix], tag: &str, suffix: &str) {
    let mut failed = Vec::new();
    for (i, reader) in readers.iter().enumerate() {
        println!("reading the suite under {}", reader.label());
        let (red, of) = red(reader, &format!("{tag}-{i}"), suffix);
        if !red.is_empty() {
            failed.push(format!(
                "{} of {of} fixtures are red for the reader under {}:\n\n{}",
                red.len(),
                reader.label(),
                red.iter()
                    .map(|(name, why)| format!("{name}: {why}"))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n\n"));
}

/// Where `program` is on this process's path, its links followed.
fn on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(program))
        .find(|p| p.is_file())
        .and_then(|p| fs::canonicalize(p).ok())
}

/// The shells of [`SHELLS`] this host has; one at least.
fn shells() -> Vec<(String, PathBuf)> {
    let found: Vec<(String, PathBuf)> = SHELLS
        .iter()
        .filter_map(|s| on_path(s).map(|p| ((*s).to_owned(), p)))
        .collect();
    assert!(!found.is_empty(), "this host has none of {SHELLS:?}");
    found
}

/// A directory per `awk` of [`AWKS`] this host has, each holding it as
/// `awk`, to put first on a reader's path; one at least.
fn awks() -> Vec<(String, PathBuf)> {
    let dir = std::env::temp_dir().join(format!("renki-manifest-awks-{}", std::process::id()));
    let mut seen = Vec::new();
    let mut found = Vec::new();
    for name in AWKS {
        let Some(program) = on_path(name) else {
            continue;
        };
        // `nawk` and `awk` are often links to one of the others
        if seen.contains(&program) {
            continue;
        }
        seen.push(program.clone());
        let bin = dir.join(name);
        fs::create_dir_all(&bin).expect("a directory for an awk");
        let link = bin.join("awk");
        // the tests of this binary run at once and make the same links
        match std::os::unix::fs::symlink(&program, &link) {
            Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
                panic!("an awk is linked: {e}")
            },
            _ => {},
        }
        found.push((format!("{name} at {}", program.display()), bin));
    }
    assert!(!found.is_empty(), "this host has none of {AWKS:?}");
    found
}

/// Whether this process may read whatever its permissions say, so a
/// directory it may not list has to be taken away from it in a namespace.
fn is_the_superuser() -> bool {
    // SAFETY: `geteuid` takes nothing and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

/// `posix/manifest.sh`, sourced into `shell` with `awk` first on the path,
/// and asked `manifest dump` and `manifest get` from the directory above the
/// fixture's, so nothing it answers can lean on where it is run from.
struct Posix {
    library: PathBuf,
    shell:   (String, PathBuf),
    awk:     (String, PathBuf),
}

impl Posix {
    /// The reader at `path`, relative to the crate's directory.
    fn at(path: &str, shell: (String, PathBuf), awk: (String, PathBuf)) -> Self {
        let library = fs::canonicalize(".")
            .expect("the crate's directory")
            .join(path);
        Posix {
            library,
            shell,
            awk,
        }
    }

    /// The reader under every shell and `awk` this host has.
    fn every() -> Vec<Self> {
        let awks = awks();
        shells()
            .into_iter()
            .flat_map(|s| {
                awks.iter()
                    .map(move |a| Self::at(POSIX_READER, s.clone(), a.clone()))
            })
            .collect()
    }

    /// The reader under the first shell and the first `awk` this host has.
    fn first(path: &str) -> Self {
        Self::at(path, shells().remove(0), awks().remove(0))
    }

    fn ask(&self, run: &Run, args: &[&str]) -> Answer {
        let base = format!("{}/{}", run.name, run.base);
        // a directory the superuser may not list is one in a namespace where
        // it is no superuser
        let mut cmd = if !run.locked.is_empty() && is_the_superuser() {
            let mut c = Command::new("unshare");
            c.arg("-U").arg(&self.shell.1);
            c
        } else {
            Command::new(&self.shell.1)
        };
        let env = run.env.iter().map(|(k, v)| {
            if k == "PATH" {
                (k.clone(), format!("{}:{v}", self.awk.1.to_string_lossy()))
            } else {
                (k.clone(), v.clone())
            }
        });
        let out = cmd
            .arg("-c")
            .arg(r#". "$1" && shift && manifest "$@""#)
            .arg("sh")
            .arg(&self.library)
            .arg(args[0])
            .arg(&base)
            .args(&args[1 ..])
            .current_dir(&run.root)
            .env_clear()
            .envs(env)
            .output()
            .expect("the shell runs");
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
    const SUBSET: bool = true;

    fn label(&self) -> String {
        format!("{} and {}", self.shell.0, self.awk.0)
    }

    fn dump(&self, run: &Run) -> Answer {
        self.ask(run, &["dump"])
    }

    fn get(&self, run: &Run, key: &str) -> Answer {
        self.ask(run, &["get", key])
    }
}

/// A manifest written under `root` at `dir`, a directory name that may hold
/// any byte, as `(path, text)` pairs relative to it, its base the first,
/// ready to read.
fn written(root: &Path, dir: &str, files: &[(&str, &str)]) -> Run {
    for (path, text) in files {
        let at = root.join(dir).join(path);
        fs::create_dir_all(at.parent().unwrap()).expect("a directory for the file");
        fs::write(at, text).expect("a file of the manifest is written");
    }
    Run {
        root:   root.to_owned(),
        name:   dir.to_owned(),
        base:   files[0].0.to_owned(),
        env:    vec![(
            "PATH".to_owned(),
            std::env::var("PATH").expect("this process has a path"),
        )],
        locked: Vec::new(),
    }
}

/// A path holding a newline is refused wherever it comes from, since no line
/// of a refusal or a dump can name it: the base's directory and its own name,
/// a link's target, a layer's name, and a name an include's glob matches. An
/// include entry holding one is a fixture.
fn refuses_a_newline_in_a_path(reader: &Posix, tag: &str) {
    let root = scratch(&format!("newline-{tag}"));
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
            written(&root, "plain", &[("ap\np.toml", "a = 1\n")]),
            "the base: its path holds a newline, which no line can name\n",
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
            reader.label(),
            run.name
        );
    }
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_posix_reader_answers_every_fixture_under_every_shell_and_awk() {
    conforms(&Posix::every(), "plain", "");
}

#[test]
fn the_posix_reader_answers_every_fixture_from_a_directory_with_a_space_and_a_quote() {
    conforms(&Posix::every(), "spaced", " it's here");
}

#[test]
fn the_posix_reader_refuses_a_newline_in_a_path() {
    for (i, reader) in Posix::every().iter().enumerate() {
        refuses_a_newline_in_a_path(reader, &i.to_string());
    }
}

/// The variable an `env` predicate of one of `files` names, each by its text
/// after `env = "`.
fn env_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l
                .strip_prefix("env")?
                .trim_start()
                .strip_prefix('=')?
                .trim_start();
            Some(rest.strip_prefix('"')?.split('"').next()?.to_owned())
        })
        .chain(text.lines().filter_map(|l| {
            let at = l.find(".env = \"")?;
            Some(l[at + 8 ..].split('"').next()?.to_owned())
        }))
        .collect()
}

/// The one variable a shell reads that a fixture's `env` predicate may name,
/// with the one value it may hold: `POSIX`, which every shell takes.
const SHELLS_TAKE: (&str, &str) = ("LC_ALL", "POSIX");

#[test]
fn no_env_predicate_of_a_fixture_names_a_variable_a_shell_reads() {
    let mut named = Vec::new();
    let mut excepted = 0;
    for fixture in suite() {
        let set_to = |var: &str| {
            fixture
                .env
                .iter()
                .find(|(k, _)| k == var)
                .map(|(_, v)| v.as_str())
        };
        for (file, path) in fixture.files() {
            let text = fs::read_to_string(&path).unwrap_or_default();
            for var in env_names(&text) {
                if !READ_BY_A_SHELL.contains(&var.as_str()) {
                    continue;
                }
                if var == SHELLS_TAKE.0 && set_to(&var) == Some(SHELLS_TAKE.1) {
                    excepted += 1;
                    continue;
                }
                named.push(format!("{}/{file}: {var}", fixture.name));
            }
        }
    }
    assert!(named.is_empty(), "{named:?}");
    assert_eq!(excepted, 1, "the one fixture over LC_ALL set to POSIX");
    // the control: the scan finds what it looks for
    assert_eq!(
        env_names("[meta.predicates.a]\nenv = \"LC_ALL\"\nmeta.predicates.b.env = \"HOME\"\n"),
        ["LC_ALL", "HOME"]
    );
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
    let (red, _) = red(&Posix::first(BROKEN_ON_PURPOSE), "broken", "");
    let names: Vec<&str> = red.keys().map(String::as_str).collect();
    assert_eq!(
        names, RED_WHEN_BROKEN,
        "the suite over the copy broken on purpose"
    );
}

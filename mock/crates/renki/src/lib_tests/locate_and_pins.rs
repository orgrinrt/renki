//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `locate` answer, the engine flag, and the pin messages. Split from the
//! crate's tests by size; the fixture is the parent module's.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use super::*;

/// Arguments, as the launcher receives them: bytes, not text.
fn s(v: &[&str]) -> Vec<std::ffi::OsString> {
    v.iter().map(|x| std::ffi::OsString::from(*x)).collect()
}

/// The answer as text, for a case whose paths are all valid UTF-8. Every
/// assertion that is about the key names rather than about the bytes reads
/// better this way, and the byte-level cases below call `locate_answer`
/// directly.
fn answer(locate: &Locate, root: &Path, config: &str, workdir: &Path) -> String {
    let bytes = locate_answer(locate, root, Path::new(config), workdir).expect("refused");
    String::from_utf8(bytes).expect("the fixture paths are all text")
}

#[test]
fn the_locate_answer_uses_the_tools_own_key_names() {
    // All three were hardcoded here while `Locate` documented them as "a
    // contract with those callers", so a tool that set them got the
    // conventional spellings anyway and its own shell helpers, parsing the
    // names it had chosen, parsed nothing at all.
    const OWN: Locate = Locate {
        subcommand:  "locate",
        root_key:    "repo",
        config_key:  "manifest",
        workdir_key: "work_dir",
    };
    let d = tempfile::tempdir().unwrap();
    let wd = d.path().join("work");
    std::fs::create_dir_all(&wd).unwrap();

    let got = answer(&OWN, d.path(), "/c/x.toml", &wd);
    assert_eq!(
        got,
        format!(
            "repo={}\nmanifest=/c/x.toml\nwork_dir={}\n",
            d.path().display(),
            wd.display()
        )
    );
    // and the control: the conventional names are not what came out, so
    // this cannot be passing against a formatter that ignores its argument
    assert!(!got.contains("root="), "{got}");
    assert!(!got.contains("config="), "{got}");
    assert!(!got.contains("workdir="), "{got}");

    // the default still answers conventionally
    let d2 = answer(&Locate::DEFAULT, d.path(), "/c/x.toml", &wd);
    assert!(d2.starts_with("root="), "{d2}");
    assert!(d2.contains("\nconfig=/c/x.toml\n"), "{d2}");
    assert!(
        d2.contains(&format!("\nworkdir={}\n", wd.display())),
        "{d2}"
    );
}

#[test]
fn a_missing_workdir_answers_with_the_key_and_no_value() {
    // The line stays, so a reader can tell an absent directory from a
    // launcher too old to answer at all.
    let d = tempfile::tempdir().unwrap();
    let absent = d.path().join("nothing-here");
    let got = answer(&Locate::DEFAULT, d.path(), "", &absent);
    assert!(got.ends_with("workdir=\n"), "{got}");
    assert!(got.contains("\nconfig=\n"), "{got}");
}

/// The one record whose key is `key`, as bytes, or `None`.
fn record<'a>(answer: &'a [u8], key: &str) -> Option<&'a [u8]> {
    let mut want = key.as_bytes().to_vec();
    want.push(b'=');
    answer
        .split(|b| *b == b'\n')
        .find(|line| line.starts_with(&want))
        .map(|line| &line[want.len() ..])
}

#[test]
fn the_answer_carries_the_paths_bytes_rather_than_their_display() {
    // `Display` on a path replaces whatever is not text with U+FFFD, and a
    // reader `cd`ing into that gets a directory that does not exist.
    let root = PathBuf::from(OsString::from_vec(b"/r/\xff\xfe".to_vec()));
    let config = PathBuf::from(OsString::from_vec(b"/r/\xff\xfe/w.toml".to_vec()));
    let workdir = PathBuf::from(OsString::from_vec(b"/r/\xff\xfe/work".to_vec()));
    let got = locate_answer(&Locate::DEFAULT, &root, &config, &workdir).expect("refused");

    assert_eq!(record(&got, "root"), Some(&b"/r/\xff\xfe"[..]));
    assert_eq!(record(&got, "config"), Some(&b"/r/\xff\xfe/w.toml"[..]));
    // The working directory is absent on disk here, so it answers empty,
    // which is the documented shape rather than a byte question.
    assert_eq!(record(&got, "workdir"), Some(&b""[..]));

    // and the control, which is the whole point: the lossy rendering of
    // those bytes is not what came out. Without it this passes against a
    // writer that renders and happens to be compared against a rendering.
    let lossy = root.display().to_string();
    assert_ne!(lossy.as_bytes(), b"/r/\xff\xfe");
    assert_ne!(record(&got, "root"), Some(lossy.as_bytes()));
}

#[test]
fn a_path_holding_a_newline_is_refused_by_name_rather_than_answered_wrongly() {
    // One record per line is the whole format. A newline inside a value
    // makes a reader see two records, the second with no `=` in it, and
    // nothing about the answer says so. A newline is legal in a path on
    // every unix, so this is reachable rather than theoretical.
    let d = tempfile::tempdir().unwrap();
    let bad = d.path().join("two\nlines");
    std::fs::create_dir_all(&bad).unwrap();
    let plain = d.path().join("one-line");
    std::fs::create_dir_all(&plain).unwrap();

    // Every value goes through the same writer, so every value refuses, and
    // each refusal names the key whose path was the problem.
    for (key, root, config, workdir) in [
        ("root", &bad, &plain, &plain),
        ("config", &plain, &bad, &plain),
        ("workdir", &plain, &plain, &bad),
    ] {
        let e = locate_answer(&Locate::DEFAULT, root, config, workdir)
            .expect_err("a newline was answered rather than refused");
        assert!(e.contains(key), "the refusal does not name {key}: {e}");
        assert!(e.contains("newline"), "{e}");
    }

    // and the control: the same three paths without the newline are
    // answered, so the refusal is about the byte rather than a writer that
    // refuses everything.
    locate_answer(&Locate::DEFAULT, &plain, &plain, &plain).expect("a plain path was refused");
}

#[test]
fn an_engine_flag_with_no_path_is_refused_rather_than_running_the_pinned_engine() {
    // Before this, the flag came back as a bare `None`, which is what an
    // absent flag also came back as, so the run fell through to the pinned
    // engine. That is the opposite of what was asked and it is silent: the
    // override path prints `ENGINE OVERRIDE` and this path printed nothing.
    //
    // The refusal is the first thing `dispatch` does, so this reaches it
    // without any discovery, build or exec.
    let e = dispatch(&T, &s(&["--engine"])).unwrap_err();
    assert!(
        e.contains("--engine") && e.contains("none followed it"),
        "the refusal does not name the flag or say what was missing: {e}"
    );

    let e = dispatch(&T, &s(&["--engine", "--verbose"])).unwrap_err();
    assert!(e.contains("--engine"), "{e}");

    // and it names the tool's own flag, not the conventional spelling
    const OTHER: Tool = Tool {
        engine_flag: "--with",
        ..T
    };
    let e = dispatch(&OTHER, &s(&["--with"])).unwrap_err();
    assert!(e.contains("--with"), "{e}");
    assert!(!e.contains("--engine"), "{e}");
}

#[test]
fn the_missing_pin_message_names_the_tools_own_key() {
    let d = tempfile::tempdir().unwrap();
    let err = resolve_pin(&T, None, d.path(), d.path()).unwrap_err();
    assert!(err.contains("t_version"), "{err}");
    assert!(err.contains("t.toml"), "{err}");
}

#[test]
fn a_config_that_is_not_toml_is_reported_as_that_and_not_as_a_missing_pin() {
    // A parse failure hands back an empty header, which is the same value a
    // config naming no pin hands back. Told to add `t_version = "0.1.0"` to a
    // file whose first line is already `t_version = "0.1.0"`, a reader goes
    // looking anywhere but at the unclosed quote three lines down.
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("t.toml");
    std::fs::write(&config, "t_version = \"0.1.0\"\nbroken = [1, 2\n").unwrap();
    let located = crate::discover::Located {
        workdir:     d.path().to_path_buf(),
        config_path: config.clone(),
    };
    let err = resolve_pin(&T, Some(&located), d.path(), d.path()).unwrap_err();
    assert!(
        err.contains("not readable as TOML"),
        "the parse failure was reported as something else: {err}"
    );
    assert!(err.contains("t.toml"), "the file is not named: {err}");
    assert!(
        !err.contains("add one to"),
        "still telling the reader to add a key the file already has: {err}"
    );
}

#[test]
fn a_config_that_is_toml_and_names_no_pin_still_says_to_add_one() {
    // The control on the arm above. Both reach `resolve_pin` with an empty
    // header, so an arm that fired on both would have replaced one wrong
    // message with another.
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("t.toml");
    std::fs::write(&config, "unrelated = \"value\"\n").unwrap();
    let located = crate::discover::Located {
        workdir:     d.path().to_path_buf(),
        config_path: config.clone(),
    };
    let err = resolve_pin(&T, Some(&located), d.path(), d.path()).unwrap_err();
    assert!(err.contains("t_version"), "{err}");
    assert!(
        !err.contains("not readable as TOML"),
        "valid TOML reported as a parse failure: {err}"
    );
}

#[test]
fn the_missing_pin_message_names_a_key_that_nearly_was_one() {
    // `t_ref` is not read for a pin and cannot be refused, since the config
    // belongs to the tool. What it does is make the message wrong in a way the
    // reader cannot see: told to add a pin to `t.toml`, they open a file that
    // already carries something pin-shaped and go looking anywhere else.
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("t.toml");
    std::fs::write(&config, "t_ref = \"abc123\"\n").unwrap();
    let located = crate::discover::Located {
        workdir:     d.path().to_path_buf(),
        config_path: config.clone(),
    };
    let err = resolve_pin(&T, Some(&located), d.path(), d.path()).unwrap_err();
    assert!(err.contains("t_ref"), "the near miss is not named: {err}");
    assert!(
        err.contains("t_version"),
        "the correct spelling is not given: {err}"
    );
}

#[test]
fn a_config_carrying_nothing_pin_shaped_gets_no_near_miss_line() {
    // The control. Without it, a near-miss check that fired on every key would
    // pass the test above while making every other missing-pin message worse.
    let d = tempfile::tempdir().unwrap();
    let config = d.path().join("t.toml");
    std::fs::write(&config, "unrelated = \"value\"\nother = 1\n").unwrap();
    let located = crate::discover::Located {
        workdir:     d.path().to_path_buf(),
        config_path: config.clone(),
    };
    let err = resolve_pin(&T, Some(&located), d.path(), d.path()).unwrap_err();
    assert!(err.contains("t_version"), "{err}");
    assert!(
        !err.contains("not one of the keys read"),
        "a near miss was reported where there is none: {err}"
    );
    assert!(!err.contains("unrelated"), "{err}");
    assert!(!err.contains("other"), "{err}");
}

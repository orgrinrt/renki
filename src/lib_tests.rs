//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;

const T: Tool = Tool {
    short: "widget",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "t",
    default_url: "u",
    launcher_crate: "cargo-widget",
    ..Tool::CONVENTIONS
};

#[test]
fn a_launcher_with_a_broken_descriptor_refuses_to_start() {
    // The point of the check is that it runs, and a predicate tested only
    // as a predicate stays green when nothing calls it. Every arm below is
    // a descriptor that would otherwise run and misbehave quietly.
    const BAD: [Tool; 16] = [
        Tool {
            short: "my-tool",
            ..T
        },
        Tool {
            config_file: "",
            ..T
        },
        // One arm per pin key rather than one for the set. Each is read on a
        // different pin form, so a check over the set with a single empty
        // name would leave four of the five unenforced.
        Tool {
            pin_keys: PinKeys {
                version: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                rev: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                tag: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                branch: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                git: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            engine_crate: "",
            ..T
        },
        Tool {
            engine_bin: Some(""),
            ..T
        },
        Tool {
            cache_namespace: "",
            ..T
        },
        Tool {
            launcher_crate: "",
            ..T
        },
        Tool {
            anchor: Anchor::Marker(""),
            ..T
        },
        // Empty, so both git attempts ask cargo to install from nowhere and
        // it fails naming a url the user never wrote.
        Tool {
            default_url: "",
            ..T
        },
        Tool { dir_flag: "", ..T },
        // The same string for both, so `normalize_args` strips the user's
        // copy as the directory flag and `dispatch` then finds no override
        // to act on. The launcher runs and quietly ignores what was asked.
        Tool {
            engine_flag: Cli::DIR_FLAG,
            ..T
        },
        // Zero, so every build is older than the window the moment it lands.
        // The collector then removes it on the next pass and the tool rebuilds
        // from scratch on every run, while its own message says once per
        // version.
        Tool {
            cache_retention: std::time::Duration::ZERO,
            ..T
        },
    ];
    for bad in &BAD {
        let Some(why) = bad.defect() else {
            panic!("no defect reported for {:?}", bad.short);
        };

        // The message is the whole point of the arm, so it is read rather than
        // counted. One of these shipped with three runs of eighteen spaces in
        // it, from a continued literal whose backslashes were dropped in a
        // reflow, and `is_some()` had nothing to say about that. A `\`
        // continuation eats the leading whitespace of the next line and a bare
        // newline in a string does not, so the run of spaces IS the tell and it
        // cannot occur in a message anybody wrote on purpose.
        assert!(
            !why.contains("  "),
            "{:?}'s defect message carries a run of spaces, which is what a \
             dropped line continuation looks like: {why:?}",
            bad.short
        );
        assert!(
            !why.contains('\n'),
            "{:?}'s defect message carries a newline, so it breaks the one-line \
             shape every other diagnostic here has: {why:?}",
            bad.short
        );
        assert!(
            why.len() > 20 && why.ends_with(|c: char| c.is_alphanumeric() || c == '"'),
            "{:?}'s defect message is too short to diagnose anything, or ends \
             mid-thought: {why:?}",
            bad.short
        );

        let err = outcome(bad, &s(&["widget"])).expect_err("a broken launcher ran");
        assert!(
            err.contains("descriptor is not usable"),
            "it failed for some other reason, so nothing checked the descriptor: {err}"
        );
    }
}

#[test]
fn an_empty_pin_key_is_reported_by_name() {
    // A message saying one of five keys is empty leaves the reader to work out
    // which, and the five are one word apart. So each arm carries its own
    // name, and this checks the name rather than that something was returned.
    const CASES: [(Tool, &str); 5] = [
        (
            Tool {
                pin_keys: PinKeys {
                    version: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.version",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    rev: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.rev",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    tag: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.tag",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    branch: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.branch",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    git: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.git",
        ),
    ];

    for (bad, name) in &CASES {
        let msg = bad
            .defect()
            .expect("an empty pin key was not reported at all");
        assert!(
            msg.contains(name),
            "the defect message does not name the field that is wrong. \
             Expected it to mention `{name}`, got: {msg}"
        );
    }

    // The control. Every arm above differs from the fixture in exactly one
    // key, so without this the loop would still pass if `defect` refused the
    // fixture itself for some unrelated reason and happened to name the field
    // in that message.
    assert!(
        T.defect().is_none(),
        "the fixture is already broken, so nothing above isolated a single key"
    );
}

#[test]
fn a_sound_descriptor_is_not_refused() {
    // The control. Without it the test above passes for a `defect` that
    // returns `Some` unconditionally, which would refuse every launcher
    // ever built on this.
    assert!(T.defect().is_none(), "the fixture itself is not usable");
    const NAMED_BIN: Tool = Tool {
        engine_bin: Some("engine"),
        ..T
    };
    assert!(NAMED_BIN.defect().is_none());
}

#[test]
fn an_empty_engine_bin_would_have_looked_for_the_directory_itself() {
    // Why that arm is in the list, computed rather than asserted from
    // memory: the join produces the bin directory, and a directory is never
    // the file the cache short-circuits on, so the engine rebuilds forever.
    const EMPTY: Tool = Tool {
        engine_bin: Some(""),
        ..T
    };
    let looked_for = Path::new("/cache/builds/k/bin").join(EMPTY.engine_bin_name());

    // The claim is about renki, not about `Path`: an empty name adds no
    // component, so what the cache would short-circuit on is the bin directory.
    // Asserting the two spellings of that path against each other only measures
    // `Path`'s own trailing-separator handling.
    assert_eq!(EMPTY.engine_bin_name(), "");
    assert_eq!(
        looked_for,
        Path::new("/cache/builds/k/bin"),
        "an empty name should add no component"
    );
    assert!(
        !looked_for.is_file(),
        "a directory is never the file the cache short-circuits on, so the \
         engine would rebuild on every run"
    );
    // The control: a name that is not empty does add one, so the assertion
    // above is about the emptiness rather than about `join` in general.
    const NAMED: Tool = Tool {
        engine_bin: Some("shortname"),
        ..T
    };
    assert_eq!(
        Path::new("/cache/builds/k/bin").join(NAMED.engine_bin_name()),
        Path::new("/cache/builds/k/bin/shortname")
    );
}

#[test]
fn the_eviction_message_is_not_prefixed_with_the_tool_name() {
    // `run_without_sanitizing` puts the tool's name in front of every error it
    // prints, so a message carrying its own copy goes out as `widget: widget:`.
    // This branch needs a build that vanishes three times running, which no test
    // can arrange, so the message is asserted where it is built instead.
    let msg = eviction_exhausted(Path::new("/cache/builds/k/bin/engine"));

    assert!(
        !msg.starts_with(T.short),
        "the message prefixes itself and the printer prefixes it again: {msg}"
    );
    assert!(
        msg.contains("/cache/builds/k/bin/engine"),
        "the message does not say which build went missing: {msg}"
    );
    assert!(
        msg.contains(&EVICTION_RETRIES.to_string()),
        "the message does not say how many attempts were made: {msg}"
    );
    // The control: a message that did self-prefix is caught by the first
    // assertion, so it is about this string rather than about any string.
    let doubled = format!("{}: {msg}", T.short);
    assert!(doubled.starts_with(T.short));
}

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
        subcommand: "locate",
        root_key: "repo",
        config_key: "manifest",
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
        .map(|line| &line[want.len()..])
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

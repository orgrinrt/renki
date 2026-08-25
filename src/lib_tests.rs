//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

const T: Tool = Tool {
    anchor: Anchor::Marker(".git"),
    short: "widget",
    config_file: "t.toml",
    pin_prefix: "t",
    engine_crate: "engine",
    engine_bin: None,
    cache_namespace: "t",
    default_url: "u",
    launcher_crate: "cargo-widget",
    workdir: None,
    dir_flag: Cli::DIR_FLAG,
    engine_flag: Cli::ENGINE_FLAG,
    locate: Locate::DEFAULT,
    hooks: Hooks::NONE,
};

#[test]
fn a_launcher_with_a_broken_descriptor_refuses_to_start() {
    // The point of the check is that it runs, and a predicate tested only
    // as a predicate stays green when nothing calls it. Every arm below is
    // a descriptor that would otherwise run and misbehave quietly.
    const BAD: [Tool; 11] = [
        Tool {
            short: "my-tool",
            ..T
        },
        Tool {
            config_file: "",
            ..T
        },
        Tool {
            pin_prefix: "",
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
    ];
    for bad in &BAD {
        assert!(
            bad.defect().is_some(),
            "no defect reported for {:?}",
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
    assert_eq!(looked_for, Path::new("/cache/builds/k/bin/"));
    assert_eq!(looked_for, Path::new("/cache/builds/k/bin"));
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn the_locate_answer_uses_the_tools_own_key_names() {
    // All three were hardcoded here while `Locate` documented them as "a
    // contract with those callers", so a tool that set them got the
    // conventional spellings anyway and its own shell helpers, parsing the
    // names it had chosen, parsed nothing at all.
    const OWN: Locate = Locate {
        subcommand: Some("locate"),
        root_key: "repo",
        config_key: "manifest",
        workdir_key: "mock_dir",
    };
    let d = tempfile::tempdir().unwrap();
    let wd = d.path().join("mock");
    std::fs::create_dir_all(&wd).unwrap();

    let got = locate_answer(&OWN, d.path(), "/c/x.toml", &wd);
    assert_eq!(
        got,
        format!(
            "repo={}\nmanifest=/c/x.toml\nmock_dir={}\n",
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
    let d2 = locate_answer(&Locate::DEFAULT, d.path(), "/c/x.toml", &wd);
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
    let got = locate_answer(&Locate::DEFAULT, d.path(), "", &absent);
    assert!(got.ends_with("workdir=\n"), "{got}");
    assert!(got.contains("\nconfig=\n"), "{got}");
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
fn the_missing_root_message_names_what_was_looked_for() {
    assert!(no_root(&T).contains(".git"), "{}", no_root(&T));
    assert!(no_root(&T).contains("WIDGET_ROOT"), "{}", no_root(&T));

    const SPAN: Tool = Tool {
        anchor: Anchor::ConfigFile,
        short: "widget",
        ..T
    };
    // a config-anchored tool has no marker, so naming one would send the
    // reader looking for a file that has nothing to do with it.
    assert!(no_root(&SPAN).contains("t.toml"), "{}", no_root(&SPAN));
    assert!(!no_root(&SPAN).contains(".git"), "{}", no_root(&SPAN));
    assert!(no_root(&SPAN).contains("WIDGET_ROOT"), "{}", no_root(&SPAN));
}

#[test]
fn a_legacy_pin_registers_as_legacy_whatever_its_reference_is() {
    let p = Pin {
        url: "u".into(),
        reference: Reference::Rev("abc".into()),
    };
    assert_eq!(
        pin_form_and_value(&p, PinSource::Config),
        (registry::PinForm::Rev, "abc".to_string())
    );
    assert_eq!(
        pin_form_and_value(&p, PinSource::Legacy),
        (registry::PinForm::Legacy, "abc".to_string())
    );
}

#[test]
fn the_missing_pin_message_names_the_tools_own_key() {
    let d = tempfile::tempdir().unwrap();
    let err = resolve_pin(&T, None, d.path(), d.path()).unwrap_err();
    assert!(err.contains("t_version"), "{err}");
    assert!(err.contains("t.toml"), "{err}");
}

#[test]
fn the_refusal_says_whether_the_override_was_set() {
    // an operator who has just exported the variable and got it wrong is
    // the one person this message has to serve, and telling them it is
    // unset sends them looking somewhere else entirely.
    let unset = no_root_with(&T, None);
    assert!(unset.contains("WIDGET_ROOT is unset"), "{unset}");
    assert!(unset.contains(".git"), "{unset}");

    let set = no_root_with(&T, Some("/nope/xyzzy".into()));
    assert!(set.contains("/nope/xyzzy"), "{set}");
    assert!(set.contains("not a directory"), "{set}");
    assert!(
        !set.contains("is unset"),
        "the set case still claims unset: {set}"
    );
}

#[test]
fn the_refusal_names_the_anchor_the_tool_actually_looks_for() {
    // a config-anchored tool never looked for `.git`, so naming it would
    // send the reader to create one.
    const SPAN: Tool = Tool {
        anchor: Anchor::ConfigFile,
        ..T
    };
    let m = no_root_with(&SPAN, None);
    assert!(m.contains(T.config_file), "{m}");
    assert!(!m.contains(".git"), "{m}");
}

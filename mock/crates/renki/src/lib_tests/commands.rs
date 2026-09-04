//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A tool's own commands, run through the launcher's dispatch. Split from the
//! crate's tests by subject; the fixture is the parent module's.

use std::path::PathBuf;
use std::sync::Mutex;

use renki_config::{Bool, Declared, List, PathText, Setting, Source, User};

use super::*;
use crate::config::Toml;

const SETTINGS: &[Declared<Toml>] = &[
    Setting::<Bool, User>::new("strict", "false", "A flag.").row(),
    Setting::<List<PathText>, User>::new("roots", "[\"~\"]", "Some paths.").row(),
];

/// What the last run of `record` was handed, since a `fn` pointer has
/// nowhere else to put it.
struct Seen {
    cwd:      PathBuf,
    root:     Option<PathBuf>,
    settings: Vec<(&'static str, String, Source)>,
    args:     Vec<OsString>,
}

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

fn record(inv: &Invocation<'_>) -> Result<(), String> {
    *SEEN.lock().unwrap() = Some(Seen {
        cwd:      inv.cwd.to_path_buf(),
        root:     inv.root.map(Path::to_path_buf),
        settings: inv
            .settings
            .iter()
            .map(|s| (s.key(), s.text().to_string(), s.source()))
            .collect(),
        args:     inv.args.to_vec(),
    });
    // and the accessor agrees with the table it reads
    assert_eq!(
        inv.setting("strict"),
        inv.settings
            .iter()
            .find(|s| s.key() == "strict")
            .map(|s| s.text())
    );
    assert_eq!(inv.setting("nope"), None);
    Ok(())
}

fn refuse(inv: &Invocation<'_>) -> Result<(), String> {
    Err(format!("refused with {} arguments", inv.args.len()))
}

const WITH_COMMANDS: Tool = Tool {
    settings: SETTINGS,
    commands: &[
        Command {
            name: "record",
            doc:  "Writes down what it was handed.",
            run:  record,
        },
        Command {
            name: "refuse",
            doc:  "Refuses, naming its argument count.",
            run:  refuse,
        },
    ],
    ..T
};

#[test]
fn a_command_runs_before_a_root_is_required_and_gets_the_resolved_settings() {
    // Serialised on the one static, since the two tests below share it.
    let _guard = SERIAL.lock().unwrap();
    // Through `outcome`, so the descriptor check, the flag stripping and the
    // dispatch all run, rather than `run_command` alone.
    outcome(
        &WITH_COMMANDS,
        &s(&["widget", "--cfg", "strict=true", "record", "a", "--cfg", "roots=[/srv, /opt]"]),
    )
    .expect("the command was not run");
    let seen = SEEN.lock().unwrap().take().expect("the command never ran");

    // The launcher's own flag came off ahead of the name and was resolved;
    // the second sits after the name and is taken too, since the flag is the
    // launcher's wherever it is, and the command sees neither.
    assert_eq!(seen.args, s(&["a"]));
    assert_eq!(seen.settings, vec![
        ("strict", "true".to_string(), Source::Flag),
        ("roots", "[\"/srv\", \"/opt\"]".to_string(), Source::Flag),
    ]);
    // The root is looked for, and this test runs inside a repository, so it
    // is found: the crate's marker is `.git` and the walk up from the test's
    // working directory reaches this checkout.
    let root = seen
        .root
        .expect("no root, so the walk up from the test did not find this repository");
    assert!(root.join(".git").exists(), "{}", root.display());
    assert!(
        seen.cwd.starts_with(&root),
        "{} is not under {}",
        seen.cwd.display(),
        root.display()
    );
}

#[test]
fn a_command_may_run_where_there_is_no_repository_at_all() {
    let _guard = SERIAL.lock().unwrap();
    // `run_command` with the root the dispatch would have handed on outside
    // any repository: `None`, as an answer rather than a refusal. The
    // repository's file is then not read, which is the only difference the
    // command can observe.
    let (cli, args) = config::Cli::take(&WITH_COMMANDS, s(&["record", "x", "y"])).unwrap();
    let command = WITH_COMMANDS.command_named(&args).unwrap();
    run_command(&WITH_COMMANDS, command, None, &cli, &args[1 ..]).unwrap();
    let seen = SEEN.lock().unwrap().take().expect("the command never ran");
    assert_eq!(seen.root, None);
    assert_eq!(seen.args, s(&["x", "y"]));
    assert_eq!(
        seen.settings[0],
        ("strict", "false".to_string(), Source::Default)
    );
}

#[test]
fn a_commands_refusal_comes_back_as_the_launchers_error() {
    let err = outcome(&WITH_COMMANDS, &s(&["widget", "refuse", "a", "b"]))
        .expect_err("a refusal ran green");
    assert_eq!(err, "refused with 2 arguments");
    // and a bare `--cfg` after the name is the launcher's refusal, not the
    // command's, since the flag comes off first wherever it sits
    let err =
        outcome(&WITH_COMMANDS, &s(&["widget", "refuse", "--cfg"])).expect_err("a bare --cfg ran");
    assert!(err.contains("nothing followed it"), "{err}");
}

#[test]
fn a_command_is_tried_after_the_crates_own_queries() {
    // `config` on a tool with settings is the crate's, whatever a command
    // table says; the descriptor check refuses that table, so the order
    // here is settled before dispatch rather than by it. What is checked at
    // dispatch is that `locate` and `config` still answer with a command
    // table present, which they do only if the command lookup comes after.
    let _guard = SERIAL.lock().unwrap();
    let err = outcome(&WITH_COMMANDS, &s(&["widget", "config", "get", "nope"]))
        .expect_err("config answered a key it lacks");
    assert!(err.contains("has no setting called"), "{err}");
    assert!(
        SEEN.lock().unwrap().is_none(),
        "the command ran for the crate's own query"
    );
}

/// The two tests that read `SEEN` cannot interleave, and cargo runs tests
/// on threads.
static SERIAL: Mutex<()> = Mutex::new(());

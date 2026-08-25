//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::tool::{Anchor, Cli, Hooks, Locate, Tool};

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

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).to_string()).collect()
}

#[test]
fn a_direct_invocation_forwards_everything() {
    assert_eq!(
        normalize_args(&T, &s(&["/usr/bin/mock", "lock", "--foo"])),
        s(&["lock", "--foo"])
    );
}

#[test]
fn a_cargo_subcommand_drops_the_repeated_name() {
    // cargo runs `cargo widget x` as `cargo-widget widget x`, so the engine
    // would otherwise be handed a subcommand it does not have.
    assert_eq!(
        normalize_args(
            &T,
            &s(&["/root/.cargo/bin/cargo-widget", "widget", "lock", "--foo"])
        ),
        s(&["lock", "--foo"])
    );
    assert_eq!(
        normalize_args(&T, &s(&["cargo-widget", "widget"])),
        Vec::<String>::new()
    );
}

#[test]
fn a_repeated_name_is_dropped_only_when_it_is_the_cargo_shape() {
    // the control, and the reason the rule is written against the program
    // name rather than the first argument. `mock mock` is a user asking the
    // engine for a subcommand called `mock`, and eating it would be wrong.
    assert_eq!(
        normalize_args(&T, &s(&["/usr/bin/mock", "mock"])),
        s(&["mock"])
    );
    // and a cargo-shaped launcher whose first argument is something else
    assert_eq!(
        normalize_args(&T, &s(&["cargo-mock", "lock"])),
        s(&["lock"])
    );
    // and the name has to match the program's own suffix
    assert_eq!(
        normalize_args(&T, &s(&["cargo-mock", "other", "lock"])),
        s(&["other", "lock"])
    );
}

#[test]
fn a_user_supplied_dir_flag_is_stripped() {
    // the launcher owns `--dir`, and two of them would leave the engine
    // reading whichever it parsed last.
    assert_eq!(
        normalize_args(
            &T,
            &s(&["widget", "check", "--dir", "/somewhere", "--scope", "x"])
        ),
        s(&["check", "--scope", "x"])
    );
}

#[test]
fn the_joined_dir_flag_is_stripped_too() {
    // The defect the single `take_flag` exists to prevent, and the one the
    // two copies before it had: the separated spelling was stripped and the
    // joined one was forwarded, so the engine saw a flag the launcher owns
    // and the launcher's own `--dir` fought it.
    assert_eq!(
        normalize_args(
            &T,
            &s(&["widget", "check", "--dir=/somewhere", "--scope", "x"])
        ),
        s(&["check", "--scope", "x"])
    );
    // the same for the flag this tool actually spells, in case a tool picks
    // another and only one of the two spellings is wired to it
    const OTHER: Tool = Tool {
        dir_flag: "--at",
        ..T
    };
    assert_eq!(
        normalize_args(&OTHER, &s(&["widget", "check", "--at=/somewhere"])),
        s(&["check"])
    );
    assert_eq!(
        normalize_args(&OTHER, &s(&["widget", "check", "--dir=/somewhere"])),
        s(&["check", "--dir=/somewhere"]),
        "a flag the tool did not choose was stripped anyway"
    );
}

#[test]
fn a_dir_flag_with_no_value_is_dropped_and_takes_nothing_with_it() {
    // Deliberate, and the counterpart to the engine flag's refusal: the
    // user's directory is discarded whether they named one or not, so
    // naming nothing changes nothing. What must not happen is the next
    // argument being eaten as the value.
    assert_eq!(
        normalize_args(&T, &s(&["widget", "check", "--dir", "--scope", "x"])),
        s(&["check", "--scope", "x"])
    );
}

#[test]
fn the_locate_query_needs_a_subcommand_to_ask_it_with() {
    // The `is_some()` half of the guard. A tool that wants no locate query
    // has `subcommand: None`, and a bare invocation has no first argument,
    // so without it `None == None` and every plain run answers the query
    // instead of running the engine.
    const NO_QUERY: Locate = Locate {
        subcommand: None,
        ..Locate::DEFAULT
    };
    assert!(
        !is_the_locate_query(&NO_QUERY, &s(&[])),
        "a tool with no locate subcommand answered the query on a bare run"
    );
    assert!(!is_the_locate_query(&NO_QUERY, &s(&["locate"])));
    assert!(!is_the_locate_query(&NO_QUERY, &s(&["lock"])));

    // and the control, so the assertions above are not passing because the
    // predicate is a constant `false`
    assert!(is_the_locate_query(&Locate::DEFAULT, &s(&["locate"])));
    assert!(!is_the_locate_query(&Locate::DEFAULT, &s(&[])));
    assert!(!is_the_locate_query(&Locate::DEFAULT, &s(&["lock"])));

    // a tool that spells it differently is asked by its own name and not by
    // the conventional one
    const RENAMED: Locate = Locate {
        subcommand: Some("where"),
        ..Locate::DEFAULT
    };
    assert!(is_the_locate_query(&RENAMED, &s(&["where"])));
    assert!(!is_the_locate_query(&RENAMED, &s(&["locate"])));
}

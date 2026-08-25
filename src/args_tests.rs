//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::tool::{Locate, Tool};

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

/// Arguments, as the launcher actually receives them.
///
/// `OsString` rather than `String`, because argv is bytes on unix and the
/// launcher carries it that way from `args_os` all the way to `exec`.
fn s(v: &[&str]) -> Vec<std::ffi::OsString> {
    v.iter().map(|x| std::ffi::OsString::from(*x)).collect()
}

#[test]
fn a_direct_invocation_forwards_everything() {
    assert_eq!(
        normalize_args(&T, &s(&["/usr/bin/widget", "lock", "--foo"])),
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
        Vec::<std::ffi::OsString>::new()
    );
}

#[test]
fn a_repeated_name_is_dropped_only_when_it_is_the_cargo_shape() {
    // the control, and the reason the rule is written against the program
    // name rather than the first argument. `widget widget` is a user asking
    // the engine for a subcommand called `widget`, and eating it would be
    // wrong.
    assert_eq!(
        normalize_args(&T, &s(&["/usr/bin/widget", "widget"])),
        s(&["widget"])
    );
    // and a cargo-shaped launcher whose first argument is something else
    assert_eq!(
        normalize_args(&T, &s(&["cargo-widget", "lock"])),
        s(&["lock"])
    );
    // and the name has to match the program's own suffix
    assert_eq!(
        normalize_args(&T, &s(&["cargo-widget", "other", "lock"])),
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
fn a_tool_with_no_locate_query_never_answers_one() {
    // The absence used to live in `Locate::subcommand` as well as in
    // `Tool::locate`, so a bare invocation with no first argument compared one
    // `None` against the other and every plain run answered the query instead
    // of running the engine. It lives in one place now, and the type is what
    // keeps it there.
    assert!(
        !is_the_locate_query(None, &s(&[])),
        "a tool with no locate query answered one on a bare run"
    );
    assert!(!is_the_locate_query(None, &s(&["locate"])));
    assert!(!is_the_locate_query(None, &s(&["lock"])));

    // and the control, so the assertions above are not passing because the
    // predicate is a constant `false`
    assert!(is_the_locate_query(Some(&Locate::DEFAULT), &s(&["locate"])));
    assert!(!is_the_locate_query(Some(&Locate::DEFAULT), &s(&[])));
    assert!(!is_the_locate_query(Some(&Locate::DEFAULT), &s(&["lock"])));

    // a tool that spells it differently is asked by its own name and not by
    // the conventional one
    const RENAMED: Locate = Locate {
        subcommand: "where",
        ..Locate::DEFAULT
    };
    assert!(is_the_locate_query(Some(&RENAMED), &s(&["where"])));
    assert!(!is_the_locate_query(Some(&RENAMED), &s(&["locate"])));
}

#[test]
fn an_argument_that_is_not_utf8_survives_the_walk_intact() {
    // On unix argv is bytes. `std::env::args()` panics on one of these rather
    // than returning it, which is why the entry point uses `args_os`, and
    // everything from there down has to carry the bytes rather than convert
    // them: a latin-1 filename handed through by a script names a real file,
    // and the engine is the thing meant to open it.
    use std::os::unix::ffi::OsStringExt;

    let odd = std::ffi::OsString::from_vec(vec![0xe9, 0x6f, 0x6b]);
    let raw = vec![
        std::ffi::OsString::from("/usr/bin/widget"),
        std::ffi::OsString::from("lock"),
        odd.clone(),
    ];
    let out = normalize_args(&T, &raw);
    assert_eq!(out, vec![std::ffi::OsString::from("lock"), odd.clone()]);

    // The control on the assertion above, since a lossy conversion would still
    // produce a two-element vector and still pass a length check. These bytes
    // are not valid UTF-8, so anything that went through a `String` on the way
    // has replaced them with U+FFFD and the comparison sees it.
    assert!(odd.to_str().is_none(), "the fixture is valid UTF-8");
    assert_eq!(out[1].as_encoded_bytes(), &[0xe9, 0x6f, 0x6b]);
}

#[test]
fn a_flag_value_that_is_not_utf8_is_taken_and_kept_whole() {
    // The other half: the launcher's own flags take paths, and a path is bytes
    // in exactly the same way. `--dir` is dropped rather than read, so the one
    // to check is that dropping it takes the value with it and leaves the rest
    // of the bytes alone.
    use std::os::unix::ffi::OsStringExt;

    let odd = std::ffi::OsString::from_vec(vec![0xff, 0xfe, 0x2f, 0x78]);
    let raw = vec![
        std::ffi::OsString::from("/usr/bin/widget"),
        std::ffi::OsString::from(T.dir_flag),
        std::ffi::OsString::from("/somewhere"),
        std::ffi::OsString::from("lock"),
        odd.clone(),
    ];
    let out = normalize_args(&T, &raw);
    assert_eq!(out, vec![std::ffi::OsString::from("lock"), odd]);
}

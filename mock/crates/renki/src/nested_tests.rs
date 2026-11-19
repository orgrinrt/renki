//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

/// A reader over the variables named, each set to a value that is not empty.
fn reading(set: &'static [&'static str]) -> impl Fn(&str) -> Option<OsString> {
    move |name| set.contains(&name).then(|| OsString::from("x"))
}

#[test]
fn a_build_scripts_variables_say_build_script() {
    assert!(inside_a_build_script(reading(&[
        "CARGO", "OUT_DIR", "TARGET", "HOST"
    ])));
}

#[test]
fn the_variables_of_cargo_run_and_cargo_test_do_not() {
    // What the sketch read in a `cargo run` program and a `cargo test` binary, which run with
    // the build directory's lock free: refusing them would refuse a caller that cannot hang.
    assert!(!inside_a_build_script(reading(&[
        "CARGO",
        "OUT_DIR",
        "CARGO_MANIFEST_DIR"
    ])));
}

#[test]
fn each_variable_missing_alone_is_not_a_build_script() {
    for missing in BUILD_SCRIPT_VARS {
        let var = |name: &str| (name != missing).then(|| OsString::from("x"));
        assert!(!inside_a_build_script(var), "{missing} missing");
    }
}

#[test]
fn an_empty_variable_counts_as_unset() {
    let var = |name: &str| Some(OsString::from(if name == "HOST" { "" } else { "x" }));
    assert!(!inside_a_build_script(var));
}

#[test]
fn the_refusal_names_the_build_script_and_the_command_to_run() {
    let err = refuse_inside_a_build_script(
        &the_tool("t"),
        Path::new("/repo"),
        reading(&["CARGO", "OUT_DIR", "TARGET", "HOST"]),
    )
    .unwrap_err();
    assert!(err.contains("build script"), "{err}");
    assert!(err.contains("Run `t` once from a shell in /repo"), "{err}");
    assert!(err.contains("TARGET"), "{err}");
}

#[test]
fn outside_a_build_script_nothing_is_refused() {
    assert!(refuse_inside_a_build_script(&the_tool("t"), Path::new("/repo"), reading(&[])).is_ok());
}

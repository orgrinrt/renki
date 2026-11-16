//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

fn config(dir: &Path, body: &str) {
    std::fs::create_dir_all(dir.join(".cargo")).unwrap();
    std::fs::write(dir.join(".cargo/config.toml"), body).unwrap();
}

fn no_vars(_: &str) -> Option<OsString> {
    None
}

#[test]
fn nothing_configured_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    assert_eq!(
        resolve(&root, Some(&tmp.path().join("home")), no_vars),
        None
    );
}

#[test]
fn a_relative_value_in_a_file_is_taken_from_the_directory_holding_its_cargo() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("a/repo");
    std::fs::create_dir_all(&root).unwrap();
    config(tmp.path(), "[build]\nbuild-dir = \"target\"\n");
    assert_eq!(
        resolve(&root, None, no_vars),
        Some(tmp.path().join("target"))
    );
}

#[test]
fn the_build_dir_at_any_level_wins_over_a_nearer_target_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\ntarget-dir = \"near\"\n");
    config(tmp.path(), "[build]\nbuild-dir = \"/far\"\n");
    assert_eq!(resolve(&root, None, no_vars), Some(PathBuf::from("/far")));
}

#[test]
fn the_target_dir_is_the_build_dir_where_nothing_sets_one() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\ntarget-dir = \"t\"\n");
    assert_eq!(resolve(&root, None, no_vars), Some(root.join("t")));
}

#[test]
fn the_nearest_file_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"/near\"\n");
    config(tmp.path(), "[build]\nbuild-dir = \"/far\"\n");
    assert_eq!(resolve(&root, None, no_vars), Some(PathBuf::from("/near")));
}

#[test]
fn the_variable_wins_over_every_file_and_is_taken_from_the_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"/near\"\n");
    let var = |n: &str| (n == "CARGO_BUILD_BUILD_DIR").then(|| OsString::from("v"));
    assert_eq!(resolve(&root, None, var), Some(root.join("v")));
}

#[test]
fn the_cargo_home_is_read_last_and_relative_to_its_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let home = tmp.path().join("elsewhere/.cargo");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[build]\nbuild-dir = \"b\"\n").unwrap();
    assert_eq!(
        resolve(&root, Some(&home), no_vars),
        Some(tmp.path().join("elsewhere/b"))
    );
    // A root under the cargo home's parent reads the same file once, nearest first.
    config(&root, "[build]\nbuild-dir = \"/near\"\n");
    assert_eq!(
        resolve(&root, Some(&home), no_vars),
        Some(PathBuf::from("/near"))
    );
}

#[test]
fn a_template_is_handed_on_for_cargo_to_expand() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"{cargo-cache-home}/build\"\n");
    assert_eq!(
        resolve(&root, None, no_vars),
        Some(PathBuf::from("{cargo-cache-home}/build"))
    );
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

fn config(dir: &Path, body: &str) {
    config_named(dir, "config.toml", body);
}

fn config_named(dir: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(dir.join(".cargo")).unwrap();
    std::fs::write(dir.join(".cargo").join(name), body).unwrap();
}

fn no_vars(_: &str) -> Option<OsString> {
    None
}

fn vars(set: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
    move |name| {
        set.iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| OsString::from(v))
    }
}

/// `resolve` in `root` with no cargo home, which must not fail.
fn at(root: &Path, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    resolve(root, None, var).unwrap()
}

#[test]
fn nothing_configured_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    let home = tmp.path().join("home");
    assert_eq!(resolve(&root, Some(&home), no_vars).unwrap(), None);
}

#[test]
fn a_relative_value_in_a_file_is_taken_from_the_directory_holding_its_cargo() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("a/repo");
    std::fs::create_dir_all(&root).unwrap();
    config(tmp.path(), "[build]\nbuild-dir = \"target\"\n");
    assert_eq!(at(&root, no_vars), Some(tmp.path().join("target")));
}

#[test]
fn the_build_dir_at_any_level_wins_over_a_nearer_target_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\ntarget-dir = \"near\"\n");
    config(tmp.path(), "[build]\nbuild-dir = \"/far\"\n");
    assert_eq!(at(&root, no_vars), Some(PathBuf::from("/far")));
}

#[test]
fn the_target_dir_is_the_build_dir_where_nothing_sets_one() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\ntarget-dir = \"t\"\n");
    assert_eq!(at(&root, no_vars), Some(root.join("t")));
}

#[test]
fn the_nearest_file_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"/near\"\n");
    config(tmp.path(), "[build]\nbuild-dir = \"/far\"\n");
    assert_eq!(at(&root, no_vars), Some(PathBuf::from("/near")));
}

#[test]
fn the_variable_wins_over_every_file_and_is_taken_from_the_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"/near\"\n");
    let var = vars(&[("CARGO_BUILD_BUILD_DIR", "v")]);
    assert_eq!(at(&root, var), Some(root.join("v")));
}

#[test]
fn the_target_dir_variables_go_cargo_target_dir_then_cargo_build_target_dir_then_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\ntarget-dir = \"/file\"\n");
    let both = vars(&[("CARGO_TARGET_DIR", "/plain"), ("CARGO_BUILD_TARGET_DIR", "/build")]);
    assert_eq!(at(&root, both), Some(PathBuf::from("/plain")));
    let build_only = vars(&[("CARGO_BUILD_TARGET_DIR", "/build")]);
    assert_eq!(at(&root, build_only), Some(PathBuf::from("/build")));
    assert_eq!(at(&root, no_vars), Some(PathBuf::from("/file")));
}

#[test]
fn a_files_build_dir_wins_over_a_variables_target_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"/file-build\"\n");
    let var = vars(&[("CARGO_TARGET_DIR", "/var-target")]);
    assert_eq!(at(&root, var), Some(PathBuf::from("/file-build")));
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
        resolve(&root, Some(&home), no_vars).unwrap(),
        Some(tmp.path().join("elsewhere/b"))
    );
    // A nearer file still wins over the cargo home's.
    config(&root, "[build]\nbuild-dir = \"/near\"\n");
    assert_eq!(
        resolve(&root, Some(&home), no_vars).unwrap(),
        Some(PathBuf::from("/near"))
    );
}

#[test]
fn a_cargo_home_among_the_roots_ancestors_reads_as_any_other_level() {
    // The cargo home is `<tmp>/.cargo`, which the walk from the root reaches too.
    // Read either way, its relative value is taken from `<tmp>`; and a file it
    // could not parse would be refused once, not skipped on one of the two reads.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir_all(&root).unwrap();
    config(tmp.path(), "[build]\nbuild-dir = \"b\"\n");
    let home = tmp.path().join(".cargo");
    assert_eq!(
        resolve(&root, Some(&home), no_vars).unwrap(),
        Some(tmp.path().join("b"))
    );
    config(tmp.path(), "[build\n");
    let err = resolve(&root, Some(&home), no_vars).unwrap_err();
    assert!(err.contains(".cargo/config.toml"), "{err}");
}

#[test]
fn the_extensionless_file_wins_where_both_are_there() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config_named(&root, "config", "[build]\nbuild-dir = \"/bare\"\n");
    config_named(&root, "config.toml", "[build]\nbuild-dir = \"/toml\"\n");
    assert_eq!(at(&root, no_vars), Some(PathBuf::from("/bare")));
}

#[test]
fn a_file_that_does_not_parse_is_refused_and_named() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build\nbuild-dir = \"/x\"\n");
    let err = resolve(&root, None, no_vars).unwrap_err();
    assert!(
        err.contains(&root.join(".cargo/config.toml").display().to_string()),
        "{err}"
    );
}

#[test]
fn workspace_root_is_the_repositorys_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(tmp.path(), "[build]\nbuild-dir = \"{workspace-root}/bd\"\n");
    assert_eq!(at(&root, no_vars), Some(root.join("bd")));
    let var = vars(&[("CARGO_BUILD_BUILD_DIR", "{workspace-root}/vd")]);
    assert_eq!(at(&root, var), Some(root.join("vd")));
}

#[test]
fn a_workspace_path_hash_names_no_build_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(
        &root,
        "[build]\nbuild-dir = \"{cargo-cache-home}/build/{workspace-path-hash}\"\n",
    );
    assert_eq!(at(&root, no_vars), None);
}

#[test]
fn a_template_that_does_not_name_the_workspace_is_handed_on_for_cargo_to_expand() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    config(&root, "[build]\nbuild-dir = \"{cargo-cache-home}/build\"\n");
    assert_eq!(
        at(&root, no_vars),
        Some(PathBuf::from("{cargo-cache-home}/build"))
    );
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use std::ffi::OsString;

use notko::Maybe;
use renki_config::{
    Bool,
    Choice,
    Declared,
    Either,
    Int,
    List,
    Repo,
    Setting,
    Source,
    Text,
    User,
    choices,
};

use super::*;
use crate::tool::Tool;

choices!(Theme = "dark" | "light");

const SETTINGS: &[Declared<Toml>] = &[
    Setting::<Choice<Theme>, User>::new("theme", "dark", "Which look the pages use.").row(),
    Setting::<Int, Either>::new("server.port", "8787", "The port.").row(),
    Setting::<Bool, Repo>::new("strict", "false", "Whether the gate refuses on warnings.").row(),
    Setting::<List<Text>, User>::new("skip", "[]", "Directories the scan never enters.").row(),
];

const T: Tool = Tool {
    short: "t",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "tns",
    default_url: "u",
    launcher_crate: "t-launcher",
    settings: SETTINGS,
    ..Tool::CONVENTIONS
};

fn s(v: &[&str]) -> Vec<OsString> {
    v.iter().map(OsString::from).collect()
}

fn cli(env: &[(&str, &str)], args: &[&str]) -> (Cli, Vec<OsString>) {
    let env = env
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Cli::take_with(env, s(args)).unwrap()
}

#[test]
fn the_cfg_flag_comes_off_in_both_spellings_and_stops_at_the_double_dash() {
    let (c, rest) = cli(&[], &[
        "run",
        "--cfg",
        "theme=light",
        "--cfg=server.port=1",
        "--",
        "--cfg",
        "x=y",
    ]);
    assert_eq!(rest, s(&["run", "--", "--cfg", "x=y"]));
    assert_eq!(c.flag("theme"), Maybe::Is("light"));
    assert_eq!(c.flag("server.port"), Maybe::Is("1"));
    assert_eq!(
        c.flag("x"),
        Maybe::Isnt,
        "after `--` the flag is the user's"
    );
    // the last one wins
    let (c, _) = cli(&[], &["--cfg", "theme=light", "--cfg", "theme=dark"]);
    assert_eq!(c.flag("theme"), Maybe::Is("dark"));
    // a value with an `=` in it keeps everything after the first
    let (c, _) = cli(&[], &["--cfg", "k=a=b"]);
    assert_eq!(c.flag("k"), Maybe::Is("a=b"));
}

#[test]
fn a_cfg_flag_without_a_value_is_refused_by_name() {
    assert!(
        Cli::take_with(Vec::new(), s(&["--cfg"]))
            .unwrap_err()
            .contains("--cfg")
    );
    assert!(
        Cli::take_with(Vec::new(), s(&["--cfg", "novalue"]))
            .unwrap_err()
            .contains("key=value")
    );
    assert!(
        Cli::take_with(Vec::new(), s(&["--cfg", "=v"]))
            .unwrap_err()
            .contains("key=value")
    );
}

#[test]
fn the_environment_answers_under_the_settings_variable_name() {
    let (c, _) = cli(&[("T_CFG_THEME", "light"), ("T_CFG_SERVER_PORT", "9")], &[]);
    assert_eq!(
        c.env(renki_config::EnvKey::of("t", "theme")),
        Maybe::Is("light")
    );
    assert_eq!(
        c.env(renki_config::EnvKey::of("t", "server.port")),
        Maybe::Is("9")
    );
    assert_eq!(c.env(renki_config::EnvKey::of("t", "strict")), Maybe::Isnt);
}

fn texts(user: Option<&str>, repo: Option<&str>) -> Texts {
    Texts {
        user: user.map_or(Maybe::Isnt, |t| Maybe::Is(t.to_string())),
        repo: repo.map_or(Maybe::Isnt, |t| Maybe::Is(t.to_string())),
    }
}

#[test]
fn every_setting_resolves_with_its_source_over_the_two_files() {
    let (c, _) = cli(&[("T_CFG_STRICT", "true")], &["--cfg", "skip=a, b"]);
    let t = texts(
        Some("theme = \"light\"\n[server]\nport = 1\n"),
        Some("[server]\nport = 2\n"),
    );
    let all = resolve_all(&T, &c, &t).unwrap();
    let got: Vec<(&str, &str, Source)> = all
        .iter()
        .map(|s| (s.key, s.text.as_str(), s.source))
        .collect();
    assert_eq!(got, [
        ("theme", "light", Source::User),
        ("server.port", "2", Source::Repo),
        ("strict", "true", Source::Env),
        ("skip", "[\"a\", \"b\"]", Source::Flag),
    ]);
    // the control: nothing anywhere is every default
    let (c, _) = cli(&[], &[]);
    let all = resolve_all(&T, &c, &texts(None, None)).unwrap();
    assert!(all.iter().all(|s| s.source == Source::Default));
    assert_eq!(
        query::lines(&all),
        "theme=dark\nserver.port=8787\nstrict=false\nskip=[]\n"
    );
}

#[test]
fn a_key_nobody_declared_and_a_misplaced_one_are_refused_by_name() {
    let (c, _) = cli(&[], &[]);
    let e = resolve_all(&T, &c, &texts(Some("thme = \"dark\"\n"), None)).unwrap_err();
    assert!(e.contains("\"thme\"") && e.contains("config schema"), "{e}");
    let e = resolve_all(&T, &c, &texts(None, Some("theme = \"dark\"\n"))).unwrap_err();
    assert!(e.contains("\"theme\"") && e.contains("user setting"), "{e}");
    // a repository file may carry a key it is allowed to
    assert!(resolve_all(&T, &c, &texts(None, Some("strict = true\n"))).is_ok());
}

#[test]
fn a_wrong_value_names_its_place_and_a_file_that_is_not_toml_names_itself() {
    let (c, _) = cli(&[], &[]);
    let e = resolve_all(&T, &c, &texts(Some("theme = \"blue\"\n"), None)).unwrap_err();
    assert!(
        e.contains("theme from user") && e.contains("one of dark, light"),
        "{e}"
    );
    let (c, _) = cli(&[("T_CFG_SERVER_PORT", "eighty")], &[]);
    let e = resolve_all(&T, &c, &texts(None, None)).unwrap_err();
    assert!(e.contains("server.port from env"), "{e}");
    let (c, _) = cli(&[], &[]);
    let e = resolve_all(&T, &c, &texts(Some("theme = \n"), None)).unwrap_err();
    assert!(
        e.contains("user configuration") && e.contains("line 1"),
        "{e}"
    );
}

#[test]
fn the_engine_receives_one_variable_per_setting_and_the_file() {
    let (c, _) = cli(&[], &[]);
    let all = resolve_all(&T, &c, &texts(None, None)).unwrap();
    let envs = engine_env(&T, std::path::Path::new("/home/u/.config/tns/t.toml"), &all);
    let names: Vec<&str> = envs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, [
        "T_CFG_THEME",
        "T_CFG_SERVER_PORT",
        "T_CFG_STRICT",
        "T_CFG_SKIP",
        "T_CONFIG_FILE"
    ]);
    assert_eq!(envs[0].1, OsString::from("dark"));
    assert_eq!(envs[4].1, OsString::from("/home/u/.config/tns/t.toml"));
}

#[test]
fn a_tool_with_no_settings_leaves_config_to_the_engine() {
    const BARE: Tool = Tool {
        settings: &[],
        ..T
    };
    assert!(!query::is_the_config_query(&BARE, &s(&["config", "path"])));
    assert!(query::is_the_config_query(&T, &s(&["config", "path"])));
    assert!(!query::is_the_config_query(&T, &s(&["run", "config"])));
    // and `--cfg` stays in the arguments for such a tool, since its engine may
    // have a flag by that name; for a tool with settings it is taken off
    let args = s(&["run", "--cfg", "x=y", "--cfg=a=b"]);
    let (_, rest) = Cli::take(&BARE, args.clone()).unwrap();
    assert_eq!(rest, args);
    let (_, rest) = Cli::take(&T, args).unwrap();
    assert_eq!(rest, s(&["run"]));
}

#[test]
fn the_users_file_is_written_into_a_directory_that_does_not_exist_yet() {
    let dir = tempfile::tempdir().unwrap();
    let user = dir.path().join("fresh").join("ns").join("t.toml");
    assert!(!user.parent().unwrap().exists());
    query::write_user(&user, "theme = \"dark\"\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(&user).unwrap(),
        "theme = \"dark\"\n"
    );
    // and a parent that is a file rather than a directory is a refusal
    // naming it, not a panic
    let blocked = dir.path().join("file");
    std::fs::write(&blocked, "").unwrap();
    let err = query::write_user(&blocked.join("t.toml"), "").unwrap_err();
    assert!(err.contains("could not create"), "{err}");
}

#[test]
fn set_writes_the_users_file_through_the_schema_and_refuses_the_repos_keys() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("t.toml");
    std::fs::write(&file, "# mine\ntheme = \"dark\"\n").unwrap();
    let out = query::set(&T, &file, "theme", "light").unwrap();
    assert_eq!(out, "# mine\ntheme = \"light\"\n");
    let out = query::set(&T, &file, "skip", "a, b").unwrap();
    assert_eq!(out, "# mine\ntheme = \"dark\"\nskip = [\"a\", \"b\"]\n");
    let out = query::set(&T, &file, "server.port", "9").unwrap();
    assert!(out.ends_with("\n[server]\nport = 9\n"), "{out:?}");
    // refused: a value the kind refuses, an unknown key, a repository key
    assert!(
        query::set(&T, &file, "theme", "blue")
            .unwrap_err()
            .contains("one of")
    );
    assert!(
        query::set(&T, &file, "thme", "dark")
            .unwrap_err()
            .contains("config schema")
    );
    assert!(
        query::set(&T, &file, "strict", "true")
            .unwrap_err()
            .contains("repository setting")
    );
    // and a file that does not exist yet is written from nothing
    let fresh = dir.path().join("fresh.toml");
    assert_eq!(
        query::set(&T, &fresh, "theme", "light").unwrap(),
        "theme = \"light\"\n"
    );
}

#[test]
fn the_users_file_sits_under_the_config_root_by_the_tools_own_name() {
    // The root itself is `renki-dirs`'s and is tested there; what is this
    // crate's is the file name under it.
    let file = user_file(&T).unwrap();
    assert!(file.ends_with("tns/t.toml"), "{}", file.display());
}

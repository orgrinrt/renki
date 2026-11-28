//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A row read at run time, and the resolver over two stacks of layers.

mod common;

use common::{Cli, Toy, doc, layer, part, sound};
use notko::Maybe;
use renki_config::{
    BadTable,
    Declared,
    Either,
    Int,
    List,
    Origin,
    Repo,
    Setting,
    Text,
    User,
    misplaced_keys,
    resolve,
    unknown_keys,
};

// --- a row by its kind's name ---------------------------------------------

#[test]
fn a_row_named_by_its_kind_checks_the_way_the_typed_row_does() {
    let key = String::from("server.port");
    let row = Declared::<Toy>::named(&key, "int", "either", "8787", "The port.").unwrap();
    let typed = Setting::<Int, Either>::new("server.port", "8787", "The port.").row::<Toy>();
    assert_eq!(
        (row.key(), row.kind(), row.scope()),
        ("server.port", "int", "either")
    );
    assert_eq!(
        (row.reads_user(), row.reads_repo()),
        (typed.reads_user(), typed.reads_repo())
    );
    assert_eq!(row.quoted(), typed.quoted());
    for text in ["1", "x", "-3", ""] {
        assert_eq!(
            row.check_text(text).is_ok(),
            typed.check_text(text).is_ok(),
            "{text}"
        );
    }
    for (kind, ok, bad) in [
        ("bool", "true", "yes"),
        ("int", "12", "twelve"),
        ("text", "", ""),
        ("path", "/srv", ""),
        ("list", "[a, b]", "[\"a, b]"),
        ("predicate", "cloud", "a.b"),
    ] {
        let row = Declared::<Toy>::named("k", kind, "user", ok, "").unwrap();
        assert_eq!(row.kind(), kind);
        assert!(row.check_text(ok).is_ok(), "{kind} takes {ok:?}");
        if kind != "text" {
            assert!(row.check_text(bad).is_err(), "{kind} refuses {bad:?}");
        }
    }
    let row = Declared::<Toy>::named("k", "text", "repo", "", "").unwrap();
    assert_eq!((row.reads_user(), row.reads_repo()), (false, true));
}

#[test]
fn a_kind_or_scope_name_nothing_ships_is_refused_and_choice_among_them() {
    for kind in ["choice", "one of dark, light", "float", ""] {
        assert_eq!(
            Declared::<Toy>::named("theme", kind, "user", "dark", "").err(),
            Maybe::Is(BadTable::UnknownKind("theme", kind)),
            "{kind}"
        );
    }
    assert_eq!(
        Declared::<Toy>::named("theme", "text", "everyone", "dark", "").err(),
        Maybe::Is(BadTable::UnknownScope("theme", "everyone"))
    );
    let shown = BadTable::UnknownKind("theme", "choice").to_string();
    assert!(
        shown.contains("theme") && shown.contains("choice"),
        "{shown}"
    );
}

#[test]
fn a_table_is_refused_for_a_key_under_meta() {
    let rows = [Setting::<Int, User>::new("meta.when", "1", "").row::<Toy>()];
    assert_eq!(
        Declared::defect(&rows),
        Maybe::Is(BadTable::UnderMeta("meta.when"))
    );
    let rows = [Setting::<Int, User>::new("meta", "1", "").row::<Toy>()];
    assert_eq!(
        Declared::defect(&rows),
        Maybe::Is(BadTable::UnderMeta("meta"))
    );
    // the control: a key that only begins with the word
    let rows = [Setting::<Int, User>::new("metadata", "1", "").row::<Toy>()];
    assert_eq!(Declared::defect(&rows), Maybe::Isnt);
    // and a run-time row is checked the same
    let rows = [
        Declared::<Toy>::named("a", "int", "user", "1", "").unwrap(),
        Declared::<Toy>::named("a", "int", "user", "2", "").unwrap(),
    ];
    assert_eq!(Declared::defect(&rows), Maybe::Is(BadTable::KeyTwice("a")));
}

// --- the resolver over stacks ---------------------------------------------

fn table() -> [Declared<'static, Toy>; 3] {
    [
        Setting::<Int, Either>::new("port", "1", "").row(),
        Setting::<List<Text>, Either>::new("skip", "[]", "").row(),
        Setting::<Text, User>::new("theme", "dark", "").row(),
    ]
}

const NONE: Cli<'static> = Cli {
    flags: &[],
    env:   &[],
};

#[test]
fn a_value_from_a_layer_names_the_layer_and_one_from_the_base_names_none() {
    let rows = table();
    let (b, l) = (doc("port = 2\nskip = [\"a\"]\n"), doc("port = 3\n"));
    let repo = [layer("app.toy", &b), layer("app.local.toy", &l)];
    let got: Vec<_> = resolve(&rows, "widget", &NONE, sound(&repo), sound(&[]))
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        (got[0].source(), got[0].layer()),
        (Origin::Repo, Maybe::Is(part("app.local.toy")))
    );
    assert_eq!(got[0].to_string(), "3");
    assert_eq!(
        (got[1].source(), got[1].layer()),
        (Origin::Repo, Maybe::Is(part("app.toy")))
    );
    assert_eq!(
        (got[2].source(), got[2].layer()),
        (Origin::Default, Maybe::Isnt)
    );
}

#[test]
fn a_list_resolves_appended_across_the_layers_of_its_stack() {
    let rows = table();
    let (b, l) = (doc("skip = [\"a\", \"b\"]\n"), doc("skip = [\"c\"]\n"));
    let user = [layer("app.toy", &b), layer("app.local.toy", &l)];
    let skip = resolve(&rows, "widget", &NONE, sound(&[]), sound(&user))
        .map(|r| r.unwrap())
        .find(|r| r.row().key() == "skip")
        .unwrap();
    assert_eq!(skip.to_string(), "[\"a\", \"b\", \"c\"]");
    assert_eq!(
        (skip.source(), skip.layer()),
        (Origin::User, Maybe::Is(part("app.local.toy")))
    );
}

#[test]
fn a_refused_value_names_its_origin_and_its_layer() {
    let rows = table();
    let (b, l) = (doc("port = 2\n"), doc("port = \"eighty\"\n"));
    let repo = [layer("app.toy", &b), layer("app.local.toy", &l)];
    let bad = resolve(&rows, "widget", &NONE, sound(&repo), sound(&[]))
        .find_map(|r| {
            match r.err() {
                Maybe::Is(e) => Some(e),
                Maybe::Isnt => None,
            }
        })
        .unwrap();
    assert_eq!((bad.key(), bad.source()), ("port", Origin::Repo));
    assert_eq!(bad.layer(), Maybe::Is(part("app.local.toy")));
    assert_eq!(
        bad.to_string(),
        "port from repo:local: \"eighty\" is not int"
    );
    // the control: the base's value refused names no layer
    let b = doc("port = \"eighty\"\n");
    let repo = [layer("app.toy", &b)];
    let bad = resolve(&rows, "widget", &NONE, sound(&repo), sound(&[]))
        .find_map(|r| {
            match r.err() {
                Maybe::Is(e) => Some(e),
                Maybe::Isnt => None,
            }
        })
        .unwrap();
    assert_eq!(bad.to_string(), "port from repo: \"eighty\" is not int");
}

#[test]
fn unknown_and_misplaced_keys_skip_meta_and_name_their_layer() {
    let rows = table();
    let (b, l) = (
        doc("meta.when = \"x\"\nport = 1\n"),
        doc("meta.replace = [\"port\"]\nport = 2\nprot = 3\ntheme = \"light\"\n"),
    );
    let repo = [layer("app.toy", &b), layer("app.local.toy", &l)];
    let unknown: Vec<(&str, &str)> = unknown_keys(&rows, &repo)
        .map(|(k, layer)| (k, layer.label()))
        .collect();
    assert_eq!(unknown, [("prot", "app.local.toy")]);
    let misplaced: Vec<(&str, &str)> = misplaced_keys(&rows, &repo)
        .map(|(k, layer)| (k, layer.label()))
        .collect();
    assert_eq!(misplaced, [("theme", "app.local.toy")]);
    // the control: a stack of declared keys and meta alone has neither
    let repo = [layer("app.toy", &b)];
    assert_eq!(unknown_keys(&rows, &repo).count(), 0);
    assert_eq!(misplaced_keys(&rows, &repo).count(), 0);
}

#[test]
fn a_repo_only_setting_is_not_read_from_the_persons_stack() {
    let rows = [Setting::<Int, Repo>::new("strict", "0", "").row::<Toy>()];
    let u = doc("strict = 1\n");
    let user = [layer("app.toy", &u)];
    let got = resolve(&rows, "widget", &NONE, sound(&[]), sound(&user))
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(
        (got.source(), got.to_string()),
        (Origin::Default, "0".to_string())
    );
}

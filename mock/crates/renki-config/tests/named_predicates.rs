//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Named predicates: their table, the caller's answers, and when one holds.

mod common;

use common::{Said, Toy, doc, layer};
use notko::{Maybe, Outcome};
use renki_config::{BadPredicate, Kind, Predicate, PredicateName, Predicates, holds, truthy};

const BASE: &str = "meta.predicates.cloud.env = \"CLOUD\"\n\
                    meta.predicates.built.shell = \"test -e built\"\n\
                    meta.predicates.local.not = \"cloud\"\n\
                    meta.predicates.both.all = [\"cloud\", \"built\"]\n\
                    meta.predicates.either.any = [\"cloud\", \"built\"]\n";

fn said<'a>(vars: &'a [(&'a str, &'a str)], exits: &'a [(&'a str, i32)]) -> Said<'a> {
    Said {
        vars,
        exits,
    }
}

#[test]
fn every_truthy_and_falsy_word_in_any_case_and_nothing_else() {
    for word in ["1", "true", "yes", "on", "TRUE", "Yes", "oN"] {
        assert_eq!(truthy(word), Maybe::Is(true), "{word}");
    }
    for word in ["", "0", "false", "no", "off", "FALSE", "No", "OFF"] {
        assert_eq!(truthy(word), Maybe::Is(false), "{word}");
    }
    for word in ["maybe", "2", " true", "y", "enabled"] {
        assert_eq!(truthy(word), Maybe::Isnt, "{word}");
    }
}

#[test]
fn the_table_reads_each_kind_of_definition() {
    let b = doc(BASE);
    let base = layer("app.toy", &b);
    let table = Predicates::read(&base, Maybe::Isnt).unwrap();
    assert_eq!(
        table.get("cloud"),
        Outcome::Ok(Maybe::Is(Predicate::Env("CLOUD")))
    );
    assert_eq!(
        table.get("built"),
        Outcome::Ok(Maybe::Is(Predicate::Shell("test -e built")))
    );
    assert_eq!(
        table.get("local"),
        Outcome::Ok(Maybe::Is(Predicate::Not("cloud")))
    );
    assert_eq!(
        table.get("both"),
        Outcome::Ok(Maybe::Is(Predicate::All("[\"cloud\", \"built\"]")))
    );
    assert_eq!(
        table.get("either"),
        Outcome::Ok(Maybe::Is(Predicate::Any("[\"cloud\", \"built\"]")))
    );
    assert_eq!(table.get("nope"), Outcome::Ok(Maybe::Isnt));
}

#[test]
fn an_env_predicate_holds_where_the_variable_is_truthy_and_refuses_another_value() {
    let b = doc(BASE);
    let base = layer("app.toy", &b);
    let table = Predicates::read(&base, Maybe::Isnt).unwrap();
    assert_eq!(
        holds("cloud", &table, &said(&[("CLOUD", "yes")], &[])),
        Outcome::Ok(true)
    );
    assert_eq!(
        holds("cloud", &table, &said(&[("CLOUD", "0")], &[])),
        Outcome::Ok(false)
    );
    assert_eq!(
        holds("cloud", &table, &said(&[], &[])),
        Outcome::Ok(false),
        "unset"
    );
    let answers = said(&[("CLOUD", "maybe")], &[]);
    let e = holds("cloud", &table, &answers).unwrap_err();
    assert_eq!(e, BadPredicate::Value("CLOUD", "maybe"));
    let shown = e.to_string();
    assert!(
        shown.contains("CLOUD") && shown.contains("maybe"),
        "{shown}"
    );
}

#[test]
fn a_shell_predicate_holds_on_zero_not_on_one_and_refuses_any_other_exit() {
    let b = doc(BASE);
    let base = layer("app.toy", &b);
    let table = Predicates::read(&base, Maybe::Isnt).unwrap();
    assert_eq!(
        holds("built", &table, &said(&[], &[("test -e built", 0)])),
        Outcome::Ok(true)
    );
    assert_eq!(
        holds("built", &table, &said(&[], &[("test -e built", 1)])),
        Outcome::Ok(false)
    );
    let answers = said(&[], &[("test -e built", 2)]);
    let e = holds("built", &table, &answers).unwrap_err();
    assert_eq!(e, BadPredicate::Exit("test -e built", 2));
    assert!(e.to_string().contains("test -e built") && e.to_string().contains('2'));
    // a command that did not start is refused the same, with no exit
    assert_eq!(
        holds("built", &table, &said(&[], &[])).err(),
        Maybe::Is(BadPredicate::NoExit("test -e built"))
    );
}

#[test]
fn not_all_and_any_combine_other_predicates_by_name() {
    let b = doc(BASE);
    let base = layer("app.toy", &b);
    let table = Predicates::read(&base, Maybe::Isnt).unwrap();
    for (cloud, built, local, both, either) in [
        ("1", 0, false, true, true),
        ("1", 1, false, false, true),
        ("0", 0, true, false, true),
        ("0", 1, true, false, false),
    ] {
        let vars = [("CLOUD", cloud)];
        let exits = [("test -e built", built)];
        let a = said(&vars, &exits);
        assert_eq!(
            holds("local", &table, &a),
            Outcome::Ok(local),
            "{cloud} {built}"
        );
        assert_eq!(
            holds("both", &table, &a),
            Outcome::Ok(both),
            "{cloud} {built}"
        );
        assert_eq!(
            holds("either", &table, &a),
            Outcome::Ok(either),
            "{cloud} {built}"
        );
    }
}

#[test]
fn a_name_the_table_does_not_define_is_refused_by_name() {
    // named by a definition, it is refused when the table is read
    let b = doc("meta.predicates.a.not = \"b\"\n");
    let base = layer("app.toy", &b);
    assert_eq!(
        Predicates::read(&base, Maybe::Isnt).err(),
        Maybe::Is(BadPredicate::Undefined("b"))
    );
    // named by a use, it is refused when the use is answered
    let b = doc("meta.predicates.a.not = \"b\"\nmeta.predicates.b.env = \"B\"\n");
    let base = layer("app.toy", &b);
    let table = Predicates::read(&base, Maybe::Isnt).unwrap();
    assert_eq!(
        holds("zed", &table, &said(&[], &[])).err(),
        Maybe::Is(BadPredicate::Undefined("zed"))
    );
    assert!(BadPredicate::Undefined("zed").to_string().contains("zed"));
}

#[test]
fn a_loop_among_not_all_and_any_is_refused_when_the_table_is_read_used_or_not() {
    let b = doc(
        "meta.predicates.a.all = [\"b\"]\nmeta.predicates.b.any = [\"c\"]\n\
         meta.predicates.c.not = \"a\"\n",
    );
    let base = layer("app.toy", &b);
    let e = Predicates::read(&base, Maybe::Isnt).unwrap_err();
    assert_eq!(e, BadPredicate::Loop("a", "c"));
    let shown = e.to_string();
    assert!(shown.contains("a") && shown.contains("c"), "{shown}");
    let b = doc("meta.predicates.s.not = \"s\"\n");
    let base = layer("app.toy", &b);
    assert_eq!(
        Predicates::read(&base, Maybe::Isnt).err(),
        Maybe::Is(BadPredicate::Loop("s", "s"))
    );
    // a loop through defaults, which nothing uses, is refused all the same,
    // the base's definitions walked first
    let (d, b) = (
        doc("meta.predicates.y.not = \"x\"\n"),
        doc("meta.predicates.x.any = [\"y\"]\nmeta.predicates.used.env = \"U\"\n"),
    );
    let (defaults, base) = (layer("app.defaults.toy", &d), layer("app.toy", &b));
    assert_eq!(
        Predicates::read(&base, Maybe::Is(&defaults)).err(),
        Maybe::Is(BadPredicate::Loop("x", "y"))
    );
    // the control: a name reached twice by two branches is no loop
    let b = doc("meta.predicates.x.env = \"X\"\nmeta.predicates.d.all = [\"x\", \"x\"]\n");
    let base = layer("app.toy", &b);
    let table = Predicates::read(&base, Maybe::Isnt).unwrap();
    assert_eq!(
        holds("d", &table, &said(&[("X", "1")], &[])),
        Outcome::Ok(true)
    );
}

#[test]
fn the_table_is_read_from_the_base_and_defaults_and_never_from_an_overlay() {
    let (d, b, c) = (
        doc("meta.predicates.x.env = \"X\"\n"),
        doc("meta.predicates.y.env = \"Y\"\n"),
        doc("meta.predicates.z.env = \"Z\"\n"),
    );
    let (defaults, base) = (layer("app.defaults.toy", &d), layer("app.toy", &b));
    let table = Predicates::read(&base, Maybe::Is(&defaults)).unwrap();
    let a = said(&[("X", "1"), ("Y", "1"), ("Z", "1")], &[]);
    assert_eq!(
        holds("x", &table, &a),
        Outcome::Ok(true),
        "defined in defaults"
    );
    assert_eq!(
        holds("y", &table, &a),
        Outcome::Ok(true),
        "defined in the base"
    );
    // the overlay's definition is not in the table, which has no way to take one
    let _overlay = layer("app.cloud.toy", &c);
    assert_eq!(
        holds("z", &table, &a).err(),
        Maybe::Is(BadPredicate::Undefined("z"))
    );
}

#[test]
fn a_definition_of_no_known_kind_or_shape_is_refused_by_name() {
    for text in [
        "meta.predicates.p.when = \"X\"\n",
        "meta.predicates.p.env = 1\n",
        "meta.predicates.p.all = \"x\"\n",
        "meta.predicates.p.env = \"X\"\nmeta.predicates.p.shell = \"true\"\n",
    ] {
        let b = doc(text);
        let base = layer("app.toy", &b);
        assert_eq!(
            Predicates::read(&base, Maybe::Isnt).err(),
            Maybe::Is(BadPredicate::Malformed("p")),
            "{text}"
        );
    }
}

#[test]
fn a_predicate_name_is_text_that_can_name_one() {
    assert_eq!(PredicateName::from_text::<Toy>("cloud").unwrap(), "cloud");
    assert_eq!(PredicateName::NAME, "predicate");
    assert_eq!(
        PredicateName::from_text::<Toy>("").unwrap_err().kind(),
        "predicate"
    );
    assert_eq!(
        PredicateName::from_text::<Toy>("a.b").unwrap_err().kind(),
        "predicate"
    );
    assert_eq!(
        PredicateName::from_literal::<Toy>(renki_config::Literal::Str("cloud")).unwrap(),
        "cloud"
    );
    assert!(PredicateName::from_literal::<Toy>(renki_config::Literal::Int(1)).is_err());
}

#[test]
fn a_refused_value_is_written_as_a_basic_string_as_the_dump_writes_one() {
    let e = BadPredicate::Value("V", "a\"b\\c\nd\te\rf\u{1}g\u{7f}h\u{e9}");
    assert_eq!(
        e.to_string(),
        "V is \"a\\\"b\\\\c\\nd\\te\\rf\\u0001g\\u007Fh\u{e9}\", which is neither true nor false"
    );
    // the control: plain text is only quoted
    assert_eq!(
        BadPredicate::Value("CLOUD", "maybe").to_string(),
        "CLOUD is \"maybe\", which is neither true nor false"
    );
}

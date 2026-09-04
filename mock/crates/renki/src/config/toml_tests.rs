//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use notko::Maybe;
use renki_config::{Literal, Rendered, Store};

use super::*;

fn doc(text: &str) -> Document<'_> {
    Toml::parse(text).unwrap()
}

#[test]
fn every_scalar_shape_is_read_under_a_dotted_key() {
    let d = doc("theme = \"dark\"\nstrict = true\n[server]\nport = 8787\n[model]\nbase = \"m\"\n");
    assert_eq!(Toml::get(&d, "theme"), Maybe::Is(Literal::Str("dark")));
    assert_eq!(Toml::get(&d, "strict"), Maybe::Is(Literal::Bool(true)));
    assert_eq!(Toml::get(&d, "server.port"), Maybe::Is(Literal::Int(8787)));
    assert_eq!(Toml::get(&d, "model.base"), Maybe::Is(Literal::Str("m")));
    // the control
    assert_eq!(Toml::get(&d, "server.host"), Maybe::Isnt);
    assert_eq!(Toml::get(&d, "nope"), Maybe::Isnt);
    // a table is not a value
    assert_eq!(Toml::get(&d, "server"), Maybe::Isnt);
}

#[test]
fn a_dotted_key_at_the_top_level_and_a_table_read_the_same() {
    let flat = doc("model.base = \"m\"\n");
    let nested = doc("[model]\nbase = \"m\"\n");
    assert_eq!(
        Toml::get(&flat, "model.base"),
        Toml::get(&nested, "model.base")
    );
    let inline = doc("model = { base = \"m\" }\n");
    assert_eq!(
        Toml::get(&inline, "model.base"),
        Maybe::Is(Literal::Str("m"))
    );
}

#[test]
fn a_list_is_handed_over_as_its_own_text_and_walked_back_by_the_store() {
    let d = doc("skip = [ \"a\", 'b', 3, true ] # trailing\n");
    let Maybe::Is(Literal::List(text)) = Toml::get(&d, "skip") else {
        panic!("not a list");
    };
    assert_eq!(text, "[ \"a\", 'b', 3, true ]");
    let items: Vec<Literal<'_>> = Toml::items(text).collect();
    assert_eq!(items, [
        Literal::Str("a"),
        Literal::Str("b"),
        Literal::Int(3),
        Literal::Bool(true)
    ]);
    // nested, and the empty one
    let d = doc("x = [[1, 2], []]\n");
    let Maybe::Is(Literal::List(text)) = Toml::get(&d, "x") else {
        panic!("not a list");
    };
    let items: Vec<Literal<'_>> = Toml::items(text).collect();
    assert_eq!(items, [Literal::List("[1, 2]"), Literal::List("[]")]);
    assert_eq!(Toml::items("[]").count(), 0);
}

#[test]
fn the_keys_are_every_dotted_leaf_in_document_order() {
    let d = doc(
        "theme = \"dark\"\n[server]\nport = 1\nhost = \"h\"\n[model]\nbase = \"m\"\nn = { k = 1 }\n",
    );
    let keys: Vec<&str> = Toml::keys(&d).collect();
    assert_eq!(keys, [
        "theme",
        "server.port",
        "server.host",
        "model.base",
        "model.n.k"
    ]);
    assert_eq!(Toml::keys(&doc("")).count(), 0);
}

#[test]
fn text_that_is_not_toml_is_refused_at_its_line() {
    let e = Toml::parse("a = 1\nb = \n").unwrap_err();
    assert_eq!(e.line(), Maybe::Is(2));
    let e = Toml::parse("= 1").unwrap_err();
    assert_eq!(e.line(), Maybe::Is(1));
}

fn set(text: &str, key: &str, value: Rendered<'_>) -> String {
    let mut out = String::new();
    Toml::set(text, key, value, &mut out).unwrap();
    out
}

#[test]
fn setting_a_key_keeps_every_other_byte_comments_included() {
    let text =
        "# the look\ntheme = \"dark\" # or light\n\n[server]\nport = 1 # local\nhost = \"h\"\n";
    assert_eq!(
        set(text, "theme", Rendered::Text("light")),
        "# the look\ntheme = \"light\" # or light\n\n[server]\nport = 1 # local\nhost = \"h\"\n"
    );
    assert_eq!(
        set(text, "server.port", Rendered::Raw("2")),
        "# the look\ntheme = \"dark\" # or light\n\n[server]\nport = 2 # local\nhost = \"h\"\n"
    );
    // a `#` inside the value is not a comment
    let text = "name = \"a # b\" # c\n";
    assert_eq!(set(text, "name", Rendered::Text("d")), "name = \"d\" # c\n");
}

#[test]
fn a_key_not_yet_in_the_file_is_added_where_a_reader_would_look() {
    // top level: at the end of the top-level section, before the first table
    let text = "theme = \"dark\"\n\n[server]\nport = 1\n";
    assert_eq!(
        set(text, "strict", Rendered::Raw("true")),
        "theme = \"dark\"\nstrict = true\n\n[server]\nport = 1\n"
    );
    // in an existing table: at the end of that table
    assert_eq!(
        set(text, "server.host", Rendered::Text("h")),
        "theme = \"dark\"\n\n[server]\nport = 1\nhost = \"h\"\n"
    );
    // in a table that does not exist yet: a new table at the end
    assert_eq!(
        set(text, "model.base", Rendered::Text("m")),
        "theme = \"dark\"\n\n[server]\nport = 1\n\n[model]\nbase = \"m\"\n"
    );
    // an empty file
    assert_eq!(
        set("", "theme", Rendered::Text("dark")),
        "theme = \"dark\"\n"
    );
    assert_eq!(
        set("", "model.base", Rendered::Text("m")),
        "[model]\nbase = \"m\"\n"
    );
    // a section header with nothing under it yet, at the end of the file
    assert_eq!(
        set("[model]\n", "model.base", Rendered::Text("m")),
        "[model]\nbase = \"m\"\n"
    );
}

#[test]
fn a_dotted_key_written_flat_is_found_and_replaced_flat() {
    let text = "model.base = \"m\"\n";
    assert_eq!(
        set(text, "model.base", Rendered::Text("n")),
        "model.base = \"n\"\n"
    );
}

#[test]
fn what_set_writes_parses_back_to_what_was_set() {
    // the round trip, including the characters a basic string escapes
    for value in ["plain", "with \"quotes\"", "back\\slash", "two\nlines", "tab\there"] {
        let out = set("", "k", Rendered::Text(value));
        let d = doc(&out);
        assert_eq!(
            Toml::get(&d, "k"),
            Maybe::Is(Literal::Str(value)),
            "{out:?}"
        );
    }
    let out = set("", "k", Rendered::Raw("[\"a\", \"b\"]"));
    let d = doc(&out);
    let Maybe::Is(Literal::List(t)) = Toml::get(&d, "k") else {
        panic!("not a list")
    };
    assert_eq!(Toml::items(t).count(), 2);
}

#[test]
fn a_comment_is_split_off_only_outside_a_string() {
    assert_eq!(split_comment("a = 1 # c"), ("a = 1 ", "# c"));
    assert_eq!(split_comment("a = \"#\" # c"), ("a = \"#\" ", "# c"));
    assert_eq!(split_comment("a = '#'"), ("a = '#'", ""));
    assert_eq!(
        split_comment("a = \"\\\"#\" # c"),
        ("a = \"\\\"#\" ", "# c")
    );
    assert_eq!(split_comment("plain"), ("plain", ""));
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The TOML store: what it answers of a document, what it spells into its
//! spelling shelf, how a short shelf is refused, and what its writer writes,
//! each writer fixture under `tests/writer/` included.

use std::fs;

use notko::lend::Exhausted;
use notko::{Maybe, Outcome};
use renki_config::{Literal, Rendered, Shape, ShortShelf, Store, StoreShelf, Walk};
use renki_manifest::{Toml, TomlDocument};
use tomlfuse_parse::{Node, nodes_for};

/// Shelves the text cannot outgrow, leaked for the test's length.
fn read(text: &'static str) -> TomlDocument<'static> {
    let nodes: &mut [Node] = Box::leak(vec![Node::EMPTY; nodes_for(text.len())].into_boxed_slice());
    let spelling: &mut [u8] = Box::leak(vec![0u8; 4 * text.len() + 64].into_boxed_slice());
    let shelves = Box::leak(Box::new((nodes, spelling)));
    Toml::parse(text, shelves).unwrap().unwrap()
}

fn keys(doc: &TomlDocument<'_>) -> Vec<String> {
    let mut walk = Toml::keys(doc);
    let mut out = Vec::new();
    while let Maybe::Is(k) = walk.step() {
        out.push(k.to_owned());
    }
    out
}

fn items(doc: &'static TomlDocument<'static>, key: &str) -> Vec<Literal<'static, Toml>> {
    let Maybe::Is(Literal::List(list)) = Toml::get(doc, key) else {
        panic!("{key} is no list");
    };
    let mut walk = Toml::items(list);
    let mut out = Vec::new();
    while let Maybe::Is(l) = walk.step() {
        out.push(l);
    }
    out
}

#[test]
fn every_key_is_spelled_whole_with_its_tables_and_an_array_of_tables_index() {
    let doc = read(
        "a.b = 1\nlist = [1, 2]\n[t]\n\"x y\" = 2\n'q.r' = 3\n[[arr]]\nk = 1\n[[arr]]\nk = 2\n[[arr.sub]]\nm = 3\n",
    );
    assert_eq!(keys(&doc), [
        "a.b",
        "list",
        "t.x y",
        "t.q.r",
        "arr.0.k",
        "arr.1.k",
        "arr.1.sub.0.m"
    ]);
    // the control: a document with no keys spells none
    assert!(keys(&read("[t]\n")).is_empty());
}

#[test]
fn a_value_is_answered_as_the_literal_of_its_kind() {
    let doc: &'static TomlDocument<'static> = Box::leak(Box::new(read(
        "n = 7\nb = true\ns = 'x'\nl = [1, 2]\n[[arr]]\nk = \"one\"\n[[arr]]\nk = \"two\"\n[t]\nu = 1\n",
    )));
    assert_eq!(Toml::get(doc, "n"), Maybe::Is(Literal::Int(7)));
    assert_eq!(Toml::get(doc, "b"), Maybe::Is(Literal::Bool(true)));
    assert_eq!(Toml::get(doc, "s"), Maybe::Is(Literal::Str("x")));
    assert_eq!(Toml::get(doc, "arr.1.k"), Maybe::Is(Literal::Str("two")));
    assert_eq!(items(doc, "l"), [Literal::Int(1), Literal::Int(2)]);
    // a table answers nothing, and neither does a key the document lacks
    assert_eq!(Toml::get(doc, "t"), Maybe::Isnt);
    assert_eq!(Toml::get(doc, "nope"), Maybe::Isnt);
    assert_eq!(Toml::get(doc, "arr.2.k"), Maybe::Isnt);
}

#[test]
fn every_shape_is_told_apart() {
    let doc = read(
        "s = 1\nl = [1]\ne = []\ni = [{ a = 1 }, { a = 2 }]\nm = [1, { a = 1 }]\n[t]\n[[aot]]\nk = 1\n",
    );
    let shape = |k| Toml::shape(&doc, k);
    assert_eq!(shape("s"), Maybe::Is(Shape::Scalar));
    assert_eq!(shape("l"), Maybe::Is(Shape::List));
    assert_eq!(shape("e"), Maybe::Is(Shape::Empty));
    assert_eq!(shape("i"), Maybe::Is(Shape::Tables));
    assert_eq!(shape("m"), Maybe::Is(Shape::List));
    assert_eq!(shape("t"), Maybe::Is(Shape::Table));
    assert_eq!(shape("aot"), Maybe::Is(Shape::Tables));
    assert_eq!(shape("nope"), Maybe::Isnt);
}

#[test]
fn every_escape_is_lent_decoded_from_the_spelling_shelf() {
    let doc: &'static TomlDocument<'static> = Box::leak(Box::new(read(
        "a = \"\\b\\t\\n\\f\\r\\e\\\"\\\\\\x41\\u00e9\\U0001F600\"\nml = \"\"\"\none \\\n   two\"\"\"\nl = [\"\\u00e9\", 'raw\\n']\nplain = \"no escape\"\n",
    )));
    assert_eq!(
        Toml::get(doc, "a"),
        Maybe::Is(Literal::Str("\u{8}\t\n\u{c}\r\u{1b}\"\\A\u{e9}\u{1F600}"))
    );
    assert_eq!(Toml::get(doc, "ml"), Maybe::Is(Literal::Str("one two")));
    assert_eq!(items(doc, "l"), [
        Literal::Str("\u{e9}"),
        Literal::Str("raw\\n")
    ]);
    // a string with no escape is lent from the text itself
    let Maybe::Is(Literal::Str(plain)) = Toml::get(doc, "plain") else {
        panic!("plain is a string");
    };
    let text = Toml::document(doc).text();
    let within = text.as_ptr() as usize .. text.as_ptr() as usize + text.len();
    assert!(within.contains(&(plain.as_ptr() as usize)));
}

#[test]
fn a_short_shelf_is_refused_naming_it_and_the_whole_need_of_the_spelling() {
    let text = "[build]\njobs = 4\nname = \"a\\tb\"\n";
    let parse = |nodes: usize, bytes: usize| {
        let (mut n, mut s) = (vec![Node::EMPTY; nodes], vec![0u8; bytes]);
        let mut shelves = (&mut n[..], &mut s[..]);
        match Toml::parse(text, &mut shelves) {
            Outcome::Ok(_) => None,
            Outcome::Err(e) => Some(e),
        }
    };
    let need = {
        // the decoded string at the place its source starts, then the keys
        let at = text.find("\"a").unwrap();
        at + "\"a\\tb\"".len() + "build.jobs\nbuild.name\n".len()
    };
    assert_eq!(
        parse(64, need - 1),
        Some(ShortShelf::new(StoreShelf::Spelling, Exhausted {
            wanted: need,
            had:    need - 1,
        }))
    );
    assert_eq!(parse(64, need), None, "the whole need is enough");
    // the index is the parser's: one more than it held
    let short = parse(1, need).expect("one node is too few");
    assert_eq!(short.shelf(), StoreShelf::Index);
    assert_eq!(short.exhausted().had, 1);
    assert_eq!(short.exhausted().wanted, 2);
}

#[test]
fn a_document_that_is_no_toml_is_refused_at_its_line_apart_from_a_short_shelf() {
    let text = "a = 1\nb = = 2\n";
    let (mut n, mut s) = (vec![Node::EMPTY; 16], vec![0u8; 64]);
    let mut shelves = (&mut n[..], &mut s[..]);
    let Outcome::Ok(Outcome::Err(bad)) = Toml::parse(text, &mut shelves) else {
        panic!("b = = 2 is no TOML");
    };
    assert_eq!(bad.line(), Maybe::Is(2));
}

#[test]
fn every_writer_fixture_writes_what_it_holds() {
    let mut ran = 0;
    for entry in fs::read_dir("tests/writer").expect("the writer fixtures") {
        let dir = entry.unwrap().path();
        let edit = fs::read_to_string(dir.join("edit")).unwrap();
        let mut lines = edit.lines();
        let (key, kind, value) = (
            lines.next().unwrap(),
            lines.next().unwrap(),
            lines.next().unwrap_or(""),
        );
        let input: &'static str = Box::leak(
            fs::read_to_string(dir.join("input.toml"))
                .unwrap()
                .into_boxed_str(),
        );
        let expected = fs::read_to_string(dir.join("expected.toml")).unwrap();
        let rendered = match kind {
            "text" => Rendered::Text(value),
            "raw" => Rendered::Raw(value),
            other => panic!("{}: no kind {other}", dir.display()),
        };
        let doc = read(input);
        let mut out = String::new();
        Toml::set(&doc, key, rendered, &mut out).unwrap();
        assert_eq!(out, expected, "{}", dir.display());
        ran += 1;
    }
    assert_eq!(ran, 7, "every fixture is read");
}

/// `text` with `key` set to `value` by the store.
fn set(text: &str, key: &str, value: Rendered<'_>) -> String {
    let text: &'static str = Box::leak(text.to_owned().into_boxed_str());
    let doc = read(text);
    let mut out = String::new();
    Toml::set(&doc, key, value, &mut out).unwrap();
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
    // a top-level key in a file that opens with a header: above the header,
    // never inside the last table, where it would read back as `model.theme`
    assert_eq!(
        set("[model]\nbase = \"m\"\n", "theme", Rendered::Text("dark")),
        "theme = \"dark\"\n\n[model]\nbase = \"m\"\n"
    );
    assert_eq!(
        set(
            "[server]\nport = 1\n\n[model]\nbase = \"m\"\n",
            "strict",
            Rendered::Raw("true")
        ),
        "strict = true\n\n[server]\nport = 1\n\n[model]\nbase = \"m\"\n"
    );
    // the same with a comment above the header: after the comment, since a
    // comment is not a line of the top level's own
    assert_eq!(
        set(
            "# mine\n[model]\nbase = \"m\"\n",
            "theme",
            Rendered::Text("dark")
        ),
        "# mine\ntheme = \"dark\"\n\n[model]\nbase = \"m\"\n"
    );
    // a file of comments only, and no table at all
    assert_eq!(
        set("# mine\n", "theme", Rendered::Text("dark")),
        "# mine\ntheme = \"dark\"\n"
    );
    // and the control the fix must not disturb: a top-level line exists, so
    // the key follows it and the header is untouched
    assert_eq!(
        set(
            "a = 1\n[model]\nbase = \"m\"\n",
            "theme",
            Rendered::Text("dark")
        ),
        "a = 1\ntheme = \"dark\"\n[model]\nbase = \"m\"\n"
    );
}

#[test]
fn what_set_writes_parses_back_to_what_was_set() {
    // the round trip, including the characters a basic string escapes
    for value in ["plain", "with \"quotes\"", "back\\slash", "two\nlines", "tab\there"] {
        let out = set("", "k", Rendered::Text(value));
        let d: &'static TomlDocument<'static> =
            Box::leak(Box::new(read(Box::leak(out.clone().into_boxed_str()))));
        assert_eq!(
            Toml::get(&d, "k"),
            Maybe::Is(Literal::Str(value)),
            "{out:?}"
        );
    }
    let out = set("", "k", Rendered::Raw("[\"a\", \"b\"]"));
    let d: &'static TomlDocument<'static> =
        Box::leak(Box::new(read(Box::leak(out.clone().into_boxed_str()))));
    let Maybe::Is(Literal::List(t)) = Toml::get(&d, "k") else {
        panic!("not a list")
    };
    let mut walk = Toml::items(t);
    let mut n = 0;
    while let Maybe::Is(_) = walk.step() {
        n += 1;
    }
    assert_eq!(n, 2);
}

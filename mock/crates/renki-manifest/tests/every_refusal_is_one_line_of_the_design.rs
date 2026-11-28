//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Every refusal is stated once, in the designs' tables under `The
//! refusals`, and held to it from three sides: each `Display` of the
//! contracts and of the host is its line of the table; each fixture's
//! `refused` file is one line of the table, and where the parser refuses one
//! of the fixture's files, the parser's own line; and each line of the table
//! but those the design names as held elsewhere has a fixture.

mod common;

use std::collections::BTreeMap;
use std::ffi::CStr;
use std::fs;

use common::{Fixture, suite};
use renki_config::{BadLayer, BadMerge, BadPredicate, Got, MetaList, Shape};
use renki_manifest::BadRead;
use tomlfuse_parse::{Node, Options, Version, nodes_for, parse};

const CONTRACTS: &str = "../renki-config/DESIGN.md.tmpl";
const READER: &str = "DESIGN.md.tmpl";

const SHAPES: [Shape; 5] = [Shape::Table, Shape::Tables, Shape::List, Shape::Empty, Shape::Scalar];
const GOTS: [Got<'static>; 3] = [Got::Bool, Got::Int, Got::List];
const FORMS: [&str; 7] = [
    "a multi-line string",
    "an inline table",
    "a nested array",
    "a quoted key",
    "a float",
    "a date or time",
    "a \\u escape",
];

/// The `text` blocks under `## The refusals` of the design at `path`, each as
/// its lines.
fn blocks(path: &str) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).expect("the design is read");
    let at = text
        .find("\n## The refusals\n")
        .expect("the design has its refusals");
    let section = &text[at + 1 ..];
    let section = &section[.. section[3 ..].find("\n## ").map_or(section.len(), |e| e + 3)];
    let mut out = Vec::new();
    let mut open: Option<Vec<String>> = None;
    for line in section.lines() {
        match (&mut open, line) {
            (None, "```text") => open = Some(Vec::new()),
            (Some(_), "```") => out.push(open.take().unwrap()),
            (Some(lines), l) => lines.push(l.to_owned()),
            (None, _) => {},
        }
    }
    assert!(open.is_none(), "{path}: a block is not closed");
    out
}

/// The contracts' table, each variant to its text.
fn contract_table() -> BTreeMap<String, String> {
    let lines = blocks(CONTRACTS).remove(0);
    let mut table = BTreeMap::new();
    for line in lines {
        let (id, text) = line
            .split_once(' ')
            .expect("a line is a variant and its text");
        assert!(
            table
                .insert(id.to_owned(), text.trim_start().to_owned())
                .is_none(),
            "{id} is in the table twice"
        );
    }
    table
}

/// The reader's two tables, each variant of the contracts named in a line
/// replaced by its text: the lines a fixture holds, and the lines held
/// elsewhere.
fn reader_tables() -> (Vec<String>, Vec<String>) {
    let contracts = contract_table();
    let expand = |line: &String| {
        let mut out = line.clone();
        for (id, text) in &contracts {
            if line.split(' ').any(|w| w == id) {
                out = out.replace(id.as_str(), text);
            }
        }
        out
    };
    let mut b = blocks(READER);
    assert_eq!(
        b.len(),
        2,
        "the reader's design has its table and the lines held elsewhere"
    );
    let elsewhere = b.remove(1).iter().map(expand).collect();
    let pinned = b.remove(0).iter().map(expand).collect();
    (pinned, elsewhere)
}

/// What each placeholder is filled with to render a sample.
const FILLED: [(&str, &str); 12] = [
    ("<file>", "app.x.toml"),
    ("<word>", "w0rd"),
    ("<key>", "meta.overlays.order"),
    ("<got>", "an integer"),
    ("<higher>", "an array of tables"),
    ("<lower>", "a list of values"),
    ("<name>", "n4me"),
    ("<from>", "fr0m"),
    ("<var>", "V4R"),
    ("<value>", "\"v\""),
    ("<command>", "c0mmand"),
    ("<code>", "7"),
];

fn fill(template: &str) -> String {
    let mut out = template.to_owned();
    for (hole, with) in FILLED {
        out = out.replace(hole, with);
    }
    out
}

/// The variant of a refusal of the contracts, by name; no arm left out, so
/// a variant added is a variant this test names.
fn layer_id(e: &BadLayer<'_>) -> &'static str {
    match e {
        BadLayer::TwoWords(_) => "BadLayer::TwoWords",
        BadLayer::NamedBase(_) => "BadLayer::NamedBase",
        BadLayer::BadWord(_) => "BadLayer::BadWord",
        BadLayer::Unordered(_) => "BadLayer::Unordered",
        BadLayer::OrderNames(_) => "BadLayer::OrderNames",
        BadLayer::OrderTwice(_) => "BadLayer::OrderTwice",
        BadLayer::NotAWord(..) => "BadLayer::NotAWord",
        BadLayer::NotAString(..) => "BadLayer::NotAString",
        BadLayer::UnknownMeta(_) => "BadLayer::UnknownMeta",
        BadLayer::MetaShape(_) => "BadLayer::MetaShape",
        BadLayer::AboveBase(_) => "BadLayer::AboveBase",
        BadLayer::AlwaysRead(_) => "BadLayer::AlwaysRead",
        BadLayer::ReplaceNotHeld(_) => "BadLayer::ReplaceNotHeld",
    }
}

fn merge_id(e: &BadMerge<'_>) -> &'static str {
    match e {
        BadMerge::Shapes(..) => "BadMerge::Shapes",
        BadMerge::Unset(_) => "BadMerge::Unset",
    }
}

fn predicate_id(e: &BadPredicate<'_>) -> &'static str {
    match e {
        BadPredicate::Undefined(_) => "BadPredicate::Undefined",
        BadPredicate::Loop(..) => "BadPredicate::Loop",
        BadPredicate::Value(..) => "BadPredicate::Value",
        BadPredicate::Exit(_, code) if Option::<i32>::from(*code).is_some() => "BadPredicate::Exit",
        BadPredicate::Exit(..) => "BadPredicate::Exit(none)",
        BadPredicate::Malformed(_) => "BadPredicate::Malformed",
    }
}

/// One of every refusal of the contracts, each with its variant's name and
/// its `Display`, made of the values [`FILLED`] names.
fn contract_samples() -> Vec<(&'static str, String)> {
    let (file, word, key) = ("app.x.toml", "w0rd", "meta.overlays.order");
    let layers = [
        BadLayer::TwoWords(file),
        BadLayer::NamedBase(file),
        BadLayer::BadWord(file),
        BadLayer::Unordered(word),
        BadLayer::OrderNames(word),
        BadLayer::OrderTwice(word),
        BadLayer::NotAWord(MetaList::Order, Got::Int),
        BadLayer::NotAString(MetaList::Order, Got::Int),
        BadLayer::UnknownMeta(key),
        BadLayer::MetaShape(key),
        BadLayer::AboveBase(key),
        BadLayer::AlwaysRead(key),
        BadLayer::ReplaceNotHeld(key),
    ];
    let merges = [BadMerge::Shapes(Shape::List, Shape::Tables), BadMerge::Unset(key)];
    let predicates = [
        BadPredicate::Undefined("n4me"),
        BadPredicate::Loop("n4me", "fr0m"),
        BadPredicate::Value("V4R", "v"),
        BadPredicate::Exit("c0mmand", Some(7).into()),
        BadPredicate::Exit("c0mmand", None.into()),
        BadPredicate::Malformed("n4me"),
    ];
    layers
        .iter()
        .map(|e| (layer_id(e), e.to_string()))
        .chain(merges.iter().map(|e| (merge_id(e), e.to_string())))
        .chain(predicates.iter().map(|e| (predicate_id(e), e.to_string())))
        .collect()
}

#[test]
fn every_display_of_the_contracts_is_its_line_of_the_table() {
    let table = contract_table();
    let samples = contract_samples();
    for (id, said) in &samples {
        let line = table
            .get(*id)
            .unwrap_or_else(|| panic!("{id} has no line in the contracts' table"));
        assert_eq!(said, &fill(line), "{id}");
    }
    let sampled: Vec<&str> = samples.iter().map(|(id, _)| *id).collect();
    for id in table.keys() {
        assert!(
            sampled.contains(&id.as_str()),
            "the table names {id}, which is no variant"
        );
    }
}

#[test]
fn the_names_a_refusal_writes_are_the_tables_own() {
    let names: Vec<&str> = SHAPES.iter().map(|s| s.name()).collect();
    assert_eq!(names, [
        "a table",
        "an array of tables",
        "a list of values",
        "an empty list",
        "a scalar"
    ]);
    let gots: Vec<String> = GOTS.iter().map(ToString::to_string).collect();
    assert_eq!(gots, ["a boolean", "an integer", "a list"]);
}

/// The line of the reader's tables a refusal of the host is, by variant; no
/// arm left out.
fn host_line(e: &BadRead<'_>) -> &'static str {
    match e {
        BadRead::Room {
            ..
        } => "<file> needs a room of <need> bytes",
        BadRead::Os {
            ..
        } => "<file> cannot be read",
        BadRead::NotAFile {
            ..
        } => "<file> is not a regular file",
    }
}

#[test]
fn every_display_of_the_host_is_its_line_of_the_table() {
    let (pinned, elsewhere) = reader_tables();
    let file: &CStr = c"app.x.toml";
    for e in [
        BadRead::Room {
            file,
            need: 7,
        },
        BadRead::Os {
            file,
            call: "open",
            errno: 2,
        },
        BadRead::NotAFile {
            file,
        },
        BadRead::Shell {
            dir:   file,
            call:  "posix_spawn",
            errno: 2,
        },
    ] {
        let line = host_line(&e);
        assert!(
            pinned.iter().chain(&elsewhere).any(|l| l == line),
            "{line} is no line of the reader's table"
        );
        let filled = fill(line)
            .replace("<need>", "7")
            .replace("<dir>", "app.x.toml");
        assert_eq!(e.to_string(), filled, "{e:?}");
    }
}

/// Whether `text` is an instance of `template`, each placeholder filled with
/// what it may hold.
fn instance(template: &str, text: &str) -> bool {
    let Some(open) = template.find('<') else {
        return template == text;
    };
    let Some(close) = template[open ..].find('>').map(|c| open + c) else {
        return template == text;
    };
    let (lit, hole, rest) = (
        &template[.. open],
        &template[open ..= close],
        &template[close + 1 ..],
    );
    let Some(after) = text.strip_prefix(lit) else {
        return false;
    };
    (1 ..= after.len())
        .filter(|&k| after.is_char_boundary(k))
        .any(|k| fits(hole, &after[.. k]) && instance(rest, &after[k ..]))
}

/// Whether `held` may stand for the placeholder `hole`.
fn fits(hole: &str, held: &str) -> bool {
    match hole {
        "<line>" | "<code>" | "<need>" => held.bytes().all(|b| b.is_ascii_digit()),
        "<higher>" | "<lower>" => SHAPES.iter().any(|s| s.name() == held),
        "<got>" => GOTS.iter().any(|g| g.to_string() == held),
        "<form>" => FORMS.contains(&held),
        // checked against the parser itself, never by its shape
        "<parser>" => false,
        "<file>" => !held.contains(['\n', ':']),
        _ => !held.contains('\n'),
    }
}

/// What the parser says of each of `fixture`'s TOML files it refuses, as the
/// reader writes it.
fn parser_refusals(fixture: &Fixture) -> Vec<String> {
    let mut out = Vec::new();
    for (name, path) in fixture.files() {
        if !name.ends_with(".toml") && name != fixture.base {
            continue;
        }
        let bytes = fs::read(&path).expect("a fixture's file is read");
        let mut nodes = vec![Node::EMPTY; nodes_for(bytes.len())];
        if let Err(e) = parse(&bytes, Options::new(Version::V1_1_0), &mut nodes, &mut ()) {
            let line = 1 + bytes[.. e.at.min(bytes.len())]
                .iter()
                .filter(|b| **b == b'\n')
                .count();
            out.push(format!("{name}:{line}: {}", e.kind));
        }
    }
    out
}

/// Every fixture's refusal against the tables: which line of them each is,
/// and what is wrong where one is not exactly one line.
fn fixtures_against_the_tables() -> (BTreeMap<String, usize>, Vec<String>) {
    let (pinned, elsewhere) = reader_tables();
    let mut held: BTreeMap<String, usize> = pinned.iter().map(|l| (l.clone(), 0)).collect();
    let mut wrong = Vec::new();
    for fixture in suite() {
        let refusals = parser_refusals(&fixture);
        let Some(refused) = &fixture.refused else {
            if !refusals.is_empty() {
                wrong.push(format!(
                    "{}: the parser refuses {refusals:?}, and the fixture holds a dump",
                    fixture.name
                ));
            }
            continue;
        };
        let line = refused.trim_end_matches('\n');
        let by_the_parser = refusals.iter().any(|r| r == line);
        if !refusals.is_empty() && fixture.dump.is_none() && !by_the_parser {
            wrong.push(format!(
                "{}: the parser refuses {refusals:?}, and the fixture wants {line:?}",
                fixture.name
            ));
            continue;
        }
        let lines: Vec<&String> = pinned
            .iter()
            .chain(&elsewhere)
            .filter(|t| {
                if t.contains("<parser>") {
                    by_the_parser
                } else {
                    !by_the_parser && instance(t, line)
                }
            })
            .collect();
        match lines.as_slice() {
            [one] if held.contains_key(*one) => *held.get_mut(*one).unwrap() += 1,
            [one] => {
                wrong.push(format!(
                    "{}: {line:?} is {one:?}, which the design holds elsewhere",
                    fixture.name
                ));
            },
            [] => {
                wrong.push(format!(
                    "{}: {line:?} is no line of the table",
                    fixture.name
                ))
            },
            many => wrong.push(format!("{}: {line:?} is each of {many:?}", fixture.name)),
        }
    }
    (held, wrong)
}

#[test]
fn every_fixture_refuses_with_one_line_of_the_table() {
    let (_, wrong) = fixtures_against_the_tables();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_line_of_the_table_but_those_held_elsewhere_has_a_fixture() {
    let (held, _) = fixtures_against_the_tables();
    let bare: Vec<&String> = held
        .iter()
        .filter(|(_, n)| **n == 0)
        .map(|(l, _)| l)
        .collect();
    assert!(bare.is_empty(), "no fixture holds {bare:#?}");
}

#[test]
fn a_placeholder_holds_only_what_it_names() {
    assert!(instance(
        "<file>:<line>: <key> holds <got>, which is no word",
        "app.toml:3: k holds an integer, which is no word"
    ));
    assert!(!instance(
        "<file>:<line>: <key> holds <got>, which is no word",
        "app.toml:3: k holds a table, which is no word"
    ));
    assert!(!instance("<file>:<line>: x", "app.toml:three: x"));
    assert!(!instance("<file>: x", "app.toml:1: y"));
    assert!(instance(
        "<higher> at <key> over <lower>",
        "an empty list at a.b over a scalar"
    ));
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The contracts, exercised over a toy store so nothing here depends on a
//! file format: one `key = value` per line, strings quoted, lists in square
//! brackets, `#` to the end of a line a comment. What the toy store lacks is
//! the point: a store is the contract and nothing more, and the resolver,
//! the kinds and the rows run over this one exactly as over TOML.

use core::fmt;

use notko::{Maybe, Outcome};
use renki_config::{
    BadDocument,
    BadTable,
    Bool,
    Choice,
    Declared,
    Either,
    EnvKey,
    Got,
    Int,
    Kind,
    List,
    Literal,
    Lookup,
    PathText,
    Rendered,
    Repo,
    Setting,
    Source,
    Store,
    Text,
    TextItems,
    User,
    Value,
    choices,
    key_is_wellformed,
    misplaced_keys,
    resolve,
    unknown_keys,
};

// --- the toy store --------------------------------------------------------

struct Toy;

#[derive(Debug)]
struct ToyDoc<'t> {
    text: &'t str,
}

fn erred<'a, T>(r: Outcome<T, renki_config::BadConfig<'a>>) -> Option<renki_config::BadConfig<'a>> {
    match r.err() {
        Maybe::Is(e) => Some(e),
        Maybe::Isnt => None,
    }
}

/// `key = value` lines, comments stripped, blank lines skipped.
fn lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines()
        .map(|l| l.split_once('#').map_or(l, |(a, _)| a).trim())
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim(), v.trim())))
}

fn literal(raw: &str) -> Literal<'_> {
    if raw == "true" {
        Literal::Bool(true)
    } else if raw == "false" {
        Literal::Bool(false)
    } else if let Ok(i) = raw.parse::<i64>() {
        Literal::Int(i)
    } else if raw.starts_with('[') {
        Literal::List(raw)
    } else {
        Literal::Str(raw.trim_matches('"'))
    }
}

impl Store for Toy {
    type Document<'t> = ToyDoc<'t>;
    type Items<'d> = ToyItems<'d>;
    type Keys<'d> = ToyKeys<'d>;

    const EXTENSION: &'static str = "toy";
    const NAME: &'static str = "toy";

    fn parse<'t>(text: &'t str) -> Outcome<ToyDoc<'t>, BadDocument> {
        for (i, l) in text.lines().enumerate() {
            let l = l.split_once('#').map_or(l, |(a, _)| a).trim();
            if !l.is_empty() && !l.contains('=') {
                return Outcome::Err(BadDocument::at(Maybe::Is(i as u32 + 1)));
            }
        }
        Outcome::Ok(ToyDoc {
            text,
        })
    }

    fn get<'d>(doc: &'d ToyDoc<'_>, key: &str) -> Maybe<Literal<'d>> {
        match lines(doc.text).find(|(k, _)| *k == key) {
            Some((_, v)) => Maybe::Is(literal(v)),
            None => Maybe::Isnt,
        }
    }

    fn keys<'d>(doc: &'d ToyDoc<'_>) -> ToyKeys<'d> {
        ToyKeys(Box::new(lines(doc.text).map(|(k, _)| k)))
    }

    fn items<'d>(list: &'d str) -> ToyItems<'d> {
        ToyItems(TextItems::over(list).unwrap())
    }

    fn set(text: &str, key: &str, value: Rendered<'_>, into: &mut impl fmt::Write) -> fmt::Result {
        let rendered = match value {
            Rendered::Text(t) => format!("\"{t}\""),
            Rendered::Raw(t) => t.to_string(),
        };
        let mut found = false;
        for l in text.lines() {
            let (body, comment) = l.split_once('#').map_or((l, ""), |(a, b)| (a, b));
            match body.split_once('=') {
                Some((k, _)) if k.trim() == key => {
                    found = true;
                    write!(into, "{key} = {rendered}")?;
                    if !comment.is_empty() {
                        write!(into, " #{comment}")?;
                    }
                    writeln!(into)?;
                },
                _ => writeln!(into, "{l}")?,
            }
        }
        if !found {
            writeln!(into, "{key} = {rendered}")?;
        }
        Ok(())
    }
}

struct ToyKeys<'d>(Box<dyn Iterator<Item = &'d str> + 'd>);
impl<'d> Iterator for ToyKeys<'d> {
    type Item = &'d str;

    fn next(&mut self) -> Option<&'d str> {
        self.0.next()
    }
}

struct ToyItems<'d>(TextItems<'d>);
impl<'d> Iterator for ToyItems<'d> {
    type Item = Literal<'d>;

    fn next(&mut self) -> Option<Literal<'d>> {
        self.0.next().map(literal)
    }
}

// --- the table ------------------------------------------------------------

choices!(Theme = "dark" | "light");

fn table() -> [Declared<Toy>; 6] {
    [
        Setting::<Choice<Theme>, User>::new("theme", "dark", "Which look the pages use.").row(),
        Setting::<Int, Either>::new("server.port", "8787", "The port the server listens on.").row(),
        Setting::<Bool, Repo>::new("strict", "false", "Whether the gate refuses on warnings.")
            .row(),
        Setting::<Text, User>::new("model.base", "", "The model the scorer runs on.").row(),
        Setting::<PathText, User>::new("root", "/srv", "Where the corpus is.").row(),
        Setting::<List<Text>, Either>::new("skip", "[]", "Directories the scan never enters.")
            .row(),
    ]
}

struct Cli<'a> {
    flags: &'a [(&'a str, &'a str)],
    env:   &'a [(&'a str, &'a str)],
}

impl Lookup for Cli<'_> {
    fn flag<'s>(&'s self, key: &str) -> Maybe<&'s str> {
        match self.flags.iter().find(|(k, _)| *k == key) {
            Some((_, v)) => Maybe::Is(v),
            None => Maybe::Isnt,
        }
    }

    fn env<'s>(&'s self, name: EnvKey<'_>) -> Maybe<&'s str> {
        let name = name.to_string();
        match self.env.iter().find(|(k, _)| *k == name) {
            Some((_, v)) => Maybe::Is(v),
            None => Maybe::Isnt,
        }
    }
}

fn doc(text: &str) -> ToyDoc<'_> {
    Toy::parse(text).unwrap()
}

fn all<'a>(
    rows: &'a [Declared<Toy>],
    cli: &'a Cli<'a>,
    repo: Maybe<&'a ToyDoc<'a>>,
    user: Maybe<&'a ToyDoc<'a>>,
) -> Vec<(&'static str, String, Source)> {
    resolve(rows, "widget", cli, repo, user)
        .map(|r| {
            let r = r.unwrap();
            (r.row().key(), r.to_string(), r.source())
        })
        .collect()
}

// --- the kinds ------------------------------------------------------------

#[test]
fn every_kind_parses_its_text_and_refuses_the_rest_by_name() {
    assert!(Bool::from_text::<Toy>("true").unwrap());
    assert!(!Bool::from_text::<Toy>(" false ").unwrap());
    let e = Bool::from_text::<Toy>("yes").unwrap_err();
    assert_eq!(e.kind(), "bool");
    assert_eq!(e.got(), Got::Text("yes"));

    assert_eq!(Int::from_text::<Toy>("-12").unwrap(), -12);
    assert_eq!(Int::from_text::<Toy>("12.5").unwrap_err().kind(), "int");

    assert_eq!(Text::from_text::<Toy>("").unwrap(), "");
    assert_eq!(PathText::from_text::<Toy>("").unwrap_err().kind(), "path");
    assert_eq!(PathText::from_text::<Toy>("~/x").unwrap(), "~/x");

    assert_eq!(Choice::<Theme>::from_text::<Toy>("light").unwrap(), "light");
    let e = Choice::<Theme>::from_text::<Toy>("blue").unwrap_err();
    assert_eq!(e.kind(), "one of dark, light");
    assert_eq!(e.to_string(), "\"blue\" is not one of dark, light");
}

#[test]
fn every_kind_reads_its_literal_and_refuses_another_shape_by_name() {
    assert!(Bool::from_literal::<Toy>(Literal::Bool(true)).unwrap());
    assert_eq!(
        Bool::from_literal::<Toy>(Literal::Int(1))
            .unwrap_err()
            .got(),
        Got::Int
    );
    assert_eq!(Int::from_literal::<Toy>(Literal::Int(3)).unwrap(), 3);
    assert_eq!(
        Int::from_literal::<Toy>(Literal::Str("3"))
            .unwrap_err()
            .got(),
        Got::Text("3")
    );
    assert_eq!(Text::from_literal::<Toy>(Literal::Str("a")).unwrap(), "a");
    assert_eq!(
        Text::from_literal::<Toy>(Literal::List("[]"))
            .unwrap_err()
            .got(),
        Got::List
    );
    assert_eq!(
        PathText::from_literal::<Toy>(Literal::Str(""))
            .unwrap_err()
            .kind(),
        "path"
    );
    assert_eq!(
        Choice::<Theme>::from_literal::<Toy>(Literal::Str("dark")).unwrap(),
        "dark"
    );
    assert_eq!(
        Choice::<Theme>::from_literal::<Toy>(Literal::Bool(true))
            .unwrap_err()
            .got(),
        Got::Bool
    );
}

#[test]
fn a_list_parses_item_by_item_from_text_and_from_a_store() {
    let items: Vec<&str> = List::<Text>::from_text::<Toy>("[a, \"b, c\", d]")
        .unwrap()
        .map(|i| i.unwrap())
        .collect();
    assert_eq!(items, ["a", "b, c", "d"]);
    // brackets are optional, since a shell has eaten one layer of quoting
    let items: Vec<&str> = List::<Text>::from_text::<Toy>("x, y")
        .unwrap()
        .map(|i| i.unwrap())
        .collect();
    assert_eq!(items, ["x", "y"]);
    // the empty list is empty rather than one empty item
    assert_eq!(List::<Text>::from_text::<Toy>("[]").unwrap().count(), 0);
    assert_eq!(List::<Text>::from_text::<Toy>("").unwrap().count(), 0);
    // an open quote is refused
    assert_eq!(
        List::<Text>::from_text::<Toy>("[\"a, b]")
            .unwrap_err()
            .kind(),
        "list"
    );
    // a bad item is refused where it sits
    let mut ints = List::<Int>::from_text::<Toy>("[1, x, 3]").unwrap();
    assert_eq!(ints.next().unwrap().unwrap(), 1);
    assert_eq!(ints.next().unwrap().unwrap_err().got(), Got::Text("x"));
    assert_eq!(ints.next().unwrap().unwrap(), 3);
    // and from a store's literal, through the store's own walk
    let items: Vec<i64> = List::<Int>::from_literal::<Toy>(Literal::List("[4, 5]"))
        .unwrap()
        .map(|i| i.unwrap())
        .collect();
    assert_eq!(items, [4, 5]);
}

// --- the rows -------------------------------------------------------------

#[test]
fn a_row_carries_its_kind_and_scope_by_name_and_checks_by_kind() {
    let rows = table();
    let theme = &rows[0];
    assert_eq!(
        (theme.key(), theme.kind(), theme.scope()),
        ("theme", "one of dark, light", "user")
    );
    assert!(theme.reads_user() && !theme.reads_repo());
    assert!(theme.quoted());
    assert!(theme.check_text("light").is_ok());
    assert!(theme.check_text("blue").is_err());
    assert!(theme.check_literal(Literal::Str("dark")).is_ok());
    assert!(theme.check_literal(Literal::Int(1)).is_err());

    let strict = &rows[2];
    assert!(!strict.reads_user() && strict.reads_repo());
    assert!(!strict.quoted());
    let port = &rows[1];
    assert!(port.reads_user() && port.reads_repo());
    assert_eq!(port.doc(), "The port the server listens on.");
    assert_eq!(port.default(), "8787");
}

#[test]
fn a_table_is_checked_for_its_keys_and_its_defaults() {
    assert_eq!(Declared::defect(&table()), Maybe::Isnt);

    let twice = [
        Setting::<Int, User>::new("a", "1", "").row::<Toy>(),
        Setting::<Int, User>::new("a", "2", "").row(),
    ];
    assert_eq!(Declared::defect(&twice), Maybe::Is(BadTable::KeyTwice("a")));

    let bad_default = [Setting::<Int, User>::new("a", "one", "").row::<Toy>()];
    assert_eq!(
        Declared::defect(&bad_default),
        Maybe::Is(BadTable::DefaultRefused("a", "int"))
    );

    let bad_key = [Setting::<Int, User>::new("a-b", "1", "").row::<Toy>()];
    assert_eq!(
        Declared::defect(&bad_key),
        Maybe::Is(BadTable::KeyNotDotted("a-b"))
    );
    assert!(BadTable::KeyNotDotted("a-b").to_string().contains("a-b"));
}

#[test]
fn a_key_is_identifiers_joined_by_single_dots() {
    for ok in ["a", "a_b", "a.b", "a.b_c.d2", "_x"] {
        assert!(key_is_wellformed(ok), "{ok}");
    }
    for bad in ["", ".", "a.", ".a", "a..b", "a-b", "1a", "a.1", "a b", "A.b c"] {
        assert!(!key_is_wellformed(bad), "{bad}");
    }
}

#[test]
fn the_environment_variable_is_the_short_and_the_key_uppercased_with_dots_as_underscores() {
    assert_eq!(
        EnvKey::of("widget", "model.base").to_string(),
        "WIDGET_CFG_MODEL_BASE"
    );
    assert_eq!(EnvKey::of("t", "theme").to_string(), "T_CFG_THEME");
    // not `WIDGET_CONFIG`, which is the config root's own variable in renki-dirs
    assert_eq!(EnvKey::file("widget").to_string(), "WIDGET_CONFIG_FILE");
}

// --- the resolver ---------------------------------------------------------

#[test]
fn every_place_wins_over_the_ones_below_it_and_the_source_says_which() {
    let rows = table();
    let repo = doc("server.port = 1\nstrict = true\nskip = [\"a\"]\n");
    let user = doc("theme = \"light\"\nserver.port = 2\nmodel.base = \"m\"\n");
    let cli = Cli {
        flags: &[("server.port", "3")],
        env:   &[("WIDGET_CFG_THEME", "dark")],
    };
    let got = all(&rows, &cli, Maybe::Is(&repo), Maybe::Is(&user));
    assert_eq!(got, [
        ("theme", "dark".to_string(), Source::Env),
        ("server.port", "3".to_string(), Source::Flag),
        ("strict", "true".to_string(), Source::Repo),
        ("model.base", "m".to_string(), Source::User),
        ("root", "/srv".to_string(), Source::Default),
        ("skip", "[\"a\"]".to_string(), Source::Repo),
    ]);

    // without the flag the variable wins; without both the repo file wins for
    // a key it may hold, and the user file for one it may not
    let cli = Cli {
        flags: &[],
        env:   &[],
    };
    let got = all(&rows, &cli, Maybe::Is(&repo), Maybe::Is(&user));
    assert_eq!(got[0], ("theme", "light".to_string(), Source::User));
    assert_eq!(got[1], ("server.port", "1".to_string(), Source::Repo));
    // the control: no files, no flags, every row is its default
    let got = all(&rows, &cli, Maybe::Isnt, Maybe::Isnt);
    assert!(got.iter().all(|(_, _, s)| *s == Source::Default), "{got:?}");
    assert_eq!(got[5].1, "[]");
}

#[test]
fn a_repo_file_cannot_set_a_persons_setting_and_a_user_file_cannot_set_the_repos() {
    let rows = table();
    // the repo file names the theme, which is the person's alone; the
    // resolver does not read it there, and `misplaced_keys` names it
    let repo = doc("theme = \"light\"\n");
    let cli = Cli {
        flags: &[],
        env:   &[],
    };
    let got = all(&rows, &cli, Maybe::Is(&repo), Maybe::Isnt);
    assert_eq!(got[0], ("theme", "dark".to_string(), Source::Default));
    let misplaced: Vec<&str> = misplaced_keys(&rows, &repo).collect();
    assert_eq!(misplaced, ["theme"]);
    // and the other way: `strict` is the repo's, and a user file naming it is
    // not read for it
    let user = doc("strict = true\n");
    let got = all(&rows, &cli, Maybe::Isnt, Maybe::Is(&user));
    assert_eq!(got[2], ("strict", "false".to_string(), Source::Default));
    // the control: a key the repo may hold is not misplaced
    let repo = doc("strict = true\nserver.port = 9\n");
    assert_eq!(misplaced_keys(&rows, &repo).count(), 0);
}

#[test]
fn a_key_the_schema_does_not_know_is_named() {
    let rows = table();
    let user = doc("theme = \"dark\"\nthme = \"light\"\nserver.prot = 1\n");
    let unknown: Vec<&str> = unknown_keys(&rows, &user).collect();
    assert_eq!(unknown, ["thme", "server.prot"]);
    // the control
    let user = doc("theme = \"dark\"\n");
    assert_eq!(unknown_keys(&rows, &user).count(), 0);
}

#[test]
fn a_value_the_kind_refuses_is_refused_with_its_source_named() {
    let rows = table();
    let user = doc("server.port = \"eighty\"\n");
    let cli = Cli {
        flags: &[],
        env:   &[],
    };
    let bad = resolve(&rows, "widget", &cli, Maybe::Isnt, Maybe::Is(&user))
        .find_map(erred)
        .unwrap();
    assert_eq!((bad.key(), bad.source()), ("server.port", Source::User));
    assert_eq!(bad.why().got(), Got::Text("eighty"));
    assert_eq!(
        bad.to_string(),
        "server.port from user: \"eighty\" is not int"
    );

    // a flag is checked too, and named as the flag
    let cli = Cli {
        flags: &[("theme", "blue")],
        env:   &[],
    };
    let bad = resolve(&rows, "widget", &cli, Maybe::Isnt, Maybe::Isnt)
        .find_map(erred)
        .unwrap();
    assert_eq!((bad.key(), bad.source()), ("theme", Source::Flag));
}

#[test]
fn a_resolved_value_keeps_where_it_was_found_and_prints_canonically() {
    let rows = table();
    let cli = Cli {
        flags: &[("skip", "a, b")],
        env:   &[],
    };
    let r = resolve(&rows, "widget", &cli, Maybe::Isnt, Maybe::Isnt)
        .map(|r| r.unwrap())
        .find(|r| r.row().key() == "skip")
        .unwrap();
    assert_eq!(r.value(), Value::Text("a, b"));
    assert_eq!(
        r.to_string(),
        "[\"a\", \"b\"]",
        "a list prints in its canonical form"
    );
    assert_eq!(r.source(), Source::Flag);
}

// --- the store's own halves the contract asks for ------------------------

#[test]
fn a_store_writes_one_key_and_keeps_every_other_byte() {
    let text = "# the theme\ntheme = \"dark\" # keep me\nserver.port = 1\n";
    let mut out = String::new();
    Toy::set(text, "theme", Rendered::Text("light"), &mut out).unwrap();
    assert_eq!(
        out,
        "# the theme\ntheme = \"light\" # keep me\nserver.port = 1\n"
    );
    let mut out = String::new();
    Toy::set(text, "strict", Rendered::Raw("true"), &mut out).unwrap();
    assert_eq!(out, format!("{text}strict = true\n"));
}

#[test]
fn a_document_that_is_not_one_is_refused_at_its_line() {
    let e = Toy::parse("a = 1\nnot a line\n").unwrap_err();
    assert_eq!(e.line(), Maybe::Is(2));
    assert_eq!(e.to_string(), "not a document, at line 2");
    assert_eq!(BadDocument::at(Maybe::Isnt).to_string(), "not a document");
}

#[test]
fn the_source_names_are_the_words_config_get_prints() {
    let names: Vec<&str> = [Source::Flag, Source::Env, Source::Repo, Source::User, Source::Default]
        .iter()
        .map(|s| s.name())
        .collect();
    assert_eq!(names, ["flag", "env", "repo", "user", "default"]);
    assert!(Source::Flag < Source::Default, "precedence is the order");
}

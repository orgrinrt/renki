//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The toy store every test binary runs the contracts over, so nothing here
//! depends on a file format: one `key = value` per line, strings quoted,
//! lists in square brackets, `#` to the end of a line a comment. A table is
//! what dotted keys imply: `a.b = 1` makes `a` a table. What the toy store
//! lacks is the point: a store is the contract and nothing more.

#![allow(dead_code)]

use core::fmt;

use notko::{Maybe, Outcome};
use renki_config::{
    Answers,
    BadDocument,
    EnvKey,
    Layer,
    Literal,
    Lookup,
    Part,
    Rendered,
    Shape,
    Stack,
    Store,
    TextItems,
    layer_of,
};

pub struct Toy;

#[derive(Debug)]
pub struct ToyDoc<'t> {
    text: &'t str,
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

    fn shape(doc: &ToyDoc<'_>, key: &str) -> Maybe<Shape> {
        if let Some((_, v)) = lines(doc.text).find(|(k, _)| *k == key) {
            return Maybe::Is(match v {
                "[]" => Shape::Empty,
                _ if v.starts_with('[') => Shape::List,
                _ => Shape::Scalar,
            });
        }
        // what follows the key and its dot, in each key under it
        let mut rests = lines(doc.text)
            .filter_map(|(k, _)| k.strip_prefix(key).and_then(|r| r.strip_prefix('.')))
            .peekable();
        if let Some(rest) = rests.peek() {
            // an array of tables is `name.0.field`, its index the next segment
            let index = rest
                .split('.')
                .next()
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()));
            return Maybe::Is(if index { Shape::Tables } else { Shape::Table });
        }
        Maybe::Isnt
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

pub struct ToyKeys<'d>(Box<dyn Iterator<Item = &'d str> + 'd>);
impl<'d> Iterator for ToyKeys<'d> {
    type Item = &'d str;

    fn next(&mut self) -> Option<&'d str> {
        self.0.next()
    }
}

pub struct ToyItems<'d>(TextItems<'d>);
impl<'d> Iterator for ToyItems<'d> {
    type Item = Literal<'d>;

    fn next(&mut self) -> Option<Literal<'d>> {
        self.0.next().map(literal)
    }
}

pub fn doc(text: &str) -> ToyDoc<'_> {
    Toy::parse(text).unwrap()
}

/// The part a file is of the manifest `app`, which every test names.
pub fn part(file: &str) -> Part<'_> {
    match layer_of::<Toy>("app", file) {
        Outcome::Ok(Maybe::Is(p)) => p,
        other => panic!("{file} is no layer of app: {other:?}"),
    }
}

/// A stack the test knows to be sound, checked.
pub fn sound<'a>(layers: &'a [Layer<'a, Toy>]) -> Stack<'a, Toy> {
    Stack::new(layers).unwrap()
}

/// One read layer of the manifest `app` from its file name and document.
pub fn layer<'a>(file: &'a str, doc: &'a ToyDoc<'a>) -> Layer<'a, Toy> {
    Layer::new(part(file), file, doc)
}

/// A flag and a variable answered from two fixed tables.
pub struct Cli<'a> {
    pub flags: &'a [(&'a str, &'a str)],
    pub env:   &'a [(&'a str, &'a str)],
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

/// A variable and a command's exit answered from two fixed tables; a
/// command the table does not hold did not start.
pub struct Said<'a> {
    pub vars:  &'a [(&'a str, &'a str)],
    pub exits: &'a [(&'a str, i32)],
}

impl Answers for Said<'_> {
    fn var<'s>(&'s self, name: &str) -> Maybe<&'s str> {
        match self.vars.iter().find(|(k, _)| *k == name) {
            Some((_, v)) => Maybe::Is(v),
            None => Maybe::Isnt,
        }
    }

    fn exit(&self, command: &str) -> Maybe<i32> {
        match self.exits.iter().find(|(c, _)| *c == command) {
            Some((_, e)) => Maybe::Is(*e),
            None => Maybe::Isnt,
        }
    }
}

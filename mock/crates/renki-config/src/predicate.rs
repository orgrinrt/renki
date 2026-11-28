//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Named predicates: the table a manifest defines them in, what only the
//! caller can answer, and when one holds.
//!
//! Every `bool`, `i32` and `&str` here is marked for the port to arvo's types
//! and `hilavitkutin_str::Str`, which is a later unit; the markers sit above
//! the item they cover, which is where rustfmt leaves them alone.

use core::fmt::{self, Write};

use notko::{Maybe, Outcome};

use crate::meta::PREDICATES;
use crate::{Layer, Literal, Store};

/// One definition under `meta.predicates.<name>`.
pub enum Predicate<'a, St: Store> {
    /// `env = "<NAME>"`: holds where the variable is truthy.
    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    Env(&'a str),
    /// `shell = "<command>"`: holds where the command exits 0.
    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    Shell(&'a str),
    /// `not = "<name>"`: holds where the named predicate does not.
    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    Not(&'a str),
    /// `all = ["<name>", ...]`, as the store's handle on the list: holds
    /// where every named predicate does.
    All(St::List<'a>),
    /// `any = ["<name>", ...]`, as the store's handle on the list: holds
    /// where one of the named predicates does.
    Any(St::List<'a>),
}

impl<St: Store> Clone for Predicate<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Predicate<'_, St> {}

impl<St: Store> PartialEq for Predicate<'_, St> {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: `PartialEq::eq` is core's signature.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Predicate::Env(a), Predicate::Env(b))
            | (Predicate::Shell(a), Predicate::Shell(b))
            | (Predicate::Not(a), Predicate::Not(b)) => a == b,
            (Predicate::All(a), Predicate::All(b)) | (Predicate::Any(a), Predicate::Any(b)) => {
                a == b
            },
            _ => false,
        }
    }
}

impl<St: Store> Eq for Predicate<'_, St> {}

impl<St: Store> fmt::Debug for Predicate<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Predicate::Env(v) => f.debug_tuple("Env").field(v).finish(),
            Predicate::Shell(v) => f.debug_tuple("Shell").field(v).finish(),
            Predicate::Not(v) => f.debug_tuple("Not").field(v).finish(),
            Predicate::All(l) => f.debug_tuple("All").field(l).finish(),
            Predicate::Any(l) => f.debug_tuple("Any").field(l).finish(),
        }
    }
}

/// The predicates a manifest defines, read from its base and its `defaults`
/// layer, the two layers always read, so no predicate depends on a layer it
/// decides. A name the base defines is the base's whole definition, as a
/// higher layer's value goes over a lower's.
pub struct Predicates<'d, St: Store> {
    base:     &'d Layer<'d, St>,
    defaults: Maybe<&'d Layer<'d, St>>,
}

impl<St: Store> Clone for Predicates<'_, St> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<St: Store> Copy for Predicates<'_, St> {}

impl<St: Store> fmt::Debug for Predicates<'_, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Predicates").finish_non_exhaustive()
    }
}

impl<'d, St: Store> Predicates<'d, St> {
    /// The table over the base and, where there is one, `defaults`, checked
    /// whole: every definition either layer holds is read, whether anything
    /// uses it or not, and one that is malformed, or a `Not`, `All` or `Any`
    /// naming what the table does not define, is refused here.
    pub fn read(
        base: &'d Layer<'d, St>,
        defaults: Maybe<&'d Layer<'d, St>>,
    ) -> Outcome<Self, BadPredicate<'d>> {
        let table = Predicates {
            base,
            defaults,
        };
        // a name a definition refers to must be one the table defines
        let defined = |other| {
            match table.get(other) {
                Outcome::Ok(Maybe::Is(_)) => Outcome::Ok(()),
                Outcome::Ok(Maybe::Isnt) => Outcome::Err(BadPredicate::Undefined(other)),
                Outcome::Err(e) => Outcome::Err(e),
            }
        };
        let layers = [Maybe::Is(base), defaults];
        for layer in layers.into_iter().filter_map(|l| {
            match l {
                Maybe::Is(l) => Some(l),
                Maybe::Isnt => None,
            }
        }) {
            for key in St::keys(layer.doc()) {
                let Some(rest) = key.strip_prefix(PREDICATES) else {
                    continue;
                };
                let name = rest.split_once('.').map_or(rest, |(name, _)| name);
                let named = match table.get(name) {
                    Outcome::Ok(Maybe::Is(Predicate::Not(other))) => Maybe::Is(other),
                    Outcome::Ok(Maybe::Is(Predicate::All(list) | Predicate::Any(list))) => {
                        for item in St::items(list) {
                            let Literal::Str(other) = item else {
                                return Outcome::Err(BadPredicate::Malformed(name));
                            };
                            if let Outcome::Err(e) = defined(other) {
                                return Outcome::Err(e);
                            }
                        }
                        Maybe::Isnt
                    },
                    Outcome::Ok(_) => Maybe::Isnt,
                    Outcome::Err(e) => return Outcome::Err(e),
                };
                if let Maybe::Is(other) = named
                    && let Outcome::Err(e) = defined(other)
                {
                    return Outcome::Err(e);
                }
            }
        }
        // then a loop, walking each definition in turn, its names depth first
        // in the order written, answering no variable and running nothing
        let quiet = |_: Predicate<'d, St>| Outcome::Ok(false);
        for layer in layers.into_iter().filter_map(|l| {
            match l {
                Maybe::Is(l) => Some(l),
                Maybe::Isnt => None,
            }
        }) {
            for key in St::keys(layer.doc()) {
                let Some(rest) = key.strip_prefix(PREDICATES) else {
                    continue;
                };
                let name = rest.split_once('.').map_or(rest, |(name, _)| name);
                if let Outcome::Err(e) = answer(name, &table, &quiet, Maybe::Isnt) {
                    return Outcome::Err(e);
                }
            }
        }
        Outcome::Ok(table)
    }

    /// The definition of `name`, `Isnt` where neither layer defines it, or a
    /// refusal of one that is no single definition of a known kind and shape.
    // lint:allow(no-bare-string) reason: a predicate's name. FIXME: port to Str.
    pub fn get<'n>(&self, name: &'n str) -> Outcome<Maybe<Predicate<'d, St>>, BadPredicate<'n>> {
        match definition(self.base, name) {
            Outcome::Ok(Maybe::Isnt) => {},
            other => return other,
        }
        match self.defaults {
            Maybe::Is(d) => definition(d, name),
            Maybe::Isnt => Outcome::Ok(Maybe::Isnt),
        }
    }
}

// lint:allow(no-bare-string) reason: a predicate's name. FIXME: port to Str.
fn definition<'d, 'n, St: Store>(
    layer: &'d Layer<'d, St>,
    name: &'n str,
) -> Outcome<Maybe<Predicate<'d, St>>, BadPredicate<'n>> {
    let mut found = Maybe::Isnt;
    for key in St::keys(layer.doc()) {
        let Some(rest) = key.strip_prefix(PREDICATES) else {
            continue;
        };
        let Some(kind) = rest.strip_prefix(name).and_then(|r| r.strip_prefix('.')) else {
            continue;
        };
        let value = St::get(layer.doc(), key);
        let one = match (kind, value) {
            ("env", Maybe::Is(Literal::Str(v))) => Predicate::Env(v),
            ("shell", Maybe::Is(Literal::Str(v))) => Predicate::Shell(v),
            ("not", Maybe::Is(Literal::Str(v))) => Predicate::Not(v),
            ("all", Maybe::Is(Literal::List(v))) => Predicate::All(v),
            ("any", Maybe::Is(Literal::List(v))) => Predicate::Any(v),
            _ => return Outcome::Err(BadPredicate::Malformed(name)),
        };
        if found.is() {
            return Outcome::Err(BadPredicate::Malformed(name));
        }
        found = Maybe::Is(one);
    }
    Outcome::Ok(found)
}

/// What only the caller can answer: a variable's value, and the exit of a
/// shell command run in the manifest's directory.
pub trait Answers {
    /// The value of the variable `name`, where it is set.
    // lint:allow(no-bare-string) reason: a variable's name and the caller's own text. FIXME: port to Str.
    fn var<'s>(&'s self, name: &str) -> Maybe<&'s str>;
    /// The exit of `sh -c <command>` in the manifest's directory, or `Isnt`
    /// where it did not start or ended with no exit.
    // lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: a command and its exit. FIXME: port to Str and arvo.
    fn exit(&self, command: &str) -> Maybe<i32>;
}

/// Why a predicate does not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadPredicate<'a> {
    /// A name the table does not define.
    // lint:allow(no-bare-string) reason: a predicate's name. FIXME: port to Str.
    Undefined(&'a str),
    /// A name reached again while it is being answered: the name, and the
    /// one that reached back to it.
    // lint:allow(no-bare-string) reason: predicates' names. FIXME: port to Str.
    Loop(&'a str, &'a str),
    /// A variable holding a value that is neither truthy nor falsy: the
    /// variable, then the value.
    // lint:allow(no-bare-string) reason: a variable's name and value. FIXME: port to Str.
    Value(&'a str, &'a str),
    /// A command whose exit is neither 0 nor 1, or that gave none.
    // lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: a command and its exit. FIXME: port to Str and arvo.
    Exit(&'a str, Maybe<i32>),
    /// A definition of no known kind, of the wrong shape, or of two kinds.
    // lint:allow(no-bare-string) reason: a predicate's name. FIXME: port to Str.
    Malformed(&'a str),
}

impl fmt::Display for BadPredicate<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadPredicate::Undefined(name) => {
                write!(f, "no predicate {name} is defined under meta.predicates")
            },
            BadPredicate::Loop(name, from) => {
                write!(
                    f,
                    "the predicate {name} is reached again from {from}, a loop"
                )
            },
            BadPredicate::Value(var, value) => {
                // the value as a TOML basic string, as the dump writes one
                write!(f, "{var} is \"")?;
                for c in value.chars() {
                    match c {
                        '"' => f.write_str("\\\"")?,
                        '\\' => f.write_str("\\\\")?,
                        '\n' => f.write_str("\\n")?,
                        '\t' => f.write_str("\\t")?,
                        '\r' => f.write_str("\\r")?,
                        c if c < ' ' || c == '\u{7f}' => {
                            // `\u{1f}` from the char itself, as `\u001F`
                            let digits = c.escape_unicode().skip(3).take_while(|d| *d != '}');
                            f.write_str("\\u")?;
                            for _ in digits.clone().count() .. 4 {
                                f.write_char('0')?;
                            }
                            for d in digits {
                                f.write_char(d.to_ascii_uppercase())?;
                            }
                        },
                        c => write!(f, "{c}")?,
                    }
                }
                f.write_str("\", which is neither true nor false")
            },
            BadPredicate::Exit(command, Maybe::Is(code)) => {
                write!(f, "`{command}` exited {code}, which is neither 0 nor 1")
            },
            BadPredicate::Exit(command, Maybe::Isnt) => write!(f, "`{command}` gave no exit"),
            BadPredicate::Malformed(name) => {
                write!(
                    f,
                    "the predicate {name} is not one of env, shell, not, all or any of its shape"
                )
            },
        }
    }
}

/// Whether a variable's value is true, `1`, `true`, `yes` or `on`, or false,
/// empty, `0`, `false`, `no` or `off`, in any case; `Isnt` for anything else.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: the caller's text and its truth. FIXME: port to Str and arvo's Bool.
pub fn truthy(value: &str) -> Maybe<bool> {
    // lint:allow(no-bare-static-str) reason: the static truthy and falsy words. FIXME: port to Str.
    let is = |words: [&'static str; 4]| words.iter().any(|w| w.eq_ignore_ascii_case(value));
    if is(["1", "true", "yes", "on"]) {
        Maybe::Is(true)
    } else if value.is_empty() || is(["0", "false", "no", "off"]) {
        Maybe::Is(false)
    } else {
        Maybe::Isnt
    }
}

/// The names being answered, innermost first, held on the call stack.
struct Path<'p, 'd> {
    // lint:allow(no-bare-string) reason: a predicate's name. FIXME: port to Str.
    name: &'d str,
    up:   Maybe<&'p Path<'p, 'd>>,
}

/// Whether the predicate `name` holds over `table` and the caller's
/// `answers`. Evaluated now and never kept. Every name an `all` or `any`
/// lists is answered, so a refusal anywhere in it is found whatever the
/// others answer. The table was checked for a loop when it was read, so
/// none is met here.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: a predicate's name and its truth. FIXME: port to Str and arvo's Bool.
pub fn holds<'r, 'd: 'r, St: Store, A: Answers>(
    name: &'r str,
    table: &Predicates<'d, St>,
    answers: &'r A,
) -> Outcome<bool, BadPredicate<'r>> {
    let leaf = |p: Predicate<'d, St>| {
        match p {
            Predicate::Env(var) => {
                match answers.var(var) {
                    Maybe::Isnt => Outcome::Ok(false),
                    Maybe::Is(value) => {
                        match truthy(value) {
                            Maybe::Is(b) => Outcome::Ok(b),
                            Maybe::Isnt => Outcome::Err(BadPredicate::Value(var, value)),
                        }
                    },
                }
            },
            Predicate::Shell(command) => {
                match answers.exit(command) {
                    Maybe::Is(0) => Outcome::Ok(true),
                    Maybe::Is(1) => Outcome::Ok(false),
                    // past 128 a shell says a signal ended the command: no exit
                    Maybe::Is(code) if code > 128 => {
                        Outcome::Err(BadPredicate::Exit(command, Maybe::Isnt))
                    },
                    other => Outcome::Err(BadPredicate::Exit(command, other)),
                }
            },
            // `answer` walks the others itself
            Predicate::Not(_) | Predicate::All(_) | Predicate::Any(_) => Outcome::Ok(false),
        }
    };
    answer(name, table, &leaf, Maybe::Isnt)
}

/// The walk under `holds` and under the loop check: `name` answered over
/// `table`, `leaf` answering a variable or a command, every name an `all` or
/// `any` lists walked in the order written, and a name met again on the path
/// that reached it refused as a loop, with the name that reached it.
// lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: a predicate's name and its truth. FIXME: port to Str and arvo's Bool.
fn answer<'r, 'd: 'r, St: Store, F: Fn(Predicate<'d, St>) -> Outcome<bool, BadPredicate<'r>>>(
    name: &'r str,
    table: &Predicates<'d, St>,
    leaf: &F,
    up: Maybe<&Path<'_, 'r>>,
) -> Outcome<bool, BadPredicate<'r>> {
    let mut at = up;
    while let Maybe::Is(p) = at {
        if p.name == name {
            let from = match up {
                Maybe::Is(u) => u.name,
                Maybe::Isnt => name,
            };
            return Outcome::Err(BadPredicate::Loop(name, from));
        }
        at = p.up;
    }
    let here = Path {
        name,
        up,
    };
    let definition = match table.get(name) {
        Outcome::Ok(Maybe::Is(d)) => d,
        Outcome::Ok(Maybe::Isnt) => return Outcome::Err(BadPredicate::Undefined(name)),
        Outcome::Err(e) => return Outcome::Err(e),
    };
    match definition {
        Predicate::Env(_) | Predicate::Shell(_) => leaf(definition),
        Predicate::Not(other) => answer(other, table, leaf, Maybe::Is(&here)).map(|b| !b),
        Predicate::All(list) | Predicate::Any(list) => {
            let all = matches!(definition, Predicate::All(_));
            let mut result = all;
            for item in St::items(list) {
                let Literal::Str(other) = item else {
                    return Outcome::Err(BadPredicate::Malformed(name));
                };
                match answer(other, table, leaf, Maybe::Is(&here)) {
                    Outcome::Ok(b) if all => result &= b,
                    Outcome::Ok(b) => result |= b,
                    Outcome::Err(e) => return Outcome::Err(e),
                }
            }
            Outcome::Ok(result)
        },
    }
}

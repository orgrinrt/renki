//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A kind of value: how text and a literal become one, and back.
//!
//! Every `bool`, `i64` and `&str` here is marked for the port to arvo's types
//! and `hilavitkutin_str::Str`, which is a later unit; the markers sit above
//! the item they cover, which is where rustfmt leaves them alone.

use core::fmt;
use core::marker::PhantomData;

use notko::Outcome;

use crate::{BadValue, Got, Literal, Store};

/// A kind of value, implemented by marker types.
///
/// A setting's kind is in its type, and a table row keeps the kind's parsers
/// as function pointers, so the kind is stateless by construction: every
/// method is associated, nothing is `self`. The value is borrowed from the
/// text or the document it came from, and is generic over the store so a
/// list can be walked through the store that wrote it.
pub trait Kind: 'static {
    /// The value a setting of this kind resolves to, borrowing from the text
    /// or the document it was read from.
    type Value<'a, S: Store>;
    /// What the schema calls it: `bool`, `int`, `text`, `path`, `list`,
    /// `one of dark, light`.
    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    const NAME: &'static str;
    /// Whether the canonical text is quoted by a store writing it. Text is;
    /// a boolean, a number and a list are written as they are.
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool;
    /// From a flag, a variable or a default, where text is all there is.
    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<Self::Value<'a, S>, BadValue<'a>>;
    /// From a file, through the store's literal.
    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<Self::Value<'a, S>, BadValue<'a>>;
    /// The canonical text of a value: what `config get` prints, what the
    /// engine reads out of its environment, and what [`Kind::from_text`]
    /// parses back. Text is bare; a list is `["a", "b"]`.
    fn write<S: Store>(value: Self::Value<'_, S>, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

/// `true` or `false`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Bool;
/// A whole number.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Int;
/// Free text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Text;
/// A path, which is text that is not empty. Kept as text, since a path is the
/// caller's to interpret and a `~` or a relative form means something only
/// where it is used.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PathText;
/// A list of one kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct List<K: Kind>(PhantomData<K>);
/// One of a closed set of names, the set being a type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Choice<C: Choices>(PhantomData<C>);

/// The options a [`Choice`] chooses among. A tool declares a marker type per
/// closed set, so two settings sharing a set share the type.
pub trait Choices: 'static {
    /// The names, as they appear in the file and on the command line.
    // lint:allow(no-bare-static-str) reason: the set's static names. FIXME: port to Str.
    const OPTIONS: &'static [&'static str];
    /// What the schema calls the set. `one of dark, light` is the shape the
    /// [`choices!`](crate::choices) macro writes.
    // lint:allow(no-bare-static-str) reason: the set's static name. FIXME: port to Str.
    const NAME: &'static str;
}

/// Declare a [`Choices`] marker in one line.
///
/// ```
/// # use renki_config::{Choices, choices};
/// choices!(Theme = "dark" | "light");
/// assert_eq!(Theme::OPTIONS, &["dark", "light"]);
/// assert_eq!(Theme::NAME, "one of dark, light");
/// ```
#[macro_export]
macro_rules! choices {
    ($name:ident = $first:literal $(| $rest:literal)*) => {
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name;
        impl $crate::Choices for $name {
            // lint:allow(no-bare-static-str) reason: the set's static names, in the consumer's own crate. FIXME: port to Str.
            const OPTIONS: &'static [&'static str] = &[$first $(, $rest)*];
            // lint:allow(no-bare-static-str) reason: the set's static name, in the consumer's own crate. FIXME: port to Str.
            const NAME: &'static str = concat!("one of ", $first $(, ", ", $rest)*);
        }
    };
}

const fn bad<'a, K: Kind>(got: Got<'a>) -> BadValue<'a> {
    BadValue::new(K::NAME, got)
}

impl Kind for Bool {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own boolean. FIXME: port to arvo's Bool.
    type Value<'a, S: Store> = bool;

    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    const NAME: &'static str = "bool";
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = false;

    // lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: the caller's text and the file's own boolean. FIXME: port to Str and arvo's Bool.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<bool, BadValue<'a>> {
        match text.trim() {
            "true" => Outcome::Ok(true),
            "false" => Outcome::Ok(false),
            other => Outcome::Err(bad::<Self>(Got::Text(other))),
        }
    }

    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own boolean. FIXME: port to arvo's Bool.
    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<bool, BadValue<'a>> {
        match lit {
            Literal::Bool(b) => Outcome::Ok(b),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own boolean. FIXME: port to arvo's Bool.
    fn write<S: Store>(value: bool, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{value}")
    }
}

impl Kind for Int {
    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own integer. FIXME: port to arvo.
    type Value<'a, S: Store> = i64;

    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    const NAME: &'static str = "int";
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = false;

    // lint:allow(no-bare-string, no-bare-numeric, arvo-types-only) reason: the caller's text and the file's own integer. FIXME: port to Str and arvo.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<i64, BadValue<'a>> {
        match text.trim().parse() {
            // lint:allow(no-bare-result) reason: `str::parse` is core's and answers in `Result`; folded into an `Outcome` here.
            Ok(i) => Outcome::Ok(i),
            // lint:allow(no-bare-result) reason: `str::parse` is core's and answers in `Result`; folded into an `Outcome` here.
            Err(_) => Outcome::Err(bad::<Self>(Got::Text(text))),
        }
    }

    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own integer. FIXME: port to arvo.
    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<i64, BadValue<'a>> {
        match lit {
            Literal::Int(i) => Outcome::Ok(i),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    // lint:allow(no-bare-numeric, arvo-types-only) reason: the file's own integer. FIXME: port to arvo.
    fn write<S: Store>(value: i64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{value}")
    }
}

impl Kind for Text {
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    type Value<'a, S: Store> = &'a str;

    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    const NAME: &'static str = "text";
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = true;

    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<&'a str, BadValue<'a>> {
        Outcome::Ok(text)
    }

    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<&'a str, BadValue<'a>> {
        match lit {
            Literal::Str(s) => Outcome::Ok(s),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    fn write<S: Store>(value: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(value)
    }
}

impl Kind for PathText {
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    type Value<'a, S: Store> = &'a str;

    // lint:allow(no-bare-static-str) reason: a kind's static name. FIXME: port to Str.
    const NAME: &'static str = "path";
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = true;

    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<&'a str, BadValue<'a>> {
        if text.is_empty() {
            return Outcome::Err(bad::<Self>(Got::Text(text)));
        }
        Outcome::Ok(text)
    }

    // lint:allow(no-bare-string) reason: borrowed from the document. FIXME: port to Str.
    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<&'a str, BadValue<'a>> {
        match lit {
            Literal::Str(s) if !s.is_empty() => Outcome::Ok(s),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    fn write<S: Store>(value: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(value)
    }
}

impl<C: Choices> Kind for Choice<C> {
    // lint:allow(no-bare-static-str) reason: one of the set's static names. FIXME: port to Str.
    type Value<'a, S: Store> = &'static str;

    // lint:allow(no-bare-static-str) reason: the set's static name. FIXME: port to Str.
    const NAME: &'static str = C::NAME;
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = true;

    // lint:allow(no-bare-string, no-bare-static-str) reason: the caller's text and the set's static names. FIXME: port to Str.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<&'static str, BadValue<'a>> {
        let text = text.trim();
        let mut options = C::OPTIONS.iter().copied();
        match options.find(|o| *o == text) {
            // lint:allow(no-bare-option) reason: `Iterator::find` is core's and answers in `Option`; folded into an `Outcome` here.
            Some(o) => Outcome::Ok(o),
            // lint:allow(no-bare-option) reason: `Iterator::find` is core's and answers in `Option`; folded into an `Outcome` here.
            None => Outcome::Err(bad::<Self>(Got::Text(text))),
        }
    }

    // lint:allow(no-bare-static-str) reason: one of the set's static names. FIXME: port to Str.
    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<&'static str, BadValue<'a>> {
        match lit {
            Literal::Str(s) => Self::from_text::<S>(s),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    // lint:allow(no-bare-static-str) reason: one of the set's static names. FIXME: port to Str.
    fn write<S: Store>(value: &'static str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(value)
    }
}

/// A list's value: the items, parsed one at a time by the element kind, from
/// whichever side the list came. Iterating yields each item's parse, so a
/// bad item is refused where it sits rather than the whole list at once.
pub enum ListValue<'a, K: Kind, S: Store> {
    /// From a flag, a variable or a default, in the canonical `[a, b]` form.
    FromText(TextItems<'a>, PhantomData<(K, S)>),
    /// From a document, walked by the store that wrote it.
    FromStore(S::Items<'a>),
}

impl<K: Kind, S: Store> fmt::Debug for ListValue<'_, K, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListValue::FromText(items, _) => f.debug_tuple("FromText").field(items).finish(),
            ListValue::FromStore(_) => f.debug_tuple("FromStore").finish_non_exhaustive(),
        }
    }
}

impl<'a, K: Kind, S: Store> Iterator for ListValue<'a, K, S> {
    type Item = Outcome<K::Value<'a, S>, BadValue<'a>>;

    // lint:allow(no-bare-option) reason: `Iterator::next` is core's signature.
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ListValue::FromText(items, _) => items.next().map(K::from_text::<S>),
            ListValue::FromStore(items) => items.next().map(K::from_literal::<S>),
        }
    }
}

impl<K: Kind> Kind for List<K> {
    type Value<'a, S: Store> = ListValue<'a, K, S>;

    // lint:allow(no-bare-static-str) reason: a kind's static name; the element kind is named beside it by the row. FIXME: port to Str.
    const NAME: &'static str = "list";
    // lint:allow(no-bare-numeric, arvo-types-only) reason: a compile-time table entry. FIXME: port to arvo's Bool.
    const QUOTED: bool = false;

    // lint:allow(no-bare-string) reason: the caller's text. FIXME: port to Str.
    fn from_text<'a, S: Store>(text: &'a str) -> Outcome<Self::Value<'a, S>, BadValue<'a>> {
        let items = match TextItems::over(text) {
            Outcome::Ok(items) => items,
            Outcome::Err(_) => return Outcome::Err(bad::<Self>(Got::Text(text))),
        };
        // A quoted item holding a quote could not be written back, since the
        // canonical form `["a"]` has no escape for one, so it is refused here
        // where the text arrives rather than at the write, which has no way
        // to say so.
        let mut probe = items;
        if K::QUOTED && probe.any(|i| i.contains('"')) {
            return Outcome::Err(bad::<Self>(Got::Text(text)));
        }
        Outcome::Ok(ListValue::FromText(items, PhantomData))
    }

    fn from_literal<'a, S: Store>(lit: Literal<'a>) -> Outcome<Self::Value<'a, S>, BadValue<'a>> {
        match lit {
            Literal::List(text) => Outcome::Ok(ListValue::FromStore(S::items(text))),
            other => Outcome::Err(bad::<Self>(got_of(other))),
        }
    }

    fn write<S: Store>(value: Self::Value<'_, S>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, item) in value.enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            let Outcome::Ok(item) = item else {
                // lint:allow(no-bare-result) reason: `fmt::Result` is core's signature.
                return Err(fmt::Error);
            };
            if K::QUOTED {
                f.write_str("\"")?;
                K::write::<S>(item, f)?;
                f.write_str("\"")?;
            } else {
                K::write::<S>(item, f)?;
            }
        }
        f.write_str("]")
    }
}

/// The items of a list in its canonical text form, `[a, "b, c", d]`: split
/// on the commas outside quotes, each item trimmed and unquoted. The brackets
/// are optional, since a flag typed by a person often omits them and a shell
/// has already eaten one layer of quoting.
#[derive(Debug, Clone, Copy)]
pub struct TextItems<'a> {
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    rest: &'a str,
    // lint:allow(no-bare-numeric, arvo-types-only, no-public-raw-field) reason: an iterator's own flag. FIXME: port to arvo's Bool.
    done: bool,
}

impl<'a> TextItems<'a> {
    /// Over `text`, or a refusal where a quote is left open.
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    pub fn over(text: &'a str) -> Outcome<Self, ()> {
        let inner = text.trim();
        let inner = match inner.strip_prefix('[') {
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            Some(s) => {
                match s.strip_suffix(']') {
                    // lint:allow(no-bare-option) reason: `str::strip_suffix` is core's and answers in `Option`; matched here.
                    Some(s) => s,
                    // lint:allow(no-bare-option) reason: `str::strip_suffix` is core's and answers in `Option`; matched here.
                    None => return Outcome::Err(()),
                }
            },
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            None => inner,
        };
        if inner.bytes().filter(|b| *b == b'"').count() % 2 != 0 {
            return Outcome::Err(());
        }
        Outcome::Ok(TextItems {
            rest: inner,
            done: inner.trim().is_empty(),
        })
    }
}

impl<'a> Iterator for TextItems<'a> {
    // lint:allow(no-bare-string) reason: borrowed from the caller's text. FIXME: port to Str.
    type Item = &'a str;

    // lint:allow(no-bare-option, no-bare-string) reason: `Iterator::next` is core's signature over a borrowed item.
    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }
        // lint:allow(no-bare-numeric, arvo-types-only) reason: a scanner's own flag. FIXME: port to arvo's Bool.
        let mut quoted = false;
        let mut end = self.rest.len();
        for (i, b) in self.rest.bytes().enumerate() {
            match b {
                b'"' => quoted = !quoted,
                b',' if !quoted => {
                    end = i;
                    break;
                },
                _ => {},
            }
        }
        let item = &self.rest[.. end];
        if end == self.rest.len() {
            self.done = true;
        } else {
            self.rest = &self.rest[end + 1 ..];
        }
        let item = item.trim();
        let item = match item.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            Some(s) => s,
            // lint:allow(no-bare-option) reason: `str::strip_prefix` is core's and answers in `Option`; matched here.
            None => item,
        };
        Some(item)
    }
}

const fn got_of(lit: Literal<'_>) -> Got<'_> {
    match lit {
        Literal::Bool(_) => Got::Bool,
        Literal::Int(_) => Got::Int,
        Literal::Str(s) => Got::Text(s),
        Literal::List(_) => Got::List,
    }
}

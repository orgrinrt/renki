//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The TOML store: `renki-config`'s [`Store`] over `toml_edit`.
//!
//! A document keeps the text it was parsed from, because a list literal is
//! handed over as the list's own source text, borrowed through the span the
//! parser recorded, and the kinds walk it back through [`Store::items`]. The
//! writer edits the text line by line rather than reserialising a value tree,
//! so a settings file keeps the person's comments through every `config set`,
//! which is what keeps them editing it by hand.

use core::fmt;

use notko::{Maybe, Outcome};
use renki_config::{BadDocument, Literal, Rendered, Store};
use toml_edit::{ImDocument, Item, Value};

/// The TOML store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Toml;

/// A parsed document, the text it came from, and every dotted key it holds,
/// computed once at parse time so listing them lends rather than allocates.
#[derive(Debug)]
pub struct Document<'t> {
    text: &'t str,
    doc:  ImDocument<&'t str>,
    keys: Vec<String>,
}

impl Store for Toml {
    type Document<'t> = Document<'t>;
    type Items<'d> = Items<'d>;
    type Keys<'d> = Keys<'d>;

    const EXTENSION: &'static str = "toml";
    const NAME: &'static str = "toml";

    fn parse<'t>(text: &'t str) -> Outcome<Document<'t>, BadDocument> {
        match ImDocument::parse(text) {
            Ok(doc) => {
                let mut keys = Vec::new();
                collect_keys(doc.as_item(), String::new(), &mut keys);
                Outcome::Ok(Document {
                    text,
                    doc,
                    keys,
                })
            },
            Err(e) => {
                let line = e
                    .span()
                    .map(|s| text[.. s.start.min(text.len())].matches('\n').count() as u32 + 1)
                    .map_or(Maybe::Isnt, Maybe::Is);
                Outcome::Err(BadDocument::at(line))
            },
        }
    }

    fn get<'d>(doc: &'d Document<'_>, key: &str) -> Maybe<Literal<'d>> {
        let mut item: &Item = doc.doc.as_item();
        for segment in key.split('.') {
            item = match item.as_table_like().and_then(|t| t.get(segment)) {
                Some(next) => next,
                None => return Maybe::Isnt,
            };
        }
        let Some(value) = item.as_value() else {
            return Maybe::Isnt;
        };
        literal_of(value, doc.text)
    }

    fn keys<'d>(doc: &'d Document<'_>) -> Keys<'d> {
        Keys(doc.keys.iter())
    }

    fn items<'d>(list: &'d str) -> Items<'d> {
        // Parsed as a one-key document rather than as a bare value, since the
        // parser records spans for a document and not for a value on its own;
        // the spans then sit past the `v = ` prefix and are shifted back.
        const PREFIX: &str = "v = ";
        let wrapped = format!("{PREFIX}{list}");
        let spans: Vec<Range> = ImDocument::parse(wrapped.as_str())
            .ok()
            .and_then(|doc| {
                doc.as_item()
                    .as_table_like()
                    .and_then(|t| t.get("v"))
                    .and_then(Item::as_array)
                    .map(|a| {
                        a.iter()
                            .map(|v| {
                                Range {
                                    span: v
                                        .span()
                                        .map(|s| s.start - PREFIX.len() .. s.end - PREFIX.len()),
                                    kind: shape_of(v),
                                }
                            })
                            .collect()
                    })
            })
            .unwrap_or_default();
        Items {
            text:  list,
            spans: spans.into_iter(),
        }
    }

    fn set(text: &str, key: &str, value: Rendered<'_>, into: &mut impl fmt::Write) -> fmt::Result {
        let rendered = match value {
            Rendered::Text(t) => quoted(t),
            Rendered::Raw(t) => t.to_string(),
        };
        let (section, leaf) = match key.rsplit_once('.') {
            Some((s, l)) => (Some(s), l),
            None => (None, key),
        };
        let lines: Vec<&str> = text.lines().collect();
        // Every line, classified once: which section it sits in, and whether it
        // is the key. The whole key spelled flat at the top level names the
        // same thing as the leaf under its section's header.
        let mut current: Option<&str> = None;
        let mut hit: Option<usize> = None;
        let mut last_in_section: Option<usize> = None;
        let mut section_seen = section.is_none();
        // The top level is its own region and it ends at the first header. A
        // top-level key with no line of its own to follow goes above that
        // header, never after the last table's lines, which is where a walk
        // that only knows the current section would put it.
        let mut first_header: Option<usize> = None;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(header) = trimmed.strip_prefix('[')
                && !trimmed.starts_with("[[")
                && let Some(name) = header.split(']').next()
            {
                if first_header.is_none() {
                    first_header = Some(i);
                }
                current = Some(name.trim());
                if current == section {
                    section_seen = true;
                    last_in_section = Some(i);
                }
                continue;
            }
            let (body, _) = split_comment(line);
            let names = |k: &str| {
                let k = k.trim();
                (current == section && k == leaf) || (current.is_none() && k == key)
            };
            if let Some((k, _)) = body.split_once('=')
                && names(k)
            {
                hit = Some(i);
                break;
            }
            if current == section && !body.trim().is_empty() {
                last_in_section = Some(i);
            }
        }
        for (i, line) in lines.iter().enumerate() {
            if hit == Some(i) {
                let (body, comment) = split_comment(line);
                let k = body.split_once('=').map_or(leaf, |(k, _)| k.trim_end());
                write!(into, "{k} = {rendered}")?;
                if !comment.is_empty() {
                    write!(into, " {comment}")?;
                }
                writeln!(into)?;
                continue;
            }
            if hit.is_none()
                && section.is_none()
                && last_in_section.is_none()
                && first_header == Some(i)
            {
                // a top-level key in a file whose top level has no line of its
                // own: above the first header, with the blank line a reader
                // expects between the top level and the first table
                writeln!(into, "{leaf} = {rendered}")?;
                writeln!(into)?;
            }
            writeln!(into, "{line}")?;
            if hit.is_none() && section_seen && last_in_section == Some(i) {
                writeln!(into, "{leaf} = {rendered}")?;
            }
        }
        if hit.is_none() && !section_seen {
            // a section the file does not have yet, at the end
            if !text.is_empty() {
                writeln!(into)?;
            }
            if let Some(s) = section {
                writeln!(into, "[{s}]")?;
            }
            writeln!(into, "{leaf} = {rendered}")?;
        } else if hit.is_none() && last_in_section.is_none() && first_header.is_none() {
            // the top level of a file with no table and no line of its own:
            // empty, or comments only
            writeln!(into, "{leaf} = {rendered}")?;
        }
        Ok(())
    }
}

/// The literal a value is, borrowing text from the document where the value
/// is text and the value's own span where it is a list.
fn literal_of<'d>(value: &'d Value, text: &'d str) -> Maybe<Literal<'d>> {
    match value {
        Value::Boolean(b) => Maybe::Is(Literal::Bool(*b.value())),
        Value::Integer(i) => Maybe::Is(Literal::Int(*i.value())),
        Value::String(s) => Maybe::Is(Literal::Str(s.value().as_str())),
        Value::Array(a) => {
            match a.span() {
                Some(span) => Maybe::Is(Literal::List(&text[span])),
                None => Maybe::Isnt,
            }
        },
        _ => Maybe::Isnt,
    }
}

fn collect_keys(item: &Item, prefix: String, out: &mut Vec<String>) {
    let Some(table) = item.as_table_like() else {
        return;
    };
    for (k, v) in table.iter() {
        let key = if prefix.is_empty() { k.to_string() } else { format!("{prefix}.{k}") };
        match v {
            Item::Value(Value::InlineTable(_)) | Item::Table(_) => collect_keys(v, key, out),
            Item::Value(_) => out.push(key),
            _ => {},
        }
    }
}

/// Every dotted key a document holds a scalar or a list under, lent from the
/// document.
#[derive(Debug)]
pub struct Keys<'d>(core::slice::Iter<'d, String>);

impl<'d> Iterator for Keys<'d> {
    type Item = &'d str;

    fn next(&mut self) -> Option<&'d str> {
        self.0.next().map(String::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Bool,
    Int,
    Str,
    List,
    Other,
}

fn shape_of(v: &Value) -> Shape {
    match v {
        Value::Boolean(_) => Shape::Bool,
        Value::Integer(_) => Shape::Int,
        Value::String(_) => Shape::Str,
        Value::Array(_) => Shape::List,
        _ => Shape::Other,
    }
}

#[derive(Debug)]
struct Range {
    span: Option<core::ops::Range<usize>>,
    kind: Shape,
}

/// The items of a list literal, by their spans in the list's own text.
#[derive(Debug)]
pub struct Items<'d> {
    text:  &'d str,
    spans: std::vec::IntoIter<Range>,
}

impl<'d> Iterator for Items<'d> {
    type Item = Literal<'d>;

    fn next(&mut self) -> Option<Literal<'d>> {
        loop {
            let r = self.spans.next()?;
            let Some(span) = r.span else { continue };
            let raw = &self.text[span];
            let lit = match r.kind {
                Shape::Bool => Literal::Bool(raw == "true"),
                Shape::Int => {
                    match raw.replace('_', "").parse() {
                        Ok(i) => Literal::Int(i),
                        Err(_) => continue,
                    }
                },
                // A basic string's text between its quotes. Escapes are not
                // decoded, since the item has to borrow from the list's text.
                // FIXME: decode escapes in list items once the store can lend decoded strings; a `"\n"` in a list reads as the two characters.
                Shape::Str => {
                    Literal::Str(
                        raw.strip_prefix('"')
                            .and_then(|s| s.strip_suffix('"'))
                            .or_else(|| raw.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                            .unwrap_or(raw),
                    )
                },
                Shape::List => Literal::List(raw),
                Shape::Other => continue,
            };
            return Some(lit);
        }
    }
}

/// `text` as a TOML basic string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A line split at the `#` that starts a comment, which is the first one
/// outside a string.
fn split_comment(line: &str) -> (&str, &str) {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (Some(_), '\\') if !escaped => escaped = true,
            (Some(q), c) if c == q && !escaped => quote = None,
            (Some(_), _) => escaped = false,
            (None, '"' | '\'') => quote = Some(c),
            (None, '#') => return (&line[.. i], &line[i ..]),
            (None, _) => {},
        }
    }
    (line, "")
}

#[cfg(test)]
#[path = "toml_tests.rs"]
mod tests;

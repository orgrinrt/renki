//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Reading a repo's config header: the engine pin, and the working directory.
//!
//! Only top-level keys count. A `<prefix>_version` nested inside some later
//! table is not a pin, and reading the document as a table rather than
//! scanning lines is what makes that true without a rule about it.
//!
//! The key names come from the tool, so this is read as a table looked up by
//! name rather than deserialised into a struct: a derive would fix the
//! spelling at compile time, which is the one thing that has to vary.

use std::path::Path;

use crate::tool::Tool;

/// The package a `Cargo.toml` in `dir` declares itself to be.
///
/// For a tool's [`verify_engine_dir`](crate::Hooks::verify_engine_dir) hook,
/// which has to answer whether a directory somebody pointed `--engine` at is a
/// checkout of the right engine. Every tool that shipped that hook has written
/// its own reader for this and got it wrong in the same two ways: scanning the
/// text for the name anywhere accepts a package under another name declaring a
/// `[[bin]]` under the engine's, or merely mentioning it in a comment, and a
/// scan that knows one of TOML's spellings of an assignment refuses a manifest
/// that uses another.
///
/// Reading the document as a document is what makes both go away, and this
/// crate already parses TOML for the build registry.
///
/// # Errors
///
/// When the manifest cannot be read, cannot be parsed, or declares no
/// `[package] name`. A virtual manifest is the last of those, and it is the
/// ordinary case for a workspace root, which is exactly what somebody reaches
/// for first.
pub fn package_name(dir: &Path) -> Result<String, String> {
    let path = dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: toml::Value = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    doc.get("package")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{} declares no [package] name", path.display()))
}

/// Which revision of an engine a repo pins.
///
/// The four forms are not interchangeable. A version is immutable and maps to
/// both a release and a tag; a rev and a tag are immutable and git-only; a
/// branch moves, and is the only one that has to be re-resolved.
///
/// Left open, for the same reason [`Anchor`](crate::Anchor) is. A fifth form is
/// plausible, a path pin for a tool whose engine sits beside it, and a consumer
/// that only builds one of these pays nothing for the marker. One that matches
/// gains a wildcard arm now instead of a broken build later.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reference {
    /// A released version.
    Version(String),
    /// An exact commit.
    Rev(String),
    /// A tag.
    Tag(String),
    /// A moving branch head.
    Branch(String),
}

/// A pinned engine source: where it lives, and which revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// Where the engine's source is. The tool's
    /// [`default_url`](crate::Tool::default_url) when the config names none.
    pub url: String,
    /// Which revision of it, in whichever of the four forms the config used.
    pub reference: Reference,
}

/// The launcher-relevant top-level keys of a config.
///
/// Open for the same reason, and it costs less here: a `Header` is what
/// [`Header::parse`] hands back, so nothing outside this crate has cause to
/// build one field by field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Header {
    /// The declared working directory, if the tool has one and the config
    /// names it.
    pub workdir: Option<String>,
    /// The declared engine source, if any.
    pub url: Option<String>,
    /// The declared revision, in whichever form the config used.
    pub pin: Option<Reference>,
}

impl Header {
    /// Read the header of `text` under `tool`'s key names. Unreadable TOML and
    /// a config with none of the keys are the same answer: an empty header.
    pub fn parse(tool: &Tool, text: &str) -> Header {
        let Ok(table) = text.parse::<toml::Table>() else {
            return Header::default();
        };
        let keys = &tool.pin_keys;
        let key = |name: &str| -> Option<String> { table.get(name)?.as_str().map(str::to_string) };
        // Ordered most specific first, so a config carrying more than one has
        // a defined answer rather than whichever the reader happened to see.
        let pin = key(keys.rev)
            .map(Reference::Rev)
            .or_else(|| key(keys.tag).map(Reference::Tag))
            .or_else(|| key(keys.branch).map(Reference::Branch))
            .or_else(|| key(keys.version).map(Reference::Version));
        Header {
            workdir: tool
                .workdir
                .as_ref()
                .and_then(|w| table.get(w.key))
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            url: key(keys.git),
            pin,
        }
    }

    /// The pin this header declares, falling back to the tool's default source
    /// when it names no url of its own.
    pub fn to_pin(&self, tool: &Tool) -> Option<Pin> {
        Some(Pin {
            url: self
                .url
                .clone()
                .unwrap_or_else(|| tool.default_url.to_string()),
            reference: self.pin.clone()?,
        })
    }
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;

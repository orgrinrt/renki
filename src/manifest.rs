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
//! The key names carry the tool's prefix, so this is read as a table with
//! names built at runtime rather than deserialised into a struct: a derive
//! would fix the spelling at compile time, which is the one thing that has to
//! vary.

use crate::tool::Tool;

/// Which revision of an engine a repo pins.
///
/// The four forms are not interchangeable. A version is immutable and maps to
/// both a release and a tag; a rev and a tag are immutable and git-only; a
/// branch moves, and is the only one that has to be re-resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// A released version: `<prefix>_version`.
    Version(String),
    /// An exact commit: `<prefix>_rev`.
    Rev(String),
    /// A tag: `<prefix>_tag`.
    Tag(String),
    /// A moving branch head: `<prefix>_branch`.
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
        let key = |suffix: &str| -> Option<String> {
            table
                .get(&format!("{}_{suffix}", tool.pin_prefix))?
                .as_str()
                .map(str::to_string)
        };
        // Ordered most specific first, so a config carrying more than one has
        // a defined answer rather than whichever the reader happened to see.
        let pin = key("rev")
            .map(Reference::Rev)
            .or_else(|| key("tag").map(Reference::Tag))
            .or_else(|| key("branch").map(Reference::Branch))
            .or_else(|| key("version").map(Reference::Version));
        Header {
            workdir: tool
                .workdir
                .as_ref()
                .and_then(|w| table.get(w.key))
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            url: key("git"),
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
mod tests {
    use super::*;
    use crate::tool::{Anchor, Cli, Hooks, Locate, Workdir};

    const T: Tool = Tool {
        anchor: Anchor::Marker(".git"),
        short: "mock",
        config_file: "t.toml",
        pin_prefix: "eng",
        engine_crate: "engine",
        engine_bin: None,
        cache_namespace: "t",
        default_url: "ssh://default",
        launcher_crate: "t-launcher",
        workdir: Some(Workdir {
            key: "work_dir",
            root_default: "mock",
        }),
        dir_flag: Cli::DIR_FLAG,
        engine_flag: Cli::ENGINE_FLAG,
        locate: Locate::DEFAULT,
        hooks: Hooks::NONE,
    };

    #[test]
    fn each_form_is_read_under_the_tools_prefix() {
        for (text, want) in [
            ("eng_version = \"1.2\"\n", Reference::Version("1.2".into())),
            ("eng_rev = \"abc\"\n", Reference::Rev("abc".into())),
            ("eng_tag = \"v1\"\n", Reference::Tag("v1".into())),
            ("eng_branch = \"dev\"\n", Reference::Branch("dev".into())),
        ] {
            assert_eq!(Header::parse(&T, text).pin, Some(want), "{text}");
        }
    }

    #[test]
    fn another_tools_prefix_is_not_this_tools_pin() {
        // the control that makes the test above mean anything: the reader is
        // keyed on the prefix, so a differently-prefixed key is invisible.
        let h = Header::parse(&T, "mockspace_version = \"1.2\"\n");
        assert_eq!(h.pin, None);
        assert!(h.to_pin(&T).is_none());
    }

    #[test]
    fn a_nested_key_is_not_a_pin() {
        let text = "[some.table]\neng_version = \"1.2\"\n";
        assert_eq!(Header::parse(&T, text).pin, None);
    }

    #[test]
    fn the_more_specific_form_wins_when_a_config_carries_several() {
        let text = "eng_version = \"1.2\"\neng_branch = \"dev\"\neng_rev = \"abc\"\n";
        assert_eq!(
            Header::parse(&T, text).pin,
            Some(Reference::Rev("abc".into()))
        );
        let text = "eng_version = \"1.2\"\neng_branch = \"dev\"\n";
        assert_eq!(
            Header::parse(&T, text).pin,
            Some(Reference::Branch("dev".into()))
        );
    }

    #[test]
    fn the_url_defaults_and_is_overridable() {
        let p = Header::parse(&T, "eng_tag = \"v1\"\n").to_pin(&T).unwrap();
        assert_eq!(p.url, "ssh://default");
        let p = Header::parse(&T, "eng_tag = \"v1\"\neng_git = \"ssh://other\"\n")
            .to_pin(&T)
            .unwrap();
        assert_eq!(p.url, "ssh://other");
    }

    #[test]
    fn a_url_without_a_revision_is_not_a_pin() {
        // a source with nothing to check out of it cannot build anything, and
        // reporting it as a pin would defer the failure to cargo.
        let h = Header::parse(&T, "eng_git = \"ssh://other\"\n");
        assert_eq!(h.url.as_deref(), Some("ssh://other"));
        assert!(h.to_pin(&T).is_none());
    }

    #[test]
    fn the_workdir_key_is_the_tools_own() {
        assert_eq!(
            Header::parse(&T, "work_dir = \"design\"\n")
                .workdir
                .as_deref(),
            Some("design")
        );
        assert_eq!(Header::parse(&T, "mock_dir = \"design\"\n").workdir, None);
    }

    #[test]
    fn unreadable_and_empty_configs_are_both_empty_headers() {
        assert_eq!(Header::parse(&T, "this is not [ toml"), Header::default());
        assert_eq!(Header::parse(&T, ""), Header::default());
    }

    #[test]
    fn a_non_string_value_is_not_a_pin() {
        // toml is typed, so a number here is a config error rather than a pin,
        // and taking it as one would key the cache on nothing.
        assert_eq!(Header::parse(&T, "eng_version = 12\n").pin, None);
    }
}

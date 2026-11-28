//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A `meta` list holding anything but strings is refused by name, never
//! skipped, and no glob pattern matches a `.` that begins a name.

mod common;

use common::{Toy, doc, part};
use notko::Maybe;
use renki_config::{BadLayer, Got, Meta, glob_matches};

fn defect(text: &str) -> Maybe<BadLayer<'_>> {
    let d = Box::leak(Box::new(doc(text)));
    Meta::<Toy>::read(d).defect(part("app.local.toy"))
}

fn defect_in<'t>(text: &'t str, file: &str) -> Maybe<BadLayer<'t>> {
    let d = Box::leak(Box::new(doc(text)));
    Meta::<Toy>::read(d).defect(part(file))
}

#[test]
fn an_item_of_include_replace_or_unset_that_is_not_a_string_is_refused_naming_what_it_holds() {
    for (text, want) in [
        (
            "meta.overlays.include = [\"a/\", 1]\n",
            BadLayer::NotAString("meta.overlays.include", Got::Int),
        ),
        (
            "meta.replace = [true]\n",
            BadLayer::NotAString("meta.replace", Got::Bool),
        ),
        (
            "meta.unset = [\"port\", 2]\nport = 1\n",
            BadLayer::NotAString("meta.unset", Got::Int),
        ),
    ] {
        assert_eq!(defect(text), Maybe::Is(want), "{text}");
    }
    // the control: lists of strings only stand
    assert_eq!(
        defect("meta.overlays.include = [\"a/\"]\nmeta.unset = [\"port\"]\n"),
        Maybe::Isnt
    );
}

#[test]
fn an_item_of_untracked_that_is_not_a_string_is_refused_as_no_word() {
    for file in ["app.toy", "app.defaults.toy"] {
        assert_eq!(
            defect_in("meta.overlays.untracked = [\"local\", true]\n", file),
            Maybe::Is(BadLayer::NotAWord("meta.overlays.untracked", Got::Bool)),
            "{file}"
        );
        // the control: words only stand
        assert_eq!(
            defect_in("meta.overlays.untracked = [\"local\"]\n", file),
            Maybe::Isnt,
            "{file}"
        );
    }
}

#[test]
fn no_pattern_matches_a_dot_that_begins_a_name() {
    for (pattern, name) in [
        ("*", ".app.local.toml"),
        ("*.toml", ".app.toml"),
        ("?app.toml", ".app.toml"),
        ("[.]app.toml", ".app.toml"),
        ("[!a]app.toml", ".app.toml"),
        ("conf/*", "conf/.app.local.toml"),
        ("*/app.toml", ".hidden/app.toml"),
    ] {
        assert!(!glob_matches(pattern, name), "{pattern} matched {name}");
    }
    // the controls: a literal leading dot matches, and a dot inside a name is
    // any byte
    for (pattern, name) in [
        (".app*", ".app.local.toml"),
        ("conf/.*", "conf/.app.local.toml"),
        ("app*", "app.local.toml"),
        ("app?local.toml", "app.local.toml"),
        ("app[.]local.toml", "app.local.toml"),
    ] {
        assert!(glob_matches(pattern, name), "{pattern} missed {name}");
    }
}

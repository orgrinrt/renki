//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The `meta` table: its keys, where each may stand, and the glob rule.

mod common;

use common::{Toy, doc, part};
use notko::Maybe;
use renki_config::{BadLayer, Literal, Meta, glob_matches};

fn defect<'d>(text: &'d common::ToyDoc<'d>, file: &str) -> Maybe<BadLayer<'d>> {
    Meta::<Toy>::read(text).defect(part(file))
}

#[test]
fn every_meta_key_is_read_out_of_its_layer() {
    let d = doc(
        "meta.overlays.include = [\"more/\", \"*.toy\"]\nmeta.overlays.order = [\"*\"]\n\
         meta.overlays.untracked = [\"local\"]\nmeta.when = \"cloud\"\nmeta.replace = [\"name\"]\n\
         meta.unset = [\"skip\"]\nname = \"x\"\n",
    );
    let m = Meta::<Toy>::read(&d);
    assert_eq!(
        m.include(),
        Maybe::Is(Literal::List("[\"more/\", \"*.toy\"]"))
    );
    assert_eq!(m.order(), Maybe::Is(Literal::List("[\"*\"]")));
    assert_eq!(m.untracked(), Maybe::Is(Literal::List("[\"local\"]")));
    assert_eq!(m.when(), Maybe::Is("cloud"));
    assert_eq!(m.replace(), Maybe::Is(Literal::List("[\"name\"]")));
    assert_eq!(m.unset(), Maybe::Is(Literal::List("[\"skip\"]")));
    // a layer with no meta has none of them
    let d = doc("name = \"x\"\n");
    let m = Meta::<Toy>::read(&d);
    assert_eq!(
        (m.include(), m.order(), m.when()),
        (Maybe::Isnt, Maybe::Isnt, Maybe::Isnt)
    );
    assert_eq!(Meta::<Toy>::UNTRACKED, ["local", "secret", "private"]);
}

#[test]
fn a_meta_key_that_is_none_of_renkis_is_refused_by_name_in_every_layer() {
    for file in ["app.toy", "app.defaults.toy", "app.local.toy"] {
        let d = doc("meta.overlays.orders = [\"*\"]\n");
        assert_eq!(
            defect(&d, file),
            Maybe::Is(BadLayer::UnknownMeta("meta.overlays.orders")),
            "{file}"
        );
        let d = doc("meta = 1\n");
        assert_eq!(
            defect(&d, file),
            Maybe::Is(BadLayer::UnknownMeta("meta")),
            "{file}"
        );
    }
    let d = doc("meta.colour = \"red\"\n");
    assert!(
        defect(&d, "app.toy")
            .to_string_or_empty()
            .contains("meta.colour")
    );
    // the control: a layer of known keys, and keys outside meta, are no defect
    let d = doc("meta.replace = [\"a\"]\na = 1\nmetadata = 2\n");
    assert_eq!(defect(&d, "app.local.toy"), Maybe::Isnt);
}

#[test]
fn an_order_untracked_or_predicate_in_an_overlay_is_refused_by_name() {
    for (text, key) in [
        ("meta.overlays.order = [\"*\"]\n", "meta.overlays.order"),
        (
            "meta.overlays.untracked = [\"local\"]\n",
            "meta.overlays.untracked",
        ),
        (
            "meta.predicates.cloud.env = \"CLOUD\"\n",
            "meta.predicates.cloud.env",
        ),
    ] {
        let d = doc(text);
        assert_eq!(
            defect(&d, "app.cloud.toy"),
            Maybe::Is(BadLayer::AboveBase(key)),
            "{text}"
        );
        // the control: the base and defaults may set each
        assert_eq!(defect(&d, "app.toy"), Maybe::Isnt, "{text}");
        assert_eq!(defect(&d, "app.defaults.toy"), Maybe::Isnt, "{text}");
    }
}

#[test]
fn when_on_the_base_or_on_defaults_is_refused_since_they_are_always_read() {
    let d = doc("meta.when = \"cloud\"\n");
    assert_eq!(
        defect(&d, "app.toy"),
        Maybe::Is(BadLayer::AlwaysRead("meta.when"))
    );
    assert_eq!(
        defect(&d, "app.defaults.toy"),
        Maybe::Is(BadLayer::AlwaysRead("meta.when"))
    );
    // the control: an overlay is read where its predicate holds
    assert_eq!(defect(&d, "app.cloud.toy"), Maybe::Isnt);
}

#[test]
fn a_replace_naming_a_key_the_layer_does_not_hold_is_refused() {
    let d = doc("meta.replace = [\"skip\"]\nname = \"x\"\n");
    assert_eq!(
        defect(&d, "app.local.toy"),
        Maybe::Is(BadLayer::ReplaceNotHeld("skip"))
    );
    // the control: a key it holds, a table among them
    let d = doc("meta.replace = [\"skip\", \"server\"]\nskip = []\nserver.port = 1\n");
    assert_eq!(defect(&d, "app.local.toy"), Maybe::Isnt);
}

#[test]
fn a_meta_key_of_the_wrong_shape_is_refused_by_name() {
    for (text, key) in [
        ("meta.when = [\"cloud\"]\n", "meta.when"),
        ("meta.replace = \"name\"\nname = 1\n", "meta.replace"),
        ("meta.unset = 1\n", "meta.unset"),
        (
            "meta.overlays.include = \"more/\"\n",
            "meta.overlays.include",
        ),
    ] {
        let d = doc(text);
        assert_eq!(
            defect(&d, "app.local.toy"),
            Maybe::Is(BadLayer::MetaShape(key)),
            "{text}"
        );
    }
}

trait OrEmpty {
    fn to_string_or_empty(&self) -> String;
}

impl OrEmpty for Maybe<BadLayer<'_>> {
    fn to_string_or_empty(&self) -> String {
        match self {
            Maybe::Is(e) => e.to_string(),
            Maybe::Isnt => String::new(),
        }
    }
}

// --- the glob -------------------------------------------------------------

#[test]
fn a_glob_is_the_shells_pattern_within_one_segment() {
    for (pattern, name) in [
        ("*.toy", "app.local.toy"),
        ("app.*.toy", "app.cloud.toy"),
        ("app.?????.toy", "app.cloud.toy"),
        ("app.[cl]*.toy", "app.local.toy"),
        ("app.[!x]*.toy", "app.local.toy"),
        ("app.[a-m]*.toy", "app.local.toy"),
        ("conf/*.toy", "conf/app.toy"),
        ("*", ""),
        ("[]]", "]"),
        ("a[", "a["),
    ] {
        assert!(glob_matches(pattern, name), "{pattern} matches {name}");
    }
    for (pattern, name) in [
        ("*.toy", "app.toml"),
        ("app.?.toy", "app.cloud.toy"),
        ("app.[!l]*.toy", "app.local.toy"),
        ("app.[n-z]*.toy", "app.local.toy"),
        // a star stays within its segment, and there is no `**`
        ("*", "conf/app.toy"),
        ("*.toy", "conf/app.toy"),
        ("**/app.toy", "a/b/app.toy"),
        ("?", ""),
        ("?", "/"),
    ] {
        assert!(
            !glob_matches(pattern, name),
            "{pattern} does not match {name}"
        );
    }
}

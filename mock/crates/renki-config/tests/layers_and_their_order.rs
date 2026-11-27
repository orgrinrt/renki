//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Which files are a manifest's layers, and the order they stack in.

mod common;

use common::{Toy, doc, part};
use notko::{Maybe, Outcome};
use renki_config::{BadLayer, Got, Meta, Order, Part, layer_of};

fn word(file: &str) -> &str {
    match part(file) {
        Part::Layer(name) => name.word(),
        Part::Base => panic!("{file} is the base"),
    }
}

fn refusal(file: &str) -> BadLayer<'_> {
    match layer_of::<Toy>("app", file) {
        Outcome::Err(e) => e,
        Outcome::Ok(p) => panic!("{file} was not refused: {p:?}"),
    }
}

#[test]
fn the_base_and_a_one_word_layer_are_parts_of_the_manifest() {
    assert_eq!(part("app.toy"), Part::Base);
    assert_eq!(word("app.local.toy"), "local");
    assert_eq!(word("app.cloud-session_2.toy"), "cloud-session_2");
    let Part::Layer(defaults) = part("app.defaults.toy") else {
        panic!("defaults is a layer");
    };
    assert!(defaults.is_defaults());
    let Part::Layer(local) = part("app.local.toy") else {
        panic!("local is a layer");
    };
    assert!(!local.is_defaults());
}

#[test]
fn a_file_of_another_name_or_extension_is_no_part_of_the_manifest() {
    for file in ["app.toml", "app.local.toml", "other.toy", "other.local.toy", "apps.toy", "app"] {
        assert_eq!(layer_of::<Toy>("app", file).unwrap(), Maybe::Isnt, "{file}");
    }
}

#[test]
fn a_layer_is_one_word_and_two_are_refused_by_name() {
    assert_eq!(
        refusal("app.cloud.local.toy"),
        BadLayer::TwoWords("app.cloud.local.toy")
    );
    assert!(refusal("app.a.b.toy").to_string().contains("app.a.b.toy"));
    // the control: one word is a layer
    assert_eq!(word("app.cloud.toy"), "cloud");
}

#[test]
fn the_word_base_is_refused_since_the_base_is_no_layer() {
    assert_eq!(refusal("app.base.toy"), BadLayer::NamedBase("app.base.toy"));
    assert!(refusal("app.base.toy").to_string().contains("app.base.toy"));
    // the control: a word that only begins with base is a layer
    assert_eq!(word("app.based.toy"), "based");
}

#[test]
fn a_word_outside_the_layer_alphabet_is_refused_by_name() {
    for file in ["app.Local.toy", "app.lo cal.toy", "app..toy", "app.ä.toy"] {
        assert_eq!(refusal(file), BadLayer::BadWord(file), "{file}");
    }
}

// --- the order ------------------------------------------------------------

fn order<'a>(base: &'a Meta<'a, Toy>, defaults: Maybe<&'a Meta<'a, Toy>>) -> Order<'a, Toy> {
    Order::read(base, defaults).unwrap()
}

fn rank<'w>(o: &Order<'_, Toy>, file: &'w str) -> (u32, &'w str) {
    let Part::Layer(name) = part(file) else {
        panic!("{file} is the base");
    };
    o.rank(name).unwrap()
}

/// The words of `files` sorted by their rank under `o`.
fn sorted<'w>(o: &Order<'_, Toy>, files: &[&'w str]) -> Vec<&'w str> {
    let mut ranked: Vec<(u32, &str)> = files.iter().map(|f| rank(o, f)).collect();
    ranked.sort();
    ranked.into_iter().map(|(_, w)| w).collect()
}

const FILES: [&str; 6] = [
    "app.private.toy",
    "app.zed.toy",
    "app.local.toy",
    "app.cloud.toy",
    "app.secret.toy",
    "app.alpha.toy",
];

#[test]
fn the_default_order_puts_every_other_word_by_bytes_below_local_secret_and_private() {
    let o = Order::<Toy>::DEFAULT;
    assert_eq!(sorted(&o, &FILES), [
        "alpha", "cloud", "zed", "local", "secret", "private"
    ]);
    // and it is what is read where neither the base nor defaults sets one
    let base = doc("name = \"x\"\n");
    let base = Meta::<Toy>::read(&base);
    assert_eq!(
        sorted(&order(&base, Maybe::Isnt), &FILES),
        sorted(&o, &FILES)
    );
}

#[test]
fn the_base_sets_the_order_and_defaults_sets_it_where_the_base_does_not() {
    let base = doc("meta.overlays.order = [\"local\", \"*\", \"zed\"]\n");
    let defaults = doc("meta.overlays.order = [\"zed\", \"*\"]\n");
    let (base, defaults) = (Meta::<Toy>::read(&base), Meta::<Toy>::read(&defaults));
    assert_eq!(sorted(&order(&base, Maybe::Is(&defaults)), &FILES), [
        "local", "alpha", "cloud", "private", "secret", "zed"
    ]);
    let silent = doc("name = \"x\"\n");
    let silent = Meta::<Toy>::read(&silent);
    assert_eq!(sorted(&order(&silent, Maybe::Is(&defaults)), &FILES), [
        "zed", "alpha", "cloud", "local", "private", "secret"
    ]);
}

#[test]
fn an_order_without_a_star_refuses_a_word_it_does_not_name() {
    let base = doc("meta.overlays.order = [\"cloud\", \"local\"]\n");
    let base = Meta::<Toy>::read(&base);
    let o = order(&base, Maybe::Isnt);
    let Part::Layer(zed) = part("app.zed.toy") else { unreachable!() };
    let e = o.rank(zed).unwrap_err();
    assert_eq!(e, BadLayer::Unordered("zed"));
    let said = e.to_string();
    assert!(
        said.contains("zed") && said.contains("meta.overlays.order"),
        "{said}"
    );
    assert!(
        said.contains('*'),
        "it says to add the word or a star: {said}"
    );
    // the control: a named word has its place
    assert_eq!(rank(&o, "app.local.toy"), (1, "local"));
    assert_eq!(rank(&o, "app.cloud.toy"), (0, "cloud"));
}

#[test]
fn an_order_naming_defaults_or_base_a_word_twice_or_two_stars_is_refused_when_read() {
    for (text, want) in [
        ("[\"defaults\", \"*\"]", BadLayer::OrderNames("defaults")),
        ("[\"*\", \"base\"]", BadLayer::OrderNames("base")),
        (
            "[\"local\", \"*\", \"local\"]",
            BadLayer::OrderTwice("local"),
        ),
        ("[\"*\", \"local\", \"*\"]", BadLayer::OrderTwice("*")),
        ("[\"*\", 3]", BadLayer::OrderItem(Got::Int)),
    ] {
        let line = format!("meta.overlays.order = {text}\n");
        let base = doc(&line);
        let base = Meta::<Toy>::read(&base);
        let got = Order::read(&base, Maybe::Isnt);
        assert_eq!(got.err(), Maybe::Is(want), "{text}");
    }
    // the control: the same words once each
    let base = doc("meta.overlays.order = [\"local\", \"*\"]\n");
    let base = Meta::<Toy>::read(&base);
    assert!(Order::read(&base, Maybe::Isnt).is_ok());
}

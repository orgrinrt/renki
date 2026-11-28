//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The predicate table is checked whole when it is read: a malformed
//! definition, and one naming what the table does not define, are refused
//! whether anything uses them or not.

mod common;

use common::{Toy, doc, layer};
use notko::Maybe;
use renki_config::{BadPredicate, Predicates};

fn refusal(base: &str, defaults: Option<&str>) -> Maybe<BadPredicate<'static>> {
    let b = Box::leak(Box::new(doc(Box::leak(base.to_string().into_boxed_str()))));
    let base = Box::leak(Box::new(layer("app.toy", b)));
    let defaults = defaults.map(|t| {
        let d = Box::leak(Box::new(doc(Box::leak(t.to_string().into_boxed_str()))));
        &*Box::leak(Box::new(layer("app.defaults.toy", d)))
    });
    let defaults = match defaults {
        Some(d) => Maybe::Is(d),
        None => Maybe::Isnt,
    };
    Predicates::<Toy>::read(base, defaults).err()
}

#[test]
fn a_malformed_definition_nothing_uses_is_refused_when_read() {
    for (text, name) in [
        ("meta.predicates.x.env = 1\n", "x"),
        ("meta.predicates.x.wat = \"a\"\n", "x"),
        (
            "meta.predicates.x.env = \"A\"\nmeta.predicates.x.shell = \"true\"\n",
            "x",
        ),
        (
            "meta.predicates.x.all = [\"y\", 3]\nmeta.predicates.y.env = \"Y\"\n",
            "x",
        ),
    ] {
        assert_eq!(
            refusal(text, None),
            Maybe::Is(BadPredicate::Malformed(name)),
            "{text}"
        );
    }
    // and in defaults the same
    assert_eq!(
        refusal("port = 1\n", Some("meta.predicates.d.not = 1\n")),
        Maybe::Is(BadPredicate::Malformed("d"))
    );
}

#[test]
fn a_definition_naming_what_the_table_does_not_define_is_refused_when_read() {
    for text in [
        "meta.predicates.x.not = \"zed\"\n",
        "meta.predicates.x.all = [\"zed\"]\n",
        "meta.predicates.x.any = [\"y\", \"zed\"]\nmeta.predicates.y.env = \"Y\"\n",
    ] {
        assert_eq!(
            refusal(text, None),
            Maybe::Is(BadPredicate::Undefined("zed")),
            "{text}"
        );
    }
}

#[test]
fn the_control_a_sound_table_reads_and_a_name_from_defaults_counts() {
    assert_eq!(
        refusal(
            "meta.predicates.x.all = [\"y\", \"z\"]\nmeta.predicates.y.env = \"Y\"\n",
            Some("meta.predicates.z.not = \"y\"\n"),
        ),
        Maybe::Isnt
    );
    assert_eq!(refusal("port = 1\n", None), Maybe::Isnt);
}

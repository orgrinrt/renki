//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A stack holding an `unset` of a key no lower layer holds, or one shape
//! over another, is refused when it is checked, before any reader asks a key,
//! so a reader that walks a schema's leaves meets the same refusal as one
//! that asks the key.

mod common;

use common::{doc, layer};
use notko::Maybe;
use renki_config::{BadMerge, Clash, Shape, Stack};

#[test]
fn an_unset_of_a_key_no_layer_holds_is_refused_for_a_reader_that_never_asks_it() {
    let (b, l) = (doc("port = 1\n"), doc("meta.unset = [\"skip\"]\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(Stack::new(&stack).err(), Maybe::Is(BadMerge::Unset("skip")));
}

#[test]
fn a_shape_over_another_above_the_leaves_is_refused_for_a_reader_asking_only_leaves() {
    let (b, l) = (doc("server = 1\n"), doc("server.port = 2\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        Stack::new(&stack).err(),
        Maybe::Is(BadMerge::Shapes(
            "server",
            "app.toy",
            Clash::new(Shape::Scalar, Shape::Table)
        ))
    );
    // and a deeper one, the higher layer's leaf two tables below the clash
    let (b, l) = (doc("a.b = 1\n"), doc("a.b.c.d = 2\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        Stack::new(&stack).err(),
        Maybe::Is(BadMerge::Shapes(
            "a.b",
            "app.toy",
            Clash::new(Shape::Scalar, Shape::Table)
        ))
    );
    // and one where no layer holds the key the shapes meet at, only keys
    // under it: a table under an array of tables
    let (b, l) = (doc("hooks.x = 1\n"), doc("hooks.0.run = \"b\"\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        Stack::new(&stack).err(),
        Maybe::Is(BadMerge::Shapes(
            "hooks",
            "app.toy",
            Clash::new(Shape::Table, Shape::Tables)
        ))
    );
}

#[test]
fn the_controls_a_sound_stack_is_checked_and_handed_back_whole() {
    let (b, l) = (
        doc("port = 1\nskip = [\"a\"]\nserver.port = 1\n"),
        doc("meta.unset = [\"skip\"]\nserver.port = 2\n"),
    );
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    let checked = Stack::new(&stack).unwrap();
    assert!(core::ptr::eq(checked.layers(), &stack[..]));
    // a stack of none is sound
    assert!(Stack::<common::Toy>::new(&[]).is_ok());
}

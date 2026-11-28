//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A stack holding an `unset` of a key no lower layer holds, or one shape
//! over another, is refused whatever key a reader asks, so a reader that
//! walks a schema's leaves meets the same refusal as one that asks the key.

mod common;

use common::{Toy, doc, layer};
use notko::Outcome;
use renki_config::{BadMerge, Layer, Shape, stacked};

/// What a reader that asks only `keys` is told: the first refusal, if any.
fn asked<'a>(stack: &'a [Layer<'a, Toy>], keys: &[&'a str]) -> Option<BadMerge<'a>> {
    keys.iter().find_map(|k| {
        match stacked(stack, k) {
            Outcome::Err(e) => Some(e),
            Outcome::Ok(_) => None,
        }
    })
}

#[test]
fn an_unset_of_a_key_no_layer_holds_is_refused_for_a_reader_that_never_asks_it() {
    let (b, l) = (doc("port = 1\n"), doc("meta.unset = [\"skip\"]\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(asked(&stack, &["port"]), Some(BadMerge::Unset("skip")));
}

#[test]
fn a_shape_over_another_above_the_leaves_is_refused_for_a_reader_asking_only_leaves() {
    let (b, l) = (doc("server = 1\n"), doc("server.port = 2\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        asked(&stack, &["server.port"]),
        Some(BadMerge::Shapes(Shape::Scalar, Shape::Table))
    );
}

#[test]
fn the_controls_a_sound_stack_is_refused_for_no_key() {
    let (b, l) = (
        doc("port = 1\nskip = [\"a\"]\nserver.port = 1\n"),
        doc("meta.unset = [\"skip\"]\nserver.port = 2\n"),
    );
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        asked(&stack, &["port", "skip", "server", "server.port"]),
        None
    );
}

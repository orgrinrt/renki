//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The merge rule, and a key answered over a stack of layers by it.

mod common;

use common::{Toy, doc, layer, sound};
use notko::{Maybe, Outcome};
use renki_config::{BadMerge, Layer, Literal, Meet, Shape, Stack, Value, meet, stacked};

#[test]
fn two_tables_merge_two_lists_append_and_two_scalars_go_over() {
    assert_eq!(
        meet(Shape::Table, Shape::Table, false),
        Outcome::Ok(Meet::Deep)
    );
    assert_eq!(
        meet(Shape::List, Shape::List, false),
        Outcome::Ok(Meet::Append)
    );
    assert_eq!(
        meet(Shape::Scalar, Shape::Scalar, false),
        Outcome::Ok(Meet::Over)
    );
    // what the higher layer replaces goes over, whatever its shapes
    for lower in [Shape::Table, Shape::List, Shape::Scalar] {
        for higher in [Shape::Table, Shape::List, Shape::Scalar] {
            assert_eq!(
                meet(lower, higher, true),
                Outcome::Ok(Meet::Over),
                "{lower:?} {higher:?}"
            );
        }
    }
}

#[test]
fn a_shape_over_another_that_nothing_replaces_is_refused_naming_both() {
    for (lower, higher) in [
        (Shape::Table, Shape::List),
        (Shape::List, Shape::Table),
        (Shape::Scalar, Shape::Table),
        (Shape::Table, Shape::Scalar),
        (Shape::List, Shape::Scalar),
        (Shape::Scalar, Shape::List),
    ] {
        let e = meet(lower, higher, false).unwrap_err();
        assert_eq!(e, BadMerge::Shapes(lower, higher));
        let said = e.to_string();
        assert!(
            said.contains(lower.name()) && said.contains(higher.name()),
            "{said}"
        );
    }
}

// --- the stack ------------------------------------------------------------

fn winner<'a>(stack: &'a [Layer<'a, Toy>], key: &'a str) -> Maybe<(&'a str, Value<'a, Toy>)> {
    match stacked(&sound(stack), key).unwrap() {
        Maybe::Is((layer, value)) => Maybe::Is((layer.label(), value)),
        Maybe::Isnt => Maybe::Isnt,
    }
}

/// Every item of a list over the stack, lowest layer first.
fn items<'a>(stack: &'a [Layer<'a, Toy>], key: &'a str) -> Vec<&'a str> {
    let Maybe::Is((_, Value::Run(run))) = winner(stack, key) else {
        panic!("{key} is no list over the stack");
    };
    let mut out = Vec::new();
    for list in run {
        let mut items = <Toy as renki_config::Store>::items(list);
        while let Maybe::Is(l) = renki_config::Walk::step(&mut items) {
            match l {
                Literal::Str(s) => out.push(s),
                other => panic!("{other:?}"),
            }
        }
    }
    out
}

#[test]
fn the_highest_layer_holding_a_scalar_wins() {
    let (d, b, l) = (
        doc("port = 1\nname = \"d\"\n"),
        doc("port = 2\n"),
        doc("port = 3\n"),
    );
    let stack = [layer("app.defaults.toy", &d), layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        winner(&stack, "port"),
        Maybe::Is(("app.local.toy", Value::Stored(Literal::Int(3))))
    );
    // a key only the lowest holds is the lowest's
    assert_eq!(
        winner(&stack, "name"),
        Maybe::Is(("app.defaults.toy", Value::Stored(Literal::Str("d"))))
    );
    // and one no layer holds is none
    assert_eq!(winner(&stack, "nope"), Maybe::Isnt);
}

#[test]
fn a_list_appends_across_the_layers_lowest_first() {
    let (b, c, l) = (
        doc("skip = [\"a\", \"b\"]\n"),
        doc("other = 1\n"),
        doc("skip = [\"c\"]\n"),
    );
    let stack = [layer("app.toy", &b), layer("app.cloud.toy", &c), layer("app.local.toy", &l)];
    assert_eq!(items(&stack, "skip"), ["a", "b", "c"]);
    // the winner named is the highest that holds the list
    let Maybe::Is((label, _)) = winner(&stack, "skip") else { unreachable!() };
    assert_eq!(label, "app.local.toy");
}

#[test]
fn a_replaced_list_starts_over_at_the_layer_that_replaces_it() {
    let (b, c, l) = (
        doc("skip = [\"a\", \"b\"]\n"),
        doc("meta.replace = [\"skip\"]\nskip = [\"c\"]\n"),
        doc("skip = [\"d\"]\n"),
    );
    let stack = [layer("app.toy", &b), layer("app.cloud.toy", &c), layer("app.local.toy", &l)];
    assert_eq!(items(&stack, "skip"), ["c", "d"]);
    // and a replaced table drops what the layers below held under it
    let (b, l) = (
        doc("server.port = 1\nserver.host = \"h\"\n"),
        doc("meta.replace = [\"server\"]\nserver.port = 2\n"),
    );
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(winner(&stack, "server.host"), Maybe::Isnt);
    assert_eq!(
        winner(&stack, "server.port"),
        Maybe::Is(("app.local.toy", Value::Stored(Literal::Int(2))))
    );
}

#[test]
fn unset_takes_a_key_out_of_the_layers_below() {
    let (b, l) = (
        doc("skip = [\"a\"]\nport = 1\n"),
        doc("meta.unset = [\"skip\", \"port\"]\n"),
    );
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(winner(&stack, "skip"), Maybe::Isnt);
    assert_eq!(winner(&stack, "port"), Maybe::Isnt);
    // a layer above the unset adds to nothing
    let h = doc("skip = [\"z\"]\n");
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l), layer("app.private.toy", &h)];
    assert_eq!(items(&stack, "skip"), ["z"]);
}

#[test]
fn unset_of_a_key_no_lower_layer_holds_is_refused() {
    let (b, l) = (doc("port = 1\n"), doc("meta.unset = [\"skip\"]\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    let e = Stack::new(&stack).unwrap_err();
    assert_eq!(e, BadMerge::Unset("skip"));
    assert!(e.to_string().contains("skip"));
    // the control: unsetting a key the base holds
    let b = doc("skip = [\"a\"]\n");
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert!(Stack::new(&stack).is_ok());
}

#[test]
fn a_shape_over_a_shape_in_the_stack_is_refused_unless_replaced() {
    let (b, l) = (doc("skip = [\"a\"]\n"), doc("skip = \"a\"\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        Stack::new(&stack).err(),
        Maybe::Is(BadMerge::Shapes(Shape::List, Shape::Scalar))
    );
    // the control: the same, replaced
    let l = doc("meta.replace = [\"skip\"]\nskip = \"a\"\n");
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        winner(&stack, "skip"),
        Maybe::Is(("app.local.toy", Value::Stored(Literal::Str("a"))))
    );
}

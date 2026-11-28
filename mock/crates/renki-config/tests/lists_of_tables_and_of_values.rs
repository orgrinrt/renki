//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! An array of tables and a list of values are two shapes: each appends to
//! its own kind, neither to the other, and the empty list holds neither, so
//! it meets both.

mod common;

use common::{Toy, doc, layer};
use notko::{Maybe, Outcome};
use renki_config::{BadMerge, Meet, Shape, Stack, Store, meet};

const LISTS: [Shape; 3] = [Shape::List, Shape::Tables, Shape::Empty];

#[test]
fn each_list_appends_to_its_own_kind_and_the_empty_list_to_either() {
    for (lower, higher) in [
        (Shape::List, Shape::List),
        (Shape::Tables, Shape::Tables),
        (Shape::Empty, Shape::List),
        (Shape::Empty, Shape::Tables),
        (Shape::List, Shape::Empty),
        (Shape::Tables, Shape::Empty),
        (Shape::Empty, Shape::Empty),
    ] {
        assert_eq!(
            meet(lower, higher, false),
            Outcome::Ok(Meet::Append),
            "{lower:?} {higher:?}"
        );
    }
}

#[test]
fn an_array_of_tables_and_a_list_of_values_do_not_append_either_way() {
    for (lower, higher) in [(Shape::List, Shape::Tables), (Shape::Tables, Shape::List)] {
        let e = meet(lower, higher, false).unwrap_err();
        assert_eq!(e, BadMerge::Shapes(lower, higher));
        let said = e.to_string();
        assert!(
            said.contains(lower.name()) && said.contains(higher.name()),
            "{said}"
        );
        // the control: replaced, it goes over
        assert_eq!(meet(lower, higher, true), Outcome::Ok(Meet::Over));
    }
}

#[test]
fn no_list_meets_a_table_or_a_scalar() {
    for list in LISTS {
        for other in [Shape::Table, Shape::Scalar] {
            assert!(meet(list, other, false).is_err(), "{list:?} {other:?}");
            assert!(meet(other, list, false).is_err(), "{other:?} {list:?}");
        }
    }
}

#[test]
fn the_names_of_the_lists_tell_them_apart() {
    assert_ne!(Shape::List.name(), Shape::Tables.name());
    assert_ne!(Shape::List.name(), Shape::Empty.name());
    assert_ne!(Shape::Tables.name(), Shape::Empty.name());
}

#[test]
fn the_toy_store_answers_each_list_shape() {
    let d = doc("a = [1]\nb = []\nc.0.run = \"x\"\nd.e = 1\n");
    for (key, shape) in [
        ("a", Shape::List),
        ("b", Shape::Empty),
        ("c", Shape::Tables),
        ("d", Shape::Table),
        ("d.e", Shape::Scalar),
    ] {
        assert_eq!(Toy::shape(&d, key), Maybe::Is(shape), "{key}");
    }
}

#[test]
fn a_stack_of_an_array_of_tables_over_a_list_of_values_is_refused() {
    let (b, l) = (doc("hooks = [\"a\"]\n"), doc("hooks.0.run = \"b\"\n"));
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert_eq!(
        Stack::new(&stack).err(),
        Maybe::Is(BadMerge::Shapes(Shape::List, Shape::Tables))
    );
    // the control: over an empty list it stands
    let b = doc("hooks = []\n");
    let stack = [layer("app.toy", &b), layer("app.local.toy", &l)];
    assert!(Stack::new(&stack).is_ok());
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The room against the files under `tests/host/`, read through `Unix`.

use core::ffi::CStr;

use tomlfuse_parse::{ErrorKind, Options, Version, nodes_for, parse};

use crate::{BadRead, Room, Unix};

const ELEVEN: &CStr = c"tests/host/eleven_bytes";
const DENSEST: &CStr = c"tests/host/the_densest_document";

#[test]
fn a_file_bigger_than_the_room_is_refused_with_the_size_the_room_needs() {
    let mut room = Room::<8, 5>::new();
    let (mut shelf, _) = room.parts();
    assert_eq!(
        shelf.read::<Unix>(ELEVEN),
        Err(BadRead::Room {
            file: ELEVEN,
            need: 11,
        })
    );
}

#[test]
fn a_room_of_the_size_a_refusal_names_reads_the_file() {
    let mut room = Room::<11, 6>::new();
    let (mut shelf, _) = room.parts();
    assert_eq!(shelf.read::<Unix>(ELEVEN), Ok("name = \"x\"\n"));
}

#[test]
fn the_size_a_room_needs_counts_the_layers_already_in_it() {
    let mut room = Room::<16, 8>::new();
    let (mut shelf, _) = room.parts();
    assert_eq!(shelf.read::<Unix>(ELEVEN), Ok("name = \"x\"\n"));
    assert_eq!(
        shelf.read::<Unix>(ELEVEN),
        Err(BadRead::Room {
            file: ELEVEN,
            need: 22,
        })
    );

    let mut room = Room::<22, 11>::new();
    let (mut shelf, _) = room.parts();
    let first = shelf.read::<Unix>(ELEVEN).unwrap();
    let second = shelf.read::<Unix>(ELEVEN).unwrap();
    assert_eq!((first, second), ("name = \"x\"\n", "name = \"x\"\n"));
}

#[test]
fn a_refused_read_leaves_the_room_as_it_was() {
    let mut room = Room::<16, 8>::new();
    let (mut shelf, _) = room.parts();
    assert!(shelf.read::<Unix>(c"tests/host/no_such_file").is_err());
    assert!(shelf.read::<Unix>(DENSEST).is_err());
    assert_eq!(shelf.read::<Unix>(ELEVEN), Ok("name = \"x\"\n"));
}

/// The densest document there is for the parser's index: one dotted key of
/// 511 segments, a node for every two bytes of its 1024.
#[test]
fn a_room_sized_by_nodes_for_never_runs_out_of_nodes() {
    let mut room = Room::<1024, { nodes_for(1024) }>::new();
    let (mut shelf, nodes) = room.parts();
    let text = shelf.read::<Unix>(DENSEST).unwrap();
    assert_eq!(text.len(), 1024);
    let held = parse(
        text.as_bytes(),
        Options::new(Version::V1_1_0),
        nodes,
        &mut (),
    )
    .unwrap();
    assert_eq!(held, 511, "the document needs a node per segment");
}

/// The same document in a room one node short of what it holds, so the test
/// above is seen to reach the bound.
#[test]
fn a_room_short_of_what_the_document_holds_runs_out_of_nodes() {
    let mut room = Room::<1024, 510>::new();
    let (mut shelf, nodes) = room.parts();
    let text = shelf.read::<Unix>(DENSEST).unwrap();
    let refused = parse(
        text.as_bytes(),
        Options::new(Version::V1_1_0),
        nodes,
        &mut (),
    )
    .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::OutOfNodes);
}

//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A tool's configuration, as contracts.
//!
//! A tool has settings, each with a kind, a scope and a default, and something
//! resolves every one of them once from five places in a fixed order: a flag,
//! the tool's own environment variable, the repository's file where the scope
//! allows it, the person's file, and the default. What resolved it says which
//! of the five won, and for a file which of its layers, since that is the
//! question behind most configuration bugs. The engine then receives the
//! result and never has two answers to choose from.
//!
//! A file is a manifest: a base `name.ext` and any number of layers
//! `name.<word>.ext` over it. Which files are layers, their order, the `meta`
//! table that steers the reading, how the layers merge and the named
//! predicates that decide whether a layer is read are rules held here once,
//! for every reader that applies them.
//!
//! This crate is the schema, the store contract, the layer rules, the
//! predicate and the resolver, and nothing else. It reads no file and no
//! environment, runs no process, allocates nothing, borrows every string from
//! text the caller holds, and speaks notko's [`Outcome`] and [`Maybe`]. A
//! reader implements a [`Store`] for the file format it keeps, does the
//! reading, and hands the layers in as a stack.
//!
//! The kinds and the scopes are marker types, so a [`Setting<K, S>`] carries
//! both in its type and a default of the wrong shape is refused where it is
//! declared. The table a tool carries is rows of [`Declared`], which a typed
//! setting turns itself into at compile time and a table read at run time is
//! built of by the kind's name: the kind's parsers and renderers are kept as
//! function pointers, so the table is one type over settings of every kind
//! and each row still checks a value the way its type would.
//!
//! [`Outcome`]: notko::Outcome
//! [`Maybe`]: notko::Maybe

#![no_std]
#![forbid(unsafe_code)]
// The readme's `rust` block is compiled as a doctest, the way the launcher's
// is, so the usage a stranger copies is one the crate still accepts. On the
// crate root rather than on a carrier type, since a type would be one more
// name the design has to account for.
#![cfg_attr(doctest, doc = include_str!("../README.md"))]

mod kind;
mod layer;
mod list;
mod literal;
mod merge;
mod meta;
mod predicate;
mod resolve;
mod row;
mod scope;
mod setting;
mod stack;
mod store;

pub use kind::{Bool, Choice, Choices, Int, Kind, List, PathText, PredicateName, Text};
pub use layer::{BadLayer, LayerName, Order, Part, layer_of};
pub use list::{ListValue, TextItems};
pub use literal::{BadValue, Got, Literal};
pub use merge::{BadMerge, Meet, Shape, meet};
pub use meta::{Meta, MetaList, glob_matches};
pub use predicate::{Answers, BadPredicate, Predicate, Predicates, holds, truthy};
pub use resolve::{
    BadConfig,
    EnvKey,
    Lookup,
    ResolvedView,
    Value,
    misplaced_keys,
    resolve,
    unknown_keys,
};
pub use row::{BadTable, Declared};
pub use scope::{Either, Repo, Scope, User};
pub use setting::{Setting, key_is_wellformed};
pub use stack::{Layer, ListRun, Stack, stacked};
pub use store::{BadDocument, Origin, Rendered, Store};

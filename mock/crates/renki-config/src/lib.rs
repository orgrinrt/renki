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
//! of the five won, since that is the question behind most configuration
//! bugs. The engine then receives the result and never has two answers to
//! choose from.
//!
//! This crate is the schema, the store contract and the resolver, and nothing
//! else. It reads no file and no environment, allocates nothing, borrows every
//! string from text the caller holds, and speaks notko's [`Outcome`] and
//! [`Maybe`]. A launcher implements a [`Store`] for the file format it keeps,
//! does the reading, and hands the documents in.
//!
//! The kinds and the scopes are marker types, so a [`Setting<K, S>`] carries
//! both in its type and a default of the wrong shape is refused where it is
//! declared. The table a tool carries is rows of [`Declared<S>`], which a
//! typed setting turns itself into at compile time: the kind's parsers and
//! renderers are kept as function pointers, so the table is one type over
//! settings of every kind and each row still checks a value the way its type
//! would.
//!
//! [`Outcome`]: notko::Outcome
//! [`Maybe`]: notko::Maybe

#![no_std]
#![forbid(unsafe_code)]

mod kind;
mod literal;
mod resolve;
mod scope;
mod setting;
mod store;

pub use kind::{Bool, Choice, Choices, Int, Kind, List, ListValue, PathText, Text, TextItems};
pub use literal::{BadValue, Got, Literal};
pub use resolve::{
    BadConfig,
    EnvKey,
    Lookup,
    Resolved,
    Value,
    misplaced_keys,
    resolve,
    unknown_keys,
};
pub use scope::{Either, Repo, Scope, User};
pub use setting::{BadTable, Declared, Setting, key_is_wellformed};
pub use store::{BadDocument, Rendered, Source, Store};

# `renki-config`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki-config)](https://crates.io/crates/renki-config)
[![docs.rs](https://img.shields.io/docsrs/renki-config)](https://docs.rs/renki-config)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> A tool's configuration as contracts. Kinds and scopes as marker types, a typed setting and its erased row, a store contract, and a resolver with provenance. no_std, no alloc, one dependency.

</div>

A command line tool has settings, and a setting has a kind, a scope and a
default. Something has to read every one of them once from the places a
value can come from, in one fixed order, and say which place won, because
the question behind most configuration bugs is not what the value is but
which of five files or variables it came from. A gui, a cli and a hand edit
of the file all want to go through one schema, so none of the three carries
a schema of its own.

This crate is that schema and that resolver, as contracts, and nothing more.
A kind is a marker type that says how text and a file's value become one of
its values and back; a scope is a marker type that says which files are
read; a setting carries both in its type, so a default of the wrong shape is
refused where it is declared. A store is the contract a file format
implements: parse a document, answer a key, list the keys, write one key back
into the text it was given with everything else left where it was. The
resolver runs over any store and yields, per setting, the value and its
source.

Nothing here reads a file or the environment, allocates, or names `std`.
Every string is borrowed from text the caller holds, and fallibility is
notko's `Outcome` and `Maybe`. The launcher that uses it, `renki`, implements
the store for TOML and does the reading; an engine in rust reads the same
contracts back out of its environment.

## Usage

```toml
[dependencies]
renki-config = "0.0.1"
```

A tool declares its settings as a static table of rows, each built from a
typed setting for the store it keeps its file in:

```rust
use renki_config::{Choice, Declared, Either, Int, Setting, Store, Text, User, choices};

choices!(Theme = "dark" | "light");

fn table<S: Store>() -> [Declared<S>; 3] {
    [
        Setting::<Choice<Theme>, User>::new("theme", "dark", "Which of the two the pages use.").row(),
        Setting::<Int, Either>::new("server.port", "8787", "The port the server listens on.").row(),
        Setting::<Text, User>::new("model.base", "", "The model the scorer runs on.").row(),
    ]
}
```

The kinds that ship are `Bool`, `Int`, `Text`, `PathText`, `List<K>` and
`Choice<C>`; the scopes are `User`, `Repo` and `Either`. A store is one
`impl Store for YourFormat`, and `resolve` takes the table, the tool's short
name, something answering flags and variables, and the two documents.

### Limitations

Strings are borrowed `&str` and integers are `i64`. Both are marked at every
site for the port to the stack's own string and numeric types, which is a
later unit. Windows has no column, since renki is unix only.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>

## License

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

This project is licensed under the terms of the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

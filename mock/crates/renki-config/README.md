# `renki-config`

[![Crates.io](https://img.shields.io/crates/v/renki-config)](https://crates.io/crates/renki-config) [![docs.rs](https://img.shields.io/docsrs/renki-config)](https://docs.rs/renki-config)

> A tool's configuration as contracts. Kinds and scopes as marker types, a typed setting and its erased row, a store contract, and a resolver with provenance. no_std, no alloc, one dependency.


A tool's settings each have a kind, a scope and a default, and something has to read every one of them from the places a value can come from, in one fixed order, and say which place won. This crate is that schema and that resolver, as contracts. A kind is a marker type saying how text and a file's value become one of its values and back, a scope says which files are read, and a setting carries both in its type, so a default of the wrong shape is refused where it is declared. A store is what a file format implements, and the resolver runs over any store and yields the value and its source for each setting.

Nothing here reads a file or the environment, allocates, or names `std`. The launcher that uses it, `renki`, implements the store for TOML and does the reading, and an engine in rust reads the same contracts back out of its environment. Strings are borrowed `&str` and integers are `i64` for now, and it's unix only.

## Usage

```toml
[dependencies]
renki-config = "0.0.2"
```

A tool declares its settings as a static table of rows, each built from a typed setting for the store it keeps its file in:

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

The kinds that ship are `Bool`, `Int`, `Text`, `PathText`, `List<K>` and `Choice<C>`, and the scopes are `User`, `Repo` and `Either`. `resolve` takes the table, the tool's short name, something answering flags and variables, and the two documents.

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

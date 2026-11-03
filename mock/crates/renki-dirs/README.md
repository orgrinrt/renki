# `renki-dirs`

[![Crates.io](https://img.shields.io/crates/v/renki-dirs)](https://crates.io/crates/renki-dirs) [![docs.rs](https://img.shields.io/docsrs/renki-dirs)](https://docs.rs/renki-dirs)

> Where a tool's files go, typed. Cache, state, config and data roots per platform, as marker types. no_std, no alloc, one dependency.

A command line tool keeps four kinds of file, told apart by what losing each one costs. A cache is rebuilt without anybody asking, state is what the tool wrote for itself, config is what the person wrote, and data is what the tool made for the person and cannot make again. Every platform has its own directory for each of the four, and this crate is that table, with the kinds and the platforms as types. A root is a `Root<Cache, Host>` or a `Root<State, MacOs>`, so a function that wants the cache root says so in its signature and can't be handed the state root by mistake.

It reads nothing. The caller reads the three environment variables a root depends on and hands them in, and the crate answers with the path's parts, which of the three decided it, and a `Display` that prints the path, so it works with no allocator.

## Usage

```toml
[dependencies]
renki-dirs = "0.0.2"
```

```rust
use renki_dirs::{Cache, EnvName, Host, Namespace, Root, Short, Sources};
use notko::{Maybe, Outcome};

let ns = match Namespace::new("widget") {
    Outcome::Ok(ns) => ns,
    Outcome::Err(e) => panic!("{e:?}"),
};
let short = match Short::new("widget") {
    Outcome::Ok(s) => s,
    Outcome::Err(e) => panic!("{e:?}"),
};
let own = std::env::var(EnvName::<Cache>::of(short).to_string()).ok();
let xdg = std::env::var("XDG_CACHE_HOME").ok();
let home = std::env::var("HOME").ok();
let root = Root::<Cache, Host>::resolve(ns, Sources {
    own: own.as_deref().map_or(Maybe::Isnt, Maybe::Is),
    xdg: xdg.as_deref().map_or(Maybe::Isnt, Maybe::Is),
    home: home.as_deref().map_or(Maybe::Isnt, Maybe::Is),
});
let cache = std::path::PathBuf::from(root.unwrap().to_string());
```

Precedence is the same for every kind: the tool's own variable, named `<SHORT>_<KIND>` and holding the whole path, then the XDG variable for the kind, then the platform's directory under the home directory. An exported XDG variable is honoured on macOS too, and an empty value is not a setting at any level. On Linux and the BSDs the cache is `~/.cache/<namespace>` and on macOS it is `~/Library/Caches/<namespace>`, with state, config and data following the same split. `Host` is `MacOs` when built for macOS and `Xdg` otherwise.

It builds on stable with notko's default features off, and it's pre-1.0, with no Windows column since renki is unix only.

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

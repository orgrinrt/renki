# `renki-dirs`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki-dirs)](https://crates.io/crates/renki-dirs)
[![docs.rs](https://img.shields.io/docsrs/renki-dirs)](https://docs.rs/renki-dirs)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> Where a tool's files go, typed. Cache, state, config and data roots per platform, as marker types. no_std, no alloc, one dependency.

</div>

A command line tool keeps four kinds of file, and they are told apart by what
losing each one costs rather than by what they contain. A cache is rebuilt
without anybody asking, so a cleanup may empty it whenever it likes. State is
what the tool wrote for itself and would behave differently without. Config is
what the person wrote. Data is what the tool made for the person and cannot
make again. Every platform has its own directory for each of the four, and
they are not where a tool written on one platform assumes: `~/.cache` on a mac
is nobody's cache, and a cleanup there looks under `~/Library/Caches`.

This crate is that table, with the kinds and the platforms as types rather than
values. A root is a `Root<Cache, Host>` or a `Root<State, MacOs>`, so a
function that wants the cache root says so in its signature and cannot be
handed the state root by mistake. It is what `renki` uses for its own cache
and state, and it is a crate of its own so a tool renki does not launch, or
that is not rust at all, follows the same table and lands its files beside
everybody else's.

It reads nothing. The caller reads the three environment variables a root
depends on and hands them in; the crate answers with the path's parts, which
of the three decided it, and a `Display` that prints the path. That is the
whole no-alloc surface, and it is enough: a `std` caller formats it into a
`PathBuf`, a no-std one writes it into a buffer of its own.

## Usage

```toml
[dependencies]
renki-dirs = "0.0.1"
```

```rust
use renki_dirs::{Cache, EnvName, Host, Namespace, Root, Short, Sources};
use notko::{Maybe, Outcome};

let ns = match Namespace::new("widget") {
    Outcome::Ok(ns) => ns,
    Outcome::Err(e) => panic!("{e:?}"),
};
let own = std::env::var(EnvName::<Cache>::of(Short("widget")).to_string()).ok();
let xdg = std::env::var("XDG_CACHE_HOME").ok();
let home = std::env::var("HOME").ok();
let root = Root::<Cache, Host>::resolve(ns, Sources {
    own: own.as_deref().map_or(Maybe::Isnt, Maybe::Is),
    xdg: xdg.as_deref().map_or(Maybe::Isnt, Maybe::Is),
    home: home.as_deref().map_or(Maybe::Isnt, Maybe::Is),
});
let cache = std::path::PathBuf::from(root.unwrap().to_string());
```

Precedence is the same for every kind: the tool's own variable, named
`<SHORT>_<KIND>` and holding the whole path, then the XDG variable for the
kind, then the platform's directory under the home directory. The XDG
variables are honoured on macOS too, since somebody who exported one has said
where their files go and the platform default is for the person who has said
nothing. An empty value is not a setting at any level, and no home and no
variable is an error naming the kind.

The defaults, with `ns` the tool's namespace:

| Kind | `Xdg` | `MacOs` |
|---|---|---|
| `Cache` | `~/.cache/ns` | `~/Library/Caches/ns` |
| `State` | `~/.local/state/ns` | `~/Library/Application Support/ns/state` |
| `Config` | `~/.config/ns` | `~/Library/Application Support/ns` |
| `Data` | `~/.local/share/ns` | `~/Library/Application Support/ns/data` |

`Host` is `MacOs` when built for macOS and `Xdg` otherwise, on the operating
system rather than the family, since the BSDs are unix and XDG.

## Motivation

The table above had been written four times across one workspace, in shell, in
TypeScript and twice in rust, and no two copies agreed on the mac column. The
one that put the cache under `~/.cache` on a mac was found the day a cleanup
emptied the real cache directory and took a build with it, because the tool's
own directory was not the cache but sat beside it. A table nobody has to copy
is one that cannot be copied wrong, and a root that is a type is one that
cannot be handed to the wrong function.

## Extras

### Status

Pre-1.0, and the surface is small on purpose. Builds on the stable floor renki
promises its installers, `rust-version = "1.88"`, with notko's default
features off; nothing here needs nightly.

### Limitations

Paths are `&str`. A home directory whose bytes are not text is refused by the
caller rather than printed wrongly by this crate, which is the same line renki
draws for a repository root. Windows has no column, since renki is unix only.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

# `renki`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki)](https://crates.io/crates/renki)
[![docs.rs](https://img.shields.io/docsrs/renki)](https://docs.rs/renki)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> A library for building the launcher half of a two-part command line tool. Repo discovery, the version pin, a shared build cache and the handover. Unix only, two dependencies.

</div>

I keep writing tools that come in two halves, and I got tired of writing the same
half twice. The engine does the work and each repo pins the version of it that it
wants; the launcher is the small thing on `PATH` that finds the repo, reads the
pin, builds that exact engine once into a shared cache and hands over. This crate
is that second half with the identity pulled out of it.

What the split buys is that a repo's tooling can't drift away from what the repo
asked for. Everyone on the project gets the version the config names, on whatever
machine, whatever they happened to install last year. And a launcher installed
off a git branch keeps itself current, which is the part that otherwise bites: a
binary you installed by hand just goes stale and nothing tells anyone.

Name the config file, the pin keys, the engine crate and a few other things in a
`const`, and the rest comes with it. Anything that's genuinely one tool's and
nobody else's goes through a named hook rather than into the crate, so the crate
stays the part every launcher shares.

The engine has to be a Rust crate `cargo install` can build, since that's what
the build path shells out to. The one other thing it owes is a flag taking an
absolute path, because the launcher always puts that flag and the working
directory in front of whatever the user typed, and the engine has to accept both.
The rest of it is the tool's own business.

## Status

Under active development, so the api hasn't settled and breaking changes should
be expected. It works and I use it for two tools daily, but I'd hold off putting
anything load-bearing on it just yet. `Tool::CONVENTIONS` is there so at least a
new field doesn't break you, and I'll try to document migrations properly when
the shape does move.

Unix only for now, and it's a build error elsewhere rather than a runtime
surprise. The handover is `exec` and there's no portable version of that, so
Windows would want a different design and not a different import, and I haven't
done that work.

## Installation

```bash
cargo add renki
```

Or in your `Cargo.toml`:

```toml
[dependencies]
renki = "0.0.2"
```

Do pin the exact version rather than a range. `Tool` is a struct literal, so a
new field on it is technically breaking even with `..Tool::CONVENTIONS` in
between, and `0.0.x` releases are incompatible with each other by semver's own
rules anyway.

You don't install `renki` itself as a command. It's a library, and what goes on
`PATH` is your own launcher built with it.

## Usage

Here's the whole of it, and it really is about this much:

```rust,no_run
use renki::{pin_keys, Tool};

const TOOL: Tool = Tool {
    short:           "widget",     // WIDGET_ROOT, WIDGET_CACHE, `widget: ...` on diagnostics
    config_file:     "widget.toml",
    pin_keys:        pin_keys!("widget"),   // widget_version, _rev, _branch, _tag, _git
    engine_crate:    "widget-engine",
    cache_namespace: "widget",     // ~/.cache/widget/, yours alone
    default_url:     "https://github.com/o/widget.git",
    launcher_crate:  "widget",     // how it finds itself in cargo's install ledger
    ..Tool::CONVENTIONS            // .git anchor, --dir, --engine, a `locate` query,
                                   // no hooks, and it chases its own branch
};

fn main() -> std::process::ExitCode {
    // SAFETY: first statement of main, before any thread exists
    unsafe { renki::run(&TOOL) }
}
```

`run` is unsafe because it drops the repo-location `GIT_*` variables out of the
environment before anything else, and removing an environment variable is
process-global. It's sound as the first statement of `main` and nowhere else, so
that's where it goes. If a `main` genuinely needs to do something first,
`run_without_sanitizing` is the same launcher without that step, and then keeping
those variables from confusing the engine is the caller's to handle.

A repo using it then carries:

```toml
# widget.toml
widget_version = "0.4.1"
```

and every `widget` run in that repo is version `0.4.1`, built once per version,
per source url and per toolchain, and shared by every repo landing on the same
three. Do note that a `rustup update` therefore rebuilds the cached engines next
time they're wanted, since the compiler really is part of the compilation input.

The config is TOML and only those few top-level keys get read out of it. The rest
of the file is yours and the launcher never looks at it.

## What's in it

`Tool` is the const describing one launcher, and `Tool::CONVENTIONS` is a base to
spread with `..` so only the fields that differ get named. Around it sit `Anchor`
for how the repo root gets found, `PinKeys` and the `pin_keys!` macro for what
the pin keys are called, `SelfUpdate`, `Hooks` and `Check` for the seven optional
places a tool does something no other tool needs, `Workdir`, `Cli` and `Locate`
for the flag and key spellings, `Pin` and `Reference` for what a repo pinned, and
`Resolved` for what that turned into. A `main` is `run` or
`run_without_sanitizing` and not much else.

The full surface with working links is on [docs.rs](https://docs.rs/renki),
which is a better place for it than a table here that goes stale the first time
something moves.

The one thing genuinely worth deciding rather than taking the default of is the
anchor, since it's the one thing that really differs between tools.
`Anchor::Marker(".git")` walks up to the nearest directory holding that name, and
suits a config living inside a repository. `Anchor::ConfigFile` walks up to the
nearest directory holding the config itself, and suits a config sitting above a
pile of repositories, where a marker anchor would stop at the first repo on the
way up and never reach it. If I picked one, the other kind of tool would just end
up working around it, so it stays a parameter.

## What the launcher answers on its own

`widget locate` prints the repo root, the config and the working directory, one
`key=value` per line, in the paths' own bytes. Read it from any shell script that
needs to know, instead of walking the tree a second time yourself. Two copies of
the same walk agree right up until they don't, and then you get to work out which
one is lying, which is not a fun afternoon. Split on the first `=`, and do quote
the values; a path with a space in it is still a path. The one thing the format
can't carry is a newline inside a path, which is legal on unix and would look
like two records, so that one gets refused by name rather than answered wrongly.

`--engine <path>` builds from a checkout on disk instead of the pin, always
rebuilding, recording nothing in the registry, and swept a day after its last
use. It's the flag for working on the engine itself. Without it you're pushing a
commit and waiting on a build to find out whether a one-liner took, and that gets
old fast.

`--dir` is the launcher's own, and a user-supplied one gets stripped before the
engine sees anything, so the engine never has two answers to choose between.

## What it keeps on disk

Everything lives under `$XDG_CACHE_HOME/<namespace>`, or `~/.cache/<namespace>`
when that isn't set: built engines, the resolved head of any branch pin, and a
small TOML registry.

The registry is worth knowing about, since it's the one thing here that records
something about you and not about a build. One row per repo that's run the tool
on this machine, and the row holds the lot of it: the repo root, its directory
name, whether that root was found exactly or by walking up, the working
directory the engine gets pointed at, the engine's source url, what was pinned
and in which form, the build key that resolved to, and when it last ran. That's what lets the collector tell a build nothing points at any more
from one that's still wanted. Nothing but the launcher writes it and nothing
sends it anywhere. It's plain TOML in your own cache directory, so go and read it
if you like, and deleting the whole cache directory is always safe.

Builds go two ways. One that nothing points at any more gets collected on the
next pass, which is what happens when a repo re-pins to a newer engine or the
repo is simply gone. One that's still pinned but that nobody has wanted for
`Tool::cache_retention`, thirty days by default, goes the same way.

Three environment variables, all named after your `short`:

| Variable | What |
|---|---|
| `WIDGET_ROOT` | Use this repo root instead of walking up for the anchor. |
| `WIDGET_CACHE` | Put the cache here. The whole path, not a parent to append the namespace to. |
| `WIDGET_NO_SELF_UPDATE` | Don't check whether the launcher itself has moved on. |

That last one is the user's own way out of the self-update. A version or tag
install is immutable so that one's left alone anyway, and `SelfUpdate::Never` in
your `Tool` turns the whole thing off for everybody if that's not wanted at all.

## What this isn't

If you're after a version manager, this probably isn't it. Not that there's no
overlap, but the proper ones do a lot this doesn't: toolchains, shims, opinions
about what your engine even is. This one is narrow. The repo names a version, the
tool that runs is that version, and that's about it.

Two dependencies, `serde` and `toml`, both with default features off. The pin
header gets read by hand since it's a handful of top-level keys, and the cache
key uses a small vendored FNV rather than a hashing crate. That likely means edge
cases in the header reading I haven't hit yet, and also that a `cargo install` of
somebody's launcher doesn't drag in a tree for the privilege.

## The name

`renki` is Finnish for a farmhand, the sort who does the fetching and carrying so
somebody else can get on with the actual work. Which is about the whole of this
crate's job, so it seemed to fit.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

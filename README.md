# `renki`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki)](https://crates.io/crates/renki)
[![docs.rs](https://img.shields.io/docsrs/renki)](https://docs.rs/renki)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> The launcher half of a pinned-engine command line tool. One const describes the tool, and the pin, cache, build and exec come with it.

</div>

## Status

Under active development, so the api hasn't settled and breaking changes should
be expected. It works and two tools already run on it, but I'd hold off building
anything load-bearing on it just yet. We'll try to document any migration
properly when the shape does move.

## What it is

A tool built this way comes in two halves. The engine does the actual work, and
each repo pins the version of it that it wants. The launcher is the small thing
that sits on `PATH`, and all it really does is find the repo, read the pin, build
that exact engine once into a shared cache, and hand over.

The point of the split is that a repo's tooling can't drift away from what the
repo asked for. Everyone on the project gets the version the config names, on
whatever machine, regardless of what they happened to install last year. And a
launcher installed off a git branch keeps itself current, so nobody has to
remember to, which is the part that otherwise bites: a hand-installed binary
goes stale quietly and nothing at all tells you. A version or tag install is
immutable, so that one is left alone, and `WIDGET_NO_SELF_UPDATE` turns the check
off either way.

`renki` is that launcher with the identity taken out. You write a `const` naming
the config file, the pin keys, the engine crate and a few other things, and the
rest comes with it. Anything genuinely one tool's and nobody else's goes through
a named hook instead of into the crate, so the crate keeps being honest about
what it actually knows.

The engine has to be a Rust crate that `cargo install` can build, since that is
what the build path shells out to. Everything else about it is yours.

## Contents

| Type | Purpose |
|---|---|
| `Tool` | The const describing one launcher: names, anchor, cache namespace, hooks. |
| `Anchor` | How the repo root gets found, walking up. A marker directory, or the config file itself. |
| `Hooks` | The places a tool does something no other tool needs. All optional. |
| `Check` | The shape of the two hooks that exist to refuse something. |
| `Workdir` | The subdirectory a config maps, for tools that have one. |
| `Cli` / `Locate` | The conventional flag spellings, and the key names the `locate` answer uses. Both yours to rename. |
| `Pin` / `Reference` | What a repo pinned: a version, a rev, a tag or a branch. |
| `Header` | The config keys the launcher reads, if you want to read them yourself. |
| `Resolved` | A pin turned into concrete build attempts, plus the git ref it landed on. |
| `run` / `run_without_sanitizing` | The launcher itself. Your `main` is one of these and not much else. |
| `GIT_REPO_ENV` / `sanitize_git_env` | The repo-location git variables `run` drops, and the function that drops them. |

## The two anchors

Finding the repo root is the one thing that genuinely differs between tools, and
neither answer generalises to the other, so it stays a parameter.

`Anchor::Marker(".git")` walks up to the nearest directory holding that entry.
Right when the config lives inside a repository. The config may then sit at the
root or one directory below it, and more than one in scope is a hard error rather
than a precedence question, mostly because nobody ever specified which should
win.

`Anchor::ConfigFile` walks up to the nearest directory holding the config file
itself. Right when the config sits above a pile of repositories rather than
inside one. A marker anchor would stop at the first repository on the way up and
never reach the config at all, and running that kind of tool from inside a member
repo is the normal way it gets used rather than some edge case.

## What the hooks are for

Six of them, all optional, and `Hooks::NONE` is a perfectly good answer for a
tool that needs none.

| Hook | When |
|---|---|
| `prepare_repo` | Something the tool keeps planted in a repo. Runs before the engine is built, so a failed build never leaves a repo unprepared. |
| `engine_args` | Extra arguments derived from the resolved pin, for handing the engine something that has to match the exact revision it was built from. |
| `engine_args_local` | The same, under `--engine <path>`, where the source is a working tree and there is no pin. |
| `verify_engine_dir` | Refuse an `--engine <path>` that isn't a checkout of your engine, reported against the flag the user actually passed rather than as a build failure about something else. |
| `legacy_pin` | A last-resort pin for a repo mid-migration that hasn't adopted an explicit one yet. |
| `verify_repo_state` | Refuse a repo state that would quietly route the user somewhere else. A retired cargo alias shadowing the launcher is the case this exists for. |

## Installation

```bash
cargo add renki
```

Or add to your `Cargo.toml`:

```toml
[dependencies]
renki = "0.0"
```

You don't install `renki` itself as a command. It's a library, and what goes on
`PATH` is your own launcher built with it.

## Usage

The whole thing:

```rust,no_run
use renki::{Anchor, Cli, Hooks, Locate, Tool};

const TOOL: Tool = Tool {
    anchor:          Anchor::Marker(".git"),   // widget.toml sits beside .git
    short:           "widget",                 // WIDGET_ROOT, and `widget: ...` on diagnostics
    config_file:     "widget.toml",
    pin_prefix:      "widget",                 // so: widget_version, widget_rev, widget_branch, widget_tag
    engine_crate:    "widget-engine",
    engine_bin:      None,        // the bin is named after the package
    cache_namespace: "widget",                 // ~/.cache/widget/, yours alone
    default_url:     "ssh://git@github.com/o/widget.git",
    launcher_crate:  "cargo-widget",           // how it finds itself in cargo's install ledger
    workdir:         None,                     // no subdirectory, the engine runs against the repo root
    dir_flag:        Cli::DIR_FLAG,
    engine_flag:     Cli::ENGINE_FLAG,
    locate:          Locate::DEFAULT,
    hooks:           Hooks::NONE,
};

fn main() -> std::process::ExitCode {
    // SAFETY: first statement of main, before any thread exists
    unsafe { renki::run(&TOOL) }
}
```

`run` is unsafe because it drops the repo-location `GIT_*` variables out of the
environment before doing anything else, and removing an environment variable is
process-global. It's sound as the first statement of `main` and nowhere else, so
that's where it goes. If your `main` genuinely needs to do something before it,
`run_without_sanitizing` is the same launcher without that step, and then keeping
those variables from confusing the engine is your problem.

A repo using it then carries:

```toml
# widget.toml
widget_version = "0.4.1"
```

and every `widget` run in that repo is version `0.4.1`, built once per version,
per source url and per toolchain, and shared by every repo that lands on the same
three. Worth knowing that a `rustup update` therefore rebuilds the cached engines
next time they're wanted, since the compiler really is part of the compilation
input.

The config is TOML, and only those few top-level keys are read out of it. The
rest of the file is yours and the launcher never looks at it.

### What you get for free

`widget locate` prints the repo root, the config and the working directory, one
`key=value` per line. Worth reading from any shell script that needs to know,
rather than walking the tree again, since a second implementation is how the two
come to disagree. Do quote the values when you use them; a path with a space in
it is still a path.

`--engine <path>` builds from a checkout on disk instead of the pin, always
rebuilding, recording nothing in the registry, and sweeping its own leftovers
after a day. For when you're working on the engine itself, where pushing to find
out whether a change took is a slow way to ask a fast question.

`--dir` is the launcher's own, and a user-supplied one gets stripped before the
engine sees anything, so the engine never has two answers to choose between.

## Positioning

If you want a version manager, this isn't one. It doesn't manage toolchains,
doesn't shim anything, and holds no opinion about what your engine does. It
solves one narrow thing, which is that the repo names a version and the tool that
runs is that version.

Unix only for now. The exec path is `execve` and nothing has been done about
Windows, so the build fails there rather than misbehaving.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/dev/LICENSE)

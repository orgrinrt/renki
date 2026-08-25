# `renki`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki)](https://crates.io/crates/renki)
[![docs.rs](https://img.shields.io/docsrs/renki)](https://docs.rs/renki)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> `renki` is the launcher half of a command-line tool whose engine each repo pins. Reads the pin, builds that exact version once into a shared cache, and hands over.

</div>

## The split, and what it buys

A tool built this way comes in two halves. The engine does the actual work, and
each repo pins the version of it that it wants. The launcher is the small thing
that sits on `PATH`, and all it really does is find the repo, read the pin, build
that exact engine once into a shared cache, and hand over.

The point of the split is that a repo's tooling can't drift away from what the
repo asked for. Everyone on the project gets the version the config names, on
whatever machine, regardless of what they happened to install last year. And a
launcher installed off a git branch can keep itself current, so nobody has to
remember to, which is the part that otherwise bites: a hand-installed binary goes
stale quietly and nothing at all says so.

`renki` is that launcher with the identity taken out. You write a `const` naming
the config file, the pin keys, the engine crate and a few other things, and the
rest comes with it. Anything genuinely one tool's and nobody else's goes through
a named hook instead of into the crate. That keeps the crate to the things
every launcher does, which is also what makes it possible to say what it does
without a list of exceptions.

The engine has to be a Rust crate that `cargo install` can build, since that is
what the build path shells out to. The one other thing it owes is a flag taking
an absolute path, because the launcher always puts that flag and the working
directory in front of whatever the user typed, and the engine has to accept both.
Everything else about it is the tool's own.

## Status

This crate is under active development, so the api hasn't settled and breaking
changes should be expected. It works, but I'd hold off building anything
load-bearing on it just yet. We'll try to document any migration properly when
the shape does move, and `Tool::CONVENTIONS` is there so that at least a new
field doesn't break you.

## Contents

| Type | Purpose |
|---|---|
| `Tool` | The const describing one launcher: names, anchor, cache namespace, hooks. |
| `Tool::CONVENTIONS` | A base to spread with `..`, so only the fields that actually differ get named. |
| `Anchor` | How the repo root gets found, walking up. A marker directory, or the config file itself. |
| `PinKeys` / `pin_keys!` | What the pin keys in a repo's config are called. The macro gives the conventional `<prefix>_version` shape. |
| `SelfUpdate` | Whether the launcher chases its own branch, or leaves itself alone. |
| `Hooks` | The places a tool does something no other tool needs. All optional. |
| `Check` | The shape of the two hooks that exist to refuse something. |
| `Workdir` | The subdirectory a config maps, for tools that have one. |
| `Cli` / `Locate` | The conventional flag spellings, and the key names the `locate` answer uses. Both renameable. |
| `Pin` / `Reference` | What a repo pinned: a version, a rev, a tag or a branch. |
| `Resolved` | A pin turned into concrete build attempts, plus the git ref it landed on. |
| `run` / `run_without_sanitizing` | The launcher itself. Your `main` is one of these and not much else. |

A few smaller doors are there for doing a piece by hand rather than taking the
one that comes with it: `Header` reads the config keys, `package_name` reports
what a `Cargo.toml` in some directory declares itself to be, and
`GIT_REPO_ENV` with `sanitize_git_env` is the set of repo-location git variables
`run` drops and the function that drops them. The rustdoc has them.

## The two anchors

Finding the repo root is the one thing that genuinely differs between tools. A
config that lives in a repository wants one answer and a config that lives
wherever it was invoked wants another, and picking one of them would just mean
the other tool works around it. So it stays a parameter.

`Anchor::Marker(".git")` walks up to the nearest directory holding that name.
Right when the config lives inside a repository. The config may then sit at the
root or one directory below it, and more than one in scope is a hard error rather
than a precedence question, mostly because nobody ever specified which should
win. One directory below is the whole depth, so a config buried at
`tools/widget/widget.toml` won't be found by this one. Which subdirectories get
looked into at all is `Tool::scan_skip`, and it's worth setting if your repos
carry a vendored tree or a build output directory, because a stray file with your
config's name in there is a hard error rather than a scan result.

`Anchor::ConfigFile` walks up to the nearest directory holding the config file
itself. Right when the config sits above a pile of repositories rather than
inside one. A marker anchor would stop at the first repository on the way up and
never reach the config at all, and running that kind of tool from inside a member
repo is the normal way it gets used rather than some edge case.

## What the hooks are for

Seven of them, all optional, and `Hooks::NONE` is a perfectly good answer for a
tool that needs none.

| Hook | When |
|---|---|
| `prepare_repo` | Something the tool keeps planted in a repo. Runs before the engine is built, so a failed build never leaves a repo unprepared. |
| `engine_args` | Extra arguments derived from the resolved pin, for handing the engine something that has to match the exact revision it was built from. |
| `engine_args_local` | The same, under `--engine <path>`, where the source is a working tree and there is no pin. |
| `verify_engine_dir` | Refuse an `--engine <path>` that isn't a checkout of your engine, reported against the flag the user actually passed rather than as a build failure about something else. |
| `legacy_pin` | A last-resort pin for a repo mid-migration that hasn't adopted an explicit one yet. |
| `version_tags` | The tag names a released version might be under, when your engine's repo doesn't tag the bare version. `v0.1.0` is at least as common as `0.1.0`, and without this a version pin can't fall back to a tag at all. |
| `verify_repo_state` | Refuse a repo state that would quietly route the user somewhere else. A retired cargo alias shadowing the launcher is the case this exists for. |

## Installation

```bash
cargo add renki
```

Or in your `Cargo.toml`:

```toml
[dependencies]
renki = "0.0.1"
```

`Tool` is a struct literal, so a field added to it is technically breaking even
with `..Tool::CONVENTIONS` in between, and `0.0.x` releases are incompatible with
each other by semver's own rules anyway. So do pin the exact version rather than
a range.

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
    ..Tool::CONVENTIONS            // .git anchor, --dir, --engine, a `locate` query, no hooks
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

### What the launcher answers on its own

`widget locate` prints the repo root, the config and the working directory, one
`key=value` per line, in the paths' own bytes. Worth reading from any shell script
that needs to know, rather than walking the tree again. Two implementations of
the same walk stay in step right up until one of them doesn't, and then it is
not obvious which one is wrong. Split on the first `=`, and do
quote the values when you use them; a path with a space in it is still a path.
The one thing the format can't carry is a newline inside a path, which is legal
on unix and would look like two records, so that one is refused by name instead
of answered wrongly.

`--engine <path>` builds from a checkout on disk instead of the pin, always
rebuilding, recording nothing in the registry, and getting swept a day after you
last used it. For when you're working on the engine itself. Otherwise you're
pushing a commit and waiting on a build just to find out whether a one-line
change took, and that gets old fast.

`--dir` is the launcher's own, and a user-supplied one gets stripped before the
engine sees anything, so the engine never has two answers to choose between.

### What it keeps on disk, and where

Everything lives under `$XDG_CACHE_HOME/<namespace>`, or `~/.cache/<namespace>`
when that isn't set. Built engines, the resolved head of any branch pin for an
hour at a time, and a small TOML registry.

The registry is worth knowing about, since it's the one thing that records
something about you rather than about a build. It holds a row per repo that has
run the tool on this machine: the repo's path, its directory name, what it pinned
and when it last ran. That's what lets the collector tell a build nothing points
at any more from one that's still in use. Nothing but the launcher writes it and
nothing sends it anywhere. It's plain TOML in your own cache directory, so read
it yourself if you're curious, and deleting the whole cache directory is always
safe. A build that no repo has wanted for `Tool::cache_retention`, thirty days by
default, gets collected.

Three environment variables, all named after your `short`:

| Variable | What |
|---|---|
| `WIDGET_ROOT` | Use this repo root instead of walking up for the anchor. |
| `WIDGET_CACHE` | Put the cache here. The whole path, not a parent to append the namespace to. |
| `WIDGET_NO_SELF_UPDATE` | Don't check whether the launcher itself has moved on. |

That last one is the user's own way out of the self-update. A version or tag
install is immutable so that one is left alone anyway, and `SelfUpdate::Never` in
your `Tool` turns the whole thing off for everybody if you'd rather it didn't
happen at all.

## What this isn't

If you're after a version manager, this probably isn't it. Not that there's no
overlap, but the proper ones do a lot this doesn't: toolchains, shims, opinions
about what your engine even is. What this does is narrow, the repo names a
version and the tool that runs is that version, and that's about the whole of it.

Unix only for now. The handover is `exec`, which is a unix call with no portable
equivalent, and nothing has been done about Windows, so the build fails there
with a message saying as much rather than misbehaving.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

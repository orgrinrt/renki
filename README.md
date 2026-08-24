# `renki`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki)](https://crates.io/crates/renki)
[![docs.rs](https://img.shields.io/docsrs/renki)](https://docs.rs/renki)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> The launcher half of a pinned-engine command-line tool. One const describes your tool, and the pin, cache, build and exec are handled.

</div>

## Status

Pre-release, and the api hasn't settled. It works and it is used, but expect
breaking changes and don't build anything load-bearing on it just yet.

## What it is

A tool built this way comes in two halves. The engine does the actual work and
each repo pins the version of it that it wants. The launcher is the thing that
sits on `PATH`, and all it does is find the repo, read the pin, build that exact
engine once into a shared cache, and hand over to it.

The point is that a repo's tooling can't drift away from what the repo asked
for. Everyone gets the version the config names, on any machine, whatever they
happened to install last year. And the launcher keeps itself up to date, so
nobody has to remember to, which is the part that actually bites: a hand
installed binary goes stale quietly and there's nothing at all to tell you.

`renki` is that launcher, minus the identity. You write a `const` naming your
config file, your pin keys, your engine crate and a few other things, and you
get the rest. Anything that's genuinely yours and nobody else's goes through a
named hook instead of into the crate, so the crate stays honest about what it
actually knows.

## Contents

| Type | Purpose |
|---|---|
| `Tool` | The const describing one launcher: names, anchor, cache namespace, hooks. |
| `Anchor` | How the repo root is found, walking up. A marker directory, or the config file itself. |
| `Hooks` | The places your tool does something no other tool needs. All optional. |
| `Workdir` | The subdirectory your config maps, for tools that have one. |
| `Pin` / `Reference` | What a repo pinned: a version, a rev, a tag or a branch. |
| `Resolved` | A pin turned into concrete build attempts, plus the git ref it landed on. |
| `run` | The whole launcher. Your `main` is this and nothing else. |

## The two anchors

Finding the repo root is the one thing that genuinely differs between tools, and
neither answer generalises to the other, so it's a parameter.

`Anchor::Marker(".git")` walks up to the nearest directory holding that entry.
Right when your config lives inside a repository. The config may then sit at the
root or one directory below it, and more than one in scope is a hard error
rather than a precedence question, because nobody ever specified which should
win.

`Anchor::ConfigFile` walks up to the nearest directory holding the config file
itself. Right when your config sits above a pile of repositories rather than
inside one. A marker anchor would stop at the first repository on the way up and
never reach the config at all, and running that kind of tool from inside a
member repo is the normal way it gets used, not some edge case.

## What the hooks are for

Six of them, all optional, and `Hooks::NONE` is a perfectly good answer.

| Hook | When |
|---|---|
| `prepare_repo` | Something your tool keeps planted in a repo. Runs before the engine is built, so a failed build never leaves a repo unprepared. |
| `engine_args` | Extra arguments derived from the resolved pin, for handing the engine something that has to match the exact revision it was built from. |
| `engine_args_local` | The same, under `--engine <path>`, where the source is a working tree and there's no pin. |
| `verify_engine_dir` | Refuse an `--engine <path>` that isn't a checkout of your engine, reported against the flag the user actually passed. |
| `legacy_pin` | A last resort pin for a repo mid migration that hasn't adopted an explicit one yet. |
| `verify_repo_state` | Refuse a repo state that would quietly route the user somewhere else. A retired cargo alias shadowing the launcher is the case this exists for. |

## Installation

You don't install `renki`. It's a library, and what goes on `PATH` is your own
launcher built with it.

```bash
cargo add renki
```

## Usage

The whole thing:

```rust
use renki::{Anchor, Hooks, Tool};

const TOOL: Tool = Tool {
    // widget.toml lives in the repo, beside .git
    anchor:          Anchor::Marker(".git"),
    // names the diagnostics (`widget: ...`) and the env vars (WIDGET_ROOT)
    short:           "widget",
    config_file:     "widget.toml",
    // so the pin keys are widget_version, widget_rev, widget_branch, widget_tag
    pin_prefix:      "widget",
    engine_crate:    "widget-engine",
    // ~/.cache/widget/, yours alone
    cache_namespace: "widget",
    default_url:     "ssh://git@github.com/o/widget.git",
    // how this launcher recognises its own entry in cargo's install ledger
    launcher_crate:  "widget",
    // no subdirectory: the engine runs against the repo root
    workdir:         None,
    hooks:           Hooks::NONE,
};

fn main() -> std::process::ExitCode {
    renki::run(&TOOL)
}
```

A repo using it then carries:

```toml
# widget.toml
widget_version = "0.4.1"
```

and every `widget` run in that repo is version `0.4.1`, built once per version
per machine and shared by every repo pinned to it.

### What you get for free

`widget locate` prints where the config and working directory are, as lines you
can `eval`. Worth using from any shell script that needs to know, rather than
walking the tree again: a second implementation is how the two drift apart.

`--engine <path>` builds from a checkout on disk instead of the pin, always
rebuilding, keeping nothing and recording nothing. For when you're working on
the engine itself and pushing to find out whether a change took is a slow way to
ask a fast question.

`--dir` is the launcher's, and a user-supplied one gets stripped before the
engine sees anything.

## Positioning

If you want a version manager, this isn't one. It doesn't manage toolchains,
doesn't shim anything, and has no opinion about what your engine does. It solves
one narrow thing: the repo names a version and the tool that runs is that
version.

Unix only for now. The exec path is `execve` and nothing has been done about
Windows.

## Support

Issues and pull requests are welcome. If you're thinking of a big one, throw an
issue in first describing what you'd do, since it might not be something that
belongs here, and that's a lot of work to find out afterwards. Forks are always
fine too, just mind the license.

## License

MPL-2.0. See [LICENSE](LICENSE).

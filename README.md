# `renki`

<div align="center" style="text-align: center;">

[![GitHub Stars](https://img.shields.io/github/stars/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/stargazers)
[![Crates.io](https://img.shields.io/crates/v/renki)](https://crates.io/crates/renki)
[![docs.rs](https://img.shields.io/docsrs/renki)](https://docs.rs/renki)
[![GitHub Issues](https://img.shields.io/github/issues/orgrinrt/renki.svg)](https://github.com/orgrinrt/renki/issues)
![License](https://img.shields.io/github/license/orgrinrt/renki?color=%23009689)

> A library for building the launcher half of a two-part command line tool. Repo discovery, the version pin, a shared cache, a backend contract and the handover. Unix only, four dependencies, two of them ours.

</div>

I keep writing tools that come in two halves, and got tired of writing the same
half twice over. The engine does the actual work and each repo pins the version
of it that it wants; the launcher is the small thing on `PATH` that finds the
repo, reads the pin, builds that exact engine once into a shared cache and hands
over to it. This crate is that second half, with everything specific to any one
tool taken back out of it.

The reason to split it up this way is that a repo's tooling then can't drift off
from what the repo actually asked for. Everyone on the project gets the version
the config names, whichever machine they're on and whatever they happened to
install last year. A launcher installed off a git branch also keeps itself
current, which is the bit that otherwise gets you: a hand-installed binary just
goes stale quietly, and usually nobody notices until something starts behaving
oddly.

Name the config file, the pin keys, the engine crate and a few other things in a
`const`, and the rest comes with it. Anything that's genuinely one tool's and
nobody else's goes through a named hook instead of into the crate, so what's
left here stays the part every launcher shares.

The engine has to be a rust crate that `cargo install` can build, since that's
what the build path shells out to. Past that it owes one thing, a flag taking an
absolute path, because the launcher always puts that flag and the working
directory in front of whatever the user typed, and the engine has to accept
both. The rest of it is the tool's own business.

## Status

Under active development, so the api hasn't settled and breaking changes should
be expected. It works and I use it for two tools daily, but I'd hold off putting
anything load-bearing on it just yet. `Tool::CONVENTIONS` is there so that at
least a new field doesn't break you, and I'll do my best to document the
migrations properly when the shape does move.

Unix only for now, and it's a build error elsewhere rather than a runtime
surprise. The handover is `exec` and there's no portable version of that, so
windows would want a different design and not a different import, and that work
I haven't done.

## Installation

```bash
cargo add renki
```

Or in your `Cargo.toml`:

```toml
[dependencies]
renki = "0.0.2"
```

Do pin the exact version rather than a range. `0.0.x` releases are incompatible
with each other by semver's own rules anyway, and `Tool` is a struct literal, so
a new field on it breaks you unless you spread `..Tool::CONVENTIONS` and let the
base answer the ones you've got no opinion about.

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

// checked at build time, not on the first run: every name here ends up in a
// path, a command line or a config key, and an empty one runs and misbehaves
const _: () = assert!(TOOL.defect().is_none());

fn main() -> std::process::ExitCode {
    // SAFETY: first statement of main, before any thread exists
    unsafe { renki::run(&TOOL) }
}
```

`run` is unsafe because it drops the repo-location `GIT_*` variables out of the
environment before anything else happens, and removing an environment variable
is process-global. It's sound as the first statement of `main` and nowhere else,
so that's where it goes. If a `main` genuinely has to do something first, then
`run_without_sanitizing` is the same launcher without that step, and keeping
those variables from confusing the engine is yours to deal with instead.

A repo using it then carries:

```toml
# widget.toml
widget_version = "0.4.1"
```

and every `widget` run in that repo is version `0.4.1`, built once per version,
per source url and per toolchain, and shared by every repo that lands on the
same three. Do note that a `rustup update` therefore rebuilds the cached engines
next time they're wanted, since the compiler really is part of the compilation
input.

The config is TOML and only those few top-level keys get read out of it. The
rest of the file is yours and the launcher never so much as looks at it.

## What's in it

`Tool` is the const that describes one launcher, and `Tool::CONVENTIONS` is a
base to spread with `..` so that only the fields which actually differ get
named. Around it sit `Anchor` for how the repo root gets found, `PinKeys` and
the `pin_keys!` macro for what the pin keys are called, `Workdir` and `Locate`
for the config key and the answer keys, `Cli` for the two flag spellings, and
`VersionSource` for where a version pin is allowed to resolve from. `Hooks` is
the seven optional places a tool does something no other tool needs, with
`Check` naming the shape two of them take, and `SelfUpdate` decides whether the
launcher chases its own branch at all. Then `Pin` and `Reference` for what a
repo pinned, `Resolved` for what that turned into, and `package_name` for when
you're writing the hook that has to tell whether some directory really is a
checkout of your engine. A `main` is `run` or `run_without_sanitizing` and not
much besides.

The full surface with working links is over on [docs.rs](https://docs.rs/renki),
which is probably a better place for it than a table here that goes out of date
the moment anything moves.

Two things are worth actually deciding on rather than just taking the default.

The anchor, since it's the one thing that really does differ between tools.
`Anchor::Marker(".git")` walks up to the nearest directory holding that name,
and suits a config living inside a repository. `Anchor::ConfigFile` walks up to
the nearest directory holding the config itself, and suits a config sitting
above a whole pile of repositories, where a marker anchor would stop at the
first repo on the way up and never get to it. If I picked one for you, the other
kind of tool would just end up working around it, so it stays a parameter.

And `version_source`, which says where a `version` pin may look. A rev, a tag or
a branch all name something inside the url you pinned. A version could mean that
same repo's tag of the same name, or it could mean a crates.io release of your
`engine_crate`, and `cargo install` resolves that one by name with nothing at
all tying the name to your url. So the default is `VersionSource::GitTag`, the
tag and nothing else. If the name isn't yours on crates.io, and it isn't while
you're starting out, then whoever ends up taking it decides what your engine is.
Switch over to `RegistryThenGitTag` once you own the name and want the faster
cold build. Do note that it's a promise you're making about that name, and not
something anyone can check on your behalf.

## Backends and extensions

Fetching and building is a contract. `Backend` is a trait of associated
functions with no `self`, since a backend is a policy and not a value, and
`Registered::of::<B>()` is its const fn-pointer form, so a host holds a
`&'static [Registered]` of several with no allocation and no dynamic dispatch.
`Cargo`, `Local` and `Git` ship, and yours goes beside them.

A backend says three things about itself. `fingerprint` is whatever about its
own environment belongs in the cache key, so a compiler that moved re-keys and
forces a coherent rebuild, and a backend that only copies bytes returns nothing.
`materialise` puts the material somewhere. `places_itself` says whether it takes
the destination directly or a scratch that gets renamed into place once it's
finished, which is where they actually differ: `cargo install --root` holds
cargo's own lock over the install root and moves the binary in itself, so a
scratch around that would be a second, weaker mechanism over a working one, and
would discard the incremental target directory on every build. Everything
fetched takes the scratch, which is what keeps a reader from seeing a
half-written tree.

Extensions are the other half. A host reads tool descriptors out of its own
config, so the tools are heterogeneous, the backend is named by a string rather
than known at compile time, and listing what a tool offers mustn't fetch
anything. So the api splits on whether it touches the disk: `Descriptor` is a
parsed `tool.toml`, `Located` is what it becomes once a backend has put it
somewhere, and only running resolves.

```toml
[tool]
name    = "rules"
summary = "read the workspace rules"
tags    = ["rules", "docs"]
backend = "git"
promote = true

[tool.source]
git = { url = "https://github.com/o/rules.git", rev = "0123456789abcdef0123456789abcdef01234567" }

[[tool.commands]]
name    = "list"
summary = "every rule"
run     = "commands/list"
```

The commands sit in the descriptor rather than behind the tool's own `--help`,
which lets a host print what a tool offers without fetching it, and means the
list can't drift from what dispatches, since this is what dispatches. `run` is a
path and not a shell string, so a command is a file with a shebang and a test on
it, where shell inside a toml value is unquotable and invisible to every lint
you own.

A descriptor can arrive from a git ref, so `Descriptor::check` refuses values
that would read as something other than data: a `rev` that's a flag to git, a
url on no scheme it knows, a `run` that's absolute or climbs out of the tool. It
runs at parse and again at `locate` and `command`, since the fields are public
and a descriptor deserialises straight out of toml, so one reaches either
without having been parsed at all.

## What the launcher answers on its own

`widget locate` prints the repo root, the config and the working directory, one
`key=value` per line, in the paths' own bytes. If a shell script of yours needs
to know any of those, read it from here instead of walking the tree a second
time yourself; two walks of the same tree agree right until the day they don't,
and then somebody gets to work out which of the two is lying. Split on the first
`=`, and do quote the values, a path is allowed to have spaces in it. A key with
nothing after it means there isn't one, so no config, or no working directory.
The one thing the format can't carry is a newline inside a path, which is legal
on unix and would read as two records, so a path with one in it gets refused by
name rather than answered wrongly.

`--engine <path>` builds from a checkout on disk instead of the pin, always
rebuilding, recording nothing in the registry, and swept a day after its last
use. It's the flag for working on the engine itself. Without it you're pushing a
commit and waiting on a build just to find out whether a one-liner took, and
that gets old fast.

`--dir` is the launcher's own, and a user-supplied one gets stripped before the
engine sees anything at all, so the engine never has two answers to pick
between.

## What it keeps on disk

Two directories, and which one a file goes in is decided by what losing it
costs. The cache holds what gets rebuilt without anybody asking: built engines,
materialised tools, the resolved head of any branch pin. The state holds the one
thing the launcher writes for itself and would behave differently without, a
small TOML registry, plus the self-update marker. A cleanup that empties the
cache costs you a rebuild and nothing else, which is the whole point of keeping
the two apart.

Where they are is the platform's call, through `renki-dirs`, which is the same
table every tool of ours reads. On linux and the BSDs that's
`$XDG_CACHE_HOME/<namespace>` and `$XDG_STATE_HOME/<namespace>`, or `~/.cache`
and `~/.local/state` under them when the variables aren't set. On macOS it's
`~/Library/Caches/<namespace>` and `~/Library/Application Support/<namespace>/state`,
since that's where the platform's own cleanup looks and `~/.cache` there is
nobody's cache; an exported XDG variable still wins on a mac, on the reasoning
that somebody who set one has said where they want their files.

The registry is the one worth knowing about, since it's the only thing here that
records something about you rather than about a build. One row per repo that has
run the tool on this machine, and the row holds the lot of it: the repo root,
its directory name, whether that root survived being written down as text
without anything in it getting replaced, the working directory the engine gets
pointed at, the engine's source url, what was pinned and in which form, the
build key that resolved to, and when it last ran. That's what lets the collector
tell a build nothing points at any more from one that's still wanted. Nothing
but the launcher writes it and nothing sends it anywhere. It's plain TOML in
your own state directory, so go and read it if you like, and deleting the whole
cache directory is always safe.

Builds get collected two ways. One that nothing points at any more goes on the
next pass, which is what happens when a repo re-pins to a newer engine, or when
the repo is simply gone. One that's still pinned but that nobody has wanted for
`Tool::cache_retention`, thirty days by default, goes the same way.

Materialised tools sit beside the builds under `tools/`, keyed on the source,
the revision and whatever the backend said about its own environment. The
workspace is not in that key, so twenty workspaces on the same tool at the same
revision share one copy.

Those age out on last use rather than on a registry row, since nothing records
who wants a tool, and the same `cache_retention` applies. One with no mark yet
gets stamped instead of taken. A scratch left by a fetch that died goes after an
hour.

Six environment variables, all named after your `short`:

| Variable | What |
|---|---|
| `WIDGET_ROOT` | Use this repo root instead of walking up for the anchor. |
| `WIDGET_CACHE` | Put the cache here. The whole path, not a parent to append the namespace to. |
| `WIDGET_STATE` | Put the registry and the marker here. The whole path, same as the cache. |
| `WIDGET_NO_SELF_UPDATE` | Don't check whether the launcher itself has moved on. |
| `WIDGET_WORKSPACE` | Set on a tool command, naming the workspace it acts on. |
| `WIDGET_TOOL_ROOT` | Set alongside it, naming that tool's materialised root. |

The last two are what a tool command inherits. A tool's code sits in a cache
shared by every workspace on the machine and its data does not, so it can't
work a data path out from where it happens to be installed, and knows which
workspace it's on because it was told.

`WIDGET_NO_SELF_UPDATE` is the way out of the self-update, which otherwise
checks at most once an hour. A version, tag or rev install is immutable so
that one is left alone anyway, and `SelfUpdate::Never` in your `Tool` turns
it off for everybody if it isn't wanted at all.

## What this isn't

If you're after a version manager, this probably isn't it. Not that there's no
overlap, but the proper ones do a lot that this doesn't: toolchains, shims,
opinions about what your engine even is. This one is narrow on purpose. The repo
names a version, the tool that runs is that version, and that's about it.

Two dependencies, `serde` and `toml`, both with default features off. The pin
header gets read by hand since it's a handful of top-level keys, and the cache
key uses a small vendored FNV rather than a hashing crate. That likely means
edge cases in the header reading I haven't hit yet, but also that a
`cargo install` of somebody's launcher doesn't drag in a whole tree for it.

## Support

Whether you use this project, have learned something from it, or just like it, please consider supporting it by buying me a coffee, so I can dedicate more time on open-source projects like this :)

<a href="https://buymeacoffee.com/orgrinrt" target="_blank"><img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me A Coffee" style="height: auto !important;width: auto !important;" ></a>

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

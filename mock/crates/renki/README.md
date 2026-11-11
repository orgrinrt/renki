# `renki`

[![Crates.io](https://img.shields.io/crates/v/renki)](https://crates.io/crates/renki) [![docs.rs](https://img.shields.io/docsrs/renki)](https://docs.rs/renki)

> A library for building the launcher half of a two-part command line tool. Repo discovery, the version pin, a shared cache, a backend contract and the handover. Unix only, six dependencies, three of them ours.

Tools that come in two halves, an engine that does the work and a launcher that finds the repo, reads the version the repo pins, builds that exact engine once into a shared cache and hands over to it, end up with the same second half every time. `renki` is that half, with everything specific to any one tool taken back out of it. A const `Tool` names the config file, the pin keys, the engine crate and a few other things, and the rest comes with it. Anything that is one tool's and nobody else's goes through a named hook.

The engine has to be a rust crate that `cargo install` can build, since that is what the build path shells out to, and it has to accept a flag taking an absolute path, because the launcher always puts that flag and the working directory in front of whatever the user typed. It's unix only, and a build error elsewhere, since the handover is `exec`. It's under active development and the api hasn't settled, so I'd not put anything load-bearing on it yet.

## Usage

```bash
cargo add renki
```

```toml
[dependencies]
renki = "0.0.5-rc.1"
```

Pin the exact version rather than a range, since `0.0.x` releases are incompatible with each other anyway, and a new field on `Tool` breaks the build unless the literal spreads `Tool::CONVENTIONS` and lets the base answer the ones it has no opinion about. `renki` is a library and not installed as a command. What goes on `PATH` is the launcher built with it.

A launcher for a tool called `widget` comes to about this much:

```rust,no_run
use renki::{pin_keys, Tool};

const TOOL: Tool = Tool {
    short:           "widget",     // WIDGET_ROOT, WIDGET_CACHE, `widget: ...` on diagnostics
    config_file:     "widget.toml",
    pin_keys:        pin_keys!("widget"),   // widget_version, _rev, _branch, _tag, _git
    engine_crate:    "widget-engine",
    cache_namespace: "widget",     // ~/.cache/widget/, the tool's alone
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

`run` is unsafe because it drops the repo-location `GIT_*` variables out of the environment before anything else happens, which is process-global, so it belongs as the first statement of `main` and nowhere else. A `main` that has to do something first can use `run_without_sanitizing` and keep those variables from confusing the engine itself.

A repo using it then carries a `widget_version = "0.4.1"` in its `widget.toml`, and every `widget` run in that repo is that version, built once per version, source url and toolchain, and shared by every repo that lands on the same three. A `rustup update` rebuilds the cached engines next time they're wanted, since the compiler is part of the compilation input. Only the pin keys get read from the config, and the rest of the file belongs to the repo.

The anchor is worth deciding rather than taking the default. `Anchor::Marker` walks up to the nearest directory holding a given name and suits a config inside a repository, and `Anchor::ConfigFile` walks up to the nearest directory holding the config itself, which suits a config sitting above a pile of repositories. The `VersionSource::GitTag` default resolves a version pin to the tag of the same name in the pinned url and nothing else, and `RegistryThenGitTag` is for once the engine's name on crates.io is yours.

The launcher answers `widget locate` itself, printing the repo root, the config and the working directory as `key=value` lines for scripts to read. `--engine <path>` builds from a checkout instead of the pin, for working on the engine, and `widget config` reads and writes the tool's settings through `renki-config`. A tool can add questions of its own with `Tool::commands`, which run before the launcher looks for a root.

## Where it keeps things

Builds, materialised tools and the resolved head of any branch pin go in the cache, and the one small registry of which repos have run the tool goes in the state directory, both placed by `renki-dirs`. Emptying the cache only costs a rebuild, so deleting it is always safe. A build nothing points at any more is collected on the next pass, and one still pinned that nobody has wanted for `Tool::cache_retention`, thirty days by default, goes the same way.

The environment variables are all named after the tool's short name. `WIDGET_ROOT` replaces the walk up for the anchor, and `WIDGET_CACHE` and `WIDGET_STATE` put the cache and the registry at a whole path, not a parent. `WIDGET_CONFIG` does the same for the person's config directory, and `WIDGET_NO_SELF_UPDATE` skips the check on whether the launcher itself has moved on, which is otherwise done at most once an hour. A tool command is handed `WIDGET_WORKSPACE` and `WIDGET_TOOL_ROOT`, and the engine reads one `WIDGET_CFG_` variable per setting, like `WIDGET_CFG_MODEL_BASE` for `model.base`, and `WIDGET_CONFIG_FILE` naming the settings file.

Fetching and building is a contract, `Backend`, with `Cargo`, `Local` and `Git` shipping and a tool's own going beside them. A host reads tool descriptors out of its own config, where `Descriptor` is a parsed `tool.toml` and `Located` is what it becomes once a backend has put it somewhere. `Descriptor::check` refuses values that would read as something other than data, like a `rev` that's a flag to git or a `run` that climbs out of the tool.

This is not a version manager. It has no toolchains and no shims, and the repo names a version and that is the one that runs.

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

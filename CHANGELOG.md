# Changelog

## 0.0.4, 2026-10-28

Two launchers asking for the same engine install no longer build it twice.
`renki-dirs` and `renki-config` move to `0.0.2`, since six of their public
types took new names.

### fix

- build one engine install at a time, under a lock per key, so a second launcher waits for the first and reuses its result
- build each key in a target directory of its own, so an interrupted build resumes and a stuck one blocks only its own key
- spare a build in progress when old builds are collected
- build the scratch `--engine` install under the toolchain of the repository it is run for

### refactor

- rename `Kind` to `DirKind`, `Source` to `PathSource` and `Sealed` to `DirsSealed` in `renki-dirs`
- rename `Source` to `Origin`, `Resolved` to `ResolvedView` and `Sealed` to `ConfigSealed` in `renki-config`

### docs

- document the directory `CargoPlan` builds in, and which toolchain the build fingerprint reads

### test

- cover the lock being exclusive per key, a waiting launcher finding the holder's binary, and the collector sparing a build in progress
- cover a build compiling into a target directory inside its own key

## 0.0.3, 2026-09-04

The first release of `renki-dirs` and `renki-config`, at `0.0.1` each, and
the launcher taking both from the registry.

### feat

- a tool declares the subcommands its launcher answers itself
- `renki-config`, the contracts a tool's settings are read through
- `renki-dirs`, the typed table of where a tool's files go

### fix

- install an engine and the launcher under the source's own lockfile

### docs

- the rendered documents name no round

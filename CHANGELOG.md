# Changelog

## 0.0.5-rc.4 (2026-11-23)

### fix

- fix: re-export the unit helpers to the tests only `c518dec`
- fix: build each git revision from its own sources in the shared dir `15b1dd2`

### docs

- docs: add the source changelist for the unit re-export again `8a18bee`
- docs: open a round for the unit re-export again `ce6daed`
- docs: add the source changelist for the rc.4 readme `7df6201`
- docs: pin rc.4 in the readme ahead of its release `8bddffd`
- docs: open a round for the readme ahead of rc.4 `4224afe`
- docs: add the source changelist for the unit re-export `f4c6d03`
- docs: open a round for the unit re-export the tests read `e41577f`

### chore

- chore: prepare renki 0.0.5-rc.4 `e6bf23e`
- chore: prepare renki 0.0.5-rc.4 `b1136e5`
- chore: close design round 202611222150-f0cy `1827b77`
- chore: lock src changelist 202611222150-f0cy `5728d2c`
- chore: lock doc changelist 202611222150-f0cy `b7892ab`
- chore: close design round 202611222148-fm3h `1a1d368`
- chore: lock src changelist 202611222148-fm3h `0e63730`
- chore: lock doc changelist 202611222148-fm3h `3693dc7`
- chore: ship `renki` as `0.0.5-rc.4` `2361d93`
- chore: close design round 202611222147-x34l `32cb14c`
- chore: lock src changelist 202611222147-x34l `9fa9e41`
- chore: lock doc changelist 202611222147-x34l `b24b5b6`

## 0.0.5-rc.3 (2026-11-19)

### fix

- fix: run no cargo from a launcher a build script started `75269b4`
- fix: build one pin once whatever toolchain the caller picked `0bcaa23`

### docs

- docs: pin rc.3 in the readme ahead of its release `7f2683c`

### chore

- chore: ship `renki` as `0.0.5-rc.3` `06974c6`

## 0.0.5-rc.2 (2026-11-16)

### fix

- fix: build engines in the shared build dir and keep one per pin `bef7c41`

## 0.0.5-rc.1, 2026-11-11

A release candidate. The launcher's self-update reinstalls into a target
directory under the cache, so it stops leaving `cargo install` scratch
directories behind in `/tmp`. `renki-dirs` and `renki-config` stay at `0.0.2`.

### fix

- reinstall the launcher into the cache target dir
- forget a refused rebuild only while the record names its revision
- give a remote branch lookup a deadline

### docs

- give `renki` its own `README.md` and shape the readmes of `renki-config` and `renki-dirs`
- take the second person out of the readme
- take the bold out of the `renki` rustdoc

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

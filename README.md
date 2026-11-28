# `renki`

> A library for building the launcher half of a two-part command line tool. Repo discovery, the version pin, a shared cache, a backend contract and the handover. Unix only, six dependencies, three of them ours.

This repository holds four crates for building the launcher half of a command line tool. `renki` is the launcher library itself, which finds the repo, reads the version it pins, builds that engine once into a shared cache and hands over to it. `renki-config` is the contracts for a tool's settings and the rules its manifest's layers follow, `renki-manifest` reads such a manifest off the disk without `std`, and `renki-dirs` is where a tool's files go on each platform. Each has its own readme on [crates.io](https://crates.io/crates/renki). Under `posix/` sits a reader of the same manifests in plain POSIX shell, for a consumer that cannot link Rust.

## License

> The project is licensed under the **Mozilla Public License 2.0**.

`SPDX-License-Identifier: MPL-2.0`

> You can check out the full license [here](https://github.com/orgrinrt/renki/blob/main/LICENSE)

# `renki-manifest`

> A manifest read off the disk with its layers, through the C library and into memory the caller gives. no_std, no alloc, unix.

A manifest is a base file `name.ext` and any number of layers `name.<word>.ext` beside it or reached through `meta.overlays.include`, which `renki-config` says how to find, order and merge. This crate is the part that reads them off the disk without `std` or an allocator, so a tool that only reads its manifest needs neither.

What is here now is the host: `Host`, the i/o a read needs as one trait, and `Unix`, which implements it over the C library for linux, macOS, freebsd, netbsd, openbsd and illumos. It reads a whole file into a buffer the caller gives, and says how big the buffer has to be when it is too small, so a caller with an allocator can retry once at the right size. It lists a directory, resolves a path, copies an environment variable, and with the feature `processes` runs `/bin/sh -c` in a directory and answers its exit. A directory or a FIFO named like a file is refused rather than read or waited on.

The reading itself, the TOML store over `tomlfuse-parse` and the deserialiser over the layers come in a later release. A POSIX shell reader of the same manifests already sits in the repository under `posix/`.

## Usage

```toml
[dependencies]
renki-manifest = "0.0.1"
```

```rust,no_run
use renki_manifest::{Host, Unix};

let mut text = [0u8; 4096];
match Unix::read(c"app.toml", &mut text) {
    Ok(n) => { let _ = &text[.. n]; },
    Err(refused) => eprintln!("{refused}"),
}
```

## License

MPL-2.0.

# Sketch findings: a no_std, no-alloc crate does every piece of a manifest reader's i/o on the unix targets

**Date:** 2026-11-27
**Outcome:** WORKS
**Unblocks:** the reading of a manifest's layers, its predicates' answers and the untracked check can sit in a `no_std`, no-alloc crate; the round reopening where that reading lives can choose a crate on that basis.

## Hypothesis

Reading a manifest needs four kinds of i/o: a whole file of unknown size, a
directory's entries, one environment variable, and `sh -c <command>` run in a
directory with its exit answered (for a shell predicate, and for asking git
whether a layer is tracked). All four are C library calls on every target
renki builds for, the unix ones `renki-dirs` names (linux, the BSDs, illumos,
macOS). If the `libc` crate with its default features off reaches all of them
from a `#![no_std]` crate that allocates nothing, the reading needs no `std`.

It would be settled by the calls building for each target and behaving on the
host, with a test that can fail.

## What was tried, in order

1. `src/lib.rs`: `read_into` (`open`, `fstat`, `read` into the caller's
   buffer, `close`), `list` (`opendir`, `readdir`, `closedir`, the name borrowed
   from the C library's entry), `var` (`getenv`), `shell` (`posix_spawn` of
   `/bin/sh -c 'cd -- "$1" && exec /bin/sh -c "$2"' sh <dir> <command>`, then
   `waitpid`). `#![no_std]`, no `alloc`, `libc = { default-features = false }`.
2. `tests/on_this_host.rs` on x86_64 linux: six tests, `cargo test`, three runs
   in `in-cargo-test.txt`: `test result: ok. 6 passed; 0 failed`.
3. Every target checked with `cargo check --lib -Zbuild-std=core --target <t>`,
   since only core is needed. The first pass, `cross-check-first.txt`, refused
   netbsd and openbsd (`help: ... *libc::__errno()`) and illumos
   (`*libc::__errno_location()` `not found in libc`). Where errno lives is per
   C library, so `errno()` takes one call per target. The second pass,
   `cross-check.txt`: aarch64 and x86_64 macOS, freebsd, netbsd, openbsd,
   illumos, aarch64 linux gnu and x86_64 linux musl all `exit 0`; dragonfly
   `exit 101` on `*libc::__error()`, the libc crate giving dragonfly no such
   accessor. Dragonfly is not among the targets kept; a target with no errno
   row does not build.
4. Shown able to fail: the size check in `read_into` turned off on purpose
   (`if false && size > buf.len()`), `broken-on-purpose.txt`: the test
   `a_file_bigger_than_the_buffer_says_the_size_it_needs` panics with
   `left: Err(TooBig { need: 11 })` / `right: Err(TooBig { need: 100 })`, the
   other five pass; restored, six pass.
5. A first run showed two tests sharing one fixture directory, a list seeing
   the other test's `marker`; the directories were split and the list's is
   made fresh, and the three runs above are after that.

## The result

- A `#![no_std]`, no-alloc crate reads a whole file into a caller's buffer,
  lists a directory, reads a variable and runs a shell command in a directory
  with its exit, through `libc` alone, on linux (gnu and musl), macOS, freebsd,
  netbsd, openbsd and illumos. It builds for all of them with core only.
- What no-alloc means for a manifest of unknown size: the caller gives the
  memory. `fstat` answers the size before a byte is read, so a file bigger than
  what was given is refused with the size it needs (`TooBig { need }`), and a
  file that grew since is caught by the read past the end. A reader of several
  layers takes one buffer from its caller and carves each layer's text from it
  in turn, an arena of the caller's own; a caller that gets `TooBig` gives more
  and reads again.
- The child's directory is set by a wrapping `sh -c 'cd -- "$1" && exec ...'`,
  so no target needs `posix_spawn_file_actions_addchdir_np`, which not every
  C library has.
- Every call is `unsafe` C: the crate that holds them is the one place in the
  reading where `unsafe` is, and `#![forbid(unsafe_code)]` cannot hold there.

## What is NOT established

- Behaviour on any target but x86_64 linux gnu: the others were only checked
  to build. A macOS or BSD run is owed by whoever has the machine or a CI.
- Windows: `renki-dirs` names no Windows platform, so none was tried.
- Paths: the sketch takes NUL-terminated bytes the caller made. Joining a
  directory and a file name into a caller's buffer without allocating is not
  written here, and is plain byte work.
- Whether the `unsafe` C calls belong in a crate of renki's or in an existing
  no_std i/o crate the stack already has.
- The environment's lifetime: `var` borrows from the process environment for
  `'static`, which holds only while nothing sets a variable.

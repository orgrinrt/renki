# The POSIX reader under every shell and awk this host has

A run of `the_posix_reader_answers_every_fixture_under_every_shell_and_awk`
in `mock/crates/renki-manifest/tests/the_posix_reader_answers_every_fixture.rs`
at f3ac31e, on one host, with the pairs the run names as it reads the suite.
The suite held 176 fixtures; every one was green under every pair.

```text
reading the suite under dash and mawk at /usr/bin/mawk
reading the suite under dash and gawk at /usr/bin/gawk
reading the suite under dash and original-awk at /usr/bin/original-awk
reading the suite under dash and busybox at /usr/bin/busybox
reading the suite under bash and mawk at /usr/bin/mawk
reading the suite under bash and gawk at /usr/bin/gawk
reading the suite under bash and original-awk at /usr/bin/original-awk
reading the suite under bash and busybox at /usr/bin/busybox
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 39.54s
```

The host:

- Linux 6.18, as the superuser, so the two fixtures that take a permission
  away were read in a user namespace (`unshare -U`).
- `dash` 0.5.12 as `/bin/sh`, and GNU `bash` 5.2.21.
- `mawk` 1.3.4 20240123, GNU `awk` 5.2.1, the one true `awk` 20231127 as
  `original-awk`, and BusyBox 1.36.1's `awk`. The path's `awk` and `nawk` were
  `mawk`, so the run read the suite under each program once.

What the four awks parted on before the reader was written to POSIX `awk`
alone, as a probe of each showed: `busybox awk`'s `system()` answers 0
for a command a signal ended, where the other three answer 256 plus the
signal; and `close()` of a command pipe answers three ways among the four,
the one true `awk` 0 whatever the exit. The reader now reads a predicate's status from the
shell that waited on it, and holds no NUL byte, since it reads each file
through `tr`.

Not read: macOS's and the BSDs' own `awk`, and their `sh`.

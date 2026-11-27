# Sketch findings: a no_std, no-alloc TOML reader on stable passes all of toml-test for TOML 1.0.0 and 1.1.0

**Date:** 2026-11-27
**Outcome:** WORKS
**Unblocks:** a generic TOML reading crate can be `#![no_std]`, allocate nothing and build on stable without giving up any conformance the official suite measures; what it asks of a caller is a slice of index nodes sized from the text's length. The crate's shape (event visitor, caller-given index, lazy strings) can be designed from this.

## Hypothesis

A TOML reader cannot build a tree on the heap if it allocates nothing, and
the rules that need memory of what came before (a key defined twice, a table
defined twice, a dotted key or an inline table reopened, a header over a
static array) look like the part that needs a heap. The belief was that all
of these can be checked with an index of fixed-size nodes in a slice the
caller gives, with everything else read in one pass and handed out as
events over the caller's text.

It would be settled by the toml-test suite as the `toml-test-data` crate
carries it (2.14.1) run against such a reader for every spec version the
data lists, valid cases compared as values in toml-test's tagged JSON form
by the `toml-test` crate's own `DecodedValue` equality (2.1.0), invalid cases
counted as passed only when refused; and by the harness being shown able to
fail.

## State of the art, read before writing

- `toml_parser` 1.1.7+spec-1.1.0 (the parser under `toml` and `toml_edit`).
  It is `no_std` and builds without `alloc`: a lexer, then a push parser
  that sends events to an `EventReceiver`, and a `StringBuilder` trait for
  decoding into whatever the caller wants (`()` validates only). Its
  `parse_document` takes the whole token stream as a `&[Token]`, so a
  no-alloc caller has to hold every token in a slice, and its own docs say
  that only the order of events is validated, "not semantics like duplicate
  keys". Taken from it: the push-event shape, and validating strings
  without decoding them.
- `toml` 1.2.3+spec-1.1.0 does the duplicate checks on its tree: each table
  carries an implicit flag and a dotted flag, and the comments in
  `src/de/parser/document.rs` quote the spec rule they enforce ("using
  dotted keys to redefine tables already defined in [table] form is not
  allowed"). Taken from it: the same flags, as the node kinds here, held in
  the caller's slice instead of a heap map. It needs `alloc`.
- `picotoml` 0.1.0: a `no_std`, no-allocator serde deserializer for
  embedded use. It refuses escapes in strings (a comment in `src/de.rs`:
  "Check if there are any escape sequences because we can't support them"),
  has many `unimplemented!()` paths and no duplicate checks; what it reads
  is shaped by the target struct. Nothing taken; it shows the gap.
- `tomling` 0.3.0: `no_std` but with `alloc` (`BTreeMap<Cow<str>, Value>`),
  and its dev-dependencies name `toml-test-harness`.
- `toml-span` 0.7.2 collects raw tables in a list and compares headers after
  the read to find a `DuplicateTable`; it needs `alloc`.

No crate found on crates.io was `no_std`, allocation-free and complete
against toml-test, with the duplicate rules.

## The shape chosen

- Input: `&[u8]`, checked as UTF-8 with `core::str::from_utf8`, a leading
  byte order mark skipped. One pass, recursive descent.
- Output: a `Visitor` trait the caller implements: `table_header`,
  `array_of_tables_header`, `key`, `scalar`, `begin_array`, `end_array`,
  `begin_table`, `end_table`. Key paths are relative (a header's to the
  root, a pair's to its table) and are spans of the text (`KeyPath`,
  iterating `Key`s). The cost: the caller who wants a tree builds it, and
  the order of events is the order of the text, so a value is only ever seen
  once, as it goes by.
- The index: `parse(text, options, &mut [Node], &mut visitor)`. A `Node` is
  28 bytes (a test asserts it): first child, next sibling, last element,
  the key's span in the text, a 32-bit hash of the decoded key, and its
  kind. `nodes_for(len) = len / 2 + 1` always suffices, since every node
  the reader keeps is paid for by at least two bytes of text, so the worst
  case is 14 bytes of index per byte of text; a short slice gives
  `ErrorKind::OutOfNodes` (tested: four nodes needed, three refused). Over
  the whole suite the most held at once was 26 nodes, for
  `valid/table/names-with-values.toml` (160 bytes), and the most per byte
  of text was 0.333. The read returns the peak, so a caller can learn it.
- Nesting: arrays and inline tables nest by the reader's own recursion,
  bounded by `Options::max_depth` (128 by default; 128 levels read and 129
  refused with `TooDeep`, tested).

## How the definitions are checked

- Each table and each key gets a node, linked to its parent. A lookup walks
  the parent's children, refusing by hash and then comparing the two keys'
  decoded characters straight from the text, so `"key"` and `'key'`
  are the same key without a buffer (tested).
- The kind decides what may still extend a node: `Implicit` (made on the way
  to a deeper header; one header may define it later), `Header`, `Dotted`
  (made by a dotted key; only further dotted keys extend it), `Value` (a
  scalar, static array or inline table; nothing extends it),
  `ArrayOfTables` (a path through it reaches its last element) and
  `Element` (a keyless table of an array of tables, or an inline table
  inside an array).
- A header walks the path: through `Implicit`, `Header` or `Dotted`, or into
  the last element of an `ArrayOfTables`; at its end it makes a node, turns
  `Implicit` into `Header`, or appends an element. Anything else is
  `DuplicateTable` or `CannotExtend`.
- A pair walks its dotted key through `Dotted` nodes only, so dotted keys
  never add to a table a header defined or implied, nor to an array of
  tables, nor to an inline table. The last key must be new
  (`DuplicateKey`).
- The inside of a value is forgotten when the value closes: the index is a
  stack there, cut back to the mark taken before the value, and the owner's
  child link cleared. Only the value's own key could link past the mark, and
  a value is never extended, so nothing is lost; an array of a thousand
  inline tables needs the nodes of one (tested: peak 4 for three of them).
- Cost: a lookup is linear in the parent's children times the key's length,
  so a flat table of n keys costs O(n^2) key comparisons in the worst case,
  each first refused by the hash. Memory is O(keys and tables outside
  values). Not reclaimed: the subtrees of earlier elements of an array of
  tables, which no later header can reach.
- Of the invalid cases, 63 per version are refused by the index (39
  `CannotExtend`, 16 `DuplicateKey`, 8 `DuplicateTable`), the counts in
  `refusals.txt`.

## Where values are decoded

- Strings are validated while read (escapes against the version, control
  characters, the lone carriage return, the line-ending backslash, the up to
  two extra quotes before a closing `"""`) and handed out as a `Str` span.
  Decoding is lazy: `chars()` decodes one character at a time with no
  buffer, `as_borrowed()` gives the text itself where decoding changes
  nothing (no escape, no CRLF), and `decode_into(&mut [u8])` writes into the
  caller's buffer or says the size it needs (tested). CRLF in a multi-line
  string decodes as LF.
- Integers in every base decode at read time into `i64` with checked
  arithmetic; overflow is refused.
- Floats are validated by hand against the grammar, then decoded with
  `core`'s `f64` parsing over a 1 KiB stack buffer with the underscores
  taken out; a float longer than that is refused with `FloatTooLong`, a
  limit no case reaches.
- Dates and times decode at read time into fields (`Date`, `Time` with
  nanoseconds cut off past nine digits, `Offset`), with month lengths and
  leap years checked.
- The version is a runtime option: under 1.1.0 inline tables take newlines,
  comments and a trailing comma, `\e` and `\xHH` are escapes, and seconds
  are optional.

## What was tried, in order

1. The reader and the harness written together. The harness's expected
   counts started at zero, so the first run had to fail; it read:

   ```
   TOTAL 1.0.0: valid 208/208 passed, invalid 501/501 refused
   TOTAL 1.1.0: valid 218/218 passed, invalid 494/494 refused
   assertion `left == right` failed
     left: [("1.0.0", 208, 208, 501, 501), ("1.1.0", 218, 218, 494, 494)]
    right: [("1.0.0", 0, 0, 0, 0), ("1.1.0", 0, 0, 0, 0)]
   ```

   The counts were then written into `EXPECTED` in `tests/toml_test.rs`, so
   any change that moves one turns the test red.
2. A second test reads the cases one version lists and the other does not
   (outside the spec example directories) under the other version: 6 valid
   1.1.0 cases must be refused as 1.0.0 and 9 invalid 1.0.0 cases accepted
   as 1.1.0. All 15 flipped (`FLIPPED` lines in `test-run.txt`), so the
   version switch is what separates them.
3. The API tests in `tests/api.rs`. The first run caught a wrong expectation
   in the test, not the reader:

   ```
   thread 'decode_into_says_the_size_it_needs' (7579) panicked at tests/api.rs:91:5:
   assertion `left == right` failed
     left: Err(5)
    right: Err(6)
   ```

   "été" is five bytes in UTF-8; the test was corrected to 5.
4. Stable and no_std. `cargo +stable` was refused in this environment, whose
   shared build directory is configured for the pinned nightly, so the
   library (it has no dependencies) was built with the stable `rustc`
   directly, for the host and for `thumbv7em-none-eabihf`, which has no
   `std`, with `-D warnings`. Both exited 0 (`stable-and-no-std-build.txt`):

   ```
   $ rustc +stable --version
   rustc 1.97.0 (2d8144b78 2026-07-07)
   $ rustc +stable --target thumbv7em-none-eabihf --edition 2024 --crate-type rlib --crate-name nostd_toml -D warnings src/lib.rs --out-dir <scratch>
   exit 0
   ```

   `src/` names `alloc` and `feature` only in its first doc line; a
   `#![no_std]` crate cannot reach `alloc` without `extern crate alloc`.
   The tests ran under the repository's pinned nightly with cargo; clippy
   over all targets is clean.
5. Shown able to fail, twice, each restored after:
   - The duplicate-key refusal turned into reuse of the found node
     (`broken-on-purpose-duplicate-key.txt`): 16 invalid cases per version
     turned red, among them `invalid/key/duplicate-keys-01.toml` to `-09`,
     `invalid/inline-table/duplicate-key-01.toml` and `-02`,
     `invalid/inline-table/overwrite-01.toml`,
     `invalid/table/overwrite-array-in-parent.toml`, and
     `invalid/inline-table/overwrite-10.toml`, which the reader then
     accepted and the harness's tree builder could not build:

     ```
     TOTAL 1.0.0: valid 208/208 passed, invalid 485/501 refused
     TOTAL 1.1.0: valid 218/218 passed, invalid 478/494 refused
     test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.09s
     ```

   - The `\n` escape decoded as `N` (`broken-on-purpose-newline-escape.txt`),
     to show the value comparison sees decoded strings: 6 valid cases per
     version turned red (`valid/key/escapes.toml`, `valid/key/space.toml`,
     `valid/string/escapes.toml`, `valid/string/nl.toml` and two spec
     examples per version), with the decoded tree printed for each:

     ```
     TOTAL 1.0.0: valid 202/208 passed, invalid 501/501 refused
     TOTAL 1.1.0: valid 212/218 passed, invalid 494/494 refused
     ```

6. The final run on the restored source (`test-run.txt`), every refusal
   with its kind and byte offset (`refusals.txt`), and the failing cases
   (`failing-cases.txt`, which lists none).

## The result

| Spec version | Valid passed | Invalid refused |
| --- | --- | --- |
| 1.0.0 | 208 / 208 | 501 / 501 |
| 1.1.0 | 218 / 218 | 494 / 494 |

A TOML reader that is `#![no_std]`, has no `alloc` and no `#![feature]`, and
builds on stable Rust 1.97, passes every valid and every invalid case
toml-test-data 2.14.1 carries for TOML 1.0.0 and 1.1.0, compared by
toml-test 2.1.0's value equality. Nothing had to be given up for the lack of
allocation. What replaces the heap is a caller-given slice of 28-byte index
nodes, `len / 2 + 1` of them at most and far fewer in practice, plus a
recursion bounded by a depth option. Values are decoded at read time where
they are fixed-size (integers, floats, dates and times) and lazily from the
text where they are not (strings and keys).

## What is NOT established

- Conformance beyond the suite: a case the suite does not have may still be
  read wrong. Not tried: fuzzing against `toml` 1.2.3 as an oracle.
- How fast it is. Nothing was timed; the O(n^2) lookup in a wide table is
  reasoned from the code, not measured, and a hashed child table in the
  caller's slice would be the thing to bench against it.
- Whether a caller can size the index without a bound from the text's
  length: there is no counting pass, only `nodes_for` and the peak a read
  returns.
- Behaviour on a real embedded target: `thumbv7em-none-eabihf` was only
  built for, not run, and recursion depth there is bounded by the stack the
  caller has, which the 128 default does not know.
- The encoder side of toml-test: nothing here writes TOML.
- The visitor cannot stop a read early or report its own error; a crate
  would want its methods to return a result.
- Versions after 1.1.0, and the cases toml-test lists for neither version.

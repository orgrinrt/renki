# The no_std reading against the store contract

Source work on round 202611272025-b9a2 stopped at e244166 on six points where
the locked design cannot be built as written. The host, the unsafe module and
the room's file reading are in (e244166, with the writer fixtures at c6b0d50);
the `Toml` store, `Manifest`, `read::<T>`, `Ambient`, the dump and the
deserializer are not. Each wall below names the design line, what fails, and
the change proposed for the design.

## W1. The store's parse is given no memory

- Design: renki-manifest DESIGN, the TOML store: "It reads a document once
  into the spans of its keys, tables and values, through the parser's
  visitor". renki-config `Store`: `fn parse<'t>(text: &'t str) ->
  Outcome<Self::Document<'t>, BadDocument>`, `fn get<'d>(doc, key: &str)`.
- Fails: `tomlfuse_parse::parse` needs `&mut [Node]`, and with no alloc a
  store has nowhere to hold the nodes or the spans. A fixed array in the store
  refuses large valid documents.
- Proposed: the parser's index becomes the document. `tomlfuse-parse` keeps
  every node it read (key span, value span, kind, child and sibling links;
  today it keeps no value span) and offers a read-only view over the filled
  slice: lookup by dotted path, children, elements of an array of tables.
  `Store::parse` takes the caller's index: `parse<'t>(text, index: &'t mut
  [St::Cell])`, the room giving the slice. `nodes_for(len)` still bounds it.

## W2. A decoded string has nowhere to live

- Design: "a list's string items decoded on demand by the parser's own
  decoding, never into a buffer of the store's". `Literal::Str(&'d str)`.
- Fails: a string with an escape cannot be lent as `&str` without decoding it
  somewhere; `Str::as_borrowed` is `None` for it.
- Proposed: the room holds a decode shelf the size of its text. A decoded
  TOML string is never longer than its source (every escape decodes to at most
  its own length, a line-ending backslash removes bytes), so one shelf of
  `TEXT` bytes always suffices. Strings are decoded into it when the document
  is read. The buffer is the caller's room, not the store's.

## W3. `items` is handed a list's text, and the parser reads documents

- Design: renki-config, `List(&'a str)`, "the list as its own source text",
  walked through `Store::items(list: &'d str)`.
- Fails: `tomlfuse-parse` reads a whole document, never a bare value, and an
  array of tables has no single contiguous text.
- Proposed: `Literal::List` carries the store's own handle, `St::List<'d>`
  (for TOML, a node of the index), and `Store::items` walks that handle.

## W4. The lint claim cannot hold once `Store` is implemented

- Design: doc changelist, renki-manifest "passes `no-std`, `no-alloc` and
  every `no-bare-*` lint with no exemption, allow-marker or ratchet".
- Fails: implementing `Store` names `notko::Outcome` and `Maybe`, so the crate
  depends on notko, and mockspace-extra-lints then counts it a stack crate
  (`nature.rs`, `is_of_the_stack`: `#![no_std]` with a direct arvo or notko
  dependency). With notko added the committed code gives 46 findings (13
  no-bare-result, 13 no-bare-numeric, 13 arvo-types-only, 5 no-bare-option, 1
  no-bare-string, 1 no-public-raw-field). Two of them no source can fix: the
  `&str` that `Store`'s own signatures fix, and the `Result` that
  `tomlfuse_parse::Visitor`'s methods return. renki-config passes today only
  through about a hundred `lint:allow` markers (ticket bbc1a8c97dd9).
- Proposed: (a) renki-config's contract is ported off bare primitives in this
  round, to `hilavitkutin_str::Str` and arvo's types as its FIXMEs say, and its
  markers go; (b) renki-manifest takes arvo and notko and passes the lints
  outright; (c) the bare-type lints do not check the signature of an impl of a
  trait another crate owns, since its signature is not the implementer's to
  choose. (c) is a change to mockspace-extra-lints, outside this repository.

## W5. A room cannot be sized at run time

- Design: `Room<const TEXT, const NODES>`; the src changelist puts the one
  sizing retry in `renki`, sized from `BadRead::Room`.
- Fails: a size known at run time cannot become a const parameter.
- Proposed: `Room<'r>` borrows its three slices (text, decode shelf, nodes)
  from the caller; a fixed-size owner, `Rooms<TEXT, NODES>`, lends one for a
  caller with no allocator. `renki` allocates the slices from the size
  `BadRead::Room` names and retries once.

## W6. The untracked check without the feature

- Design: "The check is `git ls-files --error-unmatch -- <file>` through the
  host's shell"; "The shell half is behind the feature `shell-predicate`".
- Fails: the design does not say what the check does when the feature is off.
- Proposed: the feature is the one gate on spawning a process, renamed
  `processes`, and covers both. Without it a `shell` predicate is refused by
  name and the untracked check is not run, and the dump says it was not run.

## Smaller points the amendment settles

- Paths: `read(path: &str)` against `Host`'s `&CStr`. The room holds a path
  shelf where the base path and each joined include path are written with
  their NUL; `BadRead` names a file by its place there.
- The TOML version read is 1.1.0, the newest specification.
- Writer fixtures: four of seven differ from what `toml_edit` wrote, because
  `toml_edit`'s writer was wrong there (an inline table's key appended as a
  second `[server]`, a `name = ` inside a multi-line string edited instead of
  the key, a multi-line value replaced on its first line only, `[[hooks]]` not
  seen as a section). The fixtures hold what the design calls for.

## Where it lands

W1 and W3 change `tomlfuse-parse` (unreleased, on its work branch) and
renki-config's `Store` and `Literal`. W2, W5, W6 and the paths change
renki-manifest's design. W4 (a) and (b) are this round's; (c) is the lint
owner's.

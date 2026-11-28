# What the store spells has no shelf

Source work on the ninth reopening of round 202611272025-b9a2 stopped where
the TOML store is written. `renki-config`'s `Store` now takes its index as a
`Lend<St::Cell>` and hands a list as `St::List<'d>`; the room lends four
shelves. Two things the store hands out are text that is in no document as
written, and the contract gives the store nowhere to put them.

## S1. A decoded string

- Design: renki-manifest, the TOML store: "A string with an escape is decoded
  into the room's decode shelf when the document is read, by the parser's own
  decoding, and lent from there." renki-config, the store: `Store::parse`
  takes the text and "an index the caller lends with the text, `St::Cell`
  slots through any `notko::lend::Lend`".
- Fails: `Store::parse` takes one lend, of `St::Cell`, and for TOML a cell is
  the parser's `Node`. The decode shelf is bytes and reaches no `Store::parse`,
  so `Toml::get` cannot lend `Literal::Str(&'d str)` for a string with an
  escape: `tomlfuse_parse::Str::as_borrowed` is `None` for it.

## S2. A dotted key

- Design: `Store::keys` yields `&'d str`, every dotted key a document holds,
  an array of tables' as `name.0.field`; `Stack::new`, `Meta::defect`,
  `Predicates::read`, `unknown_keys` and the refusals (`BadLayer`, `BadMerge`,
  `BadConfig`) take those strings and slices of them.
- Fails: a key under a header is no contiguous text. `[server]` then
  `port = 1` holds `server.port` nowhere, and `name.0.field` is nowhere at
  all. With no allocation the store has to write them somewhere, and their
  total length is not bounded by the text's: a long header over many short
  keys repeats the header in every key.

## Options

1. `Store::parse` takes a second lend, `spelled: &'t mut S` with
   `S: Lend<u8> + ?Sized`, where the store writes every decoded string and
   every dotted key it hands out. The store knows the whole need once it has
   read the index, so a short shelf is refused with an `Exhausted` whose
   `wanted` is the whole need, and one retry suffices. The room's decode
   shelf becomes this shelf, sized by that answer rather than by the text.
   The parse's outer refusal names which lend fell short, by a two-variant
   enum of renki-config's.
2. A key is the store's handle too, `St::Key<'d>`, walked by segment and
   written by `Display`, so no dotted key is spelled at all; every
   `renki-config` site that slices a key asks the handle instead, and the
   refusals carry the handle. Decoded strings still need S1's shelf, so this
   adds a second handle beside option 1's lend rather than replacing it.
3. The store's cell is its own type and carries spelling room: a TOML cell
   holds a `Node` and a few bytes, the index sized by both. Couples the two
   sizes and the `nodes_for` bound no longer says enough.

## Not affected

The room, `Shelf` and `BadRead::Room`, the host, `Literal<'a, St>` and
`St::List<'d>`, `Store::items` over the handle, and the index lend stand as
built.

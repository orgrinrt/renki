//! The properties the findings claim beyond the suite: what a short index
//! does, how strings decode, where nesting stops, and that the index forgets
//! the inside of a value once it is closed.

use nostd_toml::{
    ErrorKind,
    KeyPath,
    Node,
    Options,
    Scalar,
    Str,
    Version,
    Visitor,
    nodes_for,
    parse,
};

#[derive(Default)]
struct Strings(Vec<(String, Option<String>)>);

impl<'s> Visitor<'s> for Strings {
    fn table_header(&mut self, _: KeyPath<'s>) {}

    fn array_of_tables_header(&mut self, _: KeyPath<'s>) {}

    fn key(&mut self, _: KeyPath<'s>) {}

    fn scalar(&mut self, value: Scalar<'s>) {
        if let Scalar::String(s) = value {
            self.0
                .push((s.chars().collect(), s.as_borrowed().map(str::to_owned)));
        }
    }

    fn begin_array(&mut self) {}

    fn end_array(&mut self) {}

    fn begin_table(&mut self) {}

    fn end_table(&mut self) {}
}

fn read(text: &str, nodes: usize) -> Result<(usize, Strings), ErrorKind> {
    let mut index = vec![Node::EMPTY; nodes];
    let mut v = Strings::default();
    parse(
        text.as_bytes(),
        Options::new(Version::V1_1_0),
        &mut index,
        &mut v,
    )
    .map(|peak| (peak, v))
    .map_err(|e| e.kind)
}

#[test]
fn a_short_index_is_refused_and_one_more_node_is_enough() {
    let text = "a = 1\nb = 2\n[t]\nc = 3\n";
    // a, b, t, c: four nodes.
    assert_eq!(read(text, 3).err(), Some(ErrorKind::OutOfNodes));
    assert_eq!(read(text, 4).map(|r| r.0), Ok(4));
}

#[test]
fn the_inside_of_a_closed_value_is_forgotten() {
    // Each element's inline table needs an element node and its keys while
    // it is read, and none of them once the array is closed.
    let text = "a = [{x = 1, y = 2}, {x = 1, y = 2}, {x = 1, y = 2}]\nb = 1\n";
    let (peak, _) = read(text, nodes_for(text.len())).expect("reads");
    // `a`, then one element and its `x` and `y`; `b` reuses a freed slot.
    assert_eq!(peak, 4);
}

#[test]
fn a_duplicate_is_found_through_any_quoting_and_escaping() {
    let text = "\"k\\u0065y\" = 1\n'key' = 2\n";
    assert_eq!(read(text, 8).err(), Some(ErrorKind::DuplicateKey));
}

#[test]
fn strings_borrow_when_decoding_changes_nothing_and_decode_lazily_otherwise() {
    let text =
        "a = \"plain\"\nb = \"tab\\there\"\nc = '''\nraw\\n'''\nd = \"\"\"\\\n   folded\"\"\"\n";
    let (_, v) = read(text, 16).expect("reads");
    let got: Vec<_> =
        v.0.iter()
            .map(|(d, b)| (d.as_str(), b.as_deref()))
            .collect();
    assert_eq!(got, [
        ("plain", Some("plain")),
        ("tab\there", None),
        ("raw\\n", Some("raw\\n")),
        ("folded", None)
    ]);
}

#[test]
fn decode_into_says_the_size_it_needs() {
    struct First<'s>(Option<Str<'s>>);
    impl<'s> Visitor<'s> for First<'s> {
        fn table_header(&mut self, _: KeyPath<'s>) {}

        fn array_of_tables_header(&mut self, _: KeyPath<'s>) {}

        fn key(&mut self, _: KeyPath<'s>) {}

        fn scalar(&mut self, value: Scalar<'s>) {
            if let Scalar::String(s) = value {
                self.0 = Some(s);
            }
        }

        fn begin_array(&mut self) {}

        fn end_array(&mut self) {}

        fn begin_table(&mut self) {}

        fn end_table(&mut self) {}
    }
    let text = b"s = \"\\u00e9t\\u00e9\"";
    let mut index = [Node::EMPTY; 2];
    let mut first = First(None);
    parse(text, Options::new(Version::V1_0_0), &mut index, &mut first).expect("reads");
    let s = first.0.expect("a string was read");
    let mut short = [0u8; 4];
    assert_eq!(s.decode_into(&mut short), Err(5));
    let mut enough = [0u8; 5];
    assert_eq!(s.decode_into(&mut enough), Ok("été"));
}

#[test]
fn nesting_past_the_bound_is_refused() {
    let deep = format!("a = {}{}", "[".repeat(129), "]".repeat(129));
    assert_eq!(read(&deep, 8).err(), Some(ErrorKind::TooDeep));
    let fits = format!("a = {}{}", "[".repeat(128), "]".repeat(128));
    assert!(read(&fits, 8).is_ok());
}

#[test]
fn a_node_is_twenty_eight_bytes() {
    assert_eq!(size_of::<Node>(), 28);
}

//! Runs the reader over every case of the toml-test suite, for each spec
//! version the data carries, and prints one line per failing case.
//!
//! A valid case passes when the document built from the reader's events
//! equals the case's JSON, compared as values by `toml_test::DecodedValue`.
//! An invalid case passes when the reader refuses it.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use nostd_toml::{
    Datetime,
    Error,
    KeyPath,
    Node,
    Offset,
    Options,
    Scalar,
    Version,
    Visitor,
    nodes_for,
    parse,
};
use toml_test::{DecodedScalar, DecodedValue};

/// What this sketch recorded, per version: valid passed and total, invalid
/// refused and total. A change to the reader that moves any count turns
/// the test red.
const EXPECTED: [(&str, usize, usize, usize, usize); 2] =
    [("1.0.0", 208, 208, 501, 501), ("1.1.0", 218, 218, 494, 494)];

type Table = HashMap<String, DecodedValue>;

enum Frame {
    Array(Vec<DecodedValue>),
    Table(Table, Vec<String>),
}

#[derive(Default)]
struct Builder {
    root:    Table,
    table:   Vec<String>,
    pending: Vec<String>,
    stack:   Vec<Frame>,
}

fn names(path: KeyPath<'_>) -> Vec<String> {
    path.keys().map(|k| k.name().chars().collect()).collect()
}

/// The table at `path` under `map`, made where missing; an array on the
/// way is entered at its last element.
fn table_at<'a>(mut map: &'a mut Table, path: &[String]) -> &'a mut Table {
    for k in path {
        let next = map
            .entry(k.clone())
            .or_insert_with(|| DecodedValue::Table(Table::new()));
        map = match next {
            DecodedValue::Table(t) => t,
            DecodedValue::Array(a) => {
                match a.last_mut() {
                    Some(DecodedValue::Table(t)) => t,
                    _ => panic!("array entered at a non-table"),
                }
            },
            DecodedValue::Scalar(_) => panic!("scalar entered as a table"),
        };
    }
    map
}

fn insert(map: &mut Table, path: &[String], value: DecodedValue) {
    let (last, parents) = path.split_last().expect("a key path has a key");
    table_at(map, parents).insert(last.clone(), value);
}

fn datetime(d: Datetime) -> DecodedScalar {
    let date = d
        .date
        .map(|d| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day));
    let time = d.time.map(|t| {
        format!(
            "{:02}:{:02}:{:02}.{:09}",
            t.hour, t.minute, t.second, t.nanosecond
        )
    });
    let offset = d.offset.map(|o| {
        match o {
            Offset::Utc => "Z".to_string(),
            Offset::Minutes(m) => {
                format!(
                    "{}{:02}:{:02}",
                    if m < 0 { '-' } else { '+' },
                    m.abs() / 60,
                    m.abs() % 60
                )
            },
        }
    });
    match (date, time, offset) {
        (Some(d), Some(t), Some(o)) => DecodedScalar::Datetime(format!("{d}T{t}{o}")),
        (Some(d), Some(t), None) => DecodedScalar::DatetimeLocal(format!("{d}T{t}")),
        (Some(d), None, _) => DecodedScalar::DateLocal(d),
        (None, Some(t), _) => DecodedScalar::TimeLocal(t),
        (None, None, _) => unreachable!("a date-time has a date or a time"),
    }
}

fn scalar(s: Scalar<'_>) -> DecodedScalar {
    match s {
        Scalar::String(s) => DecodedScalar::String(s.chars().collect()),
        Scalar::Integer(i) => DecodedScalar::Integer(i.to_string()),
        Scalar::Float(f) if f.is_nan() => DecodedScalar::Float("nan".into()),
        Scalar::Float(f) => DecodedScalar::Float(format!("{f:?}")),
        Scalar::Boolean(b) => DecodedScalar::Bool(b.to_string()),
        Scalar::Datetime(d) => datetime(d),
    }
}

impl Builder {
    fn deliver(&mut self, value: DecodedValue) {
        match self.stack.last_mut() {
            None => {
                let path: Vec<String> = self.table.iter().chain(&self.pending).cloned().collect();
                insert(&mut self.root, &path, value);
            },
            Some(Frame::Array(a)) => a.push(value),
            Some(Frame::Table(t, pending)) => insert(t, pending, value),
        }
    }
}

impl<'s> Visitor<'s> for Builder {
    fn table_header(&mut self, path: KeyPath<'s>) {
        self.table = names(path);
        table_at(&mut self.root, &self.table);
    }

    fn array_of_tables_header(&mut self, path: KeyPath<'s>) {
        self.table = names(path);
        let (last, parents) = self.table.split_last().expect("a header has a key");
        let parent = table_at(&mut self.root, parents);
        match parent
            .entry(last.clone())
            .or_insert_with(|| DecodedValue::Array(Vec::new()))
        {
            DecodedValue::Array(a) => a.push(DecodedValue::Table(Table::new())),
            _ => panic!("array of tables over a non-array"),
        }
    }

    fn key(&mut self, path: KeyPath<'s>) {
        match self.stack.last_mut() {
            Some(Frame::Table(_, pending)) => *pending = names(path),
            _ => self.pending = names(path),
        }
    }

    fn scalar(&mut self, value: Scalar<'s>) {
        self.deliver(DecodedValue::Scalar(scalar(value)));
    }

    fn begin_array(&mut self) {
        self.stack.push(Frame::Array(Vec::new()));
    }

    fn end_array(&mut self) {
        let Some(Frame::Array(a)) = self.stack.pop() else {
            panic!("unbalanced array")
        };
        self.deliver(DecodedValue::Array(a));
    }

    fn begin_table(&mut self) {
        self.stack.push(Frame::Table(Table::new(), Vec::new()));
    }

    fn end_table(&mut self) {
        let Some(Frame::Table(t, _)) = self.stack.pop() else {
            panic!("unbalanced table")
        };
        self.deliver(DecodedValue::Table(t));
    }
}

fn read(text: &[u8], version: Version) -> Result<(DecodedValue, usize), Error> {
    let mut nodes = vec![Node::EMPTY; nodes_for(text.len())];
    let mut builder = Builder::default();
    let peak = parse(text, Options::new(version), &mut nodes, &mut builder)?;
    Ok((DecodedValue::Table(builder.root), peak))
}

#[test]
fn every_case_of_every_version() {
    let valid: BTreeMap<String, _> = toml_test_data::valid()
        .map(|c| (c.name().display().to_string(), c))
        .collect();
    let invalid: BTreeMap<String, _> = toml_test_data::invalid()
        .map(|c| (c.name().display().to_string(), c))
        .collect();
    let mut table = Vec::new();
    for (label, version) in [("1.0.0", Version::V1_0_0), ("1.1.0", Version::V1_1_0)] {
        let listed: HashSet<String> = toml_test_data::version(label)
            .map(|p| p.display().to_string())
            .collect();
        let (mut vp, mut vt, mut ip, mut it) = (0, 0, 0, 0);
        let mut most = (0, 0, String::new());
        let mut ratio: f64 = 0.0;
        for (name, case) in valid.iter().filter(|(n, _)| listed.contains(*n)) {
            vt += 1;
            let expected =
                DecodedValue::from_slice(case.expected()).expect("the case's JSON reads");
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                match read(case.fixture(), version) {
                    Ok((actual, peak)) if actual == expected => Ok(peak),
                    Ok((actual, _)) => {
                        Err(format!(
                            "decoded differently: {}",
                            actual
                                .to_string_pretty()
                                .unwrap_or_default()
                                .replace('\n', " ")
                        ))
                    },
                    Err(e) => Err(format!("refused: {:?} at byte {}", e.kind, e.at)),
                }
            }))
            .unwrap_or_else(|_| Err("the comparison panicked".into()));
            match outcome {
                Ok(peak) => {
                    vp += 1;
                    if peak > most.0 {
                        most = (peak, case.fixture().len(), name.clone());
                    }
                    ratio = ratio.max(peak as f64 / case.fixture().len().max(1) as f64);
                },
                Err(why) => println!("FAIL {label} {name}: {why}"),
            }
        }
        for (name, case) in invalid.iter().filter(|(n, _)| listed.contains(*n)) {
            it += 1;
            match catch_unwind(AssertUnwindSafe(|| read(case.fixture(), version))) {
                Ok(Err(e)) => {
                    ip += 1;
                    println!("REFUSED {label} {name}: {:?} at byte {}", e.kind, e.at);
                },
                Ok(Ok(_)) => println!("FAIL {label} {name}: accepted"),
                Err(_) => {
                    println!(
                        "FAIL {label} {name}: the builder panicked on what the reader accepted"
                    )
                },
            }
        }
        println!("TOTAL {label}: valid {vp}/{vt} passed, invalid {ip}/{it} refused");
        println!(
            "NODES {label}: most held at once {} (by {}, {} bytes), most per byte of text {ratio:.3}",
            most.0, most.2, most.1
        );
        table.push((label, vp, vt, ip, it));
    }
    let got: Vec<_> = table
        .iter()
        .map(|(l, a, b, c, d)| (*l, *a, *b, *c, *d))
        .collect();
    assert_eq!(got, EXPECTED.to_vec());
}

/// The cases one version lists and the other does not, outside the spec
/// example directories, read under the other version: each must flip.
#[test]
fn the_cases_that_differ_between_versions_flip_with_the_version() {
    let v10: HashSet<String> = toml_test_data::version("1.0.0")
        .map(|p| p.display().to_string())
        .collect();
    let v11: HashSet<String> = toml_test_data::version("1.1.0")
        .map(|p| p.display().to_string())
        .collect();
    let only = |a: &HashSet<String>, b: &HashSet<String>, n: &str| {
        a.contains(n) && !b.contains(n) && !n.contains("/spec-")
    };
    let mut flipped = Vec::new();
    let mut held = Vec::new();
    for case in toml_test_data::valid() {
        let name = case.name().display().to_string();
        if only(&v11, &v10, &name) {
            let refused = read(case.fixture(), Version::V1_0_0).is_err();
            if refused { flipped.push(name) } else { held.push(name) }
        }
    }
    for case in toml_test_data::invalid() {
        let name = case.name().display().to_string();
        if only(&v10, &v11, &name) {
            let accepted = read(case.fixture(), Version::V1_1_0).is_ok();
            if accepted { flipped.push(name) } else { held.push(name) }
        }
    }
    for n in &flipped {
        println!("FLIPPED {n}");
    }
    for n in &held {
        println!("HELD {n}");
    }
    assert_eq!((flipped.len(), held.len()), (15, 0));
}

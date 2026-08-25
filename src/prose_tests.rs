//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Every string this crate can print, held to the shape a person wrote.
//!
//! [`crate::Tool::defect`]'s messages already had a guard of their own, and it
//! caught nothing when the same defect landed one file over in a message a
//! consumer reads when a pin will not resolve. So the guard is over the source
//! rather than over one function: whatever a later diagnostic is called, it is
//! scanned.

use std::path::{Path, PathBuf};

/// A run of spaces this long, mid-sentence inside a literal, is the tell.
///
/// Eight rather than two, and the gap between them is where a sample lives. A
/// continued line is indented to at least the literal's own column, so a
/// dropped backslash welds fifteen to twenty-five spaces into the middle of a
/// sentence; the two observed were twenty and twenty-two. Deliberate spacing
/// inside a literal is alignment, which runs to three or four: a comment lined
/// up in a config sample, a parser fixture proving whitespace around `=` is
/// tolerated. Nothing anybody writes on purpose sits between them.
const RUN: usize = 8;

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file under `src`, sorted so a failure names the same file twice
/// in a row rather than wandering with the directory order.
fn sources() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(src_dir())
        .expect("the crate's own src directory is unreadable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        // This file's own fixtures are the defect, written out, so that the
        // scan can be shown catching it. Scanning them would report the
        // control as a finding.
        .filter(|p| p.file_name().is_some_and(|n| n != "prose_tests.rs"))
        .collect();
    out.sort();
    out
}

/// One offending literal: the line it sits on and the text around the run.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    line: usize,
    excerpt: String,
}

/// Find runs of [`RUN`] spaces sitting mid-sentence inside a string literal.
///
/// Mid-sentence is the whole of it. A literal that lays out a bullet or a
/// sample indents after a line break, and that is somebody's intent; a
/// continuation whose backslash was eaten leaves the next line's indentation
/// welded to the end of the previous word, with no break in front of it. So the
/// question is not whether spaces are present but whether anything precedes
/// them on their own line inside the literal.
fn mid_sentence_runs(src: &str) -> Vec<Run> {
    let b: Vec<char> = src.chars().collect();
    let mut found = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c == '/' && b.get(i + 1) == Some(&'/') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i < b.len() && !(b[i] == '*' && b.get(i + 1) == Some(&'/')) {
                if b[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else if c == 'r' && matches!(b.get(i + 1), Some('"') | Some('#')) {
            let mut hashes = 0usize;
            let mut j = i + 1;
            while b.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&'"') {
                i += 1;
                continue;
            }
            // Raw literals carry samples and fixtures and have no escapes, so
            // the scan runs over them with real newlines as the only breaks.
            i = j + 1;
            let close: String = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            let start = i;
            while i < b.len() {
                if b[i] == '"' && b[i..].iter().take(close.len()).copied().eq(close.chars()) {
                    break;
                }
                i += 1;
            }
            let body: String = b[start..i.min(b.len())].iter().collect();
            found.extend(runs_in_body(&body, line));
            line += body.matches('\n').count();
            i = (i + close.len()).min(b.len());
        } else if c == '\'' {
            // A char literal or a lifetime. Only the first can hold a quote, so
            // stepping over it matters; a lifetime is one identifier and the
            // scan may simply walk on through it.
            let one_char_then_quote = b.get(i + 2) == Some(&'\'');
            if b.get(i + 1) == Some(&'\\') || one_char_then_quote {
                i += 1;
                while i < b.len() && b[i] != '\'' {
                    i += if b[i] == '\\' { 2 } else { 1 };
                }
            }
            i += 1;
        } else if c == '"' {
            i += 1;
            let mut body = String::new();
            let mut source_newlines = 0usize;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' {
                    match b.get(i + 1) {
                        // The correct form: the backslash eats the newline and
                        // the next line's indentation with it.
                        Some('\n') => {
                            source_newlines += 1;
                            i += 2;
                            while i < b.len() && (b[i] == ' ' || b[i] == '\t') {
                                i += 1;
                            }
                            continue;
                        }
                        Some('n') => body.push('\n'),
                        _ => body.push('\u{0}'),
                    }
                    i += 2;
                    continue;
                }
                if b[i] == '\n' {
                    source_newlines += 1;
                }
                body.push(b[i]);
                i += 1;
            }
            found.extend(runs_in_body(&body, line));
            line += source_newlines;
            i += 1;
        } else {
            i += 1;
        }
    }
    found
}

fn runs_in_body(body: &str, line: usize) -> Vec<Run> {
    let mut out = Vec::new();
    for segment in body.split('\n') {
        let trimmed = segment.trim_start();
        // What was cut off the front is the literal's own indentation, which is
        // deliberate. Anything past the first word is not.
        let Some(offset) = segment.len().checked_sub(trimmed.len()) else {
            continue;
        };
        let mut run = 0usize;
        let mut seen_word = false;
        for (idx, ch) in trimmed.char_indices() {
            if ch == ' ' {
                run += 1;
                if run >= RUN && seen_word {
                    let at = offset + idx + 1;
                    let from = at.saturating_sub(40);
                    out.push(Run {
                        line,
                        excerpt: segment[from..(at + 20).min(segment.len())].to_string(),
                    });
                    break;
                }
            } else {
                run = 0;
                seen_word = true;
            }
        }
    }
    out
}

#[test]
fn no_literal_in_this_crate_carries_a_dropped_line_continuation() {
    let mut complaints = Vec::new();
    for path in sources() {
        let text = std::fs::read_to_string(&path).expect("a source file went missing mid-test");
        for run in mid_sentence_runs(&text) {
            complaints.push(format!(
                "{}:{} carries {RUN} or more spaces mid-sentence inside a literal, which is what \
                 a `\\` continuation looks like once something has eaten the backslash: {:?}",
                path.file_name().unwrap().to_string_lossy(),
                run.line,
                run.excerpt,
            ));
        }
    }
    assert!(complaints.is_empty(), "{}", complaints.join("\n"));
}

#[test]
fn the_scan_finds_a_dropped_continuation_and_leaves_a_deliberate_indent_alone() {
    // The control, and it is the whole reason the scan is trustworthy. Without
    // it a scanner that never matches anything reports a clean crate forever.
    let dropped = r#"fn f() { let _ = "the pin has no tag: this tool's hook                      returned nothing."; }"#;
    let found = mid_sentence_runs(dropped);
    assert_eq!(
        found.len(),
        1,
        "the scan missed a dropped continuation: {found:?}"
    );
    assert_eq!(found[0].line, 1);

    let continued = "fn f() { let _ = \"the pin has no tag: this tool's hook \\\n                     returned nothing.\"; }";
    assert_eq!(
        mid_sentence_runs(continued),
        vec![],
        "the scan flagged a correctly continued literal, so it is measuring the wrong thing"
    );

    let indented = r#"fn f() { let _ = "candidates:\n    - one\n    - two"; }"#;
    assert_eq!(
        mid_sentence_runs(indented),
        vec![],
        "the scan flagged an indent that follows a line break, which is somebody's layout"
    );

    let raw = "fn f() { let _ = r#\"[table]\n    key = 1\n\"#; }";
    assert_eq!(
        mid_sentence_runs(raw),
        vec![],
        "the scan flagged an indented line inside a raw fixture"
    );

    let commented = "// a comment mentioning a hook                      and a gap\nfn f() {}";
    assert_eq!(
        mid_sentence_runs(commented),
        vec![],
        "the scan read a comment as a literal"
    );
}

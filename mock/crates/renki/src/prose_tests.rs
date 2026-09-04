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
//!
//! What that covers, exactly, because a guard that reads as total and is not is
//! worse than one nobody trusts. Every `.rs` file under `src`, at any depth.
//! Every string literal in one, raw or not, including the case where a real
//! newline sits inside a non-raw literal, which is the same weld with the
//! backslash removed rather than eaten. What it does not cover: a weld shorter
//! than [`RUN`] spaces, and prose built at runtime rather than written in a
//! literal.

use std::path::{Path, PathBuf};

/// A run of spaces this long, mid-sentence inside a literal, is the tell.
///
/// A continued line is indented to at least the literal's own column, so a
/// dropped backslash welds fifteen to twenty-five spaces into the middle of a
/// sentence; the two observed were twenty and twenty-two. Eight is well under
/// that and well over the three or four an aligned comment in a config sample
/// takes.
///
/// It is a threshold rather than a boundary, and both sides of it cost
/// something. Above it, a run this long can be deliberate: an aligned TOML
/// sample whose keys are short and whose values are far right, or a usage block
/// laying out a flag against its description. Both are alignment after a word
/// on the same line, which is exactly the shape being matched. Below it, a weld
/// from a literal that started near the left margin is six spaces and walks
/// past.
///
/// Eight is the number that let both observed defects be caught with no false
/// positive in this crate. It is not a claim that nothing between four and
/// eight is ever a weld, nor that nothing above eight is ever intended.
const RUN: usize = 8;

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file under `src`, at any depth, sorted so a failure names the
/// same file twice in a row rather than wandering with the directory order.
///
/// At any depth because the crate is flat today and the guard is supposed to
/// hold when it stops being. A reader of the module comment above is told the
/// scan is over the source; a single `read_dir` would make that false the day
/// somebody adds a directory, silently and with the suite green.
fn sources() -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_into(&src_dir(), &mut out);
    out.sort();
    out
}

fn collect_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).expect("a source directory of this crate is unreadable");
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            collect_into(&path, out);
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        // This file's own fixtures are the defect, written out, so that the
        // scan can be shown catching it. Scanning them would report the
        // control as a finding.
        if path.file_name().is_some_and(|n| n == "prose_tests.rs") {
            continue;
        }
        out.push(path);
    }
}

/// One offending literal: the line it sits on and the text around the run.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    line:    usize,
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
                if b[i] == '"' && b[i ..].iter().take(close.len()).copied().eq(close.chars()) {
                    break;
                }
                i += 1;
            }
            let body: String = b[start .. i.min(b.len())].iter().collect();
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
                        },
                        Some('n') => body.push('\n'),
                        _ => body.push('\u{0}'),
                    }
                    i += 2;
                    continue;
                }
                if b[i] == '\n' {
                    // A real newline inside a non-raw literal is not a break in
                    // anybody's layout. It is the same weld the dropped
                    // backslash makes, with the backslash never written rather
                    // than eaten, and the indentation that follows it lands
                    // mid-sentence exactly the same way. So it is not passed
                    // through as a newline: doing that made `runs_in_body` read
                    // the indentation as a line's own, which is deliberate, and
                    // walk past the larger half of the class.
                    //
                    // An escaped `\n` is the opposite and keeps its newline
                    // above, because somebody typed it and what follows it is
                    // layout.
                    source_newlines += 1;
                    body.push('\u{0}');
                    i += 1;
                    continue;
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
                        excerpt: segment[from .. (at + 20).min(segment.len())].to_string(),
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

#[test]
fn a_newline_written_into_a_literal_is_a_weld_and_one_that_was_escaped_is_layout() {
    // The larger half of the class, and the half the first version walked past.
    // A backslash that was never written leaves a real newline in the literal
    // and the next line's indentation welded on behind it, which reads exactly
    // as the dropped-backslash case does to whoever gets the message.
    let welded = "fn f() { let _ = \"this tool's hook\n                     returned nothing.\"; }";
    let found = mid_sentence_runs(welded);
    assert_eq!(
        found.len(),
        1,
        "a real newline inside a literal was read as somebody's layout: {found:?}"
    );

    // And the control, on the same axis, because a scan that flagged both would
    // report every multi-line message in the crate. An escaped `\n` is typed on
    // purpose and what follows it is a bullet, a sample, a column.
    let escaped = r#"fn f() { let _ = "tried:\n                     the registry\n                     the tag"; }"#;
    assert_eq!(
        mid_sentence_runs(escaped),
        vec![],
        "an escaped newline's indentation was read as a weld"
    );
}

#[test]
fn the_scan_reaches_a_source_file_in_a_subdirectory() {
    // The module comment says every `.rs` under `src`. A single `read_dir` says
    // every `.rs` directly under it, and the difference is invisible until
    // somebody adds a directory, at which point the guard silently stops
    // covering a file while the suite stays green.
    let found = sources();
    assert!(
        !found.is_empty(),
        "the walk found no sources at all, so it proves nothing about depth"
    );
    let dir = src_dir();
    let mut deepest = 0usize;
    for path in &found {
        let rel = path.strip_prefix(&dir).expect("a source outside src");
        deepest = deepest.max(rel.components().count());
    }
    // The crate is flat today, so this cannot assert a nested file exists. What
    // it can assert is that the walk descends, over a tree built to need it.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let nested = tmp.path().join("deep").join("deeper");
    std::fs::create_dir_all(&nested).expect("the fixture tree");
    std::fs::write(nested.join("buried.rs"), "fn f() {}").expect("the buried source");
    std::fs::write(tmp.path().join("top.rs"), "fn g() {}").expect("the top source");
    let mut walked = Vec::new();
    collect_into(tmp.path(), &mut walked);
    walked.sort();
    let names: Vec<String> = walked
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec!["buried.rs".to_string(), "top.rs".to_string()],
        "the walk did not descend, so the module comment's claim is false \
         (this crate is {deepest} level(s) deep today)"
    );
}

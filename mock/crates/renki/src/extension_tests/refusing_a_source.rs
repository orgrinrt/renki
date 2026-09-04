//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Refusing a source a fetcher would misread. Split from the extension tests
//! by size; `with_source` is the parent module's.

use super::*;

#[test]
fn a_revision_that_git_would_read_as_a_flag_is_refused() {
    // The descriptor can arrive from a git ref, so this is not the workspace
    // author's own word. `--upload-pack` runs an arbitrary command.
    let bad = with_source(
        r#"git = { url = "https://e.invalid/x.git", rev = "--upload-pack=touch /tmp/pwned" }"#,
    );
    assert!(bad.is_err(), "accepted a flag as a revision: {bad:?}");
}

#[test]
fn a_revision_that_is_not_a_commit_is_refused() {
    assert!(with_source(r#"git = { url = "https://e.invalid/x.git", rev = "main" }"#).is_err());
    assert!(with_source(r#"git = { url = "https://e.invalid/x.git", rev = "abc" }"#).is_err());
}

#[test]
fn a_real_commit_is_accepted() {
    // The control. Without it a check that refused everything would pass every
    // assertion above.
    let ok = with_source(
        r#"git = { url = "https://e.invalid/x.git", rev = "0123456789abcdef0123456789abcdef01234567" }"#,
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn a_url_on_no_known_scheme_is_refused() {
    assert!(
        with_source(r#"git = { url = "--config=core.sshCommand=id", rev = "0123456789abcdef0123456789abcdef01234567" }"#)
            .is_err()
    );
    assert!(
        with_source(
            r#"git = { url = "file:///etc", rev = "0123456789abcdef0123456789abcdef01234567" }"#
        )
        .is_err()
    );
}

#[test]
fn every_accepted_scheme_is_accepted() {
    for u in ["https://e.invalid/x.git", "ssh://e.invalid/x.git", "git@e.invalid:x.git"] {
        let r = with_source(&format!(
            r#"git = {{ url = "{u}", rev = "0123456789abcdef0123456789abcdef01234567" }}"#
        ));
        assert!(r.is_ok(), "{u} was refused: {r:?}");
    }
}

#[test]
fn a_path_escaping_the_workspace_is_refused() {
    assert!(with_source(r#"path = { path = "../../etc" }"#).is_err());
    assert!(with_source(r#"path = { path = "/etc" }"#).is_err());
    assert!(with_source(r#"path = { path = "-rf" }"#).is_err());
    assert!(with_source(r#"path = { path = "tools/x" }"#).is_ok());
}

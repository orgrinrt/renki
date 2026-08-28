//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::tool::Workdir;

const T: Tool = Tool {
    short: "widget",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("eng"),
    engine_crate: "engine",
    cache_namespace: "t",
    default_url: "ssh://default",
    launcher_crate: "t-launcher",
    workdir: Some(Workdir {
        key: "work_dir",
        root_default: "work",
    }),
    ..Tool::CONVENTIONS
};

#[test]
fn each_form_is_read_under_the_tools_prefix() {
    for (text, want) in [
        ("eng_version = \"1.2\"\n", Reference::Version("1.2".into())),
        ("eng_rev = \"abc\"\n", Reference::Rev("abc".into())),
        ("eng_tag = \"v1\"\n", Reference::Tag("v1".into())),
        ("eng_branch = \"dev\"\n", Reference::Branch("dev".into())),
    ] {
        assert_eq!(Header::parse(&T, text).pin, Some(want), "{text}");
    }
}

#[test]
fn another_tools_prefix_is_not_this_tools_pin() {
    // the control that makes the test above mean anything: the reader is
    // keyed on the prefix, so a differently-prefixed key is invisible.
    let h = Header::parse(&T, "otherthing_version = \"1.2\"\n");
    assert_eq!(h.pin, None);
    assert!(h.to_pin(&T).is_none());
}

#[test]
fn a_nested_key_is_not_a_pin() {
    let text = "[some.table]\neng_version = \"1.2\"\n";
    assert_eq!(Header::parse(&T, text).pin, None);
}

#[test]
fn the_more_specific_form_wins_when_a_config_carries_several() {
    let text = "eng_version = \"1.2\"\neng_branch = \"dev\"\neng_rev = \"abc\"\n";
    assert_eq!(
        Header::parse(&T, text).pin,
        Some(Reference::Rev("abc".into()))
    );
    let text = "eng_version = \"1.2\"\neng_branch = \"dev\"\n";
    assert_eq!(
        Header::parse(&T, text).pin,
        Some(Reference::Branch("dev".into()))
    );
}

#[test]
fn the_url_defaults_and_is_overridable() {
    let p = Header::parse(&T, "eng_tag = \"v1\"\n").to_pin(&T).unwrap();
    assert_eq!(p.url, "ssh://default");
    let p = Header::parse(&T, "eng_tag = \"v1\"\neng_git = \"ssh://other\"\n")
        .to_pin(&T)
        .unwrap();
    assert_eq!(p.url, "ssh://other");
}

#[test]
fn a_url_without_a_revision_is_not_a_pin() {
    // a source with nothing to check out of it cannot build anything, and
    // reporting it as a pin would defer the failure to cargo.
    let h = Header::parse(&T, "eng_git = \"ssh://other\"\n");
    assert_eq!(h.url.as_deref(), Some("ssh://other"));
    assert!(h.to_pin(&T).is_none());
}

#[test]
fn the_workdir_key_is_the_tools_own() {
    assert_eq!(
        Header::parse(&T, "work_dir = \"design\"\n")
            .workdir
            .as_deref(),
        Some("design")
    );
    assert_eq!(Header::parse(&T, "other_dir = \"design\"\n").workdir, None);
}

#[test]
fn unreadable_and_empty_configs_are_both_empty_headers() {
    assert_eq!(Header::parse(&T, "this is not [ toml"), Header::default());
    assert_eq!(Header::parse(&T, ""), Header::default());
}

#[test]
fn a_non_string_value_is_not_a_pin() {
    // toml is typed, so a number here is a config error rather than a pin,
    // and taking it as one would key the cache on nothing.
    assert_eq!(Header::parse(&T, "eng_version = 12\n").pin, None);
}

use super::package_name;

fn dir_with(manifest: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("Cargo.toml"), manifest).unwrap();
    d
}

#[test]
fn a_package_is_named_however_the_manifest_spells_the_assignment() {
    // TOML has more than one way to write the same thing, and a reader that
    // scans for one of them refuses manifests that are perfectly valid.
    for manifest in [
        "[package]\nname = \"engine\"\n",
        "[package]\nname='engine'\n",
        "[package]\n  name   =   \"engine\"   # the engine\n",
        "package = { name = \"engine\" }\n",
        "[package]\nversion = \"1\"\nname = \"engine\"\n",
    ] {
        let d = dir_with(manifest);
        assert_eq!(
            package_name(d.path()).as_deref(),
            Ok("engine"),
            "{manifest:?}"
        );
    }
}

#[test]
fn a_manifest_that_merely_mentions_the_name_is_not_that_package() {
    // Every one of these was accepted by a hand-rolled scan somewhere. Each
    // contains the string and declares something else, or declares nothing.
    for manifest in [
        // another package shipping a binary under that name, on both sides
        // of the package section: a scan that takes the first `name =` it
        // sees is right about one of these by the order alone.
        "[package]\nname = \"other\"\n\n[[bin]]\nname = \"engine\"\n",
        "[[bin]]\nname = \"engine\"\npath = \"src/main.rs\"\n\n[package]\nname = \"other\"\n",
        // a comment
        "[package]\nname = \"other\"  # not the engine\n",
        // a renamed dependency on it
        "[package]\nname = \"other\"\n\n[dependencies]\ne = { package = \"engine\" }\n",
        // the workspace root, which is what anyone points at first
        "[workspace]\nmembers = [\"engine\"]\n",
    ] {
        let d = dir_with(manifest);
        assert_ne!(
            package_name(d.path()).as_deref(),
            Ok("engine"),
            "{manifest:?}"
        );
    }
}

#[test]
fn a_virtual_manifest_says_so_rather_than_failing_somewhere_else() {
    // The case that matters most, because a workspace root is what somebody
    // reaches for and cargo's own complaint about it names neither the flag
    // that caused it nor the directory that would have worked.
    let d = dir_with("[workspace]\nmembers = [\"a\"]\n");
    let err = package_name(d.path()).unwrap_err();
    assert!(err.contains("no [package] name"), "{err}");
    assert!(err.contains("Cargo.toml"), "{err}");
}

#[test]
fn a_missing_or_unparseable_manifest_names_the_path() {
    let empty = tempfile::tempdir().unwrap();
    let err = package_name(empty.path()).unwrap_err();
    assert!(err.contains("Cargo.toml"), "{err}");

    let broken = dir_with("[package\nname =\n");
    let err = package_name(broken.path()).unwrap_err();
    assert!(err.contains("Cargo.toml"), "{err}");
}

#[test]
fn a_key_aimed_at_this_tool_and_missing_is_named() {
    // A config carrying `eng_ref` reads as carrying no pin at all, so the
    // reader is told to add one to a file that, to them, already has one.
    assert_eq!(
        near_miss(&T, "eng_ref = \"abc\"\n").as_deref(),
        Some("eng_ref")
    );
    // Whatever else the tool keeps its own config in is not a near miss. The
    // file belongs to the tool and an unknown key cannot be refused, which is
    // the whole reason this is narrow.
    assert_eq!(near_miss(&T, "work_dir = \"w\"\n"), None);
    assert_eq!(near_miss(&T, "unrelated = 1\n"), None);
    // Nor is a key that is one of the five.
    for k in [
        T.pin_keys.version,
        T.pin_keys.rev,
        T.pin_keys.tag,
        T.pin_keys.branch,
        T.pin_keys.git,
    ] {
        assert_eq!(near_miss(&T, &format!("{k} = \"x\"\n")), None, "{k}");
    }
    // And a file that is not toml at all answers nothing rather than failing:
    // `resolve_pin` reports the syntax error itself, before it gets here.
    assert_eq!(near_miss(&T, "eng_ref = \n"), None);
}

#[test]
fn a_tool_whose_keys_share_almost_nothing_gets_no_near_miss() {
    // The guard, and it is the reason `near_miss` can be wrong only by staying
    // quiet. With keys sharing one character or none there is no way to tell a
    // near miss from any other key the config carries, so every key in the file
    // would be reported and the message would be noise.
    let one = Tool {
        pin_keys: crate::PinKeys {
            version: "vv",
            rev: "vr",
            tag: "tt",
            branch: "bb",
            git: "gg",
        },
        ..T
    };
    assert_eq!(near_miss(&one, "vx = \"1\"\n"), None);

    let shared = Tool {
        pin_keys: crate::PinKeys {
            version: "ab_version",
            rev: "ab_rev",
            tag: "ab_tag",
            branch: "ab_branch",
            git: "ab_git",
        },
        ..T
    };
    assert_eq!(
        near_miss(&shared, "ab_ref = \"1\"\n").as_deref(),
        Some("ab_ref"),
        "a two-character shared prefix is enough and is the boundary"
    );
}

#[test]
fn the_shared_prefix_is_the_longest_every_name_begins_with() {
    assert_eq!(shared_prefix(&["eng_version", "eng_rev", "eng_tag"]), "eng_");
    // No overlap at all.
    assert_eq!(shared_prefix(&["a", "b"]), "");
    // One name is itself the prefix, so the answer cannot be longer than it.
    assert_eq!(shared_prefix(&["ab", "abc", "abcd"]), "ab");
    // Identical names.
    assert_eq!(shared_prefix(&["same", "same"]), "same");
    // A single name is its own prefix, and an empty list has none.
    assert_eq!(shared_prefix(&["only"]), "only");
    assert_eq!(shared_prefix(&[]), "");
    // An empty name drives the answer to nothing whatever sits beside it.
    assert_eq!(shared_prefix(&["eng_version", ""]), "");
}

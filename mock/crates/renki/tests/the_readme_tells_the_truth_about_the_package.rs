//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What the readme says about installing this crate is what the manifest ships.
//!
//! A readme is the one surface a stranger reads before anything else, and the
//! install block is the one part of it they copy rather than read. A version in
//! it that the crate does not ship sends them to a release that either does not
//! exist or is not the one the rest of the page describes, and nothing catches
//! that: the doctest compiles the usage example and has nothing to say about a
//! fenced `toml` block, and `cargo package` does not read prose.
//!
//! It ships, because it can. Both files it reads are in the package: cargo puts
//! `Cargo.toml` in every tarball and copies in whatever `readme` names. So the
//! check travels with the thing it checks, and a consumer running the suite gets
//! the same answer we do.

use std::path::{Path, PathBuf};

/// The readme the manifest ships, wherever it names it. The repository's own
/// sits three directories up from this crate, and reading the manifest's
/// `readme` rather than assuming a place is what keeps this check pointed at
/// the file crates.io will show.
fn readme_path(manifest_dir: &Path) -> PathBuf {
    let manifest = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("no manifest");
    let (_, rest) = manifest
        .split_once("\nreadme = \"")
        .expect("the manifest names no readme");
    let named = rest.split_once('"').expect("the readme is not closed").0;
    manifest_dir.join(named)
}

/// The `version = "..."` of the `[package]` table, which is the first one.
fn package_version(manifest: &str) -> &str {
    let (_, rest) = manifest
        .split_once("\nversion = \"")
        .expect("the manifest has no version");
    rest.split_once('"').expect("the version is not closed").0
}

/// Every `renki = "..."` a readme names, in a dependency block or in prose.
fn versions_the_readme_names(readme: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = readme;
    while let Some(at) = rest.find("renki = \"") {
        let tail = &rest[at + "renki = \"".len() ..];
        let (v, after) = tail.split_once('"').expect("an unclosed version");
        out.push(v);
        rest = after;
    }
    out
}

#[test]
fn every_version_the_readme_names_is_the_one_the_manifest_ships() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");
    let readme = std::fs::read_to_string(readme_path(root)).expect("no readme");

    let shipped = package_version(&manifest);
    let named = versions_the_readme_names(&readme);

    assert!(
        !named.is_empty(),
        "the readme names no version to install, so every assertion below held \
         over an empty set. It carried an `Installation` section when this was \
         written and losing it is the thing to notice, not to pass over."
    );
    for v in &named {
        assert_eq!(
            *v, shipped,
            "the readme tells a reader to install renki {v} and the manifest \
             ships {shipped}"
        );
    }
}

#[test]
fn the_readme_does_not_promise_a_binary_this_crate_has_none_of() {
    // The tagline said "renki is the launcher half" and the body corrected it a
    // hundred lines later with "you don't install renki itself as a command".
    // There is no `[[bin]]` and no `src/main.rs`, so the correction was the true
    // half and a reader who stopped at the tagline was misled.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");
    let readme = std::fs::read_to_string(readme_path(root)).expect("no readme");

    let ships_a_binary = declares_a_bin(&manifest) || root.join("src/main.rs").exists();
    assert!(
        !ships_a_binary,
        "this crate ships a binary now, so the readme may say so and this check \
         is the thing that is out of date"
    );
    assert!(
        !readme.contains("cargo install renki"),
        "the readme tells a reader to `cargo install renki`, which installs \
         nothing: there is no binary target"
    );
}

/// Whether the manifest declares a binary target.
///
/// The table header at the start of a line, not the string anywhere. A comment
/// explaining that there is no binary here contains the spelling, and a
/// `contains` reads that comment as the declaration it denies. That is not
/// hypothetical: adding the comment is what turned this red.
fn declares_a_bin(manifest: &str) -> bool {
    manifest
        .lines()
        .any(|l| l.trim_start().starts_with("[[bin]]"))
}

/// The value of the first `key = "..."` in the manifest.
///
/// Not a TOML parse, and it does not know what table it is in. Every key it is
/// asked for here is declared once, in `[package]`, above every other table, so
/// the first match is the right one. A manifest that grew a second `description`
/// under some other table would need a real parse instead.
#[test]
fn the_description_is_the_readme_s_tagline() {
    // The description is the line the registry shows under the name and the
    // tagline is the line the forge shows under the title, and a stranger
    // reads one or the other first. They say the same thing, byte for byte:
    // the first `>` line under the title, wherever the badge block puts it.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");
    let readme = std::fs::read_to_string(readme_path(root)).expect("no readme");
    let tagline = readme
        .lines()
        .skip_while(|l| !l.starts_with("# "))
        .find(|l| l.starts_with("> "))
        .expect("the readme carries no blockquote under its title")
        .trim_start_matches("> ")
        .trim();
    assert_eq!(package_str(&manifest, "description"), tagline);
}

fn package_str<'a>(manifest: &'a str, key: &str) -> &'a str {
    let needle = format!("\n{key} = \"");
    let (_, rest) = manifest
        .split_once(&needle)
        .unwrap_or_else(|| panic!("the manifest has no {key}"));
    rest.split_once('"').expect("the value is not closed").0
}

#[test]
fn the_manifest_does_not_sell_a_binary_either() {
    // The readme is not the only place a stranger reads before installing. The
    // description is the line crates.io puts under the name in a search result,
    // and the categories are what they arrive through. Both said utility, which
    // is what `cargo install renki` failing with `no packages found with
    // binaries or examples` was the consequence of.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("no manifest");

    // The word rather than the opening phrase. The point is that the sentence
    // says what the package is, and pinning where it says it makes an ordinary
    // rewording fail a test that has nothing to say about the rewording.
    let description = package_str(&manifest, "description");
    assert!(
        description.contains("library"),
        "the description does not say this is a library, and a reader who takes \
         it for a tool reaches for `cargo install`: {description}"
    );

    let (_, categories) = manifest
        .split_once("\ncategories = [")
        .expect("the manifest names no categories");
    let categories = categories.split_once(']').expect("unclosed categories").0;
    assert!(
        !categories.contains("command-line-utilities"),
        "`command-line-utilities` is where a reader looks for something to \
         install, and there is no binary here: {categories}"
    );
}

#[test]
fn the_checks_above_can_fail() {
    // The control on both. Each reads a document, and a parse that quietly finds
    // nothing passes every assertion it feeds.
    assert_eq!(
        package_version("\nversion = \"9.9.9\"\nname = \"x\"\n"),
        "9.9.9"
    );
    assert_eq!(
        versions_the_readme_names("a\nrenki = \"1.2\"\nb\nrenki = \"3.4\"\n"),
        vec!["1.2", "3.4"]
    );
    assert!(versions_the_readme_names("nothing here").is_empty());
    assert_eq!(
        package_str("\nname = \"x\"\ndescription = \"a thing\"\n", "description"),
        "a thing"
    );
    assert!(declares_a_bin(
        "[package]\nname = \"x\"\n\n[[bin]]\nname = \"x\"\n"
    ));
    assert!(
        !declares_a_bin("# no [[bin]] here, and that is the point\nname = \"x\"\n"),
        "a comment naming the table read as the table"
    );
}

// The checks above are about the package: its version, its lack of a binary,
// how it sells itself. The ones below are about the prose, which makes a great
// many more claims and had nothing reading any of them. Each names something a
// reader would act on, so each is worth the readme going red over.

use std::time::Duration;

use renki::Tool;

/// A descriptor whose only interesting field is the short name every derived
/// environment variable comes from.
const WIDGET: Tool = Tool {
    short: "widget",
    config_file: "widget.toml",
    pin_keys: renki::pin_keys!("widget"),
    engine_crate: "widget-engine",
    cache_namespace: "widget",
    default_url: "https://example.invalid/widget.git",
    launcher_crate: "widget",
    ..Tool::CONVENTIONS
};

/// Every backticked `WIDGET_...` the readme names, in the order it names them,
/// without repeats.
///
/// Backticked only, so the same names written inside the fenced usage example
/// are not counted twice: that comment is prose about the descriptor and the
/// table below it is the claim.
fn widget_variables_the_readme_names(readme: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in readme.split('`').skip(1).step_by(2) {
        let looks_like_ours = chunk.starts_with("WIDGET_")
            && chunk
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit());
        if looks_like_ours && !out.iter().any(|s| s == chunk) {
            out.push(chunk.to_string());
        }
    }
    out
}

#[test]
fn the_variables_the_readme_names_are_the_ones_a_launcher_reads() {
    // The readme's table is the only place a user is told these exist, and the
    // names are derived rather than written down anywhere else, so a change to
    // how they are derived leaves the table naming variables nothing reads.
    // Neither half is an assertion on its own: the pairing is.
    assert_eq!(WIDGET.root_env(), "WIDGET_ROOT");
    assert_eq!(WIDGET.cache_env(), "WIDGET_CACHE");
    assert_eq!(WIDGET.state_env(), "WIDGET_STATE");
    assert_eq!(WIDGET.no_self_update_env(), "WIDGET_NO_SELF_UPDATE");

    // The two a tool command inherits are derived by `extension::command` from
    // the same short name, so they are taken from a real `Command` rather than
    // spelled out here: a spelling written twice is a spelling that can drift.
    let spawned = variables_a_tool_command_inherits("widget");
    assert_eq!(spawned, vec!["WIDGET_WORKSPACE", "WIDGET_TOOL_ROOT"]);

    // The three the configuration adds: the config directory's own override,
    // one variable per setting, which the table shows for a setting called
    // `model.base`, and the file's name on the engine's side.
    assert_eq!(WIDGET.config_env(), "WIDGET_CONFIG");
    let per_setting = renki_config::EnvKey::of("widget", "model.base").to_string();
    assert_eq!(per_setting, "WIDGET_CFG_MODEL_BASE");
    let file = renki_config::EnvKey::file("widget").to_string();
    assert_eq!(file, "WIDGET_CONFIG_FILE");

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let readme = std::fs::read_to_string(readme_path(root)).expect("no readme");
    let mut want = vec![
        WIDGET.root_env(),
        WIDGET.cache_env(),
        WIDGET.state_env(),
        WIDGET.no_self_update_env(),
    ];
    want.extend(spawned);
    want.push(WIDGET.config_env());
    want.push(per_setting);
    want.push(file);
    // As sets: the prose above the table names some of these before the table
    // does, and the order of first mention is not the claim.
    want.sort();
    let mut named = widget_variables_the_readme_names(&readme);
    named.sort();
    assert_eq!(
        named, want,
        "the readme names a different set of environment variables than a tool \
         called `widget` would actually answer to"
    );
}

/// The variables `extension::command` puts in a child's environment, read off
/// the `Command` it builds rather than written down a second time.
fn variables_a_tool_command_inherits(short: &str) -> Vec<String> {
    use renki::extension::{Descriptor, Located, command};

    static NTH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // Per call, not per process: the pattern this branch spent a blocker
    // removing from `place_via_scratch`, and no more correct in a test.
    let dir = std::env::temp_dir().join(format!(
        "renki-readme-{}-{}",
        std::process::id(),
        NTH.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("commands")).unwrap();
    std::fs::write(dir.join("commands/go"), "#!/bin/sh\n").unwrap();

    let d = Descriptor::parse(
        "[tool]\nname=\"t\"\nsummary=\"s\"\nbackend=\"local\"\n\
         [tool.source]\npath = { path = \"tools/t\" }\n\
         [[tool.commands]]\nname=\"go\"\nsummary=\"g\"\nrun=\"commands/go\"\n",
    )
    .expect("the fixture descriptor should parse");

    let at = Located {
        root: dir.clone(),
    };
    let cmd = command(&d, &at, "go", short, &dir, &[]).expect("the fixture command should build");
    let mut names: Vec<String> = cmd
        .get_envs()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    names.sort_by_key(|n| n.ends_with("_TOOL_ROOT"));
    let _ = std::fs::remove_dir_all(&dir);
    names
}

#[test]
fn the_retention_the_readme_states_is_the_one_the_base_carries() {
    // Thirty days is a number a reader plans around: it is how long a build
    // nobody has wanted survives before the collector takes it. It appears in
    // the readme as a word and in `Tool::CONVENTIONS` as a `Duration`, and
    // nothing connected the two, so the base could be cut to a week with the
    // readme still promising a month.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let readme = std::fs::read_to_string(readme_path(root)).expect("no readme");
    assert!(
        readme.contains("thirty days"),
        "the readme no longer states the default retention, so this check holds \
         over nothing. Say the figure, or delete this check on purpose"
    );
    assert_eq!(
        Tool::CONVENTIONS.cache_retention,
        Duration::from_secs(30 * 24 * 60 * 60),
        "the readme tells a reader thirty days and the base carries something else"
    );
}

/// Every backticked token in the readme shaped like a name from this crate's
/// own surface: an initial capital, alphanumerics, optionally one `::` segment.
///
/// The shape filter is what keeps `TOML`, `PATH`, `FNV`, `WIDGET_ROOT`,
/// `Cargo.toml` and `--dir` out of it, none of which this crate declares. A
/// token whose letters are all uppercase is a shouted noun rather than a type,
/// and a token carrying a `.`, a `-`, a space or a `_` in its first segment is
/// not a Rust path.
fn surface_names_the_readme_uses(readme: &str) -> Vec<String> {
    let segment_ok = |s: &str| {
        let mut c = s.chars();
        c.next()
            .is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
            && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    };
    let mut out: Vec<String> = Vec::new();
    for chunk in readme.split('`').skip(1).step_by(2) {
        let mut parts = chunk.split("::");
        let (Some(head), tail) = (parts.next(), parts.collect::<Vec<_>>()) else {
            continue;
        };
        let shaped = tail.len() <= 1
            && head.starts_with(|c: char| c.is_ascii_uppercase())
            && head.chars().all(|c| c.is_ascii_alphanumeric())
            && tail.iter().all(|t| segment_ok(t));
        let shouted = chunk
            .chars()
            .filter(char::is_ascii_alphabetic)
            .all(|c| c.is_ascii_uppercase());
        if shaped && !shouted && !out.iter().any(|s| s == chunk) {
            out.push(chunk.to_string());
        }
    }
    out
}

/// The surface the readme is allowed to name, each entry backed by a reference
/// below that does not compile if the item stops existing under that spelling.
///
/// A list rather than a source scan, because a scan for `pub struct X` reads a
/// comment mentioning one as a declaration, which is the defect `declares_a_bin`
/// above was written twice for. The list is not a free-standing declaration:
/// `every_name_here_is_a_name_this_crate_has` is what makes each entry cost
/// something, and adding a row without adding its reference there is what a
/// reviewer looks for.
const NAMEABLE: &[&str] = &[
    "Anchor",
    "Anchor::ConfigFile",
    "Anchor::Marker",
    "Backend",
    "Cargo",
    "Check",
    "Cli",
    "Descriptor",
    "Descriptor::check",
    "Git",
    "Hooks",
    "Local",
    "Locate",
    "Located",
    "Pin",
    "PinKeys",
    "Reference",
    "Registered",
    "Registered::of",
    "RegistryThenGitTag",
    "Resolved",
    "SelfUpdate",
    "SelfUpdate::Never",
    "Tool",
    "Tool::CONVENTIONS",
    "Tool::cache_retention",
    "Tool::commands",
    "VersionSource",
    "VersionSource::GitTag",
    "Workdir",
    "command",
    "fingerprint",
    "locate",
    "materialise",
    "places_itself",
];

#[test]
fn every_name_here_is_a_name_this_crate_has() {
    // The other half of `NAMEABLE`. Every row above is referenced here in a way
    // the compiler checks, so a rename that leaves the readme behind fails the
    // build of this test rather than shipping a readme naming a type nobody has.
    let _: Option<renki::Anchor> = Some(renki::Anchor::ConfigFile);
    let _: renki::Anchor = renki::Anchor::Marker(".git");
    let _: Option<renki::Check> = None;
    let _ = renki::Cli::DIR_FLAG;
    let _: renki::Hooks = renki::Hooks::NONE;
    let _: renki::Locate = renki::Locate::DEFAULT;
    let _: fn(&renki::Pin) -> (&str, &renki::Reference) = |p| (&p.url, &p.reference);
    let _: renki::PinKeys = renki::pin_keys!("widget");
    let _ = renki::VersionSource::RegistryThenGitTag;
    let _: fn(&renki::Resolved) -> &renki::Pin = |r| &r.pin;
    let _: renki::SelfUpdate = renki::SelfUpdate::Never;
    let _: renki::Tool = Tool::CONVENTIONS;
    let _: Duration = Tool::CONVENTIONS.cache_retention;
    let _: &[renki::Command] = Tool::CONVENTIONS.commands;
    let _: renki::VersionSource = renki::VersionSource::GitTag;
    let _: Option<renki::Workdir> = Tool::CONVENTIONS.workdir;

    // The extension half.
    let _: fn() -> String = <renki::extension::Cargo as renki::extension::Backend>::fingerprint;
    let _: fn(&renki::extension::Descriptor, &Path) -> Result<(), String> =
        <renki::extension::Git as renki::extension::Backend>::materialise;
    let _: fn() -> bool = <renki::extension::Local as renki::extension::Backend>::places_itself;
    let _: fn(&renki::extension::Descriptor) -> Result<(), String> =
        renki::extension::Descriptor::check;
    let _: renki::extension::Registered =
        renki::extension::Registered::of::<renki::extension::Local>();
    let _: fn(&renki::extension::Located) -> &std::path::PathBuf = |l| &l.root;
    let _ = renki::extension::locate;
    let _ = renki::extension::command;

    assert_eq!(
        NAMEABLE.len(),
        35,
        "a row was added without a reference above"
    );
}

#[test]
fn the_readme_names_nothing_this_crate_does_not_have() {
    // Containment rather than equality: dropping a mention is an editorial
    // choice, and naming a type that does not exist is a reader following a
    // link into nothing. Only the second is a defect, so only the second is red.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let readme = std::fs::read_to_string(readme_path(root)).expect("no readme");
    let used = surface_names_the_readme_uses(&readme);
    assert!(
        !used.is_empty(),
        "the readme names none of this crate's types, so the check below held \
         over an empty set"
    );
    for name in &used {
        assert!(
            NAMEABLE.contains(&name.as_str()),
            "the readme names `{name}`, which is not on the list of things this \
             crate has. Either it was renamed, or the list wants a row and a \
             reference in `every_name_here_is_a_name_this_crate_has`"
        );
    }
}

#[test]
fn the_prose_checks_above_can_fail() {
    // Controls. Each of the three readers finds nothing on the wrong input and
    // something on the right one, and a reader that quietly finds nothing
    // passes every assertion it feeds.
    assert_eq!(
        widget_variables_the_readme_names("| `WIDGET_ROOT` | a |\n| `WIDGET_CACHE` | b |"),
        vec!["WIDGET_ROOT", "WIDGET_CACHE"]
    );
    assert!(
        widget_variables_the_readme_names("WIDGET_ROOT with no backticks").is_empty(),
        "an unbackticked name in the usage example counted as a claim"
    );
    assert_eq!(
        widget_variables_the_readme_names("`WIDGET_ROOT` and `WIDGET_ROOT` again"),
        vec!["WIDGET_ROOT"],
        "the same variable named twice counted twice"
    );

    assert_eq!(
        surface_names_the_readme_uses("`Tool` and `Tool::CONVENTIONS` and `Anchor::Marker`"),
        vec!["Tool", "Tool::CONVENTIONS", "Anchor::Marker"]
    );
    for not_a_type in ["`TOML`", "`PATH`", "`FNV`", "`WIDGET_ROOT`", "`Cargo.toml`"] {
        assert!(
            surface_names_the_readme_uses(not_a_type).is_empty(),
            "{not_a_type} was read as a name this crate declares"
        );
    }
    assert!(
        surface_names_the_readme_uses("`renki` and `--dir` and `cargo add renki`").is_empty(),
        "a lowercase word, a flag or a command line read as a type"
    );

    // And the one that must fail: a readme naming a type that is not there.
    assert!(
        !NAMEABLE.contains(&"Widget"),
        "the guard list would accept a name this crate has never had"
    );
}

#[test]
fn the_package_ships_its_licence_beside_the_manifest() {
    // `include` is an allowlist naming `LICENSE`, and cargo copies only what is
    // there: when the crate root moved under `mock/crates/` the file did not
    // follow and the package shipped without one, with nothing saying so.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let licence = std::fs::read_to_string(root.join("LICENSE"))
        .expect("a LICENSE beside the manifest, since include names it");
    assert!(
        licence.contains("Mozilla Public License"),
        "the licence beside the manifest is not the MPL the manifest declares"
    );
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let include = manifest
        .split_once("\ninclude = [")
        .expect("the manifest has an include list")
        .1;
    let include = include
        .split_once(']')
        .expect("the include list is closed")
        .0;
    assert!(
        include.contains("\"LICENSE\""),
        "include no longer names LICENSE"
    );
}

//! Tests for the extension model.

use std::path::Path;

use super::*;

const DESC: &str = r#"
[tool]
name    = "rules"
summary = "read the workspace rules"
tags    = ["rules", "docs"]
backend = "git"
promote = true

[tool.source]
git = { url = "https://example.invalid/rules.git", rev = "abc123def456789" }

[[tool.commands]]
name    = "list"
summary = "every rule"
run     = "commands/list"

[[tool.commands]]
name    = "show"
summary = "one rule whole"
run     = "commands/show"
"#;

fn desc() -> Descriptor {
    Descriptor::parse(DESC).expect("the fixture should parse")
}

/// Writes one file and counts how often it was asked to.
///
/// The count is what a cache-hit test actually needs. Checking the file's
/// contents instead looks equivalent and is not: with the cache check bypassed,
/// the second run materialises into scratch and then fails to rename over the
/// existing directory, falls into the lost-the-race branch, and returns the
/// original. The content is unchanged and the work was done twice.
struct Marker;

static MATERIALISED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl Backend for Marker {
    const NAME: &'static str = "marker";

    fn fingerprint() -> String {
        "marker-v1".into()
    }

    fn materialise(d: &Descriptor, into: &Path) -> Result<(), String> {
        MATERIALISED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
        std::fs::write(into.join("who"), &d.name).map_err(|e| e.to_string())
    }
}

/// Fails, having written something first, so the cleanup path is tested
/// against a directory that exists rather than one that never did.
struct Broken;

impl Backend for Broken {
    const NAME: &'static str = "broken";

    fn fingerprint() -> String {
        String::new()
    }

    fn materialise(_: &Descriptor, into: &Path) -> Result<(), String> {
        let _ = std::fs::create_dir_all(into);
        let _ = std::fs::write(into.join("half"), "written before failing");
        Err("no".into())
    }
}

static TEST_BACKENDS: &[Registered] = &[
    Registered::of::<Marker>(),
    Registered::of::<Broken>(),
    Registered::of::<Local>(),
    Registered::of::<Git>(),
];

fn registry() -> Registry {
    Registry::new(TEST_BACKENDS)
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("renki-ext-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// --- the descriptor ------------------------------------------------------

#[test]
fn a_descriptor_parses_every_field() {
    let d = desc();
    assert_eq!(d.name, "rules");
    assert_eq!(d.summary, "read the workspace rules");
    assert_eq!(d.tags, vec!["rules", "docs"]);
    assert_eq!(d.backend, "git");
    assert!(d.promote);
    assert_eq!(
        d.source,
        Source::Git {
            url: "https://example.invalid/rules.git".into(),
            rev: "abc123def456789".into(),
        }
    );
}

#[test]
fn the_command_table_is_the_dispatch_table() {
    // Why commands live in the descriptor at all: a host prints them, and
    // dispatches them, without fetching anything.
    let d = desc();
    assert_eq!(d.commands.len(), 2);
    assert_eq!(d.command("show").map(|c| c.run.as_str()), Some("commands/show"));
    assert_eq!(d.command("list").map(|c| c.summary.as_str()), Some("every rule"));
    assert!(d.command("nope").is_none());
}

#[test]
fn tags_commands_and_promote_default_when_absent() {
    let d = Descriptor::parse(
        r#"
        [tool]
        name = "x"
        summary = "y"
        backend = "local"
        [tool.source]
        path = { path = "tools/x" }
        "#,
    )
    .expect("a minimal descriptor is legal");
    assert!(d.tags.is_empty());
    assert!(d.commands.is_empty());
    assert!(!d.promote);
}

#[test]
fn a_descriptor_with_no_backend_is_refused() {
    // It cannot be dispatched at all, so it has to fail here rather than at
    // the moment somebody runs it.
    let bad = Descriptor::parse(
        r#"
        [tool]
        name = "x"
        summary = "y"
        [tool.source]
        path = { path = "tools/x" }
        "#,
    );
    assert!(bad.is_err(), "parsed with no backend: {bad:?}");
}

#[test]
fn nonsense_is_refused() {
    assert!(Descriptor::parse("this is not toml at all {{{").is_err());
}

// --- the cache key -------------------------------------------------------

#[test]
fn the_key_is_stable_for_one_descriptor() {
    let (d, r) = (desc(), registry());
    let b = r.get("git").unwrap();
    assert_eq!(cache_key(&d, b), cache_key(&d, b));
}

#[test]
fn the_key_moves_with_the_revision() {
    let r = registry();
    let b = r.get("git").unwrap();
    let a = desc();
    let mut z = desc();
    z.source = Source::Git {
        url: "https://example.invalid/rules.git".into(),
        rev: "def456abc123789".into(),
    };
    assert_ne!(cache_key(&a, b), cache_key(&z, b));
}

#[test]
fn the_key_moves_with_the_backend_fingerprint() {
    // A backend that compiles keys on its toolchain, so changing compiler must
    // force a coherent rebuild rather than reuse what the old one produced.
    let mut d = desc();
    d.backend = "marker".into();
    let r = registry();
    let (marker, git) = (r.get("marker").unwrap(), r.get("git").unwrap());
    assert_ne!(marker.fingerprint(), git.fingerprint());
    assert_ne!(cache_key(&d, marker), cache_key(&d, git));
}

// --- the registry --------------------------------------------------------

#[test]
fn the_registry_finds_what_it_holds_and_nothing_else() {
    let r = registry();
    assert_eq!(r.get("marker").map(|b| b.name), Some("marker"));
    assert!(r.get("nonexistent").is_none());
    assert!(r.names().contains(&"local"));
}

#[test]
fn the_builtin_table_carries_both_shipped_backends() {
    let r = Registry::new(BUILTIN);
    assert!(r.get("local").is_some());
    assert!(r.get("git").is_some());
}

// --- locating ------------------------------------------------------------

#[test]
fn an_unknown_backend_names_what_was_available() {
    // A typo in a descriptor is the common case, and a bare "unknown backend"
    // leaves the reader guessing at the spelling.
    let root = scratch("unknown");
    let mut d = desc();
    d.backend = "gti".into();
    let err = locate(&d, &registry(), &root, &root.join("cache")).unwrap_err();
    assert!(err.contains("gti"), "{err}");
    assert!(err.contains("git"), "should list what it has: {err}");
}

#[test]
fn a_path_source_resolves_against_the_workspace_and_is_not_cached() {
    let root = scratch("local");
    std::fs::create_dir_all(root.join("tools/x")).unwrap();
    let mut d = desc();
    d.backend = "local".into();
    d.source = Source::Path { path: "tools/x".into() };

    let cache = root.join("cache");
    let at = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(at.root, root.join("tools/x"));
    assert!(
        !cache.exists(),
        "a local tool was copied into the cache, so an edit to it would be invisible"
    );
}

#[test]
fn a_missing_local_directory_is_reported_against_the_path() {
    let root = scratch("absent");
    let mut d = desc();
    d.backend = "local".into();
    d.source = Source::Path { path: "tools/absent".into() };
    let err = locate(&d, &registry(), &root, &root.join("cache")).unwrap_err();
    assert!(err.contains("absent"), "{err}");
}

#[test]
fn a_non_caching_backend_refuses_a_git_source() {
    let root = scratch("mismatch");
    let mut d = desc();
    d.backend = "local".into(); // the source is still git
    let err = locate(&d, &registry(), &root, &root.join("cache")).unwrap_err();
    assert!(err.contains("path source"), "{err}");
}

#[test]
fn materialising_happens_once_and_the_second_call_is_a_cache_hit() {
    let root = scratch("cachehit");
    let cache = root.join("cache");
    let mut d = desc();
    d.backend = "marker".into();

    MATERIALISED.store(0, std::sync::atomic::Ordering::SeqCst);

    let first = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(std::fs::read_to_string(first.root.join("who")).unwrap(), "rules");
    assert_eq!(
        MATERIALISED.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the first call should have fetched exactly once"
    );

    let second = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(second.root, first.root);
    assert_eq!(
        MATERIALISED.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the second call fetched again instead of hitting the cache"
    );
}

#[test]
fn a_failed_materialise_leaves_nothing_behind() {
    // Half a tool in the cache is worse than none: the next run finds the
    // directory, treats it as a hit, and executes an incomplete checkout.
    let root = scratch("failed");
    let cache = root.join("cache");
    let mut d = desc();
    d.backend = "broken".into();

    assert!(locate(&d, &registry(), &root, &cache).is_err());
    let tools = cache.join("tools");
    let left: Vec<_> = std::fs::read_dir(&tools)
        .map(|it| it.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

// --- building the command ------------------------------------------------

fn runnable(root: &Path) {
    std::fs::create_dir_all(root.join("commands")).unwrap();
    std::fs::write(root.join("commands/list"), "#!/bin/sh\n").unwrap();
}

#[test]
fn a_command_runs_the_file_the_descriptor_names() {
    let root = scratch("run");
    runnable(&root);
    let at = Located { root: root.clone() };
    let cmd = command(&desc(), &at, "list", &root, &[]).unwrap();
    assert_eq!(cmd.get_program(), root.join("commands/list").as_os_str());
}

#[test]
fn a_command_is_told_which_workspace_it_is_acting_on() {
    // The load bearing part. A tool's code sits in a cache shared by every
    // workspace on the machine and its data does not, so being told is the
    // only way it can know which one it is acting on.
    let root = scratch("ws");
    runnable(&root);
    let ws = root.join("somewhere-else");
    std::fs::create_dir_all(&ws).unwrap();

    let at = Located { root: root.clone() };
    let cmd = command(&desc(), &at, "list", &ws, &[]).unwrap();
    let envs: Vec<_> = cmd.get_envs().collect();
    assert!(
        envs.iter()
            .any(|(k, v)| *k == "HOMMA_WORKSPACE" && v.is_some_and(|v| v == ws.as_os_str())),
        "the workspace did not reach the child: {envs:?}"
    );
    assert_eq!(cmd.get_current_dir(), Some(ws.as_path()));
}

#[test]
fn arguments_are_forwarded() {
    let root = scratch("args");
    runnable(&root);
    let at = Located { root: root.clone() };
    let args = vec!["--load".to_string(), "always".to_string()];
    let cmd = command(&desc(), &at, "list", &root, &args).unwrap();
    let got: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(got, args);
}

#[test]
fn an_unknown_command_lists_the_ones_that_exist() {
    let root = scratch("typo");
    let at = Located { root: root.clone() };
    let err = command(&desc(), &at, "shwo", &root, &[]).unwrap_err();
    assert!(err.contains("shwo"), "{err}");
    assert!(err.contains("list") && err.contains("show"), "{err}");
}

#[test]
fn a_command_whose_file_is_missing_says_so_before_running_anything() {
    let root = scratch("missing");
    let at = Located { root: root.clone() };
    let err = command(&desc(), &at, "list", &root, &[]).unwrap_err();
    assert!(err.contains("commands/list"), "{err}");
}

#[test]
fn a_tool_with_no_commands_says_that_rather_than_listing_nothing() {
    let root = scratch("nocmds");
    let at = Located { root: root.clone() };
    let mut d = desc();
    d.commands.clear();
    let err = command(&d, &at, "anything", &root, &[]).unwrap_err();
    assert!(err.contains("no commands"), "{err}");
}

// --- refusing a source a fetcher would misread ---------------------------

fn with_source(src: &str) -> Result<Descriptor, String> {
    Descriptor::parse(&format!(
        "[tool]\nname=\"x\"\nsummary=\"y\"\nbackend=\"git\"\n[tool.source]\n{src}\n"
    ))
}

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
        with_source(r#"git = { url = "--config=core.sshCommand=id", rev = "0123456abcdef" }"#)
            .is_err()
    );
    assert!(with_source(r#"git = { url = "file:///etc", rev = "0123456abcdef" }"#).is_err());
}

#[test]
fn every_accepted_scheme_is_accepted() {
    for u in ["https://e.invalid/x.git", "ssh://e.invalid/x.git", "git@e.invalid:x.git"] {
        let r = with_source(&format!(r#"git = {{ url = "{u}", rev = "0123456abcdef" }}"#));
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

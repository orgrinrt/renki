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
git = { url = "https://example.invalid/rules.git", rev = "0123456789abcdef0123456789abcdef01234567" }

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

    type Plan = Descriptor;

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

    type Plan = Descriptor;

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
            rev: "0123456789abcdef0123456789abcdef01234567".into(),
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

/// The same descriptor with one field changed, for the key tests below.
fn with_url(url: &str) -> Descriptor {
    let mut d = desc();
    let Source::Git { rev, .. } = &d.source else {
        unreachable!("the fixture is a git source")
    };
    d.source = Source::Git {
        url: url.into(),
        rev: rev.clone(),
    };
    d
}

#[test]
fn the_key_moves_with_the_url() {
    // The key hashes four fields and only two of them used to be constrained.
    // Deleting the url from the hash left every test green, and two tools at
    // different urls then shared one cache directory.
    let r = registry();
    let b = r.get("git").unwrap();
    assert_ne!(
        cache_key(&with_url("https://a.invalid/x.git"), b).unwrap(),
        cache_key(&with_url("https://b.invalid/x.git"), b).unwrap()
    );
}

#[test]
fn the_key_moves_with_the_named_backend() {
    // The fourth field, and the same story: two descriptors identical but for
    // which backend they name must not land on one entry, because the two
    // backends produce different trees from the same source.
    let r = registry();
    let git = r.get("git").unwrap();
    let mut a = desc();
    a.backend = "git".into();
    let mut b = desc();
    b.backend = "marker".into();
    assert_ne!(cache_key(&a, git).unwrap(), cache_key(&b, git).unwrap());
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
    assert_ne!(cache_key(&a, b).unwrap(), cache_key(&z, b).unwrap());
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
    assert_ne!(cache_key(&d, marker).unwrap(), cache_key(&d, git).unwrap());
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


    // Counted as a delta rather than against an absolute. The counter is a
    // global and the tests run in parallel threads, so an absolute reading is a
    // claim about every other test that touches this backend, and it broke each
    // time one was added.
    let count = || MATERIALISED.load(std::sync::atomic::Ordering::SeqCst);
    let before = count();

    let first = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(std::fs::read_to_string(first.root.join("who")).unwrap(), "rules");
    let after_first = count();
    assert!(
        after_first > before,
        "the first call did not fetch at all: {before} -> {after_first}"
    );

    let second = locate(&d, &registry(), &root, &cache).unwrap();
    assert_eq!(second.root, first.root);
    assert_eq!(
        count(),
        after_first,
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
    let cmd = command(&desc(), &at, "list", "renki", &root, &[]).unwrap();
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

    // Both variables, under two different hosts, because the names are derived
    // from the host's short name rather than fixed. Asserting one name under one
    // host passes just as well when the derivation is a hardcoded constant, and
    // a constant here is one host's policy in a published crate's contract with
    // every child process anybody spawns.
    for (short, ws_var, root_var) in [
        ("renki", "RENKI_WORKSPACE", "RENKI_TOOL_ROOT"),
        ("mock", "MOCK_WORKSPACE", "MOCK_TOOL_ROOT"),
    ] {
        let cmd = command(&desc(), &at, "list", short, &ws, &[]).unwrap();
        let envs: Vec<(String, Option<std::ffi::OsString>)> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_owned())))
            .collect();
        let got = |want: &str| {
            envs.iter()
                .find(|(k, _)| k == want)
                .and_then(|(_, v)| v.clone())
        };
        assert_eq!(
            got(ws_var).as_deref(),
            Some(ws.as_os_str()),
            "the workspace did not reach the child under {short}: {envs:?}"
        );
        assert_eq!(
            got(root_var).as_deref(),
            Some(root.as_os_str()),
            "the tool root did not reach the child under {short}: {envs:?}"
        );
        // And the other host's names are absent, so a derivation that set both
        // spellings would fail here rather than passing twice.
        for absent in ["HOMMA_WORKSPACE", "RENKI_WORKSPACE", "MOCK_WORKSPACE"] {
            if absent != ws_var {
                assert!(got(absent).is_none(), "{absent} set under {short}: {envs:?}");
            }
        }
        assert_eq!(cmd.get_current_dir(), Some(ws.as_path()));
    }
}

#[test]
fn arguments_are_forwarded() {
    let root = scratch("args");
    runnable(&root);
    let at = Located { root: root.clone() };
    let args = vec!["--load".to_string(), "always".to_string()];
    let cmd = command(&desc(), &at, "list", "renki", &root, &args).unwrap();
    let got: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(got, args);
}

#[test]
fn an_unknown_command_lists_the_ones_that_exist() {
    let root = scratch("typo");
    let at = Located { root: root.clone() };
    let err = command(&desc(), &at, "shwo", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("shwo"), "{err}");
    assert!(err.contains("list") && err.contains("show"), "{err}");
}

#[test]
fn a_command_whose_file_is_missing_says_so_before_running_anything() {
    let root = scratch("missing");
    let at = Located { root: root.clone() };
    let err = command(&desc(), &at, "list", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("commands/list"), "{err}");
}

#[test]
fn a_tool_with_no_commands_says_that_rather_than_listing_nothing() {
    let root = scratch("nocmds");
    let at = Located { root: root.clone() };
    let mut d = desc();
    d.commands.clear();
    let err = command(&d, &at, "anything", "renki", &root, &[]).unwrap_err();
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
        with_source(r#"git = { url = "--config=core.sshCommand=id", rev = "0123456789abcdef0123456789abcdef01234567" }"#)
            .is_err()
    );
    assert!(with_source(r#"git = { url = "file:///etc", rev = "0123456789abcdef0123456789abcdef01234567" }"#).is_err());
}

#[test]
fn every_accepted_scheme_is_accepted() {
    for u in ["https://e.invalid/x.git", "ssh://e.invalid/x.git", "git@e.invalid:x.git"] {
        let r = with_source(&format!(r#"git = {{ url = "{u}", rev = "0123456789abcdef0123456789abcdef01234567" }}"#));
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

// --- what a descriptor cannot reach --------------------------------------

/// A descriptor with one command, whose `run` is whatever is under test.
///
/// Built as a struct literal rather than parsed, deliberately: the fields are
/// public and `Deserialize` is derived, so this is a shape a host can hand
/// `command` without ever passing through `parse`, and a check that only runs
/// at parse time guards nothing about it.
fn with_run(run: &str) -> Descriptor {
    let mut d = desc();
    d.commands = vec![CommandDef {
        name:    "go".into(),
        summary: "go".into(),
        run:     run.into(),
    }];
    d
}

#[test]
fn an_absolute_run_cannot_name_an_executable_outside_the_tool() {
    // `Path::join` throws its left side away when the right is absolute, so an
    // unchecked `run` of `/bin/sh` spawns `/bin/sh` and the tool root the
    // descriptor was materialised into is never consulted.
    let root = scratch("run-absolute");
    runnable(&root);
    let at = Located { root: root.clone() };

    let err = command(&with_run("/bin/sh"), &at, "go", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("/bin/sh"), "{err}");
    assert!(err.contains("not inside it"), "{err}");
}

#[test]
fn a_run_climbing_out_of_the_tool_is_refused() {
    // The other half of the class. Relative, so `join` keeps the root, and the
    // result is still outside it. `is_file` accepts what `..` resolves to and
    // `Command` runs it.
    let root = scratch("run-climb");
    runnable(&root);
    std::fs::write(root.join("sh"), "#!/bin/sh\n").unwrap();
    let deep = root.join("a/b/c");
    std::fs::create_dir_all(deep.join("commands")).unwrap();
    std::fs::write(deep.join("commands/list"), "#!/bin/sh\n").unwrap();
    let at = Located { root: deep.clone() };

    let err = command(&with_run("../../../sh"), &at, "go", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("not inside it"), "{err}");

    // The control: from the same root, a command that stays inside runs.
    let ok = command(&desc(), &at, "list", "renki", &root, &[]);
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn a_run_that_is_a_symlink_out_of_the_tool_is_refused() {
    // A string check cannot see this one: `commands/list` stays inside by every
    // component test and resolves anywhere the link points. The string check
    // and the resolved check are different claims and neither implies the
    // other, so both are made.
    let root = scratch("run-link");
    let outside = scratch("run-link-target");
    std::fs::write(outside.join("sh"), "#!/bin/sh\n").unwrap();
    std::fs::create_dir_all(root.join("commands")).unwrap();
    std::os::unix::fs::symlink(outside.join("sh"), root.join("commands/list")).unwrap();
    let at = Located { root: root.clone() };

    let err = command(&desc(), &at, "list", "renki", &root, &[]).unwrap_err();
    assert!(err.contains("outside the tool"), "{err}");
}

#[test]
fn locate_checks_a_descriptor_it_was_handed_rather_than_trusting_it() {
    // Same premise as the run tests. `locate` takes a `&Descriptor`, and the
    // one it gets need never have been parsed.
    let root = scratch("locate-check");
    let mut d = desc();
    d.source = Source::Git {
        url: "--config=core.sshCommand=id".into(),
        rev: "0123456789abcdef0123456789abcdef01234567".into(),
    };
    let err = locate(&d, &registry(), &root, &root).unwrap_err();
    assert!(err.contains("url"), "{err}");
    assert!(
        !root.join("tools").exists(),
        "it got as far as creating the cache before refusing"
    );
}

#[test]
fn an_unknown_field_in_a_descriptor_is_refused_rather_than_ignored() {
    // A typo in a `tool.toml` used to parse to the field's default, so a
    // `promoted = true` silently meant `promote = false` and nothing said so.
    let base = "[tool]\nname=\"x\"\nsummary=\"y\"\nbackend=\"git\"\n\
                [tool.source]\ngit = { url = \"https://e.invalid/x.git\", \
                rev = \"0123456789abcdef0123456789abcdef01234567\" }\n";
    assert!(Descriptor::parse(base).is_ok(), "the control does not parse");
    assert!(Descriptor::parse(&format!("{base}promoted = true\n")).is_err());
    assert!(Descriptor::parse(&format!("{base}tag = [\"a\"]\n")).is_err());
    assert!(
        Descriptor::parse(&format!(
            "{base}[[tool.commands]]\nname=\"a\"\nsummary=\"b\"\nrun=\"c\"\ndescriptions=\"d\"\n"
        ))
        .is_err()
    );
}

#[test]
fn an_empty_path_is_refused() {
    // It resolves `Located.root` to the workspace root itself, which makes
    // every command's `run` relative to the whole repository.
    assert!(with_source(r#"path = { path = "" }"#).is_err());
}

#[test]
fn a_caching_backend_refuses_a_path_source() {
    // A path is relative to one workspace; the cache is shared by all of them.
    // The key would be a workspace-relative string, so two workspaces each
    // holding a `tools/x` would collide on one entry.
    let root = scratch("cache-path");
    let mut d = desc();
    d.backend = "marker".into();
    d.source = Source::Path {
        path: "tools/x".into(),
    };
    let err = locate(&d, &registry(), &root, &root).unwrap_err();
    assert!(err.contains("shared by all of them"), "{err}");
}

#[test]
fn two_threads_racing_on_one_key_publish_one_fetch_whole() {
    // The scratch name used to be derived from the process id alone, which
    // every thread in it shares. `locate` is `pub` and takes shared references,
    // so two threads on one key wrote into one scratch and the published tree
    // was spliced from both fetches, with both callers returning `Ok`.
    //
    // The backend below makes a splice visible: each thread writes a file named
    // for itself and a shared file holding its own id, after a stagger long
    // enough that an interleave is certain rather than lucky.
    static RACED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    struct Slow;

    impl Backend for Slow {
        const NAME: &'static str = "slow";

        type Plan = Descriptor;

        fn fingerprint() -> String {
            String::new()
        }

        fn materialise(_: &Descriptor, into: &Path) -> Result<(), String> {
            let n = RACED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
            std::fs::write(into.join("manifest"), n.to_string()).map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            std::fs::write(into.join(format!("payload-{n}")), "x").map_err(|e| e.to_string())
        }
    }

    static SLOW: &[Registered] = &[Registered::of::<Slow>()];
    let root = scratch("race");

    let mut d = desc();
    d.backend = "slow".into();

    std::thread::scope(|s| {
        for _ in 0 .. 2 {
            let (d, root) = (d.clone(), root.clone());
            s.spawn(move || {
                let r = Registry::new(SLOW);
                locate(&d, &r, &root, &root).expect("both callers should succeed");
            });
        }
    });

    let published = locate(&d, &Registry::new(SLOW), &root, &root).unwrap();
    // The last-used marker is the launcher's, not the fetch's, so it is not part
    // of what this counts.
    let mut names: Vec<String> = std::fs::read_dir(&published.root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();

    // Exactly one fetch's output, whichever won. A spliced tree carries the
    // manifest of one and the payload of the other, which is what this saw.
    assert_eq!(names.len(), 2, "the published tree is spliced: {names:?}");
    let manifest = std::fs::read_to_string(published.root.join("manifest")).unwrap();
    assert_eq!(
        names[1],
        format!("payload-{manifest}"),
        "manifest is from one fetch and the payload from another: {names:?}"
    );

    // And nothing is left behind: the loser removes its own scratch and only
    // its own.
    let leftovers: Vec<String> = std::fs::read_dir(root.join("tools"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "scratch left behind: {leftovers:?}");
}

// --- one contract, two ways of picking the impl ---------------------------

/// Records the directory it was handed, so a test can tell whether it was given
/// the destination or a scratch beside it.
static PLACED_IN_PLACE: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);
static PLACED_VIA_SCRATCH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

struct PlacesItself;

impl Backend for PlacesItself {
    const NAME: &'static str = "places-itself";

    type Plan = Descriptor;

    fn fingerprint() -> String {
        String::new()
    }

    fn materialise(_: &Descriptor, into: &Path) -> Result<(), String> {
        *PLACED_IN_PLACE.lock().unwrap() = Some(into.to_path_buf());
        std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
        std::fs::write(into.join("in-place"), "x").map_err(|e| e.to_string())
    }

    fn places_itself() -> bool {
        true
    }
}

#[test]
fn a_backend_that_places_itself_is_handed_the_destination() {
    // `cargo install --root` holds cargo's own lock over the install root and
    // moves the binary in itself, so wrapping it in a scratch and a rename would
    // stack a weaker mechanism on a working one and throw away the incremental
    // target directory every build. The backend says which it is, and this is
    // what that says.
    let root = scratch("in-place").join("dest");
    materialise_once::<PlacesItself>(&desc(), &root).unwrap();

    assert_eq!(
        PLACED_IN_PLACE.lock().unwrap().as_deref(),
        Some(root.as_path()),
        "it was handed a scratch rather than the destination"
    );
    assert!(root.join("in-place").is_file());

    // And nothing beside it: the scratch path is not taken at all.
    let siblings: Vec<String> = std::fs::read_dir(root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(siblings.is_empty(), "a scratch was made anyway: {siblings:?}");
}

#[test]
fn a_backend_that_does_not_is_handed_a_scratch_and_renamed_into_place() {
    // The control for the test above, and the default. The backend never sees
    // the destination, so a reader cannot observe a half-written tree.
    let root = scratch("via-scratch").join("dest");
    materialise_once::<PlacesElsewhere>(&desc(), &root).unwrap();

    assert!(root.join("elsewhere").is_file(), "nothing arrived at the destination");
    assert_ne!(
        PLACED_VIA_SCRATCH.lock().unwrap().as_deref(),
        Some(root.as_path()),
        "the backend was handed the destination rather than a scratch"
    );
    let siblings: Vec<String> = std::fs::read_dir(root.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.'))
        .collect();
    assert!(siblings.is_empty(), "the scratch was left behind: {siblings:?}");
}

#[test]
fn the_launcher_backend_is_one_impl_of_the_same_contract() {
    // The whole point of the trait. `Cargo` is planned on a `CargoPlan` rather
    // than a `Descriptor`, so it is picked at compile time and cannot go in a
    // registry, and it is still the same contract the fetching backends satisfy.
    assert_eq!(<Cargo as Backend>::NAME, "cargo");
    assert!(<Cargo as Backend>::places_itself());
    assert!(<Cargo as Backend>::caches());

    // The fingerprint is the toolchain, which is what makes a compiler change
    // re-key rather than reuse what the old one produced.
    assert_eq!(
        <Cargo as Backend>::fingerprint(),
        crate::cache::rustc_fingerprint()
    );
    assert!(
        !<Cargo as Backend>::fingerprint().is_empty(),
        "rustc is not on PATH, so this measures nothing"
    );

    // And it is absent from the shipped registry, because a descriptor naming
    // `backend = \"cargo\"` has no plan to hand it.
    assert!(Registry::new(BUILTIN).get("cargo").is_none());
}

#[test]
fn the_cargo_backend_reports_every_attempt_that_failed() {
    // Two attempts is the real shape: a version pin tries the registry and then
    // the matching git tag. Both fail here, and the message has to name both
    // rather than only the last.
    let into = scratch("cargo-fail").join("root");
    std::fs::create_dir_all(&into).unwrap();
    let err = Cargo::materialise(
        &CargoPlan {
            attempts:   vec![
                vec!["--no-such-flag-alpha".into()],
                vec!["--no-such-flag-beta".into()],
            ],
            bin:        "nothing".into(),
            crate_name: "the-engine".into(),
        },
        &into,
    )
    .unwrap_err();

    assert!(err.contains("the-engine"), "{err}");
    assert!(err.contains("alpha") && err.contains("beta"), "{err}");
}

/// The scratch-path counterpart of `PlacesItself`, recording the same way.
struct PlacesElsewhere;

impl Backend for PlacesElsewhere {
    const NAME: &'static str = "places-elsewhere";

    type Plan = Descriptor;

    fn fingerprint() -> String {
        String::new()
    }

    fn materialise(_: &Descriptor, into: &Path) -> Result<(), String> {
        *PLACED_VIA_SCRATCH.lock().unwrap() = Some(into.to_path_buf());
        std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
        std::fs::write(into.join("elsewhere"), "x").map_err(|e| e.to_string())
    }
}

#[test]
fn a_cargo_install_that_succeeds_without_the_binary_is_a_failure() {
    // cargo reporting success is not the same as cargo having built the thing
    // that was wanted. A crate whose binary is named something else installs
    // cleanly and leaves nothing at `plan.bin`, and returning Ok there hands the
    // launcher a path to a file that is not on disk.
    //
    // A real install rather than a stub, because the claim is about what cargo
    // actually does. No dependencies, so it builds offline in a second or two.
    let base = scratch("cargo-wrong-bin");
    let src = base.join("crate");
    std::fs::create_dir_all(src.join("src")).unwrap();
    std::fs::write(
        src.join("Cargo.toml"),
        "[package]\nname = \"alpha\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\
         [[bin]]\nname = \"alpha\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    std::fs::write(src.join("src/main.rs"), "fn main() {}\n").unwrap();

    let into = base.join("root");
    std::fs::create_dir_all(&into).unwrap();
    let plan = |bin: &str| CargoPlan {
        attempts:   vec![
            "--path".into(),
            src.display().to_string(),
            "--target-dir".into(),
            into.join("target").display().to_string(),
        ]
        .pipe_one(),
        bin:        bin.into(),
        crate_name: "alpha".into(),
    };

    // The control first, so a failure below is about the name rather than about
    // the crate not building at all.
    Cargo::materialise(&plan("alpha"), &into).expect("the fixture crate should install");
    assert!(into.join("bin/alpha").is_file());

    let err = Cargo::materialise(&plan("beta"), &into).unwrap_err();
    assert!(err.contains("produced no binary"), "{err}");
    assert!(err.contains("alpha"), "{err}");
}

/// One attempt as the list of attempts. Named rather than written inline
/// because `vec![vec![..]]` reads as a mistake.
trait PipeOne {
    fn pipe_one(self) -> Vec<Vec<String>>;
}

impl PipeOne for Vec<String> {
    fn pipe_one(self) -> Vec<Vec<String>> {
        vec![self]
    }
}

// --- collecting -----------------------------------------------------------

/// A tool tree, marked used now.
fn marked_tool(dir: &Path, name: &str) -> std::path::PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("payload"), "x").unwrap();
    std::fs::write(root.join(".last-used"), b"").unwrap();
    root
}

/// `now`, moved forward by `secs`.
///
/// The clock moves rather than the files. Ageing a marker backwards means
/// setting an mtime, which needs a dependency this crate does not have and
/// behaves differently on a directory; `collect` already takes the time it
/// should judge against, so there is nothing to reach for.
fn later(secs: u64) -> SystemTime {
    SystemTime::now() + std::time::Duration::from_secs(secs)
}

const DAY: u64 = 86400;

#[test]
fn a_tool_nothing_has_used_is_collected_and_a_fresh_one_is_not() {
    // Nothing else reaches `<cache>/tools`: a tool has no registry row to evict
    // on, because it is named by a workspace's configuration rather than
    // resolved through a pin. Without this every superseded revision stayed on
    // disk forever.
    let cache = scratch("collect");
    let tools = cache.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let a = marked_tool(&tools, "aaaa");

    // Nothing goes while the window holds, which is the control: a collector
    // that took everything would pass the second half alone.
    assert!(collect(&cache, std::time::Duration::from_secs(30 * DAY), later(DAY)).is_empty());
    assert!(a.exists());

    let removed = collect(
        &cache,
        std::time::Duration::from_secs(30 * DAY),
        later(90 * DAY),
    );
    assert_eq!(removed, vec!["aaaa".to_string()]);
    assert!(!a.exists(), "the stale tool is still on disk");
}

#[test]
fn a_scratch_from_a_dead_fetch_goes_on_a_much_shorter_rule() {
    // A scratch is a partial tree by definition and is never read, so one that
    // outlived its fetch is one whose process is gone. An hour is far longer
    // than any fetch and short enough that a crashed run does not leave a copy
    // of a repository sitting until the retention window.
    let cache = scratch("collect-scratch");
    let tools = cache.join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    let tool = marked_tool(&tools, "aaaa");
    let scratch_dir = tools.join(".aaaa.999.0");
    std::fs::create_dir_all(&scratch_dir).unwrap();

    // A retention window far longer than the scratch rule, so a scratch judged
    // on the tool rule would survive this and the assertion would fail.
    let removed = collect(
        &cache,
        std::time::Duration::from_secs(365 * DAY),
        later(4 * 3600),
    );

    assert_eq!(removed, vec![".aaaa.999.0".to_string()]);
    assert!(!scratch_dir.exists());
    assert!(tool.exists(), "the tool went with the scratch");
}

#[test]
fn a_tool_with_no_marker_is_stamped_rather_than_evicted() {
    // Everything already on disk predates this mechanism and carries no marker.
    // Reading that as "never used" would evict every tool on the machine the
    // first time a launcher with this code runs.
    let cache = scratch("collect-unmarked");
    let tools = cache.join("tools");
    std::fs::create_dir_all(tools.join("cccc")).unwrap();
    std::fs::write(tools.join("cccc/payload"), "x").unwrap();

    let removed = collect(&cache, std::time::Duration::from_secs(0), later(365 * DAY));

    assert!(removed.is_empty(), "it evicted an unmarked tool: {removed:?}");
    assert!(tools.join("cccc/payload").is_file());
    assert!(
        tools.join("cccc/.last-used").is_file(),
        "it left the tool unmarked, so the next pass makes the same decision"
    );
}

#[test]
fn locating_a_cached_tool_marks_it_used() {
    // The marker moves on the hit, not only on the fetch. Written once at fetch
    // time it says a tool used every day has not been touched since the day it
    // arrived, and the collector then takes it.
    let cache = scratch("collect-touch");
    let mut d = desc();
    d.backend = "marker".into();

    let at = locate(&d, &registry(), &cache, &cache).unwrap();
    let marker = at.root.join(".last-used");
    assert!(marker.is_file(), "a fresh fetch left no marker");
    let fetched_at = marker.metadata().unwrap().modified().unwrap();

    std::thread::sleep(std::time::Duration::from_millis(20));
    // The second call is a cache hit, and the hit is what has to move the mark.
    locate(&d, &registry(), &cache, &cache).unwrap();

    assert!(
        marker.metadata().unwrap().modified().unwrap() > fetched_at,
        "a cache hit left the mark where the fetch put it"
    );
}

// --- what the review of the first round found ----------------------------

#[test]
fn the_key_refuses_a_path_source_the_way_locate_does() {
    // `cache_key` is public, and its path arm used to hash a workspace-relative
    // string with an empty rev, which is exactly the collision `locate` refuses:
    // two workspaces each holding a `tools/x` land on one entry. A host reaching
    // the key directly got the collision the refusal exists to prevent.
    let r = registry();
    let b = r.get("marker").unwrap();
    let mut d = desc();
    d.source = Source::Path {
        path: "tools/x".into(),
    };

    let err = cache_key(&d, b).unwrap_err();
    assert!(err.contains("shared by all of them"), "{err}");

    // The control: a git source still keys.
    assert!(cache_key(&desc(), b).is_ok());
}

#[test]
fn a_scratch_this_process_owns_is_never_collected() {
    // A launcher collects on the same run that fetches, so a fetch slow enough
    // to cross the scratch bound would be collected by its own process. The pid
    // is in the name, which is the one case liveness settles cheaply.
    let cache = scratch("collect-own");
    let tools = cache.join("tools");
    std::fs::create_dir_all(&tools).unwrap();

    let mine = tools.join(format!(".aaaa.{}.0", std::process::id()));
    let theirs = tools.join(".aaaa.999999.0");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::create_dir_all(&theirs).unwrap();

    // Far past the bound, so age is not what spares it.
    let removed = collect(
        &cache,
        std::time::Duration::from_secs(365 * DAY),
        later(365 * DAY),
    );

    assert_eq!(removed, vec![".aaaa.999999.0".to_string()]);
    assert!(mine.exists(), "it collected its own in-flight scratch");
    assert!(!theirs.exists(), "the control survived, so nothing was collected");
}

#[test]
fn a_sha256_object_name_is_accepted() {
    // A repository may use sha-256, whose object names are sixty-four hex. The
    // length check was written for sha-1 alone and refused them.
    let sha256 = "a".repeat(64);
    let ok = with_source(&format!(
        r#"git = {{ url = "https://e.invalid/x.git", rev = "{sha256}" }}"#
    ));
    assert!(ok.is_ok(), "{ok:?}");

    // The controls, on either side: still not a short prefix, and still not an
    // arbitrary length between the two.
    for bad in [40 - 1, 41, 63, 65] {
        let r = with_source(&format!(
            r#"git = {{ url = "https://e.invalid/x.git", rev = "{}" }}"#,
            "a".repeat(bad)
        ));
        assert!(r.is_err(), "{bad} hex was accepted: {r:?}");
    }
}

#[test]
fn locate_and_the_launcher_place_through_one_body() {
    // They used to carry the same `places_itself` branch twice, and the doc on
    // `materialise_once` claimed `locate` called it, which it never did. A
    // precondition added to one would silently not reach the other.
    //
    // Checked by adding one here rather than by reading: `place` is the only
    // thing that refuses a root with no parent, so if either route stopped going
    // through it, that route would stop refusing.
    let places_itself = |_: &Path| Ok(());
    assert!(place(Path::new("/"), false, places_itself).is_err());

    // And the two routes agree on what a backend that places itself gets.
    let base = scratch("one-body");
    let via_generic = base.join("generic");
    materialise_once::<PlacesItself>(&desc(), &via_generic).unwrap();
    let generic_saw = PLACED_IN_PLACE.lock().unwrap().clone();

    let via_registry = base.join("tools").join("registry");
    std::fs::create_dir_all(base.join("tools")).unwrap();
    place(&via_registry, true, |into| {
        PlacesItself::materialise(&desc(), into)
    })
    .unwrap();
    let registry_saw = PLACED_IN_PLACE.lock().unwrap().clone();

    assert_eq!(generic_saw.as_deref(), Some(via_generic.as_path()));
    assert_eq!(registry_saw.as_deref(), Some(via_registry.as_path()));
    assert!(via_generic.join("in-place").is_file());
    assert!(via_registry.join("in-place").is_file());
}

#[test]
fn nothing_but_place_decides_how_material_is_placed() {
    // Finding 1 of the second review, and it is invisible to every runtime test
    // here: `locate` open-coded the same `places_itself` branch that
    // `materialise_once` had, so the two dispatches were separate bodies that
    // happened to agree. The doc three lines above `materialise_once` said
    // `locate` called it, which it never did, and the two had already drifted:
    // one refused a root with no parent and the other derived its parent itself.
    //
    // A source read rather than a behaviour check, because the defect is that
    // two bodies exist rather than that either is wrong. The repository already
    // tests prose this way; this is the same instrument pointed at a branch.
    let src = include_str!("extension.rs");
    let deciders: Vec<&str> = src
        .lines()
        .filter(|l| l.contains("places_itself") && l.trim_start().starts_with("if "))
        .collect();
    assert_eq!(
        deciders.len(),
        1,
        "the placement branch exists in more than one body, so a precondition \
         added to one does not reach the other: {deciders:?}"
    );

    // And the one that exists is inside `place`. Without this the assertion
    // above passes just as well when the single copy is the one in `locate`.
    let place_body = src
        .split_once("pub fn place(")
        .expect("`place` is gone, so this test is measuring nothing")
        .1;
    assert!(
        place_body.starts_with(
            "\n    root: &Path,\n    places_itself: bool,\n    materialise: impl FnOnce(&Path) \
             -> Result<(), String>,\n) -> Result<(), String> {\n    if places_itself {"
        ),
        "the branch is not the first thing `place` does: {}",
        &place_body[.. place_body.len().min(220)]
    );
}

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
    type Plan = Descriptor;

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
    type Plan = Descriptor;

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
    assert_eq!(d.source, Source::Git {
        url: "https://example.invalid/rules.git".into(),
        rev: "0123456789abcdef0123456789abcdef01234567".into(),
    });
}

#[test]
fn the_command_table_is_the_dispatch_table() {
    // Why commands live in the descriptor at all: a host prints them, and
    // dispatches them, without fetching anything.
    let d = desc();
    assert_eq!(d.commands.len(), 2);
    assert_eq!(
        d.command("show").map(|c| c.run.as_str()),
        Some("commands/show")
    );
    assert_eq!(
        d.command("list").map(|c| c.summary.as_str()),
        Some("every rule")
    );
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
    let Source::Git {
        rev,
        ..
    } = &d.source
    else {
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

#[test]
fn a_cargo_install_is_locked_to_the_source_s_own_lockfile() {
    // The failure this pins: an install that resolves afresh takes whatever
    // the registry published since the source was locked, and one such crate
    // did not build on the toolchain the source pins.
    let attempt = vec![
        "--git".to_string(),
        "https://x/y.git".to_string(),
        "--rev".to_string(),
        "abc".to_string(),
        "y-engine".to_string(),
    ];
    let args = install_args(&attempt, Path::new("/cache/k"));
    assert_eq!(args, vec![
        "install",
        "--git",
        "https://x/y.git",
        "--rev",
        "abc",
        "y-engine",
        "--locked",
        "--root",
        "/cache/k",
        "--force",
    ]);
    // the attempt's own arguments come first and whole, since cargo reads
    // the package name positionally after the source selector; the four
    // shapes the crate builds, per `pin.rs` and `engine.rs`
    let shapes: [&[&str]; 3] = [
        &["--path", "/src", "--target-dir", "/cache/k/target"],
        &["--git", "https://x/y.git", "--tag", "0.2.0", "y-engine"],
        &["y-engine", "--version", "0.2.0"],
    ];
    for shape in shapes {
        let attempt: Vec<String> = shape.iter().map(|s| s.to_string()).collect();
        let args = install_args(&attempt, Path::new("/cache/k"));
        assert_eq!(&args[1 .. 1 + shape.len()], attempt.as_slice(), "{shape:?}");
        assert_eq!(
            &args[1 + shape.len() ..],
            ["--locked", "--root", "/cache/k", "--force"],
            "{shape:?}"
        );
    }
}

/// A tool tree with one runnable command in it.
fn runnable(root: &Path) {
    std::fs::create_dir_all(root.join("commands")).unwrap();
    std::fs::write(root.join("commands/list"), "#!/bin/sh\n").unwrap();
}

// Locating a tool and building its command, in a file of their own by size.
#[path = "extension_tests/locating_and_commands.rs"]
mod locating_and_commands;

// --- refusing a source a fetcher would misread ---------------------------

fn with_source(src: &str) -> Result<Descriptor, String> {
    Descriptor::parse(&format!(
        "[tool]\nname=\"x\"\nsummary=\"y\"\nbackend=\"git\"\n[tool.source]\n{src}\n"
    ))
}

// The refusals themselves, in a file of their own by size.
#[path = "extension_tests/refusing_a_source.rs"]
mod refusing_a_source;

// What a descriptor cannot reach, and one race on one key, in a file of their
// own by size.
#[path = "extension_tests/what_a_descriptor_cannot_reach.rs"]
mod what_a_descriptor_cannot_reach;

// --- one contract, two ways of picking the impl ---------------------------

/// Records the directory it was handed, so a test can tell whether it was given
/// the destination or a scratch beside it.
static PLACED_IN_PLACE: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);
static PLACED_VIA_SCRATCH: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

struct PlacesItself;

impl Backend for PlacesItself {
    type Plan = Descriptor;

    const NAME: &'static str = "places-itself";

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
    assert!(
        siblings.is_empty(),
        "a scratch was made anyway: {siblings:?}"
    );
}

#[test]
fn a_backend_that_does_not_is_handed_a_scratch_and_renamed_into_place() {
    // The control for the test above, and the default. The backend never sees
    // the destination, so a reader cannot observe a half-written tree.
    let root = scratch("via-scratch").join("dest");
    materialise_once::<PlacesElsewhere>(&desc(), &root).unwrap();

    assert!(
        root.join("elsewhere").is_file(),
        "nothing arrived at the destination"
    );
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
    assert!(
        siblings.is_empty(),
        "the scratch was left behind: {siblings:?}"
    );
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
            attempts:   vec![vec!["--no-such-flag-alpha".into()], vec![
                "--no-such-flag-beta".into(),
            ]],
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
    type Plan = Descriptor;

    const NAME: &'static str = "places-elsewhere";

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
    let plan = |bin: &str| {
        CargoPlan {
            attempts:   vec![
                "--path".into(),
                src.display().to_string(),
                "--target-dir".into(),
                into.join("target").display().to_string(),
            ]
            .pipe_one(),
            bin:        bin.into(),
            crate_name: "alpha".into(),
        }
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
    // lint:allow(no-vec-in-trait-sig, trait-first-signatures) reason: a test helper shaping a plan's attempts, which are a list of argument lists.
    fn pipe_one(self) -> Vec<Vec<String>>;
}

impl PipeOne for Vec<String> {
    fn pipe_one(self) -> Vec<Vec<String>> {
        vec![self]
    }
}

// Collecting, and what the reviews found, in a file of their own by size.
#[path = "extension_tests/collecting.rs"]
mod collecting;

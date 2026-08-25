//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;
use crate::tool::Hooks;

/// A tool that demands nothing of a checkout beyond a manifest.
const PLAIN: Tool = Tool {
    short: "t",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "t",
    default_url: "u",
    launcher_crate: "t-launcher",
    ..Tool::CONVENTIONS
};

/// A tool that demands more of a checkout than a manifest, through the
/// verification hook.
const FUSSY: Tool = Tool {
    hooks: Hooks {
        verify_engine_dir: Some(|abs| {
            abs.join("extra")
                .is_dir()
                .then_some(())
                .ok_or_else(|| format!("--engine {} has no extra/", abs.display()))
        }),
        ..Hooks::NONE
    },
    ..PLAIN
};

/// Arguments, as the launcher receives them: bytes, not text.
fn strings(args: &[&str]) -> Vec<std::ffi::OsString> {
    args.iter().map(|s| std::ffi::OsString::from(*s)).collect()
}

#[test]
fn the_flag_is_taken_out_of_the_forwarded_arguments() {
    // The engine must never see it. It is the launcher's, like `--dir`, and
    // an engine given an argument it does not know reports a usage error
    // against a flag the user passed correctly.
    let (path, rest) = take_flag(
        strings(&["lock", "--engine", "/tmp/e", "--verbose"]),
        "--engine",
    );
    assert_eq!(path, Flag::Value("/tmp/e".into()));
    assert_eq!(rest, strings(&["lock", "--verbose"]));
}

#[test]
fn the_joined_form_is_the_same_flag() {
    let (path, rest) = take_flag(strings(&["--engine=/tmp/e", "close"]), "--engine");
    assert_eq!(path, Flag::Value("/tmp/e".into()));
    assert_eq!(rest, strings(&["close"]));
}

#[test]
fn a_run_without_the_flag_is_untouched() {
    // The control. Every assertion above would hold for a parser that
    // dropped arguments it did not recognise.
    let (path, rest) = take_flag(strings(&["lock", "--verbose"]), "--engine");
    assert_eq!(path, Flag::Absent);
    assert_eq!(rest, strings(&["lock", "--verbose"]));
}

#[test]
fn a_trailing_flag_with_no_value_takes_nothing() {
    let (path, rest) = take_flag(strings(&["lock", "--engine"]), "--engine");
    assert_eq!(
        path,
        Flag::Missing,
        "a value was invented for a flag that had none, or its absence was \
         reported as the flag never having been passed"
    );
    assert_eq!(rest, strings(&["lock"]));
}

#[test]
fn a_flag_with_another_flag_after_it_is_missing_its_value_rather_than_absent() {
    // The distinction the caller acts on, and the one this returned as a
    // bare `None` before: nobody passing the flag and somebody passing it
    // with nothing after it are different facts, and only the first means
    // "do what you do when it was not asked for".
    let (never, rest) = take_flag(strings(&["lock", "--verbose"]), "--engine");
    let (empty, rest2) = take_flag(strings(&["lock", "--engine", "--verbose"]), "--engine");
    assert_eq!(never, Flag::Absent);
    assert_eq!(empty, Flag::Missing);
    assert_ne!(never, empty, "the two cases collapsed back into one");
    // and in both the user's own argument survives, which is what the
    // `-` check is for
    assert_eq!(rest, strings(&["lock", "--verbose"]));
    assert_eq!(rest2, strings(&["lock", "--verbose"]));
}

#[test]
fn the_joined_form_with_nothing_after_it_is_missing_rather_than_empty() {
    // An empty path is not a path. `PathBuf::from("")` joined onto the
    // working directory canonicalises back to it, and a working directory
    // holding a `Cargo.toml` is the ordinary case for anyone running this
    // from inside a repository, so `Flag::Value("")` builds whatever tree
    // the command happened to be typed in and announces it as an override.
    //
    // The realistic way to type it is not by hand. `--engine=$SOMEWHERE`
    // with the variable unset expands to exactly this.
    let (path, rest) = take_flag(strings(&["lock", "--engine="]), "--engine");
    assert_eq!(path, Flag::Missing);
    assert_eq!(rest, strings(&["lock"]));

    // The control, one character apart: a real joined value still arrives
    // as a value, so this cannot be passing because the joined spelling
    // stopped working altogether.
    let (real, _) = take_flag(strings(&["lock", "--engine=/tmp/e"]), "--engine");
    assert_eq!(real, Flag::Value("/tmp/e".into()));
}

#[test]
fn nothing_after_the_users_double_dash_belongs_to_the_launcher() {
    // Every command line agrees that a bare `--` ends the options and hands
    // the rest over verbatim. So an engine invoked as `widget run -- --dir
    // /x` is being given `--dir /x` as its own argument, and a launcher
    // that eats it has changed what the user wrote.
    for flag in ["--engine", "--dir"] {
        let (found, rest) = take_flag(strings(&["run", "--", flag, "/x"]), flag);
        assert_eq!(found, Flag::Absent, "{flag} was taken from after `--`");
        assert_eq!(rest, strings(&["run", "--", flag, "/x"]));

        let joined = format!("{flag}=/x");
        let (found, rest) = take_flag(strings(&["run", "--", &joined]), flag);
        assert_eq!(found, Flag::Absent, "{flag}= was taken from after `--`");
        assert_eq!(rest, strings(&["run", "--", &joined]));
    }

    // The control: the same flag before the separator is still the
    // launcher's, so this is about position rather than about the parser
    // having stopped working.
    let (found, rest) = take_flag(
        strings(&["run", "--engine", "/x", "--", "--engine", "/y"]),
        "--engine",
    );
    assert_eq!(found, Flag::Value("/x".into()));
    assert_eq!(rest, strings(&["run", "--", "--engine", "/y"]));

    // `--engine --` is the flag with nothing after it, and stays that way.
    // This one is covered twice over, since the separator also begins with
    // `-`, so it holds with or without the stop above and is here for the
    // reading rather than as the thing that pins it.
    let (found, rest) = take_flag(strings(&["run", "--engine", "--", "x"]), "--engine");
    assert_eq!(found, Flag::Missing);
    assert_eq!(rest, strings(&["run", "--", "x"]));
}

#[test]
fn value_collapses_missing_into_absent_and_nothing_else() {
    // `strip_dir_flag` reads through this, deliberately: the user's
    // directory is discarded whether they named one or not.
    assert_eq!(Flag::Absent.value(), None);
    assert_eq!(Flag::Missing.value(), None);
    assert_eq!(Flag::Value("x".into()).value().as_deref(), Some(std::ffi::OsStr::new("x")));
}

#[test]
fn a_directory_that_is_not_a_crate_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let err = locate(&PLAIN, dir.path().as_os_str()).unwrap_err();
    assert!(err.contains("no Cargo.toml"), "{err}");
    // and the message names the engine the tool actually builds
    assert!(err.contains("engine"), "{err}");
}

#[test]
fn the_tools_own_demand_is_checked_after_the_manifest() {
    // and only the tool that makes it: the same tree passes for one and
    // fails for the other, which is what makes the hook a hook rather than
    // a rule the crate hardcodes for everyone.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), b"[package]\n").unwrap();
    let raw = dir.path().to_string_lossy().to_string();

    assert!(locate(&PLAIN, raw.as_ref()).is_ok());
    let err = locate(&FUSSY, raw.as_ref()).unwrap_err();
    assert!(err.contains("no extra/"), "{err}");

    std::fs::create_dir(dir.path().join("extra")).unwrap();
    assert!(locate(&FUSSY, raw.as_ref()).is_ok());
}

#[test]
fn a_real_checkout_resolves_to_an_absolute_path() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), b"[package]\n").unwrap();
    let got = locate(&PLAIN, dir.path().as_os_str()).unwrap();
    assert!(got.is_absolute());
}

/// Push a path's modification time back by `secs`, directory or file.
fn backdate(path: &Path, secs: u64) {
    let f = std::fs::File::options()
        .read(true)
        .open(path)
        .expect("open for backdating");
    let then = std::time::SystemTime::now() - std::time::Duration::from_secs(secs);
    f.set_modified(then).expect("set the modification time");
}

#[test]
fn a_build_used_today_survives_however_long_ago_it_was_made() {
    // A directory's own timestamp moves when an entry is added to it or
    // taken out of it, and not when something inside an entry is written.
    // cargo writes into `bin` and `target`, which exist after the first
    // run, so the root's timestamp is when it was created. Reading that as
    // last use swept a checkout somebody was working on daily.
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("engines");
    let root = scratch.join("aaaa");
    std::fs::create_dir_all(root.join("target")).unwrap();
    touch(&root);
    backdate(&root, SCRATCH_TTL_SECS * 3);

    sweep(dir.path());
    assert!(
        root.is_dir(),
        "a build used a moment ago was swept for having been created a long time ago"
    );
}

#[test]
fn a_build_nothing_has_touched_since_the_ttl_goes() {
    // The control for the one above, and for the sweep as a whole: with
    // the marker backdated too, the same directory is removed. Without
    // this the test above would pass against a sweep that had simply
    // stopped deleting anything.
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("engines");
    let root = scratch.join("aaaa");
    std::fs::create_dir_all(root.join("target")).unwrap();
    touch(&root);
    backdate(&root.join(SCRATCH_MARKER), SCRATCH_TTL_SECS * 3);
    backdate(&root, SCRATCH_TTL_SECS * 3);

    sweep(dir.path());
    assert!(
        !root.exists(),
        "a build nobody has used since the ttl survived"
    );
}

#[test]
fn a_root_from_before_the_marker_falls_back_to_its_own_timestamp() {
    // Whatever is already on disk was written by a version that left no
    // marker, so the sweep still has to be able to judge one.
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("engines");
    let old = scratch.join("aaaa");
    let new = scratch.join("bbbb");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    backdate(&old, SCRATCH_TTL_SECS * 3);

    sweep(dir.path());
    assert!(!old.exists(), "an unmarked stale root survived");
    assert!(new.is_dir(), "an unmarked fresh root was swept");
}

#[test]
fn the_sweep_keeps_fresh_builds_and_survives_a_missing_directory() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("engines");
    let fresh = scratch.join("aaaa");
    std::fs::create_dir_all(&fresh).unwrap();
    sweep(dir.path());
    assert!(fresh.is_dir(), "a build made a moment ago was swept");

    // A sweep of somewhere that does not exist is a normal first run.
    sweep(&dir.path().join("nothing-here"));
}

#[test]
fn a_flag_passed_twice_takes_the_last_one_and_leaves_neither_behind() {
    let (path, rest) = take_flag(
        strings(&["lock", "--engine", "/tmp/first", "--engine", "/tmp/second"]),
        "--engine",
    );
    assert_eq!(path, Flag::Value("/tmp/second".into()));
    assert_eq!(rest, strings(&["lock"]));

    // the two spellings mix, and the later one still decides
    let (mixed, rest) = take_flag(
        strings(&["--engine=/tmp/first", "lock", "--engine", "/tmp/second"]),
        "--engine",
    );
    assert_eq!(mixed, Flag::Value("/tmp/second".into()));
    assert_eq!(rest, strings(&["lock"]));

    // a later occurrence with no value is a usage error rather than a fallback
    // to the earlier one, because the last thing the user typed is what they
    // meant and it was incomplete
    let (last_empty, rest) = take_flag(strings(&["--engine=/tmp/first", "--engine="]), "--engine");
    assert_eq!(last_empty, Flag::Missing);
    assert!(rest.is_empty());
}

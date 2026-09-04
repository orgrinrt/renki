//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

const T: Tool = Tool {
    short: "widget",
    config_file: "t.toml",
    pin_keys: crate::pin_keys!("t"),
    engine_crate: "engine",
    cache_namespace: "t",
    default_url: "u",
    launcher_crate: "cargo-widget",
    ..Tool::CONVENTIONS
};

#[test]
fn a_launcher_with_a_broken_descriptor_refuses_to_start() {
    // The point of the check is that it runs, and a predicate tested only
    // as a predicate stays green when nothing calls it. Every arm below is
    // a descriptor that would otherwise run and misbehave quietly.
    const BAD: [Tool; 30] = [
        Tool {
            short: "my-tool",
            ..T
        },
        Tool {
            config_file: "",
            ..T
        },
        // One arm per pin key rather than one for the set. Each is read on a
        // different pin form, so a check over the set with a single empty
        // name would leave four of the five unenforced.
        Tool {
            pin_keys: PinKeys {
                version: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                rev: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                tag: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                branch: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                git: "",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            engine_crate: "",
            ..T
        },
        Tool {
            engine_bin: Some(""),
            ..T
        },
        Tool {
            cache_namespace: "",
            ..T
        },
        Tool {
            launcher_crate: "",
            ..T
        },
        Tool {
            anchor: Anchor::Marker(""),
            ..T
        },
        // Empty, so both git attempts ask cargo to install from nowhere and
        // it fails naming a url the user never wrote.
        Tool {
            default_url: "",
            ..T
        },
        Tool {
            dir_flag: "",
            ..T
        },
        // The same string for both, so `normalize_args` strips the user's
        // copy as the directory flag and `dispatch` then finds no override
        // to act on. The launcher runs and quietly ignores what was asked.
        Tool {
            engine_flag: Cli::DIR_FLAG,
            ..T
        },
        // Zero, so every build is older than the window the moment it lands.
        // The collector then removes it on the next pass and the tool rebuilds
        // from scratch on every run, while its own message says once per
        // version.
        Tool {
            cache_retention: std::time::Duration::ZERO,
            ..T
        },
        // Under the hour a branch resolution counts as the branch's tip. The
        // collector sweeps resolutions on this window, so one would go while it
        // is still live and a branch-pinned repo would ask the remote on every
        // single run.
        Tool {
            cache_retention: std::time::Duration::from_secs(59 * 60),
            ..T
        },
        // The two optional descriptors, which the check reached for a while and
        // did not read. Absent is a shape rather than a defect; present and
        // empty is what these are.
        Tool {
            workdir: Some(Workdir {
                key:          "",
                root_default: "src",
            }),
            ..T
        },
        Tool {
            workdir: Some(Workdir {
                key:          "t_dir",
                root_default: "",
            }),
            ..T
        },
        // One arm per answer key, for the reason the pin keys get one each: a
        // single empty name would leave the other three unenforced.
        Tool {
            locate: Some(Locate {
                subcommand: "",
                ..Locate::DEFAULT
            }),
            ..T
        },
        Tool {
            locate: Some(Locate {
                root_key: "",
                ..Locate::DEFAULT
            }),
            ..T
        },
        Tool {
            locate: Some(Locate {
                config_key: "",
                ..Locate::DEFAULT
            }),
            ..T
        },
        Tool {
            locate: Some(Locate {
                workdir_key: "",
                ..Locate::DEFAULT
            }),
            ..T
        },
        // Not empty, and still unusable: the answer names `root` twice with
        // two values behind it and a reader takes whichever came last. One arm
        // per pair rather than one for the set, because a check written as two
        // comparisons instead of three passes every arm but one of these.
        Tool {
            locate: Some(Locate {
                config_key: "root",
                ..Locate::DEFAULT
            }),
            ..T
        },
        Tool {
            locate: Some(Locate {
                workdir_key: "root",
                ..Locate::DEFAULT
            }),
            ..T
        },
        Tool {
            locate: Some(Locate {
                workdir_key: "config",
                ..Locate::DEFAULT
            }),
            ..T
        },
        // The same shape one namespace over, and the one that does damage
        // rather than confusion. Six names come out of one table, and two of
        // them spelled the same makes one line answer two questions.
        //
        // `tag` spelled as `version` is the worst of them: the reader tries
        // the more specific form first, so a version resolves as a tag, which
        // skips the registry attempt and the `version_tags` rewrite and fetches
        // a different artifact under a config that looks correct.
        Tool {
            pin_keys: PinKeys {
                tag: "t_version",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        Tool {
            pin_keys: PinKeys {
                rev: "t_branch",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        // The url key sharing a pin key, which makes one string both where the
        // engine comes from and which revision of it.
        Tool {
            pin_keys: PinKeys {
                git: "t_tag",
                ..crate::pin_keys!("t")
            },
            ..T
        },
        // Across the two namespaces rather than inside one, which is the pair
        // a check written per-struct cannot see: the working directory and a
        // revision are read out of the same table.
        Tool {
            workdir: Some(Workdir {
                key:          "t_branch",
                root_default: "sub",
            }),
            ..T
        },
    ];
    for bad in &BAD {
        let Some(why) = bad.defect() else {
            panic!("no defect reported for {:?}", bad.short);
        };

        // The message is the whole point of the arm, so it is read rather than
        // counted. One of these shipped with three runs of eighteen spaces in
        // it, from a continued literal whose backslashes were dropped in a
        // reflow, and `is_some()` had nothing to say about that. A `\`
        // continuation eats the leading whitespace of the next line and a bare
        // newline in a string does not, so the run of spaces IS the tell and it
        // cannot occur in a message anybody wrote on purpose.
        assert!(
            !why.contains("  "),
            "{:?}'s defect message carries a run of spaces, which is what a \
             dropped line continuation looks like: {why:?}",
            bad.short
        );
        assert!(
            !why.contains('\n'),
            "{:?}'s defect message carries a newline, so it breaks the one-line \
             shape every other diagnostic here has: {why:?}",
            bad.short
        );
        assert!(
            why.len() > 20 && why.ends_with(|c: char| c.is_alphanumeric() || c == '"'),
            "{:?}'s defect message is too short to diagnose anything, or ends \
             mid-thought: {why:?}",
            bad.short
        );

        let err = outcome(bad, &s(&["widget"])).expect_err("a broken launcher ran");
        assert!(
            err.contains("descriptor is not usable"),
            "it failed for some other reason, so nothing checked the descriptor: {err}"
        );
    }
}

#[test]
fn an_empty_pin_key_is_reported_by_name() {
    // A message saying one of five keys is empty leaves the reader to work out
    // which, and the five are one word apart. So each arm carries its own
    // name, and this checks the name rather than that something was returned.
    const CASES: [(Tool, &str); 5] = [
        (
            Tool {
                pin_keys: PinKeys {
                    version: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.version",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    rev: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.rev",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    tag: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.tag",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    branch: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.branch",
        ),
        (
            Tool {
                pin_keys: PinKeys {
                    git: "",
                    ..T.pin_keys
                },
                ..T
            },
            "pin_keys.git",
        ),
    ];

    for (bad, name) in &CASES {
        let msg = bad
            .defect()
            .expect("an empty pin key was not reported at all");
        assert!(
            msg.contains(name),
            "the defect message does not name the field that is wrong. \
             Expected it to mention `{name}`, got: {msg}"
        );
    }

    // The control. Every arm above differs from the fixture in exactly one
    // key, so without this the loop would still pass if `defect` refused the
    // fixture itself for some unrelated reason and happened to name the field
    // in that message.
    assert!(
        T.defect().is_none(),
        "the fixture is already broken, so nothing above isolated a single key"
    );
}

#[test]
fn a_sound_descriptor_is_not_refused() {
    // The control. Without it the test above passes for a `defect` that
    // returns `Some` unconditionally, which would refuse every launcher
    // ever built on this.
    assert!(T.defect().is_none(), "the fixture itself is not usable");
    const NAMED_BIN: Tool = Tool {
        engine_bin: Some("engine"),
        ..T
    };
    assert!(NAMED_BIN.defect().is_none());

    // A working directory with a name of its own, because the collision check
    // compares six names and a tool that declares no working directory has
    // five. Something has to stand in for the sixth in a const context, and
    // this is what says the stand-in is not itself read as a collision.
    const WITH_WORKDIR: Tool = Tool {
        workdir: Some(Workdir {
            key:          "t_dir",
            root_default: "sub",
        }),
        ..T
    };
    assert!(WITH_WORKDIR.defect().is_none());
}

#[test]
fn an_empty_engine_bin_would_have_looked_for_the_directory_itself() {
    // Why that arm is in the list, computed rather than asserted from
    // memory: the join produces the bin directory, and a directory is never
    // the file the cache short-circuits on, so the engine rebuilds forever.
    const EMPTY: Tool = Tool {
        engine_bin: Some(""),
        ..T
    };
    let looked_for = Path::new("/cache/builds/k/bin").join(EMPTY.engine_bin_name());

    // The claim is about renki, not about `Path`: an empty name adds no
    // component, so what the cache would short-circuit on is the bin directory.
    // Asserting the two spellings of that path against each other only measures
    // `Path`'s own trailing-separator handling.
    assert_eq!(EMPTY.engine_bin_name(), "");
    assert_eq!(
        looked_for,
        Path::new("/cache/builds/k/bin"),
        "an empty name should add no component"
    );
    assert!(
        !looked_for.is_file(),
        "a directory is never the file the cache short-circuits on, so the \
         engine would rebuild on every run"
    );
    // The control: a name that is not empty does add one, so the assertion
    // above is about the emptiness rather than about `join` in general.
    const NAMED: Tool = Tool {
        engine_bin: Some("shortname"),
        ..T
    };
    assert_eq!(
        Path::new("/cache/builds/k/bin").join(NAMED.engine_bin_name()),
        Path::new("/cache/builds/k/bin/shortname")
    );
}

#[test]
fn the_eviction_message_is_not_prefixed_with_the_tool_name() {
    // `run_without_sanitizing` puts the tool's name in front of every error it
    // prints, so a message carrying its own copy goes out as `widget: widget:`.
    // This branch needs a build that vanishes three times running, which no test
    // can arrange, so the message is asserted where it is built instead.
    let msg = eviction_exhausted(Path::new("/cache/builds/k/bin/engine"));

    assert!(
        !msg.starts_with(T.short),
        "the message prefixes itself and the printer prefixes it again: {msg}"
    );
    assert!(
        msg.contains("/cache/builds/k/bin/engine"),
        "the message does not say which build went missing: {msg}"
    );
    assert!(
        msg.contains(&EVICTION_RETRIES.to_string()),
        "the message does not say how many attempts were made: {msg}"
    );
    // The control: a message that did self-prefix is caught by the first
    // assertion, so it is about this string rather than about any string.
    let doubled = format!("{}: {msg}", T.short);
    assert!(doubled.starts_with(T.short));
}

/// Arguments, as the launcher receives them: bytes, not text.
fn s(v: &[&str]) -> Vec<std::ffi::OsString> {
    v.iter().map(|x| std::ffi::OsString::from(*x)).collect()
}

// The locate answer, the engine flag and the pin messages, in a file of
// their own by size.
#[path = "lib_tests/locate_and_pins.rs"]
mod locate_and_pins;

// A tool's own commands, through the dispatch.
#[path = "lib_tests/commands.rs"]
mod commands;

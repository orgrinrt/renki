//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

use super::*;

/// Every field written out, deliberately.
///
/// This is the consumer shape `Tool::CONVENTIONS` does not protect, and it
/// sits here so that a field added to `Tool` breaks the build right at the
/// sentence saying so. Nothing else in the suite would notice: the descriptors
/// in `pin_tests.rs` and `lib_tests.rs` all spread the base, which is exactly
/// what buys them the compatibility this one does not have.
const WITH: Tool = Tool {
    anchor:          Anchor::Marker(".git"),
    short:           "widget",
    config_file:     "t.toml",
    pin_keys:        crate::pin_keys!("t"),
    engine_crate:    "engine",
    engine_bin:      None,
    cache_namespace: "t",
    cache_retention: std::time::Duration::from_secs(30 * 24 * 60 * 60),
    settings:        &[],
    commands:        &[],
    scan_skip:       &[".git", "target", "node_modules"],
    default_url:     "u",
    version_source:  VersionSource::GitTag,
    launcher_crate:  "t-launcher",
    workdir:         Some(Workdir {
        key:          "work_dir",
        root_default: "work",
    }),
    dir_flag:        Cli::DIR_FLAG,
    engine_flag:     Cli::ENGINE_FLAG,
    locate:          Some(Locate::DEFAULT),
    self_update:     SelfUpdate::ChaseTheBranch,
    hooks:           Hooks::NONE,
};

const WITHOUT: Tool = Tool {
    workdir: None,
    ..WITH
};

#[test]
fn a_short_name_a_shell_cannot_spell_is_refused() {
    // The whole matrix of what an environment variable name may hold,
    // rather than the one hyphen case that prompted this.
    for ok in ["w", "widget", "cargo_mock", "w2", "W", "_w"] {
        assert!(
            Tool {
                short: ok,
                ..WITH
            }
            .short_is_usable(),
            "{ok} should be usable"
        );
    }
    for bad in ["", "my-tool", "my.tool", "my tool", "my/tool", "2tools", "tööli"] {
        assert!(
            !Tool {
                short: bad,
                ..WITH
            }
            .short_is_usable(),
            "{bad:?} should be refused"
        );
    }
}

#[test]
fn a_refused_short_name_would_have_produced_an_unusable_variable() {
    // The control that ties the predicate to what it is about, computed a
    // second way, over the variable name rather than over the short name.
    //
    // Both halves are load-bearing, and the second was found by this test
    // disagreeing with the first version of itself. An empty short is
    // perfectly spellable: it yields `_ROOT`, which every shell accepts and
    // which belongs to no tool, so every launcher built that way would read
    // the same variable.
    fn usable_variable(name: &str) -> bool {
        let b = name.as_bytes();
        let spellable = !b.is_empty()
            && !b[0].is_ascii_digit()
            && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_');
        spellable && name != "_ROOT"
    }
    for s in ["w", "widget", "my-tool", "2tools", "", "my.tool", "tööli"] {
        let t = Tool {
            short: s,
            ..WITH
        };
        assert_eq!(
            t.short_is_usable(),
            usable_variable(&t.root_env()),
            "disagreed about {s:?}"
        );
    }
}

#[test]
fn env_names_come_from_the_short_name() {
    assert_eq!(WITH.root_env(), "WIDGET_ROOT");
    assert_eq!(WITH.no_self_update_env(), "WIDGET_NO_SELF_UPDATE");
}

#[test]
fn a_root_config_defaults_to_the_subdirectory_beside_it() {
    let root = Path::new("/r");
    assert_eq!(WITH.workdir_for(root, root, None), Path::new("/r/work"));
}

#[test]
fn a_root_config_may_name_another() {
    let root = Path::new("/r");
    let got = WITH.workdir_for(root, root, Some("design".into()));
    assert_eq!(got, Path::new("/r/design"));
}

#[test]
fn a_config_inside_the_workdir_defaults_to_its_own_directory() {
    // and the trailing `/.` that default produces is collapsed, or every
    // path derived from it carries it.
    let got = WITH.workdir_for(Path::new("/r"), Path::new("/r/work"), None);
    assert_eq!(got, Path::new("/r/work"));
}

#[test]
fn a_tool_without_a_workdir_runs_against_the_root() {
    let root = Path::new("/r");
    // the control: the same inputs that give a subdirectory above give the
    // root here, including when the config declares something.
    assert_eq!(WITHOUT.workdir_for(root, root, None), root);
    assert_eq!(WITHOUT.workdir_for(root, root, Some("design".into())), root);
    assert_eq!(WITHOUT.workdir_default(root), root);
    assert_eq!(WITH.workdir_default(root), Path::new("/r/work"));
}

fn nothing(_: &Invocation<'_>) -> Result<(), String> {
    Ok(())
}

#[test]
fn a_command_table_is_refused_for_the_three_things_that_make_one_unreachable() {
    // Every arm is a descriptor that would run and answer the wrong thing
    // quietly: an empty name matches every bare argument, a name the crate's
    // own queries take never runs, and two of one name run by table order.
    const SETTINGS: &[renki_config::Declared<crate::config::Toml>] =
        &[renki_config::Setting::<renki_config::Bool, renki_config::User>::new(
            "strict", "false", "A flag.",
        )
        .row()];
    const CASES: [(Tool, &str); 4] = [
        (
            Tool {
                commands: &[Command {
                    name: "",
                    doc:  "d",
                    run:  nothing,
                }],
                ..WITH
            },
            "name is empty",
        ),
        (
            Tool {
                commands: &[Command {
                    name: "locate",
                    doc:  "d",
                    run:  nothing,
                }],
                ..WITH
            },
            "locate's subcommand",
        ),
        (
            Tool {
                settings: SETTINGS,
                commands: &[Command {
                    name: "config",
                    doc:  "d",
                    run:  nothing,
                }],
                ..WITH
            },
            "named `config`",
        ),
        (
            Tool {
                commands: &[
                    Command {
                        name: "workspace",
                        doc:  "d",
                        run:  nothing,
                    },
                    Command {
                        name: "other",
                        doc:  "d",
                        run:  nothing,
                    },
                    Command {
                        name: "workspace",
                        doc:  "d",
                        run:  nothing,
                    },
                ],
                ..WITH
            },
            "share a name",
        ),
    ];
    for (bad, expect) in &CASES {
        let why = bad
            .defect()
            .expect("a broken command table was not refused");
        assert!(why.contains(expect), "wrong refusal for {expect:?}: {why}");
    }
}

#[test]
fn a_command_table_the_crates_queries_do_not_shadow_is_not_refused() {
    // The controls, one per refusal above. `config` is a command's to take
    // where the tool has no settings, since the crate's query is then the
    // engine's if it is anybody's; `locate` is a command's where the tool
    // declares no locate query at all.
    const NAMED_CONFIG: Tool = Tool {
        commands: &[Command {
            name: "config",
            doc:  "d",
            run:  nothing,
        }],
        ..WITH
    };
    assert!(
        NAMED_CONFIG.defect().is_none(),
        "{:?}",
        NAMED_CONFIG.defect()
    );
    const NAMED_LOCATE: Tool = Tool {
        locate: None,
        commands: &[Command {
            name: "locate",
            doc:  "d",
            run:  nothing,
        }],
        ..WITH
    };
    assert!(
        NAMED_LOCATE.defect().is_none(),
        "{:?}",
        NAMED_LOCATE.defect()
    );
    const TWO: Tool = Tool {
        commands: &[
            Command {
                name: "workspace",
                doc:  "d",
                run:  nothing,
            },
            Command {
                name: "spawn",
                doc:  "d",
                run:  nothing,
            },
        ],
        ..WITH
    };
    // and the const form, which is what a consumer writes
    const _: () = assert!(TWO.defect().is_none());
    assert!(TWO.defect().is_none());
}

#[test]
fn the_command_named_is_the_first_argument_and_only_a_declared_one() {
    const TWO: Tool = Tool {
        commands: &[
            Command {
                name: "workspace",
                doc:  "d",
                run:  nothing,
            },
            Command {
                name: "spawn",
                doc:  "d",
                run:  nothing,
            },
        ],
        ..WITH
    };
    let s = |v: &[&str]| -> Vec<std::ffi::OsString> {
        v.iter().map(std::ffi::OsString::from).collect()
    };
    assert_eq!(
        TWO.command_named(&s(&["spawn", "x"])).map(|c| c.name),
        Some("spawn")
    );
    assert_eq!(
        TWO.command_named(&s(&["workspace"])).map(|c| c.name),
        Some("workspace")
    );
    // not in second position, not a prefix, not a name the table lacks, not
    // no arguments at all, and not an argument that is not text
    assert!(TWO.command_named(&s(&["x", "spawn"])).is_none());
    assert!(TWO.command_named(&s(&["spaw"])).is_none());
    assert!(TWO.command_named(&s(&["status"])).is_none());
    assert!(TWO.command_named(&[]).is_none());
    use std::os::unix::ffi::OsStringExt as _;
    assert!(
        TWO.command_named(&[std::ffi::OsString::from_vec(vec![0xFF])])
            .is_none()
    );
    // and a tool with none matches nothing, which is what sends `spawn` to
    // the engine there
    assert!(WITH.command_named(&s(&["spawn"])).is_none());
}

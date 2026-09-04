//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! The whole table, every kind on every platform, with the three sources at
//! every level. A root is a type, so the matrix is written out per type rather
//! than looped over an enum; the loops below are over the sources only.

use notko::{Maybe, Outcome};
use renki_dirs::{
    BadNamespace,
    Cache,
    Config,
    Data,
    EnvName,
    Host,
    Kind,
    MacOs,
    Namespace,
    Platform,
    Root,
    Short,
    Source,
    Sources,
    State,
    Xdg,
};

fn ns() -> Namespace<'static> {
    match Namespace::new("tns") {
        Outcome::Ok(n) => n,
        Outcome::Err(e) => panic!("{e:?}"),
    }
}

fn home() -> Maybe<&'static str> {
    Maybe::Is("/home/u")
}

fn path<K: Kind, P: Platform>(sources: Sources<'static>) -> String {
    match Root::<K, P>::resolve(ns(), sources) {
        Outcome::Ok(r) => r.to_string(),
        Outcome::Err(e) => panic!("{e}"),
    }
}

fn defaults<K: Kind, P: Platform>() -> String {
    path::<K, P>(Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: home(),
    })
}

#[test]
fn the_xdg_defaults_follow_the_specification() {
    assert_eq!(defaults::<Cache, Xdg>(), "/home/u/.cache/tns");
    assert_eq!(defaults::<State, Xdg>(), "/home/u/.local/state/tns");
    assert_eq!(defaults::<Config, Xdg>(), "/home/u/.config/tns");
    assert_eq!(defaults::<Data, Xdg>(), "/home/u/.local/share/tns");
}

#[test]
fn macos_uses_library_and_never_a_dot_directory() {
    // op's ruling: a cleanup that empties the platform's cache directory has
    // to find the engines there, and `~/.cache` on a mac is nobody's cache.
    assert_eq!(defaults::<Cache, MacOs>(), "/home/u/Library/Caches/tns");
    assert_eq!(
        defaults::<State, MacOs>(),
        "/home/u/Library/Application Support/tns/state"
    );
    assert_eq!(
        defaults::<Config, MacOs>(),
        "/home/u/Library/Application Support/tns"
    );
    assert_eq!(
        defaults::<Data, MacOs>(),
        "/home/u/Library/Application Support/tns/data"
    );
    for p in [
        defaults::<Cache, MacOs>(),
        defaults::<State, MacOs>(),
        defaults::<Config, MacOs>(),
        defaults::<Data, MacOs>(),
    ] {
        assert!(!p.contains("/."), "a mac root under a dot directory: {p}");
    }
}

#[test]
fn an_exported_xdg_variable_wins_over_the_platform_default_on_macos_too() {
    // somebody who set it has said where their files go; the platform default
    // is for the person who has said nothing.
    let s = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Is("/x"),
        home: home(),
    };
    assert_eq!(path::<Cache, MacOs>(s), "/x/tns");
    assert_eq!(path::<State, MacOs>(s), "/x/tns");
    assert_eq!(path::<Config, Xdg>(s), "/x/tns");
    assert_eq!(path::<Data, Xdg>(s), "/x/tns");
    // and the kind's macOS tail does not follow it: the variable names the
    // directory for that kind already
    assert!(!path::<State, MacOs>(s).ends_with("/state"));
}

#[test]
fn the_tools_own_variable_is_the_whole_path_and_wins_over_everything() {
    let s = Sources {
        own:  Maybe::Is("/mnt/big"),
        xdg:  Maybe::Is("/x"),
        home: home(),
    };
    assert_eq!(path::<Cache, Xdg>(s), "/mnt/big");
    assert_eq!(path::<Cache, MacOs>(s), "/mnt/big");
    assert_eq!(path::<State, Xdg>(s), "/mnt/big");
    assert_eq!(path::<Config, MacOs>(s), "/mnt/big");
    assert_eq!(path::<Data, Xdg>(s), "/mnt/big");
}

#[test]
fn the_source_that_decided_a_root_is_reported() {
    let own = Sources {
        own:  Maybe::Is("/mnt/big"),
        xdg:  Maybe::Is("/x"),
        home: home(),
    };
    let xdg = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Is("/x"),
        home: home(),
    };
    let def = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: home(),
    };
    let r = Root::<Cache, Host>::resolve(ns(), own).unwrap();
    assert_eq!(r.source(), Source::Own);
    assert_eq!(r.base(), "/mnt/big");
    let r = Root::<Cache, Host>::resolve(ns(), xdg).unwrap();
    assert_eq!(r.source(), Source::Xdg);
    assert_eq!(r.base(), "/x");
    let r = Root::<Cache, Host>::resolve(ns(), def).unwrap();
    assert_eq!(r.source(), Source::Default);
    assert_eq!(r.base(), "/home/u");
}

#[test]
fn an_empty_value_is_not_a_setting_at_any_level() {
    let s = Sources {
        own:  Maybe::Is(""),
        xdg:  Maybe::Is(""),
        home: home(),
    };
    assert_eq!(path::<Cache, Xdg>(s), "/home/u/.cache/tns");
    assert_eq!(
        path::<State, MacOs>(s),
        "/home/u/Library/Application Support/tns/state"
    );
    // an empty home is no home
    let s = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: Maybe::Is(""),
    };
    assert!(Root::<Cache, Xdg>::resolve(ns(), s).is_err());
}

#[test]
fn no_home_and_no_variable_is_an_error_naming_the_kind() {
    let e = Root::<Cache, Xdg>::resolve(ns(), Sources::NONE).unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("XDG_CACHE_HOME"), "{msg}");
    assert!(msg.contains("HOME"), "{msg}");
    assert!(msg.contains("cache files"), "{msg}");
    let e = Root::<State, MacOs>::resolve(ns(), Sources::NONE).unwrap_err();
    let msg = e.to_string();
    assert!(msg.contains("XDG_STATE_HOME"), "{msg}");
    assert!(msg.contains("state files"), "{msg}");
}

#[test]
fn the_cache_and_the_state_never_share_a_root() {
    // the property the whole crate exists for: a cleanup of one cannot take
    // the other, on either platform.
    assert_ne!(defaults::<Cache, Xdg>(), defaults::<State, Xdg>());
    assert_ne!(defaults::<Cache, MacOs>(), defaults::<State, MacOs>());
    assert!(!defaults::<State, Xdg>().starts_with(&defaults::<Cache, Xdg>()));
    assert!(!defaults::<State, MacOs>().starts_with(&defaults::<Cache, MacOs>()));
    // and configuration sits beside state on macOS with state one level down,
    // so a listing of the person's directory shows their files first
    assert!(defaults::<State, MacOs>().starts_with(&defaults::<Config, MacOs>()));
}

#[test]
fn two_tools_never_share_a_root() {
    let other = Namespace::new("another").unwrap();
    let s = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: home(),
    };
    let a = Root::<Cache, Xdg>::resolve(ns(), s).unwrap().to_string();
    let b = Root::<Cache, Xdg>::resolve(other, s).unwrap().to_string();
    assert_ne!(a, b);
}

#[test]
fn the_override_variables_are_named_after_the_tool_and_the_kind() {
    let t = Short("t");
    assert_eq!(EnvName::<Cache>::of(t).to_string(), "T_CACHE");
    assert_eq!(EnvName::<State>::of(t).to_string(), "T_STATE");
    assert_eq!(EnvName::<Config>::of(t).to_string(), "T_CONFIG");
    assert_eq!(EnvName::<Data>::of(t).to_string(), "T_DATA");
    let w = Short("widget");
    assert_eq!(EnvName::<Cache>::of(w).to_string(), "WIDGET_CACHE");
    // a name that is not ascii still uppercases by the character rather than
    // by the byte
    assert_eq!(EnvName::<Cache>::of(Short("äö")).to_string(), "ÄÖ_CACHE");
}

#[test]
fn a_namespace_is_one_segment_that_names_rather_than_walks() {
    assert!(Namespace::new("homma").is_ok());
    assert!(Namespace::new("a.b").is_ok());
    assert_eq!(Namespace::new("").unwrap_err(), BadNamespace::Empty);
    assert_eq!(
        Namespace::new("a/b").unwrap_err(),
        BadNamespace::HasSeparator
    );
    assert_eq!(
        Namespace::new("a\\b").unwrap_err(),
        BadNamespace::HasSeparator
    );
    assert_eq!(Namespace::new(".").unwrap_err(), BadNamespace::Walks);
    assert_eq!(Namespace::new("..").unwrap_err(), BadNamespace::Walks);
    // three dots is a name, however odd
    assert!(Namespace::new("...").is_ok());
    // and it is a const fn, so a tool's namespace is checked at compile time
    const NS: Outcome<Namespace<'static>, BadNamespace> = Namespace::new("homma");
    assert!(NS.is_ok());
}

#[test]
fn the_host_platform_is_the_one_this_binary_was_built_for() {
    // the control on the cfg: a build for one platform must not carry the
    // other's table, which is what a `cfg(unix)` would have done.
    let host = defaults::<Cache, Host>();
    if cfg!(target_os = "macos") {
        assert_eq!(host, defaults::<Cache, MacOs>());
    } else {
        assert_eq!(host, defaults::<Cache, Xdg>());
    }
}

#[test]
fn a_root_of_one_kind_is_not_a_root_of_another() {
    // the typestate: a function wanting the cache root cannot be handed the
    // state root, which is the property an enum could not give.
    fn wants_cache(_: Root<'_, Cache, Host>) {}
    let s = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: home(),
    };
    wants_cache(Root::<Cache, Host>::resolve(ns(), s).unwrap());
    // `wants_cache(Root::<State, Host>::resolve(ns(), s).unwrap())` does not
    // compile, which `tests/refusals/` pins.
}

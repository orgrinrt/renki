//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Where a tool's files go, typed.
//!
//! A tool keeps four kinds of file, told apart by what losing each one costs. A
//! [`Cache`] is rebuilt without being asked, and a cleanup may take the whole
//! directory at any moment. [`State`] is what the tool wrote for itself and
//! would behave differently without. [`Config`] is what the person wrote. [`Data`]
//! is what the tool produced for the person and cannot rebuild. Each kind has
//! the platform's own directory for it, and the platform is a type too: [`Xdg`]
//! for linux, the BSDs and illumos, following the base directory specification
//! with its documented fallbacks under the home directory, and [`MacOs`] for
//! `~/Library`, which is where that platform's own cleanup looks and where
//! `~/.cache` is nobody's cache. [`Host`] is whichever this binary was built for.
//!
//! The kinds and the platforms are marker types rather than an enum, so a root
//! is a [`Root<K, P>`] and a function that wants a cache root on this machine
//! says `Root<Cache, Host>` and cannot be handed the state root by mistake.
//!
//! Nothing here reads the environment or touches a filesystem. The caller reads
//! the three variables a root depends on, hands them in as [`Sources`], and gets
//! a [`Root`] back that knows its parts and which of the three won. A root
//! prints itself through [`core::fmt::Display`], which is the whole no-alloc
//! surface: a `std` caller formats it into a `PathBuf`, a no-std caller writes
//! it into a buffer of its own.
//!
//! Precedence for every root is the same: the tool's own variable, named
//! `<SHORT>_<KIND>` and holding the whole path, then the XDG variable for the
//! kind, then the platform's default under the home directory. The XDG
//! variables are honoured on macOS too, since somebody who exported one has said
//! where their files go and the platform default is for the person who has
//! said nothing. An empty value is not a setting at any level.

#![no_std]
#![forbid(unsafe_code)]

use core::fmt;
use core::marker::PhantomData;

use notko::{Maybe, Outcome};

mod sealed {
    pub trait Sealed {}
}

/// The kind of file a root holds, which decides where it goes.
///
/// Sealed: the four kinds are the whole vocabulary, and a fifth would need a
/// platform directory this crate has not named.
pub trait Kind: sealed::Sealed + Copy + Default + 'static {
    /// The word after `<SHORT>_` in the tool's own override variable, and the
    /// word a diagnostic uses for the kind.
    const NAME: &'static str; // lint:allow(no-bare-static-str) reason: a kind's static name; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    /// The XDG base directory variable for this kind.
    const XDG_VAR: &'static str; // lint:allow(no-bare-static-str) reason: a variable's static name; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    /// The XDG fallback under the home directory when the variable is unset.
    const XDG_DEFAULT: &'static str; // lint:allow(no-bare-static-str) reason: a directory's static name; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    /// The macOS directory under the home directory, before the namespace.
    const MACOS_DIR: &'static str; // lint:allow(no-bare-static-str) reason: a directory's static name; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    /// What follows the namespace on macOS, when the platform's directory is
    /// shared between kinds and this one takes a subdirectory of it.
    const MACOS_TAIL: Maybe<&'static str>;
}

macro_rules! kind {
    ($(#[$m:meta])* $name:ident, $upper:literal, $var:literal, $default:literal, $mac:literal, $tail:expr) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name;
        impl sealed::Sealed for $name {}
        impl Kind for $name {
            const NAME: &'static str = $upper;
            const XDG_VAR: &'static str = $var;
            const XDG_DEFAULT: &'static str = $default;
            const MACOS_DIR: &'static str = $mac;
            const MACOS_TAIL: Maybe<&'static str> = $tail;
        }
    };
}

kind!(
    /// Rebuilt on loss. Built engines, materialised tools, resolved pins.
    Cache, "CACHE", "XDG_CACHE_HOME", ".cache", "Library/Caches", Maybe::Isnt
);
kind!(
    /// Written by the tool for itself; behaviour changes on loss. A registry,
    /// a marker, a minted token. On macOS a subdirectory of the same
    /// `Application Support/<ns>` the configuration sits in, so a listing of
    /// that directory shows the person's files at the top and the tool's one
    /// level down.
    State, "STATE", "XDG_STATE_HOME", ".local/state", "Library/Application Support", Maybe::Is("state")
);
kind!(
    /// Written by the person, or by a surface the tool gave them. Never
    /// rebuilt, never written unasked. On macOS this is the directory the
    /// state and the data roots sit under, so clearing it takes both, which
    /// is the platform's layout and worth knowing before a `remove_dir_all`.
    Config, "CONFIG", "XDG_CONFIG_HOME", ".config", "Library/Application Support", Maybe::Isnt
);
kind!(
    /// Produced for the person and not rebuildable. On macOS the same
    /// directory as the configuration, which is what the platform does with
    /// it too.
    Data, "DATA", "XDG_DATA_HOME", ".local/share", "Library/Application Support", Maybe::Is("data")
);

/// Which platform's directory layout applies.
///
/// Sealed for the same reason as [`Kind`]: a platform is a table, and a table
/// this crate does not carry is not one it can answer for.
pub trait Platform: sealed::Sealed + Copy + Default + 'static {
    /// The directory under the home directory for `K`, and what follows the
    /// namespace there.
    fn under_home<K: Kind>() -> (&'static str, Maybe<&'static str>);
}

/// The XDG base directory specification: linux, the BSDs, illumos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Xdg;
impl sealed::Sealed for Xdg {}
impl Platform for Xdg {
    fn under_home<K: Kind>() -> (&'static str, Maybe<&'static str>) {
        (K::XDG_DEFAULT, Maybe::Isnt)
    }
}

/// `~/Library`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MacOs;
impl sealed::Sealed for MacOs {}
impl Platform for MacOs {
    fn under_home<K: Kind>() -> (&'static str, Maybe<&'static str>) {
        (K::MACOS_DIR, K::MACOS_TAIL)
    }
}

/// The platform this binary was built for.
///
/// A `cfg` on the operating system rather than on the family, since the BSDs
/// are unix and XDG.
#[cfg(target_os = "macos")]
pub type Host = MacOs;
// A refusal rather than a wrong answer: without this a Windows build would
// take the XDG column and print `%HOME%/.cache/<ns>`, a path that is nobody's
// cache there. The table has no column for it, and a port adds one.
#[cfg(not(unix))]
compile_error!(
    "renki-dirs has a column for XDG platforms and one for macOS, and this \
     target is neither. A port adds a `Platform` for it rather than borrowing \
     the XDG layout."
);
/// The platform this binary was built for.
#[cfg(not(target_os = "macos"))]
pub type Host = Xdg;

/// The directory name a tool owns under every root: `homma`, `vibecheck`.
///
/// One path segment. Not empty, no separator, and neither of the two names
/// that walk rather than name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Namespace<'a>(&'a str); // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.

/// Why a string is not a [`Namespace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadNamespace {
    /// Nothing to name a directory with.
    Empty,
    /// A separator, so it would be two segments.
    HasSeparator,
    /// `.` or `..`, which walk.
    Walks,
}

impl<'a> Namespace<'a> {
    /// Check a segment.
    pub const fn new(s: &'a str) -> Outcome<Self, BadNamespace> {
        // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
        if s.is_empty() {
            return Outcome::Err(BadNamespace::Empty);
        }
        let b = s.as_bytes();
        if b.len() <= 2 && b[0] == b'.' && (b.len() == 1 || b[1] == b'.') {
            return Outcome::Err(BadNamespace::Walks);
        }
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'/' || b[i] == b'\\' {
                return Outcome::Err(BadNamespace::HasSeparator);
            }
            i += 1;
        }
        Outcome::Ok(Namespace(s))
    }

    /// The segment.
    pub const fn as_str(&self) -> &'a str {
        // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
        self.0
    }
}

/// The short name a tool answers to, which prefixes its environment variables.
///
/// Checked, because it becomes a variable name: ASCII letters, digits and
/// underscore, not empty, not starting with a digit. A `Short` of `a-b` would
/// name `A-B_CACHE`, which no shell exports and every caller would read as
/// unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Short<'a>(&'a str); // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.

/// Why a string is not a [`Short`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadShort {
    /// Nothing to prefix a variable with.
    Empty,
    /// A variable name cannot start with a digit.
    LeadsWithDigit,
    /// A byte outside `[A-Za-z0-9_]`, so the variable could not be exported.
    NotAVariableName,
}

impl<'a> Short<'a> {
    /// Check a short name.
    pub const fn new(s: &'a str) -> Outcome<Self, BadShort> {
        // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
        let b = s.as_bytes();
        if b.is_empty() {
            return Outcome::Err(BadShort::Empty);
        }
        if b[0].is_ascii_digit() {
            return Outcome::Err(BadShort::LeadsWithDigit);
        }
        let mut i = 0;
        while i < b.len() {
            if !(b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                return Outcome::Err(BadShort::NotAVariableName);
            }
            i += 1;
        }
        Outcome::Ok(Short(s))
    }

    /// The name as given, lowercase or not.
    pub const fn as_str(&self) -> &'a str {
        // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
        self.0
    }
}

/// The tool's own override variable for a kind: `<SHORT>_CACHE`, `<SHORT>_STATE`,
/// uppercased through [`fmt::Display`] so nothing is allocated to name it.
#[derive(Debug, Clone, Copy)]
pub struct EnvName<'a, K: Kind> {
    short: Short<'a>,
    _kind: PhantomData<K>,
}

impl<'a, K: Kind> EnvName<'a, K> {
    /// The variable named after `short` for `K`.
    pub const fn of(short: Short<'a>) -> Self {
        EnvName {
            short,
            _kind: PhantomData,
        }
    }
}

impl<K: Kind> fmt::Display for EnvName<'_, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write;
        // ASCII by construction, so the uppercase of a char is one char.
        for c in self.short.0.chars() {
            f.write_char(c.to_ascii_uppercase())?;
        }
        f.write_char('_')?;
        f.write_str(K::NAME)
    }
}

/// The three environment values a root depends on, read by the caller.
///
/// `own` is the tool's variable named by [`EnvName`]; `xdg` is [`Kind::XDG_VAR`];
/// `home` is `HOME`. A value the caller could not read, or read as something
/// that is not text, is `Isnt`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    /// `<SHORT>_<KIND>`, the whole path.
    pub own:  Maybe<&'a str>, // lint:allow(no-bare-string) reason: borrowed from the caller's environment text; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    /// The XDG base directory for the kind.
    pub xdg:  Maybe<&'a str>, // lint:allow(no-bare-string) reason: borrowed from the caller's environment text; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    /// `HOME`.
    pub home: Maybe<&'a str>, // lint:allow(no-bare-string) reason: borrowed from the caller's environment text; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
}

impl<'a> Sources<'a> {
    /// Nothing set at all.
    pub const NONE: Sources<'static> = Sources {
        own:  Maybe::Isnt,
        xdg:  Maybe::Isnt,
        home: Maybe::Isnt,
    };
}

/// Which of the three sources decided a root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The tool's own variable, holding the whole path.
    Own,
    /// The XDG variable for the kind, with the namespace under it.
    Xdg,
    /// The platform's directory under the home directory.
    Default,
}

/// Neither the XDG variable for `K` nor `HOME` was set, so there is nowhere
/// to put the files. Carries the kind so the operator learns which root
/// failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoHome<K: Kind>(PhantomData<K>);

impl<K: Kind> fmt::Display for NoHome<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "neither {} nor HOME is set, so there is nowhere to put the {} files",
            K::XDG_VAR,
            K::NAME.to_ascii_lowercase_display()
        )
    }
}

/// `str::to_ascii_lowercase` allocates; this prints.
trait LowerDisplay {
    fn to_ascii_lowercase_display(&self) -> Lower<'_>;
}
impl LowerDisplay for str {
    fn to_ascii_lowercase_display(&self) -> Lower<'_> {
        Lower(self)
    }
}
struct Lower<'a>(&'a str); // lint:allow(no-bare-string) reason: borrowed from the tool's descriptor; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
impl fmt::Display for Lower<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write;
        for c in self.0.chars() {
            f.write_char(c.to_ascii_lowercase())?;
        }
        Ok(())
    }
}

/// A resolved root for kind `K` on platform `P`: the parts of the path and
/// which source decided it.
///
/// Prints as the path through [`fmt::Display`], segments joined with `/`. A
/// root decided by the tool's own variable prints that value alone; one from
/// the XDG variable prints it with the namespace under it; a default prints
/// the home directory, the platform's directory for the kind, the namespace,
/// and the kind's tail where the platform has one.
#[derive(Debug, Clone, Copy)]
pub struct Root<'a, K: Kind, P: Platform> {
    base:   &'a str, // lint:allow(no-bare-string) reason: borrowed from the caller's environment text; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
    under:  Maybe<&'static str>,
    ns:     Maybe<Namespace<'a>>,
    tail:   Maybe<&'static str>,
    source: Source,
    _kind:  PhantomData<K>,
    _plat:  PhantomData<P>,
}

impl<'a, K: Kind, P: Platform> Root<'a, K, P> {
    /// Resolve the root for `ns` from what the caller read.
    pub fn resolve(ns: Namespace<'a>, sources: Sources<'a>) -> Outcome<Self, NoHome<K>> {
        if let Maybe::Is(o) = sources.own
            && !o.is_empty()
        {
            return Outcome::Ok(Root {
                base:   o,
                under:  Maybe::Isnt,
                ns:     Maybe::Isnt,
                tail:   Maybe::Isnt,
                source: Source::Own,
                _kind:  PhantomData,
                _plat:  PhantomData,
            });
        }
        if let Maybe::Is(x) = sources.xdg
            && !x.is_empty()
        {
            return Outcome::Ok(Root {
                base:   x,
                under:  Maybe::Isnt,
                ns:     Maybe::Is(ns),
                tail:   Maybe::Isnt,
                source: Source::Xdg,
                _kind:  PhantomData,
                _plat:  PhantomData,
            });
        }
        let Maybe::Is(home) = sources.home else {
            return Outcome::Err(NoHome(PhantomData));
        };
        if home.is_empty() {
            return Outcome::Err(NoHome(PhantomData));
        }
        let (under, tail) = P::under_home::<K>();
        Outcome::Ok(Root {
            base: home,
            under: Maybe::Is(under),
            ns: Maybe::Is(ns),
            tail,
            source: Source::Default,
            _kind: PhantomData,
            _plat: PhantomData,
        })
    }

    /// Which source decided this root.
    pub const fn source(&self) -> Source {
        self.source
    }

    /// The first segment: the override value, the XDG directory, or the home
    /// directory.
    pub const fn base(&self) -> &'a str {
        // lint:allow(no-bare-string) reason: borrowed from the caller's environment text; the port to `hilavitkutin_str::Str` is a later unit. FIXME: port to Str.
        self.base
    }
}

impl<K: Kind, P: Platform> fmt::Display for Root<'_, K, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.base)?;
        if let Maybe::Is(u) = self.under {
            f.write_str("/")?;
            f.write_str(u)?;
        }
        if let Maybe::Is(ns) = self.ns {
            f.write_str("/")?;
            f.write_str(ns.as_str())?;
        }
        if let Maybe::Is(t) = self.tail {
            f.write_str("/")?;
            f.write_str(t)?;
        }
        Ok(())
    }
}

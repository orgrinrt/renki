//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! A manifest read off the disk with its layers.
//!
//! The rules are `renki-config`'s; this crate does the reading they need. It
//! is `#![no_std]` and allocates nothing: a file is read through the C library
//! into memory the caller gives, and the i/o is a [`Host`], which [`Unix`]
//! implements over the C library. Every `unsafe` block of the crate is in the
//! one module `Unix` lives in; the rest of the crate refuses `unsafe`.

#![no_std]
#![deny(unsafe_code)]

#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "illumos"
)))]
compile_error!(
    "renki-manifest reads through the C library of linux, macOS, freebsd, netbsd, openbsd and \
     illumos, and has no `Host` for this target"
);

mod host;
#[allow(unsafe_code)]
mod unix;

pub use host::{BadRead, Host};
pub use unix::Unix;

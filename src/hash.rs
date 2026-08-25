//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! Stable FNV-1a 64-bit, vendored.
//!
//! The build cache key is persisted on disk and must mean the same thing
//! across Rust releases. `std::hash::DefaultHasher` (SipHash) is explicitly
//! not guaranteed stable across releases, so it cannot key a durable cache.
//! FNV-1a is trivial, deterministic forever, and collision-resistant enough
//! to distinguish compilation inputs for a cache directory name.

const OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

/// An FNV-1a 64-bit accumulator.
pub(crate) struct Fnv(u64);

impl Fnv {
    pub(crate) fn new() -> Self {
        Fnv(OFFSET)
    }

    /// Fold raw bytes in.
    pub(crate) fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// Fold a string in, followed by a NUL separator so that concatenating
    /// distinct fields cannot collide by running together (`"ab" + "c"` and
    /// `"a" + "bc"` hash differently).
    pub(crate) fn write_field(&mut self, s: &str) {
        self.write_bytes(s.as_bytes());
    }

    /// The same, for a field that is bytes rather than text.
    ///
    /// A path on unix is arbitrary bytes, and rendering one lossily to key a
    /// cache maps every invalid sequence onto `U+FFFD`. Two engine paths
    /// differing only in bytes no `str` can hold then hash the same and share a
    /// build directory.
    pub(crate) fn write_bytes(&mut self, b: &[u8]) {
        self.write(b);
        self.write(&[0]);
    }

    /// The 16-hex-digit cache-key string.
    pub(crate) fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv_1a_64_vectors() {
        // Known answers from outside this file, which is the whole difference
        // between a vector test and a restatement. What stood here compared
        // `Fnv::new().hex()` to `format!("{OFFSET:016x}")`, which is the
        // definition against itself and holds for any offset whatsoever.
        for (input, want) in [
            ("", 0xcbf2_9ce4_8422_2325u64),
            ("a", 0xaf63_dc4c_8601_ec8cu64),
            ("foobar", 0x8594_4171_f739_67e8u64),
        ] {
            let mut h = Fnv::new();
            h.write(input.as_bytes());
            assert_eq!(h.hex(), format!("{want:016x}"), "input {input:?}");
        }
    }

    #[test]
    fn field_separator_prevents_collision() {
        let mut a = Fnv::new();
        a.write_field("ab");
        a.write_field("c");
        let mut b = Fnv::new();
        b.write_field("a");
        b.write_field("bc");
        assert_ne!(a.hex(), b.hex());
    }

    #[test]
    fn the_field_form_is_pinned_too() {
        // `write_field` is what every caller uses and its separator is part of
        // the persisted cache key, so the vectors above do not cover it. What
        // stood here asserted `hex().len() == 16` and then compared a
        // computation to the same computation, which holds for a hash function
        // returning a constant. Changing either constant in this module
        // invalidates every cached build on every machine and left it green.
        let mut h = Fnv::new();
        h.write_field("a stable input");
        assert_eq!(h.hex(), "ccda88ec7937a739");

        let mut two = Fnv::new();
        two.write_field("a stable");
        two.write_field("input");
        assert_ne!(h.hex(), two.hex(), "the separator stopped separating");
    }
}

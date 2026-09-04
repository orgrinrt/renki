//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------

//! What must not compile, pinned so a later loosening cannot restore it.

#[test]
fn a_root_of_the_wrong_kind_or_platform_is_refused_at_compile_time() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/refusals/*.rs");
}

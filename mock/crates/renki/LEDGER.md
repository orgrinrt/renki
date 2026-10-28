# Ledger of tracked gaps

Each ignored test in this crate stands for a gap the design names and the code
does not close yet, and says which row here tracks it. A row is the number of
the issue on `orgrinrt/renki` that carries the work, and the `catalogue-ignore`
lint resolves every number an ignore names against this file, as
`mockspace.toml` configures it.

- #30 Refuse a toolchain rustup lacks by name before cargo runs. The test is `a_pinned_toolchain_rustup_lacks_is_refused_by_name_before_cargo_runs` in `retry.rs`, and the check it asks for takes the checkout as an argument.
- #31 Refuse an unreachable git source by name before cargo runs. The test is `an_unreachable_git_source_is_refused_by_name_before_cargo_runs` in `retry.rs`, and the check it asks for takes the source as an argument.

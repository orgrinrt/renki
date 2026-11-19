# Which cargo runs hold the build directory's lock

**Outcome:** WORKS

The question: when a program cargo starts calls `cargo` again, which of cargo's runs still hold the locks under the build directory, and which variables tell the program which run it is under. A launcher building its engine with `cargo install` into the same build directory waits on that lock for as long as the run holding it lasts.

`probe.sh` lists each `.cargo*lock` under the profile directory as `held` or `free` (`flock -n`) and says which of cargo's variables are set. `build.rs`, `src/bin/run.rs` and `tests/t.rs` run it from a build script, from `cargo run` and from `cargo test`. A profile of its own, `locktest`, keeps the directory apart from every other build sharing `/home/user/target`.

```
cargo test --profile locktest -q
cargo run --profile locktest -q --bin run
```

cargo 1.98.0-nightly (fbb61be30 2026-05-26). The three files the runs wrote are committed beside this one.

- Build script: `.cargo-build-lock`, `.cargo-lock` and `.cargo-artifact-lock` all held. `CARGO`, `OUT_DIR`, `TARGET`, `HOST`, `NUM_JOBS`, `RUSTC`, `CARGO_MANIFEST_DIR` and `CARGO_CFG_TARGET_OS` set.
- `cargo run`: every lock free. `CARGO`, `OUT_DIR` and `CARGO_MANIFEST_DIR` set; `TARGET`, `HOST`, `NUM_JOBS` and `RUSTC` unset.
- `cargo test`, in the test binary: every lock free, and the same variables as `cargo run`.

So only a build script runs under the lock, and a build script is told apart from the other two by `TARGET` and `HOST`, which cargo sets for build scripts alone, beside `CARGO` and `OUT_DIR`. A `cargo install` started from a build script into the same build directory waits for a lock the build that started it releases only once the build script returns: it never returns.

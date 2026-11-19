#!/bin/sh
# Revisions of one two-crate repository installed over one build directory: by `--git`, then by `--path` from clones.
# usage: probe.sh <scratch dir> <build dir>
set -eu
s="$1"; build="$2"
mkdir -p "$s/repo/a/src" "$s/repo/b/src"
cd "$s/repo"
git init -q
printf '[workspace]\nmembers = ["a", "b"]\nresolver = "3"\n' > Cargo.toml
printf '[package]\nname = "probe-a"\nversion = "0.0.1"\nedition = "2021"\n\n[dependencies]\nprobe-b = { path = "../b" }\n' > a/Cargo.toml
printf '[package]\nname = "probe-b"\nversion = "0.0.1"\nedition = "2021"\n' > b/Cargo.toml
echo 'fn main() { println!("{}", probe_b::WORD); }' > a/src/main.rs
word() {
    echo "pub const WORD: &str = \"$1\";" > b/src/lib.rs
    git add -A .
    git -c user.email=p@p -c user.name=p -c core.hooksPath=/dev/null commit -qm "$1"
    git rev-parse HEAD
}
word one >/dev/null
cargo generate-lockfile -q --offline
git add Cargo.lock
git -c user.email=p@p -c user.name=p -c core.hooksPath=/dev/null commit -qm lock
r1=$(git rev-parse HEAD)
r2=$(word two)
r3=$(word three)
r4=$(word four)
cd "$s"
git_install() {
    CARGO_BUILD_BUILD_DIR="$build" cargo install -q --git "file://$s/repo" --rev "$1" probe-a --locked \
        --root "$s/root-$1" --force
    printf -- '--git %s prints %s\n' "$(echo "$1" | cut -c1-7)" "$("$s/root-$1/bin/probe-a")"
}
echo "== --git, one build directory"
git_install "$r1"
git_install "$r2"
echo "== the fingerprint trace of the next --git install"
CARGO_LOG=cargo::core::compiler::fingerprint=trace CARGO_BUILD_BUILD_DIR="$build" \
    cargo install --git "file://$s/repo" --rev "$r3" probe-a --locked --root "$s/root-$r3" --force 2>&1 \
    | grep -E 'up-to-date' | sed 's/^.*fingerprint: //'
printf -- '--git %s prints %s\n' "$(echo "$r3" | cut -c1-7)" "$("$s/root-$r3/bin/probe-a")"
echo "== the units of the next --git install, as cargo's messages report them"
CARGO_BUILD_BUILD_DIR="$build" cargo install -q --git "file://$s/repo" --rev "$r4" probe-a --locked \
    --root "$s/root-$r4" --force --message-format json-render-diagnostics 2>/dev/null \
    | grep -o '"package_id":"[^"]*"\|"fresh":[a-z]*' | sed 's#file:///[^"]*/checkouts/#<checkouts>/#'
printf -- '--git %s prints %s\n' "$(echo "$r4" | cut -c1-7)" "$("$s/root-$r4/bin/probe-a")"
echo "== the same four by --path from clones outside CARGO_HOME"
for r in "$r1" "$r2" "$r3" "$r4"; do
    git clone -q "$s/repo" "$s/src-$r"
    git -C "$s/src-$r" checkout -q "$r"
    CARGO_BUILD_BUILD_DIR="$build" cargo install -q --path "$s/src-$r/a" --locked --root "$s/proot-$r" --force
    printf -- '--path %s prints %s\n' "$(echo "$r" | cut -c1-7)" "$("$s/proot-$r/bin/probe-a")"
done
echo "== fingerprint directories"
ls "$build/release/.fingerprint"

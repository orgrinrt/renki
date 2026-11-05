#!/usr/bin/env bash
# Mutation check for the refused build that forgets its branch resolution. Each line of the table
# below is one mutant: the file, a perl substitution that must match exactly once, and a name. A
# mutant is killed when `cargo test` fails with the change applied. A build failure is reported
# apart, since it kills nothing. Run from the repo root, with the tree not edited meanwhile.
#
# Not in the table, because no test can tell them apart: `place` mapping a scratch-route failure
# through `Into::into` (a backend that places itself, which Cargo does, never takes that route),
# and the check for cargo on the path in `Cargo::build` (this suite runs under cargo, so the
# check passes whatever it does).
set -u
SRC=mock/crates/renki/src
killed=0; survived=0; broken=0
while IFS='|' read -r name file expr; do
  [ -z "$name" ] && continue
  f="$SRC/$file"
  cp "$f" "$f.orig"
  perl -0pi -e "s#${expr%%@@*}#${expr#*@@}#" "$f"
  if cmp -s "$f" "$f.orig"; then
    echo "NO-MATCH  $name"; broken=$((broken+1))
  else
    out=$(cargo test -p renki --lib 2>&1)
    if echo "$out" | grep -qE '^error(\[E|: could not compile)'; then
      echo "BUILD     $name"; broken=$((broken+1))
    elif echo "$out" | grep -q 'test result: FAILED'; then
      echo "killed    $name"; killed=$((killed+1))
    else
      echo "SURVIVED  $name"; survived=$((survived+1))
    fi
  fi
  cp "$f.orig" "$f"; rm "$f.orig"; touch "$f"
done <<'TABLE'
forgetting removes nothing|pin.rs|let _ = std::fs::remove_file\(branch_resolution_path\(cache_root, &pin\.url, b\)\);@@let _ = (cache_root, b);
forgetting removes another branch's resolution|pin.rs|remove_file\(branch_resolution_path\(cache_root, &pin\.url, b\)\)@@remove_file(branch_resolution_path(cache_root, &pin.url, "main"))
forgetting removes another remote's resolution|pin.rs|remove_file\(branch_resolution_path\(cache_root, &pin\.url, b\)\)@@remove_file(branch_resolution_path(cache_root, "other", b))
a refusal is never a refusal|retry.rs|matches!\(self, Self::Refused\(_\)\)@@false
everything is a refusal|retry.rs|matches!\(self, Self::Refused\(_\)\)@@true
the message is dropped from the error|retry.rs|Self::NotStarted\(message\) \| Self::Refused\(message\) => message@@Self::NotStarted(message) \| Self::Refused(message) => { let _ = message; String::new() }
a bare message is a refusal|retry.rs|fn from\(message: String\) -> Self \{\n( +)Self::NotStarted\(message\)@@fn from(message: String) -> Self {\n$1Self::Refused(message)
every attempt failing is not a refusal|retry.rs|BuildError::Refused\(message\)\n( +)\}\)@@BuildError::NotStarted(message)\n$1})
nothing to try is a refusal|retry.rs|if failures\.is_empty\(\) \{@@if false {
a refused build forgets nothing|lib.rs|if e\.is_refused\(\) \{@@if false {
a build that could not start forgets too|lib.rs|if e\.is_refused\(\) \{@@if true {
a build that worked forgets|lib.rs|Ok\(bin\) => \{\n( +)Ok\(Built \{@@Ok(bin) => {\n$1pin::forget_resolution(pin, cache_root);\n$1Ok(Built {
the key leaves the toolchain out|lib.rs|cache::compute_key\(&pin\.url, &resolved\.key_rev, &toolchain\);\n( +)match build@@cache::compute_key(&pin.url, &resolved.key_rev, "");\n$1match build
the failure message is lost on the way out|lib.rs|Err\(e\.into_message\(\)\)\n( +)\},\n( +)\}\n\}@@Err(String::new())\n$1},\n$2}\n}
TABLE
echo "killed=$killed survived=$survived other=$broken"

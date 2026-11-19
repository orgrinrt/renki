#!/bin/sh
# Which of cargo's locks under the build directory are held right now, and which of the variables cargo sets are set.
dir="$1"; shift
ls -a "$dir" | sed "s/^/  ls: /"
for f in $(find "$dir" -maxdepth 2 -name '.cargo*lock' 2>/dev/null | sort); do
    if flock -n "$f" true; then echo "free $f"; else echo "held $f"; fi
done
for v in CARGO OUT_DIR TARGET HOST NUM_JOBS RUSTC CARGO_MANIFEST_DIR CARGO_CFG_TARGET_OS CARGO_PRIMARY_PACKAGE; do
    eval "x=\${$v+set}"; echo "$v=${x:-unset}"
done

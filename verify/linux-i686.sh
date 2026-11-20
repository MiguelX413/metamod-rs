#!/usr/bin/env bash
set -euo pipefail

metamod=$(realpath "${1:?path to alliedmodders/metamod-hl1 checkout}")
hlsdk=$(realpath "${2:?path to alliedmodders/hlsdk checkout}")
cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

export CARGO_TARGET_DIR="$work/target"
cargo build --locked --release --target i686-unknown-linux-gnu -p hello-world
cargo build --locked --target i686-unknown-linux-gnu -p metamod-sys --example layout

plugin="$CARGO_TARGET_DIR/i686-unknown-linux-gnu/release/libhello_world.so"
readelf -h "$plugin" | tee "$work/elf"
grep -Eq 'Class: +ELF32' "$work/elf"
grep -Eq 'Machine: +Intel 80386' "$work/elf"
nm -D --defined-only "$plugin" | awk '$2 == "T" {print $3}' | sort > "$work/exports"
printf '%s\n' GiveFnptrsToDll Meta_Attach Meta_Detach Meta_Query | diff -u - "$work/exports"

includes=(-I"$metamod/metamod" -I"$hlsdk/dlls" -I"$hlsdk/engine"
          -I"$hlsdk/common" -I"$hlsdk/pm_shared" -I"$hlsdk/public")
"${CXX:-c++}" -m32 -std=c++11 -DLINUX -DGNUC "${includes[@]}" verify/layout.cpp -o "$work/layout"
"${CXX:-c++}" -m32 -std=c++11 -DLINUX -DGNUC "${includes[@]}" verify/load.cpp -ldl -o "$work/load"
"$work/layout" > "$work/c-layout"
"$CARGO_TARGET_DIR/i686-unknown-linux-gnu/debug/examples/layout" > "$work/rust-layout"
diff -u "$work/c-layout" "$work/rust-layout"
"$work/load" "$plugin"
echo 'ABI and loader checks passed.'

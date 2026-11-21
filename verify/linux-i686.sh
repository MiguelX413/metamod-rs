#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
metamod=$(realpath "${1:-$repo/upstream/metamod-hl1}")
hlsdk=$(realpath "${2:-$repo/upstream/hlsdk}")
cd "$repo"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

export CARGO_TARGET_DIR="$work/target"
cargo build --locked --release --target i686-unknown-linux-gnu -p hello-world -p engine-demo
cargo build --locked --target i686-unknown-linux-gnu -p metamod-sys --example layout

includes=(-I"$metamod/metamod" -I"$hlsdk/dlls" -I"$hlsdk/engine"
          -I"$hlsdk/common" -I"$hlsdk/pm_shared" -I"$hlsdk/public")
"${CXX:-c++}" -m32 -std=c++11 -DLINUX -DGNUC "${includes[@]}" verify/layout.cpp -o "$work/layout"
sanitizers=()
if [[ ${METAMOD_SANITIZE_LOADER:-0} == 1 ]]; then
    sanitizers=(-fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer)
fi
"${CXX:-c++}" -m32 -std=c++11 -DLINUX -DGNUC "${includes[@]}" "${sanitizers[@]}" verify/load.cpp -pthread -ldl -o "$work/load"
"$work/layout" > "$work/c-layout"
"$CARGO_TARGET_DIR/i686-unknown-linux-gnu/debug/examples/layout" > "$work/rust-layout"
diff -u "$work/c-layout" "$work/rust-layout"
for example in hello-world engine-demo; do
    plugin="$CARGO_TARGET_DIR/i686-unknown-linux-gnu/release/lib${example//-/_}.so"
    readelf -h "$plugin" | tee "$work/elf"
    grep -Eq 'Class: +ELF32' "$work/elf"
    grep -Eq 'Machine: +Intel 80386' "$work/elf"
    nm -D --defined-only "$plugin" | awk '$2 == "T" {print $3}' | sort > "$work/exports"
    printf '%s\n' GiveFnptrsToDll Meta_Attach Meta_Detach Meta_Query | diff -u - "$work/exports"
    if [[ ${METAMOD_SANITIZE_LOADER:-0} == 1 ]]; then
        # Native registrations intentionally stay resident until process exit.
        ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_leaks=0" "$work/load" "$plugin" "$example"
    else
        "$work/load" "$plugin" "$example"
    fi
done
echo 'ABI and loader checks passed.'

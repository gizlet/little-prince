#!/bin/sh
# scripts/package.sh [OUT]: builds The Little Prince for aarch64 Gizlet OS and
# assembles the app folder (dist/littleprince by default) and its .gzl.
#
# Needs Python 3.11+, zig, the Rust target aarch64-unknown-linux-gnu and Gizlet runtime 1's
# libraylib.so: set GIZLET_RAYLIB_LIB to the folder holding it. gizlet/os
# builds it with `bazel build --config=r36s //runtime:raylib`.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$root/dist/littleprince}
target=aarch64-unknown-linux-gnu

: "${GIZLET_RAYLIB_LIB:?set GIZLET_RAYLIB_LIB to the folder holding runtime 1 libraylib.so}"
[ -f "$GIZLET_RAYLIB_LIB/libraylib.so" ] || { echo "no libraylib.so in $GIZLET_RAYLIB_LIB" >&2; exit 1; }

# bindgen reads raylib.h for the device: give it zig's glibc headers.
zig_lib=$(zig env | sed -n -e 's/.*\.lib_dir = "\(.*\)".*/\1/p' -e 's/.*"lib_dir": *"\([^"]*\)".*/\1/p')
inc=$zig_lib/libc/include
[ -d "$inc/generic-glibc" ] || { echo "zig's glibc headers are missing under $inc" >&2; exit 1; }
export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$root/scripts/zig-cc"
export CC_aarch64_unknown_linux_gnu="$root/scripts/zig-cc"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_unknown_linux_gnu="-isystem $inc/aarch64-linux-gnu -isystem $inc/generic-glibc -isystem $inc/any-linux-any"

cd "$root"
version=$(python3 -c 'import tomllib; print(tomllib.load(open("gizlet.toml", "rb"))["version"])')
GIZLET_APP_VERSION="$version" cargo build --release --locked --target "$target" --features gizlet-os

rm -rf "$out"
mkdir -p "$out/bin" "$out/licenses"
cp "target/$target/release/littleprince" "$out/bin/littleprince"
cp LICENSE "$out/licenses/littleprince-MIT.txt"
mkdir -p "$out/assets/hero"
cp assets/hero/little-prince.glb "$out/assets/hero/"
cp assets/hero/LICENSE.txt "$out/licenses/little-prince-model-CC0.txt"
cp assets/fonts/Inter-OFL.txt "$out/licenses/inter-OFL.txt"
cp assets/fonts/SpaceGrotesk-OFL.txt "$out/licenses/space-grotesk-OFL.txt"
cp assets/fonts/DroidSansFallback-LICENSE.txt "$out/licenses/droid-sans-fallback-Apache-2.0.txt"
python3 scripts/pack.py "$out" "$target"

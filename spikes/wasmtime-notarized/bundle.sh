#!/usr/bin/env bash
# Builds a signed app bundle of the spike for one variant. The WebAssembly
# variants get the guests from build-guests.sh in Contents/Resources, the Lua
# ones get lua/*.lua:
#
#   ./bundle.sh cranelift [entitlements]   -> build/Spike-Cranelift.app
#   ./bundle.sh pulley [entitlements]      -> build/Spike-Pulley.app
#   ./bundle.sh luajit [entitlements]      -> build/Spike-LuaJIT.app
#   ./bundle.sh luajit-hrt [entitlements]  -> build/Spike-LuaJIT-HRT.app
#   ./bundle.sh luau [entitlements]        -> build/Spike-Luau.app
#   ./bundle.sh luau-jit [entitlements]    -> build/Spike-LuauJIT.app
#
# luajit-hrt builds LuaJIT with LUAJIT_ENABLE_OSX_HRT, which makes it allocate
# its machine code with MAP_JIT, in a target directory of its own.
#
# The bundle is signed with the hardened runtime and a secure timestamp. The
# default entitlements are entitlements/base.plist (no exceptions) for the
# interpreters, Pulley and Luau, and entitlements/jit.plist for the others.
#
# Environment:
#   SPIKE_SIGN_IDENTITY  signing identity (default: the project's Developer ID)
set -euo pipefail

# Resolve a given entitlements path before leaving the caller's directory.
if [ -n "${2:-}" ]; then
    set -- "$1" "$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
fi
cd "$(dirname "$0")"

variant="${1:?usage: bundle.sh cranelift|pulley|luajit|luajit-hrt|luau|luau-jit [entitlements.plist]}"
target_dir="target"
case "$variant" in
    cranelift) features=() name="Spike-Cranelift" default_entitlements="entitlements/jit.plist" ;;
    pulley) features=(--features pulley) name="Spike-Pulley" default_entitlements="entitlements/base.plist" ;;
    luajit) features=(--features luajit) name="Spike-LuaJIT" default_entitlements="entitlements/jit.plist" ;;
    luajit-hrt)
        features=(--features luajit) name="Spike-LuaJIT-HRT" default_entitlements="entitlements/jit.plist"
        # luajit-src hands the C flags of the cc crate to LuaJIT's makefile.
        # In the vendored LuaJIT, the MAP_JIT branch of mcode_setprot returns
        # a value from a void function, which clang rejects by default.
        export CFLAGS_aarch64_apple_darwin="-DLUAJIT_ENABLE_OSX_HRT -Wno-return-mismatch"
        target_dir="target/luajit-hrt"
        ;;
    luau) features=(--features luau) name="Spike-Luau" default_entitlements="entitlements/base.plist" ;;
    luau-jit) features=(--features luau-jit) name="Spike-LuauJIT" default_entitlements="entitlements/jit.plist" ;;
    *) echo "unknown variant: $variant" >&2 && exit 2 ;;
esac
entitlements="${2:-$default_entitlements}"
identity="${SPIKE_SIGN_IDENTITY:-Developer ID Application: The Maintainer (XXXXXXXXXX)}"
app="build/$name.app"

# `${features[@]+...}` keeps an empty array safe under `set -u` in bash 3.2.
cargo build --release --locked -p spike --target-dir "$target_dir" ${features[@]+"${features[@]}"}
case "$variant" in
    luajit* | luau*) resources=(lua/*.lua) ;;
    *) ./build-guests.sh > /dev/null && resources=(build/guests/*.wasm) ;;
esac

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp Info.plist "$app/Contents/Info.plist"
plutil -replace CFBundleName -string "$name" "$app/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "io.github.sephiroth74.saetta.spike.$variant" "$app/Contents/Info.plist"
cp "$target_dir/release/spike" "$app/Contents/MacOS/spike"
cp "${resources[@]}" "$app/Contents/Resources/"

codesign --force --options runtime --timestamp \
    --entitlements "$entitlements" \
    --sign "$identity" \
    "$app"
codesign --verify --strict --verbose=2 "$app"

echo "Signed $app with $entitlements"

#!/usr/bin/env bash
# Builds a signed app bundle of the spike for one variant, with the guests
# from build-guests.sh in Contents/Resources:
#
#   ./bundle.sh cranelift [entitlements]  -> build/Spike-Cranelift.app
#   ./bundle.sh pulley [entitlements]     -> build/Spike-Pulley.app
#
# The bundle is signed with the hardened runtime and a secure timestamp. The
# default entitlements are entitlements/jit.plist for Cranelift and
# entitlements/base.plist (no exceptions) for Pulley.
#
# Environment:
#   SPIKE_SIGN_IDENTITY  signing identity (default: the project's Developer ID)
set -euo pipefail

# Resolve a given entitlements path before leaving the caller's directory.
if [ -n "${2:-}" ]; then
    set -- "$1" "$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
fi
cd "$(dirname "$0")"

variant="${1:?usage: bundle.sh cranelift|pulley [entitlements.plist]}"
case "$variant" in
    cranelift) features=() name="Spike-Cranelift" default_entitlements="entitlements/jit.plist" ;;
    pulley) features=(--features pulley) name="Spike-Pulley" default_entitlements="entitlements/base.plist" ;;
    *) echo "unknown variant: $variant" >&2 && exit 2 ;;
esac
entitlements="${2:-$default_entitlements}"
identity="${SPIKE_SIGN_IDENTITY:-Developer ID Application: The Maintainer (XXXXXXXXXX)}"
app="build/$name.app"

# `${features[@]+...}` keeps an empty array safe under `set -u` in bash 3.2.
cargo build --release --locked -p spike ${features[@]+"${features[@]}"}
./build-guests.sh > /dev/null

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp Info.plist "$app/Contents/Info.plist"
plutil -replace CFBundleName -string "$name" "$app/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "io.github.sephiroth74.saetta.spike.$variant" "$app/Contents/Info.plist"
cp target/release/spike "$app/Contents/MacOS/spike"
cp build/guests/*.wasm "$app/Contents/Resources/"

codesign --force --options runtime --timestamp \
    --entitlements "$entitlements" \
    --sign "$identity" \
    "$app"
codesign --verify --strict --verbose=2 "$app"

echo "Signed $app with $entitlements"

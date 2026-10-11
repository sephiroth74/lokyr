#!/usr/bin/env bash
# Builds build/Spike.app from the release binary and signs it with the
# hardened runtime and a secure timestamp.
#
# Environment:
#   SPIKE_SIGN_IDENTITY  signing identity (default: the project's Developer ID)
#   SPIKE_ENTITLEMENTS   entitlements file (default: entitlements/base.plist)
set -euo pipefail

cd "$(dirname "$0")"

identity="${SPIKE_SIGN_IDENTITY:-Developer ID Application: The Maintainer (XXXXXXXXXX)}"
entitlements="${SPIKE_ENTITLEMENTS:-entitlements/base.plist}"
app="build/Spike.app"

cargo build --release --locked

rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp Info.plist "$app/Contents/Info.plist"
cp target/release/spike "$app/Contents/MacOS/spike"

codesign --force --options runtime --timestamp \
    --entitlements "$entitlements" \
    --sign "$identity" \
    "$app"
codesign --verify --strict --verbose=2 "$app"

echo "Signed $app with $entitlements"

#!/bin/sh
# Signs copies of the measures' binary into build/, each of a signature:
# `plain`, ad hoc of no hardened runtime; `hr`, the hardened runtime alone;
# `hr-jit`, of `allow-jit`, as Lokyr's; `hr-unsigned`, of
# `allow-unsigned-executable-memory`; `hr-both`, of both. The identity of
# $LOKYR_SIGN_IDENTITY, or ad hoc.
set -eu
cd "$(dirname "$0")"
ID=${LOKYR_SIGN_IDENTITY:--}
mkdir -p build
plist() {
  printf '<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">\n<dict>\n'
  for key in "$@"; do printf '\t<key>%s</key>\n\t<true/>\n' "$key"; done
  printf '</dict>\n</plist>\n'
}
plist > build/none.plist
plist com.apple.security.cs.allow-jit > build/jit.plist
plist com.apple.security.cs.allow-unsigned-executable-memory > build/unsigned.plist
plist com.apple.security.cs.allow-jit com.apple.security.cs.allow-unsigned-executable-memory > build/both.plist
for variant in plain hr hr-jit hr-unsigned hr-both; do
  cp target/release/jit-entitlement build/$variant
done
codesign --force --sign - build/plain
codesign --force --options runtime --sign "$ID" --entitlements build/none.plist build/hr
codesign --force --options runtime --sign "$ID" --entitlements build/jit.plist build/hr-jit
codesign --force --options runtime --sign "$ID" --entitlements build/unsigned.plist build/hr-unsigned
codesign --force --options runtime --sign "$ID" --entitlements build/both.plist build/hr-both

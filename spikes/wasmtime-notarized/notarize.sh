#!/usr/bin/env bash
# Notarizes a signed bundle with Apple and staples the ticket to it:
#
#   ./notarize.sh build/Spike-Pulley.app
#
# If Apple rejects the bundle, the script prints the notarization log.
#
# Environment:
#   SPIKE_NOTARY_PROFILE  notarytool keychain profile (default: lokyr-notary)
set -euo pipefail

bundle="${1:?usage: notarize.sh <bundle.app>}"
app="$(cd "$(dirname "$bundle")" && pwd)/$(basename "$bundle")"
profile="${SPIKE_NOTARY_PROFILE:-lokyr-notary}"
zip="${app%.app}.zip"

rm -f "$zip"
ditto -c -k --keepParent "$app" "$zip"

result="$(xcrun notarytool submit "$zip" --keychain-profile "$profile" --wait --output-format json)"
rm -f "$zip"
id="$(printf '%s' "$result" | plutil -extract id raw -)"
status="$(printf '%s' "$result" | plutil -extract status raw -)"
echo "Submission $id: $status"

if [ "$status" != "Accepted" ]; then
    xcrun notarytool log "$id" --keychain-profile "$profile"
    exit 1
fi

xcrun stapler staple "$app"
xcrun stapler validate "$app"
spctl --assess --type execute -vv "$app"

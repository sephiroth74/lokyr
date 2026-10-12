#!/usr/bin/env bash
# Signs each Lua variant with each set of entitlements, then runs its
# workloads under the hardened runtime with the JIT on. For each combination
# it prints how the process ended and how long the byte-loop fuzzy search
# took: about 0.6 ms with LuaJIT's JIT and 3 ms with Luau's, about 15 ms when
# the code runs in the interpreter.
#
#   ./check-lua-hardened.sh
set -uo pipefail

cd "$(dirname "$0")"
matrix=(
    "luajit base"
    "luajit jit"
    "luajit unsigned-memory"
    "luajit-hrt base"
    "luajit-hrt jit"
    "luau base"
    "luau-jit base"
    "luau-jit jit"
)
for entry in "${matrix[@]}"; do
    read -r variant entitlements <<< "$entry"
    case "$variant" in
        luajit) name="Spike-LuaJIT" ;;
        luajit-hrt) name="Spike-LuaJIT-HRT" ;;
        luau) name="Spike-Luau" ;;
        luau-jit) name="Spike-LuauJIT" ;;
    esac
    app="build/$name.app"
    if ! ./bundle.sh "$variant" "entitlements/$entitlements.plist" > "build/$name.log" 2>&1; then
        echo "$variant, $entitlements.plist: the bundle failed, see build/$name.log"
        continue
    fi
    output="$("$app/Contents/MacOS/spike" --lua-workloads "$app/Contents/Resources/workloads.lua" 2>&1)"
    status=$?
    jit="$(printf '%s' "$output" | sed -n 's/.*"jit_active": \([a-z]*\).*/\1/p')"
    fuzzy="$(printf '%s' "$output" | sed -n 's/.*"fuzzy_us": \([0-9]*\).*/\1/p')"
    if [ "$status" -eq 0 ]; then
        echo "$variant, $entitlements.plist: exit 0, JIT on: $jit, fuzzy search: $fuzzy µs"
    else
        echo "$variant, $entitlements.plist: exit $status, last output: $(printf '%s' "$output" | tail -1)"
    fi
done

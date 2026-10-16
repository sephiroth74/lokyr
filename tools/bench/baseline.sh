#!/usr/bin/env bash
# Measures one terminal with bolt-bench, and writes its JSON reports to
# <out-dir>/<name>-{startup,memory,idle}.json:
#
#   tools/bench/baseline.sh <app> <name> <out-dir>
#
# The app must not be running. The startup probe launches it 12 times (2
# warmup runs and 10 timed ones). The script then launches it once more,
# leaves it idle for 10 s, samples its memory for 10 s and counts its wakeups
# for 60 s, then quits it. Every launch brings the app to the front, so keep
# away from the keyboard until the script has finished.
set -euo pipefail

app="${1:?usage: baseline.sh <app> <name> <out-dir>}"
name="${2:?usage: baseline.sh <app> <name> <out-dir>}"
out="${3:?usage: baseline.sh <app> <name> <out-dir>}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
bench="$root/target/release/bolt-bench"

(cd "$root" && cargo build --quiet --release --locked -p bolt-bench)
mkdir -p "$out"
executable="$app/Contents/MacOS/$(plutil -extract CFBundleExecutable raw -o - "$app/Contents/Info.plist")"

echo "$name: startup" >&2
"$bench" startup --app "$app" > "$out/$name-startup.json"

echo "$name: memory and idle" >&2
open -a "$app"
pid=""
for _ in $(seq 100); do
    # LaunchServices starts apps with the full path of their executable.
    pid="$(pgrep -n -f "^$executable" || true)"
    [ -n "$pid" ] && break
    sleep 0.1
done
if [ -z "$pid" ]; then
    echo "error: $name did not start" >&2
    exit 1
fi
trap 'kill -TERM "$pid" 2> /dev/null || true' EXIT
sleep 10
"$bench" memory --pid "$pid" --duration 9 > "$out/$name-memory.json"
"$bench" idle --pid "$pid" --duration 60 > "$out/$name-idle.json"
kill -TERM "$pid"
trap - EXIT
while kill -0 "$pid" 2> /dev/null; do
    sleep 0.1
done
echo "$name: done" >&2

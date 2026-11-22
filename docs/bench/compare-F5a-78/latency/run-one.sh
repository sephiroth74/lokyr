#!/bin/sh
# Typometer's runs of one terminal, alone and next to a flood, as
# `typo-<name>-60` and `typo-<name>-60-flood`, of the terminal whose main
# executable is `$2`, open at its prompt as `compare.py open` opens it:
# Typometer 1.1.0's own benchmark, driven by tools/bench/typometer's
# Harness of its jar, of its settings, 200 keys 150 ms apart, no pauses,
# its native API off and its synchronous mode. The terminal's window goes
# to the screen's top left, by its titlebar, and runs `cat`, whose echo is
# the line discipline's, as Typometer never presses Return; for the flood,
# a second window of it runs `yes` at the screen's right. Leave the mouse
# and the keyboard alone meanwhile.
#
#     javac -cp "$JAR" -d /tmp/lokyr/typo tools/bench/typometer/Harness.java
#     sh docs/bench/compare-F5a-78/latency/run-one.sh kitty /Applications/kitty.app/Contents/MacOS/kitty
name=$1 exe=$2
REPO=$(cd "$(dirname "$0")/../../../.." && pwd)
OUT=${OUT:-/tmp/lokyr/typo/out}
JAR=/Applications/Typometer.app/Contents/app/typometer-1.1.0.jar
H="java -Dapple.awt.UIElement=true -cp $JAR:/tmp/lokyr/typo Harness"
BENCH=$REPO/target/release/lokyr-bench
mkdir -p "$OUT"
log() { echo "$(date +%T) $name: $*"; }
pid=$(pgrep -f "^$exe" | head -1)
[ -n "$pid" ] || { log "not running"; exit 1; }
"$BENCH" motion --pid "$pid" --kind move --to 80,80 > /dev/null || log "move failed"
sleep 1.5
# Letters alone, which every keyboard's layout types alike.
$H type "cat
"
sleep 1.5
log "alone"
$H bench "typo-$name-60" "$OUT/$name-60.csv"
sleep 1
$H key cmd+n
sleep 2.5
$H type "yes
"
sleep 1
"$BENCH" motion --pid "$pid" --kind move --to 1420,80 > /dev/null || log "flood move failed"
sleep 1
# The first window's titlebar, to give it the keys back.
$H click 300 92
sleep 1.5
log "next to a flood"
$H bench "typo-$name-60-flood" "$OUT/$name-60-flood.csv"
log "done"

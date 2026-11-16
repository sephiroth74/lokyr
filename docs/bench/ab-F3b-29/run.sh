#!/bin/sh
# The A/B of 1.2.0: 1.1.0's app, of its release's DMG, against 1.2.0's,
# of its bundle, in one session of balanced orders, as 1.1.0's: each copy
# started 30 times before the measured rounds, as an executable that the
# system has seen little of starts slower; the startup in four rounds,
# AB BA BA AB, each of
# the minimal AppKit app of F0 as its floor; the latency in eight, after
# a warm-up of each; idle and memory in two; each throughput's workload in
# two. The same pause before each run of the startup and of the latency,
# as an uneven settle moves their results with the order. The config is
# config.toml's, of the least work beside the terminal's own: no blinking,
# no ligatures, an opaque window, no quick tip nor windows restored.
set -u
OUT=/tmp/lokyr/bench-F3b-29
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
HERE=$REPO/docs/bench/ab-F3b-29
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v110) echo /tmp/lokyr/bench-F3b-29/v110/Lokyr.app ;;
    v120) echo /tmp/lokyr/bench-F3b-29/v120/Lokyr.app ;;
  esac
}
# The order of round `$1` of the pair `$2 $3`: AB, BA, BA, AB, ...
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $HERE/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
# Each copy started 30 times, its window drawn, before any measure, as a
# binary that the system has seen little of starts some 10 ms slower.
for v in v110 v120; do
  log "first starts $v"
  for i in $(seq 1 30); do
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench startup > /dev/null 2>&1
  done
done
startup() {
  sleep 2
  log "startup $1 $2"
  $BENCH startup --app "$(app $1)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$1-$2.json
}
latency() {
  sleep 2
  log "latency $1 $2 place $3"
  XDG_CONFIG_HOME=$CFG "$(app $1)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$1-$2.json
}
for round in 1 2 3 4; do
  for v in $(order $round v110 v120); do startup $v $round; done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  load
done
for v in v110 v120; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round v110 v120); do place=$((place + 1)); latency $v $round $place; done
  load
done
# Idle over 60 s and memory over 10 s of the app `$2` started of
# `$3`, the measure's name `$1`, 20 s after its start, and its threads.
rest() {
  name=$1 bundle=$2
  shift 2
  log "idle $name"
  before=$(pgrep -f "$bundle/Contents/MacOS/lokyr")
  open -n "$@" "$bundle"
  sleep 3
  pid=$(pgrep -nf "$bundle/Contents/MacOS/lokyr")
  [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $name"; return; }
  $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$name.json
  sleep 18
  ps -M -p $pid | tail -n +2 | wc -l | tr -d ' ' > $OUT/threads-$name.txt
  $BENCH idle --pid $pid --duration 60 > $OUT/idle-$name.json
  $BENCH memory --pid $pid --duration 10 > $OUT/memory-$name.json
  kill $pid; sleep 2
}
for round in 1 2; do
  for v in $(order $round v110 v120); do
    rest $v-$round "$(app $v)" --env XDG_CONFIG_HOME=$CFG
  done
  load
done
for file in ascii seq sgr cjk; do
  for round in 1 2; do
    for v in $(order $round v110 v120); do
      log "throughput $file $v $round"
      rm -f $OUT/onscreen-$file-$v-$round.jsonl
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 > $OUT/frames-$file-$v-$round.json
    done
  done
  load
done
log done

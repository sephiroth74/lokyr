#!/bin/sh
# The A/B of 0.4.0's app and 0.5.0's, each the app of its release's DMG, in
# one session, of balanced orders: each measure's rounds put each bundle
# first as often, startup, idle and memory, and each throughput's workload
# in two rounds, one of each order; the latency in eight rounds, AB BA BA
# AB AB BA BA AB, after a warm-up of each. Both load Sparkle once their
# first window opens, as the idle and memory's measures, which start them
# with `open`, show.
set -u
OUT=/tmp/lokyr/bench-F3-40
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v040) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
    v050) echo /tmp/lokyr/bench-v050/Lokyr.app ;;
  esac
}
# The order of round `$1`, of `$2` rounds' pattern: AB, BA, BA, AB, ...
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v040 v050" ;;
    *) echo "v050 v040" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-40/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
for round in 1 2; do
  for v in $(order $round); do
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
done
for v in v040 v050; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round); do
    place=$((place + 1))
    log "latency $v $round place $place"
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$v-$round.json
  done
done
for round in 1 2; do
  for v in $(order $round); do
    log "idle $v $round"
    before=$(pgrep -f "$(app $v)/Contents/MacOS/lokyr")
    open -n --env XDG_CONFIG_HOME=$CFG "$(app $v)"
    sleep 3
    pid=$(pgrep -nf "$(app $v)/Contents/MacOS/lokyr")
    [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $v"; continue; }
    $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$v-$round.json
    sleep 18
    $BENCH idle --pid $pid --duration 60 > $OUT/idle-$v-$round.json
    $BENCH memory --pid $pid --duration 10 > $OUT/memory-$v-$round.json
    kill $pid; sleep 2
  done
done
for file in ascii seq sgr cjk; do
  for round in 1 2; do
    for v in $(order $round); do
      log "throughput $file $v $round"
      rm -f $OUT/onscreen-$file-$v-$round.jsonl
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 > $OUT/frames-$file-$v-$round.json
    done
  done
done
log done

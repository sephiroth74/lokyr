#!/bin/sh
# The A/B of 0.2.0's bundle and 0.3.0's, the app of its DMG, in one session,
# each measure in turns, 0.2.0 then 0.3.0, twice: startup, latency, idle and
# memory, throughput. 0.3.0 loads Sparkle once its first window opens, as
# the idle and memory's measures, which start it with `open`, show.
set -u
OUT=/tmp/lokyr/bench-F3-83
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v020) echo /tmp/lokyr/bench-v020/target/Lokyr.app ;;
    v030) echo /tmp/lokyr/bench-v030/Lokyr.app ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-83/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
for round in 1 2; do
  for v in v020 v030; do
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
done
for round in 1 2; do
  for v in v020 v030; do
    log "latency $v $round"
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$v-$round.json
  done
done
for round in 1 2; do
  for v in v020 v030; do
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
  for v in v020 v030; do
    log "throughput $file $v"
    rm -f $OUT/onscreen-$file-$v.jsonl
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
      python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v.jsonl 3 > $OUT/frames-$file-$v.json
  done
done
log done

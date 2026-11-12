#!/bin/sh
# The A/B of 0.8.0: 0.7.0's app against 0.8.0's, each of its release's
# DMG, notarized and stapled, in one session of balanced orders, as
# F3-77's: each copy started once before the measured rounds, as a scan
# of a new executable slows its first starts; the startup in four rounds,
# AB BA BA AB, each of the minimal AppKit app of F0 as its floor; the
# latency in eight, after a warm-up of each; idle and memory in two; each
# throughput's workload in two. Then the footprint of Luau's plugins:
# 0.8.0 of a home of no plugin against one of ten plugins active from its
# start, idle, memory and threads, in two rounds. The same pause before
# each run of the startup and of the latency, as an uneven settle moves
# their results with the order.
set -u
OUT=/tmp/lokyr/bench-F4-25
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v070) echo /tmp/lokyr/bench-v070/Lokyr.app ;;
    v080) echo /tmp/lokyr/bench-v080/Lokyr.app ;;
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
cp $REPO/docs/bench/ab-F4-25/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
# The homes of the plugins' footprint, of 0.8.0's Sleip.
$REPO/docs/bench/ab-F4-25/plugins.sh "$(app v080)" $OUT >> $OUT/session.log 2>&1
# Each copy started once, its window drawn, before any measure.
for v in v070 v080; do
  log "first start $v"
  $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > /dev/null
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
  for v in $(order $round v070 v080); do startup $v $round; done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  load
done
for v in v070 v080; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round v070 v080); do place=$((place + 1)); latency $v $round $place; done
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
  for v in $(order $round v070 v080); do
    rest $v-$round "$(app $v)" --env XDG_CONFIG_HOME=$CFG
  done
  load
done
for file in ascii seq sgr cjk; do
  for round in 1 2; do
    for v in $(order $round v070 v080); do
      log "throughput $file $v $round"
      rm -f $OUT/onscreen-$file-$v-$round.jsonl
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 > $OUT/frames-$file-$v-$round.json
    done
  done
  load
done
# The plugins' footprint: a home of no plugin against one of ten.
for round in 1 2; do
  for count in $(order $round 0 10); do
    home=$OUT/home-$count
    rest plugins-$count-$round "$(app v080)" --env HOME=$home --env XDG_CONFIG_HOME=$home/.config
  done
  load
done
grep -h "" $OUT/home-10/Library/Logs/Lokyr/lokyr.log >> $OUT/session.log 2>/dev/null
log done

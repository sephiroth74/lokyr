#!/bin/sh
# The A/B of 0.7.0: 0.5.1's app against 0.7.0's, each of its release's
# DMG, notarized and stapled, in one session of balanced orders, as F3-40's,
# the session after F3-49 and F3-190's: the startup in four rounds, AB BA BA AB,
# each of the minimal AppKit app of F0 as its floor; the latency in eight,
# after a warm-up of each; idle and memory in two; each throughput's
# workload in two. The same pause before each run of the startup and of
# the latency, as an uneven settle moves their results with the order.
set -u
OUT=/tmp/lokyr/bench-F3-77
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v051) echo /tmp/lokyr/bench-v051/Lokyr.app ;;
    v070) echo /tmp/lokyr/bench-v070/Lokyr.app ;;
  esac
}
config() {
  echo $CFG
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
cp $REPO/docs/bench/ab-F3-77/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
startup() {
  sleep 2
  log "startup $1 $2"
  $BENCH startup --app "$(app $1)" --first-frame --env XDG_CONFIG_HOME=$(config $1) > $OUT/startup-$1-$2.json
}
latency() {
  sleep 2
  log "latency $1 $2 place $3"
  XDG_CONFIG_HOME=$(config $1) "$(app $1)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$1-$2.json
}
# The A/B of 0.5.1 and 0.7.0.
for round in 1 2 3 4; do
  for v in $(order $round v051 v070); do startup $v $round; done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  load
done
for v in v051 v070; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$(config $v) "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round v051 v070); do place=$((place + 1)); latency $v $round $place; done
  load
done
for round in 1 2; do
  for v in $(order $round v051 v070); do
    log "idle $v $round"
    before=$(pgrep -f "$(app $v)/Contents/MacOS/lokyr")
    open -n --env XDG_CONFIG_HOME=$(config $v) "$(app $v)"
    sleep 3
    pid=$(pgrep -nf "$(app $v)/Contents/MacOS/lokyr")
    [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $v"; continue; }
    $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$v-$round.json
    sleep 18
    $BENCH idle --pid $pid --duration 60 > $OUT/idle-$v-$round.json
    $BENCH memory --pid $pid --duration 10 > $OUT/memory-$v-$round.json
    kill $pid; sleep 2
  done
  load
done
for file in ascii seq sgr cjk; do
  for round in 1 2; do
    for v in $(order $round v051 v070); do
      log "throughput $file $v $round"
      rm -f $OUT/onscreen-$file-$v-$round.jsonl
      XDG_CONFIG_HOME=$(config $v) "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 > $OUT/frames-$file-$v-$round.json
    done
  done
  load
done
log done

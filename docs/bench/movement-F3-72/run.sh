#!/bin/sh
# The cursor's movement's cost: one build, of `[cursor] movement`
# "instant" against "snappy", in one session of balanced orders, as the
# A/Bs': the latency of 200 keys in eight rounds, AB BA BA AB, after a
# warm-up of each, the same pause before each run; idle and memory in two.
# The latency's program echoes each key, so that each moves the cursor a
# cell, which slides under "snappy".
set -u
OUT=/tmp/lokyr/bench-F3-72
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
APP=/tmp/lokyr/bench-F3-72-app/Lokyr.app
config() { echo $OUT/config-$1; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
mkdir -p $OUT
for v in instant snappy; do
  mkdir -p $(config $v)/lokyr
  cp $REPO/docs/bench/movement-F3-72/config-$v.toml $(config $v)/lokyr/config.toml
done
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
latency() {
  sleep 2
  log "latency $1 $2 place $3"
  XDG_CONFIG_HOME=$(config $1) "$APP/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$1-$2.json
}
for v in instant snappy; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$(config $v) "$APP/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round instant snappy); do place=$((place + 1)); latency $v $round $place; done
  load
done
for round in 1 2; do
  for v in $(order $round instant snappy); do
    log "idle $v $round"
    before=$(pgrep -f "$APP/Contents/MacOS/lokyr")
    open -n --env XDG_CONFIG_HOME=$(config $v) "$APP"
    sleep 3
    pid=$(pgrep -nf "$APP/Contents/MacOS/lokyr")
    [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $v"; continue; }
    $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$v-$round.json
    sleep 18
    $BENCH idle --pid $pid --duration 60 > $OUT/idle-$v-$round.json
    $BENCH memory --pid $pid --duration 10 > $OUT/memory-$v-$round.json
    kill $pid; sleep 2
  done
  load
done
log done

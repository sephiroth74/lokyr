#!/bin/sh
# The A/B of 1.2.0 again, of its two noisy measures: the startup, of
# copies started 30 times more each, in eight rounds of balanced orders,
# each of the minimal AppKit app of F0 as its floor; and the memory at
# rest, 20 s after a start, over 10 s, in four rounds, with vmmap's
# summary of each. The same config and pauses as run.sh's.
set -u
OUT=/tmp/lokyr/bench-F3b-29/again
CFG=/tmp/lokyr/bench-F3b-29/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() { echo /tmp/lokyr/bench-F3b-29/$1/Lokyr.app; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
mkdir -p $OUT
for v in v110 v120; do
  log "more starts $v"
  for i in $(seq 1 30); do
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench startup > /dev/null 2>&1
  done
done
for round in 1 2 3 4 5 6 7 8; do
  for v in $(order $round v110 v120); do
    sleep 2
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  load
done
memory() {
  name=$1 bundle=$2
  log "memory $name"
  before=$(pgrep -f "$bundle/Contents/MacOS/lokyr")
  open -n --env XDG_CONFIG_HOME=$CFG "$bundle"
  sleep 3
  pid=$(pgrep -nf "$bundle/Contents/MacOS/lokyr")
  [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $name"; return; }
  sleep 17
  $BENCH memory --pid $pid --duration 10 > $OUT/memory-$name.json
  vmmap --summary $pid > $OUT/vmmap-$name.txt 2>/dev/null
  kill $pid; sleep 2
}
for round in 1 2 3 4; do
  for v in $(order $round v110 v120); do memory $v-$round "$(app $v)"; done
  load
done
log done

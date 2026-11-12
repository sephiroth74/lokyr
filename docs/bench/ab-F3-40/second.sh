#!/bin/sh
# The second session of F3-40's A/B, of the latency's tail and the
# startup, which the first, of a load average of 4 to 6.5, left
# undecided: four rounds of startup, AB BA BA AB, each of the minimal
# app's floor; eight rounds of latency, AB BA BA AB AB BA BA AB, after a
# warm-up of each; the load average told before each part.
set -u
OUT=/tmp/lokyr/bench-F3-40b
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    v040) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
    v050) echo /tmp/lokyr/bench-v050/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v040 v050" ;;
    *) echo "v050 v040" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-40/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
for round in 1 2 3 4; do
  for v in $(order $round); do
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
done
uptime >> $OUT/session.log
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
  uptime >> $OUT/session.log
done
log done

#!/bin/sh
# The startup of the same four bundles, bisected: four rounds of a Williams
# design, each of the minimal app's floor, as F3-40's second session found
# 0.5.0's first frame a refresh later than 0.4.0's in two rounds of four.
set -u
OUT=/tmp/lokyr/bench-F3-163
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    o) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
    k) echo /tmp/lokyr/bench-F3-135b/h/Lokyr.app ;;
    m) echo $OUT/m/Lokyr.app ;;
    h) echo /tmp/lokyr/bench-v050/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0) echo "o k h m" ;;
    1) echo "k m o h" ;;
    2) echo "m h k o" ;;
    3) echo "h o m k" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
for round in 1 2 3 4; do
  for v in $(order $round); do
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  uptime >> $OUT/session.log
done
log done

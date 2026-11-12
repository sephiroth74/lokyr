#!/bin/sh
# The startup bisected further, within 0.4.0 to 0acf7fd, where the first
# bisection found the refresh more: `o`, 0.4.0; `m2`, 8661a20, the slots in
# the windows; `m3`, 7ae3d41, the live resize's pace and a new size's wait
# for its own drawing on it; `k`, 0acf7fd. Four rounds of a Williams design,
# each of the minimal app's floor.
set -u
OUT=/tmp/lokyr/bench-F3-163/startup2
CFG=/tmp/lokyr/bench-F3-163/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    o) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
    k) echo /tmp/lokyr/bench-F3-135b/h/Lokyr.app ;;
    m2) echo /tmp/lokyr/bench-F3-135c/m2/Lokyr.app ;;
    m3) echo /tmp/lokyr/bench-F3-135c/m3/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0) echo "o m2 k m3" ;;
    1) echo "m2 m3 o k" ;;
    2) echo "m3 k m2 o" ;;
    3) echo "k o m3 m2" ;;
  esac
}
mkdir -p $OUT
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

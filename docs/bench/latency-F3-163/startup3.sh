#!/bin/sh
# The startup of the fix: `o`, 0.4.0's DMG; `h0`, the head as a local
# release, which finds the interface's face as the app starts, as F3-137
# made it; `l`, the same head finding it as its first text needs it. Six
# rounds of a Williams design of three bundles, each of the minimal app's
# floor.
set -u
OUT=/tmp/lokyr/bench-F3-163/startup3
CFG=/tmp/lokyr/bench-F3-163/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    o) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
    h0) echo /tmp/lokyr/bench-F3-163/h0/Lokyr.app ;;
    l) echo /tmp/lokyr/bench-F3-163/l/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 6 )) in
    0) echo "o h0 l" ;;
    1) echo "h0 l o" ;;
    2) echo "l o h0" ;;
    3) echo "l h0 o" ;;
    4) echo "o l h0" ;;
    5) echo "h0 o l" ;;
  esac
}
mkdir -p $OUT
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
for v in o h0 l; do
  $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > /dev/null
done
for round in 1 2 3 4 5 6; do
  for v in $(order $round); do
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  uptime >> $OUT/session.log
done
log done

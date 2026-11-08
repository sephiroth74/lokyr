#!/bin/sh
# The start of 0.5.1's app, of 0.7.0's and of the build that asks for the
# default terminal a second after it, in four rounds of rotated orders,
# each of the minimal AppKit app of F0 as its floor, of 0.7.0's A/B's
# config, the same pause before each run.
set -u
OUT=/tmp/lokyr/startup-F3-227
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    v051) echo /tmp/lokyr/bench-v051/Lokyr.app ;;
    v070) echo /tmp/lokyr/bench-v070/Lokyr.app ;;
    fix) echo /tmp/lokyr/bench-fix/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0) echo "v051 v070 fix" ;;
    1) echo "fix v070 v051" ;;
    2) echo "v070 fix v051" ;;
    *) echo "v051 fix v070" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-77/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
for round in 1 2 3 4; do
  for v in $(order $round); do
    sleep 2
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
done
log done

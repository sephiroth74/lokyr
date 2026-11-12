#!/bin/sh
# The start at rest: 0.5.1's app and 0.7.0's, both of their DMGs,
# notarized, and two builds of the same commit and signature, one whose
# look at the default terminal waits a second, so that its thread is out
# of the start's way. A round of each first, unmeasured, so that every
# copy has started before, as an antivirus's scans of a new executable
# take the first starts; then four rounds of rotated orders, each of the
# minimal AppKit app of F0 as its floor, of 0.7.0's A/B's config, the same
# pause before each run.
set -u
OUT=/tmp/lokyr/startup-F3-228
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() { echo /tmp/lokyr/bench-$1/Lokyr.app; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0) echo "v051 v070 head delay" ;;
    1) echo "delay head v070 v051" ;;
    2) echo "v070 delay v051 head" ;;
    *) echo "head v051 delay v070" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-77/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
for v in v051 v070 head delay; do
  log "warm-up $v"
  $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > /dev/null
done
for round in 1 2 3 4; do
  for v in $(order $round); do
    sleep 2
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log
done
uptime >> $OUT/session.log
log done

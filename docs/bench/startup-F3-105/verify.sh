#!/bin/sh
# The fix's check, in one session, in turns, three times: A, 0.2.0's; C,
# 0.3.0's code without Sparkle and the crates' licenses, as run.sh's; E,
# the code fixed, whose plugins' turns ask for frames only after a write,
# in its whole bundle, signed; and the minimal AppKit app, the floor.
set -u
OUT=/tmp/lokyr/bench-F3-105-fixed
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    A) echo /tmp/lokyr/bench-v020/target/Lokyr.app ;;
    B) echo /tmp/lokyr/bench-v030/Lokyr.app ;;
    C) echo /tmp/lokyr/f3-105/C/Lokyr.app ;;
    D) echo /tmp/lokyr/f3-105/D/Lokyr.app ;;
    E) echo /tmp/lokyr/f3-105/E/Lokyr.app ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/startup-F3-105/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
for round in 1 2 3; do
  for v in A C E; do
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
done
log done

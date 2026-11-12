#!/bin/sh
# Which part of the single tab's title costs the start and the latency: the
# titlebar of no title against the system's, and the title's view against
# none, of an experimental build that forces the titlebar,
# LOKYR_EXPERIMENT_TITLEBAR. Four variants: T0 the system's titlebar and no
# view (no tabs plugin), T1 no title and no view, T2 the system's titlebar
# over the title's view, T3 no title and the view (as shipped). The latency
# in four rounds of a Latin square, each variant once at each place; the
# startup in two rounds.
set -u
OUT=/tmp/lokyr/bench-F3-49/factors
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
APP=/tmp/lokyr/bench-exp/Lokyr.app
CFG=/tmp/lokyr/bench-F3-49/config
NOTABS=/tmp/lokyr/bench-F3-49/config-notabs
mkdir -p $OUT
cfg() { case $1 in T0|T1) echo $NOTABS ;; *) echo $CFG ;; esac; }
force() { case $1 in T1) echo untitled ;; T2) echo titled ;; *) echo none ;; esac; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
uptime >> $OUT/session.log
for v in T0 T1 T2 T3; do
  XDG_CONFIG_HOME=$(cfg $v) LOKYR_EXPERIMENT_TITLEBAR=$(force $v) $APP/Contents/MacOS/lokyr --bench latency --keys 20 > /dev/null 2>&1
done
round=0
for order in "T0 T1 T2 T3" "T1 T3 T0 T2" "T2 T0 T3 T1" "T3 T2 T1 T0"; do
  round=$((round + 1))
  for v in $order; do
    log "latency $v $round"
    XDG_CONFIG_HOME=$(cfg $v) LOKYR_EXPERIMENT_TITLEBAR=$(force $v) $APP/Contents/MacOS/lokyr --bench latency --keys 200 2>/dev/null > $OUT/latency-$v-$round.json
  done
  echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log
done
round=0
for order in "T0 T1 T2 T3" "T3 T2 T1 T0"; do
  round=$((round + 1))
  for v in $order; do
    log "startup $v $round"
    $BENCH startup --app $APP --first-frame --env XDG_CONFIG_HOME=$(cfg $v) --env LOKYR_EXPERIMENT_TITLEBAR=$(force $v) > $OUT/startup-$v-$round.json
  done
done
log done

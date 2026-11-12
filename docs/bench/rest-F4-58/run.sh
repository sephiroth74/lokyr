#!/bin/sh
# 0.8.0 against 0.7.0 again, at rest, after F4-25's A/B, whose latency's
# p99 was 7 ms longer in seven rounds of eight and whose startup's gap was
# 7 ms longer in its first two rounds: the latency in 16 rounds, then the
# startup in four, AB BA BA AB, each of the minimal AppKit app of F0 as its
# floor, both copies started many times before; the same pause before each
# run. The window's motion comes after, of motion/run.sh, as its
# recordings load the machine.
set -u
OUT=/tmp/lokyr/rest-F4-58
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
app() {
  case $1 in
    v070) echo /tmp/lokyr/bench-v070/Lokyr.app ;;
    v080) echo /tmp/lokyr/bench-v080/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
busy() { ps -Ao pcpu,comm -r | sed -n 2,4p | sed 's/^/  /' >> $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/rest-F4-58/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
busy
for v in v070 v080; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
round=1
while [ $round -le 16 ]; do
  place=0
  for v in $(order $round v070 v080); do
    place=$((place + 1))
    sleep 2
    log "latency $v $round place $place"
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$v-$round.json
  done
  load
  round=$((round + 1))
done
busy
for round in 1 2 3 4; do
  for v in $(order $round v070 v080); do
    sleep 2
    log "startup $v $round"
    $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  sleep 2
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  load
done
busy
log done

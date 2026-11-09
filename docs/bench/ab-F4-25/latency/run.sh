#!/bin/sh
# The latency of 0.7.0's app against 0.8.0's again, in a session of its own
# after the A/B's, whose p99 was 7 ms longer in seven rounds of eight: 16
# rounds of balanced orders, AB BA BA AB, after a warm-up of each, the same
# pause before each run.
set -u
OUT=/tmp/lokyr/latency-F4-25
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
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
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F4-25/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
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
log done

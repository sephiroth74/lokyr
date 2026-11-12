#!/bin/sh
# F3-35's session: the latency of `h`, the app as it ships, and of `w`, the
# app beside a plugin on its thread that renders without pause, reading the
# focused pane. The faster regime first, as F3-135's sessions; then six
# rounds, each bundle first in three of them, as the place in a round moves
# the p99; the process's CPU sampled in the middle of each run.
set -u
OUT=/tmp/lokyr/bench-F3-35
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
bench() {
  XDG_CONFIG_HOME=$CFG "$OUT/$1/Lokyr.app/Contents/MacOS/lokyr" --bench latency --keys "$2"
}
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-29/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
for v in h w; do
  bench $v 20 > /dev/null
done
sleep 20
tries=0
while :; do
  tries=$((tries + 1))
  bench h 50 > $OUT/gate-$tries.json
  p50=$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["latency_ms"]["p50"])' $OUT/gate-$tries.json)
  log "gate $tries: p50 $p50"
  if python3 -I -c 'import sys; sys.exit(0 if float(sys.argv[1]) < 21 else 1)' "$p50"; then
    break
  fi
  if [ $tries -ge 10 ]; then
    log "the regime did not come"
    exit 1
  fi
  sleep 30
done
round=0
for order in "h w" "w h" "w h" "h w" "h w" "w h"; do
  round=$((round + 1))
  for v in $order; do
    log "latency $v $round"
    bench $v 200 > $OUT/latency-$v-$round.json &
    sleep 6
    cpu=$(ps -A -o %cpu=,command= | grep "$OUT/$v/Lokyr.app/Contents/MacOS/lokyr --bench" | grep -v grep | awk '{print $1}' | head -1)
    log "cpu $v $round: $cpu"
    wait
  done
done
uptime >> $OUT/session.log
log done

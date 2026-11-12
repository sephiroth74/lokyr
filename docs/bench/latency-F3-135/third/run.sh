#!/bin/sh
# The third session of the latency's tail: six bundles, `o`, 0.3.0's DMG;
# `a`, 0.4.0; `m2`, the slots in the windows; `m3`, the live resize's pace
# and the view's contents placement; `h`, the head; `t`, the head but for
# that placement. The faster regime first, as the second session's; then
# six rounds of a Williams design, each bundle once in each place of a
# round and after each other once.
set -u
OUT=/tmp/lokyr/bench-F3-135c
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
app() {
  case $1 in
    o) echo /tmp/lokyr/bench-v030/Lokyr.app ;;
    a) echo /tmp/lokyr/bench-F3-135/a/Lokyr.app ;;
    h) echo /tmp/lokyr/bench-F3-135b/h/Lokyr.app ;;
    *) echo $OUT/$1/Lokyr.app ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
bench() {
  XDG_CONFIG_HOME=$CFG "$(app $1)/Contents/MacOS/lokyr" --bench latency --keys "$2"
}
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-29/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
for v in o a m2 m3 h t; do
  bench $v 20 > /dev/null
done
sleep 20
tries=0
while :; do
  tries=$((tries + 1))
  bench o 50 > $OUT/gate-$tries.json
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
for order in "o a t m2 h m3" "a m2 o m3 t h" "m2 m3 a h o t" "m3 h m2 t a o" "h t m3 o m2 a" "t o h a m3 m2"; do
  round=$((round + 1))
  for v in $order; do
    log "latency $v $round"
    bench $v 200 > $OUT/latency-$v-$round.json
  done
done
uptime >> $OUT/session.log
log done

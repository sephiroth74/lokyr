#!/bin/sh
# More rounds of the latency's measure, after run.sh, as its two rounds
# found 0.4.0's p99 a refresh higher, of one sample of 199: rounds 3 to 5,
# 0.3.0 then 0.4.0, as run.sh's; with `reversed`, rounds 6 to 8, 0.4.0
# then 0.3.0, as the second of a pair could be the slower.
set -u
OUT=/tmp/lokyr/bench-F3-29
CFG=$OUT/config
app() {
  case $1 in
    v030) echo /tmp/lokyr/bench-v030/Lokyr.app ;;
    v040) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
rounds="3 4 5"
order="v030 v040"
if [ "${1:-}" = reversed ]; then
  rounds="6 7 8"
  order="v040 v030"
fi
for round in $rounds; do
  for v in $order; do
    log "latency $v $round"
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$v-$round.json
  done
done
log "latency done"

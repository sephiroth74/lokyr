#!/bin/sh
# The second session of the latency's tail, in the faster regime of the
# first A/B rounds: `o`, the app of 0.3.0's DMG; `a`, 0.4.0 built of its
# commit; `h`, the head of the work since, built as a release. A short run
# of `o` first, until its p50 is under 21 ms, as the regime of a p50 near
# 19 ms has it, ten tries at most, 30 s apart; then six rounds, whose
# orders put each bundle first, second and third twice.
set -u
OUT=/tmp/lokyr/bench-F3-135b
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
app() {
  case $1 in
    o) echo /tmp/lokyr/bench-v030/Lokyr.app ;;
    a) echo /tmp/lokyr/bench-F3-135/a/Lokyr.app ;;
    h) echo $OUT/h/Lokyr.app ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
bench() {
  XDG_CONFIG_HOME=$CFG "$(app $1)/Contents/MacOS/lokyr" --bench latency --keys "$2"
}
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-29/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
# The bundles' first launches, which the system checks, before the measures.
for v in o a h; do
  bench $v 20 > /dev/null
done
sleep 20
# The regime.
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
for order in "o a h" "a h o" "h o a" "h a o" "a o h" "o h a"; do
  round=$((round + 1))
  for v in $order; do
    log "latency $v $round"
    bench $v 200 > $OUT/latency-$v-$round.json
  done
done
uptime >> $OUT/session.log
log done

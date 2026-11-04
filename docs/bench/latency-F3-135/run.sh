#!/bin/sh
# The latency of three bundles in one session, as F3-29's rounds found
# 0.4.0's p99 higher than 0.3.0's: `a`, 0.4.0 built of its commit; `b`,
# the same but for the windows' accessibility trees, which it gives a client
# that asks and builds again as the screens change no more; `o`, the app of
# 0.3.0's DMG. Six rounds, of orders that put each bundle first, second
# and third twice, and each after each of the others once at least.
set -u
OUT=/tmp/lokyr/bench-F3-135
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
app() {
  case $1 in
    a) echo $OUT/a/Lokyr.app ;;
    b) echo $OUT/b/Lokyr.app ;;
    o) echo /tmp/lokyr/bench-v030/Lokyr.app ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-29/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
# The new bundles' first launches, which the system checks, before the
# measures.
for v in a b o; do
  XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
sleep 20
round=0
for order in "a b o" "b o a" "o a b" "o b a" "b a o" "a o b"; do
  round=$((round + 1))
  for v in $order; do
    log "latency $v $round"
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$v-$round.json
  done
done
uptime >> $OUT/session.log
log done

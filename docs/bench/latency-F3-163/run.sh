#!/bin/sh
# The latency's tail of the work since 0.4.0, bisected: `o`, 0.4.0's DMG;
# `k`, 0acf7fd, F3-135's head, whose tail was 0.3.0's; `m`, 282fdc7, the
# plugins' panels, threads, config and distributions on it; `h`, 0.5.0's
# DMG, the window's motion's work and the review's fixes on that. Eight
# rounds of 200 keys, a Williams design of four bundles twice, each bundle
# once in each place of four rounds and after each other once, after a
# warm-up of each; the load average told after each round.
set -u
OUT=/tmp/lokyr/bench-F3-163
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
app() {
  case $1 in
    o) echo /tmp/lokyr/bench-v040/Lokyr.app ;;
    k) echo /tmp/lokyr/bench-F3-135b/h/Lokyr.app ;;
    m) echo $OUT/m/Lokyr.app ;;
    h) echo /tmp/lokyr/bench-v050/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0) echo "o k h m" ;;
    1) echo "k m o h" ;;
    2) echo "m h k o" ;;
    3) echo "h o m k" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
bench() {
  XDG_CONFIG_HOME=$CFG "$(app $1)/Contents/MacOS/lokyr" --bench latency --keys "$2"
}
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/ab-F3-40/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
for v in o k m h; do
  log "warm-up $v"
  bench $v 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round); do
    place=$((place + 1))
    log "latency $v $round place $place"
    bench $v 200 > $OUT/latency-$v-$round.json
  done
  uptime >> $OUT/session.log
done
log done

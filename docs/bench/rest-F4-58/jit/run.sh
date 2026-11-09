#!/bin/sh
# Whether the hardened runtime's `allow-jit`, the one entitlement of 0.8.0's
# bundle that 0.7.0's lacks, lengthens the latency's tail, as the builds
# between them, run outside a bundle, do not: 0.7.0's app and 0.8.0's, each
# of its DMG, and copies of them signed again of the same identity and the
# hardened runtime, 0.7.0's with `allow-jit` and 0.8.0's without. Eight
# rounds, each of the four in an order that turns, the last four reversed,
# so that each runs in each place twice; after a warm-up of each, the same
# pause before each run.
set -u
OUT=/tmp/lokyr/jit-F4-58
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BUILDS="v070 v070-jit v080 v080-nojit"
binary() {
  case $1 in
    v070) echo /tmp/lokyr/bench-v070/Lokyr.app/Contents/MacOS/lokyr ;;
    v080) echo /tmp/lokyr/bench-v080/Lokyr.app/Contents/MacOS/lokyr ;;
    *) echo /tmp/lokyr/jit/$1/Lokyr.app/Contents/MacOS/lokyr ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
busy() { ps -Ao pcpu,comm -r | sed -n 2,4p | sed 's/^/  /' >> $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/rest-F4-58/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
busy
for build in $BUILDS; do
  log "latency warm-up $build"
  XDG_CONFIG_HOME=$CFG $(binary $build) --bench latency --keys 20 > /dev/null
done
# The order of round `$1`, of 8: the builds turned by the round, the
# last six rounds the first six's reversed, so that each build runs in each
# place twice.
order() {
  set -- $1 $BUILDS
  round=$1
  shift
  n=$#
  turned=""
  i=0
  while [ $i -lt $n ]; do
    k=$(( (i + round - 1) % n + 1 ))
    eval "turned=\"\$turned \${$k}\""
    i=$((i + 1))
  done
  if [ $round -gt $n ]; then
    echo "$turned" | tr ' ' '\n' | sed '/^$/d' | tail -r | tr '\n' ' '
  else
    echo "$turned"
  fi
}
round=1
while [ $round -le 8 ]; do
  place=0
  for build in $(order $round); do
    place=$((place + 1))
    sleep 2
    log "latency $build $round place $place"
    XDG_CONFIG_HOME=$CFG $(binary $build) --bench latency --keys 200 > $OUT/latency-$build-$round.json
  done
  load
  busy
  round=$((round + 1))
done
log done

#!/bin/sh
# Where `allow-jit`'s time goes, of the latency's bench's phases: the press
# to the app's handling of its key, that to the frame of its echo, ready to
# present, and that to its presentation. The bundle of the commit that
# added the phases, signed as built, of `allow-jit`; again without it; and
# again of `allow-unsigned-executable-memory` in its place, which lets
# Luau's code generator map its code too. Nine rounds, each of the three in
# an order that turns, so that each runs in each place three times; after a
# warm-up of each, the same pause before each run.
set -u
OUT=/tmp/lokyr/phases-F4-58
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BUILDS="jit nojit unsigned"
binary() {
  echo /tmp/lokyr/jit2/$1/Lokyr.app/Contents/MacOS/lokyr
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
# The order of round `$1`: the builds turned by the round.
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
  echo "$turned"
}
round=1
while [ $round -le 9 ]; do
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

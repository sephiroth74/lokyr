#!/bin/sh
# The latency of six builds between 0.7.0 and 0.8.0, each the release
# binary of its commit, run outside a bundle alike: 0.7.0; the fixes of the
# review after F3-207; the host's hardening, F4-01 to F4-06; the Luau host,
# its review's fixes and the timers; the installed plugins, the consent and
# the reloads with their review's fixes; and 0.8.0. Twelve rounds, each of
# the six in an order that turns, the last six reversed, so that each build
# runs in each place twice; after a warm-up of each, the same pause before
# each run.
set -u
OUT=/tmp/lokyr/bisect-F4-58
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BUILDS="v070 a-F3fixes b-F4-06 c-F4-13 d-F4-56 v080"
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
busy() { ps -Ao pcpu,comm -r | sed -n 2,4p | sed 's/^/  /' >> $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $REPO/docs/bench/rest-F4-58/config.toml $CFG/lokyr/config.toml
uptime >> $OUT/session.log
busy
for build in $BUILDS; do
  log "latency warm-up $build"
  XDG_CONFIG_HOME=$CFG /tmp/lokyr/bisect/$build/lokyr --bench latency --keys 20 > /dev/null
done
# The order of round `$1`, of 12: the builds turned by the round, the
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
while [ $round -le 12 ]; do
  place=0
  for build in $(order $round); do
    place=$((place + 1))
    sleep 2
    log "latency $build $round place $place"
    XDG_CONFIG_HOME=$CFG /tmp/lokyr/bisect/$build/lokyr --bench latency --keys 200 > $OUT/latency-$build-$round.json
  done
  load
  busy
  round=$((round + 1))
done
log done

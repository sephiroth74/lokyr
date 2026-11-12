#!/bin/sh
# The latency of 0.7.0's app and 0.8.0's, each of its DMG, and copies of them
# signed again, 0.7.0's with `allow-jit` and 0.8.0's without, as F4-58's
# sessions on the managed Mac: 16 rounds, the four in orders that turn, the
# last eight reversed, so that each runs in each place four times; after a
# warm-up of each, the same pause before each run. Each app opened by
# `open`, which starts it in the desktop's session from an SSH one, its
# report through `--stdout`.
set -u
K=$HOME/lokyr-bench
OUT=$K/results/latency
CFG=$K/config-home
BUILDS="v070 v070-jit v080 v080-nojit"
mkdir -p $OUT $CFG/lokyr
cp $K/config.toml $CFG/lokyr/config.toml
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
busy() { ps -Ao pcpu,comm -r | sed -n 2,4p | sed 's/^/  /' >> $OUT/session.log; }
run() {
  open -n -W --stdout "$2" --stderr "$2.err" --env XDG_CONFIG_HOME=$CFG \
    "$K/apps/$1/Lokyr.app" --args --bench latency --keys "$3"
}
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
  if [ $(( (round - 1) / n % 2 )) -eq 1 ]; then
    echo "$turned" | tr ' ' '\n' | sed '/^$/d' | tail -r | tr '\n' ' '
  else
    echo "$turned"
  fi
}
caffeinate -dimsu -w $$ &
uptime >> $OUT/session.log
busy
for build in $BUILDS; do
  log "latency warm-up $build"
  run $build $OUT/warm-$build.json 20
done
round=1
while [ $round -le 16 ]; do
  place=0
  for build in $(order $round); do
    place=$((place + 1))
    sleep 2
    log "latency $build $round place $place"
    run $build $OUT/latency-$build-$round.json 200
  done
  echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log
  busy
  round=$((round + 1))
done
log done

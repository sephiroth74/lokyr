#!/bin/sh
# Every measure of the M1's session in a row, started over SSH by
# `nohup ~/lokyr-bench/all.sh &`: the entitlement's micro-benchmarks, the
# latency, the startup, then the window's motion in Terminal, which the
# session waits for.
set -u
K=$HOME/lokyr-bench
mkdir -p $K/results
log() { echo "$(date +%T) $*" >> $K/results/all.log; }
caffeinate -dimsu -w $$ &
log "rest: $(sysctl -n vm.loadavg)"
log micro
$K/micro.sh > $K/results/micro.log 2>&1
log latency
$K/latency.sh > /dev/null 2>&1
log startup
$K/startup.sh > /dev/null 2>&1
log motion
rm -rf $K/results/motion
open -a Terminal $K/motion.sh
waited=0
until grep -q " done$" $K/results/motion/session.log 2>/dev/null; do
  waited=$((waited + 10))
  if [ $waited -ge 2400 ]; then
    log "motion not done after 40 minutes"
    exit 1
  fi
  sleep 10
done
log done

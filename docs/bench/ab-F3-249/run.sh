#!/bin/sh
# The inline images' cost at rest and in throughput, of no image shown:
# 0.9.0's app, of its release, against the bundle of the images, of
# `just bundle`, in one session of balanced orders: each copy started once
# before the measured rounds, as a scan of a new executable slows its
# first starts; idle and memory in two rounds; each throughput's workload
# in two.
set -u
OUT=/tmp/lokyr/bench-F3-249
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
HERE=$REPO/docs/bench/ab-F3-249
BENCH=$REPO/target/release/lokyr-bench
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v090) echo /tmp/lokyr/bench-v090/Lokyr.app ;;
    next) echo /tmp/lokyr/bench-next/Lokyr.app ;;
  esac
}
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
mkdir -p $CFG/lokyr
cp $HERE/config.toml $CFG/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
for v in v090 next; do
  log "first start $v"
  $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG > /dev/null
done
rest() {
  name=$1 bundle=$2
  shift 2
  log "idle $name"
  before=$(pgrep -f "$bundle/Contents/MacOS/lokyr")
  open -n "$@" "$bundle"
  sleep 3
  pid=$(pgrep -nf "$bundle/Contents/MacOS/lokyr")
  [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $name"; return; }
  $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$name.json
  sleep 18
  ps -M -p $pid | tail -n +2 | wc -l | tr -d ' ' > $OUT/threads-$name.txt
  $BENCH idle --pid $pid --duration 60 > $OUT/idle-$name.json
  $BENCH memory --pid $pid --duration 10 > $OUT/memory-$name.json
  kill $pid; sleep 2
}
for round in 1 2; do
  for v in $(order $round v090 next); do
    rest $v-$round "$(app $v)" --env XDG_CONFIG_HOME=$CFG
  done
  load
done
for file in ascii seq sgr cjk; do
  for round in 1 2; do
    for v in $(order $round v090 next); do
      log "throughput $file $v $round"
      rm -f $OUT/onscreen-$file-$v-$round.jsonl
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 > $OUT/frames-$file-$v-$round.json
    done
  done
  load
done
log done

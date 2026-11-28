#!/bin/sh
# The releases' measures that ttybench does not take, of the A/B of 1.6.0:
# 1.5.0's tag built into a bundle against 1.6's bundle, signed alike, as
# the sessions of ttybench measured them. The startup's gap is ttybench's
# from 1.6.0 on, so it takes the memory at rest and the frames' time on the
# CPU alone. `sh run.sh` runs the next step not done yet, `sh run.sh list`
# lists the steps and those done, and `sh run.sh <step>` runs a step again.
# Each step holds its own balanced orders, AB BA BA AB, and the same pause
# before each run.
#
# - prepare: lokyr-bench, the workloads, and the two copies of
#   /tmp/lokyr/ttybench, which `just bench-prepare` put there; it measures
#   nothing, and the Mac stays free.
# - memory: the memory at rest without the GPU's, of vmmap's summaries in
#   four rounds, then each throughput's workload in two, of each frame's
#   time on the CPU.
#
# The config is the releases' of the least work, ab-F3b-20/config.toml's.
# The reports go to target/bench, which a restart that empties /tmp keeps.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=$REPO/target/bench/ab-F5b-67/lokyr-bench
CFG=$OUT/config
BENCH=$REPO/target/release/lokyr-bench
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
COPIES=/tmp/lokyr/ttybench
STEPS="prepare memory"
DONE=$OUT/steps.done

app() { echo $OUT/$1/Lokyr.app; }
# The order of round `$1` of the pair `$2 $3`: AB, BA, BA, AB, ...
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
minutes() {
  case $1 in
    prepare) echo "2 min, the Mac free" ;;
    memory) echo "6 min" ;;
  esac
}
conditions() {
  {
    echo "--- $1"
    pmset -g batt | head -2
    uptime
    system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|UI Looks"
  } >> $OUT/session.log
}

prepare() {
  mkdir -p $CFG/lokyr
  cp $REPO/docs/bench/ab-F3b-20/config.toml $CFG/lokyr/config.toml
  log "lokyr-bench"
  (cd $REPO && cargo build --release -p lokyr-bench) || return 1
  if [ ! -f $W/cjk.txt ]; then
    log "workloads"
    mkdir -p $W
    python3 -I $REPO/tools/bench/workloads.py $W || return 1
  fi
  for pair in v150:a v160:b; do
    v=${pair%%:*} from=$COPIES/${pair#*:}/Lokyr.app
    [ -d "$from" ] || { log "no copy at $from: just bench-prepare first"; return 1; }
    rm -rf $OUT/$v && mkdir -p $OUT/$v
    ditto "$from" $OUT/$v/Lokyr.app || return 1
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist"))"
  done
}

# The app `$1` started of the config, its pid in `pid`, 3 s after.
started() {
  bundle=$1
  before=$(pgrep -f "$bundle/Contents/MacOS/lokyr")
  open -n --env XDG_CONFIG_HOME=$CFG "$bundle"
  sleep 3
  pid=$(pgrep -nf "$bundle/Contents/MacOS/lokyr")
  [ -n "$pid" ] && [ "$pid" != "$before" ]
}
memory() {
  for round in 1 2 3 4; do
    for v in $(order $round v150 v160); do
      name=$v-$round
      log "memory $name"
      started "$(app $v)" || { log "no pid for $name"; continue; }
      sleep 17
      $BENCH memory --pid $pid --duration 10 > $OUT/vmmap-memory-$name.json
      vmmap --summary $pid > $OUT/vmmap-$name.txt 2> /dev/null
      kill $pid
      sleep 2
    done
    load
  done
  for file in ascii seq sgr cjk; do
    for round in 1 2; do
      for v in $(order $round v150 v160); do
        log "throughput $file $v $round"
        rm -f $OUT/onscreen-$file-$v-$round.jsonl
        XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
          python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 \
          > $OUT/frames-$file-$v-$round.json
      done
    done
    load
  done
}

run_step() {
  step=$1
  case $step in
    prepare | memory) ;;
    *)
      echo "no step $step: $STEPS" >&2
      return 1
      ;;
  esac
  mkdir -p $OUT
  log "step $step"
  conditions "$step"
  case $step in
    prepare) prepare ;;
    memory) memory ;;
  esac
  status=$?
  if [ $status -ne 0 ]; then
    log "step $step failed"
    return $status
  fi
  grep -qx "$step" $DONE 2> /dev/null || echo "$step" >> $DONE
  log "step $step done"
}

mkdir -p $OUT
case ${1:-next} in
  list)
    for step in $STEPS; do
      if grep -qx "$step" $DONE 2> /dev/null; then mark=done; else mark=-; fi
      printf '%-10s %-5s about %s\n' "$step" "$mark" "$(minutes $step)"
    done
    ;;
  next)
    for step in $STEPS; do
      grep -qx "$step" $DONE 2> /dev/null && continue
      echo "next: $step, about $(minutes $step)"
      run_step "$step" || exit 1
      exit 0
    done
    echo "every step done"
    ;;
  *) run_step "$1" ;;
esac

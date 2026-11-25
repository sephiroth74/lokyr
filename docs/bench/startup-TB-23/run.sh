#!/bin/sh
# ttybench's startup beside lokyr-bench's, so that the releases' startup
# gap moves to ttybench: 1.5.0's app of its notarized DMG, of the same
# config, measured
#
# 1. by ttybench, its window and the floor app's of each round,
#    `ttybench-floor` launched as Lokyr is, by `open`: `window_gap_ms`, 32
#    rounds in two runs, the copy and the floor launched 10 times first, as
#    the copy was launched many times already;
# 2. by lokyr-bench, Lokyr's first frame against the minimal AppKit app's
#    window, as the releases' gap was measured until 1.5.0: six rounds of
#    ten starts of the copy and of the minimal app, in turns.
#
# The results go to the repository's `target/bench/startup-TB-23`, which a
# restart keeps, as it empties /tmp; the copy is startup-F5b-45's, of the
# same DMG (`sh ../startup-F5b-45/run.sh prepare` makes it).
#
# Each part about 3 minutes once the load settles, from a terminal, VS
# Code and every app that can be closed closed, the Mac left alone until it
# ends:
#
# - `sh run.sh ttybench`, then `sh run.sh lokyr-bench`, or both in a row
#   of `sh run.sh`;
# - `sh run.sh dry` runs ttybench's part of two rounds and a season of
#   two, to check it end to end, about half a minute;
# - `python3 analyze.py` sums both up.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=$REPO/target/bench/startup-TB-23
COPIES=$REPO/target/bench/startup-F5b-45/copies
CFG=$OUT/configs/base
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
TTYBENCH=${TTYBENCH:-$REPO/../ttybench/target/release/ttybench}

app() { echo $COPIES/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
# The copy and the minimal app in turns, of each round's order.
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v150 floor" ;;
    *) echo "floor v150" ;;
  esac
}
conditions() {
  {
    echo "--- $1"
    pmset -g batt | head -2
    uptime
    system_profiler SPDisplaysDataType 2> /dev/null | grep -E ":$|Resolution|Main Display|UI Looks"
    top -l 2 -n 12 -o cpu -stats pid,command,cpu | awk '/^PID/ {n++} n == 2'
  } >> $OUT/session.log
}

# The config, made anew.
config() {
  rm -rf $OUT/configs && mkdir -p $OUT/configs
  cp -R $REPO/tools/bench/sessions/configs/base $CFG
}

# The terminal and ttybench's session of `$1` cycles, `$2` rounds a run
# and a season of `$3`, in `$OUT`.
session() {
  config
  cat > $OUT/terminals.toml <<EOF
# The copy of startup-TB-23, of 1.5.0's notarized DMG.

[terminals.v150]
app = "$(app v150)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }
EOF
  cat > $OUT/session.toml <<EOF
# startup-TB-23's part of ttybench: the copy beside the floor app.
terminals = ["v150"]
terminals-file = "terminals.toml"
pause = 2
cycles = $1
season = $3
folder = "$OUT/ttybench-{date}"

[[run]]
tool = "startup"
rounds = $2
args = ["--floor"]
EOF
}

# Six rounds of lokyr-bench: the copy's first frame and the minimal app's
# window, ten starts each after two not counted.
lokyr_bench() {
  [ -d $CFG ] || config
  for round in $(seq 1 6); do
    for v in $(order $round); do
      sleep 2
      log "lokyr-bench $v $round"
      if [ $v = floor ]; then
        $BENCH startup --app $SPIKE > $OUT/lokyr-bench-floor-$round.json
      else
        $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
          > $OUT/lokyr-bench-$v-$round.json
      fi
    done
    load
  done
}

# Until the one-minute load is below 3, 15 minutes at most.
settle() {
  for i in $(seq 1 90); do
    one=$(sysctl -n vm.loadavg | awk '{print $2}')
    if awk -v l="$one" 'BEGIN {exit !(l < 3)}'; then
      log "load $one"
      return
    fi
    [ $(( i % 6 )) -eq 1 ] && log "waiting for the load to settle: $one"
    sleep 10
  done
  log "the load did not settle: $(sysctl -n vm.loadavg)"
}

ready() {
  [ -d "$(app v150)" ] || { echo "no $(app v150): sh $REPO/docs/bench/startup-F5b-45/run.sh prepare first" >&2; return 1; }
  [ -x "$TTYBENCH" ] || { echo "no $TTYBENCH: cargo build --release in ttybench first, or name it in TTYBENCH" >&2; return 1; }
  [ -x "$BENCH" ] || { echo "no $BENCH: cargo build --release -p lokyr-bench first" >&2; return 1; }
  [ -d "$SPIKE" ] || { echo "no $SPIKE" >&2; return 1; }
}

dry() {
  ready || return 1
  session 1 2 2
  log "dry"
  $TTYBENCH run $OUT/session.toml
}

# The part `$1`, ttybench, lokyr-bench or both, of `$2` minutes, at rest.
measure() {
  ready || return 1
  if pgrep -f "Visual Studio Code.app/Contents/MacOS" > /dev/null; then
    echo "VS Code is open: close it, then run this again" >&2
    return 1
  fi
  echo "about $2 minutes once the load settles: leave the Mac alone until it ends; starting in 10 s"
  sleep 10
  caffeinate -d -i -w $$ &
  log "session: $1"
  conditions "start of $1"
  settle
  case $1 in
    ttybench|both)
      session 2 16 10
      $TTYBENCH run $OUT/session.toml
      load ;;
  esac
  case $1 in
    lokyr-bench|both) lokyr_bench ;;
  esac
  conditions "end of $1"
  log "session done: $1"
  echo "done: the Mac is free, and VS Code can open again"
}

mkdir -p $OUT
case ${1:-both} in
  dry) dry ;;
  ttybench) measure ttybench 3 ;;
  lokyr-bench) measure lokyr-bench 3 ;;
  both) measure both 6 ;;
  *) echo "usage: sh $0 [ttybench|lokyr-bench|dry]" >&2; exit 2 ;;
esac

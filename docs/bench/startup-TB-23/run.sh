#!/bin/sh
# ttybench's startup beside lokyr-bench's, in one session, so that the
# releases' startup gap moves to ttybench: 1.4.0's and 1.5.0's apps of
# their notarized DMGs, of the same config, measured
#
# 1. by ttybench, each copy's window and the floor app's of each round,
#    `ttybench-floor` launched as Lokyr is, by `open`: `window_gap_ms`, 32
#    rounds of each copy in four cycles, AB BA BA AB, each copy and the
#    floor launched 30 times first;
# 2. by lokyr-bench, Lokyr's first frame against the minimal AppKit app's
#    window, as the releases' gap was measured until 1.5.0: six rounds of
#    ten starts of each copy and of the minimal app, AB BA BA AB AB BA.
#
# The results go to the repository's `target/bench/startup-TB-23`, which a
# restart keeps, as it empties /tmp; the copies are startup-F5b-45's, of
# the same DMGs (`sh ../startup-F5b-45/run.sh prepare` makes them).
#
# - `sh run.sh dry` runs ttybench's part of two rounds and a season of
#   two, to check it end to end, about a minute.
# - `sh run.sh` measures, about 15 minutes once the load settles, from a
#   terminal, VS Code and every app that can be closed closed, the Mac left
#   alone until it ends.
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
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v140 v150" ;;
    *) echo "v150 v140" ;;
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

# The config, the terminals and ttybench's session of `$1` cycles, `$2`
# rounds a run and a season of `$3`, in `$OUT`.
session() {
  rm -rf $OUT/configs && mkdir -p $OUT/configs
  cp -R $REPO/tools/bench/sessions/configs/base $CFG
  cat > $OUT/terminals.toml <<EOF
# The two copies of startup-TB-23, of the releases' notarized DMGs.

[terminals.v140]
app = "$(app v140)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }

[terminals.v150]
app = "$(app v150)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }
EOF
  cat > $OUT/session.toml <<EOF
# startup-TB-23's part of ttybench: each copy beside the floor app.
terminals = ["v140", "v150"]
terminals-file = "terminals.toml"
ab = { a = "v140", b = "v150" }
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

# Six rounds of lokyr-bench: each copy's first frame and the minimal app's
# window, ten starts each after two not counted.
lokyr_bench() {
  for round in $(seq 1 6); do
    for v in $(order $round); do
      sleep 2
      log "lokyr-bench $v $round"
      $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
        > $OUT/lokyr-bench-$v-$round.json
    done
    sleep 2
    log "lokyr-bench floor $round"
    $BENCH startup --app $SPIKE > $OUT/lokyr-bench-floor-$round.json
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
  for v in v140 v150; do
    [ -d "$(app $v)" ] || { echo "no $(app $v): sh $REPO/docs/bench/startup-F5b-45/run.sh prepare first" >&2; return 1; }
  done
  [ -x "$TTYBENCH" ] || { echo "no $TTYBENCH: cargo build --release in ttybench first, or name it in TTYBENCH" >&2; return 1; }
  [ -x "$BENCH" ] || { echo "no $BENCH: cargo build --release -p lokyr-bench first" >&2; return 1; }
  [ -d "$SPIKE" ] || { echo "no $SPIKE" >&2; return 1; }
}

dry() {
  ready || return 1
  session 2 2 2
  log "dry"
  $TTYBENCH run $OUT/session.toml
}

measure() {
  ready || return 1
  if pgrep -f "Visual Studio Code.app/Contents/MacOS" > /dev/null; then
    echo "VS Code is open: close it, then run this again" >&2
    return 1
  fi
  echo "about 15 minutes once the load settles: leave the Mac alone until it ends; starting in 10 s"
  sleep 10
  caffeinate -d -i -w $$ &
  log "session"
  conditions start
  settle
  session 4 8 30
  $TTYBENCH run $OUT/session.toml
  load
  lokyr_bench
  conditions end
  log "session done"
  echo "done: the Mac is free, and VS Code can open again"
}

mkdir -p $OUT
case ${1:-measure} in
  dry) dry ;;
  measure) measure ;;
  *) echo "usage: sh $0 [dry|measure]" >&2; exit 2 ;;
esac

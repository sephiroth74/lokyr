#!/bin/sh
# The start of the first windows first, against 1.5.0: two bundles signed
# alike and not notarized, 1.5.0's tag (a) and the branch's (b), of the
# sessions' base config, measured
#
# 1. by ttybench, each copy's window and the floor app's of each round,
#    launched by `open` as a script's start is: `window_gap_ms`, 24 rounds
#    of each copy in two cycles, AB BA, each copy and the floor launched 20
#    times first;
# 2. by lokyr-bench, each copy's first frame on the display, its
#    executable started directly: four rounds of ten starts after two, AB
#    BA BA AB.
#
# The results and the copies go to the repository's `target/bench`, which
# a restart keeps, as it empties /tmp.
#
# - `sh run.sh prepare A B` copies both bundles; it measures nothing.
# - `sh run.sh ttybench`, about 6 minutes, then `sh run.sh lokyr-bench`,
#   about 2, once the load settles, from a terminal, VS Code and every app
#   that can be closed closed, the Mac left alone until each ends.
# - `python3 analyze.py` sums both up.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=$REPO/target/bench/startup-F5b-78
CFG=$OUT/configs/base
BENCH=$REPO/target/release/lokyr-bench
TTYBENCH=${TTYBENCH:-$REPO/../ttybench/target/release/ttybench}

app() { echo $OUT/copies/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "a b" ;;
    *) echo "b a" ;;
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

prepare() {
  [ -d "$1" ] && [ -d "$2" ] || { echo "usage: sh $0 prepare <a's Lokyr.app> <b's Lokyr.app>" >&2; return 1; }
  rm -rf $OUT/copies && mkdir -p $OUT/copies/a $OUT/copies/b
  ditto "$1" "$(app a)"
  ditto "$2" "$(app b)"
  for v in a b; do
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist"))"
  done
}

session() {
  rm -rf $OUT/configs && mkdir -p $OUT/configs
  cp -R $REPO/tools/bench/sessions/configs/base $CFG
  cat > $OUT/terminals.toml <<EOF
# The two copies of startup-F5b-78, signed alike.

[terminals.a]
app = "$(app a)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }

[terminals.b]
app = "$(app b)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }
EOF
  cat > $OUT/session.toml <<EOF
# startup-F5b-78's part of ttybench: each copy beside the floor app.
terminals = ["a", "b"]
terminals-file = "terminals.toml"
ab = { a = "a", b = "b" }
pause = 2
cycles = 2
season = 20
folder = "$OUT/ttybench-{date}"

[[run]]
tool = "startup"
rounds = 12
args = ["--floor"]
EOF
}

lokyr_bench() {
  [ -d $CFG ] || session
  for round in $(seq 1 4); do
    for v in $(order $round); do
      sleep 2
      log "lokyr-bench $v $round"
      $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
        > $OUT/lokyr-bench-$v-$round.json
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

# The part `$1`, of about `$2` minutes, at rest.
measure() {
  for v in a b; do
    [ -d "$(app $v)" ] || { echo "no $(app $v): sh $0 prepare first" >&2; return 1; }
  done
  [ -x "$TTYBENCH" ] || { echo "no $TTYBENCH: cargo build --release in ttybench first, or name it in TTYBENCH" >&2; return 1; }
  [ -x "$BENCH" ] || { echo "no $BENCH: cargo build --release -p lokyr-bench first" >&2; return 1; }
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
    ttybench)
      session
      $TTYBENCH run $OUT/session.toml ;;
    lokyr-bench) lokyr_bench ;;
  esac
  load
  conditions "end of $1"
  log "session done: $1"
  echo "done: the Mac is free, and VS Code can open again"
}

mkdir -p $OUT
case ${1:-} in
  prepare) shift; prepare "$@" ;;
  ttybench) measure ttybench 6 ;;
  lokyr-bench) measure lokyr-bench 2 ;;
  *) echo "usage: sh $0 prepare <a> <b> | ttybench | lokyr-bench" >&2; exit 2 ;;
esac

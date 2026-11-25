#!/bin/sh
# 1.5.0's startup of its release's DMG against 1.4.0's of its own, both
# notarized and stapled, at rest, as a copy that is not notarized starts
# slower than the same code notarized.
#
# - `sh run.sh prepare` copies both apps of their DMGs; it measures nothing.
# - `sh run.sh` measures, about 12 minutes, from a terminal, VS Code and
#   every app that can be closed closed, the Mac left alone until it ends:
#   each copy started 40 times, then twelve rounds of both startups, AB BA
#   BA AB, each round of the minimal AppKit app as its floor, the same
#   pause before each run.
#
# The config is ab-F3b-20/config.toml's, the releases' of the least work
# beside the terminal's own, smooth scrolling and Blocks off.
set -u
OUT=/tmp/lokyr/startup-F5b-21
CFG=$OUT/config
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
HERE=$REPO/docs/bench/startup-F5b-21
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
DMG140=${DMG140:-/tmp/lokyr/ttybench/dl/Lokyr-1.4.0.dmg}
DMG150=${DMG150:-$REPO/target/Lokyr-1.5.0.dmg}
COPIES="v140 v150"

app() { echo $OUT/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
# The order of round `$1`: AB, BA, BA, AB, ...
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
# The app of the DMG `$1` copied to `$2`.
copy() {
  mount=$(hdiutil attach -nobrowse -readonly "$1" | awk -F'\t' '/\/Volumes\// {print $NF}')
  [ -n "$mount" ] || return 1
  rm -rf $OUT/$2 && mkdir -p $OUT/$2
  ditto "$mount/Lokyr.app" $OUT/$2/Lokyr.app
  hdiutil detach -quiet "$mount"
  xattr -dr com.apple.quarantine $OUT/$2/Lokyr.app 2> /dev/null
}

prepare() {
  mkdir -p $CFG/lokyr
  cp $REPO/docs/bench/ab-F3b-20/config.toml $CFG/lokyr/config.toml
  [ -f $DMG140 ] || { log "no DMG of 1.4.0 at $DMG140"; return 1; }
  [ -f $DMG150 ] || { log "no DMG of 1.5.0 at $DMG150"; return 1; }
  copy $DMG140 v140 || return 1
  copy $DMG150 v150 || return 1
  for v in $COPIES; do
    if xcrun stapler validate -q "$(app $v)" > /dev/null 2>&1; then stapled=stapled; else stapled="not stapled"; fi
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist")), $stapled"
  done
}

measure() {
  for v in $COPIES; do
    [ -d "$(app $v)" ] || { echo "no $v: sh $HERE/run.sh prepare first" >&2; return 1; }
  done
  if pgrep -f "Visual Studio Code.app/Contents/MacOS" > /dev/null; then
    echo "VS Code is open: close it, then run this again" >&2
    return 1
  fi
  echo "about 12 minutes: leave the Mac alone until it ends; starting in 10 s"
  sleep 10
  caffeinate -d -i -w $$ &
  log "session"
  conditions start
  for v in $COPIES; do
    log "40 starts of $v"
    for i in $(seq 1 40); do
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench startup > /dev/null 2>&1
    done
  done
  for round in $(seq 1 12); do
    for v in $(order $round); do
      sleep 2
      log "startup $v $round"
      $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
        > $OUT/startup-$v-$round.json
    done
    sleep 2
    log "startup floor $round"
    $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
    load
  done
  conditions end
  log "session done"
  echo "done: the Mac is free, and VS Code can open again"
}

mkdir -p $OUT
case ${1:-measure} in
  prepare) prepare ;;
  measure) measure ;;
  *)
    echo "sh run.sh [prepare|measure]" >&2
    exit 1
    ;;
esac

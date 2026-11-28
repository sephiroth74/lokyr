#!/bin/sh
# 1.6.0's startup of its notarized DMG against 1.5.0's release DMG, both
# notarized and stapled, at rest, as a copy that is not notarized starts
# later than the same code notarized. The 1.6.0 copy is the release's
# candidate, of the same code as the release.
#
# 1. ttybench: each copy and the floor app launched 30 times first, then
#    four cycles of eight rounds of each copy, AB BA BA AB, the floor app
#    launched beside each round: the launch to the window, and the window
#    after the floor app's, the releases' startup gap.
# 2. lokyr-bench: eight rounds of each copy's first frame, ten warm starts
#    after two, and of the minimal AppKit app's window, in turns.
#
# The results go to the repository's `target/bench/startup-F5b-68`, which a
# restart keeps, as it empties /tmp.
#
# - `sh run.sh prepare` copies both apps of their DMGs; it measures nothing.
# - `sh run.sh dry` runs ttybench's part of two rounds and a season of two,
#   about a minute, to check it end to end.
# - `sh run.sh` measures, about 25 minutes once the load settles, from a
#   terminal, VS Code and every app that can be closed closed, the Mac left
#   alone until it ends.
# - `python3 analyze.py` sums lokyr-bench's part up; ttybench's report is
#   in its session's folder.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=$REPO/target/bench/startup-F5b-68
CFG=$OUT/configs/base
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
TTYBENCH=${TTYBENCH:-ttybench}
DMG150=${DMG150:-$REPO/target/Lokyr-1.5.0.dmg}
DMG160=${DMG160:-$REPO/target/bench/dl/Lokyr-1.6.0-rc.1.dmg}
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
ID=io.github.sephiroth74.lokyr
COPIES="v150 v160"

app() { echo $OUT/copies/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
# Each round's order of the copies and the minimal app.
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v150 v160 floor" ;;
    *) echo "floor v160 v150" ;;
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
# Lokyr's registered copies, a path a line.
registrations() {
  $LSREGISTER -dump 2> /dev/null \
    | awk -v id="$ID" '/^path:/ {sub(/^path: +/, ""); sub(/ \(0x[0-9a-f]+\)$/, ""); p = $0} $1 == "identifier:" && $2 == id {print p}' \
    | sort -u
}
# The app of the DMG `$1` copied to `$2`.
copy() {
  mount=$(hdiutil attach -nobrowse -readonly "$1" | awk -F'\t' '/\/Volumes\// {print $NF}')
  [ -n "$mount" ] || return 1
  rm -rf $OUT/copies/$2 && mkdir -p $OUT/copies/$2
  ditto "$mount/Lokyr.app" $OUT/copies/$2/Lokyr.app
  hdiutil detach -quiet "$mount"
  xattr -dr com.apple.quarantine $OUT/copies/$2/Lokyr.app 2> /dev/null
}

prepare() {
  rm -rf $OUT/configs && mkdir -p $OUT/configs
  cp -R $REPO/tools/bench/sessions/configs/base $CFG
  [ -f $DMG150 ] || { log "no DMG of 1.5.0 at $DMG150"; return 1; }
  [ -f $DMG160 ] || { log "no DMG of 1.6.0 at $DMG160"; return 1; }
  copy $DMG150 v150 || return 1
  copy $DMG160 v160 || return 1
  for v in $COPIES; do
    if xcrun stapler validate -q "$(app $v)" > /dev/null 2>&1; then stapled=stapled; else stapled="not stapled"; fi
    if spctl --assess --type execute "$(app $v)" 2> /dev/null; then accepted=accepted; else accepted="refused"; fi
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist")), $stapled, Gatekeeper $accepted"
  done
  # The registrations of copies that are gone, as a restart's /tmp.
  registrations | while read -r path; do
    [ -e "$path" ] || { log "unregistered $path, gone"; $LSREGISTER -u "$path" 2> /dev/null; }
  done
  log "registrations: $(registrations | wc -l | tr -d ' ')"
}

# ttybench's session of `$1` cycles, `$2` rounds a run and a season of
# `$3`, in `$OUT`.
session() {
  cat > $OUT/terminals.toml <<EOF
# The copies of startup-F5b-68, of the releases' notarized DMGs.

[terminals.v150]
app = "$(app v150)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }

[terminals.v160]
app = "$(app v160)"
launch = "open"
env = { XDG_CONFIG_HOME = "$CFG" }
EOF
  cat > $OUT/session.toml <<EOF
# startup-F5b-68's part of ttybench: both copies beside the floor app.
terminals = ["v150", "v160"]
terminals-file = "terminals.toml"
ab = { a = "v150", b = "v160" }
pause = 2
cycles = $1
season = $3
folder = "$OUT/ttybench-{date}"

[[run]]
tool = "startup"
rounds = $2
args = ["--floor"]

[[budget]]
metric = "window_gap_ms"
at-most = 70
EOF
}

# Eight rounds of lokyr-bench: each copy's first frame and the minimal
# app's window, ten starts each after two not counted.
lokyr_bench() {
  for round in $(seq 1 8); do
    for v in $(order $round); do
      sleep 2
      log "lokyr-bench $v $round"
      if [ $v = floor ]; then
        $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
      else
        $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
          > $OUT/startup-$v-$round.json
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
  for v in $COPIES; do
    [ -d "$(app $v)" ] || { echo "no $(app $v): sh $0 prepare first" >&2; return 1; }
  done
  command -v "$TTYBENCH" > /dev/null || { echo "no $TTYBENCH: cargo install ttybench, or name it in TTYBENCH" >&2; return 1; }
  [ -x "$BENCH" ] || { echo "no $BENCH: cargo build --release -p lokyr-bench first" >&2; return 1; }
  [ -d "$SPIKE" ] || { echo "no $SPIKE" >&2; return 1; }
}

dry() {
  ready || return 1
  session 1 2 2
  log "dry"
  "$TTYBENCH" run $OUT/session.toml
}

measure() {
  ready || return 1
  if pgrep -f "Visual Studio Code.app/Contents/MacOS" > /dev/null; then
    echo "VS Code is open: close it, then run this again" >&2
    return 1
  fi
  echo "about 25 minutes once the load settles: leave the Mac alone until it ends; starting in 10 s"
  sleep 10
  caffeinate -d -i -w $$ &
  log "session"
  conditions "start"
  settle
  session 4 8 30
  "$TTYBENCH" run $OUT/session.toml
  load
  lokyr_bench
  conditions "end"
  log "session done"
  echo "done: the Mac is free, and VS Code can open again"
}

mkdir -p $OUT
case ${1:-measure} in
  prepare) prepare ;;
  dry) dry ;;
  measure) measure ;;
  *) echo "usage: sh $0 [prepare|dry|measure]" >&2; exit 2 ;;
esac

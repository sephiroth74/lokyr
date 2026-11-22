#!/bin/sh
# 1.4's startup and throughput at rest, as 1.4.0's A/B found the startup's
# gap to the minimal app and `cat`'s throughput past their budgets for
# both versions, and 1.4.0's startup 7.5 ms longer in one of its steps.
# Three copies: 1.3.0's app of its release's DMG, notarized and stapled;
# 1.3.0's tag of a bundle, signed as 1.4.0's and not notarized; and
# 1.4.0's bundle of `release/1.4`. The tag's bundle tells a cost of the
# signature, which the system checks at each start of a copy that is not
# notarized, from one of 1.4's code.
#
# - `sh run.sh prepare` copies the three apps; it measures nothing.
# - `sh run.sh` measures, about 20 minutes, from a terminal, VS Code and
#   every app that can be closed closed, the Mac left alone until it ends:
#   each copy started 40 times, then twelve rounds of the three copies'
#   startups in the orders of a Williams square, each round of the minimal
#   AppKit app of F0 as its floor; then the four workloads' throughput on
#   screen of 1.3.0's app and 1.4.0's, `cat`'s in eight rounds and the
#   others' in four, AB BA BA AB. The same pause before each run.
#
# The config is ab-F3b-20/config.toml's, the releases' of the least work
# beside the terminal's own, smooth scrolling and Blocks off.
set -u
OUT=/tmp/lokyr/rest-F5a-123
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
HERE=$REPO/docs/bench/rest-F5a-123
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
DMG=/tmp/lokyr/bench-F5a-78/dl/Lokyr-1.3.0.dmg
TAG=/tmp/lokyr/wt-v130/target/Lokyr.app
COPIES="v130 tag130 v140"

app() { echo $OUT/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
# The three copies' order in round `$1`, of a Williams square of six rounds,
# each copy as often before and after each other.
order3() {
  case $(( ($1 - 1) % 6 )) in
    0) echo "v130 tag130 v140" ;;
    1) echo "tag130 v140 v130" ;;
    2) echo "v140 v130 tag130" ;;
    3) echo "v140 tag130 v130" ;;
    4) echo "v130 v140 tag130" ;;
    *) echo "tag130 v130 v140" ;;
  esac
}
# The order of round `$1` of the pair `$2 $3`: AB, BA, BA, AB, ...
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
# What runs beside the session, as it starts and as it ends.
conditions() {
  {
    echo "--- $1"
    pmset -g batt | head -2
    uptime
    system_profiler SPDisplaysDataType 2> /dev/null | grep -E "Resolution|Main Display|Refresh|UI Looks"
    top -l 2 -n 12 -o cpu -stats pid,command,cpu | awk '/^PID/ {n++} n == 2'
  } >> $OUT/session.log
}

prepare() {
  mkdir -p $CFG/lokyr
  cp $REPO/docs/bench/ab-F3b-20/config.toml $CFG/lokyr/config.toml
  [ -x $BENCH ] || (cd $REPO && cargo build --release -p lokyr-bench) || return 1
  [ -d $TAG ] || { log "no bundle of 1.3.0's tag at $TAG"; return 1; }
  [ -d $REPO/target/Lokyr.app ] || { log "no bundle of release/1.4"; return 1; }
  [ -f $DMG ] || gh release download v1.3.0 --repo sephiroth74/lokyr --pattern 'Lokyr-1.3.0.dmg' \
    --dir "$(dirname $DMG)" || return 1
  mount=$(hdiutil attach -nobrowse -readonly $DMG | awk -F'\t' '/\/Volumes\// {print $NF}')
  [ -n "$mount" ] || return 1
  rm -rf $OUT/v130 $OUT/tag130 $OUT/v140
  mkdir -p $OUT/v130 $OUT/tag130 $OUT/v140
  ditto "$mount/Lokyr.app" $OUT/v130/Lokyr.app
  hdiutil detach -quiet "$mount"
  ditto $TAG $OUT/tag130/Lokyr.app
  ditto $REPO/target/Lokyr.app $OUT/v140/Lokyr.app
  # 1.4.0's code still of the version 1.3.0, which its release raises.
  log "v140 of $(cd $REPO && git rev-parse --short HEAD)"
  for v in $COPIES; do
    xattr -dr com.apple.quarantine "$(app $v)" 2> /dev/null
    if xcrun stapler validate -q "$(app $v)" > /dev/null 2>&1; then stapled=stapled; else stapled="not stapled"; fi
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist")), $stapled"
  done
}

# Each copy started `$1` times, its window drawn, before the rounds, as a
# binary that the system has seen little of starts some 10 ms slower.
season() {
  for v in $COPIES; do
    log "$1 starts of $v"
    for i in $(seq 1 "$1"); do
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench startup > /dev/null 2>&1
    done
  done
}
startups() {
  for round in $(seq 1 "$1"); do
    for v in $(order3 $round); do
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
}
# The workload `$1`'s throughput on screen, of the rounds 1 to `$2`.
throughput() {
  for round in $(seq 1 "$2"); do
    for v in $(order $round v130 v140); do
      sleep 2
      log "throughput $1 $v $round"
      rm -f $OUT/onscreen-$1-$v-$round.jsonl
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$1.txt $OUT/onscreen-$1-$v-$round.jsonl 2 \
        > $OUT/frames-$1-$v-$round.json
    done
  done
  load
}

measure() {
  for v in $COPIES; do
    [ -d "$(app $v)" ] || { echo "no $v: sh $HERE/run.sh prepare first" >&2; return 1; }
  done
  if pgrep -f "Visual Studio Code.app/Contents/MacOS" > /dev/null; then
    echo "VS Code is open: close it, then run this again" >&2
    return 1
  fi
  echo "about 20 minutes: leave the Mac alone until it ends; starting in 10 s"
  sleep 10
  # The display kept awake, as the startup waits for its first frame on it.
  caffeinate -d -i -w $$ &
  log "session"
  conditions start
  season 40
  startups 12
  throughput ascii 8
  throughput seq 4
  throughput sgr 4
  throughput cjk 4
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

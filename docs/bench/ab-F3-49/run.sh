#!/bin/sh
# The A/B after F3-49: 0.5.0's app, of its release's DMG, against the app
# of the build of now, in one session of balanced orders, as F3-40's: the
# startup in four rounds, AB BA BA AB, each of the minimal AppKit app of F0
# as its floor; the latency in eight, after a warm-up of each; idle and
# memory in two; each throughput's workload in two. Then the cost of a
# single tab's title that the tabs plugin draws in the titlebar: the build
# of now with the plugin and without it, `[plugins.tabs] enabled = false`,
# of the startup in two rounds, the latency in four, and the frames of a
# program that changes the window's title 100 times, 20 a second, in two;
# and the frames of a program's progress, OSC 9;4, 50 changes, 10 a
# second, which the window server slides.
set -u
OUT=/tmp/lokyr/bench-F3-49
CFG=$OUT/config
NOTABS=$OUT/config-notabs
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
app() {
  case $1 in
    v050) echo /tmp/lokyr/bench-v050/Lokyr.app ;;
    cur|notabs) echo /tmp/lokyr/bench-cur/Lokyr.app ;;
  esac
}
config() {
  case $1 in
    notabs) echo $NOTABS ;;
    *) echo $CFG ;;
  esac
}
# The order of round `$1` of the pair `$2 $3`: AB, BA, BA, AB, ...
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
mkdir -p $CFG/lokyr $NOTABS/lokyr
cp $REPO/docs/bench/ab-F3-40/config.toml $CFG/lokyr/config.toml
cp $REPO/docs/bench/ab-F3-40/config.toml $NOTABS/lokyr/config.toml
printf '\n[plugins.tabs]\nenabled = false\n' >> $NOTABS/lokyr/config.toml
pmset -g batt | head -2 >> $OUT/session.log
uptime >> $OUT/session.log
system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$" >> $OUT/session.log
startup() {
  log "startup $1 $2"
  $BENCH startup --app "$(app $1)" --first-frame --env XDG_CONFIG_HOME=$(config $1) > $OUT/startup-$1-$2.json
}
latency() {
  log "latency $1 $2 place $3"
  XDG_CONFIG_HOME=$(config $1) "$(app $1)/Contents/MacOS/lokyr" --bench latency --keys 200 > $OUT/latency-$1-$2.json
}
# The A/B of 0.5.0 and now.
for round in 1 2 3 4; do
  for v in $(order $round v050 cur); do startup $v $round; done
  log "startup floor $round"
  $BENCH startup --app $SPIKE > $OUT/startup-floor-$round.json
  load
done
for v in v050 cur notabs; do
  log "latency warm-up $v"
  XDG_CONFIG_HOME=$(config $v) "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
done
for round in 1 2 3 4 5 6 7 8; do
  place=0
  for v in $(order $round v050 cur); do place=$((place + 1)); latency $v $round $place; done
  load
done
for round in 1 2; do
  for v in $(order $round v050 cur); do
    log "idle $v $round"
    before=$(pgrep -f "$(app $v)/Contents/MacOS/lokyr")
    open -n --env XDG_CONFIG_HOME=$(config $v) "$(app $v)"
    sleep 3
    pid=$(pgrep -nf "$(app $v)/Contents/MacOS/lokyr")
    [ -n "$pid" ] && [ "$pid" != "$before" ] || { log "no pid for $v"; continue; }
    $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$v-$round.json
    sleep 18
    $BENCH idle --pid $pid --duration 60 > $OUT/idle-$v-$round.json
    $BENCH memory --pid $pid --duration 10 > $OUT/memory-$v-$round.json
    kill $pid; sleep 2
  done
  load
done
for file in ascii seq sgr cjk; do
  for round in 1 2; do
    for v in $(order $round v050 cur); do
      log "throughput $file $v $round"
      rm -f $OUT/onscreen-$file-$v-$round.jsonl
      XDG_CONFIG_HOME=$(config $v) "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1920x1080 -- \
        python3 $ONSCREEN $W/$file.txt $OUT/onscreen-$file-$v-$round.jsonl 2 > $OUT/frames-$file-$v-$round.json
    done
  done
  load
done
# The single tab's title: the build of now with the tabs plugin and
# without it.
for round in 5 6; do
  for v in $(order $round cur notabs); do startup $v $round; done
  load
done
for round in 9 10 11 12; do
  place=0
  for v in $(order $round cur notabs); do place=$((place + 1)); latency $v $round $place; done
  load
done
TITLES='i=0; while [ $i -lt 100 ]; do printf "\033]2;title %s\007" $i; sleep 0.05; i=$((i+1)); done'
for round in 1 2; do
  for v in $(order $round cur notabs); do
    log "titles $v $round"
    XDG_CONFIG_HOME=$(config $v) "$(app $v)/Contents/MacOS/lokyr" --bench output --window 1200x800 -- \
      sh -c "$TITLES" > $OUT/titles-$v-$round.json
  done
done
# A program's progress, which the tab's outline and the window's strip
# show, of the window server's slides.
PROGRESS='i=0; while [ $i -lt 50 ]; do printf "\033]9;4;1;%s\007" $((i * 2)); sleep 0.1; i=$((i+1)); done; printf "\033]9;4;0\007"'
for round in 1 2; do
  log "progress cur $round"
  XDG_CONFIG_HOME=$CFG "$(app cur)/Contents/MacOS/lokyr" --bench output --window 1200x800 -- \
    sh -c "$PROGRESS" > $OUT/progress-cur-$round.json
done
log done

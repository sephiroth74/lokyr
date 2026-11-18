#!/bin/sh
# The A/B of 1.3.0: 1.2.0's app, of its release's DMG, against 1.3.0's, of
# its bundle, as 1.2.0's session, in steps of ten minutes at most, so that
# the Mac is free between them. `sh run.sh` runs the next step not done
# yet, `sh run.sh list` lists the steps and those done, and
# `sh run.sh <step>` runs a step again. Each step holds its own balanced
# orders, AB BA BA AB, and the same pause before each run, so that a step
# compares its two copies of its own conditions; nothing should run beside
# it but the step, and nothing touch the mouse while a motion step records.
#
# - prepare: lokyr-bench, the workloads, 1.3.0's bundle and 1.2.0's app of
#   its DMG; it measures nothing, and the Mac stays free.
# - startup: each copy started 30 times, then four rounds, each of the
#   minimal AppKit app of F0 as its floor.
# - startup-again: 30 more starts each, then eight rounds.
# - latency: a warm-up of each, then eight rounds.
# - idle: idle over 60 s, memory and threads, in two rounds.
# - memory: memory at rest of vmmap's summaries in four rounds, then each
#   throughput's workload in two.
# - motion-1 to motion-4: the window dragged and resized, of each copy, of
#   one look and one round each: the recordings first, the mouse left alone,
#   then their figures, the Mac free again.
#
# The config is ab-F3b-20/config.toml's, the releases' of the least work
# beside the terminal's own; the motion's, F3-152's.
set -u
OUT=/tmp/lokyr/bench-F5a-17
CFG=$OUT/config
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
HERE=$REPO/docs/bench/ab-F5a-17
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
STEPS="prepare startup startup-again latency idle memory motion-1 motion-2 motion-3 motion-4"
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
    prepare) echo "10 min, the Mac free" ;;
    startup) echo "4 min" ;;
    startup-again) echo "7 min" ;;
    latency) echo "4 min" ;;
    idle) echo "7 min" ;;
    memory) echo "6 min" ;;
    motion-*) echo "2 min of the mouse left alone, then 10 min of figures, the Mac free" ;;
  esac
}
conditions() {
  {
    echo "--- $1"
    pmset -g batt | head -2
    uptime
    system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh"
  } >> $OUT/session.log
}

prepare() {
  mkdir -p $CFG/lokyr $OUT/dl
  cp $REPO/docs/bench/ab-F3b-20/config.toml $CFG/lokyr/config.toml
  log "lokyr-bench"
  (cd $REPO && cargo build --release -p lokyr-bench) || return 1
  if [ ! -f $W/cjk.txt ]; then
    log "workloads"
    mkdir -p $W
    python3 -I $REPO/tools/bench/workloads.py $W || return 1
  fi
  log "1.3.0's bundle of $(cd $REPO && git rev-parse --short HEAD)"
  rm -rf $REPO/target/Lokyr.app
  (
    cd $REPO || exit 1
    set -a
    . ./private/lokyr.env
    set +a
    cargo xtask bundle 2>&1 | sed -E 's/signed by .*//'
  )
  [ -d $REPO/target/Lokyr.app ] || { log "no bundle"; return 1; }
  rm -rf $OUT/v130 && mkdir -p $OUT/v130
  ditto $REPO/target/Lokyr.app $OUT/v130/Lokyr.app || return 1
  log "1.2.0's app of its DMG"
  if [ ! -f $OUT/dl/Lokyr-1.2.0.dmg ]; then
    gh release download v1.2.0 --repo sephiroth74/lokyr --pattern 'Lokyr-1.2.0.dmg' --dir $OUT/dl \
      || return 1
  fi
  mount=$(hdiutil attach -nobrowse -readonly $OUT/dl/Lokyr-1.2.0.dmg | awk -F'\t' '/\/Volumes\// {print $NF}')
  [ -n "$mount" ] || return 1
  rm -rf $OUT/v120 && mkdir -p $OUT/v120
  ditto "$mount/Lokyr.app" $OUT/v120/Lokyr.app
  hdiutil detach -quiet "$mount"
  xattr -dr com.apple.quarantine $OUT/v120/Lokyr.app $OUT/v130/Lokyr.app 2> /dev/null
  for v in v120 v130; do
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist"))"
  done
}

# Each copy started `$1` times, its window drawn, before the rounds, as a
# binary that the system has seen little of starts some 10 ms slower.
season() {
  for v in v120 v130; do
    log "$1 starts of $v"
    for i in $(seq 1 "$1"); do
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench startup > /dev/null 2>&1
    done
  done
}
startups() {
  prefix=$1 rounds=$2
  for round in $(seq 1 "$rounds"); do
    for v in $(order $round v120 v130); do
      sleep 2
      log "startup $v $round"
      $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
        > $OUT/$prefix-$v-$round.json
    done
    sleep 2
    log "startup floor $round"
    $BENCH startup --app $SPIKE > $OUT/$prefix-floor-$round.json
    load
  done
}
startup() {
  season 30
  startups startup 4
}
startup_again() {
  season 30
  startups startup-again 8
}

latency() {
  for v in v120 v130; do
    log "latency warm-up $v"
    XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 20 > /dev/null
  done
  for round in 1 2 3 4 5 6 7 8; do
    for v in $(order $round v120 v130); do
      sleep 2
      log "latency $v $round"
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench latency --keys 200 \
        > $OUT/latency-$v-$round.json
    done
    load
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
idle() {
  for round in 1 2; do
    for v in $(order $round v120 v130); do
      name=$v-$round
      log "idle $name"
      started "$(app $v)" || { log "no pid for $name"; continue; }
      $BENCH memory --pid $pid --interval 20 --duration 2 > $OUT/memory-start-$name.json
      sleep 18
      ps -M -p $pid | tail -n +2 | wc -l | tr -d ' ' > $OUT/threads-$name.txt
      $BENCH idle --pid $pid --duration 60 > $OUT/idle-$name.json
      $BENCH memory --pid $pid --duration 10 > $OUT/memory-$name.json
      kill $pid
      sleep 2
    done
    load
  done
}
memory() {
  for round in 1 2 3 4; do
    for v in $(order $round v120 v130); do
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
      for v in $(order $round v120 v130); do
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

# The motion's configs: Menlo 20, 88 by 27 cells, opaque and of a cursor
# that switches, or translucent at 0.95 with a blur of 10 and the cursor's
# fades, as F3-152's.
motion_config() {
  look=$1 opacity=$2 blur=$3 fades=$4
  mkdir -p $OUT/motion/config-$look/lokyr
  {
    printf '[app]\nrestore-windows = "never"\n'
    printf '[plugins.quick-tips]\nenabled = false\n'
    printf '[font]\nfamily = "Menlo"\nsize = 20\n'
    printf '[window]\ncolumns = 88\nrows = 27\nremember-frame = false\n'
    printf 'background-opacity = %s\nbackground-blur = %s\n' "$opacity" "$blur"
    if [ "$fades" = fade ]; then
      printf '[cursor]\nblink = true\nfade-in = 100\nvisible = 500\nfade-out = 500\nhidden = 100\n'
    else
      printf '[cursor]\nblink = true\nfade-in = 0\nvisible = 600\nfade-out = 0\nhidden = 600\n'
    fi
  } > $OUT/motion/config-$look/lokyr/config.toml
}
# The copy `$1` of the look `$2` started, its window found by the bench.
launch() {
  XDG_CONFIG_HOME=$OUT/motion/config-$2 "$(app $1)/Contents/MacOS/lokyr" > /dev/null 2>&1 &
  pid=$!
  tries=0
  while ! $BENCH motion --pid "$pid" --kind drag --seconds 0 > /dev/null 2> $OUT/motion/launch.err; do
    tries=$((tries + 1))
    if [ $tries -ge 100 ]; then
      log "no window of $1 $2: $(cat $OUT/motion/launch.err)"
      kill "$pid"
      return 1
    fi
    sleep 0.1
  done
  sleep 2
}
# Records `$4`, a drag or a resize, of the window of `pid`, as `$1-$2-$3`.
record() {
  base=$OUT/motion/$1-$2-$4-$3
  rm -rf $base.trace
  xcrun xctrace record --template 'Animation Hitches' --all-processes --time-limit 13s \
    --output $base.trace > $base.xctrace.log 2>&1 &
  recording=$!
  sleep 1.5
  $BENCH motion --pid "$pid" --kind "$4" --seconds 10 > $base.json
  echo "$pid" > $base.pid
  wait $recording
}
# Round `$1` of the look `$2`, the copies in the order `$3`: every
# recording first, the mouse left alone, then each one's figures, and the
# trace removed as its figures are taken.
motion() {
  round=$1 look=$2 copies=$3
  mkdir -p $OUT/motion
  motion_config opaque 1 0 switch
  motion_config blur 0.95 10 fade
  # The mouse's synthetic events take the Accessibility permission of the
  # terminal that runs the script, which the bench checks first.
  if $BENCH motion --pid $$ --kind drag --seconds 0 2>&1 | grep -q Accessibility; then
    echo "give this terminal the Accessibility permission, in System Settings, Privacy & Security, Accessibility, then run the step again" >&2
    return 1
  fi
  echo "leave the mouse alone until the recordings end"
  for v in $copies; do
    log "motion $v $look $round"
    launch $v $look || continue
    record $v $look $round drag
    sleep 1
    record $v $look $round resize
    kill "$pid"
    sleep 2
  done
  log "recordings done: the Mac is free, the figures follow"
  for base in $OUT/motion/*-$look-*-$round.trace; do
    [ -e "$base" ] || continue
    stem=${base%.trace}
    python3 -I $REPO/tools/bench/motion_extract.py "$base" "$stem.json" "$(cat $stem.pid)" \
      > "$stem.figures.json" 2> "$stem.extract.log"
    rm -rf "$base"
  done
}

run_step() {
  step=$1
  case $step in
    prepare | startup | startup-again | latency | idle | memory | motion-1 | motion-2 | motion-3 | motion-4) ;;
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
    startup) startup ;;
    startup-again) startup_again ;;
    latency) latency ;;
    idle) idle ;;
    memory) memory ;;
    motion-1) motion 1 opaque "v120 v130" ;;
    motion-2) motion 1 blur "v130 v120" ;;
    motion-3) motion 2 blur "v120 v130" ;;
    motion-4) motion 2 opaque "v130 v120" ;;
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
      printf '%-14s %-5s about %s\n' "$step" "$mark" "$(minutes $step)"
    done
    ;;
  next)
    for step in $STEPS; do
      grep -qx "$step" $DONE 2> /dev/null && continue
      echo "next: $step, about $(minutes $step)"
      if [ "$step" != prepare ]; then
        echo "starting in 10 s: close what you can, and leave the Mac alone until it ends"
        sleep 10
      fi
      run_step "$step" || exit 1
      for later in $STEPS; do
        grep -qx "$later" $DONE 2> /dev/null && continue
        echo "the Mac is free; the next step, $later, about $(minutes $later): sh $HERE/run.sh"
        exit 0
      done
      echo "every step done"
      exit 0
    done
    echo "every step done: sh $HERE/run.sh <step> runs one again"
    ;;
  *) run_step "$1" ;;
esac

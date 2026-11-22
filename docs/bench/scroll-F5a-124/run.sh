#!/bin/sh
# The scroll by pixels at 120 Hz: 1.4.0's A/B's two steps of a trackpad's
# scroll, of the built-in display at 120 Hz, the lid open, as that A/B's
# display ran at 60 Hz alone. `lokyr-bench motion --kind scroll` swipes and
# lets the momentum slow, 10 s each, over the window of 1.3.0 by rows, of
# 1.4.0 by rows and of 1.4.0 by pixels, `[window] smooth-scroll`, a round
# each, in two orders: the recordings first, then idle over 30 s of each
# copy at rest where its scroll left it, then the figures.
#
# - `sh run.sh scroll-1` and `sh run.sh scroll-2`: about 2 minutes of the
#   mouse and the trackpad left alone, then 5 minutes of figures.
# - `sh run.sh hand <n>`: 1.4.0's copy of temporary logs, by pixels, open
#   for 25 s while a hand scrolls over it on the trackpad, its log kept as
#   `hand-<n>.log`, which `hand.py` reads.
#
# The copies are F5a-123's session's, 1.3.0 of its DMG and 1.4.0's bundle;
# the windows open on the main display, which has to be the built-in one.
# The refresh is `HZ`'s, 120 by default, as the system's profile tells no
# rate of a display of a variable refresh.
set -u
OUT=/tmp/lokyr/scroll-F5a-124
COPIES=/tmp/lokyr/rest-F5a-123
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
BENCH=$REPO/target/release/lokyr-bench
HZ=${HZ:-120}

app() { echo $COPIES/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
conditions() {
  {
    echo "--- $1, $HZ Hz"
    pmset -g batt | head -2
    pmset -g | grep -E "powermode|lowpowermode"
    uptime
    system_profiler SPDisplaysDataType 2> /dev/null | grep -E ":$|Resolution|Main Display|UI Looks|Connection"
  } >> $OUT/session.log
}
# The motion's configs of 1.4.0's A/B: Menlo 20, 88 by 27 cells, opaque, no
# scroll bar, by rows or by pixels, smooth scrolling on for the latter
# alone, Blocks off by default; and a zsh of the session's own `.zshrc`, of
# `ZDOTDIR`, which prints 50 000 rows at its start, so that the view has a
# scrollback.
scroll_config() {
  config_way=$1 config_smooth=$2
  mkdir -p $OUT/config-$config_way/lokyr $OUT/zdot
  {
    printf '[app]\nrestore-windows = "never"\n'
    printf '[plugins.quick-tips]\nenabled = false\n'
    printf '[font]\nfamily = "Menlo"\nsize = 20\n'
    printf '[window]\ncolumns = 88\nrows = 27\nremember-frame = false\nscrollbar = "never"\n'
    [ "$config_smooth" = true ] && printf 'smooth-scroll = true\n'
    printf '[cursor]\nblink = false\n'
  } > $OUT/config-$config_way/lokyr/config.toml
  printf 'seq 1 50000\nPS1="$ "\n' > $OUT/zdot/.zshrc
}
# The copy `$1` started of the way `$2`, its window found by the bench, its
# rows printed.
scroll_launch() {
  ZDOTDIR=$OUT/zdot XDG_CONFIG_HOME=$OUT/config-$2 "$(app $1)/Contents/MacOS/lokyr" > /dev/null 2>&1 &
  pid=$!
  tries=0
  while ! $BENCH motion --pid "$pid" --kind drag --seconds 0 > /dev/null 2> $OUT/launch.err; do
    tries=$((tries + 1))
    if [ $tries -ge 100 ]; then
      log "no window of $1 $2: $(cat $OUT/launch.err)"
      kill "$pid"
      return 1
    fi
    sleep 0.1
  done
  sleep 4
}
# xctrace's recording `$recording` waited for up to five minutes past its
# start: a save that hangs is killed, and the caller makes it again.
ended() {
  waited=0
  while kill -0 $recording 2> /dev/null; do
    if [ $waited -ge 300 ]; then
      kill -INT $recording 2> /dev/null
      sleep 5
      kill -KILL $recording 2> /dev/null
      wait $recording 2> /dev/null
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  wait $recording
  grep -q "Output file saved" $1
}
# Round `$1` of the copies and ways `$2`, each `<copy>:<way>`.
scroll() {
  round=$1 runs=$2
  scroll_config rows false
  scroll_config pixels true
  if $BENCH motion --pid $$ --kind drag --seconds 0 2>&1 | grep -q Accessibility; then
    echo "give this terminal the Accessibility permission, in System Settings, Privacy & Security, Accessibility, then run the step again" >&2
    return 1
  fi
  echo "leave the mouse and the trackpad alone until the recordings end"
  for run in $runs; do
    v=${run%%:*} way=${run#*:}
    base=$OUT/$v-$way-$round
    log "scroll $v $way $round"
    scroll_launch $v $way || continue
    for try in 1 2; do
      rm -rf $base.trace
      xcrun xctrace record --template 'Animation Hitches' --all-processes --time-limit 16s \
        --output $base.trace > $base.xctrace.log 2>&1 &
      recording=$!
      sleep 1.5
      $BENCH motion --pid "$pid" --kind scroll --seconds 10 --amplitude 600 > $base.json
      echo "$pid" > $base.pid
      ended $base.xctrace.log && break
      log "recording $base hung, try $try"
      rm -rf $base.trace
      sleep 2
    done
    sleep 2
    $BENCH idle --pid $pid --duration 30 > $OUT/idle-$v-$way-$round.json
    kill "$pid"
    sleep 2
  done
  log "recordings done: the Mac is free, the figures follow"
  refresh_ms=$(python3 -c "print(1000 / $HZ)")
  for base in $OUT/*-$round.trace; do
    [ -e "$base" ] || continue
    stem=${base%.trace}
    python3 -I $REPO/tools/bench/motion_extract.py "$base" "$stem.json" "$(cat $stem.pid)" "$refresh_ms" \
      > "$stem.figures.json" 2> "$stem.extract.log"
    rm -rf "$base"
  done
}
# A scroll by hand over 1.4.0's copy of temporary logs, by pixels, of a
# home of its own, as the app writes its log there.
hand() {
  scroll_config pixels true
  home=$OUT/vlog/home
  mkdir -p $home
  rm -f $home/Library/Logs/Lokyr/lokyr*.log
  HOME=$home ZDOTDIR=$OUT/zdot XDG_CONFIG_HOME=$OUT/config-pixels \
    $OUT/vlog/Lokyr.app/Contents/MacOS/lokyr > /dev/null 2>&1 &
  pid=$!
  log "hand $1: scroll over the window for 25 s"
  sleep 25
  kill $pid
  sleep 1
  cp $home/Library/Logs/Lokyr/lokyr.log $OUT/hand-$1.log
  log "hand $1: $(grep -c 'scroll evts' $OUT/hand-$1.log) events"
}

mkdir -p $OUT
case ${1:-} in
  scroll-1 | scroll-2)
    conditions "$1"
    log "step $1"
    case $1 in
      scroll-1) scroll 1 "v130:rows v140:rows v140:pixels" ;;
      scroll-2) scroll 2 "v140:pixels v140:rows v130:rows" ;;
    esac
    log "step $1 done"
    ;;
  hand)
    conditions "hand ${2:-1}"
    hand "${2:-1}"
    ;;
  *)
    echo "sh run.sh scroll-1|scroll-2|hand <n>" >&2
    exit 1
    ;;
esac

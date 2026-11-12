#!/bin/sh
# F3-152's session: each configuration's window dragged by its titlebar,
# then resized by its corner, 10 s each along the same path, by the mouse's
# synthetic events of `lokyr-bench motion`, while Instruments' Animation
# Hitches records every process. Lokyr opaque, translucent and translucent
# with a blur of 10, each with the cursor's fades (100, 500, 500 and 100 ms,
# the user's) and without (a switch, 600 and 600 ms, the same cycle);
# Ghostty in the same three, of the options of its command line alone. Both
# of Menlo 20, 88 by 27 cells. Two rounds, the second in the reverse order.
# Nothing else should take the mouse meanwhile.
set -u
OUT=/tmp/lokyr/motion-F3-152
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
LOKYR=$REPO/target/Lokyr.app/Contents/MacOS/lokyr
GHOSTTY=/Applications/Ghostty.app
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $OUT
config() {
  name=$1 opacity=$2 blur=$3 fades=$4
  mkdir -p $OUT/config-$name/lokyr
  {
    printf '[font]\nfamily = "Menlo"\nsize = 20\n'
    printf '[window]\ncolumns = 88\nrows = 27\nremember-frame = false\n'
    printf 'background-opacity = %s\nbackground-blur = %s\n' "$opacity" "$blur"
    if [ "$fades" = fade ]; then
      printf '[cursor]\nblink = true\nfade-in = 100\nvisible = 500\nfade-out = 500\nhidden = 100\n'
    else
      printf '[cursor]\nblink = true\nfade-in = 0\nvisible = 600\nfade-out = 0\nhidden = 600\n'
    fi
  } > $OUT/config-$name/lokyr/config.toml
}
config lokyr-opaque-switch 1 0 switch
config lokyr-opaque-fade 1 0 fade
config lokyr-translucent-switch 0.9 0 switch
config lokyr-translucent-fade 0.9 0 fade
config lokyr-blur-switch 0.9 10 switch
config lokyr-blur-fade 0.9 10 fade
launch() {
  case $1 in
    lokyr-*)
      XDG_CONFIG_HOME=$OUT/config-$1 $LOKYR > /dev/null 2>&1 &
      pid=$!
      ;;
    ghostty-*)
      case $1 in
        ghostty-opaque) flags="--background-opacity=1 --background-blur=false" ;;
        ghostty-translucent) flags="--background-opacity=0.9 --background-blur=false" ;;
        ghostty-blur) flags="--background-opacity=0.9 --background-blur=10" ;;
      esac
      open -na $GHOSTTY --args --config-default-files=false --font-family=Menlo --font-size=20 \
        --window-width=88 --window-height=27 --cursor-style=block --cursor-style-blink=true \
        --macos-titlebar-style=native $flags
      sleep 1
      pid=$(pgrep -n -f "$GHOSTTY/Contents/MacOS/ghostty")
      ;;
  esac
  tries=0
  while ! $BENCH motion --pid "$pid" --kind drag --seconds 0 > /dev/null 2>&1; do
    tries=$((tries + 1))
    if [ $tries -ge 100 ]; then
      log "no window of $1"
      return 1
    fi
    sleep 0.1
  done
  sleep 2
}
measure() {
  name=$1 round=$2 kind=$3
  base=$OUT/$name-$kind-$round
  rm -rf $base.trace
  xcrun xctrace record --template 'Animation Hitches' --all-processes --time-limit 13s \
    --output $base.trace > $base.xctrace.log 2>&1 &
  recording=$!
  sleep 1.5
  $BENCH motion --pid "$pid" --kind "$kind" --seconds 10 > $base.json
  echo "$pid" > $base.pid
  wait $recording
}
uptime >> $OUT/session.log
round=0
for order in \
  "lokyr-opaque-switch lokyr-opaque-fade lokyr-translucent-switch lokyr-translucent-fade lokyr-blur-switch lokyr-blur-fade ghostty-opaque ghostty-translucent ghostty-blur" \
  "ghostty-blur ghostty-translucent ghostty-opaque lokyr-blur-fade lokyr-blur-switch lokyr-translucent-fade lokyr-translucent-switch lokyr-opaque-fade lokyr-opaque-switch"; do
  round=$((round + 1))
  for name in $order; do
    log "$name $round"
    launch "$name" || continue
    measure "$name" $round drag
    sleep 1
    measure "$name" $round resize
    kill "$pid"
    sleep 2
  done
done
uptime >> $OUT/session.log
log done

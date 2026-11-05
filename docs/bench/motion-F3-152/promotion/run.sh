#!/bin/sh
# F3-153's check at 120 Hz, of the built-in display as the main one, in
# ProMotion, so that every window opens on it: a resize of Lokyr and of
# Ghostty, opaque, and a drag of each, blurred, Lokyr of the cursor's fades,
# 10 s each, of the mouse's events at 240 Hz, one recording at a time.
set -u
OUT=/tmp/lokyr/pro; rm -rf $OUT; mkdir -p $OUT
BENCH=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr/target/release/lokyr-bench
LOKYR=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr/target/Lokyr.app/Contents/MacOS/lokyr
CFG=/tmp/lokyr/motion-F3-152
launch() {
  case $1 in
    lokyr-*) XDG_CONFIG_HOME=$CFG/config-$1 $LOKYR > /dev/null 2>&1 & pid=$! ;;
    ghostty-*)
      case $1 in ghostty-opaque) flags="--background-opacity=1 --background-blur=false" ;; ghostty-blur) flags="--background-opacity=0.9 --background-blur=10" ;; esac
      open -na /Applications/Ghostty.app --args --config-default-files=false --font-family=Menlo --font-size=20 --window-width=88 --window-height=27 --cursor-style=block --cursor-style-blink=true --macos-titlebar-style=native $flags
      sleep 1; pid=$(pgrep -n -f "/Applications/Ghostty.app/Contents/MacOS/ghostty") ;;
  esac
  for i in $(seq 1 100); do $BENCH motion --pid "$pid" --kind drag --seconds 0 > /dev/null 2>&1 && break; sleep 0.1; done
  sleep 2
}
run() {
  name=$1 kind=$2; base=$OUT/$name-$kind
  launch $name
  xcrun xctrace record --template 'Animation Hitches' --all-processes --time-limit 13s --output $base.trace > $base.xlog 2>&1 & rec=$!
  sleep 3
  $BENCH motion --pid "$pid" --kind $kind --seconds 10 --rate 240 > $base.json
  echo $pid > $base.pid
  wait $rec
  kill $pid; sleep 2
  echo "$(date +%T) $name $kind done" >> $OUT/log
}
run lokyr-opaque-switch resize
run ghostty-opaque resize
run lokyr-blur-fade drag
run ghostty-blur drag
echo done >> $OUT/log

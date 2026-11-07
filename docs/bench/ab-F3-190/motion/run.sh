#!/bin/sh
# The window's motion of 0.5.0's app against 0.5.1's, as F3-152's
# session: each window dragged by its titlebar, then resized by its corner,
# 10 s each along the same path, by the mouse's synthetic events of
# `lokyr-bench motion`, while Instruments' Animation Hitches records every
# process; of F3-152's config, Menlo 20, 88 by 27 cells, opaque and of a
# cursor that switches, and translucent at 0.95 with a blur of 10 and the
# cursor's fades, as the user's. Two rounds, the second in the reverse
# order. Nothing else should take the mouse meanwhile.
set -u
OUT=/tmp/lokyr/motion-F3-190
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
BENCH=$REPO/target/release/lokyr-bench
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $OUT
binary() {
  case $1 in
    v050-*) echo /tmp/lokyr/bench-v050/Lokyr.app/Contents/MacOS/lokyr ;;
    v051-*) echo /tmp/lokyr/bench-v051/Lokyr.app/Contents/MacOS/lokyr ;;
  esac
}
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
config opaque 1 0 switch
config blur 0.95 10 fade
launch() {
  look=${1#*-}
  XDG_CONFIG_HOME=$OUT/config-$look $(binary $1) > /dev/null 2>&1 &
  pid=$!
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
for order in "v050-opaque v051-opaque v050-blur v051-blur" "v051-blur v050-blur v051-opaque v050-opaque"; do
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

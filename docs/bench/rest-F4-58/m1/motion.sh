#!/bin/sh
# The window's motion of 0.7.0's app against 0.8.0's, as the managed Mac's
# session tried it: each window dragged by its titlebar, then resized by its
# corner, 10 s each along the same path, by the mouse's synthetic events of
# `lokyr-bench motion`, while Instruments' Animation Hitches records every
# process; opaque, and translucent at 0.95 with a blur of 10 and the
# cursor's fades. Two rounds, the second in the reverse order; each
# recording's figures taken and its trace removed before the next.
#
# Run in Terminal, `open -a Terminal ~/lokyr-bench/motion.sh`, as the apps
# and the events must be of the desktop's session and Terminal is the app
# that Accessibility allows. `motion.sh try` runs one drag alone.
set -u
K=$HOME/lokyr-bench
OUT=$K/results/motion
BENCH=$K/bin/lokyr-bench
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
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
config opaque 1 0 switch
config blur 0.95 10 fade
launch() {
  version=${1%%-*} look=${1#*-}
  XDG_CONFIG_HOME=$OUT/config-$look $K/apps/$version/Lokyr.app/Contents/MacOS/lokyr > /dev/null 2>&1 &
  pid=$!
  tries=0
  while ! $BENCH motion --pid "$pid" --kind drag --seconds 0 > /dev/null 2> $OUT/launch.err; do
    tries=$((tries + 1))
    if [ $tries -ge 100 ]; then
      log "no window of $1: $(cat $OUT/launch.err)"
      kill "$pid"
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
  python3 -I $K/motion/motion_extract.py $base.trace $base.json "$pid" > $base.figures.json 2> $base.extract.log
  rm -rf $base.trace
  sleep 5
}
caffeinate -dimsu -w $$ &
uptime >> $OUT/session.log
if [ "${1:-}" = try ]; then
  log "try v080-opaque"
  launch v080-opaque && measure v080-opaque 0 drag && kill "$pid"
  log "try done"
  exit 0
fi
round=0
for order in "v070-opaque v080-opaque v070-blur v080-blur" "v080-blur v070-blur v080-opaque v070-opaque"; do
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

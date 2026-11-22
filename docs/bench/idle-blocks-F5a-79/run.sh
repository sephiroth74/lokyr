#!/bin/sh
# Blocks at rest: 1.4.0's bundle of the releases' config, Blocks on and off,
# its zsh running a command of its own at the first prompt (`zshrc`, of
# `ZDOTDIR`), so that a block has ended; 20 s after the start, idle over
# 60 s and the memory over 10 s, in four rounds of the orders on, off, off,
# on, each window opened in the background so that it takes no focus.
set -u
OUT=/tmp/lokyr/blocks-idle
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr-workspace/lokyr
HERE=$REPO/docs/bench/idle-blocks-F5a-79
BENCH=$REPO/target/release/lokyr-bench
APP=/tmp/lokyr/rest-F5a-123/v140/Lokyr.app
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
mkdir -p $OUT/config-on/lokyr $OUT/config-off/lokyr $OUT/zdot
cp $REPO/docs/bench/ab-F3b-20/config.toml $OUT/config-off/lokyr/config.toml
{ cat $REPO/docs/bench/ab-F3b-20/config.toml; printf '\n[plugins.blocks]\nenabled = true\n'; } \
  > $OUT/config-on/lokyr/config.toml
cp $HERE/zshrc $OUT/zdot/.zshrc
for round in 1 2 3 4; do
  case $round in 1|4) order="on off" ;; *) order="off on" ;; esac
  for way in $order; do
    name=$way-$round
    log "idle $name"
    before=$(pgrep -nf "$APP/Contents/MacOS/lokyr")
    open -g -n --env XDG_CONFIG_HOME=$OUT/config-$way --env ZDOTDIR=$OUT/zdot $APP
    sleep 3
    pid=$(pgrep -nf "$APP/Contents/MacOS/lokyr")
    if [ -z "$pid" ] || [ "$pid" = "$before" ]; then log "no pid for $name"; continue; fi
    sleep 17
    $BENCH idle --pid $pid --duration 60 > $OUT/idle-$name.json
    $BENCH memory --pid $pid --duration 10 > $OUT/memory-$name.json
    kill $pid
    sleep 2
  done
done
log done

#!/bin/sh
# The fix of the first frame's tab label against the build before it: the
# startup to the first frame and to the window on screen, of the tabs plugin
# and without it, in rounds of balanced orders.
set -u
OUT=${OUT:-/tmp/lokyr/bench-F3-49/fix}
B=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr/target/release/lokyr-bench
D=/tmp/lokyr/bench-F3-49
mkdir -p $OUT
uptime > $OUT/session.log
app() { case $1 in cur*) echo /tmp/lokyr/bench-cur/Lokyr.app ;; *) echo /tmp/lokyr/bench-fix/Lokyr.app ;; esac; }
cfg() { case $1 in *notabs) echo $D/config-notabs ;; *) echo $D/config ;; esac; }
round=0
for order in "cur fix fixnotabs" "fixnotabs fix cur" "fix cur fixnotabs" "fixnotabs cur fix"; do
  round=$((round + 1))
  for v in $order; do
    echo "$(date +%T) startup $v $round" >> $OUT/session.log
    $B startup --app $(app $v) --first-frame --env XDG_CONFIG_HOME=$(cfg $v) > $OUT/first-$v-$round.json 2>/dev/null
    $B startup --app $(app $v) --env XDG_CONFIG_HOME=$(cfg $v) > $OUT/window-$v-$round.json 2>/dev/null
  done
  echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log
done

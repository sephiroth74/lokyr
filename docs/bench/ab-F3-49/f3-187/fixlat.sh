#!/bin/sh
set -u
OUT=${OUT:-/tmp/lokyr/bench-F3-49/fix}
D=/tmp/lokyr/bench-F3-49
app() { case $1 in cur*) echo /tmp/lokyr/bench-cur/Lokyr.app ;; *) echo /tmp/lokyr/bench-fix/Lokyr.app ;; esac; }
cfg() { case $1 in *notabs) echo $D/config-notabs ;; *) echo $D/config ;; esac; }
for v in cur fix fixnotabs; do XDG_CONFIG_HOME=$(cfg $v) $(app $v)/Contents/MacOS/lokyr --bench latency --keys 20 >/dev/null 2>&1; done
round=0
for order in "cur fix fixnotabs" "fix fixnotabs cur" "fixnotabs cur fix" "cur fixnotabs fix"; do
  round=$((round + 1))
  for v in $order; do
    echo "$(date +%T) latency $v $round" >> $OUT/session.log
    XDG_CONFIG_HOME=$(cfg $v) $(app $v)/Contents/MacOS/lokyr --bench latency --keys 200 2>/dev/null > $OUT/latency-$v-$round.json
  done
  echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log
done

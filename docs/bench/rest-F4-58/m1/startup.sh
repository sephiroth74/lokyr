#!/bin/sh
# The startup of 0.7.0's app and 0.8.0's, four rounds, AB BA BA AB, each of
# the minimal AppKit app of F0 as its floor, both started before.
set -u
K=$HOME/lokyr-bench
OUT=$K/results/startup
CFG=$K/config-home
mkdir -p $OUT $CFG/lokyr
cp $K/config.toml $CFG/lokyr/config.toml
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "$2 $3" ;;
    *) echo "$3 $2" ;;
  esac
}
caffeinate -dimsu -w $$ &
for v in v070 v080; do
  log "first start $v"
  $K/bin/lokyr-bench startup --app $K/apps/$v/Lokyr.app --first-frame --env XDG_CONFIG_HOME=$CFG > /dev/null
done
for round in 1 2 3 4; do
  for v in $(order $round v070 v080); do
    sleep 2
    log "startup $v $round"
    $K/bin/lokyr-bench startup --app $K/apps/$v/Lokyr.app --first-frame --env XDG_CONFIG_HOME=$CFG > $OUT/startup-$v-$round.json
  done
  sleep 2
  log "startup floor $round"
  $K/bin/lokyr-bench startup --app $K/floor/Spike.app > $OUT/startup-floor-$round.json
done
log done

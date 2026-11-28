#!/bin/sh
# `cat` of 1 GiB of ASCII on screen in a large window, of `lokyr-bench`'s
# `--bench output`, 1.4.0's tag against 1.5.0's, both built into bundles
# signed alike, as 1.5.0's measures found 1.5.0 2.4 % slower in a window
# of 1920 × 1080 points, which ttybench's of 120 × 40 cells did not tell.
# The window is of 1720 × 1080 points, as large as a laptop's Retina
# display holds at a scale of 2.
#
# - `sh run.sh prepare`: the workloads and the copies; it measures nothing.
# - `sh run.sh`: four rounds, AB BA BA AB, of two runs each, some two
#   minutes, the Mac left alone.
#
# The config is the releases' of the least work, ab-F3b-20/config.toml's.
set -u
OUT=/tmp/lokyr/output-F5b-46
CFG=$OUT/config
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
ONSCREEN=$REPO/tools/bench/onscreen.py
W=/tmp/lokyr/workloads
A=${A:-/tmp/lokyr/wt-v140/target/Lokyr.app}
B=${B:-/tmp/lokyr/wt-v150/target/Lokyr.app}
WINDOW=1720x1080

app() { echo $OUT/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v140 v150" ;;
    *) echo "v150 v140" ;;
  esac
}

prepare() {
  mkdir -p $CFG/lokyr
  cp $REPO/docs/bench/ab-F3b-20/config.toml $CFG/lokyr/config.toml
  if [ ! -f $W/ascii.txt ]; then
    log "workloads"
    mkdir -p $W
    python3 -I $REPO/tools/bench/workloads.py $W || return 1
  fi
  for pair in v140:$A v150:$B; do
    v=${pair%%:*} from=${pair#*:}
    [ -d "$from" ] || { log "no copy at $from"; return 1; }
    rm -rf "${OUT:?}/$v" && mkdir -p "$OUT/$v"
    ditto "$from" "$(app $v)" || return 1
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist"))"
  done
}

measure() {
  for v in v140 v150; do
    [ -d "$(app $v)" ] || { echo "no $v: sh $0 prepare first" >&2; return 1; }
  done
  log "session, the load $(sysctl -n vm.loadavg)"
  for round in 1 2 3 4; do
    for v in $(order $round); do
      log "cat $v $round"
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench output --window $WINDOW -- \
        python3 $ONSCREEN $W/ascii.txt $OUT/onscreen-$v-$round.jsonl 2 \
        > $OUT/frames-$v-$round.json
      sleep 2
    done
    log "load $(sysctl -n vm.loadavg)"
  done
  log "session done"
}

mkdir -p $OUT
case ${1:-measure} in
  prepare) prepare ;;
  measure) measure ;;
  *) echo "sh run.sh [prepare|measure]" >&2; exit 1 ;;
esac

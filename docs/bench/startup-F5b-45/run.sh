#!/bin/sh
# Where the startup's 18 ms went between 22 and 25 November 2026: 1.4.0's
# and 1.5.0's release DMGs, both notarized and stapled, on a Mac just
# restarted, and whether Launch Services' other copies of Lokyr slow its
# start. In one session:
#
# 1. the Mac's load let settle, each copy started 40 times;
# 2. "few": Lokyr's registrations as they are, two App Launch traces of
#    each copy, then five rounds of both startups and the minimal AppKit
#    app, AB BA BA AB BA, the same pause before each run;
# 3. "many": 20 more bundles of Lokyr's identifier registered (Lokyr's
#    Info.plist and an executable of a line each, so that no large file is
#    written), two traces of each copy and five rounds again;
# 4. "again": those bundles unregistered and deleted, four rounds.
#
# The results and the copies go to the repository's `target/bench`, which
# a restart keeps, as it empties /tmp.
#
# - `sh run.sh prepare` copies both apps of their DMGs; it measures nothing.
# - `sh run.sh` measures, about 17 minutes once the load settles, from a
#   terminal, VS Code and every app that can be closed closed, the Mac left
#   alone until it ends.
# - `sh run.sh registrations` lists Lokyr's registrations.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=$REPO/target/bench/startup-F5b-45
CFG=$OUT/config
EXTRA=$OUT/extra
BENCH=$REPO/target/release/lokyr-bench
SPIKE=$REPO/spikes/wasmtime-notarized/build/Spike.app
DMG140=${DMG140:-$REPO/target/bench/dl/Lokyr-1.4.0.dmg}
DMG150=${DMG150:-$REPO/target/Lokyr-1.5.0.dmg}
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
ID=io.github.sephiroth74.lokyr
COPIES="v140 v150"

app() { echo $OUT/copies/$1/Lokyr.app; }
log() { echo "$(date +%T) $*" | tee -a $OUT/session.log; }
load() { echo "$(date +%T) load $(sysctl -n vm.loadavg)" >> $OUT/session.log; }
order() {
  case $(( ($1 - 1) % 4 )) in
    0|3) echo "v140 v150" ;;
    *) echo "v150 v140" ;;
  esac
}
conditions() {
  {
    echo "--- $1"
    pmset -g batt | head -2
    uptime
    system_profiler SPDisplaysDataType 2> /dev/null | grep -E ":$|Resolution|Main Display|UI Looks"
    top -l 2 -n 12 -o cpu -stats pid,command,cpu | awk '/^PID/ {n++} n == 2'
  } >> $OUT/session.log
}
# Lokyr's registered copies, a path a line.
registrations() {
  $LSREGISTER -dump 2> /dev/null \
    | awk -v id="$ID" '/^path:/ {sub(/^path: +/, ""); sub(/ \(0x[0-9a-f]+\)$/, ""); p = $0} $1 == "identifier:" && $2 == id {print p}' \
    | sort -u
}
# The app of the DMG `$1` copied to `$2`.
copy() {
  mount=$(hdiutil attach -nobrowse -readonly "$1" | awk -F'\t' '/\/Volumes\// {print $NF}')
  [ -n "$mount" ] || return 1
  rm -rf $OUT/copies/$2 && mkdir -p $OUT/copies/$2
  ditto "$mount/Lokyr.app" $OUT/copies/$2/Lokyr.app
  hdiutil detach -quiet "$mount"
  xattr -dr com.apple.quarantine $OUT/copies/$2/Lokyr.app 2> /dev/null
}

prepare() {
  mkdir -p $CFG/lokyr
  cp $REPO/docs/bench/ab-F3b-20/config.toml $CFG/lokyr/config.toml
  [ -f $DMG140 ] || { log "no DMG of 1.4.0 at $DMG140"; return 1; }
  [ -f $DMG150 ] || { log "no DMG of 1.5.0 at $DMG150"; return 1; }
  copy $DMG140 v140 || return 1
  copy $DMG150 v150 || return 1
  for v in $COPIES; do
    if xcrun stapler validate -q "$(app $v)" > /dev/null 2>&1; then stapled=stapled; else stapled="not stapled"; fi
    log "$v: $(plutil -extract CFBundleShortVersionString raw "$(app $v)/Contents/Info.plist") ($(plutil -extract CFBundleVersion raw "$(app $v)/Contents/Info.plist")), $stapled"
  done
  # The registrations of copies that are gone, as a restart's /tmp.
  registrations | while read -r path; do
    [ -e "$path" ] || { log "unregistered $path, gone"; $LSREGISTER -u "$path" 2> /dev/null; }
  done
  log "registrations: $(registrations | wc -l | tr -d ' ')"
}

# Until the one-minute load is below 3, 15 minutes at most.
settle() {
  for i in $(seq 1 90); do
    one=$(sysctl -n vm.loadavg | awk '{print $2}')
    if awk -v l="$one" 'BEGIN {exit !(l < 3)}'; then
      log "load $one"
      return
    fi
    [ $(( i % 6 )) -eq 1 ] && log "waiting for the load to settle: $one"
    sleep 10
  done
  log "the load did not settle: $(sysctl -n vm.loadavg)"
}

# Two traces of each copy, in turns, exported and deleted.
traces() {
  state=$1
  for i in 1 2; do
    for v in $(order $i); do
      sleep 2
      trace=$OUT/trace-$state-$v-$i.trace
      log "trace $state $v $i"
      xcrun xctrace record --template 'App Launch' --time-limit 6s --output "$trace" \
        --env XDG_CONFIG_HOME=$CFG --launch -- "$(app $v)/Contents/MacOS/lokyr" --bench startup \
        > $OUT/trace-$state-$v-$i.log 2>&1
      for schema in life-cycle-period dyld-activity-interval time-profile; do
        xcrun xctrace export --input "$trace" \
          --xpath "/trace-toc/run[@number=\"1\"]/data/table[@schema=\"$schema\"]" \
          > $OUT/trace-$state-$v-$i-$schema.xml 2> /dev/null
      done
      rm -rf "$trace"
    done
  done
}

rounds() {
  state=$1
  count=$2
  for round in $(seq 1 $count); do
    for v in $(order $round); do
      sleep 2
      log "startup $state $v $round"
      $BENCH startup --app "$(app $v)" --first-frame --env XDG_CONFIG_HOME=$CFG \
        > $OUT/startup-$state-$v-$round.json
    done
    sleep 2
    log "startup $state floor $round"
    $BENCH startup --app $SPIKE > $OUT/startup-$state-floor-$round.json
    load
  done
}

# Twenty bundles of Lokyr's identifier and claims, registered.
register_extra() {
  rm -rf $EXTRA
  for n in $(seq 1 20); do
    bundle=$EXTRA/$n/Lokyr.app
    mkdir -p $bundle/Contents/MacOS
    cp "$(app v150)/Contents/Info.plist" $bundle/Contents/Info.plist
    printf '#!/bin/sh\nexit 0\n' > $bundle/Contents/MacOS/lokyr
    chmod +x $bundle/Contents/MacOS/lokyr
    $LSREGISTER -f $bundle 2> /dev/null
  done
  log "registrations: $(registrations | wc -l | tr -d ' ')"
}

unregister_extra() {
  for n in $(seq 1 20); do
    $LSREGISTER -u $EXTRA/$n/Lokyr.app 2> /dev/null
  done
  rm -rf $EXTRA
  log "registrations: $(registrations | wc -l | tr -d ' ')"
}

measure() {
  for v in $COPIES; do
    [ -d "$(app $v)" ] || { echo "no $(app $v): sh $0 prepare first" >&2; return 1; }
  done
  [ -x "$BENCH" ] || { echo "no $BENCH: cargo build --release -p lokyr-bench first" >&2; return 1; }
  if pgrep -f "Visual Studio Code.app/Contents/MacOS" > /dev/null; then
    echo "VS Code is open: close it, then run this again" >&2
    return 1
  fi
  echo "about 17 minutes once the load settles: leave the Mac alone until it ends; starting in 10 s"
  sleep 10
  caffeinate -d -i -w $$ &
  log "session"
  registrations > $OUT/registrations-few.txt
  conditions start
  settle
  for v in $COPIES; do
    log "40 starts of $v"
    for i in $(seq 1 40); do
      XDG_CONFIG_HOME=$CFG "$(app $v)/Contents/MacOS/lokyr" --bench startup > /dev/null 2>&1
    done
  done
  traces few
  rounds few 5
  register_extra
  registrations > $OUT/registrations-many.txt
  sleep 30
  traces many
  rounds many 5
  unregister_extra
  sleep 30
  rounds again 4
  conditions end
  log "session done"
  echo "done: the Mac is free, and VS Code can open again"
}

mkdir -p $OUT
case ${1:-measure} in
  prepare) prepare ;;
  measure) measure ;;
  registrations) registrations ;;
  *)
    echo "sh run.sh [prepare|measure|registrations]" >&2
    exit 1
    ;;
esac

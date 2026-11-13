#!/bin/sh
# Every terminal of the first comparison measured again, of its version of
# now, beside Lokyr of main's build, under F2-134's conditions and of its
# tool, `compare.py`: the startup, idle and memory, the throughput, and the
# CPU's energy to show `cat` of 1 GiB. `setup` writes iTerm2's and
# Terminal.app's profiles, the settings of both saved first, which
# `restore` puts back at the end. With measures as arguments, those alone, as
# `sh run.sh setup check`.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=/tmp/lokyr/compare-F3-268
CONFIGS=$REPO/docs/bench/compare-F2-134/configs
export LOKYR_APP=/tmp/lokyr/bench-F3-268-app/Lokyr.app
MEASURES=${*:-setup check startup idle throughput energycat restore}
mkdir -p "$OUT"
{
  echo "== $(date '+%F %T') $MEASURES"
  pmset -g batt | head -2
  uptime
  system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$"
} >> "$OUT/session.log"
for measure in $MEASURES; do
  echo "$(date +%T) $measure, load $(sysctl -n vm.loadavg)" >> "$OUT/session.log"
  if [ "$measure" = setup ]; then
    python3 "$REPO/tools/bench/compare.py" setup "$CONFIGS" "$OUT" >> "$OUT/session.log" 2>&1
  elif [ "$measure" = restore ]; then
    python3 "$REPO/tools/bench/compare.py" restore >> "$OUT/session.log" 2>&1
  else
    python3 "$REPO/tools/bench/compare.py" "$measure" "$CONFIGS" "$OUT" >> "$OUT/session.log" 2>&1
  fi
done
echo "$(date +%T) done, load $(sysctl -n vm.loadavg)" >> "$OUT/session.log"

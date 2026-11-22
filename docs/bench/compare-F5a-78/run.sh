#!/bin/sh
# The terminals whose version changed since the comparison of 13 November
# 2026, kitty and iTerm2, measured again beside Lokyr of 1.4.0's code, the
# bundle of 1.4.0's A/B, under F2-134's conditions and of its tool,
# `compare.py`: the startup, idle and memory, the throughput, and the CPU's
# energy to show `cat` of 1 GiB. `setup` writes iTerm2's profile, the
# user's settings saved first, which `restore` puts back at the end. With
# measures as arguments, those alone, as `sh run.sh setup check`, and
# `TERMINALS` some of them alone, as each throughput in a step of its own,
# of ten minutes at most: `TERMINALS=kitty sh run.sh throughput`. vtebench
# is the one of commit ead8003, built into /tmp/lokyr/vtebench-bin, its
# benchmarks in target/vtebench.
set -u
REPO=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=/tmp/lokyr/compare-F5a-78
CONFIGS=$REPO/docs/bench/compare-F2-134/configs
TERMINALS=${TERMINALS:-lokyr,kitty,iterm2}
export LOKYR_APP=/tmp/lokyr/bench-F5a-78/v140/Lokyr.app
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
    python3 "$REPO/tools/bench/compare.py" setup "$CONFIGS" "$OUT" "$TERMINALS" >> "$OUT/session.log" 2>&1
  elif [ "$measure" = restore ]; then
    python3 "$REPO/tools/bench/compare.py" restore >> "$OUT/session.log" 2>&1
  else
    python3 "$REPO/tools/bench/compare.py" "$measure" "$CONFIGS" "$OUT" "$TERMINALS" >> "$OUT/session.log" 2>&1
  fi
done
echo "$(date +%T) done, load $(sysctl -n vm.loadavg)" >> "$OUT/session.log"

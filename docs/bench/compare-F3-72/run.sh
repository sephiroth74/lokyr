#!/bin/sh
# Lokyr alone, of the build after F3-72, under F2-134's conditions and of
# its tool, `compare.py`, so that the comparison's charts take Lokyr's
# figures of now beside the other terminals' of F2-134, which are measured
# once of their versions: the startup, idle and memory, the throughput,
# and the CPU's energy to show `cat` of 1 GiB. No `setup`, which writes
# iTerm2's and Terminal.app's settings: Lokyr's config alone, in the XDG
# directory that the measures give it, and the empty ZDOTDIR.
set -u
REPO=/Users/taacral1/Documents/git/sephiroth74/rust/lokyr
OUT=/tmp/lokyr/compare-F3-72
CONFIGS=$REPO/docs/bench/compare-F2-134/configs
export LOKYR_APP=/tmp/lokyr/bench-F3-72-app/Lokyr.app
mkdir -p $OUT /tmp/lokyr/compare-xdg /tmp/lokyr/compare-zdotdir
rm -rf /tmp/lokyr/compare-xdg/lokyr
cp -R $CONFIGS/lokyr /tmp/lokyr/compare-xdg/lokyr
{
  pmset -g batt | head -2
  uptime
  system_profiler SPDisplaysDataType 2>/dev/null | grep -E "Resolution|Main Display|Refresh|Display Type|:$"
} >> $OUT/session.log
for measure in check startup idle throughput energycat; do
  echo "$(date +%T) $measure, load $(sysctl -n vm.loadavg)" >> $OUT/session.log
  python3 $REPO/tools/bench/compare.py $measure $CONFIGS $OUT lokyr 2>> $OUT/session.log
done
echo "$(date +%T) done, load $(sysctl -n vm.loadavg)" >> $OUT/session.log

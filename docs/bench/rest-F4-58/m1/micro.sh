#!/bin/sh
# The entitlement's micro-benchmarks, of spikes/jit-entitlement, six rounds of
# its five signatures in turned orders, then again with a region of
# `MAP_JIT` mapped first; of no window, which runs from SSH as it is.
set -u
K=$HOME/lokyr-bench
VARIANTS="plain hr hr-jit hr-unsigned hr-both"
caffeinate -dimsu -w $$ &
for extra in "" --jit-region; do
  OUT=$K/results/micro${extra:+-region}
  mkdir -p $OUT
  for variant in $VARIANTS; do $K/micro/$variant $extra > /dev/null; done
  round=1
  while [ $round -le 6 ]; do
    shift_by=$(( (round - 1) % 5 ))
    set -- $VARIANTS
    shift $shift_by
    turned="$*"
    [ $shift_by -gt 0 ] && turned="$turned $(echo $VARIANTS | cut -d' ' -f1-$shift_by)"
    for variant in $turned; do
      echo "$(date +%T) round $round $variant $extra"
      $K/micro/$variant $extra > $OUT/$variant-$round.jsonl
    done
    round=$((round + 1))
  done
done

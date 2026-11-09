#!/bin/sh
# Runs each signed copy of the measures in turned orders, `rounds` rounds
# (6 by default), each copy's lines into build/results/<variant>-<round>.jsonl;
# with `--jit-region`, a region of `MAP_JIT` mapped first, where it maps.
set -u
cd "$(dirname "$0")"
ROUNDS=${ROUNDS:-6}
EXTRA=${1:-}
VARIANTS="plain hr hr-jit hr-unsigned hr-both"
OUT=build/results${EXTRA:+-region}
mkdir -p $OUT
for variant in $VARIANTS; do build/$variant $EXTRA > /dev/null; done
round=1
while [ $round -le $ROUNDS ]; do
  set -- $VARIANTS
  shift $(( (round - 1) % 5 ))
  turned="$* $(echo $VARIANTS | cut -d' ' -f1-$(( (round - 1) % 5 )))"
  [ $(( (round - 1) % 5 )) -eq 0 ] && turned="$VARIANTS"
  for variant in $turned; do
    echo "$(date +%T) round $round $variant"
    build/$variant $EXTRA > $OUT/$variant-$round.jsonl
  done
  round=$((round + 1))
done

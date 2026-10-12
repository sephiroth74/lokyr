#!/usr/bin/env bash
# Builds the host in both variants (Cranelift and Pulley) and the guests, then
# prints the metrics of every combination: each variant on each guest, first
# with a fresh compilation, then from the cache.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p build/bin

cargo build --release --locked -p spike
cp target/release/spike build/bin/spike-cranelift
cargo build --release --locked -p spike --features pulley
cp target/release/spike build/bin/spike-pulley

./build-guests.sh > /dev/null

for variant in cranelift pulley; do
    for guest in unknown wasip2; do
        echo "# $variant on $guest, compiled"
        "build/bin/spike-$variant" --bench "build/guests/$guest.wasm"
        echo "# $variant on $guest, cached"
        "build/bin/spike-$variant" --bench "build/guests/$guest.wasm" --cached
    done
done

#!/usr/bin/env bash
# Builds the guest component in two flavors, into build/guests/:
#   wasip2.wasm   built for wasm32-wasip2; imports the WASI interfaces that
#                 the standard library links in;
#   unknown.wasm  built for wasm32-unknown-unknown and turned into a component
#                 by the toolchain's wasm-component-ld; imports nothing.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p build/guests

cargo build --release --locked -p spike-guest --target wasm32-wasip2
cp target/wasm32-wasip2/release/spike_guest.wasm build/guests/wasip2.wasm

linker="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/wasm-component-ld"
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER="$linker" \
    cargo build --release --locked -p spike-guest --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/spike_guest.wasm build/guests/unknown.wasm

ls -l build/guests

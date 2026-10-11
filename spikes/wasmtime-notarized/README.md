# Spike: wasmtime in a notarized macOS app

A throwaway experiment, kept out of the Cargo workspace. It answers two questions:
- Can a notarized app with the hardened runtime run WebAssembly components with wasmtime? This compares Cranelift, which needs a JIT entitlement, with Pulley, wasmtime's interpreter, which needs none.
- Can the host run components without `wasmtime-wasi`, which would bring `tokio` into the app?

The spike is a small Cargo workspace:
- the root package `spike` is a minimal AppKit app (one window, Quit on ⌘Q), and also the wasmtime host;
- `guest/` is the WebAssembly component, built against `wit/spike.wit`.

## Build the guests

```sh
./build-guests.sh
```

This produces two flavors in `build/guests/`:
- `wasip2.wasm`: built for `wasm32-wasip2`. It imports the WASI interfaces the standard library links in, even though it never calls them.
- `unknown.wasm`: built for `wasm32-unknown-unknown`, then turned into a component by the toolchain's `wasm-component-ld`. It imports nothing.

The `wasm32-unknown-unknown` target must be installed: `rustup target add wasm32-unknown-unknown`.

## Run the host from a terminal

```sh
cargo build --release
target/release/spike --imports build/guests/wasip2.wasm          # list the component's imports
target/release/spike --bench build/guests/unknown.wasm           # compile, cache, run, print JSON metrics
target/release/spike --bench build/guests/unknown.wasm --cached  # reuse the cached .cwasm
```

Imports a component never calls are linked as traps, so no WASI implementation is needed. Without arguments, the binary opens the app window.

## Build and sign the app

```sh
./bundle.sh
```

The script builds the release binary, assembles `build/Spike.app` and signs it with the hardened runtime. Two environment variables override the defaults:
- `SPIKE_SIGN_IDENTITY`: the signing identity;
- `SPIKE_ENTITLEMENTS`: the entitlements file (default: `entitlements/base.plist`, which grants no exceptions).

Check the signature with:

```sh
codesign -dv --verbose=4 build/Spike.app   # look for "flags=0x10000(runtime)"
```

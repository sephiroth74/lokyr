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

The default build runs guests as native code compiled by Cranelift. Build with `--features pulley` to run them on Pulley, wasmtime's interpreter, instead.

## Compare the variants

```sh
./run-bench.sh
```

The script builds both variants (`build/bin/spike-cranelift` and `build/bin/spike-pulley`) and both guests. It then prints the metrics of each variant on each guest, first with a fresh compilation, then from the cache.

## Build and sign the app

```sh
./bundle.sh pulley                                     # build/Spike-Pulley.app, no entitlement exceptions
./bundle.sh cranelift entitlements/unsigned-memory.plist   # build/Spike-Cranelift.app
```

Each bundle contains the host for one variant and the guests, in `Contents/Resources`. It is signed with the hardened runtime and a secure timestamp. `SPIKE_SIGN_IDENTITY` overrides the signing identity.

The `entitlements/` folder holds the variants tested under the hardened runtime:

| File | Exception |
|---|---|
| `base.plist` | none |
| `jit.plist` | `allow-jit` |
| `unsigned-memory.plist` | `allow-unsigned-executable-memory` |
| `no-page-protection.plist` | `disable-executable-page-protection` |

Under the hardened runtime, Cranelift works only with `unsigned-memory.plist` (or the broader `no-page-protection.plist`), and only with `--from-memory`. Without it, the kernel kills the process. Pulley works with `base.plist`.

```sh
build/Spike-Pulley.app/Contents/MacOS/spike --bench build/Spike-Pulley.app/Contents/Resources/unknown.wasm
build/Spike-Cranelift.app/Contents/MacOS/spike --bench build/Spike-Cranelift.app/Contents/Resources/unknown.wasm --from-memory
codesign -dv --verbose=4 build/Spike-Pulley.app   # look for "flags=0x10000(runtime)"
```

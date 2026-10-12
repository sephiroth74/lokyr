# Spike: wasmtime in a notarized macOS app

A throwaway experiment, kept out of the Cargo workspace. It answers three questions:
- Can a notarized app with the hardened runtime run WebAssembly components with wasmtime? This compares Cranelift, which needs a JIT entitlement, with Pulley, wasmtime's interpreter, which needs none.
- Can the host run components without `wasmtime-wasi`, which would bring `tokio` into the app?
- How do Lua runtimes (LuaJIT and Luau) compare with WebAssembly for plugins, in speed, sandboxing and entitlements?

The spike is a small Cargo workspace:
- the root package `spike` is a minimal AppKit app (one window, Quit on ⌘Q), and also the wasmtime host;
- `guest/` is the WebAssembly component, built against `wit/spike.wit`;
- `workload/` holds plugin-like work on terminal lines (fuzzy search, highlighting), compiled into both the guest and the host.

## Build the guests

```sh
./build-guests.sh
```

This produces two flavors in `build/guests/`:
- `wasip2.wasm`: built for `wasm32-wasip2`. It imports the WASI interfaces the standard library links in, even though it never calls them.
- `unknown.wasm`: built for `wasm32-unknown-unknown`, then turned into a component by the toolchain's `wasm-component-ld`. It imports nothing.

Both targets must be installed, since the workspace toolchain no longer lists them: `rustup target add wasm32-wasip2 wasm32-unknown-unknown`.

## Run the host from a terminal

```sh
cargo build --release
target/release/spike --imports build/guests/wasip2.wasm          # list the component's imports
target/release/spike --bench build/guests/unknown.wasm           # compile, cache, run, print JSON metrics
target/release/spike --bench build/guests/unknown.wasm --cached  # reuse the cached .cwasm
target/release/spike --workloads build/guests/unknown.wasm       # time the workloads in the guest and natively
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

## Notarize, and launch like a download

```sh
./notarize.sh build/Spike-Pulley.app        # submit, staple, verify with spctl
./notarize.sh build/Spike-Cranelift.app

# Mark the bundle as downloaded, then launch it through LaunchServices,
# collecting the JSON metrics from its standard output.
xattr -r -w com.apple.quarantine "0081;$(printf '%x' "$(date +%s)");Safari;" build/Spike-Pulley.app
app="$PWD/build/Spike-Pulley.app"
open -W --stdout pulley.json "$app" --args --bench "$app/Contents/Resources/unknown.wasm"
```

For the Cranelift bundle, add `--from-memory` to the arguments. `notarize.sh` uses the `saetta-notary` keychain profile; `SPIKE_NOTARY_PROFILE` overrides it.

## Lua runtimes

The same host can run plugins written in Lua instead. Build it with one of these features:

| Feature | Runtime |
|---|---|
| `luajit` | LuaJIT 2.1 and its JIT compiler; `--no-jit` turns the compiler off |
| `luau` | Luau, interpreted |
| `luau-jit` | Luau and its native code generator; `--no-jit` turns it off |

```sh
cargo build --release --features luau-jit
target/release/spike --lua-workloads lua/workloads.lua   # time the workloads, print JSON
target/release/spike --lua-limits lua/workloads.lua      # hostile plugins and sandbox probes
./measure-lua.sh [runs]                                  # build the three runtimes, print median tables
```

`lua/workloads.lua` holds the workloads of `workload/` written in Lua, in two styles:
- byte loops, with the same algorithms as the Rust code;
- `string.find` patterns, the way most Lua code is written.

The host checks that every result equals the Rust one.

Plugins run in a plugin state (`lua_bench::new_state`):
- it holds only the string, table, math and bit libraries, plus utf8 and buffer with Luau;
- it has none of the functions that load code, read files, change environments or drive the garbage collector.

`--lua-limits` also shows what mlua's `Lua::new()` leaves reachable. It stops runaway code in two ways:
- a callback that checks the clock while the code runs (a LuaJIT hook, or Luau's interrupt);
- a watchdog thread that installs the stop only once the deadline has passed.

### Under the hardened runtime

```sh
./check-lua-hardened.sh   # sign each Lua variant with each entitlement file, run it with the JIT on
./bundle.sh luau-jit      # build/Spike-LuauJIT.app, signed with entitlements/jit.plist
```

`bundle.sh` also takes the variants `luajit`, `luajit-hrt`, `luau` and `luau-jit`:
- the Lua bundles carry `lua/*.lua` in `Contents/Resources`;
- `luajit-hrt` builds LuaJIT with `LUAJIT_ENABLE_OSX_HRT`, which makes it allocate its machine code with `MAP_JIT`.

LuaJIT as mlua vendors it is killed by the kernel as soon as its JIT runs, with or without `allow-jit`. **macOS then shows a crash dialog, once per run.** The crash reports land in `~/Library/Logs/DiagnosticReports/spike-*.ips`.

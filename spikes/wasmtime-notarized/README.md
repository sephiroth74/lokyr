# Spike: wasmtime in a notarized macOS app

A throwaway experiment, kept out of the Cargo workspace. It answers one question: can a notarized app with the hardened runtime run WebAssembly components with wasmtime? It compares Cranelift, which needs a JIT entitlement, with Pulley, wasmtime's interpreter, which needs none.

The app itself is minimal: one window, and an application menu whose Quit item answers to ⌘Q.

## Build and sign

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

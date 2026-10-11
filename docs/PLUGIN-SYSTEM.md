# Plugin system

Everything you see in Saetta is a plugin: the tab bar, splits, the find bar, themes, the palette. The kernel provides data and services, and plugins decide how things behave and look ([ADR 0002](adr/0002-micro-kernel-and-plugins.md), [ADR 0003](adr/0003-native-and-wasm-plugins.md)).

## Two tiers, one API

| | First-party plugins | Third-party plugins |
|---|---|---|
| Form | Rust crate in `plugins/`, statically linked | WebAssembly component (`wasm32-wasip2`) |
| Loading | At startup, from the distribution (`dist/*.toml`) | From disk (installed by Faretra), only when needed |
| Trust | Our own reviewed code | Sandboxed: only the capabilities granted to it |
| Cost | Direct calls, zero overhead | One Wasm instance (≤ 2 MB when active); calls copy data |
| API | `plugin-sdk` | `plugin-sdk`, from the same source |
| Failures | `catch_unwind` at the boundary; the plugin is disabled | Trap; the plugin is disabled and restarted with backoff |

## Extension points

| Area | What a plugin can do | Examples | Phase |
|---|---|---|---|
| Commands | Register and run commands with typed arguments; add menu and palette entries | `tabs.new`, `search.open` | F3 |
| Keymap | Bindings with a context (`when`), chords and leaders, modal key tables | `cmd+f` → `search.open` when a pane has focus | F3 |
| Events | Receive kernel events (batched) and emit its own | `tab-created`, `title-changed`, `command-finished` | F3 |
| Workspace and layout | Create, close and move tabs and panes; edit the layout tree | split, zoom, move to a new window | F3 |
| UI | Provide view trees for slots | tab bar, find bar, status bar segments | F3 |
| Terminal | Read lines and the selection; write input (with a capability); use the search and decoration services | highlight matches, gutter marks | F3 |
| Themes | Register themes and importers | `.itermcolors`, base16 | F3 |
| Config | A `[plugins.<name>]` section with a JSON schema and migrations | `[plugins.tabs] position = "top"` | F3 |
| Storage | Persistent per-plugin state | search history | F4 |
| System | Clipboard, notifications, opening URLs and files, processes, network, global hotkeys (with capabilities) | notify when a command finishes | F4+ |
| Protocols | Handlers for rare OSC/DCS/APC sequences | iTerm2's OSC 1337 | F5 |
| Triggers | Regexes over completed lines, with batched callbacks | highlight `ERROR` in red | F5b |

### UI slots

| Slot | Providers | Used for |
|---|---|---|
| `tab-bar` | One, chosen in the config | The tab bar |
| `status-bar` | Many (ordered segments) | Status information |
| `pane-header` | One | Pane titles in splits |
| `pane-overlay` | Many | Find bar, hints |
| `window-overlay` | Many | Command palette, notifications |
| `popup` | Many | Custom menus and tooltips |
| `sidebar` | Many (F5+) | Side panels |

## Execution model

Native and Wasm plugins follow the same rules, so they behave the same way.

- **Short, synchronous calls**:
  - The kernel calls into the plugin (`activate`, `on-event`, `on-command`, `render`, `on-ui-event`), and the plugin calls kernel services.
  - Long-running work goes to asynchronous host services (search, timers, processes), whose results come back as events.
- **Owned data**: plugins receive copies, never references into kernel state.
- **No re-entrancy**: events generated during a callback are queued and delivered only after the callback returns.
- **No threads, no blocking I/O**: time is provided by the host, through timers and `tick` events that are sent only on request.
- **Filtered, batched events**: the kernel delivers only the events a plugin subscribed to, batched per frame.
- **Rendering**:
  - `render(slot)` is called only when the plugin asks for an update (`request-render`) or the theme changes. The result is cached.
  - Native plugins run on the main thread. Wasm plugins run on the executor, with a deadline per call.
  - If a Wasm plugin misses its deadline, the last valid view tree stays on screen.

## One source, two targets

```rust
use saetta_plugin_sdk::{prelude::*, types::*};

#[derive(Default)]
struct Search { query: String }

impl Plugin for Search {
    fn activate(&mut self, cx: &mut Cx) -> Result<()> {
        cx.commands().register("search.open", Self::open)?;
        cx.keymap().bind("cmd+f", "search.open", When::PaneFocused)?;
        Ok(())
    }
    fn on_event(&mut self, cx: &mut Cx, ev: Event) { /* results, pane closed, … */ }
    fn render(&mut self, cx: &mut Cx, slot: Slot) -> View { /* the find bar */ }
}

export_plugin!(Search); // wasm32-wasip2 → wit-bindgen's export!; native → a registration function for gen-dist
```

- **Types**: `plugin-sdk` runs `wit_bindgen::generate!` once, over the `saetta:plugin` world. The generated types also compile outside Wasm. A plugin sees only `types`, `Cx`, `Plugin` and `export_plugin!`.
- **`export_plugin!(T)`**: for `wasm32-wasip2` it expands to wit-bindgen's `export!`. Natively it generates a registration function, which `xtask gen-dist` wires into the app.
- **Two backends for `Cx`**:
  - **Wasm**: calls the imports generated by wit-bindgen.
  - **Native**: calls `&mut dyn HostApi`, an object-safe trait that mirrors the WIT imports and is implemented by the kernel.
- **Host side**: the implementations generated by `wasmtime::component::bindgen!` forward to the same `HostApi`, with generated type conversions. The semantics are implemented once, behind two transports.
- **Conformance**:
  - From F3 on, every first-party plugin is also built for `wasm32-wasip2`.
  - The conformance suite runs the same scenarios against both builds and compares their command and event traces.
- **Testing for plugin authors**: the SDK ships a fake host for unit tests. It simulates events, lets tests assert on commands and view trees, and snapshots view trees.

## WIT

- **What `plugin-wit` holds**: the `saetta:host` package (the host's interfaces) and the `saetta:plugin` package (the world a plugin implements).
- **Single source of truth**: the SDK's Rust types, `HostApi`, the wasmtime bindings and the reference docs are all generated from it.
- **Publishing**: when `plugin-sdk` and `plugin-host-wasm` are published, `xtask` copies the WIT files into them. CI checks that the copies match.

### Constraints and how we handle them

| WIT constraint | Our answer |
|---|---|
| No recursive types | The view tree is a flat arena: `list<node>` with child indices |
| No `any` type | Command arguments use a flat schema (scalars, lists of scalars, key/value pairs) shared by the keymap, the palette and IPC |
| Plugin-defined events | Sent as `custom-event { name, schema, payload }`, with a CBOR payload and a schema id |
| Handles to host objects | Opaque `u64` ids (window, tab, pane) instead of `resource`s, which keeps parity with the native transport trivial |

### Versioning

- The SDK version matches the WIT package version (`saetta:host@0.1.0` ↔ `plugin-sdk 0.1.0`).
- New APIs start behind `@unstable(feature = …)` and become stable with `@since(version = …)`.
- The manifest declares the API version it requires (`api = "^0.1"`). The host supports the current major version and, through adapters, the previous one.
- Before 1.0, minor versions may break compatibility. After 1.0, semver is followed strictly.

### Illustrative draft

> This sketch only illustrates the shape of the API. Names and signatures will be settled in F3 and validated with `wasm-tools`.

```wit
package saetta:host@0.1.0;

interface types {
    type window-id = u64;
    type tab-id = u64;
    type pane-id = u64;

    /// Flat values for command arguments: WIT has neither `any` nor recursive types.
    variant scalar {
        boolean(bool),
        integer(s64),
        float(f64),
        text(string),
    }

    variant value {
        single(scalar),
        many(list<scalar>),
    }

    type args = list<tuple<string, value>>;

    record rgba { r: u8, g: u8, b: u8, a: u8 }

    /// Plugins should prefer theme tokens (e.g. "tab.active.bg").
    variant color {
        token(string),
        literal(rgba),
    }

    enum direction { left, right, up, down }

    variant error {
        denied(string),
        not-found(string),
        invalid-args(string),
        failed(string),
    }
}

interface commands {
    use types.{args, error};

    register: func(name: string, title: string) -> result<_, error>;
    execute: func(name: string, args: args) -> result<_, error>;
}

interface keymap {
    use types.{error};

    /// `keys`: e.g. "cmd+f" or "ctrl+a>c"; `when`: e.g. "pane.focused && !search.open".
    bind: func(keys: string, command: string, when: option<string>) -> result<_, error>;
}

interface workspace {
    use types.{window-id, tab-id, pane-id, error};

    record tab-info {
        id: tab-id,
        window: window-id,
        title: string,
        active: bool,
        panes: list<pane-id>,
    }

    tabs: func(window: window-id) -> list<tab-info>;
    new-tab: func(window: window-id) -> result<tab-id, error>;
    close-tab: func(tab: tab-id) -> result<_, error>;
    move-tab: func(tab: tab-id, window: window-id, index: u32) -> result<_, error>;
    set-tab-title: func(tab: tab-id, title: option<string>) -> result<_, error>;
}

interface layout {
    use types.{pane-id, direction, error};

    split: func(pane: pane-id, dir: direction, ratio: f32) -> result<pane-id, error>;
    resize: func(pane: pane-id, dir: direction, cells: s32) -> result<_, error>;
    focus: func(pane: pane-id) -> result<_, error>;
    toggle-zoom: func(pane: pane-id) -> result<_, error>;
}

interface ui {
    use types.{color};

    enum axis { row, column }

    record style {
        direction: axis,
        gap: f32,
        padding: f32,
        grow: f32,
        background: option<color>,
        foreground: option<color>,
        corner-radius: f32,
    }

    record input-props { value: string, placeholder: string }

    variant node-kind {
        container,
        text(string),
        icon(string),
        input(input-props),
        spacer,
    }

    /// The view tree is a flat arena: `children` holds indices into `view.nodes`.
    record node {
        key: u32,
        kind: node-kind,
        style: style,
        children: list<u32>,
        role: option<string>,
        label: option<string>,
    }

    record view { nodes: list<node>, root: u32 }

    request-render: func(slot: string);
}

interface terminal {
    use types.{pane-id, error};

    record line { text: string, wrapped: bool }

    /// Requires `terminal-read`; returns `denied` without it.
    read-lines: func(pane: pane-id, start: s64, count: u32) -> result<list<line>, error>;
}

interface search {
    use types.{pane-id, error};

    record query { pattern: string, regex: bool, case-sensitive: bool, whole-word: bool }

    /// Runs in the background: results arrive as `search-progress` events.
    start: func(pane: pane-id, q: query) -> result<u64, error>;
    select-next: func(id: u64, forward: bool) -> result<_, error>;
    cancel: func(id: u64);
}

interface clipboard {
    use types.{error};

    write-text: func(text: string) -> result<_, error>;
    read-text: func() -> result<string, error>;
}
```

```wit
package saetta:plugin@0.1.0;

world plugin {
    import saetta:host/commands@0.1.0;
    import saetta:host/keymap@0.1.0;
    import saetta:host/workspace@0.1.0;
    import saetta:host/layout@0.1.0;
    import saetta:host/ui@0.1.0;
    import saetta:host/terminal@0.1.0;
    import saetta:host/search@0.1.0;
    import saetta:host/clipboard@0.1.0;

    use saetta:host/types@0.1.0.{tab-id, pane-id, args, error};
    use saetta:host/ui@0.1.0.{view};

    record custom-event { name: string, schema: string, payload: list<u8> }

    record search-progress { id: u64, matches: u32, done: bool }

    variant event {
        tab-created(tab-id),
        tab-closed(tab-id),
        pane-focused(pane-id),
        title-changed(tuple<pane-id, string>),
        search-progress(search-progress),
        custom(custom-event),
    }

    enum ui-event-kind { click, double-click, hover-enter, hover-leave, context-menu, input-changed, submit }

    record ui-event { slot: string, key: u32, kind: ui-event-kind, text: option<string> }

    export activate: func() -> result<_, error>;
    export deactivate: func();
    export on-event: func(events: list<event>);
    export on-command: func(name: string, args: args) -> result<_, error>;
    export render: func(slot: string) -> view;
    export on-ui-event: func(ev: ui-event);
}
```

## Capabilities

| Capability | Allows | Risk |
|---|---|---|
| *(none)* | Commands, keymap, events, UI, themes, config, storage, logging, timers | — |
| `terminal-read` | Reading screen and scrollback text, and the selection | High: the screen may show secrets |
| `pty-write` | Sending input to the shell | Very high |
| `clipboard-read` | Reading the clipboard | High |
| `clipboard-write` | Writing to the clipboard | Medium |
| `notifications` | Showing system notifications | Low |
| `open` | Opening URLs and files with the system handler | Medium |
| `process-spawn` | Starting processes | Very high |
| `net:<host>` | HTTP requests to the listed hosts | High |
| `fs-read:<path>`, `fs-write:<path>` | Access to the listed paths | High |
| `global-hotkey` | System-wide shortcuts | Medium |

- **Declaration and consent**:
  - Capabilities are declared in the manifest. Faretra shows them at install time, and the app asks for confirmation on first use.
  - Decisions are stored in `~/.config/saetta/permissions.toml`, which can be versioned with dotfiles and revoked at any time.
- **Linking**:
  - Fine-grained interfaces (clipboard, `pty-write`, `terminal-read`, …) are **always linked**. Without the capability they return `denied`. A plugin can therefore treat a capability as optional and request it at the moment it's needed.
  - Coarse WASI interfaces (filesystem, HTTP) are linked **only when granted**, and only for the granted paths and hosts.
- **Networking without an HTTP client in the app**:
  - Requests from plugins with `net:<host>` go through a helper process started on demand (`faretra net-helper`), which enforces the host allowlist.
  - The app itself still contains no HTTP/TLS client ([ADR 0008](adr/0008-separate-package-manager.md)).
- **First-party plugins** are trusted code and don't go through capabilities, but they are still limited to the SDK.

## Lifecycle

```
installed ──▶ resolved ──▶ consent ──▶ loaded ──▶ active
                               ▲                     │ trap / panic
                               │                     ▼
                         re-enabled ◀── disabled ◀── restart with backoff
```

- **Lazy activation** through `activation.on`: `startup`, `command:<glob>`, `event:<name>`, `slot:<name>`, `osc:<number>`.
- **Zero cost without Wasm**: the wasmtime `Engine` is created when the first Wasm plugin activates.
- **Backoff**: after 3 traps within a minute, the plugin stays disabled until the user re-enables it.
- **Hot reload**: `faretra dev` rebuilds a plugin and reloads it into the running app over IPC.

## Limits and isolation

- **Memory**: a `ResourceLimiter` per instance (64 MB by default, configurable per plugin).
- **CPU**:
  - Epoch interruption, with a deadline per call type (e.g. 16 ms for `render`, 250 ms for commands).
  - The thread that advances the epoch runs **only while a plugin is executing**, so there are no wakeups when idle.
- **Failures**: a trap never affects the terminal. The plugin is disabled and the user gets a notification. In native plugins, `catch_unwind` at the boundary stops panics.

## AOT cache and wasmtime

- **Compilation**: on first load, each component is compiled ahead of time (AOT) and cached.
- **Cache key**: `Engine::precompile_compatibility_hash()` plus the component's hash. Upgrading wasmtime or changing its config invalidates old entries automatically, and they are recompiled in the background.
- **`deserialize` is `unsafe`**: the cache directory is created with 0700 permissions, and each file's hash is checked before loading.
- **wasmtime version**: wasmtime ships a major version every month, so we pin an **LTS** line and upgrade deliberately.
- **macOS**:
  - The Hardened Runtime, required for notarization, blocks code generated at runtime unless the app has the `com.apple.security.cs.allow-jit` (or `allow-unsigned-executable-memory`) entitlement.
  - The F0 spike chooses between that entitlement and Pulley, wasmtime's interpreter ([ADR 0003](adr/0003-native-and-wasm-plugins.md)).

## First-party plugins

| Plugin | Role | Phase |
|---|---|---|
| `core-actions` | Copy, paste, select all, scrolling, font size, new windows | F2 |
| `tabs` | iTerm2-style tab bar and tab commands | F3 |
| `splits` | Split commands, dividers and interaction | F3 |
| `search` | Find bar and search commands | F3 |
| `themes` | Theme model, light/dark switching, picker | F3 |
| `theme-iterm2` | `.itermcolors` importer | F3 |
| `keymap-macos` | Default macOS shortcuts | F3 |
| `keymap-linux` | Default Linux shortcuts | F7 |
| `command-palette` | Command palette and theme picker | F3 |
| `plugin-manager` | UI for Faretra (calls `faretra --json`) | F4 |
| `shell-integration`, `blocks` | Prompt marks, command + output blocks | F5a |
| `tmux-cc`, `quick-terminal`, `triggers`, `profiles`, `broadcast` | iTerm2 power features | F5b |
| `hints`, `images` | Hints, URLs, inline images | F5c |

**Rules** (details in [ARCHITECTURE.md](ARCHITECTURE.md#dependency-rules)):
- depend only on `plugin-sdk`;
- use `#![forbid(unsafe_code)]`, with clippy `disallowed-methods` on;
- build for `wasm32-wasip2` too, and pass the conformance suite.

## Distribution

The "distribution" is the list of first-party plugins compiled into the app.

```toml
# dist/default.toml
[distribution]
name = "default"

[plugins]
core-actions    = { path = "../plugins/core-actions" }
tabs            = { path = "../plugins/tabs" }
splits          = { path = "../plugins/splits" }
search          = { path = "../plugins/search" }
themes          = { path = "../plugins/themes" }
theme-iterm2    = { path = "../plugins/theme-iterm2" }
command-palette = { path = "../plugins/command-palette" }
plugin-manager  = { path = "../plugins/plugin-manager" }   # from F4
keymap-macos    = { path = "../plugins/keymap-macos", target = "macos" }
keymap-linux    = { path = "../plugins/keymap-linux", target = "linux" }    # from F7
```

- **Generation**: `xtask gen-dist <file>` generates the `app` crate's dependencies and the static registration function.
- **Minimal distribution**: `dist/minimal.toml` contains only `core-actions` and a keymap, Alacritty-style. It proves the terminal works without any of the "visible" features.
- **At runtime**:
  - `[plugins] disabled = ["tabs"]` turns off a first-party plugin.
  - For single-provider slots (`tab-bar`), the config picks the provider, so a third-party plugin can replace our tab bar.

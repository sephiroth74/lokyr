# Plugin system

Everything you see in Lokyr is a plugin: the tab bar, splits, the find bar, themes, the palette. The kernel provides data and services, and plugins decide how things behave and look ([ADR 0002](adr/0002-micro-kernel-and-plugins.md), [ADR 0003](adr/0003-native-and-wasm-plugins.md), [ADR 0012](adr/0012-luau-plugins.md)).

## Two tiers, one API

| | First-party plugins | Third-party plugins |
|---|---|---|
| Form | Rust crate in `plugins/`, statically linked | Luau scripts, each plugin in its own sandboxed VM |
| Loading | At startup, from the distribution (`dist/*.toml`) | From disk (installed by Sleip), only when needed |
| Trust | Our own reviewed code | Sandboxed: only the capabilities granted to it |
| Cost | Direct calls, zero overhead | One Luau VM (about 1–2 MB when active); calls copy data |
| API | `plugin-sdk`, the Rust front end | The Luau front end of the same API |
| Failures | `catch_unwind` at the boundary; the plugin is disabled | Lua error, memory limit or missed deadline; the plugin is disabled and restarted with backoff |

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

Native and Luau plugins follow the same rules, so they behave the same way.

- **Short, synchronous calls**:
  - The kernel calls into the plugin (`activate`, `on_event`, `on_command`, `render`, `on_ui_event`), and the plugin calls kernel services.
  - Long-running work goes to asynchronous host services (search, timers, processes), whose results come back as events.
- **Owned data**: plugins receive copies, never references into kernel state.
- **No re-entrancy**: events generated during a callback are queued and delivered only after the callback returns.
- **No threads, no blocking I/O**: time is provided by the host, through timers and `tick` events that are sent only on request.
- **Filtered, batched events**: the kernel delivers only the events a plugin subscribed to, batched per frame.
- **Rendering**:
  - `render(slot)` is called only when the plugin asks for an update (`request_render`) or the theme changes. The result is cached.
  - Native plugins run on the main thread. Luau plugins run on the plugin executor, with a deadline per call.
  - If a Luau plugin misses its deadline, the last valid view tree stays on screen.

## One API, two front ends

The API is defined once, in Rust: the `HostApi` trait and its types live in `plugin-sdk`, and the kernel implements the trait. Each tier reaches it through its own front end.

**Rust**, for first-party plugins:

```rust
use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error};
use lokyr_plugin_sdk::host::Binding;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

#[derive(Default)]
struct Search { query: String }

impl Plugin for Search {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "search.open".to_owned(),
            title: "Find".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
        })?;
        cx.bind(Binding {
            keys: "cmd+f".to_owned(),
            command: "search.open".to_owned(),
            args: Args::new(),
            when: Some("pane.focused".to_owned()),
        })
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        match name {
            "search.open" => { /* the find bar */ Ok(()) }
            _ => Err(Error::new(Code::NotFound, format!("no command `{name}`"))),
        }
    }
    // Later: on_event, for results and panes that close, and render, for the find bar.
}

export_plugin!(Search); // a registration function, which gen-dist wires into the app
```

**Luau**, for third-party plugins. The entry module returns a table of callbacks, and the host passes `cx` to each of them:

```luau
--!strict
-- src/init.luau
local Search = {}

function Search.activate(cx: Cx)
    cx.commands.register("search-plus.open", "Advanced search")
    cx.keymap.bind("cmd+shift+f", "search-plus.open", "pane.focused")
end

function Search.on_command(cx: Cx, name: string, args: Args)
    if name == "search-plus.open" then
        cx.ui.request_render("pane-overlay")
    end
end

function Search.on_event(cx: Cx, events: { Event })
    -- results, pane closed, …
end

function Search.render(cx: Cx, slot: string): View
    -- the find bar
end

return Search
```

- **Rust front end**: `Cx` calls `&mut dyn HostApi`, an object-safe trait. `export_plugin!(T)` generates a registration function, which `xtask gen-dist` wires into the app.
  - The host's API and the `Plugin` trait come with `plugin-sdk`'s `unstable` feature while they grow.
  - A command's calls come to the plugin's `on_command`, by the command's name, as in Luau: only data crosses the boundary, never code or references into the kernel's state. A table of handlers, one a command, can come later inside the SDK, over `on_command`, without changing the host.
  - What changes the app, as opening or closing a window, quitting, the font's size or running another command, comes once the plugin's call returns; a command that runs commands goes 16 deep at most.
  - The plugins' bindings come after the app's and before the user's `[keys]`, which win over them.
- **Luau front end**:
  - `plugin-host-luau` binds the same `HostApi` to Luau.
  - The bindings and the Luau type definitions are generated from the Rust types where possible.
  - The semantics are implemented once, in the kernel, behind both front ends.
- **Names**: the Luau API uses the Rust names (`request_render`, `on_event`), so the two front ends read the same and share one reference.
- **Conformance**: the conformance suite runs the same scenarios through both front ends, with Rust and Luau test plugins, and compares their command and event traces.
- **Testing for plugin authors**:
  - The Rust SDK ships a fake host for unit tests. It simulates events, lets tests assert on commands and view trees, and snapshots view trees.
  - For Luau, `sleip test` runs the plugin in the same headless host.

## The Luau API

- **Shape**:
  - A plugin is a package of Luau modules. Its entry point, `src/init.luau`, returns the table of callbacks shown above. Every callback is optional.
  - `require` resolves only the modules inside the plugin's own package.
- **Types**: a definitions file declares the API's types (`Cx`, `Event`, `View`, …) for Luau's type checker. Sleip's templates include it, with a `luau-lsp` configuration that points to it. Plugins are written with `--!strict`.
- **Errors**: host calls that can fail return their result, or `nil` and an error, as Lua's standard library does. A Lua error raised by a callback ends the call, and counts as a plugin failure.
- **Data**:
  - Small values are plain tables, possibly nested: command arguments, view trees, events.
  - The host stores view trees as a flat arena of nodes and diffs them between renders.
  - Bulk data uses buffers. For example, a highlighter returns its spans in one `buffer`, which the host reads in one copy: reading 100000 spans from tables costs 6–7 ms, against about 1.5 ms through Lua's C API ([F0-22](tasks/F0-foundations.md#f0-22--spike-lua-plugin-runtimes)).
- **Handles to host objects**: windows, tabs and panes are opaque integer ids, as in the Rust front end.
- **Plugin-defined events**: `custom` events carry a name, a schema id and a plain value. The host encodes the value as CBOR for Rust plugins.

### Versioning

- The API version is the version of `plugin-sdk` (`plugin-sdk 0.1.0` ↔ API 0.1).
- New APIs start as unstable: in Rust behind the `unstable` feature, in Luau under `cx.unstable`. They become stable in a minor release.
- The manifest declares the API version it requires (`api = "^0.1"`). The host supports the current major version and, through adapters, the previous one.
- Before 1.0, minor versions may break compatibility. After 1.0, semver is followed strictly.

### Illustrative draft

> This sketch only illustrates the shape of the API. Names and signatures will be settled in F3, and the definitions file will be generated from the Rust types.

```luau
-- lokyr.d.luau: the types of the plugin API, for the type checker.

type WindowId = number
type TabId = number
type PaneId = number

-- Command arguments: named values of a flat schema, each a string, an
-- integer, a number or a boolean.
type Args = { [string]: string | number | boolean }

-- Plugins should prefer theme tokens (e.g. "tab.active.bg") to literals.
type Color = string | { r: number, g: number, b: number, a: number? }

type Direction = "left" | "right" | "up" | "down"

-- An error's code, whose hundreds tell its category: 1xx usage, 2xx
-- failure, 3xx security, 4xx fault (ADR 0019).
type Error = {
    code: number,
    kind: "not-found" | "invalid-args" | "invalid" | "failed" | "denied" | "fault",
    message: string,
}

type TabInfo = {
    id: TabId,
    window: WindowId,
    title: string,
    active: boolean,
    panes: { PaneId },
}

type Style = {
    direction: ("row" | "column")?,
    gap: number?,
    padding: number?,
    grow: number?,
    background: Color?,
    foreground: Color?,
    corner_radius: number?,
}

-- A view is a tree of nodes; the host flattens it into its arena.
type Node = {
    key: number?,
    kind: "container" | "text" | "icon" | "input" | "spacer",
    text: string?, -- for text and icon nodes
    value: string?, -- for input nodes
    placeholder: string?,
    style: Style?,
    children: { Node }?,
    role: string?,
    label: string?,
}
type View = Node

type Line = { text: string, wrapped: boolean }

type Query = {
    pattern: string,
    regex: boolean?,
    case_sensitive: boolean?,
    whole_word: boolean?,
}

type Event =
    { kind: "tab-created", tab: TabId }
    | { kind: "tab-closed", tab: TabId }
    | { kind: "pane-focused", pane: PaneId }
    | { kind: "title-changed", pane: PaneId, title: string }
    | { kind: "search-progress", id: number, matches: number, done: boolean }
    | { kind: "custom", name: string, schema: string, payload: any }

type UiEvent = {
    slot: string,
    key: number,
    kind: "click" | "double-click" | "hover-enter" | "hover-leave" | "context-menu" | "input-changed" | "submit",
    text: string?,
}

type Cx = {
    commands: {
        register: (name: string, title: string) -> (boolean, Error?),
        execute: (name: string, args: Args?) -> (boolean, Error?),
    },
    keymap: {
        -- `keys`: e.g. "cmd+f" or "ctrl+a>c"; `when`: e.g. "pane.focused && !search.open".
        bind: (keys: string, command: string, when: string?) -> (boolean, Error?),
    },
    workspace: {
        tabs: (window: WindowId) -> { TabInfo },
        new_tab: (window: WindowId) -> (TabId?, Error?),
        close_tab: (tab: TabId) -> (boolean, Error?),
        move_tab: (tab: TabId, window: WindowId, index: number) -> (boolean, Error?),
        set_tab_title: (tab: TabId, title: string?) -> (boolean, Error?),
    },
    layout: {
        split: (pane: PaneId, dir: Direction, ratio: number) -> (PaneId?, Error?),
        resize: (pane: PaneId, dir: Direction, cells: number) -> (boolean, Error?),
        focus: (pane: PaneId) -> (boolean, Error?),
        toggle_zoom: (pane: PaneId) -> (boolean, Error?),
    },
    ui: {
        request_render: (slot: string) -> (),
    },
    terminal: {
        -- Requires `terminal-read`; returns a `denied` error without it.
        read_lines: (pane: PaneId, start: number, count: number) -> ({ Line }?, Error?),
    },
    search: {
        -- Runs in the background: results arrive as `search-progress` events.
        start: (pane: PaneId, query: Query) -> (number?, Error?),
        select_next: (id: number, forward: boolean) -> (boolean, Error?),
        cancel: (id: number) -> (),
    },
    clipboard: {
        write_text: (text: string) -> (boolean, Error?),
        read_text: () -> (string?, Error?),
    },
}

-- What a plugin's entry module returns.
type Plugin = {
    activate: ((cx: Cx) -> ())?,
    deactivate: ((cx: Cx) -> ())?,
    on_event: ((cx: Cx, events: { Event }) -> ())?,
    on_command: ((cx: Cx, name: string, args: Args) -> ())?,
    render: ((cx: Cx, slot: string) -> View)?,
    on_ui_event: ((cx: Cx, event: UiEvent) -> ())?,
}
```

## Commands

- **A command** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)) has a name, `namespace.action`, each part of lowercase letters, digits and `-`, from a letter, as `window.new`, `tabs.goto` or `search-plus.open`; a title, for the menus and the palette; a category; the capabilities that its action needs; and its parameters, a flat schema: each named as the parts are, of a string, an integer, a number or a boolean, and required, or optional with a default of its kind.
- **The kernel's registry**, `kernel::commands::Registry`, registers the commands of the app and of the plugins, and checks a call before its action runs: its name, and its arguments, to which it adds the defaults of those left out; an integer is a number too. A plugin's call is checked as well against the commands that its manifest declares and the capabilities that it has, its own commands aside. The action runs where it can: in the app for those that the platform does, in the plugin that registered the command otherwise.
- **The app's commands**: `app.quit` and `window.new`, which its menu runs.
- **Errors**: a code, never renumbered, whose hundreds tell the category and how serious it is, a kind, as the Luau API names it, and a message:

  | Code | Kind | Category | Meaning |
  |---|---|---|---|
  | 101 | `not-found` | Usage | No command of the name |
  | 102 | `invalid-args` | Usage | An argument that the command does not take |
  | 103 | `invalid-args` | Usage | A required argument missing |
  | 104 | `invalid-args` | Usage | An argument of another kind than its parameter's |
  | 105 | `invalid` | Usage | A command's or a parameter's name that is not one |
  | 106 | `invalid` | Usage | A command registered already |
  | 107 | `invalid` | Usage | A schema that repeats a parameter, or whose default is of another kind |
  | 108 | `invalid` | Usage | A binding whose keys, command or context is not one |
  | 201 | `failed` | Failure | The command ran, and failed |
  | 301 | `denied` | Security | A command that the plugin's manifest does not declare |
  | 302 | `denied` | Security | A capability that the command needs, which the plugin lacks |
  | 401 | `fault` | Fault | A command that broke, as by a panic |

  Usage errors go to the caller, failures to the log and, later, a notice in the window, security errors to the log and the audit log, and faults to the log, with a notice, and can disable the plugin.

## Capabilities

| Capability | Allows | Risk |
|---|---|---|
| *(none)* | Registering commands, and running those that the manifest declares; keymap, events, UI, themes, config, storage, logging, timers | — |
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
| `env-read:<name>` | Reading the listed variables of the app's environment and of the panes' programs', with `*` as `AWS_*` | High: they often hold secrets, as `GITHUB_TOKEN` |
| `input-read` | Seeing the keys typed before they reach the program | Very high: a keylogger's power |
| `history-read` | Reading the commands that the shell ran and their output, which shell integration marks | High: a password typed by mistake ends up there |
| `process-read` | Seeing the panes' processes: their command lines and working directories | Medium: command lines can hold passwords |

- **Commands** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)): each has a category, `app`, `windows`, `tabs`, `panes`, `input`, `clipboard` or `view` to start with, and the capabilities that its action needs, as `pane.paste` needs `clipboard-read` and `pty-write`. A plugin declares in its manifest the commands that it runs, `[uses] commands`, by name or by namespace, as `tabs.*`: one that it did not declare fails with `denied`, 301, and one whose capabilities it lacks with `denied`, 302. The commands that the user runs, through the keys, the menus or the palette, and the plugin's own need no declaration.
- **Declaration and consent**:
  - Capabilities are declared in the manifest. Sleip shows them at install time, with the commands that the plugin runs, grouped by category, and the capabilities that those need; the app asks for confirmation on first use.
  - Decisions are stored in `~/.config/lokyr/permissions.toml`, which can be versioned with dotfiles and revoked at any time.
- **Enforcement**:
  - Every host function exists in every plugin. Without the capability it returns a `denied` error, so a plugin can treat a capability as optional and request it at the moment it's needed.
  - The VM itself reaches nothing outside the computation: it has no `io`, `os` or `debug` library, and its `require` stays inside the package. Files, processes and the network exist only as host functions, limited to the granted paths and hosts.
- **Networking without an HTTP client in the app**:
  - Requests from plugins with `net:<host>` go through a helper process started on demand (`sleip net-helper`), which enforces the host allowlist.
  - The app itself still contains no HTTP/TLS client ([ADR 0008](adr/0008-separate-package-manager.md)).
- **First-party plugins** are trusted code and don't go through capabilities, but they are still limited to the SDK.

## Lifecycle

```
installed ──▶ resolved ──▶ consent ──▶ loaded ──▶ active
                               ▲                     │ error / deadline / panic
                               │                     ▼
                         re-enabled ◀── disabled ◀── restart with backoff
```

- **Lazy activation** through `activation.on`: `startup`, `command:<glob>`, `event:<name>`, `slot:<name>`, `osc:<number>`.
- **Zero cost without third-party plugins**: no Luau VM exists until the first one activates.
- **Backoff**: after 3 failures within a minute, the plugin stays disabled until the user re-enables it.
- **Hot reload**: `sleip dev` watches the plugin's sources and reloads it into the running app over IPC. Reloading means creating a fresh VM: nothing needs to be compiled ahead.

## Limits and isolation

- **One VM per plugin**: plugins share no Lua state, so one plugin cannot see or change another's globals.
- **Memory**: a limit per VM (64 MB by default, configurable per plugin). An allocation over the limit raises a memory error, which ends the call.
- **CPU**:
  - A deadline per call type (e.g. 16 ms for `render`, 250 ms for commands).
  - A watchdog thread installs Luau's interrupt only once the deadline has passed, so the check costs nothing until then. The interrupt then ends the call at the next loop iteration or function call, native code included.
  - The watchdog runs **only while a plugin is executing**, so there are no wakeups when idle.
  - The watchdog writes the interrupt from another thread, which races with the code that reads it. The vendored Luau gets an atomic store for that field, unless Luau guarantees that the race is harmless.
- **Failures**: a failing plugin never affects the terminal. The plugin is disabled and the user gets a notification. In native plugins, `catch_unwind` at the boundary stops panics.
- **Hostile plugins in CI**: an infinite loop, a memory bomb, a runaway recursion and the escape probes of [F0-22](tasks/F0-foundations.md#f0-22--spike-lua-plugin-runtimes) run against every build, so the sandbox stays closed.

## Native code and the hardened runtime

- **Compilation**: when a plugin loads, Luau compiles its sources to bytecode, and its code generator turns the plugin's functions into native code. F4 measures this against the [activation budget](PERFORMANCE.md#budgets). If needed, the host caches the bytecode it compiled itself, keyed by the source hash and the Luau version, in a directory with 0700 permissions.
- **Sources only**: packages carry Luau sources, never bytecode. The host compiles every script itself.
- **macOS**:
  - The code generator maps its code with `MAP_JIT`, so the app needs the `com.apple.security.cs.allow-jit` entitlement, and no other hardened-runtime exception.
  - This was checked in a notarized app, launched as a download ([F0-22](tasks/F0-foundations.md#f0-22--spike-lua-plugin-runtimes)).
  - Without the entitlement, Luau runs the same code in its interpreter, about 5× slower on loops.
- **Luau version**: Luau comes from mlua's vendored sources. Its version is pinned and upgraded deliberately, with the hostile plugins and the conformance suite as the gate.

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
| `plugin-manager` | UI for Sleip (calls `sleip --json`) | F4 |
| `shell-integration`, `blocks` | Prompt marks, command + output blocks | F5a |
| `tmux-cc`, `quick-terminal`, `triggers`, `profiles`, `broadcast` | iTerm2 power features | F5b |
| `hints`, `images` | Hints, URLs, inline images | F5c |

**Rules** (details in [ARCHITECTURE.md](ARCHITECTURE.md#dependency-rules)):
- depend only on `plugin-sdk`;
- use `#![forbid(unsafe_code)]`, with clippy `disallowed-methods` on, so they follow the same rules as Luau plugins;
- pass the conformance suite.

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

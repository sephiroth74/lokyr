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
| Config | A `[plugins.<name>]` section with a JSON schema and migrations, or a file of the plugin's own, which is [open](ROADMAP.md#open-questions) | `[plugins.tabs] position = "top"` | F3 |
| Storage | Persistent per-plugin state | search history | F3 |
| System | Clipboard, notifications, opening URLs and files, global hotkeys and quick windows in F3; processes and the network, through the app to Sleip's helper, from F4 (with capabilities) | notify when a command finishes | F3–F4 |
| Protocols | Handlers for rare OSC/DCS/APC sequences | iTerm2's OSC 1337 | F5 |
| Triggers | Regexes over completed lines, with batched callbacks | highlight `ERROR` in red | F5b |

### UI slots

| Slot | Providers | Used for | Region |
|---|---|---|---|
| `tab-bar` | One, chosen in the config | The tab bar | In the titlebar past the window's buttons, of the titlebar's height, or below a titlebar of the window's title, of its view's height; at the window's top in full screen |
| `status-bar` | Many (ordered segments) | Status information | Across the window's bottom, of its view's height, which the panes give up |
| `window-strip` | Many | A thin strip across the window's top, under the titlebar or the tab bar: the programs' progress | Across the panes' top, over them, taking none of their rows |
| `pane-header` | One | Pane titles in splits | Across its pane's top, of its view's height, above the grid |
| `pane-overlay` | Many | Find bar, hints, the command's sticky header, which takes no row of the grid | Its pane's grid, where its floating panels anchor |
| `window-overlay` | Many | Command palette, notifications | Below the titlebar and the tab bar, to the window's bottom |
| `popup` | Many | Custom menus and tooltips | The whole window |
| `sidebar` | Many (F5+) | Side panels | |

A slot of no view takes no room. `ui-core`'s `chrome` gives the regions of a window's size, its titlebar's height and its buttons' room, the tab bar's place and the heights of the views, as the slots' trees take them laid out across the window; each slot's view is a surface of its own, drawn over the panes in the window's one drawable and taking the pointer before them.

The dividers between panes are no slot's: the host draws them from the layout tree, in the theme's tokens, and gives their drags to `splits`, which decides what they do.

## Execution model

Native and Luau plugins follow the same rules, so they behave the same way ([ADR 0028](adr/0028-plugin-calls-as-messages.md)).

- **Short calls, of messages**:
  - The kernel calls into the plugin (`activate`, `on_event`, `on_command`, `render`, `on_ui_event`), and the plugin sends the host requests, `plugin-sdk`'s `message::Call`, each with its identity and the capability that it needs, which the host answers with a `Response`. The host checks the capability against the plugin's: a built-in plugin has every one until its manifest says which; a call that comes as another plugin's fails.
  - Long-running work goes to asynchronous host services (search, timers, processes), whose results come back as events.
- **Reads at the host's time**: a request reads the main thread's state as the host handles it; an id that is gone gives `not-found`. A read of much text runs on a thread of the host's, which holds a pane's lock as briefly as a frame's copy.
- **Writes queued, for every plugin**: what a plugin changes, the kernel's state and the app's, the host applies in order as the plugin's call returns, on the main thread as on a worker and in Luau; plugins never change the state in place. A write to a pane that is not there fails as it is asked. The kernel's writes, as a selection, a paste or the clipboard's text, come first, then the app's requests, as a window to open or a command to run; a read in the same call reads the state before the call's writes. A plugin's declarations, its commands and its keys, take effect at once, as the registry checks them.
- **Owned data**: plugins receive copies, never references into kernel state.
- **No re-entrancy**: events generated during a callback are queued and delivered only after the callback returns; those that a delivery's calls emit, in the next batch.
- **No threads of their own, no blocking I/O**: time is provided by the host, through timers and `tick` events that are sent only on request.
- **Filtered, batched events**: a plugin subscribes to the events that it names, `cx.subscribe`, the workspace's, as `tab-created`, and plugins' own, `namespace.event`; the kernel gathers the workspace's changes and the events that plugins emit, and delivers them to `on_event` in their order, each plugin those that it subscribed to, as a batch, once a turn of the main thread, which a frame, a key, a click or a wake is.
- **Rendering**:
  - `render(slot)` is called only when the plugin asks for an update (`request_render`) or the theme changes. The result is cached.
  - Native plugins run on the main thread, unless their manifest asks for a thread of their own, `thread = "worker"`, as the user chose on 1 November 2026: each such plugin then has a thread of the host's, which makes all its calls, its renders too, with the same messages as a Luau plugin's, and the host takes their results on the main thread as they are ready, never waiting for them; a call of the main thread's plugins that takes long is logged as it returns, and stopped by nothing, as the plugins are ours. The main thread stays for the plugins that must answer at once, as the tab bar's. Luau plugins run on the plugin executor, with a deadline per call.
  - If a Luau plugin misses its deadline, the last valid view tree stays on screen.

## One API, two front ends

The API is defined once, in Rust: a protocol of requests, responses and events, whose types live in `plugin-sdk`, and which the kernel serves ([ADR 0028](adr/0028-plugin-calls-as-messages.md)). Each tier reaches it through its own front end.

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

- **Rust front end**: `Cx`'s methods make the requests, which the host serves in place for a plugin of the main thread and through a channel for a worker's. `export_plugin!(T)` generates a registration function, which `xtask gen-dist` wires into the app.
  - The host's API and the `Plugin` trait come with `plugin-sdk`'s `unstable` feature while they grow.
  - A command's calls come to the plugin's `on_command`, by the command's name, as in Luau: only data crosses the boundary, never code or references into the kernel's state. A table of handlers, one a command, can come later inside the SDK, over `on_command`, without changing the host.
  - What changes the kernel or the app, as a tab's title, opening or closing a window, quitting, the font's size or running another command, comes once the plugin's call returns, in its order; a command that runs commands goes 16 deep at most.
  - The plugins' bindings come after the app's and before the user's `[keys]`, which win over them.
- **Luau front end**:
  - `plugin-host-luau` binds the same messages to Luau.
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
  - The host stores view trees as a flat arena of nodes and diffs them between renders. A node that holds state, as a text field or a list, has a key, by which the host keeps its state, its text, caret, selection and composition, its scroll; the plugin learns of changes through events, and sets a field's text through a request. A large list is data, its items flat with a row's template, which the host instantiates for the rows that show: its plugin gives its count and a window of its items, those that the host asks for about the rows that show, by `list-range`, as they scroll past the window.
  - Bulk data uses buffers. For example, a highlighter returns its spans in one `buffer`, which the host reads in one copy: reading 100000 spans from tables costs 6–7 ms, against about 1.5 ms through Lua's C API ([F0-22](tasks/F0-foundations.md#f0-22--spike-lua-plugin-runtimes)).
- **Handles to host objects**: windows, tabs and panes are opaque integer ids, as in the Rust front end.
- **Rows** are numbers, which output, the view's scrolling and the scrollback's limit leave a row, until it leaves the scrollback; a resize numbers the rows anew. A command of a cell, as a click's, takes the number of the row clicked, as the drawn frame showed it ([ARCHITECTURE.md](ARCHITECTURE.md#kernel-data-model)).
- **Plugin-defined events**: `custom` events carry a name, of the plugin's namespace, as `search.progress`, a schema id, as `search.progress/1`, and a plain value, the SDK's `message::Data`: text, numbers, true or false, and lists and maps of them, as a Luau table is. `cx.emit` emits one, which comes after the call returns.

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
    pointer: ("arrow" | "text" | "pointing-hand")?, -- its children's too, unless they give their own
    opacity: number?, -- from 0 to 1, of its children too
    transition: { duration: number?, easing: ("linear" | "ease-in" | "ease-out" | "ease-in-out")? }?, -- its moves and fades, over seconds
}

-- A view is a tree of nodes; the host flattens it into its arena. A node
-- that holds state, a field or a list, needs its key, by which the host
-- keeps that state; a field's text is the host's, which events tell. A key
-- is unique in the whole tree, as "tab-3.close", as the events tell a node
-- by its key alone; a list's rows are told apart by their item's index.
type Node = {
    key: string?,
    kind: "container" | "text" | "icon" | "field" | "button" | "toggle" | "separator" | "list" | "scroll"
        | "progress" | "spacer" | "tabs",
    text: string?, -- for text and icon nodes; a button's and a toggle's label
    icon: string?, -- a button's and a toggle's icon, before its label
    on: boolean?, -- for toggle nodes, the plugin's state
    disabled: boolean?, -- for button and toggle nodes: dimmed, and takes neither a click nor the focus
    placeholder: string?, -- for field nodes, shown while the field is empty
    progress: {
        state: ("normal" | "error" | "paused" | "indeterminate")?,
        value: number?, -- from 0 to 1
        shape: ("bar" | "ring")?,
        radius: number?, -- a bar's corners', a pill's without it
        thickness: number?, -- a ring's line's
        segment: number?, -- an indeterminate one's moving part, of the bar or of the turn, from 0 to 1
        period: number?, -- an indeterminate one's sweep or turn, in seconds
    }?, -- for progress nodes: its track of the style's background, its fill of the style's color
    count: number?, -- for list nodes, of all their items
    first: number?, -- for list nodes, the index of the first of `items`
    items: { ListItem }?, -- for list nodes, flat: a window of them, from `first`, as the host asks for them
    row: Node?, -- for list nodes: the template of a row, of containers, texts, icons, spacers and separators
    row_height: number?, -- for list nodes, of rows of one height, a point at least; without it, each row takes its own, a point at least
    tabs: { { label: string, icon: string?, disabled: boolean? } }?, -- for tabs nodes, in their order: a control of them over the box of its page, its child
    chosen: number?, -- for tabs nodes, the tab chosen, whose page is its child; a click or the arrows choose another, as "select" tells
    style: Style?,
    focusable: boolean?, -- takes the keyboard's focus, by its key
    tooltip: string?, -- shown as the pointer rests over the node, by its key
    children: { Node }?,
    role: string?,
    label: string?,
}
type View = Node

-- A list's item: the texts that its row's template shows, by their places
-- in it, its texts' and its icons' in the tree's order; its index tells it
-- apart.
type ListItem = { texts: { string } }

-- A row's text, by its number, without the spaces that end it, but where it wraps into the next.
type Line = { row: number, text: string, wrapped: boolean }

-- The cells of a row from a column to another, of one style, as the program gave it.
type StyleRun = { first: number, last: number, style: { foreground: any?, background: any?, bold: boolean, italic: boolean, underline: string? } }

-- A decoration's look: an underline, of its own color or the text's, a background, and the pointing hand over its cells.
type DecorationLook = {
    underline: ("single" | "double" | "curly" | "dotted" | "dashed")?,
    underline_color: { number }?, -- red, green, blue
    background: { number }?,
    pointing: boolean?,
}

type Query = {
    pattern: string,
    regex: boolean?,
    case_sensitive: boolean?,
    whole_word: boolean?,
}

-- The workspace's changes, as its operations record them; an index is a
-- tab's place among its window's tabs.
type Event =
    { kind: "window-opened", window: WindowId }
    | { kind: "window-closed", window: WindowId }
    | { kind: "tab-created", tab: TabId, window: WindowId, index: number }
    | { kind: "tab-closed", tab: TabId, window: WindowId }
    | { kind: "tab-activated", tab: TabId, window: WindowId }
    | { kind: "tab-moved", tab: TabId, from: WindowId, window: WindowId, index: number }
    | { kind: "pane-created", pane: PaneId, tab: TabId }
    | { kind: "pane-closed", pane: PaneId, tab: TabId }
    | { kind: "pane-focused", pane: PaneId, tab: TabId }
    | { kind: "pane-moved", pane: PaneId, from: TabId, tab: TabId }
    | { kind: "layout-changed", tab: TabId }
    | { kind: "title-changed", pane: PaneId, title: string }
    | { kind: "search-progress", id: number, matches: number, done: boolean }
    -- The pointer over another cell of a pane, or out of it, or the modifiers changed over it; while Command is up, its
    -- moves come only as the system gives them, as a program that tracks the mouse asks.
    | { kind: "pointer", pane: PaneId, at: { number }?, mods: { shift: boolean, control: boolean, option: boolean, command: boolean } }
    -- A read of rows that the plugin asked for, to it alone; incomplete if a resize renumbered the rows.
    | { kind: "lines", id: number, pane: PaneId, lines: { Line }, complete: boolean }
    | { kind: "custom", name: string, schema: string, value: any }

type UiEvent = {
    slot: string,
    key: string,
    kind: "click" | "double-click" | "middle-click" | "hover-enter" | "hover-leave" | "context-menu"
        | "input-changed" | "submit" | "focus" | "blur" | "scroll"
        | "drag-start" | "drag-move" | "drag-end" | "drag-cancel" | "set-value" | "toggle"
        | "select" | "activate" | "list-range",
    text: string?, -- a field's text, as the host holds it: at each change, "input-changed", and as Return goes to it, "submit"
    item: number?, -- a list's item, selected or activated
    first: number?, count: number?, -- the items of a list that the host asks for, which the next render gives
    x: number?, y: number?, -- a drag's, from its start, or a scroll's, in points
    value: string?, -- set by accessibility, as VoiceOver's; its press comes as a click, its focus as a focus
    on: boolean?, -- a toggle's state asked, the other than its render's, which the next render gives
}

type Cx = {
    events: {
        -- The workspace's events, as "tab-created", and plugins' own, as "search.progress".
        subscribe: (names: { string }) -> (boolean, Error?),
        -- An event of the plugin's namespace, which comes after the call returns.
        emit: (name: string, schema: string, value: any) -> (boolean, Error?),
    },
    commands: {
        register: (name: string, title: string) -> (boolean, Error?),
        execute: (name: string, args: Args?) -> (boolean, Error?),
    },
    keymap: {
        -- `keys`: e.g. "cmd+f" or "ctrl+a>c"; `when`: e.g. "pane.focused && !search.open", where a
        -- plugin's panels give `namespace.open` while one shows, and `namespace.focused` while one has the keyboard.
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
        -- Sets the text of a field of the slot's view, by its key, cut to the field's 10 000 characters: its caret at its end, and no "input-changed".
        set_text: (slot: string, key: string, text: string) -> (boolean, Error?),
    },
    terminal: {
        -- Requires `terminal-read`; returns a `denied` error without it. Reads 100 000 rows at most, from the row numbered
        -- `first`, on a thread of the host's, a part at a time: returns the read's id, and the rows come as a "lines" event.
        read_lines: (pane: PaneId, first: number, count: number) -> (number?, Error?),
        -- The styles of a row, by its number, as runs of cells of one style.
        styles: (pane: PaneId, row: number) -> ({ StyleRun }?, Error?),
        -- Decorates the cells from one to another, in reading order, each { column, row's number }: returns the
        -- decoration's id. It keeps its text as output scrolls it and a resize reflows it, and goes with it.
        decorate: (pane: PaneId, from: { number }, to: { number }, look: DecorationLook) -> (number?, Error?),
        undecorate: (pane: PaneId, id: number) -> (boolean, Error?),
        -- The logical line of a row, by its number, its wrapped rows joined as far as `reach` rows before it and after it, 256 at most, with each character's cell.
        line_text: (pane: PaneId, row: number, reach: number) -> (LineText?, Error?),
        -- The target of the OSC 8 link at a cell, by its row's number.
        link_at: (pane: PaneId, column: number, row: number) -> (string?, Error?),
    },
    process: {
        -- Requires `process-read`: the directory of the program in the pane's foreground.
        working_directory: (pane: PaneId) -> (string?, Error?),
    },
    system: {
        -- Requires `open`: a URL of `http`, `https`, `mailto` or `file`, or a path from `/` or `~`.
        open: (target: string) -> (boolean, Error?),
        -- Whether `open` would open `target`: a URL of its schemes, or a path that is there.
        opens: (target: string) -> (boolean, Error?),
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
    -- Whether a command would do something with `args`, as a menu asks before it dims its item; what
    -- the call asks of the app is dropped. Without it, every command would.
    applies: ((cx: Cx, name: string, args: Args) -> boolean)?,
    render: ((cx: Cx, slot: string) -> View)?,
    on_ui_event: ((cx: Cx, event: UiEvent) -> ())?,
}
```

## Commands

- **A command** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)) has a name, `namespace.action`, each part of lowercase letters, digits and `-`, from a letter, as `window.new`, `tabs.goto` or `search-plus.open`; a title, for the menus and the palette; a category, which also tells the menu that lists it; the capabilities that its action needs; its parameters, a flat schema: each named as the parts are, of a string, an integer, a number or a boolean, and required, or optional with a default of its kind; and, if it needs one, the context in which it can run, written as a binding's `when`, as Copy's `selection`, outside of which its menu's item is dimmed. Its plugin's `applies` can dim the item too, where the command would do nothing, as Open Link where no link is. Its title, in the language that Lokyr shows, comes from its plugin's own catalogs ([ADR 0020](adr/0020-localization.md)), by the message `command-` and the command's name with `-` for its `.`, as `command-pane-copy`, or English's; a title that they lack is the one registered. A native plugin gives its catalogs through `Plugin::catalogs`; a Luau plugin carries them in its package.
- **The pane's context menu**, of a right click, lists the commands of a cell, which take the `column` and the `row` of the cell clicked, then those of the clipboard and of the editing that need no arguments; its commands run in the pane that it opened over ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).
- **The kernel's registry**, `kernel::commands::Registry`, registers the commands of the app and of the plugins, and checks a call before its action runs: its name, and its arguments, to which it adds the defaults of those left out; an integer is a number too. A plugin's call is checked as well against the commands that its manifest declares and the capabilities that it has, its own commands aside. The action runs where it can: in the app for those that the platform does, in the plugin that registered the command otherwise.
- **The app's commands**, whose actions the platform does: `app.quit`, `window.new` and `window.close`; the appearances, `app.appearance-system`, `app.appearance-light` and `app.appearance-dark`; and `pane.look-up` and `pane.man-page`, the context menu's Look Up and Open man Page.
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
  | 109 | `invalid` | Usage | A config that does not read, or whose values are not its keys' |
  | 201 | `failed` | Failure | The command ran, and failed |
  | 202 | `failed` | Failure | What an action needs is not there, as a font's family that is not installed |
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
| `user-commands` | Running any command as the user's, without declaring it, as the palette does; reading the whole registry, with each command's plugin, keys and context, and the plugins installed ([ADR 0024](adr/0024-user-commands.md)) | Very high: `pane.send-text` types into the shell |

- **Commands** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)): each has a category, `app`, `windows`, `tabs`, `panes`, `input`, `clipboard` or `view` to start with, and the capabilities that its action needs, as `pane.paste` needs `clipboard-read` and `pty-write`. A plugin declares in its manifest the commands that it runs, `[uses] commands`, by name or by namespace, as `tabs.*`: one that it did not declare fails with `denied`, 301, and one whose capabilities it lacks with `denied`, 302. The commands that the user runs, through the keys, the menus or a plugin with `user-commands`, as the palette, and the plugin's own need no declaration. Without `user-commands`, a plugin reads the commands' names, titles, categories and parameters, not their plugins.
- **Declaration and consent**:
  - Capabilities are declared in the manifest. Sleip shows them at install time, with the commands that the plugin runs, grouped by category, and the capabilities that those need; the app asks for confirmation on first use.
  - Decisions are stored in `~/.config/lokyr/permissions.toml`, which can be versioned with dotfiles and revoked at any time.
- **Enforcement**:
  - Every host function exists in every plugin. Without the capability it returns a `denied` error, so a plugin can treat a capability as optional and request it at the moment it's needed.
  - The VM itself reaches nothing outside the computation: it has no `io`, `os` or `debug` library, and its `require` stays inside the package. Files, processes and the network exist only as host functions, limited to the granted paths and hosts.
- **The app as the plugins' gateway to the network**, from F4:
  - A plugin has no network of its own: a request with `net:<host>` goes to the app, which checks its capability and its host against the allowlist, and logs it.
  - The connection runs in a helper process started on demand, `sleip net-helper`, of the Sleip that the bundle holds, which enforces the allowlist too, so that no response from the network is parsed in the process that sees what the user types.
  - Lokyr's own code contains no HTTP/TLS client; Sparkle's updater is the app's one network client ([ADR 0008](adr/0008-separate-package-manager.md), [ADR 0026](adr/0026-app-updates.md), [ADR 0029](adr/0029-update-feed-and-releases.md)).
- **First-party plugins** are trusted code, which holds every capability and runs any command ([ADR 0024](adr/0024-user-commands.md)), but they are still limited to the SDK. Each has a manifest, `plugin.toml`, of the kind `builtin`, whose capabilities tell what it uses, which the build checks; the kernel adds it by its manifest's name, as `lokyr/links`.
- **Other plugins** hold the capabilities that their manifest declares, and run their own commands, those that they registered, and those of their `[uses]`: a request that needs another capability fails with 302, and a run of another command with 301, both `denied`. Each plugin's namespace, its name's after its owner, as `links` of `lokyr/links`, is its own: a second plugin of a namespace is refused; such a plugin registers commands of its namespace alone, which no other plugin's commands, nor the app's, have, and every plugin emits events of its own namespace alone.

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
| `links` | ⌘-click opens a link, a URL or a path of the text; later hints, `file:line` in the editor and smart selection | F2, F5c |
| `tabs` | iTerm2-style tab bar and tab commands | F3 |
| `splits` | Split commands, dividers and interaction | F3 |
| `search` | Find bar and search commands | F3 |
| `themes` | Theme model, light/dark switching, picker | F3 |
| `theme-iterm2` | `.itermcolors` importer | F3 |
| `keymap-macos` | Default macOS shortcuts | F3 |
| `keymap-linux` | Default Linux shortcuts | F7 |
| `command-palette` | Command palette and theme picker | F3 |
| `plugin-manager` | UI for Sleip (calls `sleip --json`) | F4 |
| `blocks` | Command + output blocks, over the prompts' marks that the kernel keeps since F3 | F5a |
| `quick-terminal` | The quick terminal's policy, which the app holds in F2 | F3 |
| `progress` | The programs' progress (OSC 9;4), in the `window-strip` slot; the tab shows it with `tabs` | F3 |
| `command-header` | The command whose output the view shows, at the pane's top, in the `pane-overlay` slot, over OSC 133's marks, as iTerm2's | F3 |
| `copy-mode` | The keyboard's selection over the screen and the scrollback, as tmux's and vim's | F3 |
| `tmux-cc`, `triggers`, `profiles`, `broadcast` | iTerm2 power features | F5b |
| `images` | Inline images | F5c |

**Rules** (details in [ARCHITECTURE.md](ARCHITECTURE.md#dependency-rules)):
- depend only on `plugin-sdk`;
- use `#![forbid(unsafe_code)]`, with clippy `disallowed-methods` on, so they follow the same rules as Luau plugins;
- pass the conformance suite.

## Reference third-party plugins

Plugins that we write as a third party would, in Luau, in a repository of their own made with `sleip new`, published to the registry and installed with `sleip add`: they go through the sandbox, the capabilities, the consent and the limits, and find where the API falls short before other authors do.

| Plugin | Role | Phase |
|---|---|---|
| `snap` | An image of a terminal's text, as VS Code's CodeSnap makes of code | F4 |

### `snap`

As the user asked on 31 October 2026, after VS Code's [CodeSnap](https://marketplace.visualstudio.com/items?itemName=adpyke.codesnap):

- **Commands**: `snap.selection`, the selection; `snap.screen`, the visible screen; and, once shell integration marks the commands, in F5a, `snap.output`, a command with its output. Each opens a preview in the `window-overlay` slot, the image as it will be, with a shutter and toggles of the frame's parts, which save the image or copy it.
- **The image**: the text with its colors, styles and the terminal's font, in a frame as a macOS window's, with its three buttons and a title, the pane's; rounded corners, a shadow, padding and a background around it. The host draws it: an `image` service renders cells offscreen, with the terminal's own renderer, so that the image is what the window shows, and returns a PNG, at the display's scale or another. Any plugin can use the service, without a capability.
- **Its config**, after CodeSnap's settings:

| Key | Values | Default | What it does |
|---|---|---|---|
| `background` | a color | the theme's background, darker | The color around the window |
| `transparent-background` | `true`, `false` | `false` | No background around the window |
| `shadow` | `true`, `false` | `true` | A shadow under the window |
| `padding` | points | `32` | The space around the window |
| `rounded-corners` | `true`, `false` | `true` | The window's corners rounded or square |
| `window-controls` | `true`, `false` | `true` | The window's three buttons |
| `title` | `true`, `false` | `true` | The pane's title in the window's bar |
| `target` | `container`, `window` | `container` | The image of the window with its background, or of the window alone |
| `action` | `save`, `copy`, `both` | `copy` | What the shutter does |
| `folder` | a path | `~/Pictures/Lokyr` | Where `save` writes the PNG |
| `scale` | `1`, `2`, `3` | the display's | The pixels of a point |

- **Capabilities**: `terminal-read`, for the cells; `clipboard-write`, for `copy`; `fs-write` of its folder, for `save`. Sleip shows them at install, and the app asks for each on first use.
- **What the API needs for it**: the cells of the selection, of the screen and of a command, with their styles, not only their text; the `image` service; an image on the clipboard, not only text; a file's write of bytes; and a config of the plugin's own, whose form is an [open question](ROADMAP.md#open-questions).

## Distribution

The "distribution" is the list of first-party plugins compiled into the app.

```toml
# dist/default.toml
[distribution]
name = "default"

[plugins]
core-actions    = { path = "../plugins/core-actions" }
links           = { path = "../plugins/links" }
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

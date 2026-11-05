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
| Events | Receive kernel events (batched) and emit its own | `tab-created`, `layout-changed`, `pointer`, `search.progress` | F3 |
| Workspace and layout | Create, close and move tabs and panes; edit the layout tree | split, zoom, move to a new window | F3 |
| UI | Provide view trees for slots | tab bar, find bar, status bar segments | F3 |
| Terminal | Read lines and the selection; write input (with a capability); use the search and decoration services | highlight matches, gutter marks | F3 |
| Themes | Register themes and importers | `.itermcolors`, base16 | F3 |
| Config | A `[plugins.<namespace>]` section of the config file, checked against the plugin's JSON Schema, of the keywords that the host checks, a wrong value refusing that section alone, and what another version would know ignored with a notice ([ADR 0032](adr/0032-plugins-config-and-state.md)), the notice at the value's line as the file's lines move; `enum` and `const` comparing numbers by their values, and a whole number of a fraction, as `5.0`, of a schema of integers alone the integer; read as a request, a change told as `config-changed` | `[plugins.tabs] position = "top"` | F3 |
| Storage | A file of the plugin's state, of plain values as JSON, 1 MiB at most, in the app's state directory, read as a request and written as the call returns ([ADR 0032](adr/0032-plugins-config-and-state.md)) | search history | F3 |
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
| `sidebar` | Many (F5+) | Side panels | |

The overlays and the popups are no slots: a plugin's floating panels, which it opens and closes, anchored to a cell, the cursor, the pointer, a node of its views, or a corner of a pane or of the window's room below the tab bar, as the find bar, the hints, the command's sticky header, the palette and the menus are.

A slot of no view takes no room, and a slot of many shares its width equally among its plugins' views, in their order. `ui-core`'s `chrome` gives the regions of a window's size, its titlebar's height and its buttons' room, the tab bar's place and the heights of the views, as the slots' trees take them laid out across the window; each slot's view is a surface of its own, drawn over the panes in the window's one drawable and taking the pointer before them.

The dividers between panes are no slot's: the host draws them from the layout tree, in the theme's tokens, and gives their drags to `splits`, which decides what they do.

## Execution model

Native and Luau plugins follow the same rules, so they behave the same way ([ADR 0028](adr/0028-plugin-calls-as-messages.md)).

- **Short calls, of messages**:
  - The kernel calls into the plugin (`activate`, `on_event`, `on_command`, `render`, `on_ui_event`), and the plugin sends the host requests, `plugin-sdk`'s `message::Call`, each with its identity and the capability that it needs, which the host answers with a `Response`. The host checks the capability against the plugin's: a built-in plugin has every one until its manifest says which; a call that comes as another plugin's fails.
  - Long-running work goes to asynchronous host services (search, timers, processes), whose results come back as events.
- **Reads at the host's time**: a request reads the main thread's state as the host handles it; an id that is gone gives `not-found`. A read of much text runs on a thread of the host's, which holds a pane's lock as briefly as a frame's copy.
- **Writes queued, for every plugin**: what a plugin changes, the kernel's state and the app's, the host applies in order as the plugin's call returns, on the main thread as on a worker and in Luau; plugins never change the state in place. A write to a pane that is not there fails as it is asked; one to a tab or a pane that an earlier write of the call opens, of the ids that its request gave, finds it, as the writes apply in their order, so that a call opens a tab and activates it. The kernel's writes, as a selection, a paste or the clipboard's text, come first, then the app's requests, as a window to open or a command to run; a read in the same call reads the state before the call's writes. A plugin's declarations, its commands and its keys, take effect at once, as the registry checks them.
- **Owned data**: plugins receive copies, never references into kernel state.
- **No re-entrancy**: events generated during a callback are queued and delivered only after the callback returns; those that a delivery's calls emit, in the next batch.
- **No threads of their own, no blocking I/O**: time is provided by the host, through timers and `tick` events that are sent only on request.
- **Filtered, batched events**: a plugin subscribes to the events that it names, `cx.subscribe`, the workspace's, as `tab-created`, and plugins' own, `namespace.event`; the kernel gathers the workspace's changes and the events that plugins emit, and delivers them to `on_event` in their order, each plugin those that it subscribed to, as a batch, once a turn of the main thread, which a frame, a key, a click or a wake is.
- **Rendering**:
  - `render(view)`, of a view of the plugin's, a slot of a window or a pane that its manifest declares, or one of its panels, is called as a window or a pane shows the slot, as a panel opens, as the plugin asks for an update (`request_render`) and as the theme changes. The result is cached, and stays as the plugin is disabled, so that the windows' layout does not jump: the view takes no event then, and a notice of the windows tells that the plugin stopped, its error in the log. A view of a window or a pane that closed goes with it.
  - Native plugins run on the main thread, unless their manifest asks for a thread of their own, `thread = "worker"`, as the user chose on 1 November 2026: each such plugin then has a thread of the host's, which makes all its calls, its renders too, with the same messages as a Luau plugin's, and the host takes their results on the main thread as they are ready, never waiting for them; a call of the main thread's plugins that takes long is logged as it returns, and stopped by nothing, as the plugins are ours. The main thread stays for the plugins that must answer at once, as the tab bar's. Luau plugins run on the plugin executor, with a deadline per call.
  - **A plugin on its thread**, one each, as the user confirmed on 5 November 2026, which waits on a channel at rest, without a wakeup: the thread starts as the plugin activates, and makes one call at a time; the host keeps each call's writes, which it applies as the call returns, as for a plugin of the main thread.
    - Each request crosses to the main thread, whose turn answers it with the state as it is then, and the plugin's thread waits for the answer: a request costs a turn of the main thread, its wake included. A turn takes the threads' messages for 1 ms at most, each plugin's in turn, those left for the next turn, so that frames and keys come between.
    - What the host asks of it meanwhile waits for the call under way: the events of the deliveries join one batch, and a render of a view whose render is under way or waits is not asked again; the plugin's own `request_render` within its render brings the next.
    - A call reads the focus of the time that it was asked, as it would have run then on the main thread, a context menu's command its window's. The events posted as the plugin activates wait for its activation's end, whose requests make its subscriptions, and come then; its commands, registered as its activation's requests are served, take their catalogs' titles and show in the menus then. The commands that its command runs are one deeper than it, whenever its call ends, so that a command that runs itself stops at the same depth as on the main thread.
    - A view shows its last tree until its render comes, and nothing before the first; a render that ends once its view went keeps nothing. A menu's question of whether its command applies is not asked of it, as the main thread does not wait: its commands apply.
    - A command's failure tells the user as its call ends, as one of the main thread's does; a panic disables the plugin, whose thread ends, and its call's writes go.
  - **A slow call** of a plugin on the main thread, of 8 ms or longer, a frame's time at 120 Hz, goes to the log by the plugin's name and its callback, as `on_command`.
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
            when: None,
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
    cx.register_command({ name = "search-plus.open", title = "Advanced search", category = "view" })
    cx.bind({ keys = "cmd+shift+f", command = "search-plus.open", when = "pane.focused" })
end

function Search.on_command(cx: Cx, name: string, args: Args)
    if name == "search-plus.open" then
        cx.open_panel("find", { anchor = { pane_corner = { pane = cx.focused_pane(), corner = "top-right" } }, role = "dialog", close_escape = true })
    end
end

function Search.on_event(cx: Cx, events: { Event })
    -- results, pane closed, …
end

function Search.render(cx: Cx, view: ViewId): View?
    -- the find bar, of the panel "find"
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
- **Conformance**: the conformance suite runs the same scenarios through both front ends, with Rust and Luau test plugins, and compares their command and event traces. It is `lokyr-kernel`'s test `conformance`: each scenario is of steps, each a call of the test plugin's, of the requests that it makes with the answers that they take, the workspace's events that the host posts after it, and the batch of events that the plugin then hears; the test plugin makes the requests through its front end, and its trace, each request with its answer, the ids of reads and decorations of 0, and each batch, must be the scenario's. The scenarios hold every request of the API, its failures among them, which a test checks of an exhaustive match: those of the first-party plugins, `links`' and `core-actions'`, and of the events, the panels, the capabilities, the declarations, the app's requests, the state, the reads of rows, the views and the workspace's writes that every plugin meets; the Rust test plugin runs them on the kernel's host and on the SDK's fake host, which must answer alike, in CI's macOS and Linux jobs. A window, a tab or a pane that a scenario opens, whose id each host gives its own, is a symbol of the order in which the trace first meets it, which the test plugin asks with the host's id of. The harness of the kernel's host does the app's part: it posts the workspace's changes, makes the renders that the calls ask for, and gives each scenario a state directory of its own. They cross as JSON, for the Luau test plugin of F4.
- **Testing for plugin authors**:
  - The Rust SDK ships a fake host for unit tests, `lokyr_plugin_sdk::fake::FakeHost`, of its `fake` feature, a plugin's dev-dependency. The test sets what it holds, its panes, their lines, rows, links, styles, selections and directories, the focus, the clipboard, the text's size, the targets not there, and the plugin's config and state; it answers the requests as the app's host does, the reads at once and the writes as the call returns, then the app's requests, refuses what the plugin's capabilities lack, and keeps the plugin's commands, bindings and subscriptions, what it did as the requests themselves, its decorations, its panels and its views' trees. It checks what the app's host checks, of the same code where the SDK holds it, the grammars of a binding's keys and of a context, the names, a command's registration and a call's arguments: a request of a pane, a tab, a window or a view not there, of a value out of its range, of a name, keys or a context that is none, of a command that is not registered or that the manifest does not declare, fails as it would in the app; `with_capabilities`, `with_uses`, `with_slots` and `with_commands` give what the plugin's manifest declares and the commands of the app's and of the other plugins'; `take_renders` tells the renders that the calls asked for, a panel's opening among them; a config's change reaches an active plugin alone, its schema's defaults the test's to give. Its workspace is the app's, of the panes that the test gives: each window's tabs in their order and the one that it shows, each tab's layout, the pane of its keyboard and its zoom; the plugin's writes change it as the app's workspace changes, owing their events in the app's order, and a later write of a call finds what an earlier one opens; a drag of a pane's edge changes nothing of it, as it has no sizes. It owes the plugin its events, which `deliver` gives it as a batch, and `snapshot` writes a view tree as text, a node a line, for tests to compare. `core-actions`' and `links`' tests run on it.
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
- **Plugin-defined events**: `custom` events carry a name, of the plugin's namespace, as `search.progress`, a schema id, as `search.progress/1`, and a plain value, the SDK's `message::Data`: text, numbers, true or false, and lists and maps of them, as a Luau table is. `cx.emit` emits one, which comes after the call returns. A value, an event's or a state's, nests `Data::DEEPEST` levels at most, 64, as the host walks the values that it takes recursively on the main thread: a deeper one is refused, `failed`, checked without recursion and dropped a level at a time, a worker's on its own thread; the Luau front end's conversion of tables keeps the same limit.

### Versioning

- The API version is the version of `plugin-sdk` (`plugin-sdk 0.1.0` ↔ API 0.1).
- New APIs start as unstable: in Rust behind the `unstable` feature, in Luau under `cx.unstable`. They become stable in a minor release.
- The manifest declares the API version it requires (`api = "^0.1"`). The host supports the current major version and, through adapters, the previous one.
- Before 1.0, minor versions may break compatibility. After 1.0, semver is followed strictly.

### Illustrative draft

> The Luau front end of the API as `plugin-sdk` 0.1 has it: each of `Cx`'s requests by the Rust front end's name, of its arguments and its answer, and the Rust types' shapes as tables. F4 generates the definitions file from the Rust types, which settles the last details; the requests of the services still to come, as the search's, the tabs' renames and the themes', come with their tasks.

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

-- How a split lays its two parts out, the first left or above; a part of
-- a split; an edge of a pane, which a resize moves.
type Direction = "horizontal" | "vertical"
type Side = "first" | "second"
type Edge = "left" | "right" | "top" | "bottom"

-- An error's code, whose hundreds tell its category: 1xx usage, 2xx
-- failure, 3xx security, 4xx fault (ADR 0019).
type Error = {
    code: number,
    kind: "not-found" | "invalid-args" | "invalid" | "failed" | "denied" | "fault",
    message: string,
}

type WindowInfo = {
    id: WindowId,
    tabs: { TabId }, -- in their order
    active: TabId, -- the tab that it shows
    number: number?, -- from 1, the lowest that no other open window had as it opened; none for the quick terminal's
    keys: string?, -- the keys that bring it forward, as the platform writes them, as "⌥⌘1", for its titlebar to show; nil while the config hides them
}

type BarPlace =
    | { kind: "titlebar" }
    | { kind: "below-titlebar" }
    | { kind: "bottom" }
    | { kind: "left", width: number }
    | { kind: "right", width: number }

type TabInfo = {
    id: TabId,
    window: WindowId,
    title: string, -- its own, as `set_tab_title` gave it, or its active pane's program's, or, of `process-read`, its pane's folder and program, as "src — vim"
    color: Color?, -- its own, as `set_tab_color` gave it, or its active pane's program's, an RGB one
    active: boolean, -- whether its window shows it
    panes: { PaneId }, -- in its layout's order
    focused: PaneId, -- the pane that has its keyboard
    zoomed: PaneId?,
}

-- A tab's layout: a pane, or a split of two layouts, whose ratio is the
-- first part's share, from 0 to 1, or none to share it by their panes.
type Layout =
    { pane: PaneId }
    | { direction: Direction, ratio: number?, first: Layout, second: Layout }

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
    transition: { duration: number?, easing: ("linear" | "ease-in" | "ease-out" | "ease-in-out")? }?, -- its moves and fades, over seconds, and its hover's
    -- What a node of a key takes while the pointer is over it, what a node takes while the pointer is over its
    -- nearest node of a key around it, as a tab's close button under its tab, and what a node of a key takes while
    -- the primary button that went down over it is down, the pointer still over it: the host applies them as the
    -- pointer moves, with no render, their colors over the node's own, each over the ones before, fading them as
    -- the transition says, at once where the system reduces motion.
    hover: Hover?,
    parent_hover: Hover?,
    pressed: Hover?,
}

type Hover = { background: Color?, color: Color?, border: { color: Color, width: number }?, opacity: number? }

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
    distribution: ("fit" | "fill" | "fill-equally" | "fill-proportionally")?, -- for tabs nodes, as macOS's segmented controls: each tab of its content's width, the control in the middle, by default; or the control of the view's width, each tab of its content's and an equal share of the room left, of equal widths, or of its content's share
    style: Style?,
    focusable: boolean?, -- takes the keyboard's focus, by its key
    tooltip: string?, -- shown as the pointer rests over the node, by its key
    children: { Node }?,
    -- What accessibility, as VoiceOver, takes the node for, in place of its kind's: a container a group, a
    -- toolbar, a status or an alert, a text a heading, an icon an image; a widget keeps its own.
    role: ("group" | "toolbar" | "status" | "alert" | "heading" | "image")?,
    -- What accessibility calls it, in place of its text, a button's or a toggle's label, or its tooltip: what a
    -- button of an icon alone gives, without a tooltip; an icon of none is left out, as a decoration.
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
    -- A tab's title or color changed, its own, its program's, or its active pane's folder and program as output comes.
    | { kind: "tab-changed", tab: TabId }
    -- The keymap's bindings changed, as the config's or a plugin's: the keys of `keys_of` may be others.
    | { kind: "keys-changed" }
    -- The pointer over another cell of a pane, or out of it, or the modifiers changed over it, or a frame changed the text
    -- of its row or scrolled its pane's rows under it, at the same cell; while Command is up, its
    -- moves come only as the system gives them, as a program that tracks the mouse asks.
    | { kind: "pointer", pane: PaneId, at: { number }?, mods: { shift: boolean, control: boolean, option: boolean, command: boolean } }
    -- A read of rows that the plugin asked for, to it alone; incomplete if a resize renumbered the rows.
    | { kind: "lines", id: number, pane: PaneId, lines: { Line }, complete: boolean }
    -- A panel of the plugin's closed, as a click outside it, Escape, the app or the plugin closed it, to it alone.
    | { kind: "panel-closed", id: string }
    -- The user's answer to a question of `ask_text`, to it alone: the text, or none as the user cancelled.
    | { kind: "answered", id: number, text: string? }
    -- The user's drag of the edge of the plugin's tab bar's column ended, to it alone: its width since, in points,
    -- which the plugin keeps, as for its next start.
    | { kind: "tab-bar-resized", width: number }
    -- The plugin's section of the config changed, to it alone, which `config` reads.
    | { kind: "config-changed" }
    -- An event that a plugin emitted, of its namespace; a value nests 64 levels at most.
    | { kind: "custom", name: string, schema: string, value: any }

-- A view of a plugin's: a window's slot, a pane's header, or one of its panels, by the id that it gives it.
type ViewId =
    { tab_bar: WindowId } | { window_strip: WindowId } | { status_bar: WindowId }
    | { pane_header: PaneId } | { panel: string }

-- Where a panel goes, and the window that it shows in.
type Anchor =
    { cell: { pane: PaneId, column: number, row: number } } -- below it
    | { cursor: PaneId } -- below it
    | { pointer: WindowId } -- where it is as the panel opens
    | { node: { view: ViewId, key: string } } -- below a node of a view of the plugin's
    | { pane_corner: { pane: PaneId, corner: Corner } }
    | { window_corner: { window: WindowId, corner: Corner } } -- of its room below the tab bar
type Corner = "top-left" | "top-right" | "bottom-left" | "bottom-right"

type PanelOptions = {
    anchor: Anchor,
    role: ("tooltip" | "menu" | "dialog")?, -- a dialog by default; a menu of menu items alone shows as the platform's own
    close_outside: boolean?, -- a click outside it closes it
    close_escape: boolean?, -- Escape closes it, while it has the keyboard's focus
}

type UiEvent = {
    view: ViewId,
    key: string,
    kind: "click" | "double-click" | "middle-click" | "hover-enter" | "hover-leave" | "context-menu"
        | "input-changed" | "submit" | "focus" | "blur" | "scroll"
        | "drag-start" | "drag-move" | "drag-end" | "drag-cancel" | "set-value" | "toggle"
        | "select" | "activate" | "list-range",
    text: string?, -- a field's text, as the host holds it: at each change, "input-changed", and as Return goes to it, "submit"
    revision: number?, -- an "input-changed" text's, one more at each edit, which `set_text` gives back
    item: number?, -- a list's item, selected or activated
    first: number?, count: number?, -- the items of a list that the host asks for, which the next render gives
    x: number?, y: number?, -- a drag's, from its start, or a scroll's, in points
    value: string?, -- set by accessibility, as VoiceOver's; its press comes as a click, its focus as a focus
    on: boolean?, -- a toggle's state asked, the other than its render's, which the next render gives
}

-- A command, as its registration describes it.
type Command = {
    name: string, -- `namespace.action`
    title: string,
    category: "app" | "windows" | "tabs" | "panes" | "input" | "clipboard" | "edit" | "view",
    capabilities: { string }?, -- those that its action needs, as "pty-write"
    params: { { name: string, kind: "string" | "integer" | "number" | "boolean", default: (string | number | boolean)? } }?,
    when: string?, -- the context in which it can run, as a binding's `when`
}

-- `keys`: e.g. "cmd+f" or "ctrl+a>c"; `when`: e.g. "pane.focused && !search.open", where a plugin's
-- panels give `namespace.open` while one shows, and `namespace.focused` while one has the keyboard.
type Binding = { keys: string, command: string, args: Args?, when: string? }

-- The logical line of a row: its text, and each character's cell, { column, row's number }.
type LineText = { text: string, cells: { { number } } }

type Scroll = { up: number } | { down: number } | "page-up" | "page-down" | "top" | "bottom"

-- The host's requests, each by the Rust front end's name. A read answers at once, of the state as it is; a
-- write, and a request of the app's, as a window to open or a command to run, applies as the call returns,
-- the writes first; a read of the call reads the state before them. A write to a tab or a pane that an
-- earlier write of the call opens finds it.
type Cx = {
    -- Declarations, at once.
    register_command: (command: Command) -> (boolean, Error?),
    bind: (binding: Binding) -> (boolean, Error?),
    -- The workspace's events, as "tab-created", the pointer's, "pointer", and plugins' own, as "search.progress".
    subscribe: (names: { string }) -> (boolean, Error?),
    -- An event of the plugin's namespace, which comes after the call returns.
    emit: (name: string, schema: string, value: any) -> (boolean, Error?),
    run_command: (name: string, args: Args?) -> (boolean, Error?),

    -- The focus: the window that has it, and its pane that has the keyboard.
    focused_window: () -> WindowId?,
    focused_pane: () -> PaneId?,

    -- The workspace. A new tab's and a split's ids come at once; what a move made, as a new window, comes as
    -- the workspace's events.
    windows: () -> ({ WindowInfo }?, Error?),
    tabs: (window: WindowId) -> ({ TabInfo }?, Error?),
    layout: (tab: TabId) -> (Layout?, Error?),
    -- A tab of a new pane, whose program starts as a new pane's beside the window's focused one, at `index` of
    -- its tabs or at their end: the tab's id and the pane's. The window shows the tab that it showed.
    new_tab: (window: WindowId, index: number?) -> (TabId?, PaneId?, Error?),
    -- A new pane on `side` of `pane`'s part, whose program starts as a new pane's beside it: its id.
    split: (pane: PaneId, direction: Direction, side: Side) -> (PaneId?, Error?),
    -- Closes a tab, or a pane; with `ask`, once the user confirms it while a program other than a shell runs there,
    -- as the host asks for a window.
    close_tab: (tab: TabId, ask: boolean?) -> (boolean, Error?),
    close_pane: (pane: PaneId, ask: boolean?) -> (boolean, Error?),
    -- A title or a color of a tab's own, in place of its program's; none gives the program's back.
    set_tab_title: (tab: TabId, title: string?) -> (boolean, Error?),
    set_tab_color: (tab: TabId, color: Color?) -> (boolean, Error?),
    activate_tab: (tab: TabId) -> (boolean, Error?),
    focus_pane: (pane: PaneId) -> (boolean, Error?),
    move_tab: (tab: TabId, window: WindowId, index: number) -> (boolean, Error?),
    tab_to_new_window: (tab: TabId) -> (boolean, Error?),
    pane_to_new_tab: (pane: PaneId, window: WindowId, index: number) -> (boolean, Error?),
    pane_to_new_window: (pane: PaneId) -> (boolean, Error?),
    -- Moves an edge by `by`, a share from -1 to 1 of the room of the split whose divider it is.
    resize_pane: (pane: PaneId, edge: Edge, by: number) -> (boolean, Error?),
    -- The split at `path`, the parts from the layout's top, takes the first part's share `ratio`, or its panes'.
    set_ratio: (tab: TabId, path: { Side }, ratio: number?) -> (boolean, Error?),
    equalize: (tab: TabId) -> (boolean, Error?),
    zoom: (tab: TabId, pane: PaneId?) -> (boolean, Error?),
    open_window: () -> (boolean, Error?),
    close_window: (window: WindowId) -> (boolean, Error?),
    quit: () -> (boolean, Error?),

    -- The views and the panels.
    request_render: (view: ViewId) -> (boolean, Error?),
    -- Sets the text of a field of the view, by its key, cut to the field's 10 000 characters: its caret at its end, and no
    -- "input-changed"; of the revision of the "input-changed" that it answers, so that it is not set over the keys typed since.
    set_text: (view: ViewId, key: string, text: string, revision: number) -> (boolean, Error?),
    -- Moves the selection of a list of the view, by its key, to an item, which the list scrolls to.
    select: (view: ViewId, key: string, item: number) -> (boolean, Error?),
    -- Opens a panel, by an id of the plugin's, which the host then renders; a "panel-closed" event tells as it closes.
    open_panel: (id: string, options: PanelOptions) -> (boolean, Error?),
    close_panel: (id: string) -> (boolean, Error?),
    -- Puts the plugin's tab bar, in the windows that show it, in the titlebar, below a titlebar of the window's title,
    -- at the bottom, or down a column on a side of 60 to 600 points; `tab_bar_place` reads it, a column's width as the
    -- user's drag of its edge left it, as the bar's view lays its tabs out across or down.
    set_tab_bar_place: (place: BarPlace) -> (boolean, Error?),
    tab_bar_place: () -> (BarPlace?, Error?),
    -- The keys that run a command of its arguments, one key that no later binding takes, as the platform writes them,
    -- as "⌘1"; none without a binding.
    keys_of: (command: string, args: { [string]: any }) -> (string?, Error?),
    -- Asks the user for a text, in an alert on `window`, or on the focused one, of a field that holds `text`:
    -- returns the question's id, and the answer comes as an "answered" event. The message and the detail are
    -- messages of the plugin's catalogs, shown in the language shown, or the texts themselves where they lack them.
    ask_text: (window: WindowId?, message: string, detail: string, text: string) -> (number?, Error?),

    -- The terminal. The reads require `terminal-read`, and return a `denied` error without it.
    selection_text: (pane: PaneId) -> (string?, Error?),
    -- The logical line of a row, by its number, its wrapped rows joined as far as `reach` rows before it and after it, 256 at most, with each character's cell.
    line_text: (pane: PaneId, row: number, reach: number) -> (LineText?, Error?),
    -- The target of the OSC 8 link at a cell, by its row's number.
    link_at: (pane: PaneId, column: number, row: number) -> (string?, Error?),
    -- Reads 100 000 rows at most, from the row numbered `first`, on a thread of the host's, a part at a time:
    -- returns the read's id, and the rows come as a "lines" event.
    read_lines: (pane: PaneId, first: number, count: number) -> (number?, Error?),
    -- The styles of a row, by its number, as runs of cells of one style.
    styles: (pane: PaneId, row: number) -> ({ StyleRun }?, Error?),
    -- Decorates the cells from one to another, in reading order, each { column, row's number }: returns the
    -- decoration's id. It keeps its text as output scrolls it and a resize reflows it, and goes with it.
    decorate: (pane: PaneId, from: { number }, to: { number }, look: DecorationLook) -> (number?, Error?),
    undecorate: (pane: PaneId, id: number) -> (boolean, Error?),
    select_all: (pane: PaneId) -> (boolean, Error?),
    clear_scrollback: (pane: PaneId) -> (boolean, Error?),
    clear: (pane: PaneId) -> (boolean, Error?),
    -- Require `pty-write`: a paste, as the user's, which a text of line breaks may ask the user to confirm,
    -- and text sent as typed.
    paste: (pane: PaneId, text: string) -> (boolean, Error?),
    send_text: (pane: PaneId, text: string) -> (boolean, Error?),
    scroll: (pane: PaneId, scroll: Scroll) -> (boolean, Error?),
    -- The text's size, in points, of every window, from 4 to 200, or the config's with none.
    font_size: () -> (number?, Error?),
    set_font_size: (size: number?) -> (boolean, Error?),

    -- Require `process-read`: the directory of the program in the pane's foreground.
    working_directory: (pane: PaneId) -> (string?, Error?),
    -- Requires `open`: a URL of `http`, `https`, `mailto` or `file`, or a path from `/` or `~`.
    open: (target: string) -> (boolean, Error?),
    -- Whether `open` would open `target`: a URL of its schemes, or a path that is there.
    opens: (target: string) -> (boolean, Error?),
    -- Require `clipboard-read` and `clipboard-write`.
    clipboard_text: () -> (string?, Error?),
    set_clipboard_text: (text: string) -> (boolean, Error?),

    -- The plugin's config, its section's values with its schema's defaults; its state, of plain values,
    -- 1 MiB at most as JSON, kept for the next run, as the call returns.
    config: () -> (any, Error?),
    state: () -> (any, Error?),
    set_state: (value: any) -> (boolean, Error?),
}

-- What a plugin's entry module returns.
type Plugin = {
    activate: ((cx: Cx) -> ())?,
    on_event: ((cx: Cx, events: { Event }) -> ())?,
    on_command: ((cx: Cx, name: string, args: Args) -> ())?,
    -- Whether a command would do something with `args`, as a menu asks before it dims its item; what
    -- the call asks of the app is dropped. Without it, every command would.
    applies: ((cx: Cx, name: string, args: Args) -> boolean)?,
    render: ((cx: Cx, view: ViewId) -> View?)?,
    on_ui_event: ((cx: Cx, event: UiEvent) -> ())?,
}
```

## Commands

- **A command** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)) has a name, `namespace.action`, each part of lowercase letters, digits and `-`, from a letter, as `window.new`, `tabs.goto` or `search-plus.open`; a title, for the menus and the palette; a category, which also tells the menu that lists it; the capabilities that its action needs; its parameters, a flat schema: each named as the parts are, of a string, an integer, a number or a boolean, and required, or optional with a default of its kind; and, if it needs one, the context in which it can run, written as a binding's `when`, as Copy's `selection`, outside of which its menu's item is dimmed. Its plugin's `applies` can dim the item too, where the command would do nothing, as Open Link where no link is. Its title, in the language that Lokyr shows, comes from its plugin's own catalogs ([ADR 0020](adr/0020-localization.md)), by the message `command-` and the command's name with `-` for its `.`, as `command-pane-copy`, or English's; a title that they lack is the one registered. A native plugin gives its catalogs through `Plugin::catalogs`; a Luau plugin carries them in its package.
- **The pane's context menu**, of a right click, lists the commands of a cell, which take the `column` and the `row` of the cell clicked, then those of the clipboard and of the editing that need no arguments; its commands run in the pane that it opened over ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).
- **The kernel's registry**, `kernel::commands::Registry`, registers the commands of the app and of the plugins, and checks a call before its action runs: its name, and its arguments, to which it adds the defaults of those left out; an integer is a number too. The checks of a command and of a call are the SDK's, `Command::check` and `Command::check_call`, which the registry and the fake host share. A plugin's call is checked as well against the commands that its manifest declares and the capabilities that it has, its own commands aside. The action runs where it can: in the app for those that the platform does, in the plugin that registered the command otherwise.
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

- **Commands** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)): each has a category, `app`, `windows`, `tabs`, `panes`, `input`, `clipboard`, `edit` or `view` to start with, and the capabilities that its action needs, as `pane.paste` needs `clipboard-read` and `pty-write`. A plugin declares in its manifest the commands that it runs, `[uses] commands`, by name or by namespace, as `tabs.*`: one that it did not declare fails with `denied`, 301, and one whose capabilities it lacks with `denied`, 302. The commands that the user runs, through the keys, the menus or a plugin with `user-commands`, as the palette, and the plugin's own need no declaration. Without `user-commands`, a plugin reads the commands' names, titles, categories and parameters, not their plugins.
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
| `keymap-macos` | Default macOS shortcuts, as iTerm2's: the keys of the app's commands and of the other plugins', which bind none of their own | F3 |
| `keymap-linux` | Default Linux shortcuts | F7 |
| `command-palette` | Command palette and theme picker | F3 |
| `plugin-manager` | UI for Sleip (calls `sleip --json`) | F4 |
| `blocks` | Command + output blocks, over the prompts' marks that the kernel keeps since F3 | F5a |
| `quick-terminal` | The quick terminal's policy, which the app holds in F2 | F3 |
| `progress` | The programs' progress (OSC 9;4), in the `window-strip` slot; the tab shows it with `tabs` | F3 |
| `command-header` | The command whose output the view shows, at the pane's top, in a panel of its pane's top corner, over OSC 133's marks, as iTerm2's | F3 |
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

- **Commands**: `snap.selection`, the selection; `snap.screen`, the visible screen; and, once shell integration marks the commands, in F5a, `snap.output`, a command with its output. Each opens a preview in a panel over the window, the image as it will be, with a shutter and toggles of the frame's parts, which save the image or copy it.
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
- **What the API needs for it**: the cells of the selection, of the screen and of a command, with their styles, not only their text; the `image` service; an image on the clipboard, not only text; a file's write of bytes; and a config of the plugin's own, its section of the config file ([ADR 0032](adr/0032-plugins-config-and-state.md)).

## Distribution

The "distribution" is the list of first-party plugins compiled into the app.

```toml
# dist/default.toml
[distribution]
name = "default"

[plugins]
keymap-macos    = { path = "../plugins/keymap-macos", target = "macos" }
keymap-linux    = { path = "../plugins/keymap-linux", target = "linux" }    # from F7
core-actions    = { path = "../plugins/core-actions" }
links           = { path = "../plugins/links" }
tabs            = { path = "../plugins/tabs" }
splits          = { path = "../plugins/splits" }
search          = { path = "../plugins/search" }
themes          = { path = "../plugins/themes" }
theme-iterm2    = { path = "../plugins/theme-iterm2" }
command-palette = { path = "../plugins/command-palette" }
plugin-manager  = { path = "../plugins/plugin-manager" }   # from F4
```

- **Generation**: `cargo xtask gen-dist` reads every `dist/*.toml`, and writes, between its markers, each distribution as a feature of `lokyr-app-core` and of `lokyr-app`, `dist-<name>`, of a feature of each of its plugins, `plugin-<name>`, which takes the plugin's crate, a dependency of `lokyr-app-core` that is optional; and `crates/app-core/src/dist.rs`, the build's plugins' registrations, `export_plugin!`'s, in the files' order, `default` first. The default distribution is the crates' default; `just check-dist` checks that what it would write is written, and `just build-minimal` builds the minimal one, as CI does.
- **Minimal distribution**: `dist/minimal.toml` contains only `keymap-macos` and `core-actions`, Alacritty-style. It proves the terminal works without any of the "visible" features: `cargo xtask bundle --no-default-features --features dist-minimal` builds its app.
- **The keys**: the system's keymap plugin, `keymap-macos` on the Mac, binds the default keys of every command, the app's and the other first-party plugins', which bind none of their own; it comes first, so that a plugin's binding after it wins, and the user's `[keys]` after all of them. A binding of a command that no plugin registers, as one of a plugin turned off, takes no key; with the keymap off, the menus keep the system's own keys alone, ⌘H, ⌥⌘H, ⌘M and ⌃⌘Space.
- **At runtime**:
  - `[plugins.<namespace>] enabled = false` turns off a first-party plugin, at once as the config is saved, and `true` turns it on again, a new one of it ([ADR 0033](adr/0033-plugins-turned-off-in-their-sections.md)): its commands, its keys, its views, its panels and its decorations go as it stops.
  - For single-provider slots (`tab-bar`, `pane-header`), the config's [`[slots]`](CONFIGURATION.md#slots) picks the provider, the first that fills it by default, so a third-party plugin can replace our tab bar.

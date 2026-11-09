# Plugin system

Everything you see in Lokyr is a plugin: the tab bar, splits, the find bar, the palette; the themes are the host's ([ADR 0035](adr/0035-themes-are-the-hosts.md)). The kernel provides data and services, and plugins decide how things behave and look ([ADR 0002](adr/0002-micro-kernel-and-plugins.md), [ADR 0003](adr/0003-native-and-wasm-plugins.md), [ADR 0012](adr/0012-luau-plugins.md)).

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
| Terminal | Read lines and the selection; read the commands that the shell marked and hear their starts and ends (with a capability); follow a pane's view; write input (with a capability); use the search and decoration services | highlight matches, gutter marks, a command's header | F3 |
| Config | A `[plugins.<namespace>]` section of the config file, checked against the plugin's JSON Schema, of the keywords that the host checks, a wrong value refusing that section alone, and what another version would know ignored with a notice ([ADR 0032](adr/0032-plugins-config-and-state.md)), the notice at the value's line as the file's lines move; `enum` and `const` comparing numbers by their values, and a whole number of a fraction, as `5.0`, of a schema of integers alone the integer; read as a request, a change told as `config-changed` | `[plugins.tabs] position = "top"` | F3 |
| Storage | A file of the plugin's state, of plain values as JSON, 1 MiB at most, in the app's state directory, read as a request and written as the call returns ([ADR 0032](adr/0032-plugins-config-and-state.md)) | search history | F3 |
| System | Clipboard, notifications, opening URLs and files, global hotkeys and quick windows in F3; processes and the network, through the app to Sleip's helper, after 1.0.0, in F6, as [ADR 0038](adr/0038-plugin-ecosystem-scope.md) decided (with capabilities) | notify when a command finishes | F3; processes and the network F6 |
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

The dividers between panes are no slot's: the host draws them from the layout tree, in the theme's `pane.divider`, and takes their drags itself, a drag giving its split a ratio and a double click half and half, which the plugins learn of by `layout-changed`.

## Execution model

Native and Luau plugins follow the same rules, so they behave the same way ([ADR 0028](adr/0028-plugin-calls-as-messages.md)).

- **Short calls, of messages**:
  - The kernel calls into the plugin (`activate`, `on_event`, `on_command`, `render`, `on_ui_event`), and the plugin sends the host requests, `plugin-sdk`'s `message::Call`, each with its identity and the capability that it needs, which the host answers with a `Response`. The host checks the capability against the plugin's: a built-in plugin has every one until its manifest says which; a call that comes as another plugin's fails.
  - Long-running work goes to asynchronous host services (search, timers, processes), whose results come back as events.
- **Reads at the host's time**: a request reads the main thread's state as the host handles it; an id that is gone gives `not-found`. A read of much text runs on a thread of the host's, which holds a pane's lock as briefly as a frame's copy.
- **Writes queued, for every plugin**: what a plugin changes, the kernel's state and the app's, the host applies in order as the plugin's call returns, on the main thread as on a worker and in Luau; plugins never change the state in place. A write to a pane that is not there fails as it is asked; one to a tab or a pane that an earlier write of the call opens, of the ids that its request gave, finds it, as the writes apply in their order, so that a call opens a tab and activates it. The kernel's writes, as a selection, a paste or the clipboard's text, come first, then the app's requests, as a window to open or a command to run; a read in the same call reads the state before the call's writes. A call that returns an error, or panics, leaves things as they were: its writes, its requests of the app and its reads of rows go, the timers that it started too, those that it cancelled coming back, and the ids that it was given name nothing. A plugin's declarations, its commands, its keys, its subscriptions and its timers, take effect at once, as the host checks them; a call that only looks, as `applies`, declares nothing.
- **Owned data**: plugins receive copies, never references into kernel state.
- **No re-entrancy**: events generated during a callback are queued and delivered only after the callback returns; those that a delivery's calls emit, in the next batch.
- **No threads of their own, no blocking I/O**: time is the host's, of the timers that a plugin starts, `cx.start_timer`, of a delay and whether it repeats, and cancels, `cx.cancel_timer`, whose `timer` events come to it alone, which no subscription needs, once however many times a timer fired since the plugin last heard of it; a plugin of a thread of its own hears of them once its thread is free, so that a timer that its call under way cancels tells it nothing. A timer that repeats and falls behind, as the app was busy, waits its delay again from the time that it fires; its plugin's timers end as it stops. The app wakes for the next of them alone, none while no timer waits.
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
  - A render and a look of `applies` return no error: a plugin whose code can fail there, as a Luau plugin's, tells it after the call through `take_failure`, which the host and the fake host take as a call's error, a fault faulting the plugin.
  - What changes the kernel or the app, as a tab's title, opening or closing a window, quitting, the font's size or running another command, comes once the plugin's call returns, in its order; a chain of commands that run commands, from a command that the user ran or from a call of no command's, all of whose runs it is however many the call asks, goes 16 deep at most and runs 256 commands at most past its first, as a command that runs itself twice would run 2^16 times within the depth; a chain whose commands run keeps its count, however many chains come after it.
  - The plugins' bindings come after the app's and before the user's `[keys]`, which win over them. A key runs its call as the user's, so that a plugin of a manifest that is not a first-party one binds a call that it may make alone, as its menu items: a command that it registered or that its `[uses]` declares, of the capabilities that the command needs, of arguments that make a whole call; a global key too.
- **Luau front end**:
  - `plugin-host-luau` binds the same messages to Luau.
  - The bindings and the Luau type definitions are generated from the Rust types where possible.
  - The semantics are implemented once, in the kernel, behind both front ends.
- **Names**: the Luau API uses the Rust names (`request_render`, `on_event`), so the two front ends read the same and share one reference.
- **Conformance**: the conformance suite runs the same scenarios through both front ends, with Rust and Luau test plugins, and compares their command and event traces. It is `lokyr-kernel`'s test `conformance`: each scenario is of steps, each a call of the test plugin's, of the requests that it makes with the answers that they take, the workspace's events that the host posts after it, the time that the host's clock moves on, which fires the plugin's timers, and the batch of events that the plugin then hears; the test plugin makes the requests through its front end, and its trace, each request with its answer, the ids of reads and decorations of 0, and each batch, must be the scenario's. The scenarios hold every request of the API, its failures among them, which a test checks of an exhaustive match: those of the first-party plugins, `links`' and `core-actions'`, and of the events, the panels, the capabilities, the declarations, the app's requests, the state, the reads of rows, the views and the workspace's writes that every plugin meets; the Rust test plugin runs them on the kernel's host and on the SDK's fake host, which must answer alike, in CI's macOS and Linux jobs. A window, a tab, a pane or a timer that a scenario opens, whose id each host gives its own, is a symbol of the order in which the trace first meets it, which the test plugin asks with the host's id of. The harness of the kernel's host does the app's part: it posts the workspace's changes, makes the renders that the calls ask for, and gives each scenario a state directory of its own. The Luau test plugin takes the scenarios as JSON: its code makes each request through its function of `cx`, of the symbols, which a host between it and the host of the call gives the host's ids of, and returns what each answered and each batch of events as it took them; its trace is of the requests as they reached the host from Luau, and of the answers and the events that Luau took, back as the SDK's types by the front end's own conversions, and must be the scenario's on both hosts, as the Rust test plugin's.
- **Testing for plugin authors**:
  - The Rust SDK ships a fake host for unit tests, `lokyr_plugin_sdk::fake::FakeHost`, of its `fake` feature, a plugin's dev-dependency. The test sets what it holds, its panes, their lines, rows, links, styles, selections and directories, the commands that their shells marked and their views, the focus, the clipboard, the text's size, the targets not there, and the plugin's config and state; it answers the requests as the app's host does, the reads at once and the writes as the call returns, then the app's requests, refuses what the plugin's capabilities lack, and keeps the plugin's commands, bindings and subscriptions, what it did as the requests themselves, its decorations, its panels and its views' trees. It checks what the app's host checks, of the same code where the SDK holds it, the grammars of a binding's keys and of a context, the names, a command's registration and a call's arguments: a request of a pane, a tab, a window or a view not there, of a value out of its range, of a name, keys or a context that is none, of a command that is not registered or that the manifest does not declare, fails as it would in the app; `with_capabilities`, `with_uses`, `with_slots` and `with_commands` give what the plugin's manifest declares and the commands of the app's and of the other plugins'; `take_renders` tells the renders that the calls asked for, a panel's opening among them; a config's change reaches an active plugin alone, its schema's defaults the test's to give. Its workspace is the app's, of the panes that the test gives: each window's tabs in their order and the one that it shows, each tab's layout, the pane of its keyboard and its zoom; the plugin's writes change it as the app's workspace changes, owing their events in the app's order, and a later write of a call finds what an earlier one opens; a drag of a pane's edge changes nothing of it, as it has no sizes. It owes the plugin its events, which `deliver` gives it as a batch, and `snapshot` writes a view tree as text, a node a line, for tests to compare. It searches literal text alone, in each row apart, its matches in one event, where the app's host searches regexes and logical lines too. `core-actions`' and `links`' tests run on it.
  - For Luau, `sleip test` runs the plugin in the same headless host.

## The Luau API

- **Shape**:
  - A plugin is a package of Luau modules. Its entry point, `src/init.luau`, returns the table of callbacks shown above. Every callback is optional.
  - `require` resolves only the modules inside the plugin's own package: each module's environment holds the package's own, and the globals none, mlua's `require` of the file system taken from them.
- **Types**: a definitions file, [`lokyr.d.luau`](../crates/plugin-host-luau/luau/lokyr.d.luau), declares the API's types for Luau's type checker: `Cx`, of a function of each request, its parameters and its answer, `(T?, string?, number?)`, as each request tells the response that answers it (`Request::answer`); `Event` and `UiEvent`, of their flat shapes; `Node`; `Plugin`, the table of callbacks; and every type that they use, by its Rust name, but where two share one, as `NodeLayout` and `NodeRole`. `cargo xtask luau-types` makes it of the SDK's own types, as serde tells them, and a test of the Luau host fails as it is not the SDK's. Sleip's templates include it, with a `luau-lsp` configuration that points to it, `.vscode/settings.json`, and `.luaurc`'s strict mode, as in [the template](../crates/plugin-host-luau/luau/template/), whose analysis by luau-lsp `just luau-lsp` runs. `sleip check` checks a plugin's sources against the same definitions with Luau's own type checker, built into Sleip of the version of the app's Luau (`sleip-analyzer`), in strict mode and of the solver that luau-lsp takes by default, so that a plugin that the editor passes passes Sleip, and the registry's CI. Plugins are written with `--!strict`. luau-lsp tells a field that a table lacks as it is read, a value of another type, a node of no such kind and a field that a table requires and lacks; Luau's table types take more fields than they name, so that a field of a wrong name in a table that a plugin gives is told by the host, as it refuses it.
- **Calls**: `cx`, which each callback takes first, holds a function of each of the SDK's requests, by its Rust name, read from the SDK's own type, so that a new request reaches Luau at once. A request of fields takes them as arguments in their order, as `cx.line_text(pane, row, reach)` or `cx.new_tab(window, index, directory)`, a `nil` leaving an optional one out, its default or its none, and a plain value's `nil` being its value; a request of a value takes it, as `cx.selection_text(pane)`; a request of none takes nothing, as `cx.focused_pane()`. An argument of another shape, or more arguments than the request takes, raises an error that names it, as `bad argument to cx.line_text: argument 2 (row): expected a whole number, not a text`, and the host takes no request. `cx` is read-only, and reaches the host within the plugin's calls alone.
- **Answers**: a call answers with the response's value, as a pane's id, a table of a line or `nil` of none; `true` of a write done, a declaration or a request of the app's taken; or, of a failure, `nil`, the error's message and its code. A response of fields is a table of them, as a new tab's `{ tab = 3, pane = 7 }` and a grid's `{ columns = 80, rows = 24 }`.
- **Errors**: a failed request answers `nil`, its error's message and its code, as Lua's standard library answers, so that `assert(cx.paste(pane, text))` raises the message, after the place of the call, and `local done, message, code = cx.paste(pane, text)` reads them; the code's hundreds tell its category, as 101 of a pane that is not there. A Lua error raised by a callback ends the call, and counts as a plugin failure; a render's or a look's of `applies`, which return no error, the plugin tells after the call, through `Plugin::take_failure`, which the host takes as the error of a call that tells one: its writes go, a view's last tree stays, and a fault, as a deadline passed, faults the plugin.
- **Shapes**: the SDK's types cross as tables of their Rust names: a struct is a table of its fields, as `close_escape` and `min_width`, a field of `nil` left out; a variant of a value is a table of one field, of its name, as `{ pane_corner = { pane = 3, corner = "top-right" } }` or `{ up = 3 }`, and a variant of none its name, a text, as `"page-up"` or `"dialog"`; a map's keys are as they are, a command's arguments' and a state's; a list is a sequence; a capability is its text, as a manifest writes it, `"pty-write"` or `"net:example.com"`. A plain value, an event's, a state's, a config's or a command's argument, is as a Luau table is: `nil`, `true` or `false`, a number, a text, or a table of them, a sequence a list and another table, an empty one too, a map. A whole number is an integer. A plain value nests 64 levels of tables at most, as the host takes it, and another value 144, a tab's layout of many splits; a value holds a million values at most, those read out of its tables and not converted yet among them, so that a table that holds itself many times holds no more of the host's memory, and 16 MiB of texts and buffers, as the VM holds one text however many times its tables hold it; a value past them is refused, a table that holds itself too; a conversion stops as its call's deadline passes, a request's arguments as the call runs and a render's view within the render's deadline, as a deadline passed; a number is finite, a NaN or an infinity refused; its tables are read raw, of no metamethod, as no code of the plugin's runs outside its calls.
- **Views**: a view's node is a flat table of its fields, its `kind` a text, and its kind's own fields beside the node's, as `{ kind = "button", key = "go", text = "Go", icon = "play" }`: a text's text and a button's or a toggle's label are its `text`, an icon's name its `icon`, as the node's `label` is what accessibility calls it; its `children` a list of nodes, a list's `row` a node, and its `layout`, `style` and `progress` tables of their fields. A field of another kind than the node's, or of none, is refused, of its place in the tree, as `the node at children[2].row: no field on of a text node`. A view converts without recursion, a node at a time, and nests `Node::DEEPEST` levels at most as it converts, a list's row a level of its own, so that no tree deeper than the host takes is made: a deeper one, or a node that holds itself, is refused; a node that a list of children holds many times converts each time, and counts each time against the million values.
- **Events**: an event is a flat table of its `kind`, its name, and its fields, as `{ kind = "tab-created", tab = 3, window = 1, index = 0 }`, `{ kind = "keys-changed" }` or a panel's closing `{ kind = "panel-closed", id = "find" }`; `on_event` takes a list of them. A view's event alike, of its `view`, its node's `key`, its `kind` and its fields, as `{ view = { panel = "find" }, key = "query", kind = "submit", text = "x", shift = false }`.
- **Data**:
  - Small values are plain tables, possibly nested: command arguments, view trees, events.
  - The host stores view trees as a flat arena of nodes and diffs them between renders. A node that holds state, as a text field or a list, has a key, by which the host keeps its state, its text, caret, selection and composition, its scroll; the plugin learns of changes through events, and sets a field's text through a request. A large list is data, its items flat with a row's template, which the host instantiates for the rows that show: its plugin gives its count and a window of its items, those that the host asks for about the rows that show, by `list-range`, as they scroll past the window.
  - Bulk data uses buffers. For example, a highlighter returns its spans in one `buffer`, which the host reads in one copy: reading 100000 spans from tables costs 6–7 ms, against about 1.5 ms through Lua's C API ([F0-22](tasks/F0-foundations.md#f0-22--spike-lua-plugin-runtimes)).
- **Handles to host objects**: windows, tabs and panes are opaque integer ids, as in the Rust front end.
- **Rows** are numbers, which output, the view's scrolling and the scrollback's limit leave a row, until it leaves the scrollback; a resize numbers the rows anew. A command of a cell, as a click's, takes the number of the row clicked, as the drawn frame showed it ([ARCHITECTURE.md](ARCHITECTURE.md#kernel-data-model)).
- **Plugin-defined events**: `custom` events carry a name, of the plugin's namespace, as `search.progress`, a schema id, as `search.progress/1`, and a plain value, the SDK's `message::Data`: text, numbers, true or false, and lists and maps of them, as a Luau table is. `cx.emit` emits one, which comes after the call returns. A value, an event's or a state's, nests `Data::DEEPEST` levels at most, 64, as the host walks the values that it takes recursively on the main thread: a deeper one is refused, `failed`, checked without recursion and dropped a level at a time, a worker's on its own thread; the Luau front end's conversion of tables keeps the same limit. A view's tree nests `Node::DEEPEST` levels at most, 64, alike: a render of a deeper one is refused, the last staying, a worker's checked on its own thread.

### Versioning

- The API version is the version of `plugin-sdk` (`plugin-sdk 0.1.0` ↔ API 0.1).
- New APIs start as unstable: in Rust behind the `unstable` feature; in Luau every request is a function of `cx` by its Rust name, unprefixed, as the user chose on 9 November 2026, the whole API experimental while `plugin-sdk` is 0.x, until F6, so that a request that becomes stable keeps its name and no plugin breaks of a rename. They become stable in a minor release.
- The manifest declares the API version it requires (`api = "^0.1"`). The host supports the current major version and, through adapters, the previous one.
- Before 1.0, minor versions may break compatibility. After 1.0, semver is followed strictly.

### Illustrative draft

> The Luau front end of the API as `plugin-sdk` 0.1 has it: each of `Cx`'s requests by the Rust front end's name, of its arguments and its answer, and the Rust types' shapes as tables. The definitions file that `cargo xtask luau-types` makes of the Rust types, [`lokyr.d.luau`](../crates/plugin-host-luau/luau/lokyr.d.luau), is the reference of their names and shapes; this draft tells what they mean. The calls are as the [Luau API](#the-luau-api) above has them, a write answering `true`, or `nil`, the error's message and its code: `(boolean, Error?)` below reads `(true?, string?, number?)`. A view's nodes and the events take the flat shapes shown here, as `{ kind = "text", text = "…" }` and `{ kind = "tab-created", … }`.

```luau
-- lokyr.d.luau: the types of the plugin API, for the type checker.

type WindowId = number
type TabId = number
type PaneId = number

-- Command arguments: named values of a flat schema, each a string, an
-- integer, a number or a boolean.
type Args = { [string]: string | number | boolean }

-- Plugins should prefer theme tokens (e.g. "tabs.active.bg") to literals.
type Color = string | { r: number, g: number, b: number, a: number? }

-- How a split lays its two parts out, the first left or above; a part of
-- a split; an edge of a pane, which a resize moves.
type Direction = "horizontal" | "vertical"
type Side = "first" | "second"
type Edge = "left" | "right" | "top" | "bottom"

-- A failed call's error: its message, then its code, whose hundreds tell
-- its category: 1xx usage, 2xx failure, 3xx security, 4xx fault (ADR 0019).
-- `(T?, Error?)` below reads `(T?, string?, number?)`.
type Error = string

type WindowInfo = {
    id: WindowId,
    tabs: { TabId }, -- in their order
    active: TabId, -- the tab that it shows
    number: number?, -- from 1, the lowest that no other open window had as it opened; none for the quick terminal's
    keys: string?, -- the keys that bring it forward, as the platform writes them, as "⌥⌘1", for its titlebar to show; nil while the config hides them
    bell: boolean, -- its bell rang while it had not the focus, until it takes it, as `[bell] mark-title` marks its title, for a title that a plugin draws, as a single tab's; its views of the tab bar render again as it changes
    quick: boolean, -- a quick window, which slides in over every app's windows
    hidden: boolean, -- hidden, as a quick window hides
}

-- Where a quick window shows: the edge that it slides in from, or the centre, where it fades; how far it reaches into
-- its screen's area for windows, and its length along the edge, nil for the screen's whole, each a percentage, above 0
-- and at most 100, or points, from 1 to 100000; its screen; its slide, in milliseconds, from 0 to 2000; and whether it
-- shows on every Space.
type QuickPlace = {
    edge: "top" | "bottom" | "left" | "right" | "center",
    primary: { percent: number } | { points: number },
    secondary: ({ percent: number } | { points: number })?,
    screen: ("main" | "pointer" | "menu-bar")?,
    animation: number,
    ["every-space"]: boolean,
}

type BarPlace =
    | { kind: "titlebar" }
    | { kind: "below-titlebar" }
    | { kind: "bottom" }
    | { kind: "left", width: number }
    | { kind: "right", width: number }
    | { kind: "sidebar", width: number } -- down the left from the window's top, through the titlebar, the window's buttons over its top, the panes beside it below the titlebar
    | { kind: "title" } -- across the whole titlebar, in place of the window's title, as a window of a single tab shows its title and its marks; none in full screen

type TabInfo = {
    id: TabId,
    window: WindowId,
    title: string, -- its own, as `set_tab_title` gave it, or its active pane's program's, or, of `process-read`, its pane's folder and program, as "src — vim"
    color: Color?, -- its own, as `set_tab_color` gave it, or its active pane's program's, an RGB one
    active: boolean, -- whether its window shows it
    panes: { PaneId }, -- in its layout's order
    focused: PaneId, -- the pane that has its keyboard
    zoomed: PaneId?,
    activity: boolean, -- output came while it did not show, until its window shows it
    bell: boolean, -- its bell rang while it did not show, until its window shows it
    progress: { state: "normal" | "error" | "paused" | "indeterminate", value: number? }?, -- OSC 9;4's, of its active pane or another, the value from 0 to 1 if told
    printing: boolean, -- of `process-read`: the program of `running` prints of its own, output within the last 3 seconds apart from the echo of the user's input and the redraw of a new size, as a build, rather than ssh, a REPL or an agent waiting for the user
    running: boolean, -- of `process-read`: a program other than a shell runs in its foreground
    alternate: boolean, -- of `process-read`: its active pane's program shows the alternate screen, as vim's or htop's
    secure: boolean, -- the keyboard's protected input is on for it, as its window has the focus and shows it
    agent: ("working" | "waiting" | "done")?, -- the state of an AI agent that runs in it
}

-- A tab's layout: a pane, or a split of two layouts, whose ratio is the
-- first part's share, from 0 to 1, or none to share it by their panes.
type Layout =
    { pane: PaneId }
    | { direction: Direction, ratio: number?, first: Layout, second: Layout }

-- Of its content, of points, of a fraction of its parent's, from 0 to 1, or the window's: the titlebar's height,
-- and the width that the window's buttons take at its left, which a sidebar leaves them; 0 without them, as in
-- full screen.
type Length = "auto" | number | { fraction: number } | "titlebar" | "buttons"

-- A node's place and size among its siblings, and its children's, of flexbox's properties.
type NodeLayout = {
    direction: ("row" | "column")?,
    gap: number?,
    padding: { top: number?, right: number?, bottom: number?, left: number? }?,
    align: ("start" | "center" | "end" | "stretch")?,
    justify: ("start" | "center" | "end" | "space-between")?,
    width: Length?,
    height: Length?,
    min_width: Length?, -- its least width, which it keeps as its siblings share less room, as a tab among many
    grow: number?,
}

type Style = {
    hides_indicator: boolean?, -- of a scroll area, no indicator of its scroll, as a bar of tabs
    holds_ring: boolean?, -- the ring of the keyboard's focus of a node inside it round its own box, as a search field's box round its text
    hides_ring: boolean?, -- no ring of the keyboard's focus round it, as a palette's field, whose caret alone tells it
    text_size: ("regular" | "small" | "large")?, -- its texts' size, its children's too, unless they give their own: small, eleven thirteenths of the interface's, as a find bar's count and field; large, eighteen thirteenths, as a palette's field
    font_size: number?, -- its texts' size in points, 4 to 200, in place of text_size's, its children's too
    font_weight: ("light" | "regular" | "medium" | "semibold" | "bold" | "heavy")?, -- its texts' weight, its family's face nearest to it, its children's too; regular of the terminal's family its own face
    font_family: ("interface" | "terminal")?, -- its texts' family, its children's too: the interface's, or the terminal's, of [font]'s face and size, as a command's line
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
    text: string?, -- for text nodes; a button's and a toggle's label
    icon: string?, -- for icon nodes, by its name; a button's and a toggle's icon, before its label
    on: boolean?, -- for toggle nodes, the plugin's state
    disabled: boolean?, -- for button and toggle nodes: dimmed, and takes neither a click nor the focus
    placeholder: string?, -- for field nodes, shown while the field is empty
    progress: {
        state: ("normal" | "error" | "paused" | "indeterminate")?,
        value: number?, -- from 0 to 1
        shape: ("bar" | "ring" | "outline")?, -- an outline its parent's, along its edges, over its children
        radius: number?, -- a bar's corners', a pill's without it; an outline's, its parent's without it
        thickness: number?, -- a ring's line's, or an outline's
        segment: number?, -- an indeterminate one's moving part, of the bar, of the turn or of the outline, from 0 to 1
        period: number?, -- an indeterminate one's sweep, turn or run along, in seconds
    }?, -- for progress nodes: its track of the style's background, its fill of the style's color
    count: number?, -- for list nodes, of all their items
    first: number?, -- for list nodes, the index of the first of `items`
    items: { ListItem }?, -- for list nodes, flat: a window of them, from `first`, as the host asks for them
    row: Node?, -- for list nodes: the template of a row, of containers, texts, icons, spacers and separators
    row_height: number?, -- for list nodes, of rows of one height, a point at least; without it, each row takes its own, a point at least
    menu: boolean?, -- for list nodes, a menu's look: the row under the pointer hovered, the selection a rounded band inset from its edges, of the accent's whether the list has the keyboard or not, the first item selected as it shows, as a palette's, and a row activated of a click, the selected one's too, as a menu's item, where another list's takes a double click
    tabs: { { label: string, icon: string?, disabled: boolean? } }?, -- for tabs nodes, in their order: a control of them over the box of its page, its child
    chosen: number?, -- for tabs nodes, the tab chosen, whose page is its child; a click or the arrows choose another, as "select" tells
    distribution: ("fit" | "fill" | "fill-equally" | "fill-proportionally")?, -- for tabs nodes, as macOS's segmented controls: each tab of its content's width, the control in the middle, by default; or the control of the view's width, each tab of its content's and an equal share of the room left, of equal widths, or of its content's share
    layout: NodeLayout?,
    style: Style?,
    focusable: boolean?, -- takes the keyboard's focus, by its key
    -- Dragged among its siblings that are reorderable too, along its parent's direction, by its key, as a
    -- bar's tabs: the host moves it under the pointer within their room, over them, slides the others out of
    -- its way and scrolls a scroll area around them past its ends; Escape puts it back. A drop at another
    -- place gives it a "reorder" of its index among them, which the next render shows; a render that keeps
    -- the order, or none within half a second, slides them back.
    reorderable: boolean?,
    -- A reorderable node's tab, as a bar's tab carries it: dragged across its parent past a margin, or out
    -- of its window, it leaves its siblings, which close its room, and the host shows the tab's panes under
    -- the pointer, its window showing another tab meanwhile; the bar of another window opens a room for it.
    -- A drop there gives it a "drop-on-bar", on a pane of another tab, whose half nearest the pointer lights
    -- up, a "drop-on-pane", outside every window a "drop-outside", before its "drag-end", of which its
    -- plugin makes the tab's move; Escape, or a drop elsewhere, puts it back.
    carries: number?,
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

-- What a search of a pane's terminal looks for: the text, as it is, or a regex, of the syntax of Rust's `regex`;
-- case "smart" counts of a text of a capital letter alone; whole words alone, as `whole_word` asks.
type SearchQuery = { text: string, regex: boolean?, case: ("smart" | "sensitive" | "insensitive")?, whole_word: boolean? }

-- What a selection takes of its cells: the cells in reading order, the words or the logical lines that its ends are in,
-- or the block of their columns and rows.
type SelectionKind = "cells" | "words" | "lines" | "block"

-- A match of a search: its first cell and its last, each { column, row's number }.
type SearchMatch = { first: { number }, last: { number } }

-- A command that the shell marked, as its integration tells it (OSC 133): its id, of the pane's commands from 1 in their
-- prompts' order; each mark's cell, { column, row's number }, if its row is in the pane still, of its prompt, the input
-- typed past it, its output, and its end; and its exit status, if it ended and the shell told it. Its output is the
-- cells from `output` to before `end`, in reading order, which `read_lines` reads.
type ShellCommand = {
    id: number,
    prompt: { number }?, input: { number }?, output: { number }?, ["end"]: { number }?,
    status: number?,
}

-- A pane's view, as its last frame showed it: the number of its first row, its rows, the number of the screen's first
-- row, as `top` is while it shows the screen and less above, the number of the oldest row, which the scrollback's limit
-- raises, the epoch of the rows' numbers, which a resize changes as it numbers them anew, and whether the alternate
-- screen shows, of rows of its own, of no scrollback nor commands.
type Viewport = { top: number, rows: number, screen: number, oldest: number, epoch: number, alternate: boolean }

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
    -- A tab's title or color changed, its own, its program's, or its active pane's folder and program as output comes;
    -- to a plugin without `process-read`, which reads no folder nor program, as its title or color changes alone.
    | { kind: "tab-changed", tab: TabId }
    -- A pane's header changed: its program's title, or, of none, its folder and program as output comes; to a plugin
    -- without `process-read`, which reads no folder nor program, as its program's title changes alone.
    | { kind: "pane-changed", pane: PaneId, tab: TabId }
    -- The keymap's bindings changed, as the config's or a plugin's: the keys of `keys_of` may be others.
    | { kind: "keys-changed" }
    -- A window took the keyboard's focus, or lost it, as the user clicks another app or another window; not as a question of
    -- the app's on it takes it.
    | { kind: "window-focus", window: WindowId, focused: boolean }
    -- The pointer over another cell of a pane, or out of it, or the modifiers changed over it, or a frame changed the text
    -- of its row or scrolled its pane's rows under it, at the same cell; while Command is up, its
    -- moves come only as the system gives them, as a program that tracks the mouse asks.
    | { kind: "pointer", pane: PaneId, at: { number }?, mods: { shift: boolean, control: boolean, option: boolean, command: boolean } }
    -- A pane's view changed, as a frame showed it: its first row, as the user or output scrolled it, its rows, the
    -- screen's first row, as output scrolled the screen under a view above it, the oldest row, as the scrollback's limit
    -- took rows away, the epoch or the screen shown; once a frame of the pane at most, a pane that does not show telling
    -- it as it shows again.
    | { kind: "viewport-changed", pane: PaneId, viewport: Viewport }
    -- A command that the shell marked started, as its output's mark came, and ended, as its end's came, of the status
    -- that the shell told; or of none, as its pane let go of it before its end, as a reset or a screen cleared under it
    -- does. A shell that marks no end tells none. To a plugin of `history-read`, which a subscription needs.
    | { kind: "command-started", pane: PaneId, command: number }
    | { kind: "command-ended", pane: PaneId, command: number, status: number? }
    -- A read of rows that the plugin asked for, to it alone; incomplete if a resize renumbered the rows.
    | { kind: "lines", id: number, pane: PaneId, lines: { Line }, complete: boolean }
    -- A part's matches of a search that the plugin asked for, to it alone, the nearest to the view first; how many it
    -- found so far, past the 100 000 kept too; and whether it went to its end, or stopped at a resize or the pane's close.
    | { kind: "searched", id: number, pane: PaneId, matches: { SearchMatch }, count: number, done: boolean }
    -- A panel of the plugin's closed, as a click outside it, Escape, the app or the plugin closed it, to it alone.
    | { kind: "panel-closed", id: string }
    -- The user's answer to a question of `ask_text`, to it alone: the text, or none as the user cancelled.
    | { kind: "answered", id: number, text: string? }
    -- The user's drag of the edge of the plugin's tab bar's column ended, to it alone: its width since, in points,
    -- which the plugin keeps, as for its next start.
    | { kind: "tab-bar-resized", width: number }
    -- The plugin's section of the config changed, to it alone, which `config` reads.
    | { kind: "config-changed" }
    -- A timer of the plugin's fired, to it alone, once however many times it fired since the plugin last heard of it.
    | { kind: "timer", id: number }
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
    | { pane_top: PaneId } -- across a pane's top, of its width, at its edge, as a command's header
    | { window_corner: { window: WindowId, corner: Corner } } -- of its room below the tab bar
type Corner = "top-left" | "top-right" | "bottom-left" | "bottom-right" | "top" -- "top": the top's middle, an eighth of the height down, as a palette's

type PanelOptions = {
    anchor: Anchor,
    -- a dialog by default; a menu of menu items alone shows as the platform's own; an overlay takes the pointer, a scroll
    -- going through to the pane under it but where a list or a scroll area of its own is, and not the keyboard
    role: ("tooltip" | "menu" | "dialog" | "overlay")?,
    close_outside: boolean?, -- a click outside it closes it
    close_escape: boolean?, -- Escape closes it, while it has the keyboard's focus
    -- it holds the keys back from the program, as a mode of the keyboard's, as the copy mode's: while it shows and the
    -- keyboard is its pane's, of an anchor of a pane, or any pane's, of another, the keys that no binding takes and the
    -- text typed go nowhere; the pane keeps the keyboard, and its cursor its look
    takes_keys: boolean?,
}

type UiEvent = {
    view: ViewId,
    key: string,
    kind: "click" | "double-click" | "middle-click" | "hover-enter" | "hover-leave" | "context-menu"
        | "input-changed" | "submit" | "focus" | "blur" | "scroll"
        | "drag-start" | "drag-move" | "drag-end" | "drag-cancel" | "reorder" | "drop-on-bar"
        | "drop-on-pane" | "drop-outside" | "set-value" | "toggle" | "select" | "activate" | "list-range",
    text: string?, -- a field's text, as the host holds it: at each change, "input-changed", and as Return goes to it, "submit"
    revision: number?, -- an "input-changed" text's, one more at each edit, which `set_text` gives back
    shift: boolean?, -- a "submit"'s, whether Shift was held, as Shift-Return goes the other way
    item: number?, -- a list's item, selected or activated
    first: number?, count: number?, -- the items of a list that the host asks for, which the next render gives
    x: number?, y: number?, -- a drag's, from its start, or a scroll's, in points; a "drop-outside"'s, where the tab's new window puts its content's corner on the screens
    index: number?, -- a "reorder"'s, the node's new index among its reorderable siblings, before its "drag-end"; a "drop-on-bar"'s, the tab's index among the window's
    window: number?, -- a "drop-on-bar"'s, the window whose bar took the tab
    pane: PaneId?, edge: ("left" | "right" | "top" | "bottom")?, -- a "drop-on-pane"'s, the pane and its edge nearest the pointer
    value: string?, -- set by accessibility, as VoiceOver's; its press comes as a click, its focus as a focus
    on: boolean?, -- a toggle's state asked, the other than its render's, which the next render gives
}

-- A command, as its registration describes it.
type Command = {
    name: string, -- `namespace.action`
    title: string,
    category: "app" | "windows" | "tabs" | "panes" | "input" | "clipboard" | "edit" | "find" | "view",
    capabilities: { string }?, -- those that its action needs, as "pty-write"
    params: { { name: string, kind: "string" | "integer" | "number" | "boolean", default: (string | number | boolean)? } }?,
    when: string?, -- the context in which it can run, as a binding's `when`
}

-- The themes that the app can show, as a read of them gives them: the appearance in force, its theme as `[theme]` names
-- it, and every theme by name, a file of the user's folder before one of its name that the app holds or builds in.
type Themes = {
    appearance: "light" | "dark", current: string,
    themes: { { name: string, variant: "light" | "dark", origin: "folder" | "held" | "built-in" } },
}

-- A command, as a read of the registry gives it; `plugin`, `keys`, `when` and `enabled` to a plugin of `user-commands`
-- alone: the plugin that registered it, nil of the app's own; its keys of no arguments, as "⌘T"; its context; and whether
-- the menus enable it now.
type CommandInfo = {
    name: string, title: string, category: string,
    params: { { name: string, kind: string, default: (string | number | boolean)? } },
    plugin: string?, keys: string?, when: string?, enabled: boolean?,
}

-- `keys`: e.g. "cmd+f" or "ctrl+a>c"; `when`: e.g. "pane.focused && !search.open", where a plugin's
-- panels give `namespace.open` while one shows, not one of a pane behind another tab or a zoomed pane, and
-- `namespace.focused` while one has the keyboard. A key taken from every
-- app, with `global-hotkey`, starts with "global:", as "global:ctrl+`", without `when`: one key, by its place on the US
-- layout, with Command, Control or Alt, or a function key; the config's global keys come first.
type Binding = { keys: string, command: string, args: Args?, when: string? }

-- An item of the menu bar's menu of a category, after its commands, or of the pane's context menu, after the editing's:
-- a command and its arguments, a whole call, as Split Left's `{ direction = "left" }`; its title the message `title` of
-- the plugin's catalogs, or `title` itself where they lack it, or the command's without one. The menu bar shows the keys
-- bound to the same call; the command's `when` dims the item, and its plugin's `applies` the context menu's.
type MenuItem = {
    place: { bar: "app" | "windows" | "tabs" | "panes" | "input" | "clipboard" | "edit" | "find" | "view" } | "pane-context",
    command: string, args: Args?, title: string?,
}

-- The logical line of a row: its text, and each character's cell, { column, row's number }.
type LineText = { text: string, cells: { { number } } }

-- `row`: to the row of that number, at the view's top, as far as the scrollback's first row and the screen let it.
type Scroll = { up: number } | { down: number } | "page-up" | "page-down" | "top" | "bottom" | "previous-prompt" | "next-prompt" | { row: number }

-- The host's requests, each by the Rust front end's name. A read answers at once, of the state as it is; a
-- write, and a request of the app's, as a window to open or a command to run, applies as the call returns,
-- the writes first; a read of the call reads the state before them. A write to a tab or a pane that an
-- earlier write of the call opens finds it. A call that returns an error, or fails, drops its writes and requests.
type Cx = {
    -- Declarations, at once.
    register_command: (command: Command) -> (boolean, Error?),
    bind: (binding: Binding) -> (boolean, Error?),
    -- An item of a menu, of a registered command and its arguments, which the plugin may run, for as long as it runs.
    add_menu_item: (item: MenuItem) -> (boolean, Error?),
    -- The workspace's events, as "tab-created", the pointer's, "pointer", the windows' focus, "window-focus", and plugins' own, as
    -- "search.progress".
    subscribe: (names: { string }) -> (boolean, Error?),
    -- An event of the plugin's namespace, which comes after the call returns.
    emit: (name: string, schema: string, value: any) -> (boolean, Error?),
    -- Runs a command, once the call returns: of a plugin of `user-commands`, any as the user's, without a declaration.
    run_command: (name: string, args: Args?) -> (boolean, Error?),
    -- The registered commands, in the order of their registrations, as the menus list them.
    commands: () -> ({ CommandInfo }?, Error?),
    themes: () -> (Themes?, Error?),
    -- Require `user-commands`: shows a theme in place of the appearance in force's, for a while, as a picker
    -- previews it; nil shows the config's.
    preview_theme: (name: string?) -> (boolean, Error?),
    -- Names a theme in the config's `[theme]` for an appearance, which shows at once, a preview ending: one of the
    -- appearance's themes, as `themes` lists them, else refused.
    set_theme: (variant: "light" | "dark", name: string) -> (boolean, Error?),

    -- The focus: the window that has it, and its pane that has the keyboard.
    focused_window: () -> WindowId?,
    focused_pane: () -> PaneId?,

    -- The workspace. A new tab's and a split's ids come at once; what a move made, as a new window, comes as
    -- the workspace's events.
    windows: () -> ({ WindowInfo }?, Error?),
    tabs: (window: WindowId) -> ({ TabInfo }?, Error?),
    layout: (tab: TabId) -> (Layout?, Error?),
    -- A tab of a new pane, whose program starts as a new pane's beside the window's focused one, at `index` of
    -- its tabs or at their end: the tab's id and the pane's. The window shows the tab that it showed. Its pane
    -- starts in `directory`, a path absolute or from `~`, or in the folder of a file there, if given, and as a new
    -- pane's of a path that is not there.
    new_tab: (window: WindowId, index: number?, directory: string?) -> ({ tab: TabId, pane: PaneId }?, Error?),
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
    -- The tab's panes beside a pane of another tab, at its edge, the tab's layout whole in that half, half and
    -- half: the tab goes, its programs running on, its window closing if it has none left.
    tab_into_pane: (tab: TabId, pane: PaneId, edge: "left" | "right" | "top" | "bottom") -> (boolean, Error?),
    -- A window of its own, its content's corner at `at` on the screens, from the menu bar's screen's top
    -- left corner, where it is given; a tab alone in its window stays, its window moving to `at`.
    tab_to_new_window: (tab: TabId, at: { x: number, y: number }?) -> (boolean, Error?),
    pane_to_new_tab: (pane: PaneId, window: WindowId, index: number) -> (boolean, Error?),
    pane_to_new_window: (pane: PaneId) -> (boolean, Error?),
    -- Moves an edge by `by`, a share from -1 to 1 of the room of the split whose divider it is.
    resize_pane: (pane: PaneId, edge: Edge, by: number) -> (boolean, Error?),
    -- A pane's grid, as its window last laid it out, as a resize by cells reads it.
    pane_grid: (pane: PaneId) -> ({ columns: number, rows: number }?, Error?),
    -- A pane's title, as its header shows it: its program's, or, of `process-read`, its folder and its program.
    pane_title: (pane: PaneId) -> (string?, Error?),
    -- The panes that have not their tab's keyboard, of a tab of more than one, faded toward their background by this
    -- share, from 0 to 1, their text, lines and cells' backgrounds, while the plugin runs; the most that the plugins ask.
    set_inactive_dim: (dim: number) -> (boolean, Error?),
    -- The keyboard goes to the pane that the pointer comes over, while the plugin runs, as any plugin asks.
    set_focus_follows_mouse: (on: boolean) -> (boolean, Error?),
    -- Swaps two panes of one tab, each taking the other's room, the tab's keyboard staying with its pane.
    swap_panes: (pane: PaneId, with: PaneId) -> (boolean, Error?),
    -- The split at `path`, the parts from the layout's top, takes the first part's share `ratio`, or its panes'.
    set_ratio: (tab: TabId, path: { Side }, ratio: number?) -> (boolean, Error?),
    equalize: (tab: TabId) -> (boolean, Error?),
    zoom: (tab: TabId, pane: PaneId?) -> (boolean, Error?),
    -- A window, its pane in `directory`, as `new_tab`'s, if given.
    open_window: (directory: string?) -> (boolean, Error?),
    -- A quick window of a new pane, which shows at `place` over every app's windows and takes the focus, its pane in
    -- `directory`, if given: its id, at once. With `global-hotkey`. The plugin's own, which closes as the plugin stops,
    -- shown first if hidden, of the close's question of a program that runs.
    open_quick_window: (place: QuickPlace, directory: string?) -> (WindowId?, Error?),
    -- Shows one of the plugin's quick windows at `place`, which takes the focus, or hides it, of nil, the app that had
    -- the focus taking it back. With `global-hotkey`.
    show_quick_window: (window: WindowId, place: QuickPlace?) -> (boolean, Error?),
    close_window: (window: WindowId) -> (boolean, Error?),
    quit: () -> (boolean, Error?),

    -- The views and the panels.
    request_render: (view: ViewId) -> (boolean, Error?),
    -- Sets the text of a field of the view, by its key, cut to the field's 10 000 characters: its caret at its end, and no
    -- "input-changed"; of the revision of the "input-changed" that it answers, so that it is not set over the keys typed since.
    set_text: (view: ViewId, key: string, text: string, revision: number) -> (boolean, Error?),
    -- Moves the selection of a list of the view, by its key, to an item, which the list scrolls to.
    select: (view: ViewId, key: string, item: number) -> (boolean, Error?),
    -- Scrolls the nearest scroll area around a node of the view, by its key, as little as shows it, once the view shows
    -- it, as a tab bar shows its active tab: the node of a render yet to show too.
    reveal: (view: ViewId, key: string) -> (boolean, Error?),
    -- Gives the keyboard's focus to a node of the view, by its key, which tells it "focus": a field selects all of its
    -- text, as macOS's does as it becomes the first responder, so that typing replaces it.
    focus_node: (view: ViewId, key: string) -> (boolean, Error?),
    -- Opens a panel, by an id of the plugin's, which the host then renders; a "panel-closed" event tells as it closes.
    -- A dialog or a menu takes the keyboard as it opens, at its first field, or at its first node that takes it.
    open_panel: (id: string, options: PanelOptions) -> (boolean, Error?),
    -- Closes it at once, so that the same call can open it again; its tree goes, and its plugin hears "panel-closed", as
    -- the call returns: a call that fails, or a look of `applies`, leaves it open, of its tree.
    close_panel: (id: string) -> (boolean, Error?),
    -- Puts the plugin's tab bar, in the windows that show it, in the titlebar, below a titlebar of the window's title,
    -- at the bottom, or down a column on a side of 60 to 600 points; `tab_bar_place` reads it, a column's width as the
    -- user's drag of its edge left it, as the bar's view lays its tabs out across or down.
    set_tab_bar_place: (place: BarPlace) -> (boolean, Error?),
    -- Puts the plugin's tab bar of one window at its own place, over the place of every window, as a window of a
    -- single tab shows its title in its titlebar; nil gives it that place again.
    set_window_tab_bar_place: (window: WindowId, place: BarPlace?) -> (boolean, Error?),
    tab_bar_place: () -> (BarPlace?, Error?),
    -- The keys that run a command of its arguments, one key that no later binding takes, as the platform writes them,
    -- as "⌘1"; none without a binding.
    keys_of: (command: string, args: { [string]: any }) -> (string?, Error?),
    -- Asks the user for a text, in an alert on `window`, or on the focused one, of a field that holds `text`:
    -- returns the question's id, and the answer comes as an "answered" event. The message and the detail are
    -- messages of the plugin's catalogs, shown in the language shown, or the texts themselves where they lack them.
    ask_text: (window: WindowId?, message: string, detail: string, text: string) -> (number?, Error?),
    -- The text of a message of the plugin's catalogs, of its arguments, a text or a number each, in the language
    -- shown, or English's where that one lacks it, as a view's texts, labels and tooltips take them: the id itself
    -- where the catalogs lack it. A change of the language rebuilds the menus, which `keys-changed` tells.
    message: (id: string, args: { [string]: string | number }?) -> (string, Error?),

    -- The terminal. The reads require `terminal-read`, and return a `denied` error without it.
    selection_text: (pane: PaneId) -> (string?, Error?),
    -- The logical line of a row, by its number, its wrapped rows joined as far as `reach` rows before it and after it, 256 at most, with each character's cell.
    line_text: (pane: PaneId, row: number, reach: number) -> (LineText?, Error?),
    -- The target of the OSC 8 link at a cell, by its row's number.
    link_at: (pane: PaneId, column: number, row: number) -> (string?, Error?),
    -- Reads 100 000 rows at most, from the row numbered `first`, on a thread of the host's, a part at a time:
    -- returns the read's id, and the rows come as a "lines" event. Four of a plugin's reads are under way at most,
    -- past which another fails until one ends; they stop as the plugin stops.
    read_lines: (pane: PaneId, first: number, count: number) -> (number?, Error?),
    -- Searches a pane's terminal, its scrollback too, on a thread of the host's, a part of its rows at a time from the
    -- view's outward: returns the search's id, and the matches come as "searched" events. The plugin's search of the pane
    -- before ends. The host keeps 100 000 matches at most as their cells' pins, through new output, scrolls and reflows,
    -- until the next search of the pane, `end_search` or the pane's close; the count goes on past them.
    search: (pane: PaneId, query: SearchQuery) -> (number?, Error?),
    end_search: (id: number) -> (boolean, Error?),
    -- Shows a search's matches over its pane's cells in the theme's `search.match`, or hides them, as `all` says, and
    -- its current match apart in `search.match.current`, whose row the view scrolls to, as the call returns: the host
    -- draws those that the view shows at each frame, until the search ends. Those of the plugin's search before of the
    -- pane, if they showed, show until this one does. While a current match shows, output leaves the view where it is,
    -- whatever `scroll-on-output` says.
    show_matches: (id: number, all: boolean, current: SearchMatch?) -> (boolean, Error?),
    -- A search's matches in the rows' order, from the one at `first`, 100 000 at most, as their cells are now, those that
    -- left the scrollback gone; how many it found; and whether it went to its end.
    search_matches: (id: number, first: number, count: number) -> ({ matches: { SearchMatch }, count: number, done: boolean }?, Error?),
    -- A search's current match as its cells are now, which a resize moves, and its place among the matches now, which
    -- those that leave the scrollback lower; nil of none, or of one that left; and how many matches the host keeps.
    current_match: (id: number) -> ({ current: { place: number, match: SearchMatch }?, kept: number }?, Error?),
    -- The styles of a row, by its number, as runs of cells of one style.
    styles: (pane: PaneId, row: number) -> ({ StyleRun }?, Error?),
    -- Requires `history-read`: the commands that the shell marked, 10 000 at most kept, whose rows meet the rows
    -- numbered from `top` to before `bottom`, oldest first: the last whose first row is at `top` or above it, whose
    -- output the row `top` may show, and those after it whose first rows are above `bottom`; a command's first row is
    -- its prompt's, or the oldest row of one whose prompt left the scrollback but not its end.
    shell_commands: (pane: PaneId, top: number, bottom: number) -> ({ ShellCommand }?, Error?),
    -- Requires `history-read`: a command's line, as the user typed it: as the shell told it, of OSC 133 `C`'s
    -- `cmdline_url`, which a right prompt on its row leaves out, or else the text from its input's mark to its
    -- output's; as the long command's notification takes it.
    command_line: (pane: PaneId, command: number) -> (string?, Error?),
    -- The view, as its last frame showed it, which "viewport-changed" tells the changes of; no capability, as it tells
    -- no text.
    viewport: (pane: PaneId) -> (Viewport?, Error?),
    -- Decorates the cells from one to another, in reading order, each { column, row's number }: returns the
    -- decoration's id. It keeps its text as output scrolls it and a resize reflows it, and goes with it.
    decorate: (pane: PaneId, from: { number }, to: { number }, look: DecorationLook) -> (number?, Error?),
    undecorate: (pane: PaneId, id: number) -> (boolean, Error?),
    -- Lays a mark of a color, { red, green, blue }, in the pane's gutter, at the left of its cells, on a row by its
    -- number: returns the mark's id. It keeps its row as output scrolls it and a resize reflows it, and goes with it;
    -- it shows over the mark of a prompt of its row, and only while `[window] gutter` is on.
    mark_gutter: (pane: PaneId, row: number, color: { number }) -> (number?, Error?),
    unmark_gutter: (pane: PaneId, id: number) -> (boolean, Error?),
    select_all: (pane: PaneId) -> (boolean, Error?),
    -- Selects the cells from one to another, each { column, row's number }, as `kind` takes them, "cells" by default,
    -- as a drag would, which a copy then takes, and scrolls the view, through the scrollback, to show the last, the end
    -- that a keyboard's selection moves, as the call returns.
    select_cells: (pane: PaneId, first: { number }, last: { number }, kind: SelectionKind?) -> (boolean, Error?),
    -- A cursor of the plugin's over a cell, { column, row's number }, in place of the terminal's, which hides while it
    -- shows, as a mode of the keyboard's over the scrollback moves one: of the terminal's cursor's shape and colors,
    -- still, while the view shows its row, keeping its text as output scrolls it and a resize reflows it; while the
    -- other screen shows, the terminal's; nil gives the terminal's cursor back, as the plugin's stop or fault does.
    show_cursor: (pane: PaneId, at: { number }?) -> (boolean, Error?),
    -- Takes the selection away, as a click would.
    deselect: (pane: PaneId) -> (boolean, Error?),
    -- The cell of the terminal's own cursor, { column, row's number }.
    cursor_cell: (pane: PaneId) -> ({ number }?, Error?),
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
    -- Requires `open`: a URL of `http`, `https`, `mailto` or `file`, a `file` URL of this machine alone, of no host,
    -- `localhost` or its name, as OSC 7's folder, or a path from `/` or `~`.
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

    -- Declarations, at once: a timer of the host's time, which fires once `delay` milliseconds passed, from 10 ms, a
    -- shorter delay waiting 10, to a day, and again at each delay after, if it repeats: its id, of its "timer" events.
    -- 64 of a plugin's at most; they end as it stops. A cancelled timer's events come no more, the one that it owes
    -- neither; one that is not there stays so.
    start_timer: (delay: number, repeats: boolean) -> (number?, Error?),
    cancel_timer: (timer: number) -> (boolean, Error?),
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
- **The pane's context menu**, of a right click, lists the commands of a cell, which take the `column` and the `row` of the cell clicked, then those of the clipboard and of the editing that need no arguments, then the plugins' items, a group of each namespace of their commands; its commands run in the pane that it opened over ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).
- **A plugin's menu items** add a command with its arguments to a menu, as a command that needs an argument has no item of its own: `add_menu_item`, a declaration, of the menu bar's menu of a category, after its commands, each plugin's a group of their own, or of the pane's context menu, each plugin's a group of their own too, as `splits`' Split Left, `splits.split` of `{ direction = "left" }`. The command is registered, the arguments make a whole call, and the plugin may run it, as its manifest's commands and its capabilities say; the items go with their plugin, as it stops or the config turns it off. An item whose title is that of one of the app's items in the same menu, whatever its case and the dots of an item that asks more, as a plugin's "Copy" in the Edit menu, is left out, as the log tells, so that the user never takes a plugin's item for the app's. The menu bar shows the keys bound to the same call, as ⌘D beside Split Right; the command's `when` dims an item in both menus, and its plugin's `applies`, of the item's arguments, in the context menu.
- **The kernel's registry**, `kernel::commands::Registry`, registers the commands of the app and of the plugins, and checks a call before its action runs: its name, and its arguments, to which it adds the defaults of those left out; an integer is a number too. The checks of a command and of a call are the SDK's, `Command::check` and `Command::check_call`, which the registry and the fake host share. A plugin's call is checked as well against the commands that its manifest declares and the capabilities that it has, its own commands aside. The action runs where it can: in the app for those that the platform does, in the plugin that registered the command otherwise.
- **The app's commands**, whose actions the platform does: `app.quit`, `window.new` and `window.close`; the appearances, `app.appearance-system`, `app.appearance-light` and `app.appearance-dark`; and `pane.look-up`, `pane.man-page`, `pane.man-search` and `pane.search-web`, the context menu's Look Up, Open man Page, Search man Page Index and Search the Web, of the selection.
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
| `process-spawn` | Starting processes; refused before 1.0.0, with a notice ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) | Very high |
| `net:<host>` | HTTP requests to the listed hosts, whatever their case; refused before 1.0.0, with a notice ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) | High |
| `fs-read:<path>`, `fs-write:<path>` | Access to the listed paths and to the paths under them, of whole components and no `..` | High |
| `global-hotkey` | System-wide shortcuts, bindings of `global:` keys, and quick windows, which slide in over every app's | Medium |
| `env-read:<name>` | Reading the listed variables of the app's environment and of the panes' programs', with `*` as `AWS_*` | High: they often hold secrets, as `GITHUB_TOKEN` |
| `input-read` | Seeing the keys typed before they reach the program | Very high: a keylogger's power |
| `history-read` | Reading the commands that the shell ran and their output, which shell integration marks, and hearing their starts and ends, as `shell_commands`, `command-started` and `command-ended` | High: a password typed by mistake ends up there |
| `process-read` | Seeing the panes' processes: their command lines and working directories | Medium: command lines can hold passwords |
| `workspace-write` | Opening, closing and moving windows, tabs and panes, the keyboard's focus between them, a split's ratio, the panes equalized and swapped, the zoom, and the app's quit ([ADR 0039](adr/0039-workspace-view-decorate-pointer-capabilities.md)) | High: it can end every program, start shells, or move the keys typed to another pane |
| `pane-view` | The panes' scroll and selection, the screen and the scrollback cleared, the font's size, the tabs' titles and colors, the tab bar's place, the inactive panes' fade and the focus that follows the mouse | Medium: a cleared scrollback is lost text, and a selection is what a copy takes |
| `decorate` | Drawing over a pane: decorations, the gutter's marks, a search's matches and a held cursor | Medium: a drawing over the text can show what the terminal does not hold |
| `pointer-read` | The `pointer` event: the cell under the pointer over the panes, and the modifiers held | Low |
| `user-commands` | Running any command as the user's, without declaring it, as the palette does; reading the whole registry, with each command's plugin, keys and context, and the plugins installed ([ADR 0024](adr/0024-user-commands.md)); previewing a theme and naming one in the config's `[theme]`, as the palette's picker does | Very high: `pane.send-text` types into the shell |

A request takes every capability that it needs, as a subscription to `pointer` and to `command-started` takes `pointer-read` and `history-read`. The reads of the workspace, the plugin's own views, panels and questions, its declarations, its config and its state take none.

- **Commands** ([ADR 0019](adr/0019-commands-errors-and-permissions.md)): each has a category, `app`, `windows`, `tabs`, `panes`, `input`, `clipboard`, `edit`, `find` or `view` to start with, and the capabilities that its action needs, as `pane.paste` needs `clipboard-read` and `pty-write`. A plugin declares in its manifest the commands that it runs, `[uses] commands`, by name or by namespace, as `tabs.*`: one that it did not declare fails with `denied`, 301, and one whose capabilities it lacks with `denied`, 302. The commands that the user runs, through the keys, the menus or a plugin with `user-commands`, as the palette, and the plugin's own need no declaration. Without `user-commands`, a plugin reads the commands' names, titles, categories and parameters, not their plugins.
- **Declaration and consent**:
  - Capabilities are declared in the manifest. Sleip shows them at install time, with the commands that the plugin runs, grouped by category, and the capabilities that those need.
  - The app asks at an installed plugin's first activation, one question, of the app's, of the capabilities that its manifest declares and that no decision grants or refuses yet, each told in the language shown with its risk, the default button refusing; its activation waits for the answer, and what its trigger brought meanwhile, a command, its events or a view, comes once it activates. Refused, the plugin runs without them, its requests that need them `denied`. A new version asks for its new capabilities alone. `net:<host>` and `process-spawn` are withheld before 1.0.0, never asked for.
  - Decisions are stored in `~/.config/lokyr/permissions.toml`, beside the config file, `[plugins."owner/name"]` of `granted` and `refused`, which can be versioned with dotfiles and edited at any time: the app watches it, and a capability that it no longer grants stops a plugin that held it, until it is turned off and on again or the app's next start, as the notice of the plugins that stopped tells; one taken out of both lists is asked for again.
  - Each grant, refusal and revocation goes to the audit log, `audit.log` in the state's folder, a line each of the time, the plugin, its version and the capabilities.
- **Enforcement**:
  - Every host function exists in every plugin. Without the capability it returns a `denied` error, so a plugin can treat a capability as optional and request it at the moment it's needed.
  - The VM itself reaches nothing outside the computation: it has no `io`, `os` or `debug` library, and its `require` stays inside the package. Files, processes and the network exist only as host functions, limited to the granted paths and hosts.
- **The app as the plugins' gateway to the network**, after 1.0.0, in F6 ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)); until then `net:<host>` and `process-spawn` are refused with a notice:
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

- **Installed plugins**: the app reads `plugins.lock`, beside the config file, and the store as it starts, and adds each plugin of Luau of its package, whose digest it checks, or of the folder that `sleip dev` links, of no digest, each on a thread of its own; a package changed since it was stored, a manifest that does not read or that names another plugin, and a plugin whose `api` does not take the app's plugin API, which `lokyr --api-version` and the bundle's `api-version` tell, are refused, as a notice tells.
- **Lazy activation** through `activation.on`, the app's start of none: `startup`; `command:<glob>`, of `*` for any text, as the first run of a command of the pattern, or of the manifest's commands, which show in the menus and the palette before, of their titles and categories, activates the plugin, then runs the command, after its activation on its thread; `event:<name>`, as an event of the name comes, which the plugin hears as its activation subscribes to it; `slot:<name>`, as a view of the slot first shows, which the plugin fills as it waits. `osc:<number>` comes with the OSC handlers, in F5: a plugin of it is refused before, as a notice tells. A menu's question of whether a command applies activates nothing. A plugin that waits makes no VM.
- **Zero cost without third-party plugins**: no Luau VM exists until the first one activates.
- **Backoff**: after 3 failures within a minute, the plugin stays disabled until the user re-enables it.
- **Hot reload**: the app watches `plugins.lock`, the store and the folder of each plugin that `sleip dev` links, and, once a burst of changes settles, a quarter of a second after the last, takes the lockfile again, without IPC ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)): a plugin installed comes and activates at its trigger, the app's start's at once, without a restart; one removed goes, as `enabled = false` stops it; one updated, of another version, digest or source, or whose linked folder changed, is made anew, a fresh VM, nothing compiled ahead; each package's digest is checked again, so that one changed in the store is refused; a plugin refused is told once while it stays so. The watches wake nothing while nothing changes.

## Limits and isolation

- **One VM per plugin**: plugins share no Lua state, so one plugin cannot see or change another's globals.
- **Memory**: a limit per VM, 64 MB, fixed in 1.0.0, of no key of the config; a key, if one is asked for, in F6. An allocation over the limit raises a memory error, which ends the call.
- **CPU**:
  - A deadline per call type: 16 ms for `render` and `applies`, 250 ms for commands, events and a view's events, a second for the activation, the entry's run included. Past it, the call fails and the plugin faults.
  - A watchdog thread installs Luau's interrupt only once the deadline has passed, so the check costs nothing until then. The interrupt then ends the call at the next loop iteration or function call, native code included, and again at each of them until the call ends, so that a `pcall` that catches the error ends at its own next loop or call. The time that the plugin's code waits for the host's answers, as a request of a plugin's thread waits for the main thread, is not the plugin's: the clock stops meanwhile. A single call of the libraries runs to its end, as Luau's table library checks no interrupt: `table.sort` of a table of millions of numbers runs some hundreds of milliseconds past a render's deadline, and the plugin faults after it.
  - The watchdog, a thread of each VM's, runs **only while a plugin is executing**: it sleeps on a condition variable otherwise, so there are no wakeups when idle.
  - The watchdog writes the interrupt from another thread while the code reads it, as Luau's own command line writes it from a signal's handler at a Control-C: one aligned pointer, which a 64-bit CPU stores whole, of no patch to the vendored Luau.
- **Failures**: a failing plugin never affects the terminal. The plugin is disabled and the user gets a notification; a fault, a panic, a deadline passed or the memory's limit, of a plugin that the host can make anew, a built-in one of its registration or an installed one of its package, a fresh VM of it, starts it again after a second, two at the next fault, and the third within a minute turns it off until the app's next start, as the notification tells; what it left in the panes and the app goes, its decorations, its gutter's marks, the cursors that it held, its panels, its searches and its reads, its views showing as they were, taking no event. In native plugins, `catch_unwind` at the boundary stops panics; a panic of the host's own, as it answers a request of a plugin's thread, disables that plugin and leaves the app running.
- **Bounded queues**: what a plugin's calls leave for later is bounded, so that a plugin cannot grow the app's memory without end:
  - a call's writes and its requests of the app's, which apply as it returns, 10 000 at most together, past which another fails;
  - its reads of rows, each on a thread of the host's, four under way at most;
  - the searches that a call starts, each on a thread of the host's, 16 at most, a pane's ending the one before; a look of `applies` starts and ends none, and a call that fails ends those that it started;
  - a plugin's timers, 64 at most, past which another fails until one fires or is cancelled, of a delay of 10 ms at least, which a shorter one takes, so that a timer wakes the app 100 times a second at most, and a day at most; a timer that fired many times before its plugin heard of it owes it one event;
  - the calls that wait for a plugin's thread while it makes one, its commands, renders and its views' events, 256 at most, and 10 000 events: past them, its thread, which took none of them, is taken for stuck, and its plugin disabled, as a notice tells;
  - a chain of commands that run commands, 16 deep and 256 runs at most;
  - a plugin's commands, keys and menu items, 4096 at most together; a key or an item that it declared already changes nothing, so that the keymap and the menus are not made anew, and `keys-changed` comes as the keymap changes alone.
- **Numbers**: a request's numbers are finite, as JSON holds them, its values', its arguments', a window's point, a share, a size and a fade: a NaN or an infinity is refused, as `wrong-kind`; a text's points, from 4 to 200, go to a quarter of one, and the interface keeps 32 faces of other sizes at most, as a plugin's text of a new size at each frame would keep one each.
- **Hostile plugins in CI**: plugins of Luau of a third party's manifest run on the plugins' host against every build, `lokyr-kernel`'s test `hostile`, each stopped as its limit says, what it asked gone, and a plugin beside it running its command and hearing its events: a loop, faulted at its deadline, on the main thread and on a thread of its own, which the main thread never waits for; an allocation and a string that doubles, faulted at the memory's limit; a table a hundred thousand levels deep and one that holds itself, refused as they convert; a flood of writes, refused past a call's 10 000, then faulted at the deadline; a capability not granted, refused; a `require` out of its package, which stops the activation; and a runaway recursion, whose call fails of a stack overflow. With the escape probes of [F0-22](tasks/F0-foundations.md#f0-22--spike-lua-plugin-runtimes), so the sandbox stays closed.

## Native code and the hardened runtime

- **Compilation**: when a plugin loads, Luau compiles its sources to bytecode, and its code generator turns the plugin's functions into native code. F4 measures this against the [activation budget](PERFORMANCE.md#budgets). If needed, the host caches the bytecode it compiled itself, keyed by the source hash and the Luau version, in a directory with 0700 permissions.
- **Sources only**: packages carry Luau sources, never bytecode. The host compiles every script itself, loading each file as text alone, so that a file of bytecode fails to compile rather than load as bytecode.
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
| `keymap-macos` | Default macOS shortcuts, as iTerm2's: the keys of the app's commands and of the other plugins', which bind none of their own | F3 |
| `keymap-linux` | Default Linux shortcuts | F7 |
| `command-palette` | Command palette and theme picker | F3 |
| `plugin-manager` | UI for Sleip (calls `sleip --json`) | F4 |
| `blocks` | Command + output blocks, over the prompts' marks that the kernel keeps since F3 | F5a |
| `quick-terminal` | The quick terminal: its command, which shows and hides its quick window, its autohide and its place, of `[plugins.quick-terminal]`, through the quick windows and the global keys of `global-hotkey` | F3 |
| `progress` | The programs' progress (OSC 9;4), in the `window-strip` slot; the tab shows it with `tabs` | F3 |
| `command-header` | The command whose output the view shows, at the pane's top, a pill of its line on a hairline across the pane, in an overlay of the pane's top, over OSC 133's marks, as iTerm2's | F3 |
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
| `awake` | The Mac kept awake, always or while programs run | F4 |

### `snap`

Similar to VS Code's [CodeSnap](https://marketplace.visualstudio.com/items?itemName=adpyke.codesnap):

- **Commands**: `snap.selection`, the selection; `snap.screen`, the visible screen; and `snap.output`, a command with its output, in F4, of `history-read`, as shell integration marks the commands and their lines. Each opens a preview in a panel over the window, the image as it will be, with a shutter and toggles of the frame's parts, which save the image or copy it.
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

### `awake`

Menu beside the window's buttons:

- **A button** beside the window's buttons, of the state's symbol, which opens a menu of three states, the current one ticked: **Default**, the system's rules of sleep; **Stay awake**, the system and the display kept awake; **Sleep after jobs**, kept awake while a tab runs a program, as its marks tell, and let sleep once the last one ends. Commands set each state, for the palette and the keys.
- **Capabilities**: `keep-awake`, new, of a high risk, as a display kept awake keeps the screen from locking, and of a battery's cost: a request that keeps the system, or the system and the display, from idle sleep, which the host holds as a power assertion of the platform's, IOKit's on macOS, and releases as the plugin asks, ends, is turned off or fails, so that no plugin leaves the Mac awake past its life; and `process-read`, of the tabs' `running`, which Sleep after jobs reads.
- **What the API needs for it**: the request and its capability, of an ADR; a slot of small buttons in the titlebar, beside the window's buttons, which no slot has yet; the tabs' marks of a running program, which `TabInfo` gives, and their changes as events, of no polling.

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
command-palette = { path = "../plugins/command-palette" }
plugin-manager  = { path = "../plugins/plugin-manager" }   # from F4
```

- **Generation**: `cargo xtask gen-dist` reads every `dist/*.toml`, and writes, between its markers, each distribution as a feature of `lokyr-app-core` and of `lokyr-app`, `dist-<name>`, of a feature of each of its plugins, `plugin-<name>`, which takes the plugin's crate, a dependency of `lokyr-app-core` that is optional; and `crates/app-core/src/dist.rs`, the build's plugins' registrations, `export_plugin!`'s, in the files' order, `default` first. The default distribution is the crates' default; `just check-dist` checks that what it would write is written, and `just build-minimal` builds the minimal one, as CI does.
- **Minimal distribution**: `dist/minimal.toml` contains only `keymap-macos` and `core-actions`, Alacritty-style. It proves the terminal works without any of the "visible" features: `cargo xtask bundle --no-default-features --features dist-minimal` builds its app.
- **The keys**: the system's keymap plugin, `keymap-macos` on the Mac, binds the default keys of every command, the app's and the other first-party plugins', which bind none of their own; it comes first, so that a plugin's binding after it wins, and the user's `[keys]` after all of them. A binding of a command that no plugin registers, as one of a plugin turned off, takes no key; with the keymap off, the menus keep the system's own keys alone, ⌘H, ⌥⌘H, ⌘M and ⌃⌘Space.
- **At runtime**:
  - `[plugins.<namespace>] enabled = false` turns off a first-party plugin, at once as the config is saved, and `true` turns it on again, a new one of it ([ADR 0033](adr/0033-plugins-turned-off-in-their-sections.md)): its commands, its keys, its views, its panels and its decorations go as it stops.
  - For single-provider slots (`tab-bar`, `pane-header`), the config's [`[slots]`](CONFIGURATION.md#slots) picks the provider, the first that fills it by default, so a third-party plugin can replace our tab bar.

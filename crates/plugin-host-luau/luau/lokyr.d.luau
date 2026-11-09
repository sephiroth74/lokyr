-- lokyr.d.luau: the types of Lokyr's plugin API, for Luau's type checker.
-- Made by `cargo xtask luau-types` of plugin-sdk's types: change those, not
-- this file.
--
-- A call of `cx` answers with its value, `true` of a write, or `nil`, the
-- error's message and its code, whose hundreds tell its category: 1xx
-- usage, 2xx failure, 3xx security, 4xx fault.

-- A plain value, as an event's, a state's or a config's.
export type Data = nil | boolean | number | string | { Data } | { [string]: Data }

-- A command's argument, and its arguments by their names.
export type Value = string | number | boolean
export type Args = { [string]: Value }

-- A capability, as a manifest writes it: "pty-write", or "net:example.com".
export type Capability = string

-- A failed call's error: its message, then its code.
export type Error = string

export type Cx = {
    register_command: (command: Command) -> (true?, string?, number?),
    bind: (binding: Binding) -> (true?, string?, number?),
    subscribe: (names: { string }) -> (true?, string?, number?),
    emit: (name: string, schema: string, value: Data) -> (true?, string?, number?),
    commands: () -> ({ CommandInfo }?, string?, number?),
    themes: () -> (Themes?, string?, number?),
    preview_theme: (name: string?) -> (true?, string?, number?),
    set_theme: (variant: ThemeVariant, name: string) -> (true?, string?, number?),
    add_menu_item: (item: MenuItem) -> (true?, string?, number?),
    run_command: (name: string, args: { [string]: Value }?) -> (true?, string?, number?),
    clipboard_text: () -> (string?, string?, number?),
    set_clipboard_text: (text: string) -> (true?, string?, number?),
    focused_window: () -> (WindowId?, string?, number?),
    focused_pane: () -> (PaneId?, string?, number?),
    selection_text: (pane: PaneId) -> (string?, string?, number?),
    line_text: (pane: PaneId, row: number, reach: number) -> (LineText?, string?, number?),
    link_at: (pane: PaneId, column: number, row: number) -> (string?, string?, number?),
    working_directory: (pane: PaneId) -> (string?, string?, number?),
    read_lines: (pane: PaneId, first: number, count: number) -> (number?, string?, number?),
    search: (pane: PaneId, query: SearchQuery) -> (number?, string?, number?),
    end_search: (search: number) -> (true?, string?, number?),
    select_cells: (pane: PaneId, first: { number }, last: { number }, kind: SelectionKind?) -> (true?, string?, number?),
    deselect: (pane: PaneId) -> (true?, string?, number?),
    show_cursor: (pane: PaneId, at: { number }?) -> (true?, string?, number?),
    cursor_cell: (pane: PaneId) -> ({ number }?, string?, number?),
    show_matches: (search: number, all: boolean, current: SearchMatch?) -> (true?, string?, number?),
    search_matches: (search: number, first: number, count: number) -> (Found?, string?, number?),
    current_match: (search: number) -> (Current?, string?, number?),
    shell_commands: (pane: PaneId, top: number, bottom: number) -> ({ ShellCommand }?, string?, number?),
    command_line: (pane: PaneId, command: number) -> (string?, string?, number?),
    viewport: (pane: PaneId) -> (Viewport?, string?, number?),
    styles: (pane: PaneId, row: number) -> ({ StyleRun }?, string?, number?),
    decorate: (pane: PaneId, from: { number }, to: { number }, look: DecorationLook) -> (number?, string?, number?),
    undecorate: (pane: PaneId, id: number) -> (true?, string?, number?),
    mark_gutter: (pane: PaneId, row: number, color: { number }) -> (number?, string?, number?),
    unmark_gutter: (pane: PaneId, id: number) -> (true?, string?, number?),
    open: (target: string) -> (true?, string?, number?),
    opens: (target: string) -> (boolean?, string?, number?),
    select_all: (pane: PaneId) -> (true?, string?, number?),
    clear_scrollback: (pane: PaneId) -> (true?, string?, number?),
    clear: (pane: PaneId) -> (true?, string?, number?),
    paste: (pane: PaneId, text: string) -> (true?, string?, number?),
    send_text: (pane: PaneId, text: string) -> (true?, string?, number?),
    scroll: (pane: PaneId, scroll: Scroll) -> (true?, string?, number?),
    font_size: () -> (number?, string?, number?),
    set_font_size: (size: number?) -> (true?, string?, number?),
    open_window: (directory: string?) -> (true?, string?, number?),
    open_quick_window: (place: QuickPlace, directory: string?) -> (WindowId?, string?, number?),
    show_quick_window: (window: WindowId, place: QuickPlace?) -> (true?, string?, number?),
    close_window: (window: WindowId) -> (true?, string?, number?),
    quit: () -> (true?, string?, number?),
    windows: () -> ({ WindowInfo }?, string?, number?),
    tabs: (window: WindowId) -> ({ TabInfo }?, string?, number?),
    layout: (tab: TabId) -> (Layout?, string?, number?),
    new_tab: (window: WindowId, index: number?, directory: string?) -> ({ tab: TabId, pane: PaneId }?, string?, number?),
    split: (pane: PaneId, direction: Direction, side: Side) -> (PaneId?, string?, number?),
    close_tab: (tab: TabId, ask: boolean?) -> (true?, string?, number?),
    close_pane: (pane: PaneId, ask: boolean?) -> (true?, string?, number?),
    set_tab_title: (tab: TabId, title: string?) -> (true?, string?, number?),
    set_tab_color: (tab: TabId, color: Color?) -> (true?, string?, number?),
    set_tab_bar_place: (place: BarPlace) -> (true?, string?, number?),
    set_inactive_dim: (dim: number) -> (true?, string?, number?),
    set_focus_follows_mouse: (on: boolean) -> (true?, string?, number?),
    set_window_tab_bar_place: (window: WindowId, place: BarPlace?) -> (true?, string?, number?),
    tab_bar_place: () -> (BarPlace?, string?, number?),
    keys_of: (command: string, args: { [string]: Value }?) -> (string?, string?, number?),
    message: (id: string, args: { [string]: Data }?) -> (string?, string?, number?),
    ask_text: (window: WindowId?, message: string, detail: string, text: string) -> (number?, string?, number?),
    activate_tab: (tab: TabId) -> (true?, string?, number?),
    focus_pane: (pane: PaneId) -> (true?, string?, number?),
    move_tab: (tab: TabId, window: WindowId, index: number) -> (true?, string?, number?),
    tab_into_pane: (tab: TabId, pane: PaneId, edge: Edge) -> (true?, string?, number?),
    tab_to_new_window: (tab: TabId, at: ScreenPoint?) -> (true?, string?, number?),
    pane_to_new_tab: (pane: PaneId, window: WindowId, index: number) -> (true?, string?, number?),
    pane_to_new_window: (pane: PaneId) -> (true?, string?, number?),
    resize_pane: (pane: PaneId, edge: Edge, by: number) -> (true?, string?, number?),
    set_ratio: (tab: TabId, path: { Side }, ratio: number?) -> (true?, string?, number?),
    equalize: (tab: TabId) -> (true?, string?, number?),
    pane_grid: (pane: PaneId) -> ({ columns: number, rows: number }?, string?, number?),
    pane_title: (pane: PaneId) -> (string?, string?, number?),
    swap_panes: (pane: PaneId, with: PaneId) -> (true?, string?, number?),
    zoom: (tab: TabId, pane: PaneId?) -> (true?, string?, number?),
    request_render: (view: ViewId) -> (true?, string?, number?),
    set_text: (view: ViewId, key: Key, text: string, revision: number) -> (true?, string?, number?),
    focus_node: (view: ViewId, key: Key) -> (true?, string?, number?),
    select: (view: ViewId, key: Key, item: number) -> (true?, string?, number?),
    reveal: (view: ViewId, key: Key) -> (true?, string?, number?),
    open_panel: (id: PanelId, options: PanelOptions) -> (true?, string?, number?),
    close_panel: (panel: PanelId) -> (true?, string?, number?),
    config: () -> (Data?, string?, number?),
    state: () -> (Data?, string?, number?),
    set_state: (state: Data) -> (true?, string?, number?),
    start_timer: (delay: number, repeats: boolean) -> (number?, string?, number?),
    cancel_timer: (timer: number) -> (true?, string?, number?),
}

export type Event =
    | { kind: "window-opened", window: WindowId }
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
    | { kind: "tab-changed", tab: TabId }
    | { kind: "pane-changed", pane: PaneId, tab: TabId }
    | { kind: "pointer", pane: PaneId, at: { number }?, mods: Mods }
    | { kind: "lines", id: number, pane: PaneId, lines: { Line }, complete: boolean }
    | { kind: "searched", id: number, pane: PaneId, matches: { SearchMatch }, count: number, done: boolean }
    | { kind: "panel-closed", id: PanelId }
    | { kind: "keys-changed" }
    | { kind: "window-focus", window: WindowId, focused: boolean }
    | { kind: "viewport-changed", pane: PaneId, viewport: Viewport }
    | { kind: "command-started", pane: PaneId, command: number }
    | { kind: "command-ended", pane: PaneId, command: number, status: number? }
    | { kind: "tab-bar-resized", width: number }
    | { kind: "answered", id: number, text: string? }
    | { kind: "config-changed" }
    | { kind: "timer", id: number }
    | { kind: "custom", name: string, schema: string, value: Data }

export type UiEvent =
    | { view: ViewId, key: string, kind: "click" }
    | { view: ViewId, key: string, kind: "double-click" }
    | { view: ViewId, key: string, kind: "middle-click" }
    | { view: ViewId, key: string, kind: "context-menu" }
    | { view: ViewId, key: string, kind: "hover-enter" }
    | { view: ViewId, key: string, kind: "hover-leave" }
    | { view: ViewId, key: string, kind: "drag-start" }
    | { view: ViewId, key: string, kind: "drag-move", x: number, y: number }
    | { view: ViewId, key: string, kind: "drag-end", x: number, y: number }
    | { view: ViewId, key: string, kind: "drag-cancel" }
    | { view: ViewId, key: string, kind: "reorder", index: number }
    | { view: ViewId, key: string, kind: "drop-on-bar", window: WindowId, index: number }
    | { view: ViewId, key: string, kind: "drop-on-pane", pane: PaneId, edge: Edge }
    | { view: ViewId, key: string, kind: "drop-outside", x: number, y: number }
    | { view: ViewId, key: string, kind: "scroll", x: number, y: number }
    | { view: ViewId, key: string, kind: "focus" }
    | { view: ViewId, key: string, kind: "blur" }
    | { view: ViewId, key: string, kind: "set-value", value: string }
    | { view: ViewId, key: string, kind: "input-changed", text: string, revision: number }
    | { view: ViewId, key: string, kind: "submit", text: string, shift: boolean }
    | { view: ViewId, key: string, kind: "toggle", on: boolean }
    | { view: ViewId, key: string, kind: "select", item: number }
    | { view: ViewId, key: string, kind: "activate", item: number }
    | { view: ViewId, key: string, kind: "list-range", first: number, count: number }

export type Node = {
    kind: "container" | "text" | "icon" | "spacer" | "field" | "button" | "toggle" | "separator" | "list" | "scroll" | "progress" | "tabs",
    key: Key?,
    layout: NodeLayout?,
    style: Style?,
    focusable: boolean?,
    reorderable: boolean?,
    carries: TabId?,
    tooltip: string?,
    label: string?,
    role: NodeRole?,
    children: { Node }?,
    text: string?,
    icon: string?,
    placeholder: string?,
    disabled: boolean?,
    on: boolean?,
    row: Node?,
    count: number?,
    first: number?,
    items: { ListItem }?,
    row_height: number?,
    menu: boolean?,
    progress: Progress?,
    tabs: { Tab }?,
    chosen: number?,
    distribution: Distribution?,
}
export type View = Node

-- A plugin's table of callbacks, which its entry returns.
export type Plugin = {
    activate: ((cx: Cx) -> ())?,
    on_command: ((cx: Cx, name: string, args: Args) -> ())?,
    applies: ((cx: Cx, name: string, args: Args) -> boolean)?,
    on_event: ((cx: Cx, events: { Event }) -> ())?,
    render: ((cx: Cx, view: ViewId) -> Node?)?,
    on_ui_event: ((cx: Cx, event: UiEvent) -> ())?,
}

-- The host of a plugin's tests, which `sleip test` gives each test of
-- `tests/*.luau`: the plugin made anew, on a host of the tests, of a pane
-- that has the focus. A call that the plugin fails raises its error, which
-- fails the test.
export type TestHost = {
    -- Activates the plugin.
    activate: () -> (),
    -- Runs its command `name`, of `args`.
    command: (name: string, args: Args?) -> (),
    -- Whether its command `name`, of `args`, applies, as a menu asks.
    applies: (name: string, args: Args?) -> boolean,
    -- Gives it `events`, as the app posts them.
    events: (events: { Event }) -> (),
    -- Gives it the event of a node of one of its views.
    ui_event: (event: UiEvent) -> (),
    -- Its tree of `view`, as it renders it, if it renders one.
    render: (view: ViewId) -> Node?,
    -- The ids of its panels that show, in their order.
    panels: () -> { PanelId },
    -- Moves the host's time on by `milliseconds`, its timers' events given.
    advance: (milliseconds: number) -> (),
    -- The writes that its calls made since the last look, each a table of
    -- its request's name, as `{ Notify = { ... } }`.
    done: () -> { { [string]: any } },
    -- The names of the commands that it registered.
    commands: () -> { string },
    -- The clipboard's text.
    clipboard: () -> string?,
    -- Sets the clipboard's text.
    set_clipboard: (text: string?) -> (),
    -- Gives its message `id` the text `text`, or takes it away.
    set_message: (id: string, text: string?) -> (),
    -- Its state, as it last wrote it.
    state: () -> Data,
    -- Sets its section of the config.
    set_config: (config: Data) -> (),
}

export type AgentState =
    | "working"
    | "waiting"
    | "done"

export type Align =
    | "start"
    | "center"
    | "end"
    | "stretch"

export type Anchor =
    | { cell: { pane: PaneId, column: number, row: number } }
    | { cursor: PaneId }
    | { pointer: WindowId }
    | { node: { view: ViewId, key: Key } }
    | { pane_corner: { pane: PaneId, corner: Corner } }
    | { pane_top: PaneId }
    | { window_corner: { window: WindowId, corner: Corner } }

export type BarPlace =
    | "titlebar"
    | "below-titlebar"
    | "bottom"
    | { left: { width: number } }
    | { right: { width: number } }
    | { sidebar: { width: number } }
    | "title"

export type Binding = {
    keys: string,
    command: string,
    args: { [string]: Value }?,
    when: string?,
}

export type Border = {
    color: Token,
    width: number,
}

export type Category =
    | "app"
    | "windows"
    | "tabs"
    | "panes"
    | "input"
    | "clipboard"
    | "edit"
    | "find"
    | "view"

export type Color =
    | { palette: number }
    | { rgb: { number } }

export type Command = {
    name: string,
    title: string,
    category: Category,
    capabilities: { Capability }?,
    params: { Param }?,
    when: string?,
}

export type CommandInfo = {
    name: string,
    title: string,
    category: Category,
    params: { Param },
    plugin: string?,
    keys: string?,
    when: string?,
    enabled: boolean?,
}

export type Corner =
    | "top-left"
    | "top-right"
    | "bottom-left"
    | "bottom-right"
    | "top"

export type Current = {
    current: { any }?,
    kept: number,
}

export type DecorationLook = {
    underline: Underline?,
    underline_color: { number }?,
    background: { number }?,
    pointing: boolean?,
}

export type Direction =
    | "horizontal"
    | "vertical"

export type Distribution =
    | "fit"
    | "fill"
    | "fill-equally"
    | "fill-proportionally"

export type Easing =
    | "linear"
    | "ease-in"
    | "ease-out"
    | "ease-in-out"

export type Edge =
    | "left"
    | "right"
    | "top"
    | "bottom"

export type Edges = {
    top: number?,
    right: number?,
    bottom: number?,
    left: number?,
}

export type FontFamily =
    | "interface"
    | "terminal"

export type FontWeight =
    | "light"
    | "regular"
    | "medium"
    | "semibold"
    | "bold"
    | "heavy"

export type Found = {
    matches: { SearchMatch },
    count: number,
    done: boolean,
}

export type Hover = {
    background: Token?,
    color: Token?,
    border: Border?,
    opacity: number?,
}

export type Justify =
    | "start"
    | "center"
    | "end"
    | "space-between"

export type Key = string

export type Layout =
    | { pane: PaneId }
    | { split: { direction: Direction, ratio: number?, first: Layout, second: Layout } }

export type Length =
    | "auto"
    | { points: number }
    | { fraction: number }
    | "titlebar"
    | "buttons"

export type Line = {
    row: number,
    text: string,
    wrapped: boolean,
}

export type LineText = {
    text: string,
    cells: { { number } },
}

export type ListItem = {
    texts: { string }?,
}

export type MenuItem = {
    place: MenuPlace,
    command: string,
    args: { [string]: Value }?,
    title: string?,
}

export type MenuPlace =
    | { bar: Category }
    | "pane-context"

export type Mods = {
    shift: boolean,
    control: boolean,
    option: boolean,
    command: boolean,
}

export type NodeDirection =
    | "row"
    | "column"

export type NodeLayout = {
    direction: NodeDirection?,
    gap: number?,
    padding: Edges?,
    align: Align?,
    justify: Justify?,
    width: Length?,
    height: Length?,
    min_width: Length?,
    grow: number?,
}

export type NodeRole =
    | "group"
    | "toolbar"
    | "status"
    | "alert"
    | "heading"
    | "image"

export type PaneId = number

export type PanelId = string

export type PanelOptions = {
    anchor: Anchor,
    role: PanelRole?,
    close_outside: boolean?,
    close_escape: boolean?,
    takes_keys: boolean?,
}

export type PanelRole =
    | "tooltip"
    | "menu"
    | "dialog"
    | "overlay"

export type Param = {
    name: string,
    kind: ParamKind,
    default: Value?,
}

export type ParamKind =
    | "string"
    | "integer"
    | "number"
    | "boolean"

export type PointerShape =
    | "arrow"
    | "text"
    | "pointing-hand"

export type Progress = {
    state: ProgressState?,
    value: number?,
    shape: ProgressShape?,
    radius: number?,
    thickness: number?,
    segment: number?,
    period: number?,
}

export type ProgressShape =
    | "bar"
    | "ring"
    | "outline"
    | "dots"

export type ProgressState =
    | "normal"
    | "error"
    | "paused"
    | "indeterminate"

export type QuickEdge =
    | "top"
    | "bottom"
    | "left"
    | "right"
    | "center"

export type QuickLength =
    | { percent: number }
    | { points: number }

export type QuickPlace = {
    edge: QuickEdge,
    primary: QuickLength,
    secondary: QuickLength?,
    screen: QuickScreen?,
    animation: number,
    every_space: boolean,
}

export type QuickScreen =
    | "main"
    | "pointer"
    | "menu-bar"

export type ScreenPoint = {
    x: number,
    y: number,
}

export type Scroll =
    | { up: number }
    | { down: number }
    | "page-up"
    | "page-down"
    | "top"
    | "bottom"
    | "previous-prompt"
    | "next-prompt"
    | { row: number }

export type SearchCase =
    | "smart"
    | "sensitive"
    | "insensitive"

export type SearchMatch = {
    first: { number },
    last: { number },
}

export type SearchQuery = {
    text: string,
    regex: boolean?,
    case: SearchCase?,
    whole_word: boolean?,
}

export type SelectionKind =
    | "cells"
    | "words"
    | "lines"
    | "block"

export type Shadow = {
    color: Token?,
    x: number?,
    y: number?,
    blur: number?,
}

export type ShellCommand = {
    id: number,
    prompt: { number }?,
    input: { number }?,
    output: { number }?,
    ["end"]: { number }?,
    status: number?,
}

export type Side =
    | "first"
    | "second"

export type Style = {
    background: Token?,
    border: Border?,
    radius: number?,
    shadow: Shadow?,
    color: Token?,
    clip: boolean?,
    hides_indicator: boolean?,
    holds_ring: boolean?,
    hides_ring: boolean?,
    text_size: TextSize?,
    font_size: number?,
    font_weight: FontWeight?,
    font_family: FontFamily?,
    pointer: PointerShape?,
    opacity: number?,
    transition: Transition?,
    hover: Hover?,
    parent_hover: Hover?,
    pressed: Hover?,
}

export type StyleRun = {
    first: number,
    last: number,
    style: TextStyle,
}

export type Tab = {
    label: string?,
    icon: string?,
    disabled: boolean?,
}

export type TabId = number

export type TabInfo = {
    id: TabId,
    window: WindowId,
    title: string,
    color: Color?,
    active: boolean,
    panes: { PaneId },
    focused: PaneId,
    zoomed: PaneId?,
    activity: boolean?,
    bell: boolean?,
    progress: TabProgress?,
    printing: boolean?,
    running: boolean?,
    alternate: boolean?,
    secure: boolean?,
    agent: AgentState?,
}

export type TabProgress = {
    state: ProgressState,
    value: number?,
}

export type TextSize =
    | "regular"
    | "small"
    | "large"

export type TextStyle = {
    foreground: Color?,
    background: Color?,
    underline: Underline?,
    underline_color: Color?,
    bold: boolean,
    faint: boolean,
    italic: boolean,
    blink: boolean,
    inverse: boolean,
    invisible: boolean,
    strikethrough: boolean,
    overline: boolean,
}

export type ThemeInfo = {
    name: string,
    variant: ThemeVariant,
    origin: ThemeOrigin,
}

export type ThemeOrigin =
    | "folder"
    | "held"
    | "built-in"

export type ThemeVariant =
    | "light"
    | "dark"

export type Themes = {
    appearance: ThemeVariant,
    current: string,
    themes: { ThemeInfo },
}

export type Token = string

export type Transition = {
    duration: number?,
    easing: Easing?,
}

export type Underline =
    | "single"
    | "double"
    | "curly"
    | "dotted"
    | "dashed"

export type ViewId =
    | { tab_bar: WindowId }
    | { window_strip: WindowId }
    | { status_bar: WindowId }
    | { pane_header: PaneId }
    | { panel: PanelId }

export type Viewport = {
    top: number,
    rows: number,
    screen: number,
    oldest: number,
    epoch: number,
    alternate: boolean,
}

export type WindowId = number

export type WindowInfo = {
    id: WindowId,
    tabs: { TabId },
    active: TabId,
    number: number?,
    keys: string?,
    bell: boolean?,
    quick: boolean?,
    hidden: boolean?,
}

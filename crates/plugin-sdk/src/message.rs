//! The plugin API as messages: the requests of a plugin to the host, each
//! with the plugin that asks, the host's responses, and the events that the
//! host tells. They are plain data, which serde carries, so that a plugin
//! on the main thread, one on a thread of its own, a Luau VM, and the IPC's
//! and MCP's clients reach the host through the same messages.
//!
//! The host answers a read with its state as it handles the request, and
//! queues a write, which it applies, in the order of the plugin's writes,
//! as the plugin's call returns: a plugin that reads what it just wrote, in
//! the same call, reads the state before its write. What changes the app,
//! as a window to open or a command to run, comes once the call returns
//! too, after the writes. A plugin's declarations, its commands, its keys
//! and the events that it subscribes to, take effect at once, as the host
//! checks them.
//!
//! The host gathers the events, the workspace's changes and those that
//! plugins emit, and delivers them in batches, in their order, each plugin
//! the events that it subscribed to, once a turn of the main thread: an
//! event that a call emits comes after the call returns, never within it,
//! and one that a delivery's call emits, in the next batch.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::{Args, Capability, Command, Error};
use crate::event::WorkspaceEvent;
use crate::host::{
    BarPlace, Binding, Color, CommandInfo, Current, DecorationLook, Direction, Edge, Found, Layout,
    Line, LineText, PaneId, ScreenPoint, Scroll, SearchMatch, SearchQuery, Side, StyleRun, TabId,
    TabInfo, WindowId, WindowInfo,
};
use crate::ui::{PanelId, PanelOptions, ViewId};
use crate::view::Key;

/// A request, with the plugin that asks it, by its name, as it crosses to
/// the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    /// The plugin that asks.
    pub caller: String,
    /// What it asks.
    pub request: Request,
}

/// The message of the error of [`Request::TabIntoPane`] of a pane of the
/// tab itself, which every host gives alike.
pub const OWN_PANE: &str = "a tab goes into a pane of another tab, not of its own";

/// The message of the error of [`Request::SetInactiveDim`] of a dim out of
/// its range, which every host gives alike.
pub const DIM_RANGE: &str = "a dim is a share from 0 to 1";

/// What a plugin asks of the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Request {
    /// Registers a command, whose calls come to the plugin.
    RegisterCommand(Command),
    /// Binds keys to a command, which needs not be registered yet.
    Bind(Binding),
    /// Subscribes to the events of these names: the workspace's, as
    /// `tab-created`, or plugins' own, as `search.progress`.
    Subscribe(Vec<String>),
    /// Emits an event of the plugin's own, whose name is of its namespace,
    /// as `search.progress`, to the plugins that subscribed to it, once the
    /// call returns.
    Emit {
        /// Its name, `namespace.event`.
        name: String,
        /// The id of its value's schema, as `search.progress/1`.
        schema: String,
        /// Its value.
        value: Data,
    },
    /// The registered commands, in the order of their registrations, as
    /// the menus list them.
    Commands,
    /// The themes that the app can show, the appearance in force and its
    /// theme.
    Themes,
    /// Shows the theme of a name in place of the appearance in force's,
    /// for a while, as a picker previews it, until the next preview, the
    /// theme's choice, or `None`, which shows the config's again.
    PreviewTheme(Option<String>),
    /// Names a theme in the config's `[theme]`, for the appearance of
    /// `variant`, which the app shows at once, a preview ending.
    SetTheme {
        /// The appearance whose theme it names.
        variant: crate::theme::ThemeVariant,
        /// The theme's name.
        name: String,
    },
    /// Adds a menu item of a command of its arguments, registered, to the
    /// menu bar or to the pane's context menu, for as long as the plugin
    /// runs.
    AddMenuItem(crate::host::MenuItem),
    /// Runs a command with arguments, once the call returns; the call is
    /// checked as it is asked: of a plugin of `user-commands`, as the
    /// user's, any command without a declaration.
    RunCommand {
        /// The command's name.
        name: String,
        /// Its arguments.
        args: Args,
    },
    /// The clipboard's text.
    ClipboardText,
    /// Puts text on the clipboard.
    SetClipboardText(String),
    /// The window that has the focus.
    FocusedWindow,
    /// The pane that has the focus: the active one of the focused window.
    FocusedPane,
    /// The text of a pane's selection.
    SelectionText(PaneId),
    /// The text of the logical line of a pane's row, by its number, as far
    /// as `reach` rows before it and after it, with each character's cell.
    LineText {
        /// The pane.
        pane: PaneId,
        /// The row's number.
        row: i64,
        /// The rows before it and after it that the read takes.
        reach: usize,
    },
    /// The target of the OSC 8 link at a cell of a pane, by its column and
    /// its row's number.
    LinkAt {
        /// The pane.
        pane: PaneId,
        /// The cell's column.
        column: u16,
        /// Its row's number.
        row: i64,
    },
    /// The working directory of the program in a pane's foreground.
    WorkingDirectory(PaneId),
    /// Reads the texts of a pane's rows, from the row numbered `first`,
    /// `count` at most, on a thread of the host's, a part at a time: the
    /// host answers with the read's id, and the rows come as an
    /// [`Event::Lines`] of that id, to the plugin alone.
    ReadLines {
        /// The pane.
        pane: PaneId,
        /// The first row's number.
        first: i64,
        /// How many rows, [`MOST_LINES`](crate::host::MOST_LINES) at most.
        count: usize,
    },
    /// Searches a pane's terminal, its scrollback too, for `query`, on a
    /// thread of the host's, a part of its rows at a time, from those that
    /// its view shows outward: the host answers with the search's id, and
    /// the matches come as [`Event::Searched`] events of that id, a part's
    /// each, to the plugin alone. The plugin's search of the pane before
    /// ends. The matches, kept as their cells' pins through new output, the
    /// view's scrolls and a resize's reflow,
    /// [`MOST_MATCHES`](crate::host::MOST_MATCHES) at most and the count
    /// past them, stay until the plugin's next search of the pane, its
    /// [`Request::EndSearch`], or the pane's close.
    Search {
        /// The pane.
        pane: PaneId,
        /// What it looks for.
        query: SearchQuery,
    },
    /// Ends one of the plugin's searches, at once: it stops, and its
    /// matches go.
    EndSearch(u64),
    /// Selects a pane's cells from one to another, by their columns and
    /// their rows' numbers, in reading order, as a drag across them would,
    /// which a copy then takes, and scrolls its view, through the
    /// scrollback, to show the first.
    SelectCells {
        /// The pane.
        pane: PaneId,
        /// The first cell.
        first: (u16, i64),
        /// The last cell.
        last: (u16, i64),
    },
    /// Shows one of the plugin's searches' matches over its pane's cells,
    /// in the colors of the theme's `search.match`, or hides them, as `all`
    /// says, and its current match apart, if it gives one, in those of
    /// `search.match.current`, whose row the view scrolls to: at each frame,
    /// the host draws those that the view shows, until the search ends.
    ShowMatches {
        /// The search's id.
        search: u64,
        /// Whether the view shows every match.
        all: bool,
        /// The current match, if one.
        current: Option<SearchMatch>,
    },
    /// The matches of one of the plugin's searches, in the rows' order,
    /// from the one at `first`, `count` at most, as their cells are now: a
    /// match whose cells left the scrollback is no more.
    SearchMatches {
        /// The search's id.
        search: u64,
        /// The first match's place among them, from 0.
        first: usize,
        /// How many, [`MOST_LINES`](crate::host::MOST_LINES) at most.
        count: usize,
    },
    /// The current match of one of the plugin's searches, as its cells
    /// are now, which a resize's reflow moves, and its place among the
    /// search's matches now, which the matches that leave the scrollback
    /// lower; and how many of them the host keeps.
    CurrentMatch(u64),
    /// The styles of a pane's row, by its number, as runs of cells.
    Styles {
        /// The pane.
        pane: PaneId,
        /// The row's number.
        row: i64,
    },
    /// Decorates a pane's cells, from one to another in reading order,
    /// each by its column and its row's number, with a look: the host
    /// answers with the decoration's id, and lays it as the call returns.
    /// The decoration keeps its text as output scrolls it and a resize
    /// reflows it, and goes with it.
    Decorate {
        /// The pane.
        pane: PaneId,
        /// The first cell.
        from: (u16, i64),
        /// The last cell.
        to: (u16, i64),
        /// How it looks.
        look: DecorationLook,
    },
    /// Takes away one of the plugin's decorations of a pane.
    Undecorate {
        /// The pane.
        pane: PaneId,
        /// The decoration's id.
        id: u64,
    },
    /// Lays a mark of a color in a pane's gutter, at the left of its cells,
    /// on a row by its number: the host answers with the mark's id, and
    /// lays it as the call returns. The mark keeps its row as output
    /// scrolls it and a resize reflows it, and goes with it.
    MarkGutter {
        /// The pane.
        pane: PaneId,
        /// The row's number.
        row: i64,
        /// Its color, red, green and blue.
        color: (u8, u8, u8),
    },
    /// Takes away one of the plugin's marks of a pane's gutter.
    UnmarkGutter {
        /// The pane.
        pane: PaneId,
        /// The mark's id.
        id: u64,
    },
    /// Opens a URL, or a path, absolute or from `~`, with the system's
    /// handler, as the user's config says.
    Open(String),
    /// Whether [`Request::Open`] would open the target.
    Opens(String),
    /// Selects everything of a pane: its scrollback and its screen.
    SelectAll(PaneId),
    /// Clears a pane's scrollback.
    ClearScrollback(PaneId),
    /// Clears a pane's scrollback, and its screen but the prompt.
    Clear(PaneId),
    /// Pastes text into a pane, as the user pastes.
    Paste {
        /// The pane.
        pane: PaneId,
        /// The text.
        text: String,
    },
    /// Sends text to a pane's program as it is, as typed keys would.
    SendText {
        /// The pane.
        pane: PaneId,
        /// The text.
        text: String,
    },
    /// Scrolls a pane's view.
    Scroll {
        /// The pane.
        pane: PaneId,
        /// How.
        scroll: Scroll,
    },
    /// The size of the text, in points, of every window.
    FontSize,
    /// Gives the text a size, in points, from 4 to 200; with `None`, the
    /// config's.
    SetFontSize(Option<f32>),
    /// Opens a window, its pane in `directory`, a path absolute or from
    /// `~`, or in the folder of a file there, if given; in the folder of a
    /// new window's pane otherwise.
    OpenWindow {
        /// The folder of its pane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
    },
    /// Opens a quick window of a new pane, which shows at `place` over every
    /// app's windows, as a global key shows it, and takes the focus: its id,
    /// which the host gives at once, as it opens it once the call returns.
    /// Its pane starts in `directory`, as a new tab's does, if given; in the
    /// folder of a new window's pane otherwise.
    OpenQuickWindow {
        /// Where it shows.
        place: crate::host::QuickPlace,
        /// The folder of its pane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
    },
    /// Shows a quick window at `place`, giving it the focus, or hides it, of
    /// `None`, the focus going back to the app that had it.
    ShowQuickWindow {
        /// The quick window.
        window: WindowId,
        /// Where it shows, or `None` to hide it.
        place: Option<crate::host::QuickPlace>,
    },
    /// Closes a window.
    CloseWindow(WindowId),
    /// Quits the app.
    Quit,
    /// The windows, each with its tabs and the one that it shows.
    Windows,
    /// A window's tabs, in their order.
    Tabs(WindowId),
    /// A tab's layout.
    Layout(TabId),
    /// Opens a tab in a window, at `index` of its tabs, or at their end
    /// without one or past them, of a pane whose program starts as a new
    /// pane's does, beside the window's focused pane: the host answers
    /// with the tab's id and the pane's, and opens them as the call
    /// returns. The window shows the tab that it showed. Its pane starts in
    /// `directory`, a path absolute or from `~`, or in the folder of a file
    /// there, if given.
    NewTab {
        /// The window.
        window: WindowId,
        /// The tab's place among the window's tabs, from the first.
        index: Option<usize>,
        /// The folder of its pane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
    },
    /// Splits a pane's part along `direction`, with a new pane on `side`,
    /// whose program starts as a new pane's does, beside the pane: the host
    /// answers with the new pane's id, and splits as the call returns. The
    /// tab's keyboard stays where it was, and its zoom ends.
    Split {
        /// The pane.
        pane: PaneId,
        /// How the two parts lie.
        direction: Direction,
        /// The new pane's part.
        side: Side,
    },
    /// Closes a tab, with its panes, whose programs are hung up; its window
    /// shows the tab after it, or before it, and closes with its last tab.
    /// With `ask`, the host asks the user first, as it asks for a window,
    /// while a program other than a shell runs in one of its panes, and
    /// closes it once the user confirms.
    CloseTab {
        /// The tab.
        tab: TabId,
        /// Whether the host asks the user first.
        #[serde(default)]
        ask: bool,
    },
    /// Closes a pane, whose program is hung up: its room goes to its
    /// sibling, and its tab closes with its last pane. With `ask`, the
    /// host asks the user first, as [`Request::CloseTab`] says.
    ClosePane {
        /// The pane.
        pane: PaneId,
        /// Whether the host asks the user first.
        #[serde(default)]
        ask: bool,
    },
    /// Gives a tab a title of its own, in place of its active pane's
    /// program's, which `None` gives back.
    SetTabTitle {
        /// The tab.
        tab: TabId,
        /// Its title.
        title: Option<String>,
    },
    /// Gives a tab a color of its own, an RGB one, in place of its
    /// program's, which `None` gives back.
    SetTabColor {
        /// The tab.
        tab: TabId,
        /// Its color.
        color: Option<Color>,
    },
    /// Puts the plugin's tab bar at `place` in the windows that show it, a
    /// column's width from [`COLUMN_WIDTHS`](crate::host::COLUMN_WIDTHS)'s
    /// narrowest to its widest, in
    /// place of the place before, as its config says: a plugin that fills
    /// no tab bar puts it nowhere.
    SetTabBarPlace(BarPlace),
    /// Fades the panes that have not their tab's keyboard, of a tab of more
    /// than one, toward their background by `dim`, a share from 0, none, to
    /// 1: their text's colors, their lines and their cells' own
    /// backgrounds, while the plugin runs; the most that the plugins ask
    /// for.
    SetInactiveDim(f32),
    /// Gives the keyboard to the pane of a tab that the pointer comes over,
    /// while the plugin runs, as any plugin asks it.
    SetFocusFollowsMouse(bool),
    /// Puts the plugin's tab bar of `window` at `place`, over the place of
    /// its bar in every window, as a window of a single tab shows its title
    /// in its titlebar, [`BarPlace::Title`]; `None` gives it that place
    /// again.
    SetWindowTabBarPlace {
        /// The window.
        window: WindowId,
        /// Its own place, or none.
        place: Option<BarPlace>,
    },
    /// The plugin's tab bar's place, a column's width as the user's drag
    /// of its edge left it, as its view lays its tabs out across or down.
    TabBarPlace,
    /// The keys that run `command` with `args`, one key that no later
    /// binding takes, as a menu's item would show them, written as the
    /// platform writes them, as `⌘1`: none without a binding, as a tab bar
    /// shows each tab's keys; [`Event::KeysChanged`] tells as the bindings
    /// change.
    KeysOf {
        /// The command.
        command: String,
        /// Its arguments.
        args: Args,
    },
    /// The text of `id`, a message of the plugin's catalogs, of `args`, its
    /// placeables', a text or a number each, in the language that the app
    /// shows, or English's where that one lacks it, as a view shows its
    /// texts, labels and tooltips: none of a message that the catalogs
    /// lack.
    Message {
        /// The message.
        id: String,
        /// Its arguments, by their names.
        args: BTreeMap<String, Data>,
    },
    /// Asks the user for a text, in an alert of `message` and `detail` on
    /// `window`, or on the focused window without one, in a field that
    /// holds `text` as it opens: the host answers with the question's id,
    /// and the user's answer comes as [`Event::Answered`] to the plugin
    /// alone, the text given, or none as the user cancels. The message and
    /// the detail are messages of the plugin's catalogs, which the host
    /// shows in the language shown, or the texts themselves where the
    /// catalogs lack them.
    AskText {
        /// The window.
        window: Option<WindowId>,
        /// What the alert asks, in a line, or its message's id.
        message: String,
        /// What it explains under the message, or its message's id.
        detail: String,
        /// The field's text as it opens.
        text: String,
    },
    /// Has a tab's window show it.
    ActivateTab(TabId),
    /// Gives a pane its tab's keyboard.
    FocusPane(PaneId),
    /// Moves a tab to `index` of a window's tabs, its own or another's, or
    /// to their end past them.
    MoveTab {
        /// The tab.
        tab: TabId,
        /// The window.
        window: WindowId,
        /// Its place among the window's tabs, from the first.
        index: usize,
    },
    /// Moves the panes of a tab beside a pane of another tab, at its `edge`,
    /// the tab's layout whole in that half of the pane's part, half and
    /// half, as a tab dropped on a pane: the tab goes, its programs running
    /// on, its window closing if it has none left; the pane's tab shows,
    /// its keyboard at the moved tab's pane that had it.
    TabIntoPane {
        /// The tab.
        tab: TabId,
        /// The pane of another tab.
        pane: PaneId,
        /// The pane's edge where the tab goes.
        edge: Edge,
    },
    /// Moves a tab out of its window into a window of its own, whose
    /// content's top left corner goes at `at` on the screens, as a tab
    /// dropped outside every window, or where the platform places a new
    /// window without it; a tab alone in its window stays there, its
    /// window moving to `at`.
    TabToNewWindow {
        /// The tab.
        tab: TabId,
        /// Where its window goes.
        #[serde(default)]
        at: Option<ScreenPoint>,
    },
    /// Moves a pane out of its tab into a tab of its own, at `index` of a
    /// window's tabs, or at their end past them; a pane alone in its tab
    /// moves with it.
    PaneToNewTab {
        /// The pane.
        pane: PaneId,
        /// The window.
        window: WindowId,
        /// The tab's place among the window's tabs, from the first.
        index: usize,
    },
    /// Moves a pane out of its tab into a window of its own; a pane alone
    /// in its tab moves with it.
    PaneToNewWindow(PaneId),
    /// Moves an edge of a pane by `by`, a share of the room of the split
    /// whose divider the edge is, from -1 to 1: a positive one toward the
    /// right or the bottom.
    ResizePane {
        /// The pane.
        pane: PaneId,
        /// The edge, whose split's divider moves.
        edge: Edge,
        /// How far.
        by: f64,
    },
    /// Gives a split of a tab's layout, by the parts that lead to it from
    /// the layout's top, the share of its first part, from 0 to 1, or,
    /// with `None`, its panes' share.
    SetRatio {
        /// The tab.
        tab: TabId,
        /// The parts from the top to the split, none for the top's.
        path: Vec<Side>,
        /// The first part's share.
        ratio: Option<f64>,
    },
    /// Shares each split of a tab's layout by its panes again, the ratios
    /// that the drags gave dropped.
    Equalize(TabId),
    /// The grid of a pane, its columns and its rows, as its window last
    /// laid it out, as a resize by cells reads it.
    PaneGrid(PaneId),
    /// The title of a pane, as its header shows it: its program's, or, to
    /// a plugin of `process-read`, its folder and its program; none without
    /// either, as [`Response::Text`].
    PaneTitle(PaneId),
    /// Swaps two panes of a tab's layout, each taking the other's room,
    /// their programs running on, the tab's keyboard staying with its pane.
    SwapPanes {
        /// The pane.
        pane: PaneId,
        /// The other, of the same tab.
        with: PaneId,
    },
    /// Has a pane of a tab take its whole room, or, with `None`, ends the
    /// tab's zoom.
    Zoom {
        /// The tab.
        tab: TabId,
        /// The pane.
        pane: Option<PaneId>,
    },
    /// Asks the host for a render of one of the plugin's views, once the
    /// call returns.
    RequestRender(ViewId),
    /// Sets the text of a field of one of the plugin's views, by its key,
    /// cut to the field's 10 000 characters, its caret at its end, and no
    /// `input-changed`: of the revision of the `input-changed` that the
    /// text answers, which the host sets over no keys typed since, or the
    /// field's last revision that the plugin knows.
    SetText {
        /// The view.
        view: ViewId,
        /// The field's key.
        key: Key,
        /// The text.
        text: String,
        /// The revision of the field's text that it answers.
        revision: u64,
    },
    /// Gives the keyboard's focus to a node of one of the plugin's views,
    /// by its key, which tells it `focus`: a field selects all of its
    /// text, as macOS's does as it becomes the first responder, so that
    /// typing replaces it.
    FocusNode {
        /// The view.
        view: ViewId,
        /// The node's key.
        key: Key,
    },
    /// Moves the selection of a list of one of the plugin's views, by its
    /// key, to an item, which the list scrolls to.
    Select {
        /// The view.
        view: ViewId,
        /// The list's key.
        key: Key,
        /// The item's index.
        item: usize,
    },
    /// Scrolls the nearest scroll area around a node of one of the plugin's
    /// views, by its key, as little as shows the whole node, once the view
    /// shows it, as a tab bar shows its active tab: the node of a render
    /// that the view has yet to show too.
    Reveal {
        /// The view.
        view: ViewId,
        /// The node's key.
        key: Key,
    },
    /// Opens a panel of the plugin's, of an id that none of its open panels
    /// has, `already-registered` otherwise, as the call returns, and asks
    /// for its render: over its window, at its anchor.
    OpenPanel {
        /// Its id.
        id: PanelId,
        /// Its anchor, its role and what closes it.
        options: PanelOptions,
    },
    /// Closes a panel of the plugin's, as the call returns.
    ClosePanel(PanelId),
    /// The plugin's config: the values of its section of the config file,
    /// `[plugins.<namespace>]`, as its schema checked them, with the
    /// defaults of the keys that it leaves out, as a map; an empty map of
    /// a plugin without a schema.
    Config,
    /// The plugin's state, which it kept for the app's next runs: what it
    /// last wrote, or nothing.
    State,
    /// Keeps a value as the plugin's state, 1 MiB at most as JSON, written
    /// whole as the call returns.
    SetState(Data),
}

/// What a request is to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// It reads the host's state, which it answers at once.
    Read,
    /// It changes the kernel's state, which the host applies as the call
    /// returns.
    Write,
    /// It asks the app, which acts once the call returns.
    App,
    /// It declares the plugin's commands or keys, which take effect at
    /// once.
    Declaration,
}

impl Request {
    /// The capability that the request needs, which the host checks
    /// against the plugin's, if it needs one.
    pub fn capability(&self) -> Option<Capability> {
        match self {
            Self::ClipboardText => Some(Capability::ClipboardRead),
            Self::SetClipboardText(_) => Some(Capability::ClipboardWrite),
            Self::SelectionText(_)
            | Self::LineText { .. }
            | Self::LinkAt { .. }
            | Self::ReadLines { .. }
            | Self::Search { .. }
            | Self::SearchMatches { .. }
            | Self::CurrentMatch(_)
            | Self::Styles { .. } => Some(Capability::TerminalRead),
            Self::WorkingDirectory(_) => Some(Capability::ProcessRead),
            Self::OpenQuickWindow { .. } | Self::ShowQuickWindow { .. } => {
                Some(Capability::GlobalHotkey)
            }
            // A key taken from every app.
            Self::Bind(binding) if binding.keys.starts_with(crate::keys::GLOBAL) => {
                Some(Capability::GlobalHotkey)
            }
            Self::Open(_) | Self::Opens(_) => Some(Capability::Open),
            Self::Paste { .. } | Self::SendText { .. } => Some(Capability::PtyWrite),
            Self::RegisterCommand(_)
            | Self::Bind(_)
            | Self::Subscribe(_)
            | Self::Emit { .. }
            | Self::RunCommand { .. }
            | Self::FocusedWindow
            | Self::FocusedPane
            | Self::Decorate { .. }
            | Self::Undecorate { .. }
            | Self::MarkGutter { .. }
            | Self::UnmarkGutter { .. }
            | Self::EndSearch(_)
            | Self::SelectCells { .. }
            | Self::ShowMatches { .. }
            | Self::SelectAll(_)
            | Self::ClearScrollback(_)
            | Self::Clear(_)
            | Self::Scroll { .. }
            | Self::FontSize
            | Self::SetFontSize(_)
            | Self::OpenWindow { .. }
            | Self::CloseWindow(_)
            | Self::Quit
            | Self::Windows
            | Self::Tabs(_)
            | Self::Layout(_)
            | Self::PaneGrid(_)
            | Self::PaneTitle(_)
            | Self::NewTab { .. }
            | Self::Split { .. }
            | Self::CloseTab { .. }
            | Self::ClosePane { .. }
            | Self::SetTabTitle { .. }
            | Self::SetTabColor { .. }
            | Self::AskText { .. }
            | Self::SetTabBarPlace(_)
            | Self::SetWindowTabBarPlace { .. }
            | Self::SetInactiveDim(_)
            | Self::SetFocusFollowsMouse(_)
            | Self::TabBarPlace
            | Self::KeysOf { .. }
            | Self::Message { .. }
            | Self::ActivateTab(_)
            | Self::FocusPane(_)
            | Self::MoveTab { .. }
            | Self::TabIntoPane { .. }
            | Self::TabToNewWindow { .. }
            | Self::PaneToNewTab { .. }
            | Self::PaneToNewWindow(_)
            | Self::ResizePane { .. }
            | Self::SetRatio { .. }
            | Self::Equalize(_)
            | Self::SwapPanes { .. }
            | Self::Zoom { .. }
            | Self::RequestRender(_)
            | Self::SetText { .. }
            | Self::Select { .. }
            | Self::FocusNode { .. }
            | Self::Reveal { .. }
            | Self::OpenPanel { .. }
            | Self::ClosePanel(_)
            | Self::Config
            | Self::State
            | Self::SetState(_)
            | Self::Commands
            | Self::Themes
            | Self::PreviewTheme(_)
            | Self::SetTheme { .. }
            | Self::AddMenuItem(_) => None,
        }
    }

    /// What the request is to the host: a read, a write, a request of the
    /// app's, or a declaration.
    pub fn effect(&self) -> Effect {
        match self {
            Self::ClipboardText
            | Self::FocusedWindow
            | Self::FocusedPane
            | Self::SelectionText(_)
            | Self::LineText { .. }
            | Self::LinkAt { .. }
            | Self::WorkingDirectory(_)
            | Self::ReadLines { .. }
            | Self::Search { .. }
            | Self::EndSearch(_)
            | Self::SearchMatches { .. }
            | Self::CurrentMatch(_)
            | Self::Styles { .. }
            | Self::Opens(_)
            | Self::FontSize
            | Self::Windows
            | Self::Tabs(_)
            | Self::Layout(_)
            | Self::PaneGrid(_)
            | Self::PaneTitle(_)
            | Self::Config
            | Self::State
            | Self::TabBarPlace
            | Self::KeysOf { .. }
            | Self::Message { .. }
            | Self::Commands
            | Self::Themes => Effect::Read,
            Self::SetClipboardText(_)
            | Self::SelectCells { .. }
            | Self::ShowMatches { .. }
            | Self::SelectAll(_)
            | Self::ClearScrollback(_)
            | Self::Clear(_)
            | Self::Paste { .. }
            | Self::SendText { .. }
            | Self::Scroll { .. }
            | Self::Decorate { .. }
            | Self::Undecorate { .. }
            | Self::MarkGutter { .. }
            | Self::UnmarkGutter { .. }
            | Self::Emit { .. }
            | Self::NewTab { .. }
            | Self::Split { .. }
            | Self::CloseTab { .. }
            | Self::ClosePane { .. }
            | Self::SetTabTitle { .. }
            | Self::SetTabColor { .. }
            | Self::SetTabBarPlace(_)
            | Self::SetWindowTabBarPlace { .. }
            | Self::SetInactiveDim(_)
            | Self::SetFocusFollowsMouse(_)
            | Self::ActivateTab(_)
            | Self::FocusPane(_)
            | Self::MoveTab { .. }
            | Self::TabIntoPane { .. }
            | Self::TabToNewWindow { .. }
            | Self::PaneToNewTab { .. }
            | Self::PaneToNewWindow(_)
            | Self::ResizePane { .. }
            | Self::SetRatio { .. }
            | Self::Equalize(_)
            | Self::SwapPanes { .. }
            | Self::Zoom { .. }
            | Self::RequestRender(_)
            | Self::SetState(_) => Effect::Write,
            Self::RunCommand { .. }
            | Self::Open(_)
            | Self::SetFontSize(_)
            | Self::OpenWindow { .. }
            | Self::OpenQuickWindow { .. }
            | Self::ShowQuickWindow { .. }
            | Self::CloseWindow(_)
            | Self::Quit
            | Self::SetText { .. }
            | Self::Select { .. }
            | Self::FocusNode { .. }
            | Self::Reveal { .. }
            | Self::OpenPanel { .. }
            | Self::ClosePanel(_)
            | Self::AskText { .. }
            | Self::PreviewTheme(_)
            | Self::SetTheme { .. } => Effect::App,
            Self::RegisterCommand(_)
            | Self::Bind(_)
            | Self::Subscribe(_)
            | Self::AddMenuItem(_) => Effect::Declaration,
        }
    }
}

/// The host's answer to a request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Response {
    /// A write queued, a request of the app's taken, or a declaration
    /// taken.
    Done,
    /// A window, or none.
    Window(Option<WindowId>),
    /// A pane, or none.
    Pane(Option<PaneId>),
    /// A text, or none.
    Text(Option<String>),
    /// A line's text, or none.
    Line(Option<LineText>),
    /// Whether it holds.
    Boolean(bool),
    /// The text's size, in points.
    FontSize(f32),
    /// A read's id, whose rows come as an event.
    Reading(u64),
    /// A search's id, whose matches come as events.
    Searching(u64),
    /// Matches of a search, as a read of them gives them.
    Matches(Found),
    /// A search's current match, as a read of it gives it.
    Current(Current),
    /// The registered commands.
    Commands(Vec<CommandInfo>),
    /// The themes that the app can show.
    Themes(crate::host::Themes),
    /// A question's id, whose answer comes as an event.
    Asking(u64),
    /// A tab bar's place.
    TabBarPlace(BarPlace),
    /// A row's styles, or none of a row that the pane does not hold.
    Styles(Option<Vec<StyleRun>>),
    /// A decoration's id, or a mark's of a pane's gutter.
    Decoration(u64),
    /// The windows.
    Windows(Vec<WindowInfo>),
    /// A window's tabs.
    Tabs(Vec<TabInfo>),
    /// A tab's layout.
    Layout(Layout),
    /// A pane's grid.
    Grid {
        /// Its columns.
        columns: u16,
        /// Its rows.
        rows: u16,
    },
    /// A plain value, as the plugin's config or its state.
    Data(Data),
    /// A tab that opens as the call returns, and its pane.
    Tab {
        /// The tab.
        tab: TabId,
        /// Its pane.
        pane: PaneId,
    },
    /// The request failed, and did nothing.
    Failed(Error),
}

/// The name of [`Event::Pointer`], as plugins subscribe to it.
pub const POINTER: &str = "pointer";

/// The name of [`Event::KeysChanged`], as a plugin subscribes to it.
pub const KEYS_CHANGED: &str = "keys-changed";

/// The name of [`Event::WindowFocus`], as a plugin subscribes to it.
pub const WINDOW_FOCUS: &str = "window-focus";

/// The modifiers held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mods {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// Option, or Alt.
    pub option: bool,
    /// Command, or Super.
    pub command: bool,
}

/// What the host tells a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// A change of the workspace's windows, tabs, layouts or panes.
    Workspace(WorkspaceEvent),
    /// The pointer went over another cell of a pane, or out of it, or the
    /// modifiers held changed over it, or the text under it changed: the
    /// cell, by its column and its row's number, or none out of the pane's
    /// cells. The host tells it as the pointer moves, as a modifier goes
    /// down or up with the pointer still, and again at its cell as a frame
    /// changes the text of its row or scrolls its pane's rows under it;
    /// while Command is up, it tells moves only as the system gives them,
    /// as a program that tracks the mouse asks.
    Pointer {
        /// The pane.
        pane: PaneId,
        /// The cell under the pointer, if one is.
        at: Option<(u16, i64)>,
        /// The modifiers held.
        mods: Mods,
    },
    /// The rows of a read that the plugin asked for, to it alone: all
    /// that it asked for, of those that the pane holds, unless `complete`
    /// is false, as a resize renumbered the rows, or the pane closed, in
    /// the middle.
    Lines {
        /// The read's id.
        id: u64,
        /// The pane.
        pane: PaneId,
        /// The rows, oldest first.
        lines: Vec<Line>,
        /// Whether the read went to its end.
        complete: bool,
    },
    /// Matches of a search that the plugin asked for, to it alone: a
    /// part's, of its rows, the nearest to the view first, as their cells
    /// were found; how many the search found so far, those past
    /// [`MOST_MATCHES`](crate::host::MOST_MATCHES) too; and whether it went
    /// to its end, of its last part, or of a pane that closed or a resize
    /// that renumbered its rows in the middle, as it stops.
    Searched {
        /// The search's id.
        id: u64,
        /// The pane.
        pane: PaneId,
        /// The part's matches, in its rows' order.
        matches: Vec<SearchMatch>,
        /// How many matches it found so far.
        count: usize,
        /// Whether it went to its end.
        done: bool,
    },
    /// A panel of the plugin's closed, as a click outside it, Escape, the
    /// app or the plugin closed it, or a menu shown as the platform's own
    /// closed with a choice or without one, to it alone.
    PanelClosed(PanelId),
    /// The keymap's bindings changed, as the config's `[keys]` or a
    /// plugin's: the keys of [`Request::KeysOf`] may be others.
    KeysChanged,
    /// A window took the keyboard's focus, or lost it, as the user clicks
    /// another app or another window; not as a question of the app's on it
    /// takes it.
    WindowFocus {
        /// The window.
        window: WindowId,
        /// Whether it has the focus now.
        focused: bool,
    },
    /// The user's drag of the edge of the plugin's tab bar's column ended,
    /// to it alone: the column's width, in points, in force since, which
    /// the plugin keeps, as for its next start.
    TabBarResized {
        /// The column's width.
        width: f32,
    },
    /// The user's answer to a question that the plugin asked, to it alone:
    /// the text given, or none as the user cancelled, or the question's
    /// window closed.
    Answered {
        /// The question's id.
        id: u64,
        /// The text.
        text: Option<String>,
    },
    /// The plugin's config changed, as the config file reloaded, to it
    /// alone: its request of [`Request::Config`] reads the new one.
    ConfigChanged,
    /// An event of a plugin's own.
    Custom {
        /// Its name, `namespace.event`, of the plugin's namespace.
        name: String,
        /// The id of its value's schema.
        schema: String,
        /// Its value.
        value: Data,
    },
}

impl Event {
    /// The event's name, as a plugin subscribes to it.
    pub fn name(&self) -> &str {
        match self {
            Self::Workspace(event) => event.name(),
            Self::Pointer { .. } => POINTER,
            Self::Lines { .. } => "lines",
            Self::Searched { .. } => "searched",
            Self::PanelClosed(_) => "panel-closed",
            Self::Answered { .. } => "answered",
            Self::TabBarResized { .. } => "tab-bar-resized",
            Self::KeysChanged => KEYS_CHANGED,
            Self::WindowFocus { .. } => WINDOW_FOCUS,
            Self::ConfigChanged => "config-changed",
            Self::Custom { name, .. } => name,
        }
    }
}

/// A plain value, as an event of a plugin's carries: text, numbers, true
/// or false, and lists and maps of them, as a Luau table is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Data {
    /// Nothing.
    Nil,
    /// True or false.
    Boolean(bool),
    /// A whole number.
    Integer(i64),
    /// A number.
    Number(f64),
    /// Text.
    Text(String),
    /// Values in their order.
    List(Vec<Data>),
    /// Values by their keys.
    Map(BTreeMap<String, Data>),
}

impl Data {
    /// The most levels that a value nests, of its lists and its maps, which
    /// the host takes: a deeper one it refuses, as it walks the values that
    /// it takes recursively, on the main thread.
    pub const DEEPEST: usize = 64;

    /// Whether the value nests [`Data::DEEPEST`] levels at most, which it
    /// tells without recursion, of a value of any depth.
    pub fn nests_within_deepest(&self) -> bool {
        let mut left = vec![(self, 0)];
        while let Some((value, depth)) = left.pop() {
            let inner: Box<dyn Iterator<Item = &Data>> = match value {
                Self::List(items) => Box::new(items.iter()),
                Self::Map(map) => Box::new(map.values()),
                _ => continue,
            };
            if depth == Self::DEEPEST {
                return false;
            }
            left.extend(inner.map(|item| (item, depth + 1)));
        }
        true
    }
}

/// Drops `data` a level at a time, without recursion, as a value too deep
/// for a recursive drop.
pub fn dismantle(data: Data) {
    let mut left = vec![data];
    while let Some(value) = left.pop() {
        match value {
            Data::List(items) => left.extend(items),
            Data::Map(map) => left.extend(map.into_values()),
            _ => {}
        }
    }
}

impl Request {
    /// Takes the data that the request carries away, a state's or an
    /// event's value, `Nil` in its place.
    fn take_data(&mut self) -> Option<Data> {
        match self {
            Self::SetState(value) | Self::Emit { value, .. } => {
                Some(std::mem::replace(value, Data::Nil))
            }
            _ => None,
        }
    }

    /// The data that the request carries, a state's or an event's value.
    fn data(&self) -> Option<&Data> {
        match self {
            Self::SetState(value) | Self::Emit { value, .. } => Some(value),
            _ => None,
        }
    }
}

/// `request`, if the data that it carries nests [`Data::DEEPEST`] levels at
/// most, as the host takes it; `failed` otherwise, its data dropped
/// without recursion, where it is checked, on the plugin's own thread.
pub fn within_depth(mut request: Request) -> Result<Request, crate::command::Error> {
    if request.data().is_none_or(Data::nests_within_deepest) {
        return Ok(request);
    }
    if let Some(data) = request.take_data() {
        dismantle(data);
    }
    Err(crate::command::Error::new(
        crate::command::Code::Failed,
        format!("a plugin's value nests {} levels at most", Data::DEEPEST),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value of lists nested `depth` levels deep, made without recursion.
    fn nested(depth: usize) -> Data {
        let mut value = Data::Nil;
        for _ in 0..depth {
            value = Data::List(vec![value]);
        }
        value
    }

    #[test]
    fn a_value_nests_within_its_depth_and_one_too_deep_drops_without_recursion() {
        assert!(nested(Data::DEEPEST).nests_within_deepest());
        assert!(!nested(Data::DEEPEST + 1).nests_within_deepest());
        let mut map = Data::Nil;
        for _ in 0..=Data::DEEPEST {
            map = Data::Map([("x".to_owned(), map)].into());
        }
        assert!(!map.nests_within_deepest());
        // Far deeper than a recursive walk or drop takes, refused, and
        // dropped a level at a time.
        let deep = Request::SetState(nested(1_000_000));
        assert_eq!(within_depth(deep).unwrap_err().code, Code::Failed);
        let kept = Request::SetState(nested(3));
        assert_eq!(within_depth(kept.clone()), Ok(kept));
    }
    use crate::command::{Category, Code, Value};

    #[test]
    fn the_messages_cross_as_data_and_come_back_the_same() {
        let calls = [
            Request::RegisterCommand(Command {
                name: "x.y".to_owned(),
                title: "Y".to_owned(),
                category: Category::View,
                capabilities: vec![Capability::Net("lokyr.dev".to_owned())],
                params: Vec::new(),
                when: None,
            }),
            Request::RunCommand {
                name: "x.y".to_owned(),
                args: Args::from([("n".to_owned(), Value::Integer(3))]),
            },
            Request::LineText {
                pane: PaneId(4),
                row: -2,
                reach: 32,
            },
            Request::Scroll {
                pane: PaneId(4),
                scroll: Scroll::Up(3),
            },
            Request::SetFontSize(None),
            Request::Quit,
        ]
        .map(|request| Call {
            caller: "links".to_owned(),
            request,
        });
        for call in calls {
            let text = serde_json::to_string(&call).unwrap();
            assert_eq!(serde_json::from_str::<Call>(&text).unwrap(), call, "{text}");
        }
        let responses = [
            Response::Done,
            Response::Line(Some(LineText {
                text: "日".to_owned(),
                cells: vec![(0, 7)],
            })),
            Response::Failed(Error::new(Code::NotFound, "there is no pane 4")),
        ];
        for response in responses {
            let text = serde_json::to_string(&response).unwrap();
            assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
        }
        let event = Event::Workspace(WorkspaceEvent::LayoutChanged {
            tab: crate::host::TabId(9),
        });
        let text = serde_json::to_string(&event).unwrap();
        assert_eq!(text, r#"{"workspace":{"layout-changed":{"tab":9}}}"#);
        assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), event);
        assert_eq!(event.name(), "layout-changed");
        let custom = Event::Custom {
            name: "search.progress".to_owned(),
            schema: "search.progress/1".to_owned(),
            value: Data::Map(BTreeMap::from([
                ("matches".to_owned(), Data::Integer(12)),
                ("done".to_owned(), Data::Boolean(false)),
                (
                    "panes".to_owned(),
                    Data::List(vec![
                        Data::Integer(4),
                        Data::Nil,
                        Data::Text("x".to_owned()),
                    ]),
                ),
            ])),
        };
        let text = serde_json::to_string(&custom).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), custom);
        assert_eq!(custom.name(), "search.progress");
    }

    #[test]
    fn each_request_needs_its_capability_and_has_its_effect() {
        let pane = PaneId(1);
        assert_eq!(
            Request::ClipboardText.capability(),
            Some(Capability::ClipboardRead)
        );
        assert_eq!(
            Request::SendText {
                pane,
                text: String::new()
            }
            .capability(),
            Some(Capability::PtyWrite)
        );
        assert_eq!(Request::SelectAll(pane).capability(), None);
        assert_eq!(Request::SelectionText(pane).effect(), Effect::Read);
        assert_eq!(
            Request::SetClipboardText(String::new()).effect(),
            Effect::Write
        );
        assert_eq!(Request::Open(String::new()).effect(), Effect::App);
        assert_eq!(
            Request::Bind(Binding {
                keys: "cmd+c".to_owned(),
                command: "x.y".to_owned(),
                args: Args::new(),
                when: None,
            })
            .effect(),
            Effect::Declaration
        );
    }
}

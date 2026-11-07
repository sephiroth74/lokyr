//! The plugins' side: the trait that a plugin implements, and the context
//! through which its callbacks reach the host, whose methods send the
//! host [requests](crate::message::Request) and read its responses.
//!
//! ```
//! use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error};
//! use lokyr_plugin_sdk::host::Binding;
//! use lokyr_plugin_sdk::plugin::{Cx, Plugin};
//!
//! #[derive(Default)]
//! struct Copying;
//!
//! impl Plugin for Copying {
//!     fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
//!         cx.register_command(Command {
//!             name: "selection.copy".to_owned(),
//!             title: "Copy".to_owned(),
//!             category: Category::Clipboard,
//!             capabilities: Vec::new(),
//!             params: Vec::new(),
//!             when: Some("selection".to_owned()),
//!         })?;
//!         cx.bind(Binding {
//!             keys: "cmd+c".to_owned(),
//!             command: "selection.copy".to_owned(),
//!             args: Args::new(),
//!             when: Some("selection".to_owned()),
//!         })
//!     }
//!
//!     fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _args: Args) -> Result<(), Error> {
//!         match name {
//!             "selection.copy" => {
//!                 let Some(pane) = cx.focused_pane() else {
//!                     return Ok(());
//!                 };
//!                 if let Some(text) = cx.selection_text(pane)? {
//!                     cx.set_clipboard_text(&text)?;
//!                 }
//!                 Ok(())
//!             }
//!             _ => Err(Error::new(Code::NotFound, format!("no command `{name}`"))),
//!         }
//!     }
//! }
//! ```

use crate::command::{Args, Code, Command, Error};
use crate::host::{
    BarPlace, Binding, Color, CommandInfo, Current, DecorationLook, Direction, Edge, Found, Host,
    Layout, LineText, PaneId, ScreenPoint, Scroll, SearchMatch, SearchQuery, Side, StyleRun, TabId,
    TabInfo, WindowId, WindowInfo,
};
use crate::message::{Call, Data, Event, Request, Response};
use crate::ui::{PanelId, PanelOptions, UiEvent, ViewId};
use crate::view::{Key, Node};

/// A plugin: its activation, which registers its commands and binds its
/// keys, and the calls of its commands, which the host checked against
/// their parameters, with their defaults.
///
/// The host calls it on the main thread, or on a thread of its own, which
/// its manifest asks for with `thread = "worker"` and which makes all its
/// calls, and never while it runs: what its calls ask of the app comes
/// once they return. On its own thread, each request waits for the main
/// thread's answer, and the host renders its views as their trees come,
/// the last ones staying meanwhile. A call that panics disables the
/// plugin, and takes its commands and its bindings away.
pub trait Plugin {
    /// Starts the plugin.
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let _ = cx;
        Ok(())
    }

    /// Runs `name`, one of the commands that it registered, with `args`.
    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        let _ = (cx, args);
        Err(Error::new(
            Code::NotFound,
            format!("the plugin has no command `{name}`"),
        ))
    }

    /// Takes `events`, those that it subscribed to, in their order, as a
    /// batch: a turn's of the main thread, which come after the calls that
    /// made them. What it emits comes in the next batch.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let _ = (cx, events);
        Ok(())
    }

    /// Its catalogs of [Project Fluent](https://projectfluent.org), each by
    /// its language, as `("it", source)`, English's the source of the
    /// others: the host titles its commands in the language shown with
    /// their messages, `command-` and the command's name, with `-` for its
    /// `.`, as `command-pane-copy`; a title that they lack is the one that
    /// the command registered. By default, none.
    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// Whether `name`, one of the commands that it registered, would do
    /// something now with `args`, which the host checked, as a menu asks
    /// before it shows the command's item enabled or dimmed: by default, it
    /// would. What the call asks of the app is dropped, as it only looks.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        let _ = (cx, name, args);
        true
    }

    /// The tree of `view`, a slot of a window's or a pane's that its
    /// manifest declares, or one of its panels: the host asks for it as a
    /// window or a pane shows the slot, as the plugin asks, as a panel
    /// opens and as the theme changes, and keeps it until the next. `None`
    /// shows nothing, and takes no room. By default, nothing.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let _ = (cx, view);
        None
    }

    /// Takes `event`, of a node of a key of one of its views.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let _ = (cx, event);
        Ok(())
    }

    /// The text of the JSON Schema of its section of the config file,
    /// `[plugins.<namespace>]`, the file that its manifest names as
    /// `config-schema`, which a built-in plugin includes as it builds: an
    /// object of its keys, each with its `default`, of the keywords that
    /// the host checks. By default, none: the plugin has no keys.
    fn config_schema(&self) -> Option<&'static str> {
        None
    }
}

/// A built-in plugin as the app takes it: the text of its manifest, its
/// `plugin.toml`, and what makes it, a new one each time, as the config
/// turns it on again. [`export_plugin!`](crate::export_plugin) gives a
/// plugin's crate its registration.
#[derive(Clone, Copy)]
pub struct Registration {
    /// The text of its manifest.
    pub manifest: &'static str,
    /// Makes the plugin, anew.
    pub make: fn() -> Box<dyn Plugin + Send>,
}

impl std::fmt::Debug for Registration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registration")
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

/// Gives a built-in plugin's crate its manifest, `MANIFEST`, the text of the
/// `plugin.toml` beside its `Cargo.toml`, and its registration,
/// `registration()`, which the app's distribution takes: of the plugin's
/// type, made by its `Default`, or by the expression after it.
///
/// ```ignore
/// #[derive(Default)]
/// struct Search;
///
/// impl Plugin for Search {}
///
/// lokyr_plugin_sdk::export_plugin!(Search);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        $crate::export_plugin!($plugin, <$plugin as ::core::default::Default>::default());
    };
    ($plugin:ty, $make:expr) => {
        /// The plugin's manifest, its `plugin.toml`, which the app reads as
        /// it adds the plugin.
        pub const MANIFEST: &str =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/plugin.toml"));

        /// The plugin's registration, which the app's distribution takes.
        pub fn registration() -> $crate::plugin::Registration {
            fn make() -> ::std::boxed::Box<dyn $crate::plugin::Plugin + ::core::marker::Send> {
                let plugin: $plugin = $make;
                ::std::boxed::Box::new(plugin)
            }
            $crate::plugin::Registration {
                manifest: MANIFEST,
                make,
            }
        }
    };
}

/// What a plugin's callbacks reach the host through: each method sends a
/// request, as the plugin, and reads the host's response. A read answers
/// with the host's state as it handles the request; a write is queued, and
/// applied as the plugin's call returns, so that a read in the same call
/// reads the state before it.
pub struct Cx<'a> {
    host: &'a mut dyn Host,
    caller: String,
}

impl<'a> Cx<'a> {
    /// The context of a callback of the plugin `caller`, on `host`.
    pub fn new(host: &'a mut dyn Host, caller: &str) -> Self {
        Self {
            host,
            caller: caller.to_owned(),
        }
    }

    /// Sends `request`, and returns the host's response.
    pub fn request(&mut self, request: Request) -> Response {
        self.host.handle(Call {
            caller: self.caller.clone(),
            request,
        })
    }

    /// Sends `request`, which the host does, or queues.
    fn done(&mut self, request: Request) -> Result<(), Error> {
        match self.request(request) {
            Response::Done => Ok(()),
            other => Err(failed(other)),
        }
    }

    /// Sends `request`, which a text answers.
    fn text(&mut self, request: Request) -> Result<Option<String>, Error> {
        match self.request(request) {
            Response::Text(text) => Ok(text),
            other => Err(failed(other)),
        }
    }

    /// Registers `command`, whose calls come to the plugin's
    /// [`on_command`](Plugin::on_command).
    pub fn register_command(&mut self, command: Command) -> Result<(), Error> {
        self.done(Request::RegisterCommand(command))
    }

    /// Runs the command `name` with `args`, once this call of the plugin
    /// returns: the call is checked now, and the command's own failure goes
    /// to the log.
    pub fn run_command(&mut self, name: &str, args: Args) -> Result<(), Error> {
        self.done(Request::RunCommand {
            name: name.to_owned(),
            args,
        })
    }

    /// Binds keys to a command, which needs not be registered yet: its keys
    /// tell the log of it, as they are pressed, until it is.
    pub fn bind(&mut self, binding: Binding) -> Result<(), Error> {
        self.done(Request::Bind(binding))
    }

    /// Subscribes to the events of `names`: the workspace's, as
    /// `tab-created`, or plugins' own, as `search.progress`, which
    /// [`on_event`](Plugin::on_event) takes from the next batch on.
    pub fn subscribe(&mut self, names: &[&str]) -> Result<(), Error> {
        self.done(Request::Subscribe(
            names.iter().map(|&name| name.to_owned()).collect(),
        ))
    }

    /// Emits the event `name`, of the plugin's namespace, as
    /// `search.progress`, of the schema `schema`, with `value`, to the
    /// plugins that subscribed to it, once this call returns.
    pub fn emit(&mut self, name: &str, schema: &str, value: Data) -> Result<(), Error> {
        self.done(Request::Emit {
            name: name.to_owned(),
            schema: schema.to_owned(),
            value,
        })
    }

    /// The clipboard's text, if it holds any.
    pub fn clipboard_text(&mut self) -> Result<Option<String>, Error> {
        self.text(Request::ClipboardText)
    }

    /// Puts `text` on the clipboard, as this call returns.
    pub fn set_clipboard_text(&mut self, text: &str) -> Result<(), Error> {
        self.done(Request::SetClipboardText(text.to_owned()))
    }

    /// The window that has the focus.
    pub fn focused_window(&mut self) -> Option<WindowId> {
        match self.request(Request::FocusedWindow) {
            Response::Window(window) => window,
            _ => None,
        }
    }

    /// The pane that has the focus: the active one of the focused window.
    pub fn focused_pane(&mut self) -> Option<PaneId> {
        match self.request(Request::FocusedPane) {
            Response::Pane(pane) => pane,
            _ => None,
        }
    }

    /// The text of `pane`'s selection, if it has one.
    pub fn selection_text(&mut self, pane: PaneId) -> Result<Option<String>, Error> {
        self.text(Request::SelectionText(pane))
    }

    /// The text of the logical line that `pane`'s row numbered `row` is in,
    /// of the screen or of its scrollback, as far as `reach` rows before it
    /// and after it, [`MOST_REACH`](crate::host::MOST_REACH) at most, with
    /// each character's cell; `None` for a row that the pane does not hold.
    ///
    /// A row's number stays its own as output scrolls it into the
    /// scrollback and the view scrolls, until the scrollback's limit takes
    /// it away; a resize, which lays the rows out again, numbers them
    /// anew. A command of a cell, as a click's, takes the number of the
    /// row that the click was on, as the drawn frame showed it.
    pub fn line_text(
        &mut self,
        pane: PaneId,
        row: i64,
        reach: usize,
    ) -> Result<Option<LineText>, Error> {
        match self.request(Request::LineText { pane, row, reach }) {
            Response::Line(line) => Ok(line),
            other => Err(failed(other)),
        }
    }

    /// The target of the OSC 8 link that the cell of `pane` at `column` of
    /// the row numbered `row` is part of, if it is one's, as
    /// [`Cx::line_text`] numbers the rows.
    pub fn link_at(
        &mut self,
        pane: PaneId,
        column: u16,
        row: i64,
    ) -> Result<Option<String>, Error> {
        self.text(Request::LinkAt { pane, column, row })
    }

    /// Reads the texts of `pane`'s rows from the row numbered `first`,
    /// `count` at most, [`MOST_LINES`](crate::host::MOST_LINES) at most, on
    /// a thread of the host's: returns the read's id, and the rows come to
    /// [`on_event`](Plugin::on_event) as an [`Event::Lines`] of that id,
    /// which no subscription needs.
    pub fn read_lines(&mut self, pane: PaneId, first: i64, count: usize) -> Result<u64, Error> {
        match self.request(Request::ReadLines { pane, first, count }) {
            Response::Reading(id) => Ok(id),
            other => Err(failed(other)),
        }
    }

    /// Searches `pane`'s terminal, its scrollback too, for `query`, on a
    /// thread of the host's, from the rows that its view shows outward:
    /// returns the search's id, and the matches come to
    /// [`on_event`](Plugin::on_event) as [`Event::Searched`] events of that
    /// id, a part's each, which no subscription needs. The plugin's search
    /// of the pane before ends; the matches stay until its next one,
    /// [`Cx::end_search`], or the pane's close.
    pub fn search(&mut self, pane: PaneId, query: SearchQuery) -> Result<u64, Error> {
        match self.request(Request::Search { pane, query }) {
            Response::Searching(id) => Ok(id),
            other => Err(failed(other)),
        }
    }

    /// Selects `pane`'s cells from `first` to `last`, each by its column
    /// and its row's number, in reading order, as a drag would, which a
    /// copy then takes, and scrolls the pane's view to show the first, as
    /// the call returns.
    pub fn select_cells(
        &mut self,
        pane: PaneId,
        first: (u16, i64),
        last: (u16, i64),
    ) -> Result<(), Error> {
        self.done(Request::SelectCells { pane, first, last })
    }

    /// Shows the matches of the plugin's search `search` over its pane's
    /// cells, or hides them, as `all` says, and `current` apart, if one,
    /// whose row the pane's view scrolls to, as the call returns: the host
    /// draws them in the theme's colors until the search ends.
    pub fn show_matches(
        &mut self,
        search: u64,
        all: bool,
        current: Option<SearchMatch>,
    ) -> Result<(), Error> {
        self.done(Request::ShowMatches {
            search,
            all,
            current,
        })
    }

    /// Ends the plugin's search `search`, at once: it stops, and its
    /// matches go.
    pub fn end_search(&mut self, search: u64) -> Result<(), Error> {
        self.done(Request::EndSearch(search))
    }

    /// The matches of the plugin's search `search`, in the rows' order,
    /// from the one at `first`, `count` at most, as their cells are now.
    pub fn search_matches(
        &mut self,
        search: u64,
        first: usize,
        count: usize,
    ) -> Result<Found, Error> {
        match self.request(Request::SearchMatches {
            search,
            first,
            count,
        }) {
            Response::Matches(found) => Ok(found),
            other => Err(failed(other)),
        }
    }

    /// The registered commands, in the order of their registrations, as
    /// the menus list them: each one's plugin, keys, context and state of
    /// the menus too, of a plugin of `user-commands`.
    pub fn commands(&mut self) -> Result<Vec<CommandInfo>, Error> {
        match self.request(Request::Commands) {
            Response::Commands(commands) => Ok(commands),
            other => Err(failed(other)),
        }
    }

    /// The themes that the app can show, the appearance in force and its
    /// theme.
    pub fn themes(&mut self) -> Result<crate::host::Themes, Error> {
        match self.request(Request::Themes) {
            Response::Themes(themes) => Ok(themes),
            other => Err(failed(other)),
        }
    }

    /// Shows the theme `name` in place of the appearance in force's, for a
    /// while, as a picker previews it; `None` shows the config's again.
    pub fn preview_theme(&mut self, name: Option<&str>) -> Result<(), Error> {
        self.done(Request::PreviewTheme(name.map(str::to_owned)))
    }

    /// Names the theme `name` in the config's `[theme]` for the appearance
    /// of `variant`, which the app shows at once, a preview ending.
    pub fn set_theme(
        &mut self,
        variant: crate::theme::ThemeVariant,
        name: &str,
    ) -> Result<(), Error> {
        self.done(Request::SetTheme {
            variant,
            name: name.to_owned(),
        })
    }

    /// The current match of the plugin's search `search`, as its cells are
    /// now, and its place among the search's matches now; and how many of
    /// them the host keeps.
    pub fn current_match(&mut self, search: u64) -> Result<Current, Error> {
        match self.request(Request::CurrentMatch(search)) {
            Response::Current(current) => Ok(current),
            other => Err(failed(other)),
        }
    }

    /// The styles of `pane`'s row numbered `row`, as runs of cells; `None`
    /// for a row that the pane does not hold.
    pub fn styles(&mut self, pane: PaneId, row: i64) -> Result<Option<Vec<StyleRun>>, Error> {
        match self.request(Request::Styles { pane, row }) {
            Response::Styles(runs) => Ok(runs),
            other => Err(failed(other)),
        }
    }

    /// Decorates `pane`'s cells from `from` to `to`, in reading order, each
    /// by its column and its row's number, with `look`, as this call
    /// returns: returns the decoration's id. It keeps its text as output
    /// scrolls it and a resize reflows it, and goes with it.
    pub fn decorate(
        &mut self,
        pane: PaneId,
        from: (u16, i64),
        to: (u16, i64),
        look: DecorationLook,
    ) -> Result<u64, Error> {
        match self.request(Request::Decorate {
            pane,
            from,
            to,
            look,
        }) {
            Response::Decoration(id) => Ok(id),
            other => Err(failed(other)),
        }
    }

    /// Takes away the plugin's decoration `id` of `pane`, as this call
    /// returns.
    pub fn undecorate(&mut self, pane: PaneId, id: u64) -> Result<(), Error> {
        self.done(Request::Undecorate { pane, id })
    }

    /// The working directory of the program in `pane`'s foreground, as a
    /// shell's at its prompt.
    pub fn working_directory(&mut self, pane: PaneId) -> Result<Option<String>, Error> {
        self.text(Request::WorkingDirectory(pane))
    }

    /// Opens `target`, once this call returns, with the system's handler:
    /// a URL of `http`, `https`, `mailto` or `file`, or a path, absolute or
    /// from `~`, a file in its application and a directory in the file
    /// manager. Another scheme, or a relative path, fails. As the user's
    /// config says, a URL or a file asks the user first, showing it, and a
    /// file can show in the file manager instead; a file that would run
    /// never does.
    pub fn open(&mut self, target: &str) -> Result<(), Error> {
        self.done(Request::Open(target.to_owned()))
    }

    /// Whether [`open`](Self::open) would open `target`: a URL of its
    /// schemes, or a path, absolute or from `~`, that is there.
    pub fn opens(&mut self, target: &str) -> Result<bool, Error> {
        match self.request(Request::Opens(target.to_owned())) {
            Response::Boolean(opens) => Ok(opens),
            other => Err(failed(other)),
        }
    }

    /// Selects everything of `pane`, its scrollback and its screen, as
    /// this call returns.
    pub fn select_all(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::SelectAll(pane))
    }

    /// Clears `pane`'s scrollback, as this call returns.
    pub fn clear_scrollback(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::ClearScrollback(pane))
    }

    /// Clears `pane`'s scrollback, and its screen but the prompt that the
    /// cursor is in, which goes to the top, as this call returns; the
    /// program is told nothing.
    pub fn clear(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::Clear(pane))
    }

    /// Pastes `text` into `pane`, as the user pastes, as this call returns:
    /// bracketed when its program asks, without its control characters but
    /// tab and the line breaks. Into a program that does not ask, a text of
    /// several lines, whose line breaks reach it as Return, waits for the
    /// user to confirm it, and a single line pastes without the line break
    /// after it; the user's config can ask always, or never.
    pub fn paste(&mut self, pane: PaneId, text: &str) -> Result<(), Error> {
        self.done(Request::Paste {
            pane,
            text: text.to_owned(),
        })
    }

    /// Sends `text` to `pane`'s program as it is, as this call returns, its
    /// control characters too, as typed keys would, which can run
    /// commands; the view comes back to the screen.
    pub fn send_text(&mut self, pane: PaneId, text: &str) -> Result<(), Error> {
        self.done(Request::SendText {
            pane,
            text: text.to_owned(),
        })
    }

    /// Scrolls `pane`'s view, as this call returns.
    pub fn scroll(&mut self, pane: PaneId, scroll: Scroll) -> Result<(), Error> {
        self.done(Request::Scroll { pane, scroll })
    }

    /// The size of the text, in points, of every window.
    pub fn font_size(&mut self) -> Result<f32, Error> {
        match self.request(Request::FontSize) {
            Response::FontSize(size) => Ok(size),
            other => Err(failed(other)),
        }
    }

    /// Gives the text `size`, in points, from 4 to 200, once this call
    /// returns; with `None`, the config's.
    pub fn set_font_size(&mut self, size: Option<f32>) -> Result<(), Error> {
        self.done(Request::SetFontSize(size))
    }

    /// Opens a window, once this call returns.
    pub fn open_window(&mut self) -> Result<(), Error> {
        self.done(Request::OpenWindow)
    }

    /// Closes `window`, once this call returns.
    pub fn close_window(&mut self, window: WindowId) -> Result<(), Error> {
        self.done(Request::CloseWindow(window))
    }

    /// Quits the app, once this call returns.
    pub fn quit(&mut self) -> Result<(), Error> {
        self.done(Request::Quit)
    }

    /// The windows, each with its tabs and the one that it shows.
    pub fn windows(&mut self) -> Result<Vec<WindowInfo>, Error> {
        match self.request(Request::Windows) {
            Response::Windows(windows) => Ok(windows),
            other => Err(failed(other)),
        }
    }

    /// The tabs of `window`, in their order.
    pub fn tabs(&mut self, window: WindowId) -> Result<Vec<TabInfo>, Error> {
        match self.request(Request::Tabs(window)) {
            Response::Tabs(tabs) => Ok(tabs),
            other => Err(failed(other)),
        }
    }

    /// The layout of `tab`.
    pub fn layout(&mut self, tab: TabId) -> Result<Layout, Error> {
        match self.request(Request::Layout(tab)) {
            Response::Layout(layout) => Ok(layout),
            other => Err(failed(other)),
        }
    }

    /// Opens a tab in `window`, at `index` of its tabs, or at their end,
    /// of a new pane, as this call returns: the tab's id and the pane's.
    pub fn new_tab(
        &mut self,
        window: WindowId,
        index: Option<usize>,
    ) -> Result<(TabId, PaneId), Error> {
        match self.request(Request::NewTab { window, index }) {
            Response::Tab { tab, pane } => Ok((tab, pane)),
            other => Err(failed(other)),
        }
    }

    /// Splits `pane`'s part along `direction`, with a new pane on `side`,
    /// as this call returns: the new pane's id.
    pub fn split(
        &mut self,
        pane: PaneId,
        direction: Direction,
        side: Side,
    ) -> Result<PaneId, Error> {
        match self.request(Request::Split {
            pane,
            direction,
            side,
        }) {
            Response::Pane(Some(pane)) => Ok(pane),
            other => Err(failed(other)),
        }
    }

    /// Closes `tab`, with its panes, as this call returns; with `ask`, once
    /// the user confirms it while a program other than a shell runs there,
    /// as the host asks for a window.
    pub fn close_tab(&mut self, tab: TabId, ask: bool) -> Result<(), Error> {
        self.done(Request::CloseTab { tab, ask })
    }

    /// Closes `pane`, as this call returns; with `ask`, as
    /// [`Cx::close_tab`] says.
    pub fn close_pane(&mut self, pane: PaneId, ask: bool) -> Result<(), Error> {
        self.done(Request::ClosePane { pane, ask })
    }

    /// Gives `tab` a title of its own, in place of its program's, or with
    /// `None` gives its program's back, as this call returns.
    pub fn set_tab_title(&mut self, tab: TabId, title: Option<&str>) -> Result<(), Error> {
        self.done(Request::SetTabTitle {
            tab,
            title: title.map(str::to_owned),
        })
    }

    /// Gives `tab` a color of its own, an RGB one, in place of its
    /// program's, or with `None` gives its program's back, as this call
    /// returns.
    pub fn set_tab_color(&mut self, tab: TabId, color: Option<Color>) -> Result<(), Error> {
        self.done(Request::SetTabColor { tab, color })
    }

    /// The keys that run `command` with `args`, as the platform writes
    /// them, as `⌘1`; none without a binding.
    pub fn keys_of(&mut self, command: &str, args: Args) -> Result<Option<String>, Error> {
        self.text(Request::KeysOf {
            command: command.to_owned(),
            args,
        })
    }

    /// The text of `id`, a message of the plugin's catalogs, of `args`, in
    /// the language that the app shows, as a view's texts, labels and
    /// tooltips take it: `id` itself where the catalogs lack it.
    pub fn message(&mut self, id: &str, args: &[(&str, Data)]) -> Result<String, Error> {
        let text = self.text(Request::Message {
            id: id.to_owned(),
            args: args
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect(),
        })?;
        Ok(text.unwrap_or_else(|| id.to_owned()))
    }

    /// Puts the plugin's tab bar at `place` in the windows that show it, as
    /// this call returns.
    pub fn set_tab_bar_place(&mut self, place: BarPlace) -> Result<(), Error> {
        self.done(Request::SetTabBarPlace(place))
    }

    /// Fades the panes that have not their tab's keyboard toward their
    /// background by `dim`, from 0, none, to 1, while the plugin runs, as
    /// this call returns.
    pub fn set_inactive_dim(&mut self, dim: f32) -> Result<(), Error> {
        self.done(Request::SetInactiveDim(dim))
    }

    /// Gives the keyboard to the pane that the pointer comes over, or no
    /// more, while the plugin runs, as this call returns.
    pub fn set_focus_follows_mouse(&mut self, on: bool) -> Result<(), Error> {
        self.done(Request::SetFocusFollowsMouse(on))
    }

    /// Puts the plugin's tab bar of `window` at `place`, over its place in
    /// every window, or at that place again with `None`, as this call
    /// returns.
    pub fn set_window_tab_bar_place(
        &mut self,
        window: WindowId,
        place: Option<BarPlace>,
    ) -> Result<(), Error> {
        self.done(Request::SetWindowTabBarPlace { window, place })
    }

    /// The plugin's tab bar's place, a column's width as the user's drag
    /// left it.
    pub fn tab_bar_place(&mut self) -> Result<BarPlace, Error> {
        match self.request(Request::TabBarPlace) {
            Response::TabBarPlace(place) => Ok(place),
            other => Err(failed(other)),
        }
    }

    /// Asks the user for a text, in an alert of `message` and `detail` on
    /// `window`, or on the focused window, of a field that holds `text`:
    /// the question's id, whose answer comes as [`Event::Answered`].
    pub fn ask_text(
        &mut self,
        window: Option<WindowId>,
        message: &str,
        detail: &str,
        text: &str,
    ) -> Result<u64, Error> {
        match self.request(Request::AskText {
            window,
            message: message.to_owned(),
            detail: detail.to_owned(),
            text: text.to_owned(),
        }) {
            Response::Asking(id) => Ok(id),
            other => Err(failed(other)),
        }
    }

    /// Has `tab`'s window show it, as this call returns.
    pub fn activate_tab(&mut self, tab: TabId) -> Result<(), Error> {
        self.done(Request::ActivateTab(tab))
    }

    /// Gives `pane` its tab's keyboard, as this call returns.
    pub fn focus_pane(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::FocusPane(pane))
    }

    /// Moves `tab` to `index` of `window`'s tabs, as this call returns.
    pub fn move_tab(&mut self, tab: TabId, window: WindowId, index: usize) -> Result<(), Error> {
        self.done(Request::MoveTab { tab, window, index })
    }

    /// Moves the panes of `tab` beside `pane`, of another tab, at its
    /// `edge`, the tab's layout whole in that half, as this call returns.
    pub fn tab_into_pane(&mut self, tab: TabId, pane: PaneId, edge: Edge) -> Result<(), Error> {
        self.done(Request::TabIntoPane { tab, pane, edge })
    }

    /// Moves `tab` into a window of its own, as this call returns.
    pub fn tab_to_new_window(&mut self, tab: TabId) -> Result<(), Error> {
        self.done(Request::TabToNewWindow { tab, at: None })
    }

    /// Moves `tab` into a window of its own whose content's top left
    /// corner goes at `at` on the screens, or, alone in its window, moves
    /// its window there, as this call returns.
    pub fn tab_to_new_window_at(&mut self, tab: TabId, at: ScreenPoint) -> Result<(), Error> {
        self.done(Request::TabToNewWindow { tab, at: Some(at) })
    }

    /// Moves `pane` into a tab of its own, at `index` of `window`'s tabs,
    /// as this call returns.
    pub fn pane_to_new_tab(
        &mut self,
        pane: PaneId,
        window: WindowId,
        index: usize,
    ) -> Result<(), Error> {
        self.done(Request::PaneToNewTab {
            pane,
            window,
            index,
        })
    }

    /// Moves `pane` into a window of its own, as this call returns.
    pub fn pane_to_new_window(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::PaneToNewWindow(pane))
    }

    /// The title of `pane`, as its header shows it: its program's, or its
    /// folder and its program, to a plugin that reads the processes.
    pub fn pane_title(&mut self, pane: PaneId) -> Result<Option<String>, Error> {
        match self.request(Request::PaneTitle(pane)) {
            Response::Text(title) => Ok(title),
            other => Err(failed(other)),
        }
    }

    /// The grid of `pane`, its columns and its rows, as its window last
    /// laid it out.
    pub fn pane_grid(&mut self, pane: PaneId) -> Result<(u16, u16), Error> {
        match self.request(Request::PaneGrid(pane)) {
            Response::Grid { columns, rows } => Ok((columns, rows)),
            other => Err(failed(other)),
        }
    }

    /// Swaps `pane` and `with`, of one tab, each taking the other's room,
    /// as this call returns.
    pub fn swap_panes(&mut self, pane: PaneId, with: PaneId) -> Result<(), Error> {
        self.done(Request::SwapPanes { pane, with })
    }

    /// Moves `edge` of `pane` by `by`, a share of the room of the split
    /// whose divider it is, as this call returns.
    pub fn resize_pane(&mut self, pane: PaneId, edge: Edge, by: f64) -> Result<(), Error> {
        self.done(Request::ResizePane { pane, edge, by })
    }

    /// Gives the split at `path` of `tab`'s layout the first part's share
    /// `ratio`, or its panes' with `None`, as this call returns.
    pub fn set_ratio(
        &mut self,
        tab: TabId,
        path: Vec<Side>,
        ratio: Option<f64>,
    ) -> Result<(), Error> {
        self.done(Request::SetRatio { tab, path, ratio })
    }

    /// Shares each split of `tab`'s layout by its panes again, as this call
    /// returns.
    pub fn equalize(&mut self, tab: TabId) -> Result<(), Error> {
        self.done(Request::Equalize(tab))
    }

    /// Has `pane` take `tab`'s whole room, or ends its zoom with `None`, as
    /// this call returns.
    pub fn zoom(&mut self, tab: TabId, pane: Option<PaneId>) -> Result<(), Error> {
        self.done(Request::Zoom { tab, pane })
    }

    /// Asks for a render of `view`, one of the plugin's, as this call
    /// returns.
    pub fn request_render(&mut self, view: ViewId) -> Result<(), Error> {
        self.done(Request::RequestRender(view))
    }

    /// Sets the text of the field `key` of `view`, as this call returns,
    /// over no keys typed since its `revision`.
    pub fn set_text(
        &mut self,
        view: ViewId,
        key: impl Into<Key>,
        text: &str,
        revision: u64,
    ) -> Result<(), Error> {
        self.done(Request::SetText {
            view,
            key: key.into(),
            text: text.to_owned(),
            revision,
        })
    }

    /// Moves the selection of the list `key` of `view` to `item`, as this
    /// call returns.
    pub fn select(&mut self, view: ViewId, key: impl Into<Key>, item: usize) -> Result<(), Error> {
        self.done(Request::Select {
            view,
            key: key.into(),
            item,
        })
    }

    /// Gives the keyboard's focus to the node `key` of `view`, as this call
    /// returns: a field selects all of its text, so that typing replaces
    /// it.
    pub fn focus_node(&mut self, view: ViewId, key: impl Into<Key>) -> Result<(), Error> {
        self.done(Request::FocusNode {
            view,
            key: key.into(),
        })
    }

    /// Scrolls the nearest scroll area around the node `key` of `view` as
    /// little as shows it, once the view shows it, as this call returns.
    pub fn reveal(&mut self, view: ViewId, key: impl Into<Key>) -> Result<(), Error> {
        self.done(Request::Reveal {
            view,
            key: key.into(),
        })
    }

    /// Opens the panel `id`, as this call returns, which the host then
    /// renders.
    pub fn open_panel(
        &mut self,
        id: impl Into<PanelId>,
        options: PanelOptions,
    ) -> Result<(), Error> {
        self.done(Request::OpenPanel {
            id: id.into(),
            options,
        })
    }

    /// Closes the panel `id`, as this call returns.
    pub fn close_panel(&mut self, id: impl Into<PanelId>) -> Result<(), Error> {
        self.done(Request::ClosePanel(id.into()))
    }

    /// The plugin's config, as a map of its keys: the values of its section
    /// of the config file, `[plugins.<namespace>]`, as its schema checked
    /// them, with the defaults of the keys that it leaves out. As the file
    /// reloads, an [`Event::ConfigChanged`] tells that it changed.
    pub fn config(&mut self) -> Result<Data, Error> {
        self.data(Request::Config)
    }

    /// The plugin's state, as it last kept it with
    /// [`set_state`](Self::set_state), of this run or an earlier one;
    /// [`Data::Nil`] if it kept none.
    pub fn state(&mut self) -> Result<Data, Error> {
        self.data(Request::State)
    }

    /// Keeps `value` as the plugin's state for the app's next runs, as this
    /// call returns: 1 MiB at most as JSON, whose numbers are finite.
    pub fn set_state(&mut self, value: Data) -> Result<(), Error> {
        self.done(Request::SetState(value))
    }

    /// Sends `request`, which a plain value answers.
    fn data(&mut self, request: Request) -> Result<Data, Error> {
        match self.request(request) {
            Response::Data(data) => Ok(data),
            other => Err(failed(other)),
        }
    }
}

/// The error of `response`, which failed, or answered another request.
fn failed(response: Response) -> Error {
    match response {
        Response::Failed(error) => error,
        other => Error::new(
            Code::Fault,
            format!("the host answered another request: {other:?}"),
        ),
    }
}

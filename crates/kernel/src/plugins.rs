//! The plugins that the app runs, built in, and the host that they reach
//! the app through. Each call of a plugin runs within `catch_unwind`, so
//! that one that panics is disabled, with its commands and its bindings,
//! rather than taking the app down.
//!
//! A plugin reaches the host through messages: its reads answer with the
//! state as the host handles them; its writes, as a pane's selection, are
//! queued, and applied in their order as its call returns, a plugin on the
//! main thread too; what changes the app, as a window to open or another
//! command to run, comes as a request, which the app takes after them, as
//! a plugin is never called while it runs. Each request needs its
//! capability, which the plugin's grants hold: a first-party plugin holds
//! every one, and runs any command; another, those that its manifest
//! declares, and the commands of its `[uses]`.
//!
//! A plugin whose manifest asks for a thread of its own, `thread =
//! "worker"`, makes its calls there, its renders too: the main thread
//! answers its requests and takes its calls' ends as it turns, through
//! [`Plugins::serve`], never waiting for them.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use lokyr_pkg_model::manifest::{Kind as PackageKind, Manifest, ManifestError, Thread};
use lokyr_pkg_model::schema::Schema;
pub use lokyr_plugin_sdk::API_VERSION;
pub use lokyr_plugin_sdk::event::WorkspaceEvent;
pub use lokyr_plugin_sdk::host;
use lokyr_plugin_sdk::host::MOST_WRITES;
pub use lokyr_plugin_sdk::image;
pub use lokyr_plugin_sdk::message::Event;
pub use lokyr_plugin_sdk::message::{
    COMMAND_ENDED, COMMAND_STARTED, Data, KEYS_CHANGED, Mods, POINTER, SAVING, VIEWPORT_CHANGED,
    WINDOW_FOCUS,
};
use lokyr_plugin_sdk::message::{Call, Effect, Request as Asked, Response, within_depth};
pub use lokyr_plugin_sdk::plugin::{Catalogs, Cx, Plugin, Registration, built_in};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Slot, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{Key, Kind, Node};
pub use lokyr_plugin_sdk::{ui, view};
use lokyr_session::Reader;
use lokyr_text_core::Size;
use lokyr_vt_core::page::{self, Rgb, Style};
use lokyr_vt_core::terminal::{Look, Scroll, SelectionShape, SelectionUnit};
use serde_json::{Map, Value};

use crate::commands::{self, Args, Capability, Code, Command, Error, Registry, Uses};
use crate::config::{ConfirmPaste, PluginSection, PluginsConfig};
use crate::keymap::{self, Binding, Invocation, When};
use crate::l10n::{Arg, Texts};
use crate::paste::{self, Paste};
use crate::workspace::{
    Direction, Edge, Layout, Pane, PaneId, Ratio, Side, TabId, WindowId, Workspace,
};

mod search;
mod worker;

use worker::{Called, Done, FromWorker, Job, Worker};

/// The chain of commands that ran commands that a command's run is of:
/// its id, which the app gives the run that starts it, a command that the
/// user ran or that a call of no command's asked, 0 before; and how deep
/// in it the command ran, 0 for the first. The app bounds a chain in
/// depth and in its runs, as a command that runs itself twice would run
/// twice as often at each step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Chain {
    /// The chain's id, 0 for none yet.
    pub id: u64,
    /// How deep in it the command ran.
    pub depth: usize,
}

/// What a plugin asks of the app, which the app does once the plugin's
/// call returns.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Run a command, whose call the registry checked.
    Run {
        /// The command and its arguments.
        invocation: Invocation,
        /// The chain of the command whose call asked it, as commands ran
        /// commands: none for a call of no command's, an event's or a
        /// render's.
        chain: Chain,
    },
    /// Open a window, its pane in `directory`, a path absolute or from `~`,
    /// or in the folder of a file there, if given.
    OpenWindow(Option<String>),
    /// Open a quick window of the id `window`, which the plugin's request
    /// gave, shown at `place`, its pane in `directory`, if given.
    OpenQuickWindow {
        /// Its id.
        window: WindowId,
        /// Where it shows.
        place: host::QuickPlace,
        /// The folder of its pane.
        directory: Option<String>,
    },
    /// Show the quick window `window` at `place`, or hide it, of `None`.
    ShowQuickWindow {
        /// The quick window.
        window: WindowId,
        /// Where it shows, or `None` to hide it.
        place: Option<host::QuickPlace>,
    },
    /// Close a window.
    CloseWindow(WindowId),
    /// Quit.
    Quit,
    /// Give the text a size, in points; with `None`, the config's.
    FontSize(Option<f32>),
    /// Show the theme of a name in place of the appearance in force's, for
    /// a while; with `None`, the config's again.
    PreviewTheme(Option<String>),
    /// Name a theme in the config's `[theme]` for an appearance.
    SetTheme {
        /// The appearance.
        variant: crate::themes::ThemeVariant,
        /// The theme's name.
        name: String,
    },
    /// Show the monospaced font family of a name in every pane in place of
    /// the config's, for a while; with `None`, the config's again.
    PreviewFont(Option<String>),
    /// Name a monospaced font family in the config's `[font] family`.
    SetFont(String),
    /// Open a URL, or an absolute path, with the system's handler.
    Open(String),
    /// Ask the user to confirm a paste of `text` into `pane`, whose
    /// program takes its line breaks as Return.
    ConfirmPaste {
        /// The pane.
        pane: PaneId,
        /// What to paste.
        text: String,
    },
    /// Set the text of a field of a plugin's view, over no keys typed
    /// since `revision`.
    SetText {
        /// The plugin.
        plugin: String,
        /// The view.
        view: ViewId,
        /// The field's key.
        key: Key,
        /// The text.
        text: String,
        /// The revision of the field's text that it answers.
        revision: u64,
    },
    /// Move the selection of a list of a plugin's view to an item.
    Select {
        /// The plugin.
        plugin: String,
        /// The view.
        view: ViewId,
        /// The list's key.
        key: Key,
        /// The item's index.
        item: usize,
    },
    /// Give the keyboard's focus to a node of a plugin's view.
    FocusNode {
        /// The plugin.
        plugin: String,
        /// The view.
        view: ViewId,
        /// The node's key.
        key: Key,
    },
    /// Scroll the nearest scroll area around a node of a plugin's view as
    /// little as shows it, once the view shows it.
    Reveal {
        /// The plugin.
        plugin: String,
        /// The view.
        view: ViewId,
        /// The node's key.
        key: Key,
    },
    /// Open a panel of a plugin's in `window`, whose tree the plugin's
    /// render gives.
    OpenPanel {
        /// The plugin.
        plugin: String,
        /// Its id.
        id: PanelId,
        /// The window.
        window: WindowId,
        /// Its anchor, its role and what closes it.
        options: PanelOptions,
    },
    /// Close a panel of a plugin's, which the plugins' host forgot as its
    /// plugin asked, and whose closing it tells.
    ClosePanel {
        /// The plugin.
        plugin: String,
        /// Its id.
        id: PanelId,
    },
    /// Close a tab or a pane once the user confirms it, as the app asks
    /// for a window while a program other than a shell runs there.
    CloseAsked(Closing),
    /// Put a window's content's top left corner at `at` on the screens, a
    /// window that a plugin's move of a tab opened, as it shows, or the
    /// window of a tab that was alone in it.
    PlaceWindow {
        /// The window.
        window: WindowId,
        /// Where its content's top left corner goes.
        at: host::ScreenPoint,
    },
    /// Ask the user whether the plugin may hold `capabilities`, which its
    /// manifest declares and no decision of the user's grants or refuses
    /// yet: its activation waits for the answer, which
    /// [`Plugins::consent`] gives.
    AskConsent {
        /// The plugin.
        plugin: String,
        /// The capabilities, in their order.
        capabilities: Vec<Capability>,
    },
    /// Ask the user where to save `bytes` as a file, in the save panel,
    /// whose end [`Plugins::saved`] tells the plugin: the app writes the
    /// file where the user chose.
    SaveFile {
        /// The plugin.
        plugin: String,
        /// The question's id.
        id: u64,
        /// The window that the panel shows on: the one given, or the focused
        /// one as the plugin asked.
        window: WindowId,
        /// The file's name, as the panel suggests it.
        name: String,
        /// Its bytes.
        bytes: Vec<u8>,
    },
    /// Run a call of the packages' service, Sleip's command, whose lines
    /// and end [`Plugins::package_line`] and [`Plugins::packages_ended`]
    /// tell the plugin.
    Packages {
        /// The plugin.
        plugin: String,
        /// The job's id.
        id: u64,
        /// The call.
        call: host::Packages,
    },
    /// End the plugin's job `id` of the packages' service, if it runs.
    CancelPackages {
        /// The plugin.
        plugin: String,
        /// The job's id.
        id: u64,
    },
    /// Ask the user for a text, whose answer [`Plugins::answer`] gives the
    /// plugin.
    AskText {
        /// The plugin.
        plugin: String,
        /// The question's id.
        id: u64,
        /// The window that the alert shows on, or the focused one.
        window: Option<WindowId>,
        /// What the alert asks.
        message: String,
        /// What it explains.
        detail: String,
        /// The field's text as it opens.
        text: String,
    },
}

/// What a close that asks first closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closing {
    /// A tab, with its panes.
    Tab(TabId),
    /// A pane.
    Pane(PaneId),
}

/// The clipboard, which the app reaches through its platform.
pub trait Clipboard {
    /// Its text, if it holds any.
    fn text(&self) -> Option<String>;

    /// Puts `text` on it.
    fn set_text(&mut self, text: &str);

    /// Puts the image of `png`, a PNG, on it, in place of what it held.
    fn set_image(&mut self, png: &[u8]);
}

/// What renders the images of cells that plugins ask for, the app's own
/// way, as its windows draw their panes.
pub trait Images {
    /// The image of `image`'s cells that `plugin` asks for, checked
    /// already, as a PNG: in the theme's colors and the terminal's font, in
    /// its frame, at its scale or the display's of the focused window, kept
    /// by an id of the app's as it asks, the oldest of the plugin's let go
    /// past [`MOST_KEPT`](lokyr_plugin_sdk::image::MOST_KEPT); the error
    /// that tells why it cannot be, as of an app that does not draw, or of
    /// an image too large.
    fn render(&mut self, plugin: &str, image: &ImageSpec) -> Result<Image, String>;

    /// Lets go `plugin`'s image `id`, if the app keeps it.
    fn drop_image(&mut self, plugin: &str, id: u64);
}

/// What starts the programs of the panes that plugins open, the app's own
/// way, as it starts a window's pane.
pub trait Panes {
    /// A pane of the id `id`, which nothing of the process took, beside
    /// `near`, the pane that it splits or the focused one of the window
    /// that it opens in, if there is one: its program, its directory and
    /// its size as a new pane's beside it; its directory `directory`, if
    /// given, a path absolute or from `~`, or the folder of a file there.
    fn spawn(&mut self, id: PaneId, near: Option<&Pane>, directory: Option<&str>) -> Pane;
}

/// What the host reaches of the app during a plugin's call.
pub struct Context<'a> {
    /// The commands.
    pub registry: &'a mut Registry,
    /// The windows, the tabs and the panes.
    pub workspace: &'a mut Workspace,
    /// What starts the panes that plugins open.
    pub panes: &'a mut dyn Panes,
    /// The clipboard.
    pub clipboard: &'a mut dyn Clipboard,
    /// What renders the images of cells.
    pub images: &'a mut dyn Images,
    /// The window that has the focus.
    pub focused: Option<WindowId>,
    /// The text's size, in points.
    pub font_size: f32,
    /// When a paste with line breaks asks the user first.
    pub confirm_paste: ConfirmPaste,
    /// The keys that run a command with its arguments, as the platform
    /// writes them, as `⌘1`; none without a binding.
    pub keys: &'a dyn Fn(&str, &Args) -> Option<String>,
    /// Whether the menus enable a command now, by its name, as its context
    /// and the focused pane say.
    pub enabled: &'a dyn Fn(&str) -> bool,
    /// The themes that the app can show, the appearance in force and its
    /// theme.
    pub themes: &'a dyn Fn() -> host::Themes,
    /// The system's monospaced font families, and the family in force, as
    /// the app reads them.
    pub fonts: &'a dyn Fn() -> host::Fonts,
    /// The day of the system's clock, in days since 1970-01-01, in UTC.
    pub day: u64,
    /// Whether a notice of the app's shows.
    pub notice: bool,
    /// Whether a window shows a save panel.
    pub saving: &'a dyn Fn(WindowId) -> bool,
    /// The language that the app shows, of its own catalogs, which the
    /// plugins' messages take.
    pub language: &'a str,
    /// The host's time, which the timers that the plugins start wait from.
    pub now: Instant,
}

/// Where a plugin is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Added, and not activated yet.
    Inactive,
    /// Activated: its commands run, and its keys are bound.
    Active,
    /// Stopped, as its activation failed or a call panicked: it has no
    /// commands nor bindings.
    Disabled,
    /// Turned off by its section of the config, `enabled = false`: it has
    /// no commands, bindings, views nor panels, until the config turns it
    /// on again.
    Off,
}

/// What makes a plugin anew, as a registration's function or an installed
/// plugin's package does.
pub type Make = Box<dyn Fn() -> Box<dyn Plugin + Send>>;

/// A plugin, and what the host keeps of it: what makes it anew, as the
/// config turns it on again or a fault's while passes, if it was
/// registered or installed, and whether the new one runs on a thread of
/// its own.
struct Entry {
    runner: Runner,
    state: State,
    make: Option<(Make, bool)>,
}

/// Where a plugin's calls run.
enum Runner {
    /// On the main thread, which calls it in place.
    Main(Box<dyn Plugin>),
    /// On a thread of its own, which starts as it activates.
    Unstarted(Box<dyn Plugin + Send>),
    /// On its thread.
    Worker(Box<Worker>),
    /// Nowhere, as it stopped on its thread, which ended.
    Stopped,
}

/// What the host keeps of a plugin: its name, its manifest, its status,
/// the capabilities that it was granted and the commands of others that it
/// runs, and the commands and the bindings that it gave.
struct State {
    name: String,
    /// Its manifest, if it has one.
    manifest: Option<Manifest>,
    status: Status,
    /// Its capabilities; `None` for every one, as a first-party plugin's.
    granted: Option<Vec<Capability>>,
    /// The commands of others that it runs; `None` for every command, as a
    /// first-party plugin's.
    uses: Option<Uses>,
    /// The namespace of its own commands and events: its manifest's, or
    /// its name without a manifest.
    namespace: String,
    /// The names of the events that it subscribed to.
    subscribed: Vec<String>,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    /// Its bindings of keys taken from every app, each of its command.
    global_bindings: Vec<(keymap::Combo, Invocation)>,
    /// Its items of the menus, in their order.
    menu_items: Vec<host::MenuItem>,
    /// Its catalogs, of its commands' titles and its messages.
    catalogs: lokyr_plugin_sdk::plugin::Catalogs,
    /// The last tree of each of its views that it rendered, `None` for a
    /// view that shows nothing, which stays as it is disabled, with the
    /// render's number, which no other render took.
    views: BTreeMap<ViewId, (u64, Option<Node>)>,
    /// Its open panels, each in its window.
    panels: BTreeMap<PanelId, WindowId>,
    /// The decorations that it laid, by their ids, each of its pane.
    decorations: BTreeMap<u64, host::PaneId>,
    /// The marks of the panes' gutters that it laid, by their ids, each of
    /// its pane.
    gutter_marks: BTreeMap<u64, host::PaneId>,
    /// The panes where it shows a cursor of its own.
    held_cursors: BTreeSet<host::PaneId>,
    /// What it keeps from idle sleep, which the app holds as it is active.
    awake: host::Awake,
    /// What its images may still draw, and when it last asked for one.
    images: (image::Budget, Option<Instant>),
    /// The quick windows that it opened, which close as it stops.
    quick_windows: Vec<WindowId>,
    /// Its calls on the main thread that took long, since the last look:
    /// each callback's name, and how long.
    slow: Vec<(&'static str, Duration)>,
    /// What disabled it, if something did: its error's message.
    fault: Option<String>,
    /// The schema of its section of the config, if it has keys.
    schema: Option<Schema>,
    /// Its config in force: a map of its keys' values.
    config: Data,
    /// Its state on the disk.
    stored: Stored,
    /// Whether its section of the config turns it on, as by default.
    wanted: bool,
    /// Whether it was activated, as a plugin turned on again is made anew.
    ran: bool,
    /// Where its tab bar goes in the windows that show it.
    bar_place: host::BarPlace,
    /// The fade of the panes that have not the keyboard that it asked for,
    /// and whether it asked that the keyboard follow the pointer.
    dim: f32,
    follows: bool,
    /// Where its tab bar goes in windows of a place of their own, as a
    /// window of a single tab shows its title, which goes as the window
    /// closes.
    window_places: BTreeMap<host::WindowId, host::BarPlace>,
    /// Of a plugin that does not read the processes, what it last learned
    /// of each tab, which a change of the tab's folder or program, that it
    /// cannot read, leaves.
    told_tabs: BTreeMap<host::TabId, host::TabInfo>,
    /// Of a plugin that does not read the processes, the title that it last
    /// learned of each pane, its program's, of none at first, which a
    /// change of the pane's folder or program, that it cannot read, leaves.
    told_panes: BTreeMap<host::PaneId, Option<String>>,
    /// Its searches of the panes' terminals, a pane's last alone.
    searches: Vec<search::Search>,
    /// Its restarts after its faults: the times of its faults within the
    /// last [`FAULTS_WITHIN`], when it starts again, if it does, and
    /// whether its last fault was counted.
    restarts: Restarts,
    /// How many of its reads of rows are under way, whose threads have not
    /// ended, [`MOST_READS`] at most.
    reading: Arc<AtomicUsize>,
    /// Its searches that showed their matches, stopped as a later one of
    /// their pane started, whose matches show until the later one shows
    /// its own or ends, so that the view does not lose them in between.
    lingering: Vec<search::Search>,
    /// Its searches that a later one of their pane replaced in the call
    /// under way, which stop, or linger, as the call returns, and run on of
    /// a call that fails, as if it had not been.
    replaced: Vec<search::Search>,
    /// Its timers, by their ids, [`host::MOST_TIMERS`] at most.
    timers: BTreeMap<u64, Timer>,
    /// What activates it, of its manifest's `activation.on`: the app's
    /// start, as of none.
    triggers: Vec<Trigger>,
    /// The capabilities that its manifest declares and that no decision of
    /// the user's grants or refuses, which its activation asks for first.
    undecided: Vec<Capability>,
    /// Whether the user is asked for them, its activation waiting.
    asking: bool,
    /// What its trigger brought as the user was asked, which comes once it
    /// activates.
    deferred: Deferred,
    /// The runs of its commands that woke it, on a thread of its own, which
    /// run as its activation ends, of their arguments checked by its own
    /// commands.
    woken: Vec<(String, Args, Chain)>,
}

/// What a plugin's trigger brought as its activation waited for the user's
/// consent: the commands to run, [`MOST_HELD_RUNS`] at most, the events to
/// hear, the first, which woke it, and the last of each name and subject
/// after it, [`MOST_HELD_EVENTS`] at most, and the views to render.
#[derive(Default)]
struct Deferred {
    commands: Vec<(String, Args, Chain)>,
    events: Vec<Event>,
    renders: Vec<ViewId>,
    /// Whether the first of `events` woke the plugin, which nothing takes
    /// the place of: none of a plugin that a command or a view woke.
    waking: bool,
}

/// The most events that a plugin's question holds, the one that woke it
/// among them.
pub const MOST_HELD_EVENTS: usize = 64;

/// The most runs of its commands that a plugin's question holds.
pub const MOST_HELD_RUNS: usize = 16;

impl Deferred {
    /// Holds `event`: the first kept, if nothing else woke the plugin
    /// before it, as it did; a later one in the place of the one before it
    /// of its name and subject, the oldest after the first going past
    /// [`MOST_HELD_EVENTS`].
    fn hold(&mut self, event: Event) {
        if self.events.is_empty() && self.commands.is_empty() && self.renders.is_empty() {
            self.waking = true;
            self.events.push(event);
            return;
        }
        let first = usize::from(self.waking);
        let subject = subject_of(&event);
        if let Some(at) = self.events[first..]
            .iter()
            .position(|kept| kept.name() == event.name() && subject_of(kept) == subject)
        {
            self.events.remove(at + first);
        }
        self.events.push(event);
        if self.events.len() > MOST_HELD_EVENTS {
            self.events.remove(first);
        }
    }
}

/// The subject of `event`: the ids of the window, the tab, the pane, the
/// panel, the view and the command that it is about, as its fields name
/// them, so that two events of a name about two panes, or two commands of
/// a pane, are two.
fn subject_of(event: &Event) -> Vec<(String, serde_json::Value)> {
    fn gather(
        value: &serde_json::Value,
        depth: usize,
        into: &mut Vec<(String, serde_json::Value)>,
    ) {
        let serde_json::Value::Object(fields) = value else {
            return;
        };
        for (key, field) in fields {
            if ["window", "tab", "pane", "panel", "view", "command", "id"].contains(&key.as_str())
                && !field.is_object()
            {
                into.push((key.clone(), field.clone()));
            } else if depth < 3 {
                gather(field, depth + 1, into);
            }
        }
    }
    let mut subject = Vec::new();
    if let Ok(value) = serde_json::to_value(event) {
        gather(&value, 0, &mut subject);
    }
    subject.sort_by(|left, right| left.0.cmp(&right.0));
    subject
}

/// What activates a plugin, of its manifest's `activation.on`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Trigger {
    /// The app's start.
    Startup,
    /// A run of a command whose name the pattern matches, of `*` for any
    /// text.
    Command(String),
    /// An event of the name.
    Event(String),
    /// A render of a view of the slot of the name.
    Slot(String),
}

impl Trigger {
    /// The triggers of `on`, a manifest's activation, of the plugin `name`
    /// of `namespace`; the app's start of none; an error of one that the
    /// app does not take yet, or of a command's of another namespace than
    /// its own.
    fn of(name: &str, namespace: &str, on: &[String]) -> Result<Vec<Self>, Error> {
        if on.is_empty() {
            return Ok(vec![Self::Startup]);
        }
        on.iter()
            .map(|on| match on.split_once(':') {
                None if on == "startup" => Ok(Self::Startup),
                Some(("command", pattern))
                    if lokyr_pkg_model::manifest::is_of_namespace(pattern, namespace) =>
                {
                    Ok(Self::Command(pattern.to_owned()))
                }
                Some(("command", _)) => Err(Error::new(
                    Code::InvalidConfig,
                    format!(
                        "the plugin `{name}` activates at `{on}`, a command of another namespace than its own, `{namespace}`"
                    ),
                )),
                Some(("event", event)) => Ok(Self::Event(event.to_owned())),
                Some(("slot", slot)) => Ok(Self::Slot(slot.to_owned())),
                Some(("osc", _)) => Err(Error::new(
                    Code::Unavailable,
                    format!(
                        "the plugin `{name}` activates at `{on}`, which comes with the handlers of OSC sequences, in a later Lokyr"
                    ),
                )),
                _ => Err(Error::new(
                    Code::InvalidConfig,
                    format!("the plugin `{name}` activates at `{on}`, which is no activation"),
                )),
            })
            .collect()
    }
}

/// Whether `pattern`, of `*` for any text, matches `text` whole.
fn matches_pattern(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

/// A timer of a plugin's.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Timer {
    /// When it fires next; none once it fired, of one that does not repeat,
    /// whose event waits for its plugin.
    at: Option<Instant>,
    /// How long it waits again after it fires, of one that repeats.
    every: Option<Duration>,
    /// Whether it fired since its plugin last heard of it: its event waits,
    /// one however many times it fired meanwhile.
    due: bool,
}

impl State {
    /// Whether it waits for a trigger other than the app's start to
    /// activate.
    fn lazy(&self) -> bool {
        !self.triggers.contains(&Trigger::Startup)
    }

    /// Whether it waits for a trigger to activate, as it is: added, or
    /// turned on again, and lazy.
    fn waits(&self) -> bool {
        self.status == Status::Inactive && self.wanted && self.lazy()
    }

    /// Whether it reads the processes, whose folders and programs a tab's
    /// title tells it.
    fn reads_processes(&self) -> bool {
        self.granted
            .as_ref()
            .is_none_or(|granted| granted.contains(&Capability::ProcessRead))
    }

    /// Learns what it reads of every tab of `workspace`, as it starts, a
    /// plugin that does not read the processes.
    fn learn_tabs(&mut self, workspace: &Workspace) {
        self.told_tabs.clear();
        self.told_panes.clear();
        if self.reads_processes() {
            return;
        }
        for window in workspace.windows() {
            for &tab in window.tabs() {
                if let Some(shown) = workspace.tab(tab) {
                    self.told_tabs
                        .insert(tab.into(), tab_info(workspace, shown, false));
                }
            }
        }
    }

    /// Whether it hears `event`, of the workspace, of the tabs and the panes
    /// that `workspace` has: a tab's or a pane's change that it cannot read
    /// it does not, as a plugin that does not read the processes, which
    /// learns what it reads of each tab, its title, its color and its
    /// marks, and of each pane, its program's title.
    fn hears(&mut self, event: &Event, workspace: &Workspace) -> bool {
        if self.reads_processes() {
            return true;
        }
        let seen = |tab: host::TabId| {
            workspace
                .tab(crate::workspace::TabId::of(tab.0))
                .map(|tab| tab_info(workspace, tab, false))
        };
        match event {
            Event::Workspace(WorkspaceEvent::TabCreated { tab, .. }) => {
                if let Some(now) = seen(*tab) {
                    self.told_tabs.insert(*tab, now);
                }
                true
            }
            Event::Workspace(WorkspaceEvent::TabClosed { tab, .. }) => {
                self.told_tabs.remove(tab);
                true
            }
            Event::Workspace(WorkspaceEvent::TabChanged { tab }) => match seen(*tab) {
                Some(now) => self.told_tabs.insert(*tab, now.clone()) != Some(now),
                None => true,
            },
            Event::Workspace(WorkspaceEvent::PaneChanged { pane, .. }) => {
                match workspace.pane(crate::workspace::PaneId::of(pane.0)) {
                    Some(found) => {
                        let now = found.header_title(false);
                        self.told_panes.insert(*pane, now.clone()).flatten() != now
                    }
                    None => true,
                }
            }
            Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => {
                self.told_panes.remove(pane);
                true
            }
            _ => true,
        }
    }
}

/// A plugin's state on the disk: its file, in the app's state directory,
/// and what it holds once read or written.
#[derive(Debug, Default)]
struct Stored {
    file: Option<PathBuf>,
    held: Option<Data>,
}

impl State {
    /// Whether its manifest declares that it fills `slot`.
    fn fills(&self, slot: Slot) -> bool {
        self.manifest.as_ref().is_some_and(|manifest| {
            manifest
                .contributes
                .slots
                .iter()
                .any(|name| name == slot.name())
        })
    }
}

/// The plugins, in the order of their activation, and what their calls
/// asked of the app.
#[derive(Default)]
pub struct Plugins {
    entries: Vec<Entry>,
    /// What the calls asked and emitted.
    out: Outbox,
    /// Whether the plugins' commands or bindings changed since the last
    /// look.
    declarations_changed: bool,
    /// The plugins disabled that [`Plugins::take_disabled`] told.
    told_disabled: std::collections::BTreeSet<String>,
    /// What wakes the main thread as the plugins' threads send it
    /// messages, if the app gave it.
    wake: Option<worker::Wake>,
    /// The plugin whose thread's messages the next turn takes first, after
    /// the one that the last turn's time ran out at.
    serve_from: usize,
    /// How long a turn takes the plugins' threads' messages at most,
    /// [`SERVE_FOR`] but in the tests.
    serve_for: Option<Duration>,
    /// The app's state directory, which the plugins' states are kept in.
    state_directory: Option<PathBuf>,
    /// The capabilities that the host withholds and that a declared
    /// plugin's manifest asked for, by the plugin's name, which
    /// [`Plugins::take_withheld`] tells.
    withheld: Vec<(String, Capability)>,
}

/// What a plugin's section of the config tells the user, of the plugin's
/// namespace: why it was refused, or what it ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigNotice {
    /// The namespace, of `[plugins.<namespace>]`.
    pub namespace: String,
    /// Whether the section was refused, which leaves the plugin's config in
    /// force as it was.
    pub refused: bool,
    /// What it tells, of the file's place, as `config.toml:3:1: …`.
    pub text: String,
}

pub use lokyr_plugin_sdk::host::MOST_STATE;

/// A call of a plugin's on the main thread that took long, as the log
/// tells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlowCall {
    /// The plugin.
    pub plugin: String,
    /// The callback, as `on_command`.
    pub callback: &'static str,
    /// How long it took.
    pub took: Duration,
}

/// What a turn of [`Plugins::serve`] took of the plugins' threads and the
/// reads of rows.
#[derive(Debug, Default)]
pub struct Served {
    /// Whether it took a message, or a read's end, whose changes the app
    /// then takes, as [`Plugins::take_requests`] and the others give them.
    pub any: bool,
    /// The failures of the commands that ran on plugins' threads, as the
    /// user is told of a command's.
    pub commands: Vec<Error>,
    /// The other calls' errors, for the log, of which a panic's disabled
    /// its plugin.
    pub errors: Vec<Error>,
    /// Whether messages may wait still, for the next turn, which the host
    /// woke the main thread for, as the turn's time ran out.
    pub more: bool,
}

/// How long a call of a plugin's on the main thread takes at most before
/// the log tells of it: a frame's time at 120 Hz.
pub const SLOW: Duration = Duration::from_millis(8);

/// How long a turn of the main thread takes the messages of the plugins'
/// threads at most, those left waiting for the next turn, so that frames
/// and keys come between.
const SERVE_FOR: Duration = Duration::from_millis(1);

/// What plugins' calls send out, once they return: their requests of the
/// app's, since the app's last take, and the events to deliver, since the
/// last delivery, with the workspace's.
#[derive(Default)]
struct Outbox {
    requests: Vec<Request>,
    /// Each event, with the plugin that it goes to alone, if it is one's.
    events: Vec<(Option<String>, Event)>,
    /// The reads of rows under way, whose rows come as events.
    reads: Vec<Read>,
    /// Whether a call wrote to the kernel's state since the last look, as
    /// what the windows show may have changed.
    written: bool,
    /// The failures of the writes that the calls' returns made, as of the
    /// plugins' states, for the log.
    faults: Vec<Error>,
    /// The views whose renders the plugins asked for, or whose panels
    /// they opened, by their plugins, in their order.
    renders: Vec<(String, ViewId)>,
}

impl Outbox {
    /// Takes what a call of a plugin's thread sent out, as it returned,
    /// after what came before it.
    fn merge(&mut self, call: Outbox) {
        self.requests.extend(call.requests);
        self.events.extend(call.events);
        self.reads.extend(call.reads);
        self.written |= call.written;
        self.faults.extend(call.faults);
        for render in call.renders {
            if !self.renders.contains(&render) {
                self.renders.push(render);
            }
        }
    }
}

/// A read of a pane's rows, on a thread of the host's.
struct Read {
    id: u64,
    plugin: String,
    pane: host::PaneId,
    rows: mpsc::Receiver<(Vec<host::Line>, bool)>,
    /// What stops its thread at its next part, as the read goes.
    _cancel: Cancel,
}

/// What stops a read's thread at its next part once dropped, as its plugin
/// stops or a read of no plugin's waits no more.
struct Cancel(Arc<AtomicBool>);

impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// A write of a call, which the host applies as the call returns.
enum Write {
    /// One that the plugin asked, as it asked it.
    Asked(Asked),
    /// A tab in `window`, of the ids that the host gave it and its pane.
    NewTab {
        window: WindowId,
        index: Option<usize>,
        tab: TabId,
        pane: PaneId,
        directory: Option<String>,
    },
    /// A split of `pane`, of the id that the host gave the new pane.
    Split {
        pane: PaneId,
        direction: Direction,
        side: Side,
        new: PaneId,
    },
    /// A render of one of the call's plugin's views.
    Render(ViewId),
    /// A decoration, of the id that the host gave it.
    Decorate {
        id: u64,
        pane: host::PaneId,
        from: (u16, i64),
        to: (u16, i64),
        look: host::DecorationLook,
    },
    /// A mark of a pane's gutter, of the id that the host gave it.
    MarkGutter {
        id: u64,
        pane: host::PaneId,
        row: i64,
        color: (u8, u8, u8),
    },
}

/// The rows that a read takes under the lock at a time: about a frame's
/// copy's time.
const READ_PART: usize = 256;

/// The most reads of rows that a plugin has under way, each of a thread,
/// past which another is refused until one ends.
const MOST_READS: usize = 4;

pub use lokyr_plugin_sdk::host::{MOST_DECLARED, MOST_SEARCHES};
pub use lokyr_plugin_sdk::image::{IMAGES_BOUND, Image, ImageSpec, MOST_IMAGES};

/// How long a plugin that faulted waits before it starts again, its first
/// time, which each fault within [`FAULTS_WITHIN`] doubles.
const RESTART_AFTER: Duration = Duration::from_secs(1);

/// The faults of a plugin within [`FAULTS_WITHIN`] that turn it off, past
/// restarting, until the app's next start or its section turns it on.
const MOST_FAULTS: usize = 3;

/// The span of time that a plugin's faults count within.
const FAULTS_WITHIN: Duration = Duration::from_secs(60);

/// A plugin's restarts after its faults.
#[derive(Default)]
struct Restarts {
    /// The times of its faults within the last [`FAULTS_WITHIN`].
    faults: Vec<Instant>,
    /// When it starts again, if it does.
    at: Option<Instant>,
    /// Whether its last fault was counted.
    counted: bool,
}

/// The last id of a read, a search, a decoration or a timer, which none
/// takes twice.
static LAST_ID: AtomicU64 = AtomicU64::new(0);

/// An id that no read, search, decoration nor timer of the process took.
fn new_id() -> u64 {
    LAST_ID.fetch_add(1, Ordering::Relaxed) + 1
}

/// A new chain of commands, of an id of its own among every chain's, which
/// the commands that a call of no command's asks start.
pub fn new_chain() -> Chain {
    Chain {
        id: new_id(),
        depth: 0,
    }
}

impl std::fmt::Debug for Plugins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(
                self.entries
                    .iter()
                    .map(|entry| (&entry.state.name, entry.state.status)),
            )
            .finish()
    }
}

impl Plugins {
    /// No plugins.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `plugin`, by its `name`, without a manifest, to activate, with
    /// every capability, as a first-party plugin has them; its namespace is
    /// its name, which no other plugin's may be.
    pub fn add(&mut self, name: &str, plugin: Box<dyn Plugin>) -> Result<(), Error> {
        self.insert(name, None, Runner::Main(plugin), None)
    }

    /// Adds `plugin`, a first-party one, which the text `manifest`
    /// describes, by the manifest's name, to activate: with every
    /// capability and any command, as first-party plugins have them; its
    /// manifest's capabilities tell what it uses. A manifest that does not
    /// read, or of another plugin's namespace, or that asks for a thread of
    /// its own, which [`Plugins::add_builtin_send`] gives, adds nothing.
    pub fn add_builtin(&mut self, manifest: &str, plugin: Box<dyn Plugin>) -> Result<(), Error> {
        let manifest = builtin_manifest(manifest)?;
        let name = manifest.name.clone();
        self.insert(&name, Some(manifest), Runner::Main(plugin), None)
    }

    /// Adds `plugin`, as [`Plugins::add_builtin`] does, on the thread that
    /// its manifest asks for: one of its own, which starts as it
    /// activates, for `thread = "worker"`, or the main thread.
    pub fn add_builtin_send(
        &mut self,
        manifest: &str,
        plugin: Box<dyn Plugin + Send>,
    ) -> Result<(), Error> {
        let manifest = builtin_manifest(manifest)?;
        let name = manifest.name.clone();
        let runner = match manifest.thread {
            Thread::Worker => Runner::Unstarted(plugin),
            Thread::Main => Runner::Main(plugin),
        };
        self.insert(&name, Some(manifest), runner, None)
    }

    /// Adds the plugin of `registration`, a built-in one, as
    /// [`Plugins::add_builtin_send`] does, which its config can turn off,
    /// and on again, a new one of it.
    pub fn add_registered(&mut self, registration: Registration) -> Result<(), Error> {
        self.add_builtin_send(registration.manifest, (registration.make)())?;
        if let Some(entry) = self.entries.last_mut() {
            let worker = entry
                .state
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.thread == Thread::Worker);
            entry.make = Some((Box::new(registration.make), worker));
        }
        Ok(())
    }

    /// Adds `plugin`, which `manifest` describes, by the manifest's name,
    /// to activate, with what the manifest declares: its capabilities, but
    /// those that the host withholds, the network's and the processes',
    /// which [`Plugins::take_withheld`] tells, and the commands of its
    /// `[uses]` with those that it registers, of its own namespace alone; a
    /// request that needs another capability, or a run of another command,
    /// fails. One of another plugin's namespace adds nothing.
    pub fn add_declared(
        &mut self,
        manifest: Manifest,
        plugin: Box<dyn Plugin>,
    ) -> Result<(), Error> {
        self.add_declared_on(manifest, Runner::Main(plugin))
    }

    /// Adds the plugin of `runner`, as [`Plugins::add_declared`] does.
    fn add_declared_on(&mut self, manifest: Manifest, runner: Runner) -> Result<(), Error> {
        let name = manifest.name.clone();
        let (withheld, capabilities): (Vec<Capability>, Vec<Capability>) = manifest
            .capabilities
            .iter()
            .cloned()
            .partition(|capability| withheld_from(&manifest, capability));
        let granted = (capabilities, Uses::new(manifest.uses.clone()));
        self.insert(&name, Some(manifest), runner, Some(granted))?;
        self.withheld.extend(
            withheld
                .into_iter()
                .map(|capability| (name.clone(), capability)),
        );
        Ok(())
    }

    /// Adds the plugin that `make` makes, an installed one, which
    /// `manifest` describes, as [`Plugins::add_declared`] does, on a thread
    /// of its own, which starts as it activates: at the app's start, or at
    /// its trigger, as its manifest's activation says, its commands showing
    /// before. `make` makes it anew as a fault's while passes or its
    /// section turns it on again, as a registration's function does.
    /// It holds the capabilities that the user's decisions grant, of
    /// `granted`; those that they neither grant nor refuse, of `refused`,
    /// its activation asks the user for first, as a [`Request::AskConsent`].
    pub fn add_installed(
        &mut self,
        manifest: Manifest,
        make: Make,
        (granted, refused): (&[Capability], &[Capability]),
    ) -> Result<(), Error> {
        self.add_declared_on(manifest, Runner::Unstarted(make()))?;
        if let Some(entry) = self.entries.last_mut() {
            decide(&mut entry.state, granted, refused);
            entry.make = Some((make, true));
        }
        Ok(())
    }

    /// Takes the plugin `name` away, as it was uninstalled, or to be made
    /// anew of a new version or its folder's changes: it stops, as its
    /// section turning it off stops it, its commands, keys, views, panels
    /// and decorations going and its thread ending, and the host forgets
    /// it. Whether it was there.
    pub fn remove(&mut self, name: &str, context: &mut Context<'_>) -> bool {
        let Some(at) = self
            .entries
            .iter()
            .position(|entry| entry.state.name == name)
        else {
            return false;
        };
        let mut entry = self.entries.remove(at);
        stop(
            &mut entry,
            context,
            &mut self.out,
            &mut self.declarations_changed,
        );
        // What waited for it goes with it, as its panels' closing goes to
        // the app.
        self.out
            .events
            .retain(|(to, _)| to.as_deref() != Some(name));
        self.out.renders.retain(|(plugin, _)| plugin != name);
        self.told_disabled.remove(name);
        self.serve_from = 0;
        self.out.written = true;
        true
    }

    /// Gives the installed plugin `name` the decisions of the user's, as
    /// they changed, as [`Plugins::add_installed`] takes them: one that
    /// held a capability that they no longer grant, as the user revoked
    /// it, stops, as a fault stops it, until the app's next start. Whether
    /// it stopped.
    pub fn set_consent(
        &mut self,
        name: &str,
        (granted, refused): (&[Capability], &[Capability]),
        context: &mut Context<'_>,
    ) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.state.name == name)
        else {
            return false;
        };
        let held = entry.state.granted.clone().unwrap_or_default();
        decide(&mut entry.state, granted, refused);
        let now = entry.state.granted.clone().unwrap_or_default();
        let lost: Vec<String> = held
            .iter()
            .filter(|capability| !now.contains(capability))
            .map(ToString::to_string)
            .collect();
        if lost.is_empty() || entry.state.status != Status::Active {
            return false;
        }
        let error = Error::new(
            Code::Fault,
            format!(
                "the plugin `{name}` lost {}, which the user revoked; it starts again at the app's next start",
                lost.iter()
                    .map(|capability| format!("`{capability}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        let Entry { runner, state, .. } = entry;
        disable(state, context.registry, &mut self.declarations_changed);
        clear_leftovers(state, context, &mut self.out);
        state.fault = Some(error.message);
        // Not a fault of its own, which would start it again after a while.
        state.restarts.counted = true;
        if let Runner::Worker(_) = runner {
            *runner = Runner::Stopped;
        }
        true
    }

    /// Gives the plugin `name`, whose activation asked the user for its
    /// capabilities, the answer of the question of `answered`: those of
    /// them that it asks for still granted, or refused, which it runs
    /// without; then it activates, and what its trigger brought comes, its
    /// commands run, its events heard and its views rendered. One that asks
    /// for others than the question's, as a new version of it, is asked
    /// again of them. The errors of its activation and its commands, as
    /// they fail at once.
    pub fn consent(
        &mut self,
        name: &str,
        answered: &[Capability],
        allowed: bool,
        context: &mut Context<'_>,
    ) -> Vec<Error> {
        let Some(at) = self
            .entries
            .iter()
            .position(|entry| entry.state.name == name && entry.state.asking)
        else {
            return Vec::new();
        };
        let state = &mut self.entries[at].state;
        state.asking = false;
        let (decided, left): (Vec<Capability>, Vec<Capability>) =
            std::mem::take(&mut state.undecided)
                .into_iter()
                .partition(|capability| answered.contains(capability));
        if let Some(granted) = &mut state.granted {
            if allowed {
                granted.extend(decided);
            } else {
                granted.retain(|capability| !decided.contains(capability));
            }
        }
        state.undecided = left;
        if state.status != Status::Inactive || !state.wanted {
            state.deferred = Deferred::default();
            return Vec::new();
        }
        let mut errors = Vec::new();
        // Asked again of the others, what its trigger brought waiting.
        if !state.undecided.is_empty() {
            if let Err(error) = self.waken(at, context) {
                errors.push(error);
            }
            return errors;
        }
        let deferred = std::mem::take(&mut self.entries[at].state.deferred);
        if let Err(error) = self.waken(at, context) {
            errors.push(error);
            return errors;
        }
        // Of the events that the plugin may subscribe to, of the
        // capabilities that it holds now, as a subscription takes them.
        let granted = self.entries[at].state.granted.clone();
        for event in deferred.events {
            let needed =
                lokyr_plugin_sdk::message::Request::Subscribe(vec![event.name().to_owned()])
                    .capabilities();
            let may = granted
                .as_ref()
                .is_none_or(|granted| needed.iter().all(|capability| granted.contains(capability)));
            if may {
                self.out.events.push((Some(name.to_owned()), event));
            }
        }
        for view in deferred.renders {
            let render = (name.to_owned(), view);
            if !self.out.renders.contains(&render) {
                self.out.renders.push(render);
            }
        }
        for (command, args, chain) in deferred.commands {
            if let Err(error) = self.run_woken(at, &command, args, chain, context) {
                errors.push(error);
            }
        }
        errors
    }

    /// Takes the capabilities that the host withheld from the declared
    /// plugins since the last take, by each plugin's name, which the user
    /// is told of: the network's and the processes', which no plugin has
    /// yet.
    pub fn take_withheld(&mut self) -> Vec<(String, Capability)> {
        std::mem::take(&mut self.withheld)
    }

    fn insert(
        &mut self,
        name: &str,
        manifest: Option<Manifest>,
        runner: Runner,
        declared: Option<(Vec<Capability>, Uses)>,
    ) -> Result<(), Error> {
        if matches!(runner, Runner::Main(_))
            && manifest
                .as_ref()
                .is_some_and(|manifest| manifest.thread == Thread::Worker)
        {
            return Err(Error::new(
                Code::InvalidConfig,
                format!(
                    "the plugin `{name}` asks for a thread of its own, which a plugin that cannot be sent to one does not have"
                ),
            ));
        }
        let namespace = manifest
            .as_ref()
            .map_or(name, Manifest::namespace)
            .to_owned();
        // The namespaces of the app's own contexts, which a plugin's panels
        // would give as `namespace.focused`, holding as the app's.
        if crate::keymap::CONTEXTS
            .iter()
            .any(|context| context.split('.').next() == Some(namespace.as_str()))
        {
            return Err(Error::new(
                Code::AlreadyRegistered,
                format!(
                    "the plugin `{name}` takes the namespace `{namespace}`, of the app's own contexts"
                ),
            ));
        }
        if let Some(other) = self
            .entries
            .iter()
            .find(|entry| entry.state.namespace == namespace)
        {
            return Err(Error::new(
                Code::AlreadyRegistered,
                format!(
                    "the plugin `{name}` takes the namespace `{namespace}`, which is `{}`'s",
                    other.state.name
                ),
            ));
        }
        // A plugin whose catalogs panic has none.
        let catalogs = panic::catch_unwind(AssertUnwindSafe(|| match &runner {
            Runner::Main(plugin) => plugin.catalogs(),
            Runner::Unstarted(plugin) => plugin.catalogs(),
            Runner::Worker(_) | Runner::Stopped => Vec::new(),
        }))
        .unwrap_or_default();
        let triggers = Trigger::of(
            name,
            &namespace,
            manifest
                .as_ref()
                .map_or(&[][..], |manifest| &manifest.activation),
        )?;
        let schema = self::schema(name, manifest.as_ref(), &runner)?;
        let config = schema.as_ref().map_or_else(Map::new, Schema::defaults);
        let file = self
            .state_directory
            .as_ref()
            .map(|directory| state_file(directory, &namespace));
        let (granted, uses) = declared.unzip();
        self.entries.push(Entry {
            runner,
            make: None,
            state: State {
                name: name.to_owned(),
                manifest,
                status: Status::Inactive,
                granted,
                uses,
                namespace,
                subscribed: Vec::new(),
                commands: Vec::new(),
                bindings: Vec::new(),
                global_bindings: Vec::new(),
                menu_items: Vec::new(),
                catalogs,
                views: BTreeMap::new(),
                panels: BTreeMap::new(),
                decorations: BTreeMap::new(),
                gutter_marks: BTreeMap::new(),
                held_cursors: BTreeSet::new(),
                awake: host::Awake::Off,
                images: (image::Budget::default(), None),
                quick_windows: Vec::new(),
                slow: Vec::new(),
                fault: None,
                schema,
                config: data(&Value::Object(config)),
                stored: Stored { file, held: None },
                wanted: true,
                ran: false,
                bar_place: host::BarPlace::default(),
                window_places: BTreeMap::new(),
                dim: 0.0,
                follows: false,
                told_tabs: BTreeMap::new(),
                told_panes: BTreeMap::new(),
                searches: Vec::new(),
                reading: Arc::default(),
                restarts: Restarts::default(),
                lingering: Vec::new(),
                replaced: Vec::new(),
                timers: BTreeMap::new(),
                triggers,
                undecided: Vec::new(),
                asking: false,
                deferred: Deferred::default(),
                woken: Vec::new(),
            },
        });
        // A lazy plugin's commands, of its manifest, show in the menus and
        // the palette before it activates.
        if let Some(state) = self.entries.last().map(|entry| &entry.state)
            && state.lazy()
            && state
                .manifest
                .as_ref()
                .is_some_and(|manifest| !manifest.contributes.commands.is_empty())
        {
            self.declarations_changed = true;
        }
        Ok(())
    }

    /// Gives the host the app's state directory, which the plugins' states
    /// are kept in, as `plugins/<namespace>.json`: before their activation,
    /// which reads them.
    pub fn set_state_directory(&mut self, directory: Option<PathBuf>) {
        for entry in &mut self.entries {
            let state = &mut entry.state;
            state.stored = Stored {
                file: directory
                    .as_ref()
                    .map(|directory| state_file(directory, &state.namespace)),
                held: None,
            };
        }
        self.state_directory = directory;
    }

    /// Checks the plugins' sections of `config` against their schemas: a
    /// section that one refuses leaves its plugin's config in force; one
    /// that it takes, with the defaults of the keys that it leaves out,
    /// becomes the plugin's, which an [`Event::ConfigChanged`] tells an
    /// active plugin as it changes. What the sections tell the user, a
    /// notice of each plugin's at most: a section refused, its keys that
    /// the schema lacks, or a section of no plugin's or of a plugin of no
    /// keys, which it ignores.
    pub fn configure(&mut self, config: &PluginsConfig) -> Vec<ConfigNotice> {
        let mut notices = Vec::new();
        for entry in &mut self.entries {
            let state = &mut entry.state;
            let namespace = state.namespace.clone();
            let section = config.get(&namespace);
            let at = |path: &[String]| {
                section
                    .map(|section| section.at(path) + ": ")
                    .unwrap_or_default()
            };
            // `enabled`, the host's key of every section, which turns its
            // plugin off, and on again.
            let mut values = section
                .map(|section| section.values().clone())
                .unwrap_or_default();
            match values.remove(ENABLED) {
                None => state.wanted = true,
                Some(Value::Bool(enabled)) => state.wanted = enabled,
                Some(other) => {
                    notices.push(ConfigNotice {
                        text: format!(
                            "{}`{ENABLED}` is true or false, not {other} in `[plugins.{namespace}]`; the plugin `{}` stays as it is",
                            at(&[ENABLED.to_owned()]),
                            state.name
                        ),
                        namespace,
                        refused: true,
                    });
                    continue;
                }
            }
            // A plugin turned off takes no keys.
            if !state.wanted {
                continue;
            }
            // A value that JSON does not hold refuses the section alone.
            if let Some((path, message)) = section.and_then(PluginSection::unread) {
                notices.push(ConfigNotice {
                    text: format!(
                        "{}{message} in `[plugins.{namespace}]`; the plugin `{}` keeps its config in force",
                        at(path),
                        state.name
                    ),
                    namespace,
                    refused: true,
                });
                continue;
            }
            let Some(schema) = &state.schema else {
                if !values.is_empty() {
                    notices.push(ConfigNotice {
                        text: format!(
                            "{}the plugin `{}` has no keys, and `[plugins.{namespace}]` is ignored",
                            at(&[]),
                            state.name
                        ),
                        namespace,
                        refused: false,
                    });
                }
                continue;
            };
            let checked = schema.check(&values);
            match checked.values {
                Ok(values) => {
                    let values = data(&Value::Object(values));
                    if values != state.config {
                        state.config = values;
                        if state.status == Status::Active {
                            self.out
                                .events
                                .push((Some(state.name.clone()), Event::ConfigChanged));
                        }
                    }
                    if let Some(first) = checked.ignored.first() {
                        let known = schema.keys();
                        let more = match checked.ignored.len() {
                            1 => String::new(),
                            count => format!(", and {} more", count - 1),
                        };
                        notices.push(ConfigNotice {
                            text: format!(
                                "{}unknown key `{first}` in `[plugins.{namespace}]`{more}, which the plugin `{}` ignores{}",
                                at(std::slice::from_ref(first)),
                                state.name,
                                crate::config::nearest(first, &known, "", ""),
                            ),
                            namespace,
                            refused: false,
                        });
                    }
                }
                Err(problem) => notices.push(ConfigNotice {
                    text: format!(
                        "{}{} in `[plugins.{namespace}]`; the plugin `{}` keeps its config in force",
                        at(&problem.path),
                        problem.message,
                        state.name
                    ),
                    namespace,
                    refused: true,
                }),
            }
        }
        for (namespace, section) in config.sections() {
            if !self
                .entries
                .iter()
                .any(|entry| entry.state.namespace == namespace)
            {
                notices.push(ConfigNotice {
                    namespace: namespace.to_owned(),
                    refused: false,
                    text: format!(
                        "{}: no plugin of the namespace `{namespace}` is here, and `[plugins.{namespace}]` is ignored",
                        section.at(&[])
                    ),
                });
            }
        }
        notices
    }

    /// Takes the failures of the writes that the plugins' calls made as
    /// they returned, as of their states, for the log.
    pub fn take_faults(&mut self) -> Vec<Error> {
        std::mem::take(&mut self.out.faults)
    }

    /// Gives the host what wakes the main thread, as the plugins' threads
    /// send it messages, which [`Plugins::serve`] takes: before their
    /// activation, which starts them.
    pub fn set_wake(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.wake = Some(Arc::new(wake));
    }

    /// Activates the plugins that are not active yet, in their order, and
    /// returns the errors of those whose activation failed or panicked,
    /// which are disabled. A plugin of a thread of its own starts it, and
    /// activates there: its failure comes through [`Plugins::serve`]. A
    /// plugin that its section of the config, as [`Plugins::configure`]
    /// took it, turns off stops, its commands, bindings, views, panels and
    /// decorations going; one that it turns on again activates, a new one
    /// of its registration if it ran.
    pub fn activate(&mut self, context: &mut Context<'_>) -> Vec<Error> {
        let mut errors = Vec::new();
        for entry in &mut self.entries {
            // A plugin that its section turns off stops; one that it turns
            // on again starts, a new one of it if it ran.
            if !entry.state.wanted && entry.state.status != Status::Off {
                if matches!(entry.state.status, Status::Active | Status::Disabled) {
                    stop(
                        entry,
                        context,
                        &mut self.out,
                        &mut self.declarations_changed,
                    );
                }
                entry.state.status = Status::Off;
                self.out.written = true;
                continue;
            }
            if entry.state.wanted && entry.state.status == Status::Off {
                if entry.state.ran {
                    let Some((make, worker)) = &entry.make else {
                        errors.push(Error::new(
                            Code::Unavailable,
                            format!(
                                "the plugin `{}` starts again at the app's next start",
                                entry.state.name
                            ),
                        ));
                        continue;
                    };
                    entry.runner = if *worker {
                        Runner::Unstarted(make())
                    } else {
                        Runner::Main(make())
                    };
                    // A fault of the new one is told again, and counted.
                    entry.state.fault = None;
                    entry.state.restarts.counted = false;
                    self.told_disabled.remove(&entry.state.name);
                }
                entry.state.status = Status::Inactive;
                self.out.written = true;
            }
            // A lazy plugin waits for its trigger.
            if entry.state.status != Status::Inactive || entry.state.lazy() {
                continue;
            }
            if let Err(error) = start(
                entry,
                context,
                (&mut self.out, &mut self.declarations_changed),
                self.wake.as_ref(),
            ) {
                errors.push(error);
            }
        }
        errors
    }

    /// Whether an active plugin registered `command`, or a plugin that
    /// waits for its trigger runs it, as its manifest's commands or its
    /// `command:` activation say, which its run activates.
    pub fn owns(&self, command: &str) -> bool {
        self.owner(command).is_some() || self.lazy_owner(command).is_some()
    }

    /// The plugin that waits for its trigger that runs `command`, as its
    /// manifest's commands or its `command:` activation say, or that
    /// activates on its thread, as such a run woke it.
    fn lazy_owner(&self, command: &str) -> Option<usize> {
        self.entries.iter().position(|entry| {
            let state = &entry.state;
            (state.waits() || activating(&entry.runner))
                && (state.triggers.iter().any(|trigger| {
                    matches!(trigger, Trigger::Command(pattern) if matches_pattern(pattern, command))
                }) || state.manifest.as_ref().is_some_and(|manifest| {
                    manifest
                        .contributes
                        .commands
                        .iter()
                        .any(|contributed| contributed.name == command)
                }))
        })
    }

    /// Activates the plugin at `at`, which waits for its trigger, which
    /// came: its error, if its activation failed at once.
    fn waken(&mut self, at: usize, context: &mut Context<'_>) -> Result<(), Error> {
        let wake = self.wake.clone();
        start(
            &mut self.entries[at],
            context,
            (&mut self.out, &mut self.declarations_changed),
            wake.as_ref(),
        )
    }

    fn owner(&self, command: &str) -> Option<usize> {
        self.entries.iter().position(|entry| {
            entry.state.status == Status::Active
                && entry.state.commands.iter().any(|ours| ours.name == command)
        })
    }

    /// Runs `command`, which an active plugin registered, with `args`,
    /// which the registry checked: the plugin's own error, or a fault when
    /// it panicked, which disables it.
    pub fn run(
        &mut self,
        command: &str,
        args: Args,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        self.run_deep(command, args, Chain::default(), context)
    }

    /// Runs `command` as [`Plugins::run`] does, of the chain `chain`, as
    /// commands ran it: the commands that its call runs, whenever it ends,
    /// are [`Request::Run`]s of that chain.
    pub fn run_deep(
        &mut self,
        command: &str,
        args: Args,
        chain: Chain,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        if let Some(at) = self.owner(command) {
            return self.run_on(at, command, args, chain, context);
        }
        // A plugin that waits for its trigger activates, of the main thread
        // at once, or of a thread of its own, which runs the command after
        // its activation.
        let Some(at) = self.lazy_owner(command) else {
            return Err(Error::new(
                Code::NotFound,
                format!("no plugin runs `{command}`"),
            ));
        };
        if self.entries[at].state.waits() {
            self.waken(at, context)?;
        }
        // Its activation waits for the user's consent: the command runs
        // after it.
        let state = &mut self.entries[at].state;
        if state.asking {
            if state.deferred.commands.len() >= MOST_HELD_RUNS {
                return Err(Error::new(
                    Code::Unavailable,
                    format!(
                        "`{command}` waits for the user's consent to its plugin's capabilities, as {MOST_HELD_RUNS} runs do already"
                    ),
                ));
            }
            state
                .deferred
                .commands
                .push((command.to_owned(), args, chain));
            return Ok(());
        }
        self.run_woken(at, command, args, chain, context)
    }

    /// Runs `command` with `args`, of the chain `chain`, on the plugin at
    /// `at`, which its trigger woke: a plugin of the main thread, which
    /// activated, of its own command, which checks the arguments, as the
    /// command that its manifest gave, of none, took any; one of a thread
    /// of its own as its activation ends, which [`Plugins::serve`] tells.
    fn run_woken(
        &mut self,
        at: usize,
        command: &str,
        args: Args,
        chain: Chain,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        let Entry { runner, state, .. } = &mut self.entries[at];
        if let Runner::Worker(worker) = runner
            && worker.activating()
        {
            state.woken.push((command.to_owned(), args, chain));
            return Ok(());
        }
        let args = own_call(state, command, args)?;
        self.run_on(at, command, args, chain, context)
    }

    /// Runs `command` with `args`, of the chain `chain`, on the plugin at
    /// `at`, as [`Plugins::run_deep`] does.
    fn run_on(
        &mut self,
        at: usize,
        command: &str,
        args: Args,
        chain: Chain,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        let Entry { runner, state, .. } = &mut self.entries[at];
        match runner {
            Runner::Main(plugin) => {
                let before = self.out.requests.len();
                let ran = call(
                    (plugin.as_mut(), state),
                    context,
                    &mut self.out,
                    &mut self.declarations_changed,
                    "on_command",
                    |plugin, cx| plugin.on_command(cx, command, args),
                );
                deepen(&mut self.out.requests[before..], chain);
                ran
            }
            // Its failure comes as its call ends.
            Runner::Worker(worker) => {
                let within = worker.ask(
                    Job::Command(command.to_owned(), args, chain),
                    context.focused,
                );
                within_bound(
                    within,
                    (runner, state),
                    context,
                    (&mut self.out, &mut self.declarations_changed),
                )
            }
            Runner::Unstarted(_) | Runner::Stopped => Ok(()),
        }
    }

    /// Whether `command`, which an active plugin registered, would do
    /// something with `args`, which the registry checked, as its plugin
    /// tells: what the call asks of the app is dropped; a fault when it
    /// panicked, which disables it. A plugin of a thread of its own is not
    /// asked, as the main thread does not wait for it: its commands apply.
    pub fn applies(
        &mut self,
        command: &str,
        args: &Args,
        context: &mut Context<'_>,
    ) -> Result<bool, Error> {
        let Some(at) = self.owner(command) else {
            // A plugin that waits for its trigger is not activated for a
            // menu's question: its commands apply.
            if self.lazy_owner(command).is_some() {
                return Ok(true);
            }
            return Err(Error::new(
                Code::NotFound,
                format!("no plugin runs `{command}`"),
            ));
        };
        let Entry { runner, state, .. } = &mut self.entries[at];
        let Runner::Main(plugin) = runner else {
            return Ok(true);
        };
        look(
            (plugin.as_mut(), state),
            context,
            (&mut self.out, &mut self.declarations_changed),
            "applies",
            |plugin, cx| {
                let applies = plugin.applies(cx, command, args);
                plugin.take_failure().map_or(Ok(applies), Err)
            },
        )
    }

    /// The bindings of the active plugins, in their order, which the
    /// keymap takes after the app's and before the config's.
    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.active().flat_map(|state| &state.bindings)
    }

    /// The bindings of keys taken from every app of the active plugins, in
    /// their order, which the app takes after the config's.
    pub fn global_bindings(&self) -> impl Iterator<Item = &(keymap::Combo, Invocation)> {
        self.active().flat_map(|state| &state.global_bindings)
    }

    /// The commands of the active plugins, which a registry made anew, as
    /// for another language, takes again.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.active().flat_map(|state| &state.commands)
    }

    /// The commands of the active plugins, as [`Plugins::commands`], titled
    /// in `language` from their plugins' catalogs: by their messages, as
    /// [`title_message`] names them, the language's or English's; or by the
    /// titles that they registered; each with its plugin's name, which a
    /// registry keeps as its owner, and whether its plugin waits for its
    /// trigger, which [`Registry::register_waiting`] takes: such a command,
    /// of its manifest, has no parameters, its plugin checking a call's
    /// arguments as it activates, and needs of another plugin's call every
    /// capability that its plugin declares, which its own command's needs
    /// cannot pass, as its action takes them.
    pub fn commands_in(&self, language: &str) -> Vec<(Command, String, bool)> {
        let mut commands = Vec::new();
        for state in self.active() {
            let texts = Texts::new(language, &pairs(&state.catalogs));
            commands.extend(state.commands.iter().map(|command| {
                let titled = Command {
                    title: texts
                        .get(&title_message(&command.name))
                        .unwrap_or_else(|| command.title.clone()),
                    ..command.clone()
                };
                (titled, state.name.clone(), false)
            }));
        }
        // Those of the plugins that wait for their triggers, as their
        // manifests give them, which a run of one activates.
        for state in self.entries.iter().map(|entry| &entry.state) {
            let Some(manifest) = state.manifest.as_ref().filter(|_| state.waits()) else {
                continue;
            };
            let texts = Texts::new(language, &pairs(&state.catalogs));
            let capabilities: Vec<Capability> = manifest
                .capabilities
                .iter()
                .filter(|capability| !withheld_from(manifest, capability))
                .cloned()
                .collect();
            commands.extend(manifest.contributes.commands.iter().map(|contributed| {
                let command = Command {
                    name: contributed.name.clone(),
                    title: texts
                        .get(&title_message(&contributed.name))
                        .unwrap_or_else(|| contributed.title.clone()),
                    category: contributed.category.unwrap_or(commands::Category::View),
                    capabilities: capabilities.clone(),
                    params: Vec::new(),
                    when: None,
                };
                (command, state.name.clone(), true)
            }));
        }
        commands
    }

    /// The menus' items of the active plugins, in their order, each titled
    /// in `language` from its plugin's catalogs, the language's or
    /// English's, or by its own text where they lack it; `None` for the
    /// command's title.
    pub fn menu_items_in(&self, language: &str) -> Vec<MenuEntry> {
        let mut items = Vec::new();
        for state in self.active() {
            let texts = Texts::new(language, &pairs(&state.catalogs));
            items.extend(state.menu_items.iter().map(|item| {
                MenuEntry {
                    plugin: state.name.clone(),
                    place: item.place,
                    invocation: Invocation {
                        command: item.command.clone(),
                        args: item.args.clone(),
                    },
                    title: item
                        .title
                        .as_ref()
                        .map(|title| texts.get(title).unwrap_or_else(|| title.clone())),
                    submenu: item
                        .submenu
                        .as_ref()
                        .map(|title| texts.get(title).unwrap_or_else(|| title.clone())),
                    ticked: item.ticked,
                    own: state
                        .commands
                        .iter()
                        .any(|command| command.name == item.command),
                }
            }));
        }
        items
    }

    /// What the active plugins keep from idle sleep: the strongest that one
    /// asks, and the names of those that ask anything, in their order.
    pub fn keep_awake(&self) -> (host::Awake, Vec<&str>) {
        let asking: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.state.status == Status::Active && entry.state.awake != host::Awake::Off
            })
            .collect();
        let strongest = asking
            .iter()
            .map(|entry| entry.state.awake)
            .max()
            .unwrap_or_default();
        let names = asking
            .iter()
            .map(|entry| entry.state.name.as_str())
            .collect();
        (strongest, names)
    }

    /// The fade of the panes that have not their tab's keyboard, toward their
    /// background, from 0 to 1: the most that the active plugins asked for.
    pub fn inactive_dim(&self) -> f32 {
        self.entries
            .iter()
            .filter(|entry| entry.state.status == Status::Active)
            .map(|entry| entry.state.dim)
            .fold(0.0, f32::max)
    }

    /// Whether the keyboard goes to the pane that the pointer comes over, as
    /// an active plugin asked.
    pub fn focus_follows_mouse(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.state.status == Status::Active && entry.state.follows)
    }

    /// Where the tab bar of `plugin`, the provider of a window's, goes; at
    /// the titlebar for a plugin not there.
    pub fn tab_bar_place(&self, plugin: &str) -> host::BarPlace {
        self.entries
            .iter()
            .find(|entry| entry.state.name == plugin)
            .map(|entry| entry.state.bar_place)
            .unwrap_or_default()
    }

    /// Where the tab bar of `plugin`, the provider of `window`'s, goes in
    /// it: the window's own place, or the bar's; at the titlebar for a
    /// plugin not there.
    pub fn window_tab_bar_place(&self, plugin: &str, window: host::WindowId) -> host::BarPlace {
        self.entries
            .iter()
            .find(|entry| entry.state.name == plugin)
            .map(|entry| {
                entry
                    .state
                    .window_places
                    .get(&window)
                    .copied()
                    .unwrap_or(entry.state.bar_place)
            })
            .unwrap_or_default()
    }

    /// Gives the column of `plugin`'s tab bar `width` points, within its
    /// limits, as the user drags its edge; a bar of no column stays as it
    /// is.
    pub fn set_tab_bar_width(&mut self, plugin: &str, width: f32) {
        let (narrowest, widest) = host::COLUMN_WIDTHS;
        let width = width.clamp(narrowest, widest);
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.state.name == plugin)
        {
            match &mut entry.state.bar_place {
                host::BarPlace::Left { width: kept }
                | host::BarPlace::Right { width: kept }
                | host::BarPlace::Sidebar { width: kept } => {
                    *kept = width;
                }
                _ => {}
            }
        }
    }

    /// Tells `plugin`, to it alone, that the user's drag of its column's
    /// edge left it `width` points wide.
    pub fn tab_bar_resized(&mut self, plugin: &str, width: f32) {
        self.out
            .events
            .push((Some(plugin.to_owned()), Event::TabBarResized { width }));
    }

    /// The text of `text`, a message of `plugin`'s catalogs, in
    /// `language`'s or English's, or `text` itself where they lack it, as a
    /// plugin's question shows it.
    pub fn text_of(&self, plugin: &str, language: &str, text: &str) -> String {
        self.entries
            .iter()
            .find(|entry| entry.state.name == plugin)
            .and_then(|entry| Texts::new(language, &pairs(&entry.state.catalogs)).get(text))
            .unwrap_or_else(|| text.to_owned())
    }

    fn active(&self) -> impl Iterator<Item = &State> {
        self.entries
            .iter()
            .map(|entry| &entry.state)
            .filter(|state| state.status == Status::Active)
    }

    /// The names of the plugins, in the order of their activation.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.state.name.as_str())
    }

    /// The manifest of the plugin `name`, if it was added with one.
    pub fn manifest(&self, name: &str) -> Option<&Manifest> {
        self.entries
            .iter()
            .find(|entry| entry.state.name == name)
            .and_then(|entry| entry.state.manifest.as_ref())
    }

    /// The status of the plugin `name`, if it was added.
    pub fn status(&self, name: &str) -> Option<Status> {
        self.entries
            .iter()
            .find(|entry| entry.state.name == name)
            .map(|entry| entry.state.status)
    }

    /// Whether the plugin `name` is active and its activation ended, of a
    /// thread of its own too, which is active as its activation begins.
    pub fn activated(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| {
            entry.state.name == name
                && entry.state.status == Status::Active
                && !activating(&entry.runner)
        })
    }

    /// Takes what the plugins' calls asked of the app since the last take.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.out.requests)
    }

    /// Takes the quick windows of the plugins that stopped, by a fault or
    /// their sections of the config, which the app closes, as none can
    /// show them again.
    pub fn take_stopped_quick_windows(&mut self) -> Vec<WindowId> {
        self.entries
            .iter_mut()
            .filter(|entry| entry.state.status != Status::Active)
            .flat_map(|entry| std::mem::take(&mut entry.state.quick_windows))
            .collect()
    }

    /// Gives `plugin` the user's answer to its question `id`, the text
    /// given or none, for the next delivery, to it alone.
    pub fn answer(&mut self, plugin: &str, id: u64, text: Option<String>) {
        self.out
            .events
            .push((Some(plugin.to_owned()), Event::Answered { id, text }));
    }

    /// Tells `plugin` the end of its save `id`: the file's path, none as
    /// the user cancelled, or the error of a write that failed, for the
    /// next delivery, to it alone.
    pub fn saved(&mut self, plugin: &str, id: u64, path: Option<String>, error: Option<String>) {
        self.out
            .events
            .push((Some(plugin.to_owned()), Event::Saved { id, path, error }));
    }

    /// Tells `plugin` a line of its job `id` of the packages' service,
    /// Sleip's event of JSON, for the next delivery, to it alone.
    pub fn package_line(&mut self, plugin: &str, id: u64, line: &serde_json::Value) {
        self.out.events.push((
            Some(plugin.to_owned()),
            Event::Package {
                id,
                line: data(line),
            },
        ));
    }

    /// Tells `plugin` the end of its job `id` of the packages' service,
    /// whether Sleip's command succeeded, for the next delivery, to it
    /// alone.
    pub fn packages_ended(&mut self, plugin: &str, id: u64, ok: bool) {
        self.out
            .events
            .push((Some(plugin.to_owned()), Event::PackagesEnded { id, ok }));
    }

    /// Posts `event`, of the workspace's, for the next delivery.
    pub fn post(&mut self, event: Event) {
        // The views of a window or a pane that closed go with it, and the
        // panels of a window tell their plugins that they closed.
        match &event {
            Event::Workspace(WorkspaceEvent::WindowClosed { window }) => {
                let closed = WindowId::of(window.0);
                for entry in &mut self.entries {
                    let state = &mut entry.state;
                    let gone: Vec<PanelId> = state
                        .panels
                        .iter()
                        .filter(|(_, shown)| **shown == closed)
                        .map(|(id, _)| id.clone())
                        .collect();
                    for id in gone {
                        state.panels.remove(&id);
                        state.views.remove(&ViewId::Panel(id.clone()));
                        self.out
                            .events
                            .push((Some(state.name.clone()), Event::PanelClosed(id)));
                    }
                    // The place of its bar in the window, too.
                    state.window_places.remove(window);
                    state.views.retain(|view, _| match view {
                        ViewId::TabBar(shown)
                        | ViewId::WindowStrip(shown)
                        | ViewId::StatusBar(shown)
                        | ViewId::TitleButtons(shown) => shown != window,
                        ViewId::PaneHeader(_) | ViewId::Panel(_) => true,
                    });
                }
            }
            // A pane's header goes with it, and its searches stop, its
            // terminal gone.
            Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => {
                for entry in &mut self.entries {
                    entry.state.views.remove(&ViewId::PaneHeader(*pane));
                    entry.state.searches.retain(|search| search.pane != *pane);
                    entry.state.lingering.retain(|search| search.pane != *pane);
                    entry.state.replaced.retain(|search| search.pane != *pane);
                }
            }
            _ => {}
        }
        self.out.events.push((None, event));
    }

    /// The plugins that fill `slot`, as their manifests declare, in their
    /// order: the active ones, and those disabled since, whose last trees
    /// stay.
    pub fn providers(&self, slot: Slot) -> Vec<&str> {
        self.entries
            .iter()
            .map(|entry| &entry.state)
            .filter(|state| {
                let shows = !matches!(state.status, Status::Inactive | Status::Off)
                    || (state.waits()
                        && state
                            .triggers
                            .contains(&Trigger::Slot(slot.name().to_owned())));
                shows && state.fills(slot)
            })
            .map(|state| state.name.as_str())
            .collect()
    }

    /// The plugin whose view fills `slot`, one of one plugin's: `chosen`,
    /// as the config names it, if it fills it, or the first that does.
    pub fn provider(&self, slot: Slot, chosen: &str) -> Option<&str> {
        let providers = self.providers(slot);
        providers
            .iter()
            .find(|&&name| name == chosen)
            .or_else(|| providers.first())
            .copied()
    }

    /// Asks the plugin `name` for the tree of `view`, one of its views, and
    /// keeps it: a disabled plugin is not asked, and keeps its last; a
    /// panel that is not open, or a view of a window or a pane that closed,
    /// is not asked, as no tree of it stays; a fault when it panicked, which
    /// disables it, its last tree staying. A plugin of a thread of its own
    /// renders there, its tree coming through [`Plugins::serve`], its last
    /// one staying meanwhile.
    pub fn render(
        &mut self,
        name: &str,
        view: &ViewId,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        let Some(at) = self
            .entries
            .iter()
            .position(|entry| entry.state.name == name)
        else {
            return Err(Error::new(
                Code::NotFound,
                format!("there is no plugin `{name}`"),
            ));
        };
        // A plugin that waits for its slot's trigger activates as its view
        // first shows, of the main thread at once, or of a thread of its
        // own, which renders after its activation.
        let state = &self.entries[at].state;
        if state.waits()
            && view_slot(view).is_some_and(|slot| {
                state
                    .triggers
                    .contains(&Trigger::Slot(slot.name().to_owned()))
            })
            && shows(state, view, context.workspace)
        {
            self.waken(at, context)?;
            // Its activation waits for the user's consent: the view renders
            // after it.
            let state = &mut self.entries[at].state;
            if state.asking && !state.deferred.renders.contains(view) {
                state.deferred.renders.push(view.clone());
            }
        }
        let Entry { runner, state, .. } = &mut self.entries[at];
        if state.status != Status::Active || !shows(state, view, context.workspace) {
            return Ok(());
        }
        match runner {
            Runner::Main(plugin) => {
                let tree = call(
                    (plugin.as_mut(), state),
                    context,
                    &mut self.out,
                    &mut self.declarations_changed,
                    "render",
                    |plugin, cx| rendered(plugin, cx, view),
                )?;
                state.views.insert(view.clone(), (new_id(), tree));
            }
            Runner::Worker(worker) => {
                let within = worker.ask(Job::Render(view.clone()), context.focused);
                within_bound(
                    within,
                    (runner, state),
                    context,
                    (&mut self.out, &mut self.declarations_changed),
                )?;
            }
            Runner::Unstarted(_) | Runner::Stopped => {}
        }
        Ok(())
    }

    /// The number of the last render of `view` by the plugin `name`, which
    /// no other render took, if it rendered it: what tells a host whose
    /// surface shows an older one to take the tree again.
    pub fn rendered(&self, name: &str, view: &ViewId) -> Option<u64> {
        self.entries
            .iter()
            .find(|entry| entry.state.name == name)
            .and_then(|entry| entry.state.views.get(view))
            .map(|(number, _)| *number)
    }

    /// Counts the faults of the plugins that faults disabled since the last
    /// count, at `now`, of those that the host can make anew, as a
    /// registration's or an installed plugin's: each starts again after a while, [`RESTART_AFTER`],
    /// twice as long at each fault, and three faults within a minute turn
    /// it off, past restarting; its fault tells which, as the app's notice
    /// of the plugins that stopped shows it.
    pub fn schedule_restarts(&mut self, now: Instant) {
        for entry in &mut self.entries {
            let state = &mut entry.state;
            if entry.make.is_none() || state.status != Status::Disabled || state.restarts.counted {
                continue;
            }
            let Some(fault) = &mut state.fault else {
                continue;
            };
            let restarts = &mut state.restarts;
            restarts.counted = true;
            restarts
                .faults
                .retain(|at| now.saturating_duration_since(*at) < FAULTS_WITHIN);
            restarts.faults.push(now);
            let count = restarts.faults.len();
            if count >= MOST_FAULTS {
                restarts.at = None;
                fault.push_str(&format!(
                    "; it is turned off, after {count} faults within a minute, until the app's next start"
                ));
            } else {
                let wait = RESTART_AFTER * 2_u32.pow(u32::try_from(count - 1).unwrap_or(0));
                restarts.at = Some(now + wait);
                fault.push_str(&format!("; it starts again in {} s", wait.as_secs()));
            }
        }
    }

    /// When the next plugin that faulted starts again, if one does.
    pub fn next_restart(&self) -> Option<Instant> {
        self.entries
            .iter()
            .filter_map(|entry| entry.state.restarts.at)
            .min()
    }

    /// Starts again the plugins that faulted whose time came at `now`: each
    /// stopped, as its section turning it off stops it, and made anew of
    /// its registration, as its section turning it on again makes it, and
    /// activated; the errors of those whose activation failed.
    pub fn restart_due(&mut self, now: Instant, context: &mut Context<'_>) -> Vec<Error> {
        let mut any = false;
        for entry in &mut self.entries {
            if entry.state.restarts.at.is_none_or(|at| at > now) {
                continue;
            }
            entry.state.restarts.at = None;
            entry.state.restarts.counted = false;
            if entry.state.status == Status::Disabled {
                stop(
                    entry,
                    context,
                    &mut self.out,
                    &mut self.declarations_changed,
                );
                entry.state.status = Status::Off;
                any = true;
            }
        }
        if any {
            self.activate(context)
        } else {
            Vec::new()
        }
    }

    /// The plugins that a fault disabled, or whose activation failed, since
    /// the last look, in their order, each with its error's message, as the
    /// app tells the user that they stopped.
    pub fn take_disabled(&mut self) -> Vec<(String, String)> {
        let told = &mut self.told_disabled;
        self.entries
            .iter()
            .filter(|entry| entry.state.status == Status::Disabled)
            .filter(|entry| told.insert(entry.state.name.clone()))
            .map(|entry| {
                let fault = entry.state.fault.clone().unwrap_or_default();
                (entry.state.name.clone(), fault)
            })
            .collect()
    }

    /// The plugin that runs `command`, if an active one registered it.
    pub fn owner_of(&self, command: &str) -> Option<&str> {
        self.owner(command)
            .or_else(|| self.lazy_owner(command))
            .map(|at| self.entries[at].state.name.as_str())
    }

    /// The last tree of `view` that the plugin `name` rendered, if it gave
    /// one.
    pub fn tree(&self, name: &str, view: &ViewId) -> Option<&Node> {
        self.entries
            .iter()
            .find(|entry| entry.state.name == name)
            .and_then(|entry| entry.state.views.get(view))
            .and_then(|(_, tree)| tree.as_ref())
    }

    /// Every view that a plugin rendered, by its plugin, as a change of the
    /// theme renders them again.
    pub fn views(&self) -> Vec<(String, ViewId)> {
        self.entries
            .iter()
            .flat_map(|entry| {
                entry
                    .state
                    .views
                    .keys()
                    .map(|view| (entry.state.name.clone(), view.clone()))
            })
            .collect()
    }

    /// Takes the views whose renders the plugins' calls asked for, and those
    /// of the panels that they opened, by their plugins, in their order.
    pub fn take_renders(&mut self) -> Vec<(String, ViewId)> {
        std::mem::take(&mut self.out.renders)
    }

    /// Gives the plugin `name` `event`, of a node of one of its views, if
    /// it is active.
    pub fn ui_event(
        &mut self,
        name: &str,
        event: &UiEvent,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        let Some(at) = self
            .entries
            .iter()
            .position(|entry| entry.state.name == name && entry.state.status == Status::Active)
        else {
            return Ok(());
        };
        let Entry { runner, state, .. } = &mut self.entries[at];
        match runner {
            Runner::Main(plugin) => call(
                (plugin.as_mut(), state),
                context,
                &mut self.out,
                &mut self.declarations_changed,
                "on_ui_event",
                |plugin, cx| plugin.on_ui_event(cx, event),
            ),
            Runner::Worker(worker) => {
                let within = worker.ask(Job::Ui(event.clone()), context.focused);
                within_bound(
                    within,
                    (runner, state),
                    context,
                    (&mut self.out, &mut self.declarations_changed),
                )
            }
            Runner::Unstarted(_) | Runner::Stopped => Ok(()),
        }
    }

    /// Tells the plugin `name` that its panel `id` closed, which its next
    /// delivery gives it, and forgets the panel.
    pub fn panel_closed(&mut self, name: &str, id: &PanelId) {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.state.name == name)
        else {
            return;
        };
        if entry.state.panels.remove(id).is_none() {
            return;
        }
        entry.state.views.remove(&ViewId::Panel(id.clone()));
        self.out
            .events
            .push((Some(name.to_owned()), Event::PanelClosed(id.clone())));
    }

    /// Takes the reads of rows that ended, and the searches' parts that
    /// came, as events to their plugins.
    fn take_reads(&mut self) {
        // The searches' parts that came, to their plugins.
        for entry in &mut self.entries {
            let name = &entry.state.name;
            for search in &mut entry.state.searches {
                for part in search.take() {
                    let event = Event::Searched {
                        id: search.id,
                        pane: search.pane,
                        matches: part.matches,
                        count: part.count,
                        done: part.done,
                    };
                    self.out.events.push((Some(name.clone()), event));
                }
            }
        }
        // The reads of a plugin that a panic disabled stop.
        let active: BTreeSet<&str> = self
            .entries
            .iter()
            .filter(|entry| entry.state.status == Status::Active)
            .map(|entry| entry.state.name.as_str())
            .collect();
        let Outbox { reads, events, .. } = &mut self.out;
        reads.retain(|read| active.contains(read.plugin.as_str()));
        reads.retain(|read| {
            let (lines, complete) = match read.rows.try_recv() {
                Ok(read) => read,
                Err(mpsc::TryRecvError::Empty) => return true,
                // The read's thread did not start, or broke.
                Err(mpsc::TryRecvError::Disconnected) => (Vec::new(), false),
            };
            let event = Event::Lines {
                id: read.id,
                pane: read.pane,
                lines,
                complete,
            };
            events.push((Some(read.plugin.clone()), event));
            false
        });
    }

    /// Delivers the events posted and emitted since the last delivery, in
    /// their order, as a batch to each active plugin, in their order, of
    /// those that it subscribed to, and then those of its timers that fired,
    /// in their ids' order; those that the deliveries' calls emit wait for
    /// the next. Returns the calls' errors, of which a panic's disables its
    /// plugin. A plugin of a thread of its own takes its batch there, which
    /// joins the one that waits for it, if one does; its timers' events wait
    /// until its thread is free.
    pub fn deliver(&mut self, context: &mut Context<'_>) -> Vec<Error> {
        self.take_reads();
        let events = std::mem::take(&mut self.out.events);
        let mut errors = Vec::new();
        if events.is_empty() && !self.timers_owed() {
            return errors;
        }
        // The plugins that wait for an event of the batch activate, and hear
        // it as they subscribe to it: one of the main thread at once, one of
        // a thread of its own as its activation's subscriptions come.
        let names: BTreeSet<&str> = events
            .iter()
            .filter(|(to, _)| to.is_none())
            .map(|(_, event)| event.name())
            .collect();
        for at in 0..self.entries.len() {
            let state = &self.entries[at].state;
            let wakes = state.waits()
                && state.triggers.iter().any(
                    |trigger| matches!(trigger, Trigger::Event(name) if names.contains(name.as_str())),
                );
            if !wakes {
                continue;
            }
            if let Err(error) = self.waken(at, context) {
                errors.push(error);
            }
            // Its activation waits for the user's consent: the events of
            // its trigger come after it, the one that woke it and the last
            // of each name and subject, so that a question held while a
            // pane's view moves keeps one of each pane, bounded.
            let state = &mut self.entries[at].state;
            if state.asking {
                let triggered = |name: &str| {
                    state
                        .triggers
                        .iter()
                        .any(|trigger| matches!(trigger, Trigger::Event(event) if event == name))
                };
                let held: Vec<Event> = events
                    .iter()
                    .filter(|(to, event)| to.is_none() && triggered(event.name()))
                    .map(|(_, event)| event.clone())
                    .collect();
                for event in held {
                    state.deferred.hold(event);
                }
            }
        }
        for entry in &mut self.entries {
            if entry.state.status != Status::Active {
                continue;
            }
            // A plugin of a thread of its own that activates subscribes as
            // its requests come, after the activation began: it holds the
            // events that may be its own until its subscriptions are known,
            // those that it hears, as a tab's change that it cannot read is
            // none.
            if let Entry {
                runner: Runner::Worker(worker),
                state,
                ..
            } = entry
                && worker.activating()
            {
                let name = state.name.clone();
                let held: Vec<(Option<String>, Event)> = events
                    .iter()
                    .filter(|(to, _)| to.as_ref().is_none_or(|to| *to == name))
                    .filter(|(_, event)| state.hears(event, context.workspace))
                    .cloned()
                    .collect();
                worker.hold(held);
                continue;
            }
            let (name, subscribed) = (&entry.state.name, &entry.state.subscribed);
            let batch: Vec<Event> = events
                .iter()
                .filter(|(to, event)| match to {
                    Some(to) => to == name,
                    None => subscribed.iter().any(|name| name == event.name()),
                })
                .map(|(_, event)| event.clone())
                .collect();
            let mut batch: Vec<Event> = batch
                .into_iter()
                .filter(|event| entry.state.hears(event, context.workspace))
                .collect();
            // Its timers that fired, once it can take them: a plugin of a
            // thread of its own as its thread is free, so that a timer that
            // its call under way cancels tells it nothing.
            if takes_timers(&entry.runner) {
                batch.extend(fired(&mut entry.state));
            }
            if batch.is_empty() {
                continue;
            }
            let Entry { runner, state, .. } = entry;
            let delivered = match runner {
                Runner::Main(plugin) => call(
                    (plugin.as_mut(), state),
                    context,
                    &mut self.out,
                    &mut self.declarations_changed,
                    "on_event",
                    |plugin, cx| plugin.on_event(cx, &batch),
                ),
                Runner::Worker(worker) => {
                    let within = worker.ask(Job::Events(batch), context.focused);
                    within_bound(
                        within,
                        (runner, state),
                        context,
                        (&mut self.out, &mut self.declarations_changed),
                    )
                }
                Runner::Unstarted(_) | Runner::Stopped => Ok(()),
            };
            if let Err(error) = delivered {
                errors.push(error);
            }
        }
        errors
    }

    /// Whether an active plugin subscribed to the events of `name`, or a
    /// waiting plugin's trigger names them, which the app then tells, as
    /// the pointer's, whose moves cost wakeups.
    pub fn subscribes(&self, name: &str) -> bool {
        self.active()
            .any(|state| state.subscribed.iter().any(|subscribed| subscribed == name))
            || self.entries.iter().any(|entry| {
                entry.state.waits()
                    && entry
                        .state
                        .triggers
                        .iter()
                        .any(|trigger| matches!(trigger, Trigger::Event(event) if event == name))
            })
    }

    /// Whether events wait for the next delivery: those of the reads of
    /// rows that ended once [`Plugins::serve`] took them, and those of the
    /// timers that fired, of plugins that can take them.
    pub fn has_events(&self) -> bool {
        !self.out.events.is_empty() || self.timers_owed()
    }

    /// Whether a timer that fired owes its plugin its event, which the
    /// plugin can take now.
    fn timers_owed(&self) -> bool {
        self.entries.iter().any(|entry| {
            entry.state.status == Status::Active
                && takes_timers(&entry.runner)
                && entry.state.timers.values().any(|timer| timer.due)
        })
    }

    /// Fires the timers of the active plugins whose time came at `now`:
    /// each owes its plugin its event, once however many times it fired
    /// since the plugin last heard of it, and one that repeats waits its
    /// delay again, from its time, or from `now` if it fell behind by more
    /// than its delay, as the app was busy.
    pub fn timers_due(&mut self, now: Instant) {
        for entry in &mut self.entries {
            if entry.state.status != Status::Active {
                continue;
            }
            for timer in entry.state.timers.values_mut() {
                let Some(at) = timer.at.filter(|&at| at <= now) else {
                    continue;
                };
                timer.due = true;
                timer.at = timer.every.map(|every| {
                    let next = at + every;
                    if next > now { next } else { now + every }
                });
            }
        }
    }

    /// When the next timer of an active plugin fires, if one waits, which
    /// the app wakes for: none costs nothing.
    pub fn next_timer(&self) -> Option<Instant> {
        self.active()
            .flat_map(|state| state.timers.values().filter_map(|timer| timer.at))
            .min()
    }

    /// Whether reads of rows, or calls of plugins on threads of their own,
    /// are under way, whose ends [`Plugins::serve`] takes, as they wake the
    /// main thread.
    pub fn waiting(&self) -> bool {
        !self.out.reads.is_empty()
            || self
                .entries
                .iter()
                .any(|entry| entry.state.searches.iter().any(search::Search::is_busy))
            || self
                .entries
                .iter()
                .any(|entry| matches!(&entry.runner, Runner::Worker(worker) if worker.is_busy()))
    }

    /// Takes what came since the last turn: the reads of rows that ended,
    /// as events to their plugins, and the messages of the plugins'
    /// threads, in turn, for [`SERVE_FOR`] at most, past which those left
    /// wait for the next turn, which takes the next plugin's first, and
    /// which the host wakes the main thread for: each request answered with the state as it
    /// is now, a write queued for its call; each call that ended, its
    /// writes applied and what it asked of the app passed on, a render's
    /// tree kept, of a view that still shows, and the next call that waits
    /// for its plugin begun. A call that panicked disables its plugin,
    /// whose thread ends, and its writes go.
    pub fn serve(&mut self, context: &mut Context<'_>) -> Served {
        let mut served = Served::default();
        let reads = self.out.reads.len();
        self.take_reads();
        served.any = self.out.reads.len() < reads;
        self.serve_messages(context, &mut served);
        // A read that a call began and that ended before the call did woke
        // the main thread before the call's end brought it here: it comes
        // now, as no wake is left to bring it.
        self.take_reads();
        served
    }

    /// Takes the messages of the plugins' threads, as [`Plugins::serve`]
    /// tells.
    fn serve_messages(&mut self, context: &mut Context<'_>, served: &mut Served) {
        let (start, most) = (Instant::now(), self.serve_for.unwrap_or(SERVE_FOR));
        let count = self.entries.len();
        loop {
            let mut took = false;
            for step in 0..count {
                let at = (self.serve_from + step) % count;
                if self.serve_one(at, context, served) {
                    took = true;
                    served.any = true;
                    if start.elapsed() >= most {
                        // The messages left bring the next turn, as their
                        // wakes came already.
                        self.serve_from = (at + 1) % count;
                        served.more = true;
                        if let Some(wake) = &self.wake {
                            wake();
                        }
                        return;
                    }
                }
            }
            if !took {
                return;
            }
        }
    }

    /// Takes the next message of the thread of the plugin at `at`, if it
    /// has one, and one came: whether one did.
    fn serve_one(&mut self, at: usize, context: &mut Context<'_>, served: &mut Served) -> bool {
        let Plugins {
            entries,
            out,
            declarations_changed,
            ..
        } = self;
        let Entry { runner, state, .. } = &mut entries[at];
        let Runner::Worker(worker) = runner else {
            return false;
        };
        let done = match worker.take() {
            Err(mpsc::TryRecvError::Empty) => return false,
            // A thread that ended within a call, as none should.
            Err(mpsc::TryRecvError::Disconnected) => {
                if !worker.is_busy() {
                    return false;
                }
                Done::Panicked(": its thread ended".to_owned())
            }
            Ok(FromWorker::Request(asked)) => {
                let response = match worker.busy_mut() {
                    Some(busy) => {
                        // The call reads the focus of the time that it was
                        // asked, as it would have run then on the main
                        // thread.
                        let now = std::mem::replace(&mut context.focused, busy.focused);
                        if busy.kept.is_none() {
                            busy.kept = Some(Kept::of(state));
                        }
                        let handled = panic::catch_unwind(AssertUnwindSafe(|| {
                            let mut host = Host {
                                state: &mut *state,
                                context: &mut *context,
                                out: &mut busy.out,
                                writes: &mut busy.writes,
                                declarations_changed: &mut *declarations_changed,
                                looking: false,
                                tally: &mut busy.tally,
                            };
                            host::Host::handle(&mut host, asked)
                        }));
                        context.focused = now;
                        handled
                    }
                    None => Ok(Response::Failed(Error::new(
                        Code::Fault,
                        "a request came outside a call",
                    ))),
                };
                match response {
                    Ok(response) => worker.answer(response),
                    // The host broke on the request: its plugin is
                    // disabled, as a panic of its own disables it, and its
                    // thread, which waits for the answer, ends without it.
                    Err(payload) => {
                        if let Some(kept) = worker.busy_mut().and_then(|busy| busy.kept.take()) {
                            kept.restore(state);
                        }
                        disable(state, context.registry, declarations_changed);
                        clear_leftovers(state, context, out);
                        let fault = format!(
                            "the host broke on a request of the plugin `{}`{}; it is disabled",
                            state.name,
                            message(payload.as_ref())
                        );
                        state.fault = Some(fault.clone());
                        served.errors.push(Error::new(Code::Fault, fault));
                        *runner = Runner::Stopped;
                    }
                }
                return true;
            }
            Ok(FromWorker::Done(done)) => done,
        };
        let Some(busy) = worker.end() else {
            return true;
        };
        let worker::Busy {
            called,
            writes,
            out: mut sent,
            chain,
            kept,
            tally,
            ..
        } = busy;
        let result = match done {
            Done::Panicked(message) => {
                if let Some(kept) = kept {
                    kept.restore(state);
                }
                disable(state, context.registry, declarations_changed);
                clear_leftovers(state, context, out);
                let fault = format!(
                    "the plugin `{}` panicked{message}; it is disabled",
                    state.name
                );
                state.fault = Some(fault.clone());
                served.errors.push(Error::new(Code::Fault, fault));
                *runner = Runner::Stopped;
                return true;
            }
            Done::Returned(result) => result,
            Done::Rendered(tree) => {
                if let Called::Render(view) = &called
                    && shows(state, view, context.workspace)
                {
                    state
                        .views
                        .insert(view.clone(), (new_id(), tree.map(|tree| *tree)));
                }
                Ok(())
            }
        };
        // A call that returns a fault faults its plugin, as a panic does.
        if let Err(error) = &result
            && error.code == Code::Fault
        {
            if let Some(kept) = kept {
                kept.restore(state);
            }
            disable(state, context.registry, declarations_changed);
            clear_leftovers(state, context, out);
            let fault = format!("{}; it is disabled", error.message);
            state.fault = Some(fault.clone());
            served.errors.push(Error::new(Code::Fault, fault));
            *runner = Runner::Stopped;
            return true;
        }
        if result.is_ok() {
            closed_panels(state, &mut sent, tally.closed);
            settle_replaced(state, context.workspace);
            apply(writes, context, &mut sent, state);
        } else {
            // A call that failed leaves things as they were, as the main
            // thread's does: its writes, its requests and its reads go,
            // and what its requests took at once of its state comes back.
            sent.requests.clear();
            sent.reads.clear();
            if let Some(kept) = kept {
                kept.restore(state);
            }
            end_searches(state, &tally.searches, context.workspace);
        }
        deepen(&mut sent.requests, chain);
        out.merge(sent);
        let activated = called == Called::Activate && result.is_ok();
        match (called, result) {
            (_, Ok(())) => {}
            (Called::Activate, Err(error)) => {
                served.errors.push(failed_start(
                    state,
                    context.registry,
                    declarations_changed,
                    &error,
                ));
                *runner = Runner::Stopped;
                return true;
            }
            (Called::Command, Err(error)) => served.commands.push(error),
            (_, Err(error)) => served.errors.push(error),
        }
        if activated {
            worker.activated(&state.subscribed, context.focused);
            // The runs that woke it, of its own commands now: a few, after
            // a call of its first; a queue past its bound would fault it at
            // its next call.
            for (command, args, chain) in std::mem::take(&mut state.woken) {
                match own_call(state, &command, args) {
                    Ok(args) => {
                        let _ = worker.ask(Job::Command(command, args, chain), context.focused);
                    }
                    Err(error) => served.commands.push(error),
                }
            }
        }
        worker.next();
        true
    }

    /// Takes the calls of the plugins on the main thread that took
    /// [`SLOW`] or longer since the last look, as the log tells of them.
    pub fn take_slow(&mut self) -> Vec<SlowCall> {
        let mut slow = Vec::new();
        for entry in &mut self.entries {
            let state = &mut entry.state;
            slow.extend(state.slow.drain(..).map(|(callback, took)| SlowCall {
                plugin: state.name.clone(),
                callback,
                took,
            }));
        }
        slow
    }

    /// Whether the plugins' commands or bindings changed since the last
    /// call, as a plugin registered a command, bound keys or was disabled:
    /// a plugin of a thread of its own registers its commands after the
    /// app built its menu.
    pub fn take_declarations_changed(&mut self) -> bool {
        std::mem::take(&mut self.declarations_changed)
    }

    /// Whether the plugins' calls wrote to the kernel's state since the
    /// last call, a pane's decorations, its selection, its scroll or its
    /// text among them, as what the windows show may have changed; a call
    /// that wrote nothing, as a delivery that a plugin only read, changes
    /// nothing shown.
    pub fn take_written(&mut self) -> bool {
        std::mem::take(&mut self.out.written)
    }

    /// Whether a call wrote since the last [`Plugins::take_written`], as
    /// that tells, without taking it.
    pub fn written(&self) -> bool {
        self.out.written
    }
}

/// The schema of the config of the plugin `name`, of `manifest` and of
/// `runner`'s plugin: the text that the plugin gives, which its manifest
/// names, if it has one; an error of a schema that does not read, or that
/// one names and the other lacks.
fn schema(
    name: &str,
    manifest: Option<&Manifest>,
    runner: &Runner,
) -> Result<Option<Schema>, Error> {
    let text = panic::catch_unwind(AssertUnwindSafe(|| match runner {
        Runner::Main(plugin) => plugin.config_schema(),
        Runner::Unstarted(plugin) => plugin.config_schema(),
        Runner::Worker(_) | Runner::Stopped => None,
    }))
    .unwrap_or(None);
    let invalid = |message: String| Error::new(Code::InvalidConfig, message);
    if let Some(manifest) = manifest {
        match (&manifest.contributes.config_schema, &text) {
            (Some(file), None) => {
                return Err(invalid(format!(
                    "the plugin `{name}`'s manifest names the config schema `{file}`, which the plugin does not give"
                )));
            }
            (None, Some(_)) => {
                return Err(invalid(format!(
                    "the plugin `{name}` gives a config schema that its manifest does not name as `config-schema`"
                )));
            }
            _ => {}
        }
    }
    let Some(text) = text else {
        return Ok(None);
    };
    let schema = Schema::parse(&text).map_err(|error| {
        invalid(format!(
            "the config schema of the plugin `{name}` does not read: {error}"
        ))
    })?;
    if schema.keys().contains(&ENABLED) {
        return Err(invalid(format!(
            "the config schema of the plugin `{name}` has the key `{ENABLED}`, which is the host's, as it turns plugins off"
        )));
    }
    Ok(Some(schema))
}

/// The file of the state of the plugin of `namespace`, in the app's state
/// `directory`.
fn state_file(directory: &std::path::Path, namespace: &str) -> PathBuf {
    directory.join("plugins").join(format!("{namespace}.json"))
}

/// The plain value of `value`: JSON's null is nothing, and a number is an
/// integer if it has no fraction and fits.
fn data(value: &Value) -> Data {
    match value {
        Value::Null => Data::Nil,
        Value::Bool(boolean) => Data::Boolean(*boolean),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => Data::Integer(integer),
            None => Data::Number(number.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(text) => Data::Text(text.clone()),
        Value::Array(items) => Data::List(items.iter().map(data).collect()),
        Value::Object(map) => Data::Map(
            map.iter()
                .map(|(key, value)| (key.clone(), self::data(value)))
                .collect(),
        ),
    }
}

/// The JSON of `data`; `None` of a number that is not finite, as JSON has
/// none.
fn json(data: &Data) -> Option<Value> {
    Some(match data {
        Data::Nil => Value::Null,
        Data::Boolean(boolean) => Value::Bool(*boolean),
        Data::Integer(integer) => Value::from(*integer),
        Data::Number(number) => Value::Number(serde_json::Number::from_f64(*number)?),
        Data::Text(text) => Value::String(text.clone()),
        Data::List(items) => Value::Array(items.iter().map(json).collect::<Option<_>>()?),
        Data::Map(map) => Value::Object(
            map.iter()
                .map(|(key, value)| Some((key.clone(), self::json(value)?)))
                .collect::<Option<_>>()?,
        ),
    })
}

/// The manifest of a built-in plugin, of the text `manifest`; an error if
/// it does not read.
fn builtin_manifest(manifest: &str) -> Result<Manifest, Error> {
    Manifest::parse(manifest).map_err(|error: ManifestError| {
        Error::new(
            Code::InvalidConfig,
            format!("a plugin's manifest does not read: {error}"),
        )
    })
}

/// Checks that `name` can name a theme, a file's name of the themes'
/// folder.
fn theme_name(name: &str) -> Result<(), Error> {
    if crate::themes::is_name(name) {
        Ok(())
    } else {
        Err(Error::new(
            Code::Failed,
            format!("`{name}` is no theme's name"),
        ))
    }
}

/// A plugin's item of the menus, as the app shows it: where, the command
/// that it runs with its arguments, and its title, `None` for the
/// command's.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuEntry {
    /// The plugin that added it.
    pub plugin: String,
    /// The menu.
    pub place: host::MenuPlace,
    /// What it runs.
    pub invocation: Invocation,
    /// Its title, in the app's language.
    pub title: Option<String>,
    /// The title of the submenu that it goes into, in the app's language.
    pub submenu: Option<String>,
    /// Whether the menu bar shows it ticked.
    pub ticked: bool,
    /// Whether its command is its plugin's own, which it registered.
    pub own: bool,
}

/// The message of a plugin's catalogs that titles its command `name`:
/// `command-` and the name, with `-` for its `.`, as `command-pane-copy`.
pub fn title_message(name: &str) -> String {
    format!("command-{}", name.replace('.', "-"))
}

/// Calls `call`, of the callback `callback`, on a plugin of the main
/// thread, of `state`, within `catch_unwind`, and applies its writes, in
/// their order, as it returns: a plugin that panics is disabled, and the
/// writes and the requests of the call go.
fn call<T>(
    (plugin, state): (&mut dyn Plugin, &mut State),
    context: &mut Context<'_>,
    out: &mut Outbox,
    declarations_changed: &mut bool,
    callback: &'static str,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let (requests, reads) = (out.requests.len(), out.reads.len());
    let kept = Kept::of(state);
    let (mut writes, mut tally) = (Vec::new(), Tally::default());
    let outcome = within(
        (plugin, state),
        context,
        (&mut *out, &mut writes, &mut tally),
        (declarations_changed, callback, false),
        call,
    );
    match outcome {
        Ok(Ok(result)) => {
            closed_panels(state, out, tally.closed);
            settle_replaced(state, context.workspace);
            apply(writes, context, out, state);
            deepen(&mut out.requests[requests..], Chain::default());
            Ok(result)
        }
        // A call that returns a fault, as a limit of its plugin's VM gives
        // one, faults its plugin, as a panic does.
        Ok(Err(error)) if error.code == Code::Fault => {
            out.requests.truncate(requests);
            out.reads.truncate(reads);
            kept.restore(state);
            fault(state, context, (out, declarations_changed), &error);
            Err(error)
        }
        // A call that failed leaves things as they were: its writes, its
        // requests and its reads go, what its requests took at once of its
        // state comes back, and the searches that it started end.
        Ok(Err(error)) => {
            out.requests.truncate(requests);
            out.reads.truncate(reads);
            kept.restore(state);
            end_searches(state, &tally.searches, context.workspace);
            Err(error)
        }
        // One that panicked too, and its plugin, disabled, leaves nothing.
        Err(fault) => {
            out.requests.truncate(requests);
            out.reads.truncate(reads);
            kept.restore(state);
            clear_leftovers(state, context, out);
            fault
        }
    }
}

/// What a call's requests take of its plugin's state at once, as they give
/// their ids, whose writes apply as the call returns: kept before a call
/// whose writes may go, as a look's or a failed call's, to come back.
struct Kept {
    panels: BTreeMap<PanelId, WindowId>,
    decorations: BTreeMap<u64, host::PaneId>,
    gutter_marks: BTreeMap<u64, host::PaneId>,
    held_cursors: BTreeSet<host::PaneId>,
    quick_windows: Vec<WindowId>,
    timers: BTreeMap<u64, Timer>,
}

impl Kept {
    /// What `state` holds now.
    fn of(state: &State) -> Self {
        Self {
            panels: state.panels.clone(),
            decorations: state.decorations.clone(),
            gutter_marks: state.gutter_marks.clone(),
            held_cursors: state.held_cursors.clone(),
            quick_windows: state.quick_windows.clone(),
            timers: state.timers.clone(),
        }
    }

    /// Gives `state` what it held back.
    fn restore(self, state: &mut State) {
        state.panels = self.panels;
        state.decorations = self.decorations;
        state.gutter_marks = self.gutter_marks;
        state.held_cursors = self.held_cursors;
        state.quick_windows = self.quick_windows;
        // The timers that the call started go, and those that it cancelled
        // come back; the others stay as they are, of the firings that came
        // meanwhile, as a call of a plugin's thread runs beside the main
        // thread's timers.
        let mut timers = self.timers;
        for (id, timer) in &mut timers {
            if let Some(now) = state.timers.get(id) {
                timer.clone_from(now);
            }
        }
        state.timers = timers;
    }
}

/// Calls `call` on a plugin of the main thread, as [`call`] does, but
/// drops its writes and its requests, as the call only looks: the panels
/// that it opens or closes, its decorations, its gutters' marks and its
/// quick windows, stay as they were.
fn look<T>(
    (plugin, state): (&mut dyn Plugin, &mut State),
    context: &mut Context<'_>,
    (outbox, declarations_changed): (&mut Outbox, &mut bool),
    callback: &'static str,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let (mut out, mut writes) = (Outbox::default(), Vec::new());
    let kept = Kept::of(state);
    let outcome = within(
        (plugin, state),
        context,
        (&mut out, &mut writes, &mut Tally::default()),
        (declarations_changed, callback, true),
        call,
    );
    kept.restore(state);
    match outcome {
        // A look that returns a fault, as a limit of its plugin's VM gives
        // one, faults its plugin, as a panic does.
        Ok(Err(error)) if error.code == Code::Fault => {
            fault(state, context, (outbox, declarations_changed), &error);
            Err(error)
        }
        Ok(result) => result,
        // A look that panicked disabled its plugin, whose leftovers go.
        Err(fault) => {
            clear_leftovers(state, context, outbox);
            fault
        }
    }
}

/// Faults the plugin of `state` of `error`, a fault that one of its calls
/// returned: it is disabled, as a panic disables it, and its leftovers go.
fn fault(
    state: &mut State,
    context: &mut Context<'_>,
    (out, declarations_changed): (&mut Outbox, &mut bool),
    error: &Error,
) {
    disable(state, context.registry, declarations_changed);
    state.fault = Some(format!("{}; it is disabled", error.message));
    clear_leftovers(state, context, out);
}

/// Calls `call` on a plugin of the main thread within `catch_unwind`, its
/// writes and its requests into `queues`: the call's result, or, as it
/// panicked, which disables the plugin, the fault's error. A call of
/// [`SLOW`] or longer is noted, by its `callback`, for the log.
fn within<T>(
    (plugin, state): (&mut dyn Plugin, &mut State),
    context: &mut Context<'_>,
    (out, writes, tally): (&mut Outbox, &mut Vec<Write>, &mut Tally),
    (declarations_changed, callback, looking): (&mut bool, &'static str, bool),
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<Result<T, Error>, Result<T, Error>> {
    let name = state.name.clone();
    let start = Instant::now();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut host = Host {
            state: &mut *state,
            context: &mut *context,
            out: &mut *out,
            writes: &mut *writes,
            declarations_changed: &mut *declarations_changed,
            looking,
            tally: &mut *tally,
        };
        call(plugin, &mut Cx::new(&mut host, &name))
    }));
    let took = start.elapsed();
    if took >= SLOW {
        state.slow.push((callback, took));
    }
    outcome.map_err(|payload| {
        disable(state, context.registry, declarations_changed);
        let fault = format!(
            "the plugin `{}` panicked{}; it is disabled",
            state.name,
            message(payload.as_ref())
        );
        state.fault = Some(fault.clone());
        Err(Error::new(Code::Fault, fault))
    })
}

/// The key of every plugin's section of the config that turns it off with
/// `false`, the host's, which no schema can have.
pub const ENABLED: &str = "enabled";

/// Activates the plugin of `entry`, which is inactive: one of the main
/// thread is called at once; one of a thread of its own starts it, and
/// activates there, its failure coming through [`Plugins::serve`]. The
/// error of an activation that failed at once, which disables it.
fn start(
    entry: &mut Entry,
    context: &mut Context<'_>,
    (out, declarations_changed): (&mut Outbox, &mut bool),
    wake: Option<&worker::Wake>,
) -> Result<(), Error> {
    // The capabilities that the user did not decide on yet, asked for
    // once, its activation waiting for the answer.
    if !entry.state.undecided.is_empty() {
        if !entry.state.asking {
            entry.state.asking = true;
            out.requests.push(Request::AskConsent {
                plugin: entry.state.name.clone(),
                capabilities: entry.state.undecided.clone(),
            });
        }
        return Ok(());
    }
    entry.state.status = Status::Active;
    entry.state.ran = true;
    entry.state.learn_tabs(context.workspace);
    let Entry { runner, state, .. } = entry;
    let plugin = match runner {
        Runner::Main(plugin) => plugin,
        Runner::Unstarted(_) => {
            let Runner::Unstarted(plugin) = std::mem::replace(runner, Runner::Stopped) else {
                return Ok(());
            };
            let wake = wake.cloned().unwrap_or_else(|| Arc::new(|| {}));
            return match Worker::start(&state.name, plugin, wake) {
                Ok(mut worker) => {
                    // Its first call, of nothing waiting.
                    let _ = worker.ask(Job::Activate, context.focused);
                    *runner = Runner::Worker(Box::new(worker));
                    Ok(())
                }
                Err(error) => Err(failed_start(
                    state,
                    context.registry,
                    declarations_changed,
                    &Error::new(
                        Code::Unavailable,
                        format!("its thread did not start: {error}"),
                    ),
                )),
            };
        }
        Runner::Worker(_) | Runner::Stopped => return Ok(()),
    };
    let activated = call(
        (plugin.as_mut(), state),
        context,
        out,
        declarations_changed,
        "activate",
        |plugin, cx| plugin.activate(cx),
    );
    match activated {
        Ok(()) => Ok(()),
        Err(error) if state.status == Status::Active => Err(failed_start(
            state,
            context.registry,
            declarations_changed,
            &error,
        )),
        Err(error) => Err(error),
    }
}

/// Gives the plugin of `state` the capabilities of its manifest that
/// `granted` holds, but the host's withheld ones; those that neither
/// `granted` nor `refused` holds are undecided, which its activation asks
/// for, unless a question of them is out.
fn decide(state: &mut State, granted: &[Capability], refused: &[Capability]) {
    let declared: Vec<Capability> = state
        .manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .capabilities
                .iter()
                .filter(|capability| !withheld_from(manifest, capability))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    state.granted = Some(
        declared
            .iter()
            .filter(|capability| granted.contains(capability))
            .cloned()
            .collect(),
    );
    if state.asking {
        // The question's capabilities that the user decided meanwhile, as
        // of a file fixed, are its no longer: its answer leaves them.
        state
            .undecided
            .retain(|capability| !granted.contains(capability) && !refused.contains(capability));
    } else {
        state.undecided = declared
            .into_iter()
            .filter(|capability| !granted.contains(capability) && !refused.contains(capability))
            .collect();
    }
}

/// The slot that `view` is of, if it is one's, as a panel is not.
fn view_slot(view: &ViewId) -> Option<Slot> {
    Some(match view {
        ViewId::TabBar(_) => Slot::TabBar,
        ViewId::WindowStrip(_) => Slot::WindowStrip,
        ViewId::StatusBar(_) => Slot::StatusBar,
        ViewId::PaneHeader(_) => Slot::PaneHeader,
        ViewId::TitleButtons(_) => Slot::TitleButtons,
        ViewId::Panel(_) => return None,
    })
}

/// Stops the plugin of `entry`, which its section of the config turns off:
/// its commands and its bindings go, as a disabled plugin's, its panels
/// close, as the app hears, its decorations and its gutters' marks leave
/// their panes, its views
/// show no more, the places of its tab bar, its fade and its keyboard's
/// following go, for the plugin made anew to ask again, and its thread,
/// if it has one, ends.
fn stop(
    entry: &mut Entry,
    context: &mut Context<'_>,
    out: &mut Outbox,
    declarations_changed: &mut bool,
) {
    let state = &mut entry.state;
    disable(state, context.registry, declarations_changed);
    *declarations_changed = true;
    clear_leftovers(state, context, out);
    state.views.clear();
    state.subscribed.clear();
    state.bar_place = host::BarPlace::default();
    state.window_places.clear();
    state.dim = 0.0;
    state.follows = false;
    if let Runner::Worker(_) = entry.runner {
        entry.runner = Runner::Stopped;
    }
}

/// Takes away what the plugin of `state`, which stops or a fault disabled,
/// left in the panes and the app: its decorations and its gutters' marks
/// leave their panes, the cursors that it held go back to the terminals,
/// its panels close, as the app hears through `out`, its searches end,
/// their matches gone, its reads of rows stop at their next parts, and its
/// timers end. Its views stay as they were, of a disabled plugin's, which
/// take no event.
fn clear_leftovers(state: &mut State, context: &mut Context<'_>, out: &mut Outbox) {
    for (id, pane) in std::mem::take(&mut state.decorations) {
        if let Some(pane) = context.workspace.pane(PaneId::of(pane.0)) {
            pane.undecorate(id);
        }
    }
    for (id, pane) in std::mem::take(&mut state.gutter_marks) {
        if let Some(pane) = context.workspace.pane(PaneId::of(pane.0)) {
            pane.unmark_gutter(id);
        }
    }
    release_cursors(state, context.workspace);
    state.awake = host::Awake::Off;
    for id in std::mem::take(&mut state.panels).into_keys() {
        out.requests.push(Request::ClosePanel {
            plugin: state.name.clone(),
            id,
        });
    }
    out.reads.retain(|read| read.plugin != state.name);
    state.timers.clear();
    let searches = std::mem::take(&mut state.searches);
    for search in searches
        .into_iter()
        .chain(std::mem::take(&mut state.lingering))
        .chain(std::mem::take(&mut state.replaced))
    {
        let pane = context.workspace.pane(PaneId::of(search.pane.0));
        search.end(pane.and_then(Pane::terminal).as_deref_mut());
    }
}

/// Whether `view`, of the plugin of `state`, shows: a slot's of a window
/// or a pane that is there, or one of its open panels.
fn shows(state: &State, view: &ViewId, workspace: &Workspace) -> bool {
    match view {
        ViewId::TabBar(window)
        | ViewId::WindowStrip(window)
        | ViewId::StatusBar(window)
        | ViewId::TitleButtons(window) => workspace.window(WindowId::of(window.0)).is_some(),
        ViewId::PaneHeader(pane) => workspace.pane(PaneId::of(pane.0)).is_some(),
        ViewId::Panel(id) => state.panels.contains_key(id),
    }
}

/// Disables the plugin of `state`, whose activation failed with `error`,
/// as [`disable`] does: the error that tells it, which its fault keeps.
fn failed_start(
    state: &mut State,
    registry: &mut Registry,
    declarations_changed: &mut bool,
    error: &Error,
) -> Error {
    disable(state, registry, declarations_changed);
    let message = format!(
        "the plugin `{}` did not start, and is disabled: {}",
        state.name, error.message
    );
    state.fault = Some(message.clone());
    Error::new(error.code, message)
}

/// Ends the searches of `lingering` of `pane`, of `workspace`, their
/// matches gone.
fn end_lingering(lingering: &mut Vec<search::Search>, pane: host::PaneId, workspace: &Workspace) {
    let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(lingering)
        .into_iter()
        .partition(|search| search.pane == pane);
    *lingering = kept;
    if gone.is_empty() {
        return;
    }
    let mut terminal = workspace.pane(PaneId::of(pane.0)).and_then(Pane::terminal);
    for search in gone {
        search.end(terminal.as_deref_mut());
    }
}

/// Whether the plugin of `runner` can take its timers' events now: one of
/// the main thread, or one of a thread of its own that is free.
fn takes_timers(runner: &Runner) -> bool {
    match runner {
        Runner::Main(_) => true,
        Runner::Worker(worker) => !worker.is_busy(),
        Runner::Unstarted(_) | Runner::Stopped => false,
    }
}

/// Takes the events of the timers of the plugin of `state` that fired
/// since it last heard of them, in their ids' order: a timer that does not
/// repeat goes with its event.
fn fired(state: &mut State) -> Vec<Event> {
    let mut events = Vec::new();
    state.timers.retain(|&id, timer| {
        if timer.due {
            timer.due = false;
            events.push(Event::Timer { id });
        }
        timer.at.is_some()
    });
    events
}

/// Ends the searches of `ids` of the plugin of `state`, which a call that
/// failed started, of `workspace`: its ids name nothing, as the call left
/// things as they were.
fn end_searches(state: &mut State, ids: &[u64], workspace: &Workspace) {
    let started = |search: &search::Search| ids.contains(&search.id);
    let (ended, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut state.searches)
        .into_iter()
        .chain(std::mem::take(&mut state.replaced))
        .partition(started);
    // Those that the call replaced, which it did not start, run on.
    state.searches = kept;
    for search in ended {
        let mut terminal = workspace
            .pane(PaneId::of(search.pane.0))
            .and_then(Pane::terminal);
        search.end(terminal.as_deref_mut());
    }
}

/// Settles the searches that the call of the plugin of `state` replaced,
/// as it returns: each stops, its matches gone; shown, of a pane there
/// still, they show until its pane's search shows its own, as one that
/// lingered before, which goes.
fn settle_replaced(state: &mut State, workspace: &Workspace) {
    for search in std::mem::take(&mut state.replaced) {
        let pane = search.pane;
        let mut terminal = workspace.pane(PaneId::of(pane.0)).and_then(Pane::terminal);
        if search.is_shown() && terminal.is_some() {
            drop(terminal);
            end_lingering(&mut state.lingering, pane, workspace);
            search.stop();
            state.lingering.push(search);
        } else {
            search.end(terminal.as_deref_mut());
        }
    }
}

/// Applies a call's `writes`, of the plugin of `state`, to the kernel's
/// state, in their order: the writes to a pane that closed since do
/// nothing; a paste that asks the user first becomes a request of the
/// app's; an event emitted waits for the next delivery.
fn apply(writes: Vec<Write>, context: &mut Context<'_>, out: &mut Outbox, state: &mut State) {
    let plugin = state.name.as_str();
    out.written |= writes
        .iter()
        .any(|write| !matches!(write, Write::Render(_)));
    let requests = &mut out.requests;
    for write in writes {
        let pane = |pane: host::PaneId| context.workspace.pane(PaneId::of(pane.0));
        let write = match write {
            Write::Asked(write) => write,
            Write::Decorate {
                id,
                pane: on,
                from,
                to,
                look,
            } => {
                if !pane(on).is_some_and(|pane| pane.decorate(id, from, to, decoration(look))) {
                    state.decorations.remove(&id);
                }
                continue;
            }
            Write::MarkGutter {
                id,
                pane: on,
                row,
                color: (r, g, b),
            } => {
                if !pane(on).is_some_and(|pane| pane.mark_gutter(id, row, Rgb { r, g, b })) {
                    state.gutter_marks.remove(&id);
                }
                continue;
            }
            // A tab or a split of a window or a pane that an earlier write
            // of the call closed starts no program.
            Write::NewTab {
                window,
                index,
                tab,
                pane,
                directory,
            } => {
                if context.workspace.window(window).is_some() {
                    let near = context.workspace.active_pane(window);
                    let near = near.and_then(|near| context.workspace.pane(near));
                    let new = context.panes.spawn(pane, near, directory.as_deref());
                    context
                        .workspace
                        .open_tab_as(window, new, index.unwrap_or(usize::MAX), tab);
                }
                continue;
            }
            Write::Split {
                pane: split,
                direction,
                side,
                new,
            } => {
                if let Some(near) = context.workspace.pane(split) {
                    let new = context.panes.spawn(new, Some(near), None);
                    context.workspace.split(split, direction, new, side);
                }
                continue;
            }
            Write::Render(view) => {
                if !out
                    .renders
                    .iter()
                    .any(|(by, asked)| by == plugin && *asked == view)
                {
                    out.renders.push((plugin.to_owned(), view));
                }
                continue;
            }
        };
        match write {
            Asked::SetClipboardText(text) => context.clipboard.set_text(&text),
            Asked::SetClipboardImage(png) => context.clipboard.set_image(&png.0),
            Asked::KeepAwake(awake) => state.awake = awake,
            Asked::DropImage(id) => context.images.drop_image(&state.name, id),
            Asked::SelectAll(id) => {
                if let Some(pane) = pane(id) {
                    pane.select_all();
                }
            }
            Asked::SelectCells {
                pane: id,
                first,
                last,
                kind,
            } => {
                if let Some(pane) = pane(id) {
                    let (shape, unit) = match kind {
                        host::SelectionKind::Cells => (SelectionShape::Cells, SelectionUnit::Cell),
                        host::SelectionKind::Words => (SelectionShape::Cells, SelectionUnit::Word),
                        host::SelectionKind::Lines => (SelectionShape::Cells, SelectionUnit::Line),
                        host::SelectionKind::Block => {
                            (SelectionShape::Rectangle, SelectionUnit::Cell)
                        }
                    };
                    pane.select(first, last, shape, unit);
                    pane.show_row(last.1);
                }
            }
            Asked::Deselect(id) => {
                if let Some(pane) = pane(id) {
                    pane.clear_selection();
                }
            }
            // The plugin's cursor, which its stop takes away.
            Asked::ShowCursor { pane: id, at } => {
                if let Some(pane) = pane(id) {
                    pane.hold_cursor(at);
                    if at.is_some() {
                        state.held_cursors.insert(id);
                    } else {
                        state.held_cursors.remove(&id);
                    }
                }
            }
            // Of a search that the call or an earlier one did not end; the
            // one before of its pane, which lingered, goes.
            Asked::ShowMatches {
                search,
                all,
                current,
            } => {
                let Some(kept) = state.searches.iter_mut().find(|kept| kept.id == search) else {
                    continue;
                };
                let Some(found) = context.workspace.pane(PaneId::of(kept.pane.0)) else {
                    continue;
                };
                let showed = found
                    .terminal()
                    .is_some_and(|mut terminal| kept.show(&mut terminal, all, current));
                if showed && let Some(current) = current {
                    found.show_row(current.first.1);
                }
                let shown = kept.pane;
                end_lingering(&mut state.lingering, shown, context.workspace);
            }
            Asked::ClearScrollback(id) => {
                if let Some(pane) = pane(id) {
                    pane.clear_scrollback();
                }
            }
            Asked::Clear(id) => {
                if let Some(pane) = pane(id) {
                    pane.clear();
                }
            }
            Asked::Paste { pane: id, text } => {
                let Some(found) = pane(id) else {
                    continue;
                };
                let bracketed = found
                    .input_modes()
                    .is_some_and(|modes| modes.bracketed_paste);
                match paste::decide(&text, bracketed, context.confirm_paste) {
                    Paste::Now(text) => found.paste(&text),
                    Paste::Ask => requests.push(Request::ConfirmPaste {
                        pane: PaneId::of(id.0),
                        text,
                    }),
                }
            }
            Asked::SendText { pane: id, text } => {
                if let Some(pane) = pane(id) {
                    pane.send_text(&text);
                }
            }
            Asked::Scroll { pane: id, scroll } => {
                if let Some(pane) = pane(id) {
                    pane.scroll(scrolled(scroll));
                }
            }
            Asked::Undecorate { pane: on, id } => {
                if let Some(pane) = pane(on) {
                    pane.undecorate(id);
                }
                state.decorations.remove(&id);
            }
            Asked::UnmarkGutter { pane: on, id } => {
                if let Some(pane) = pane(on) {
                    pane.unmark_gutter(id);
                }
                state.gutter_marks.remove(&id);
            }
            Asked::Emit {
                name,
                schema,
                value,
            } => out.events.push((
                None,
                Event::Custom {
                    name,
                    schema,
                    value,
                },
            )),
            // The workspace's operations do nothing to what an earlier write
            // of the call closed, and record what they change.
            // A close that asks goes to the app, which asks the user as it
            // asks for a window.
            Asked::CloseTab { tab, ask: true } => {
                requests.push(Request::CloseAsked(Closing::Tab(TabId::of(tab.0))));
            }
            Asked::ClosePane { pane, ask: true } => {
                requests.push(Request::CloseAsked(Closing::Pane(PaneId::of(pane.0))));
            }
            Asked::CloseTab { tab, ask: false } => {
                context.workspace.close_tab(TabId::of(tab.0));
            }
            Asked::ClosePane { pane, ask: false } => {
                context.workspace.close_pane(PaneId::of(pane.0));
            }
            Asked::SetTabTitle { tab, title } => {
                context.workspace.set_tab_title(TabId::of(tab.0), title);
            }
            Asked::SetTabBarPlace(place) => state.bar_place = place,
            Asked::SetInactiveDim(dim) => state.dim = dim,
            Asked::SetFocusFollowsMouse(on) => state.follows = on,
            Asked::SetWindowTabBarPlace { window, place } => {
                // The places of windows that closed go.
                state
                    .window_places
                    .retain(|kept, _| context.workspace.window(WindowId::of(kept.0)).is_some());
                match place {
                    Some(place) => {
                        state.window_places.insert(window, place);
                    }
                    None => {
                        state.window_places.remove(&window);
                    }
                }
            }
            Asked::SetTabColor { tab, color } => {
                let color = match color {
                    Some(host::Color::Rgb(red, green, blue)) => Some(Rgb::new(red, green, blue)),
                    _ => None,
                };
                context.workspace.set_tab_color(TabId::of(tab.0), color);
            }
            Asked::ActivateTab(tab) => {
                context.workspace.activate_tab(TabId::of(tab.0));
            }
            Asked::FocusPane(pane) => {
                context.workspace.focus_pane(PaneId::of(pane.0));
            }
            Asked::MoveTab { tab, window, index } => {
                context
                    .workspace
                    .move_tab(TabId::of(tab.0), WindowId::of(window.0), index);
            }
            Asked::TabToNewWindow { tab, at } => {
                let window = context.workspace.tab_to_new_window(TabId::of(tab.0));
                if let (Some(window), Some(at)) = (window, at) {
                    requests.push(Request::PlaceWindow { window, at });
                }
            }
            Asked::PaneToNewTab {
                pane,
                window,
                index,
            } => {
                context.workspace.pane_to_new_tab(
                    PaneId::of(pane.0),
                    WindowId::of(window.0),
                    index,
                );
            }
            Asked::PaneToNewWindow(pane) => {
                context.workspace.pane_to_new_window(PaneId::of(pane.0));
            }
            Asked::ResizePane {
                pane,
                edge: moved,
                by,
            } => {
                context
                    .workspace
                    .resize_pane(PaneId::of(pane.0), edge(moved), by);
            }
            Asked::TabIntoPane {
                tab,
                pane,
                edge: at,
            } => {
                context
                    .workspace
                    .tab_into_pane(TabId::of(tab.0), PaneId::of(pane.0), edge(at));
            }
            Asked::SetRatio { tab, path, ratio } => {
                let path: Vec<Side> = path.into_iter().map(side).collect();
                context
                    .workspace
                    .set_ratio(TabId::of(tab.0), &path, ratio.map(Ratio::new));
            }
            Asked::Equalize(tab) => {
                context.workspace.equalize(TabId::of(tab.0));
            }
            Asked::SwapPanes { pane, with } => {
                context
                    .workspace
                    .swap_panes(PaneId::of(pane.0), PaneId::of(with.0));
            }
            Asked::Zoom { tab, pane } => {
                context
                    .workspace
                    .zoom(TabId::of(tab.0), pane.map(|pane| PaneId::of(pane.0)));
            }
            Asked::SetState(value) => {
                let Some(file) = &state.stored.file else {
                    continue;
                };
                let text = json(&value).unwrap_or(Value::Null).to_string();
                if let Err(error) = crate::state::write_whole(file, text.as_bytes()) {
                    out.faults.push(Error::new(
                        Code::Failed,
                        format!(
                            "the state of the plugin `{}` cannot be written to {}: {error}",
                            state.name,
                            file.display()
                        ),
                    ));
                }
                state.stored.held = Some(value);
            }
            // The host queues writes alone.
            _ => {}
        }
    }
}

/// The most levels of a tree that a look for a key goes down, as the host
/// refuses a deeper tree.
const DEEPEST: usize = 64;

/// The node of `key` in `node`'s tree, `depth` levels down already, if it
/// has one within [`DEEPEST`] levels.
fn keyed<'a>(node: &'a Node, key: &Key, depth: usize) -> Option<&'a Node> {
    if node.key.as_ref() == Some(key) {
        return Some(node);
    }
    if depth >= DEEPEST {
        return None;
    }
    node.children
        .iter()
        .find_map(|child| keyed(child, key, depth + 1))
}

/// The workspace's direction of a split of the SDK's.
fn direction(direction: host::Direction) -> Direction {
    match direction {
        host::Direction::Horizontal => Direction::Horizontal,
        host::Direction::Vertical => Direction::Vertical,
    }
}

/// The workspace's part of a split of the SDK's.
fn side(side: host::Side) -> Side {
    match side {
        host::Side::First => Side::First,
        host::Side::Second => Side::Second,
    }
}

/// The workspace's edge of a pane of the SDK's.
fn edge(edge: host::Edge) -> Edge {
    match edge {
        host::Edge::Left => Edge::Left,
        host::Edge::Right => Edge::Right,
        host::Edge::Top => Edge::Top,
        host::Edge::Bottom => Edge::Bottom,
    }
}

/// The SDK's layout of a tab's `layout`.
fn layout(layout: &Layout) -> host::Layout {
    match layout {
        Layout::Leaf(pane) => host::Layout::Pane((*pane).into()),
        Layout::Split {
            direction,
            ratio,
            first,
            second,
        } => host::Layout::Split {
            direction: match direction {
                Direction::Horizontal => host::Direction::Horizontal,
                Direction::Vertical => host::Direction::Vertical,
            },
            ratio: ratio.map(Ratio::get),
            first: Box::new(self::layout(first)),
            second: Box::new(self::layout(second)),
        },
    }
}

/// The terminal's look of a decoration of the SDK's.
fn decoration(look: host::DecorationLook) -> Look {
    let rgb = |(r, g, b): (u8, u8, u8)| Rgb { r, g, b };
    Look {
        underline: match look.underline {
            None => page::Underline::None,
            Some(host::Underline::Single) => page::Underline::Single,
            Some(host::Underline::Double) => page::Underline::Double,
            Some(host::Underline::Curly) => page::Underline::Curly,
            Some(host::Underline::Dotted) => page::Underline::Dotted,
            Some(host::Underline::Dashed) => page::Underline::Dashed,
        },
        underline_color: look.underline_color.map(rgb),
        background: look.background.map(rgb),
        pointing: look.pointing,
    }
}

/// The SDK's style of a run of the terminal's.
fn text_style(style: &Style) -> host::TextStyle {
    let color = |color: page::Color| match color {
        page::Color::Default => None,
        page::Color::Palette(index) => Some(host::Color::Palette(index)),
        page::Color::Rgb(Rgb { r, g, b }) => Some(host::Color::Rgb(r, g, b)),
    };
    host::TextStyle {
        foreground: color(style.fg),
        background: color(style.bg),
        underline: match style.underline {
            page::Underline::None => None,
            page::Underline::Single => Some(host::Underline::Single),
            page::Underline::Double => Some(host::Underline::Double),
            page::Underline::Curly => Some(host::Underline::Curly),
            page::Underline::Dotted => Some(host::Underline::Dotted),
            page::Underline::Dashed => Some(host::Underline::Dashed),
        },
        underline_color: color(style.underline_color),
        bold: style.bold,
        faint: style.faint,
        italic: style.italic,
        blink: style.blink,
        inverse: style.inverse,
        invisible: style.invisible,
        strikethrough: style.strikethrough,
        overline: style.overline,
    }
}

/// The pairs of `catalogs`, each its language and its source, as the texts
/// read them.
fn pairs(catalogs: &lokyr_plugin_sdk::plugin::Catalogs) -> Vec<(&str, &str)> {
    catalogs
        .iter()
        .map(|(language, source)| (language.as_ref(), source.as_ref()))
        .collect()
}

/// The rows of `cells`, as an image draws them, of the plugins' styles.
pub fn cell_rows(cells: Vec<lokyr_vt_core::terminal::RowCells>) -> Vec<image::CellRow> {
    cells
        .into_iter()
        .map(|row| image::CellRow {
            text: row.text,
            runs: row
                .runs
                .iter()
                .map(|(first, last, style)| host::StyleRun {
                    first: *first,
                    last: *last,
                    style: text_style(style),
                })
                .collect(),
        })
        .collect()
}

/// The bytes of the file at `path`, `most` of them at most: `None` of a
/// longer one, which it reads no further than that.
fn read_capped(path: &std::path::Path, most: usize) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(most).unwrap_or(u64::MAX).saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() <= most).then_some(bytes))
}

/// Reads `count` rows of `reader`'s terminal from the row numbered
/// `first`, of the rows' `epoch`, the request's, on a thread of its own,
/// [`READ_PART`] rows under the lock at a time, and wakes the main thread
/// once it is done: the rows, and whether the read went to its end, as the
/// rows' epoch stayed from the request to the last part, come through the
/// receiver. `cancel` stops it at its next part; `reading`, its plugin's
/// count of reads under way, counts it until its thread ends.
fn read_rows(
    reader: Reader,
    (epoch, first, count): (u64, i64, usize),
    cancel: Arc<AtomicBool>,
    reading: Arc<AtomicUsize>,
) -> mpsc::Receiver<(Vec<host::Line>, bool)> {
    /// Counts a read under way until it drops, at its thread's end, as it
    /// ends or panics, or as its thread does not start.
    struct Counted(Arc<AtomicUsize>);
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }
    reading.fetch_add(1, Ordering::AcqRel);
    let counted = Counted(reading);
    let (sender, receiver) = mpsc::channel();
    let read = move || {
        let _counted = counted;
        let (mut lines, mut next, mut complete) = (Vec::new(), first, true);
        while lines.len() < count {
            if cancel.load(Ordering::Relaxed) {
                complete = false;
                break;
            }
            let part = {
                let terminal = reader.lock();
                if terminal.origin().epoch != epoch {
                    complete = false;
                    break;
                }
                terminal.read_rows(next, READ_PART.min(count - lines.len()))
            };
            let Some(last) = part.last() else {
                break;
            };
            next = last.row + 1;
            lines.extend(part.into_iter().map(|row| host::Line {
                row: row.row,
                text: row.text,
                wrapped: row.wrapped,
            }));
        }
        // A receiver that went, as its plugin's look's, takes nothing.
        let _ = sender.send((lines, complete));
        reader.wake();
    };
    // A thread that does not start drops the sender, which tells the read
    // broken.
    let _ = std::thread::Builder::new()
        .name("lokyr-read".to_owned())
        .spawn(read);
    receiver
}

/// The terminal's scroll of the SDK's.
fn scrolled(scroll: host::Scroll) -> Scroll {
    let rows = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    match scroll {
        host::Scroll::Up(n) => Scroll::Up(rows(n)),
        host::Scroll::Down(n) => Scroll::Down(rows(n)),
        host::Scroll::PageUp => Scroll::PageUp,
        host::Scroll::PageDown => Scroll::PageDown,
        host::Scroll::Top => Scroll::Top,
        host::Scroll::Bottom => Scroll::Bottom,
        host::Scroll::PreviousPrompt => Scroll::PreviousPrompt,
        host::Scroll::NextPrompt => Scroll::NextPrompt,
        host::Scroll::Row(row) => Scroll::Row(row),
    }
}

/// `: ` and the message of a panic's `payload`, when it has one.
fn message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .map(|message| format!(": {message}"))
        .unwrap_or_default()
}

/// Disables the plugin of `state`, unless the work that waits for its
/// thread is `within` its bound: its thread, which took none of it, is
/// left to end, as the calls that waited go, which a notice tells.
fn within_bound(
    within: bool,
    (runner, state): (&mut Runner, &mut State),
    context: &mut Context<'_>,
    (out, declarations_changed): (&mut Outbox, &mut bool),
) -> Result<(), Error> {
    if within {
        return Ok(());
    }
    disable(state, context.registry, declarations_changed);
    clear_leftovers(state, context, out);
    let fault = format!(
        "the plugin `{}` took none of the {} calls or {} events that waited for its thread; it is disabled",
        state.name,
        worker::MOST_WAITING,
        worker::MOST_EVENTS
    );
    state.fault = Some(fault.clone());
    *runner = Runner::Stopped;
    Err(Error::new(Code::Fault, fault))
}

/// Gives the terminal's cursor back in each pane where the plugin of
/// `state` shows one of its own, as it stops or a panic disables it, so
/// that no cursor stays that nothing moves.
fn release_cursors(state: &mut State, workspace: &Workspace) {
    for pane in std::mem::take(&mut state.held_cursors) {
        if let Some(pane) = workspace.pane(PaneId::of(pane.0)) {
            pane.hold_cursor(None);
        }
    }
}

/// Disables the plugin of `state`: its commands leave `registry`, and its
/// bindings and its menus' items go.
fn disable(state: &mut State, registry: &mut Registry, declarations_changed: &mut bool) {
    state.status = Status::Disabled;
    state.woken.clear();
    if !state.commands.is_empty()
        || !state.bindings.is_empty()
        || !state.global_bindings.is_empty()
        || !state.menu_items.is_empty()
    {
        *declarations_changed = true;
    }
    for command in state.commands.drain(..) {
        registry.unregister(&command.name);
    }
    state.bindings.clear();
    state.global_bindings.clear();
    state.menu_items.clear();
}

/// Whether the plugin of `runner` activates on its thread.
fn activating(runner: &Runner) -> bool {
    matches!(runner, Runner::Worker(worker) if worker.activating())
}

/// The arguments `args` of a call of `command`, which the plugin of `state`
/// registered as its trigger woke it, checked by its command, with the
/// defaults of those left out; or why the call fails, as the plugin
/// registered no such command.
fn own_call(state: &State, command: &str, args: Args) -> Result<Args, Error> {
    let Some(own) = state.commands.iter().find(|own| own.name == command) else {
        return Err(Error::new(
            Code::NotFound,
            format!(
                "the plugin `{}` activated, and registers no `{command}`",
                state.name
            ),
        ));
    };
    own.check_call(args)
}

/// Gives the commands that `requests` run the chain `chain`, of the
/// command whose call asked them, or a new chain, one for all of them, of a
/// call of no command's, as `chain`'s id of 0 tells: so that each call's
/// runs count against one chain's bound, however many it asks.
fn deepen(requests: &mut [Request], chain: Chain) {
    let chain = if chain.id == 0 { new_chain() } else { chain };
    for request in requests {
        if let Request::Run { chain: asked, .. } = request {
            *asked = chain;
        }
    }
}

/// The schemes of the URLs that a plugin can open: none that starts another
/// application on the user's behalf.
const SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

/// What opening `target` opens: a URL of one of [`SCHEMES`], a `file`
/// URL of this machine alone, as OSC 7 takes its folder, or a path,
/// absolute or from `~`, which becomes the home directory.
fn opening(target: &str) -> Result<String, String> {
    if let Some(host) = host::file_host(target)
        && !host::is_local_host(host, &lokyr_pty::host_name())
    {
        return Err(format!(
            "a `file:` URL of another machine, `{host}`, does not open"
        ));
    }
    if target.starts_with('/') {
        return Ok(target.to_owned());
    }
    if target == "~" || target.starts_with("~/") {
        let home = std::env::home_dir().ok_or("there is no home directory")?;
        return Ok(format!("{}{}", home.to_string_lossy(), &target[1..]));
    }
    let scheme = target
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| {
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        });
    match scheme {
        Some(scheme) if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) => {
            Ok(target.to_owned())
        }
        Some(scheme) => Err(format!(
            "`{scheme}:` does not open: only {}",
            SCHEMES.join(", ")
        )),
        None => Err(format!(
            "`{target}` is neither a URL nor a path that is absolute or from `~`"
        )),
    }
}

/// Whether opening `target` would open something: a URL of one of
/// [`SCHEMES`], or a path, absolute or from `~`, that is there, as a link
/// that leads somewhere.
fn opens(target: &str) -> bool {
    opening(target)
        .is_ok_and(|target| !target.starts_with('/') || std::path::Path::new(&target).exists())
}

/// The host of one plugin's call.
struct Host<'h, 'c> {
    state: &'h mut State,
    context: &'h mut Context<'c>,
    out: &'h mut Outbox,
    /// The call's writes, which it applies as the call returns.
    writes: &'h mut Vec<Write>,
    declarations_changed: &'h mut bool,
    /// Whether the call only looks, as `applies`, which declares nothing.
    looking: bool,
    /// What the call asked of the app and searched so far.
    tally: &'h mut Tally,
}

/// What a call's requests asked of the app, which count with its writes;
/// the searches that they started, which a call that fails ends; the images
/// that they rendered; and the panels that they closed, whose trees go and whose plugin hears that
/// they closed as the call returns, unless it fails.
#[derive(Debug, Default)]
pub(super) struct Tally {
    asked: usize,
    searches: Vec<u64>,
    /// How many images the call rendered.
    images: usize,
    closed: Vec<PanelId>,
}

/// The panels that a call closed, of the plugin of `state`, as the call
/// returns: their trees go, and the plugin hears that they closed.
fn closed_panels(state: &mut State, out: &mut Outbox, closed: Vec<PanelId>) {
    for id in closed {
        state.views.remove(&ViewId::Panel(id.clone()));
        out.events
            .push((Some(state.name.clone()), Event::PanelClosed(id)));
    }
}

impl Host<'_, '_> {
    /// The plugin's state, as it last wrote it: of its file, read the first
    /// time, whose failure tells the log as the state starts anew.
    fn stored(&mut self) -> Data {
        if let Some(held) = &self.state.stored.held {
            return held.clone();
        }
        let read = match &self.state.stored.file {
            None => Ok(Data::Nil),
            Some(file) => match read_capped(file, MOST_STATE) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Data::Nil),
                Err(error) => Err(format!(
                    "it cannot be read from {}: {error}",
                    file.display()
                )),
                Ok(None) => Err(format!(
                    "{} holds more than {MOST_STATE} bytes, the most of a state",
                    file.display()
                )),
                Ok(Some(bytes)) => serde_json::from_slice::<Value>(&bytes)
                    .map(|value| data(&value))
                    .map_err(|error| format!("{} is not JSON: {error}", file.display())),
            },
        };
        let held = read.unwrap_or_else(|message| {
            self.out.faults.push(Error::new(
                Code::Failed,
                format!(
                    "the state of the plugin `{}` starts anew, as {message}",
                    self.state.name
                ),
            ));
            Data::Nil
        });
        self.state.stored.held = Some(held.clone());
        held
    }

    /// The pane of `pane`'s handle; `not-found` once it closed.
    fn pane(&self, pane: host::PaneId) -> Result<&Pane, Error> {
        self.context
            .workspace
            .pane(PaneId::of(pane.0))
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// The window of `window`'s handle; `not-found` once it closed.
    fn window(&self, window: host::WindowId) -> Result<&crate::workspace::Window, Error> {
        self.context
            .workspace
            .window(WindowId::of(window.0))
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no window {}", window.0)))
    }

    /// The tab of `tab`'s handle; `not-found` once it closed.
    fn tab(&self, tab: host::TabId) -> Result<&crate::workspace::Tab, Error> {
        self.context
            .workspace
            .tab(TabId::of(tab.0))
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no tab {}", tab.0)))
    }

    /// The pane of the tab `tab` that the call's writes so far open, whose
    /// ids their request gave, if they open it.
    fn opened_tab(&self, tab: host::TabId) -> Option<PaneId> {
        self.writes.iter().find_map(|write| match write {
            Write::NewTab {
                tab: opened, pane, ..
            } if opened.get() == tab.0 => Some(*pane),
            _ => None,
        })
    }

    /// Whether the call's writes so far open the pane `pane`, of a new tab
    /// or of a split.
    fn opens_pane(&self, pane: host::PaneId) -> bool {
        self.writes.iter().any(|write| match write {
            Write::NewTab { pane: opened, .. } | Write::Split { new: opened, .. } => {
                opened.get() == pane.0
            }
            _ => false,
        })
    }

    /// The tab of `pane`, of a tab or a split that the call's writes so far
    /// open, if they open it.
    fn opened_tab_of(&self, pane: host::PaneId) -> Option<TabId> {
        self.writes.iter().find_map(|write| match write {
            Write::NewTab {
                tab, pane: opened, ..
            } if opened.get() == pane.0 => Some(*tab),
            Write::Split { pane: of, new, .. } if new.get() == pane.0 => self
                .context
                .workspace
                .tab_of(*of)
                .or_else(|| self.opened_tab_of(host::PaneId(of.get()))),
            _ => None,
        })
    }

    /// Whether the tab `tab` is there, or the call's writes so far open it,
    /// which a later write of the call then finds; `not-found` otherwise.
    fn known_tab(&self, tab: host::TabId) -> Result<(), Error> {
        if self.opened_tab(tab).is_some() {
            return Ok(());
        }
        self.tab(tab).map(|_| ())
    }

    /// Whether the pane `pane` is there, or the call's writes so far open
    /// it; `not-found` otherwise.
    fn known_pane(&self, pane: host::PaneId) -> Result<(), Error> {
        if self.opens_pane(pane) {
            return Ok(());
        }
        self.pane(pane).map(|_| ())
    }

    /// The window that `view` shows in, if the view is one of the plugin's:
    /// a slot's that its manifest declares, of a window or a pane that is
    /// there, or one of its open panels; `not-found` otherwise.
    fn view_window(&self, view: &ViewId) -> Result<WindowId, Error> {
        let not_found = || Error::new(Code::NotFound, format!("the plugin has no view {view:?}"));
        if let Some(slot) = view.slot()
            && !self.state.fills(slot)
        {
            return Err(not_found());
        }
        let workspace = &self.context.workspace;
        let window = match view {
            ViewId::TabBar(window)
            | ViewId::WindowStrip(window)
            | ViewId::StatusBar(window)
            | ViewId::TitleButtons(window) => {
                Some(WindowId::of(window.0)).filter(|&window| workspace.window(window).is_some())
            }
            ViewId::PaneHeader(pane) => workspace
                .tab_of(PaneId::of(pane.0))
                .and_then(|tab| workspace.tab(tab))
                .map(crate::workspace::Tab::window),
            ViewId::Panel(id) => self.state.panels.get(id).copied(),
        };
        window.ok_or_else(not_found)
    }

    /// The node of `key` in the last tree of `view`, one of the plugin's;
    /// `not-found` without one.
    fn node(&self, view: &ViewId, key: &Key) -> Result<&Node, Error> {
        self.view_window(view)?;
        self.state
            .views
            .get(view)
            .and_then(|(_, tree)| tree.as_ref())
            .and_then(|tree| keyed(tree, key, 0))
            .ok_or_else(|| {
                Error::new(
                    Code::NotFound,
                    format!("the view {view:?} has no node `{}`", key.0),
                )
            })
    }

    /// The window that a panel at `anchor` shows in: its pane's, its
    /// window, or its node's view's, whose node is there.
    fn anchor_window(&self, anchor: &Anchor) -> Result<WindowId, Error> {
        let pane_window = |pane: host::PaneId| {
            let workspace = &self.context.workspace;
            workspace
                .tab_of(PaneId::of(pane.0))
                .and_then(|tab| workspace.tab(tab))
                .map(crate::workspace::Tab::window)
                .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
        };
        match anchor {
            Anchor::Cell { pane, .. }
            | Anchor::Cursor(pane)
            | Anchor::PaneCorner { pane, .. }
            | Anchor::PaneTop(pane) => pane_window(*pane),
            Anchor::Pointer(window) | Anchor::WindowCorner { window, .. } => {
                Ok(self.window(*window)?.id())
            }
            Anchor::Node { view, key } => {
                self.node(view, key)?;
                self.view_window(view)
            }
        }
    }

    /// What a read of a window's tabs tells of `tab`.
    fn tab_info(&self, tab: &crate::workspace::Tab) -> host::TabInfo {
        tab_info(self.context.workspace, tab, self.state.reads_processes())
    }

    /// Whether the plugin may make `request`: it has the capability that
    /// the request needs, if it needs one.
    fn allows(&self, request: &Asked) -> Result<(), Error> {
        let Some(granted) = &self.state.granted else {
            return Ok(());
        };
        match request
            .capabilities()
            .into_iter()
            .find(|needed| !lokyr_plugin_sdk::command::grants(granted, needed))
        {
            Some(needed) => Err(Error::new(
                Code::MissingCapability,
                format!(
                    "the plugin `{}` lacks the capability `{needed}`",
                    self.state.name
                ),
            )),
            None => Ok(()),
        }
    }

    /// Whether a declared plugin may register the command `name`: of its
    /// own namespace, which no other plugin's command, nor the app's, has.
    fn of_its_namespace(&self, name: &str) -> Result<(), Error> {
        let namespace = &self.state.namespace;
        let prefix = format!("{namespace}.");
        if !name.starts_with(&prefix) {
            return Err(Error::new(
                Code::InvalidName,
                format!("`{name}` is not of the plugin's namespace, `{namespace}`"),
            ));
        }
        let registry = &self.context.registry;
        let taken = registry.in_order().any(|command| {
            command.name.starts_with(&prefix)
                && registry.owner(&command.name) != Some(self.state.name.as_str())
                && !self
                    .state
                    .commands
                    .iter()
                    .any(|own| own.name == command.name)
        });
        if taken {
            return Err(Error::new(
                Code::AlreadyRegistered,
                format!("the namespace `{namespace}` has another plugin's commands, or the app's"),
            ));
        }
        Ok(())
    }

    /// Answers `request`, a read, with the state as it is now.
    /// Whether the call may start a search: none of a call that only
    /// looks, and [`MOST_SEARCHES`] at most of one.
    fn searches_within(&self) -> Result<(), Error> {
        if self.looking {
            return Err(Error::new(
                Code::Failed,
                "a call that only looks, as `applies`, starts no search",
            ));
        }
        if self.tally.searches.len() >= MOST_SEARCHES {
            return Err(Error::new(
                Code::Failed,
                format!("a call starts {MOST_SEARCHES} searches at most"),
            ));
        }
        Ok(())
    }

    fn read(&mut self, request: Asked) -> Result<Response, Error> {
        Ok(match request {
            Asked::ClipboardText => Response::Text(self.context.clipboard.text()),
            Asked::FocusedWindow => Response::Window(
                self.context
                    .focused
                    .map(|window| host::WindowId(window.get())),
            ),
            Asked::FocusedPane => Response::Pane(
                self.context
                    .focused
                    .and_then(|window| self.context.workspace.active_pane(window))
                    .map(|pane| host::PaneId(pane.get())),
            ),
            Asked::SelectionText(pane) => Response::Text(self.pane(pane)?.selection_text()),
            Asked::LineText { pane, row, reach } => Response::Line(
                self.pane(pane)?
                    .line_text(row, reach.min(host::MOST_REACH))
                    .map(|(text, cells)| host::LineText { text, cells }),
            ),
            Asked::LinkAt { pane, column, row } => {
                Response::Text(self.pane(pane)?.link_at(column, row))
            }
            Asked::WorkingDirectory(pane) => Response::Text(
                self.pane(pane)?
                    .working_directory()
                    .map(|directory| directory.to_string_lossy().into_owned()),
            ),
            Asked::ReadLines { pane, first, count } => {
                let found = self.pane(pane)?;
                let unavailable = || {
                    Error::new(
                        Code::Unavailable,
                        format!("the pane {} has no terminal", pane.0),
                    )
                };
                // The rows' numbers are the request's, of the epoch now,
                // which a resize before the read's first part changes.
                let epoch = found.origin().ok_or_else(unavailable)?.epoch;
                let reader = found.reader().ok_or_else(unavailable)?;
                // A thread each, a few of them at a time.
                let reading = Arc::clone(&self.state.reading);
                if reading.load(Ordering::Acquire) >= MOST_READS {
                    return Err(Error::new(
                        Code::Unavailable,
                        format!(
                            "the plugin `{}` has {MOST_READS} reads of rows under way, the most, until one ends",
                            self.state.name
                        ),
                    ));
                }
                let id = new_id();
                let cancel = Arc::new(AtomicBool::new(false));
                let rows = read_rows(
                    reader,
                    (epoch, first, count.min(host::MOST_LINES)),
                    Arc::clone(&cancel),
                    reading,
                );
                self.out.reads.push(Read {
                    id,
                    plugin: self.state.name.clone(),
                    pane,
                    rows,
                    _cancel: Cancel(cancel),
                });
                Response::Reading(id)
            }
            Asked::Search { pane, query } => {
                self.searches_within()?;
                let found = self.pane(pane)?;
                let reader = found.reader().ok_or_else(|| {
                    Error::new(
                        Code::Unavailable,
                        format!("the pane {} has no terminal", pane.0),
                    )
                })?;
                let query =
                    search::query(&query).map_err(|message| Error::new(Code::Failed, message))?;
                // Its search of the pane before ends as the call returns, its
                // matches gone; shown, they show until this one shows its
                // own, as one that lingered before, which goes.
                if let Some(at) = self
                    .state
                    .searches
                    .iter()
                    .position(|kept| kept.pane == pane)
                {
                    let before = self.state.searches.remove(at);
                    self.state.replaced.push(before);
                }
                let id = new_id();
                self.state
                    .searches
                    .push(search::Search::start(id, pane, reader, query));
                self.tally.searches.push(id);
                Response::Searching(id)
            }
            Asked::EndSearch(id) => {
                if self.looking {
                    return Err(Error::new(
                        Code::Failed,
                        "a call that only looks, as `applies`, ends no search",
                    ));
                }
                if let Some(at) = self.state.searches.iter().position(|kept| kept.id == id) {
                    let ended = self.state.searches.remove(at);
                    end_lingering(
                        &mut self.state.lingering,
                        ended.pane,
                        self.context.workspace,
                    );
                    end_lingering(&mut self.state.replaced, ended.pane, self.context.workspace);
                    let pane = self.context.workspace.pane(PaneId::of(ended.pane.0));
                    ended.end(pane.and_then(Pane::terminal).as_deref_mut());
                }
                Response::Done
            }
            Asked::SearchMatches {
                search,
                first,
                count,
            } => {
                let not_found =
                    || Error::new(Code::NotFound, format!("there is no search {search}"));
                let kept = self
                    .state
                    .searches
                    .iter()
                    .find(|kept| kept.id == search)
                    .ok_or_else(not_found)?;
                let terminal = self
                    .context
                    .workspace
                    .pane(PaneId::of(kept.pane.0))
                    .and_then(Pane::terminal)
                    .ok_or_else(not_found)?;
                Response::Matches(kept.matches(&terminal, first, count.min(host::MOST_LINES)))
            }
            Asked::CurrentMatch(search) => {
                let not_found =
                    || Error::new(Code::NotFound, format!("there is no search {search}"));
                let kept = self
                    .state
                    .searches
                    .iter()
                    .find(|kept| kept.id == search)
                    .ok_or_else(not_found)?;
                let terminal = self
                    .context
                    .workspace
                    .pane(PaneId::of(kept.pane.0))
                    .and_then(Pane::terminal)
                    .ok_or_else(not_found)?;
                Response::Current(kept.current(&terminal))
            }
            Asked::Styles { pane, row } => {
                Response::Styles(self.pane(pane)?.row_styles(row).map(|runs| {
                    runs.iter()
                        .map(|(first, last, style)| host::StyleRun {
                            first: *first,
                            last: *last,
                            style: text_style(style),
                        })
                        .collect()
                }))
            }
            Asked::Opens(target) => Response::Boolean(opens(&target)),
            Asked::FontSize => Response::FontSize(self.context.font_size),
            Asked::KeysOf { command, args } => Response::Text((self.context.keys)(&command, &args)),
            Asked::Message { id, args } => {
                let args: Vec<(&str, Arg<'_>)> = args
                    .iter()
                    .filter_map(|(name, value)| {
                        let arg = match value {
                            Data::Text(text) => Arg::Text(text),
                            Data::Integer(number) => Arg::Number(*number),
                            _ => return None,
                        };
                        Some((name.as_str(), arg))
                    })
                    .collect();
                Response::Text(
                    Texts::new(self.context.language, &pairs(&self.state.catalogs))
                        .get_with(&id, &args),
                )
            }
            Asked::TabBarPlace => Response::TabBarPlace(self.state.bar_place),
            Asked::Windows => Response::Windows(
                self.context
                    .workspace
                    .windows()
                    .map(|window| host::WindowInfo {
                        id: window.id().into(),
                        tabs: window.tabs().iter().copied().map(Into::into).collect(),
                        active: window.active_tab().into(),
                        number: window.number(),
                        keys: window.keys().map(str::to_owned),
                        bell: window.bell(),
                        quick: window.quick(),
                        hidden: window.hidden(),
                    })
                    .collect(),
            ),
            Asked::Tabs(window) => {
                let tabs = self.window(window)?.tabs().to_vec();
                let infos: Vec<host::TabInfo> = tabs
                    .into_iter()
                    .filter_map(|tab| self.context.workspace.tab(tab))
                    .map(|tab| self.tab_info(tab))
                    .collect();
                // What it learns of each tab, of which a change that it
                // cannot read tells it nothing.
                if !self.state.reads_processes() {
                    for info in &infos {
                        self.state.told_tabs.insert(info.id, info.clone());
                    }
                }
                Response::Tabs(infos)
            }
            Asked::Layout(tab) => Response::Layout(layout(self.tab(tab)?.layout())),
            Asked::CommandLine { pane, command } => {
                Response::Text(self.pane(pane)?.command_line(command))
            }
            Asked::ShellCommands { pane, top, bottom } => {
                Response::ShellCommands(self.pane(pane)?.commands_meeting(top, bottom))
            }
            Asked::CursorCell(pane) => Response::Cell(self.pane(pane)?.cursor_cell()),
            Asked::Viewport(pane) => {
                Response::Viewport(self.pane(pane)?.viewport().ok_or_else(|| {
                    Error::new(
                        Code::Unavailable,
                        format!("the pane {} has no terminal", pane.0),
                    )
                })?)
            }
            Asked::PaneGrid(pane) => {
                let grid = self.pane(pane)?.grid();
                Response::Grid {
                    columns: grid.columns,
                    rows: grid.rows,
                }
            }
            Asked::Themes => Response::Themes((self.context.themes)()),
            Asked::Fonts => Response::Fonts((self.context.fonts)()),
            Asked::Day => Response::Day(self.context.day),
            Asked::NoticeShows => Response::Boolean(self.context.notice),
            Asked::KeepsState => Response::Boolean(self.state.stored.file.is_some()),
            Asked::SelectionCells(pane) => Response::Cells(
                self.pane(pane)?
                    .selection_cells(image::MOST_ROWS)
                    .map(cell_rows),
            ),
            Asked::ScreenCells(pane) => {
                Response::Cells(Some(cell_rows(self.pane(pane)?.view_cells())))
            }
            Asked::RenderImage(image) => {
                if self.looking {
                    return Err(Error::new(Code::Failed, lokyr_plugin_sdk::image::LOOKING));
                }
                if self.tally.images >= MOST_IMAGES {
                    return Err(Error::new(Code::Failed, IMAGES_BOUND));
                }
                let image = image.bounded();
                image.check()?;
                // Of the plugin's budget of pixels in time, which each image
                // takes, refused or drawn.
                let (budget, at) = &mut self.state.images;
                if let Some(at) = *at {
                    budget.refill(self.context.now.saturating_duration_since(at));
                }
                *at = Some(self.context.now);
                budget.check()?;
                self.tally.images += 1;
                let drawn = self.context.images.render(&self.state.name, &image);
                budget.spend(&image, drawn.as_ref().ok());
                Response::Image(drawn.map_err(|message| Error::new(Code::Unavailable, message))?)
            }
            Asked::Commands => {
                // The rest of the registry to a plugin of `user-commands`
                // alone, which tells the plugins installed and the user's
                // keys.
                let user = self
                    .state
                    .granted
                    .as_ref()
                    .is_none_or(|granted| granted.contains(&Capability::UserCommands));
                let registry = &*self.context.registry;
                let infos = registry
                    .in_order()
                    .map(|command| {
                        let name = &command.name;
                        host::CommandInfo {
                            name: name.clone(),
                            title: command.title.clone(),
                            category: command.category,
                            params: command.params.clone(),
                            plugin: user
                                .then(|| registry.owner(name).map(str::to_owned))
                                .flatten(),
                            keys: user
                                .then(|| (self.context.keys)(name, &Args::new()))
                                .flatten(),
                            when: user.then(|| command.when.clone()).flatten(),
                            enabled: user.then(|| (self.context.enabled)(name)),
                        }
                    })
                    .collect();
                Response::Commands(infos)
            }
            Asked::PaneTitle(pane) => {
                let reads = self.state.reads_processes();
                let title = self.pane(pane)?.header_title(reads);
                // What it learns of the pane, of which a change that it
                // cannot read tells it nothing.
                if !reads {
                    self.state.told_panes.insert(pane, title.clone());
                }
                Response::Text(title)
            }
            Asked::Config => Response::Data(self.state.config.clone()),
            Asked::State => Response::Data(self.stored()),
            other => {
                return Err(Error::new(Code::Fault, format!("{other:?} is not a read")));
            }
        })
    }

    /// Checks `request`, a write, and queues it: one to a pane that is not
    /// there, nor opened by the call's writes so far, fails now, and so
    /// does one past the [`MOST_WRITES`] of a call, which waits for its
    /// end.
    fn queue(&mut self, request: Asked) -> Result<Response, Error> {
        self.within_writes()?;
        match &request {
            Asked::ShowMatches { search, .. } => {
                if !self.state.searches.iter().any(|kept| kept.id == *search) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no search {search}"),
                    ));
                }
            }
            Asked::SelectCells { pane, .. }
            | Asked::Deselect(pane)
            | Asked::ShowCursor { pane, .. }
            | Asked::SelectAll(pane)
            | Asked::ClearScrollback(pane)
            | Asked::Clear(pane)
            | Asked::Paste { pane, .. }
            | Asked::SendText { pane, .. }
            | Asked::Scroll { pane, .. } => {
                self.known_pane(*pane)?;
            }
            Asked::Decorate {
                pane,
                from,
                to,
                look,
            } => {
                if !self.pane(*pane)?.holds(&[from.1, to.1]) || (to.1, to.0) < (from.1, from.0) {
                    return Err(Error::new(
                        Code::Failed,
                        "a decoration's cells are not the pane's, or not in reading order",
                    ));
                }
                let id = new_id();
                self.state.decorations.insert(id, *pane);
                self.writes.push(Write::Decorate {
                    id,
                    pane: *pane,
                    from: *from,
                    to: *to,
                    look: *look,
                });
                return Ok(Response::Decoration(id));
            }
            Asked::Undecorate { pane, id } => {
                let own = self.state.decorations.get(id) == Some(pane);
                if !own {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no decoration {id} of the pane {}", pane.0),
                    ));
                }
            }
            Asked::MarkGutter { pane, row, color } => {
                if !self.pane(*pane)?.holds(&[*row]) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("the pane {} holds no row {row}", pane.0),
                    ));
                }
                let id = new_id();
                self.state.gutter_marks.insert(id, *pane);
                self.writes.push(Write::MarkGutter {
                    id,
                    pane: *pane,
                    row: *row,
                    color: *color,
                });
                return Ok(Response::Decoration(id));
            }
            Asked::UnmarkGutter { pane, id } => {
                let own = self.state.gutter_marks.get(id) == Some(pane);
                if !own {
                    return Err(Error::new(
                        Code::NotFound,
                        format!(
                            "the plugin has no mark {id} of the pane {}'s gutter",
                            pane.0
                        ),
                    ));
                }
            }
            Asked::NewTab {
                window,
                index,
                directory,
            } => {
                self.window(*window)?;
                if let Some(directory) = directory {
                    host::check_directory(directory)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                let (tab, pane) = (TabId::new(), PaneId::new());
                self.writes.push(Write::NewTab {
                    window: WindowId::of(window.0),
                    index: *index,
                    tab,
                    pane,
                    directory: directory.clone(),
                });
                return Ok(Response::Tab {
                    tab: tab.into(),
                    pane: pane.into(),
                });
            }
            Asked::Split {
                pane,
                direction: lying,
                side: new_side,
            } => {
                self.known_pane(*pane)?;
                let new = PaneId::new();
                self.writes.push(Write::Split {
                    pane: PaneId::of(pane.0),
                    direction: direction(*lying),
                    side: side(*new_side),
                    new,
                });
                return Ok(Response::Pane(Some(new.into())));
            }
            Asked::SetTabBarPlace(place) => {
                place
                    .check()
                    .map_err(|message| Error::new(Code::Failed, message))?;
            }
            Asked::SetInactiveDim(dim) => {
                if !(0.0..=1.0).contains(dim) {
                    return Err(Error::new(
                        Code::Failed,
                        lokyr_plugin_sdk::message::DIM_RANGE,
                    ));
                }
            }
            Asked::SetClipboardImage(png) => png.check_png()?,
            Asked::SetWindowTabBarPlace { window, place } => {
                self.window(*window)?;
                if let Some(place) = place {
                    place
                        .check()
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
            }
            Asked::SetTabColor { tab, color } => {
                self.known_tab(*tab)?;
                if matches!(color, Some(host::Color::Palette(_))) {
                    return Err(Error::new(
                        Code::Failed,
                        "a tab's color is an RGB one, not one of the palette's",
                    ));
                }
            }
            Asked::CloseTab { tab, .. }
            | Asked::SetTabTitle { tab, .. }
            | Asked::ActivateTab(tab)
            | Asked::TabToNewWindow { tab, .. }
            | Asked::Equalize(tab) => {
                self.known_tab(*tab)?;
            }
            Asked::ClosePane { pane, .. }
            | Asked::FocusPane(pane)
            | Asked::PaneToNewWindow(pane) => {
                self.known_pane(*pane)?;
            }
            Asked::SwapPanes { pane, with } => {
                self.known_pane(*pane)?;
                self.known_pane(*with)?;
                let tab_of = |pane: &host::PaneId| {
                    self.context
                        .workspace
                        .tab_of(PaneId::of(pane.0))
                        .or_else(|| self.opened_tab_of(*pane))
                };
                if tab_of(pane) != tab_of(with) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("the panes {} and {} are not of one tab", pane.0, with.0),
                    ));
                }
            }
            Asked::MoveTab { tab, window, .. } => {
                self.known_tab(*tab)?;
                self.window(*window)?;
            }
            Asked::PaneToNewTab { pane, window, .. } => {
                self.known_pane(*pane)?;
                self.window(*window)?;
            }
            Asked::TabIntoPane { tab, pane, .. } => {
                self.known_tab(*tab)?;
                self.known_pane(*pane)?;
                if self.context.workspace.tab_of(PaneId::of(pane.0)) == Some(TabId::of(tab.0)) {
                    return Err(Error::new(
                        Code::Failed,
                        lokyr_plugin_sdk::message::OWN_PANE,
                    ));
                }
            }
            Asked::ResizePane { pane, by, .. } => {
                self.known_pane(*pane)?;
                if !(-1.0..=1.0).contains(by) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("a resize of {by} is not a share from -1 to 1"),
                    ));
                }
            }
            Asked::SetRatio { tab, ratio, .. } => {
                self.known_tab(*tab)?;
                if ratio.is_some_and(|ratio| !(0.0..=1.0).contains(&ratio)) {
                    return Err(Error::new(
                        Code::Failed,
                        "a split's ratio is a share from 0 to 1",
                    ));
                }
            }
            Asked::Zoom { tab, pane } => {
                let panes = match self.opened_tab(*tab) {
                    Some(opened) => vec![opened],
                    None => self.tab(*tab)?.layout().panes(),
                };
                if let Some(pane) = pane
                    && !panes.contains(&PaneId::of(pane.0))
                {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the tab {} has no pane {}", tab.0, pane.0),
                    ));
                }
            }
            Asked::RequestRender(view) => {
                self.view_window(view)?;
                self.writes.push(Write::Render(view.clone()));
                return Ok(Response::Done);
            }
            Asked::SetState(value) => {
                let failed = |message: String| Error::new(Code::Failed, message);
                let text = json(value)
                    .ok_or_else(|| {
                        failed("a plugin's state holds finite numbers alone".to_owned())
                    })?
                    .to_string();
                if text.len() > MOST_STATE {
                    return Err(failed(format!(
                        "a plugin's state is {MOST_STATE} bytes at most as JSON, not {}",
                        text.len()
                    )));
                }
                if self.state.stored.file.is_none() {
                    return Err(Error::new(
                        Code::Unavailable,
                        "the app keeps no state, as it has no state directory",
                    ));
                }
            }
            Asked::Emit { name, schema, .. } => {
                let own = name
                    .strip_prefix(self.state.namespace.as_str())
                    .is_some_and(|rest| rest.starts_with('.'));
                if !commands::is_name(name) || !own {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!(
                            "`{name}` is not an event of the plugin's, `{}.event`",
                            self.state.namespace
                        ),
                    ));
                }
                if schema.is_empty() {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!("the event `{name}` names no schema"),
                    ));
                }
            }
            _ => {}
        }
        self.writes.push(Write::Asked(request));
        Ok(Response::Done)
    }

    /// Whether the call may write or ask the app once more: its writes and
    /// its requests of the app's, [`MOST_WRITES`] at most, wait for its
    /// end.
    fn within_writes(&self) -> Result<(), Error> {
        if self.writes.len() + self.tally.asked >= MOST_WRITES {
            return Err(Error::new(
                Code::Failed,
                format!(
                    "a call writes and asks the app {MOST_WRITES} times at most, \
                     which apply as it returns"
                ),
            ));
        }
        Ok(())
    }

    /// Checks `request`, of the app's, and passes it on, for the app to do
    /// once the call returns.
    fn ask_app(&mut self, request: Asked) -> Result<Response, Error> {
        self.within_writes()?;
        self.tally.asked += 1;
        let asked = match request {
            Asked::RunCommand { name, args } => {
                let args = self.check_run(&name, args)?;
                Request::Run {
                    invocation: Invocation {
                        command: name,
                        args,
                    },
                    chain: Chain::default(),
                }
            }
            Asked::Open(target) => Request::Open(
                opening(&target).map_err(|message| Error::new(Code::Failed, message))?,
            ),
            Asked::SetFontSize(size) => {
                if let Some(size) = size {
                    Size::try_from(f64::from(size))
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                Request::FontSize(size)
            }
            Asked::OpenQuickWindow { place, directory } => {
                place
                    .check()
                    .map_err(|message| Error::new(Code::Failed, message))?;
                if let Some(directory) = &directory {
                    host::check_directory(directory)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                let window = WindowId::new();
                // Those that closed are forgotten.
                let workspace = &self.context.workspace;
                self.state
                    .quick_windows
                    .retain(|&kept| workspace.window(kept).is_some());
                self.state.quick_windows.push(window);
                self.out.requests.push(Request::OpenQuickWindow {
                    window,
                    place,
                    directory,
                });
                return Ok(Response::Window(Some(window.into())));
            }
            Asked::ShowQuickWindow { window, place } => {
                if let Some(place) = &place {
                    place
                        .check()
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                // The plugin's own alone.
                let id = WindowId::of(window.0);
                let quick = self
                    .context
                    .workspace
                    .window(id)
                    .filter(|shown| shown.quick());
                if quick.is_none() || !self.state.quick_windows.contains(&id) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no quick window {}", window.0),
                    ));
                }
                Request::ShowQuickWindow {
                    window: WindowId::of(window.0),
                    place,
                }
            }
            Asked::OpenWindow { directory } => {
                if let Some(directory) = &directory {
                    host::check_directory(directory)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                Request::OpenWindow(directory)
            }
            Asked::PreviewTheme(name) => {
                if let Some(name) = &name {
                    theme_name(name)?;
                }
                Request::PreviewTheme(name)
            }
            // A choice that the config keeps, of a theme that is there.
            Asked::SetTheme { variant, name } => {
                theme_name(&name)?;
                (self.context.themes)()
                    .check(&name, variant)
                    .map_err(|message| Error::new(Code::NotFound, message))?;
                Request::SetTheme { variant, name }
            }
            // A monospaced family that the system has, previewed or chosen.
            Asked::PreviewFont(family) => {
                if let Some(family) = &family {
                    (self.context.fonts)()
                        .check(family)
                        .map_err(|message| Error::new(Code::NotFound, message))?;
                }
                Request::PreviewFont(family)
            }
            Asked::SetFont(family) => {
                (self.context.fonts)()
                    .check(&family)
                    .map_err(|message| Error::new(Code::NotFound, message))?;
                Request::SetFont(family)
            }
            Asked::SaveFile {
                window,
                name,
                bytes,
            } => {
                host::check_file_name(&name)?;
                bytes.check()?;
                let window = match window {
                    Some(window) => {
                        let window = WindowId::of(window.0);
                        if self.context.workspace.window(window).is_none() {
                            return Err(Error::new(
                                Code::NotFound,
                                format!("there is no window {}", window.get()),
                            ));
                        }
                        window
                    }
                    None => self.context.focused.ok_or_else(|| {
                        Error::new(Code::Unavailable, lokyr_plugin_sdk::message::NO_WINDOW)
                    })?,
                };
                // A window shows one panel at a time, of any plugin's, the
                // panels that this turn's calls asked for among them.
                let queued = self.out.requests.iter().any(|request| {
                    matches!(request, Request::SaveFile { window: asked, .. } if *asked == window)
                });
                if queued || (self.context.saving)(window) {
                    return Err(Error::new(Code::Failed, lokyr_plugin_sdk::message::SAVING));
                }
                let id = new_id();
                self.out.requests.push(Request::SaveFile {
                    plugin: self.state.name.clone(),
                    id,
                    window,
                    name,
                    bytes: bytes.0,
                });
                return Ok(Response::Asking(id));
            }
            Asked::AskText {
                window,
                message,
                detail,
                text,
            } => {
                let window = window.map(|window| WindowId::of(window.0));
                if let Some(window) = window
                    && self.context.workspace.window(window).is_none()
                {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.get()),
                    ));
                }
                let id = new_id();
                self.out.requests.push(Request::AskText {
                    plugin: self.state.name.clone(),
                    id,
                    window,
                    message,
                    detail,
                    text,
                });
                return Ok(Response::Asking(id));
            }
            Asked::CloseWindow(window) => {
                let id = WindowId::of(window.0);
                if self.context.workspace.window(id).is_none() {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                }
                Request::CloseWindow(id)
            }
            Asked::Quit => Request::Quit,
            Asked::SetText {
                view,
                key,
                text,
                revision,
            } => {
                if !matches!(self.node(&view, &key)?.kind, Kind::Field(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a field", key.0),
                    ));
                }
                Request::SetText {
                    plugin: self.state.name.clone(),
                    view,
                    key,
                    text,
                    revision,
                }
            }
            Asked::Select { view, key, item } => {
                if !matches!(self.node(&view, &key)?.kind, Kind::List(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a list", key.0),
                    ));
                }
                Request::Select {
                    plugin: self.state.name.clone(),
                    view,
                    key,
                    item,
                }
            }
            Asked::FocusNode { view, key } => {
                self.node(&view, &key)?;
                Request::FocusNode {
                    plugin: self.state.name.clone(),
                    view,
                    key,
                }
            }
            Asked::Reveal { view, key } => {
                if !self.state.views.contains_key(&view) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has rendered no view {view:?}"),
                    ));
                }
                Request::Reveal {
                    plugin: self.state.name.clone(),
                    view,
                    key,
                }
            }
            Asked::OpenPanel { id, options } => {
                if id.0.is_empty() {
                    return Err(Error::new(Code::InvalidName, "a panel's id is not empty"));
                }
                if self.state.panels.contains_key(&id) {
                    return Err(Error::new(
                        Code::AlreadyRegistered,
                        format!("the plugin has a panel `{}` open", id.0),
                    ));
                }
                let window = self.anchor_window(&options.anchor)?;
                self.state.panels.insert(id.clone(), window);
                let view = ViewId::Panel(id.clone());
                if !self
                    .out
                    .renders
                    .iter()
                    .any(|(by, asked)| *by == self.state.name && *asked == view)
                {
                    self.out.renders.push((self.state.name.clone(), view));
                }
                Request::OpenPanel {
                    plugin: self.state.name.clone(),
                    id,
                    window,
                    options,
                }
            }
            Asked::ClosePanel(id) => {
                if self.state.panels.remove(&id).is_none() {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no panel `{}` open", id.0),
                    ));
                }
                // Forgotten now, so that the same call can open it again; its
                // tree goes, and its plugin hears that it closed, as the call
                // returns, unless it fails.
                self.tally.closed.push(id.clone());
                Request::ClosePanel {
                    plugin: self.state.name.clone(),
                    id,
                }
            }
            Asked::Packages(call) => {
                call.check()?;
                let id = new_id();
                self.out.requests.push(Request::Packages {
                    plugin: self.state.name.clone(),
                    id,
                    call,
                });
                return Ok(Response::Asking(id));
            }
            Asked::CancelPackages(id) => Request::CancelPackages {
                plugin: self.state.name.clone(),
                id,
            },
            other => {
                return Err(Error::new(
                    Code::Fault,
                    format!("{other:?} is not a request of the app's"),
                ));
            }
        };
        self.out.requests.push(asked);
        Ok(Response::Done)
    }

    /// Checks a run of `name` with `args` that the plugin asks for, as of
    /// its manifest's commands and its capabilities; a plugin of
    /// `user-commands` runs any as the user's.
    fn check_run(&self, name: &str, args: Args) -> Result<Args, Error> {
        let registry = &self.context.registry;
        match (&self.state.uses, &self.state.granted) {
            (Some(uses), Some(granted)) if !granted.contains(&Capability::UserCommands) => {
                registry.check_for_plugin(name, args, &self.state.commands, uses, granted)
            }
            _ => registry.check(name, args),
        }
    }

    /// Checks `binding` of a plugin of a manifest that is not a first-party
    /// one, as its menu items are: its keys run the call as the plugin
    /// would, a command that it may run, of arguments that make a whole
    /// call, as a key's run is the user's, which no check of the plugin's
    /// meets after.
    fn check_binding(&self, binding: &host::Binding) -> Result<(), Error> {
        if self.state.uses.is_some() {
            self.check_run(&binding.command, binding.args.clone())?;
        }
        Ok(())
    }

    /// Whether the plugin may declare once more, within [`MOST_DECLARED`].
    fn within_declared(&self) -> Result<(), Error> {
        let state = &self.state;
        let declared = state.commands.len()
            + state.bindings.len()
            + state.global_bindings.len()
            + state.menu_items.len();
        if declared >= MOST_DECLARED {
            return Err(Error::new(
                Code::Failed,
                format!("a plugin declares {MOST_DECLARED} commands, keys and menu items at most"),
            ));
        }
        Ok(())
    }

    /// Takes `request`, a declaration of the plugin's commands, its keys,
    /// its subscriptions or its timers, at once: a command, a key or an item
    /// that the plugin declared already changes nothing, so that the keymap
    /// and the menus are not made anew for it. A timer of the call's waits
    /// from the host's time of the call, and goes, as a timer that it
    /// cancels comes back, if the call fails.
    fn declare(&mut self, request: Asked) -> Result<Response, Error> {
        if !matches!(
            request,
            Asked::Subscribe(_) | Asked::StartTimer { .. } | Asked::CancelTimer(_)
        ) {
            self.within_declared()?;
        }
        match request {
            Asked::RegisterCommand(command) => {
                if self.state.uses.is_some() {
                    self.of_its_namespace(&command.name)?;
                }
                // A command of its manifest, which showed as it waited for
                // its trigger, takes its place.
                if self.context.registry.owner(&command.name) == Some(self.state.name.as_str())
                    && !self
                        .state
                        .commands
                        .iter()
                        .any(|own| own.name == command.name)
                {
                    self.context.registry.unregister(&command.name);
                }
                self.context
                    .registry
                    .register_for(command.clone(), &self.state.name)?;
                self.state.commands.push(command);
                *self.declarations_changed = true;
            }
            Asked::Bind(binding) if binding.keys.starts_with(keymap::GLOBAL) => {
                let invalid = |message: String| Error::new(Code::InvalidBinding, message);
                let combo =
                    keymap::parse_global(&binding.keys[keymap::GLOBAL.len()..]).map_err(invalid)?;
                if !commands::is_name(&binding.command) {
                    return Err(invalid(format!(
                        "`{}` is not a command's name, `namespace.action`",
                        binding.command
                    )));
                }
                if binding.when.is_some() {
                    return Err(invalid(format!(
                        "a global key holds everywhere, without `when`, in `{}`",
                        binding.keys
                    )));
                }
                self.check_binding(&binding)?;
                let global = (
                    combo,
                    Invocation {
                        command: binding.command,
                        args: binding.args,
                    },
                );
                if !self.state.global_bindings.contains(&global) {
                    self.state.global_bindings.push(global);
                    *self.declarations_changed = true;
                }
            }
            Asked::Bind(binding) => {
                let invalid = |message: String| Error::new(Code::InvalidBinding, message);
                let keys = keymap::parse_keys(&binding.keys).map_err(invalid)?;
                if !commands::is_name(&binding.command) {
                    return Err(invalid(format!(
                        "`{}` is not a command's name, `namespace.action`",
                        binding.command
                    )));
                }
                let when = binding
                    .when
                    .as_deref()
                    .map(When::parse)
                    .transpose()
                    .map_err(invalid)?;
                self.check_binding(&binding)?;
                let binding = Binding {
                    keys,
                    invocation: Invocation {
                        command: binding.command,
                        args: binding.args,
                    },
                    when,
                };
                if !self.state.bindings.contains(&binding) {
                    self.state.bindings.push(binding);
                    *self.declarations_changed = true;
                }
            }
            Asked::AddMenuItem(item) => {
                item.check()?;
                if !commands::is_name(&item.command) {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!(
                            "`{}` is not a command's name, `namespace.action`",
                            item.command
                        ),
                    ));
                }
                // The item runs as the plugin would: its arguments make a
                // whole call, which it may make.
                self.check_run(&item.command, item.args.clone())?;
                // The same item again, of another tick, changes its tick.
                match self
                    .state
                    .menu_items
                    .iter_mut()
                    .find(|kept| kept.same_as(&item))
                {
                    Some(kept) if kept.ticked != item.ticked => {
                        kept.ticked = item.ticked;
                        *self.declarations_changed = true;
                    }
                    Some(_) => {}
                    None => {
                        self.state.menu_items.push(item);
                        *self.declarations_changed = true;
                    }
                }
            }
            Asked::Subscribe(names) => {
                if let Some(name) = names
                    .iter()
                    .find(|name| !lokyr_plugin_sdk::message::is_event_name(name))
                {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!(
                            "`{name}` is no event: one of the workspace's, as `tab-created`, or a plugin's, `namespace.event`"
                        ),
                    ));
                }
                for name in names {
                    if !self.state.subscribed.contains(&name) {
                        self.state.subscribed.push(name);
                    }
                }
            }
            Asked::StartTimer { delay, repeats } => {
                if delay > host::MOST_DELAY {
                    return Err(Error::new(
                        Code::Failed,
                        lokyr_plugin_sdk::message::DELAY_RANGE,
                    ));
                }
                if self.state.timers.len() >= host::MOST_TIMERS {
                    return Err(Error::new(
                        Code::Unavailable,
                        lokyr_plugin_sdk::message::TIMERS_BOUND,
                    ));
                }
                let every = Duration::from_millis(delay.max(host::LEAST_DELAY));
                let id = new_id();
                self.state.timers.insert(
                    id,
                    Timer {
                        at: Some(self.context.now + every),
                        every: repeats.then_some(every),
                        due: false,
                    },
                );
                return Ok(Response::Timer(id));
            }
            Asked::CancelTimer(id) => {
                self.state.timers.remove(&id);
            }
            other => {
                return Err(Error::new(
                    Code::Fault,
                    format!("{other:?} is not a declaration"),
                ));
            }
        }
        Ok(Response::Done)
    }
}

#[cfg(test)]
thread_local! {
    /// Whether the next request that the host handles on this thread
    /// panics, as a bug of the host's would, which no request is known to.
    pub(super) static BREAKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl host::Host for Host<'_, '_> {
    fn handle(&mut self, call: Call) -> Response {
        #[cfg(test)]
        if BREAKS.with(|breaks| breaks.replace(false)) {
            panic!("the host broke, on purpose");
        }
        let answered = if call.caller == self.state.name {
            let request = call.request;
            within_depth(request)
                .and_then(|request| self.allows(&request).map(|()| request))
                .and_then(|request| match request.effect() {
                    Effect::Read => self.read(request),
                    Effect::Write => self.queue(request),
                    Effect::App => self.ask_app(request),
                    Effect::Declaration if self.looking => Err(Error::new(
                        Code::Failed,
                        "a call that only looks, as `applies`, declares nothing",
                    )),
                    Effect::Declaration => self.declare(request),
                })
        } else {
            Err(Error::new(
                Code::Fault,
                format!(
                    "a call of the plugin `{}` came as `{}`'s",
                    self.state.name, call.caller
                ),
            ))
        };
        answered.unwrap_or_else(Response::Failed)
    }
}

/// What a read of a window's tabs tells of `tab` of `workspace`, to a plugin
/// that `reads` the processes or not: their folders and programs, what
/// runs, and its agent, of the program that runs, to one that does alone.
fn tab_info(workspace: &Workspace, tab: &crate::workspace::Tab, reads: bool) -> host::TabInfo {
    let active = workspace
        .window(tab.window())
        .is_some_and(|window| window.active_tab() == tab.id());
    let title = if reads { tab.label() } else { tab.title() };
    let marks = tab.marks();
    host::TabInfo {
        id: tab.id().into(),
        window: tab.window().into(),
        title: title.to_owned(),
        color: host_color(tab.color()),
        active,
        panes: tab.layout().panes().into_iter().map(Into::into).collect(),
        focused: tab.active_pane().into(),
        zoomed: tab.zoomed().map(Into::into),
        activity: marks.activity,
        bell: marks.bell,
        progress: marks.progress.and_then(tab_progress),
        printing: marks.printing && reads,
        running: marks.running && reads,
        alternate: marks.alternate && reads,
        secure: marks.secure,
        agent: marks.agent.filter(|_| reads),
    }
}

/// The color of a tab of `color`, as a plugin learns it.
fn host_color(color: Option<lokyr_vt_core::page::Rgb>) -> Option<host::Color> {
    color.map(|color| host::Color::Rgb(color.r, color.g, color.b))
}

/// The tree that `plugin` renders of `view`, as the host takes it: refused
/// past its depth, or as a titlebar's button that is none, or of the
/// failure that the plugin tells after it, which leaves its last tree.
fn rendered(
    plugin: &mut dyn Plugin,
    cx: &mut Cx<'_>,
    view: &ViewId,
) -> Result<Option<Node>, Error> {
    let tree = plugin.render(cx, view);
    if let Some(failure) = plugin.take_failure() {
        if let Some(tree) = tree {
            view::dismantle(tree);
        }
        return Err(failure);
    }
    let tree = tree_within_depth(tree)?;
    if let (ViewId::TitleButtons(_), Some(button)) = (view, &tree) {
        button
            .check_title_button()
            .map_err(|reason| Error::new(Code::Failed, reason))?;
    }
    Ok(tree)
}

/// Whether the host refuses `capability` to the plugin of `manifest`,
/// whatever it declares: one that it gives no plugin yet, or one of Lokyr's
/// own plugins alone to another.
fn withheld_from(manifest: &Manifest, capability: &Capability) -> bool {
    capability.is_withheld()
        || (capability.is_first_party_only() && manifest.kind != PackageKind::Builtin)
}

/// `tree`, a plugin's render, if it nests [`Node::DEEPEST`] levels at most,
/// as the host takes it; `failed` otherwise, the tree dropped without
/// recursion, where the render returned it, on the plugin's own thread.
fn tree_within_depth(tree: Option<Node>) -> Result<Option<Node>, Error> {
    match tree {
        Some(tree) if !tree.nests_within_deepest() => {
            view::dismantle(tree);
            Err(Error::new(
                Code::Failed,
                format!("a plugin's view nests {} levels at most", Node::DEEPEST),
            ))
        }
        tree => Ok(tree),
    }
}

#[cfg(test)]
mod tests;

/// The progress of a tab of `progress`, OSC 9;4's, as the plugins read it:
/// its state and its value, from 0 to 1, if it tells one; none of none.
fn tab_progress(progress: lokyr_vt_core::terminal::Progress) -> Option<host::TabProgress> {
    use lokyr_plugin_sdk::view::ProgressState;
    use lokyr_vt_core::terminal::Progress;

    let share = |percent: u8| f32::from(percent.min(100)) / 100.0;
    let (state, value) = match progress {
        Progress::Remove => return None,
        Progress::Set(percent) => (ProgressState::Normal, Some(share(percent))),
        Progress::Error(percent) => (ProgressState::Error, percent.map(share)),
        Progress::Paused(percent) => (ProgressState::Paused, percent.map(share)),
        Progress::Indeterminate => (ProgressState::Indeterminate, None),
    };
    Some(host::TabProgress { state, value })
}

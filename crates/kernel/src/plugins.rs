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
use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use lokyr_pkg_model::manifest::{Manifest, ManifestError, Thread};
use lokyr_pkg_model::schema::Schema;
pub use lokyr_plugin_sdk::event::WorkspaceEvent;
pub use lokyr_plugin_sdk::host;
pub use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::message::{Call, Effect, Request as Asked, Response, within_depth};
pub use lokyr_plugin_sdk::message::{Data, KEYS_CHANGED, Mods, POINTER};
pub use lokyr_plugin_sdk::plugin::{Cx, Plugin, Registration};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Slot, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{Key, Kind, Node};
pub use lokyr_plugin_sdk::{ui, view};
use lokyr_session::Reader;
use lokyr_text_core::Size;
use lokyr_vt_core::page::{self, Rgb, Style};
use lokyr_vt_core::terminal::{Look, Scroll};
use serde_json::{Map, Value};

use crate::commands::{self, Args, Capability, Code, Command, Error, Registry, Uses};
use crate::config::{ConfirmPaste, PluginsConfig};
use crate::keymap::{self, Binding, Invocation, When};
use crate::l10n::{Arg, Texts};
use crate::paste::{self, Paste};
use crate::workspace::{
    Direction, Edge, Layout, Pane, PaneId, Ratio, Side, TabId, WindowId, Workspace,
};

mod worker;

use worker::{Called, Done, FromWorker, Job, Worker};

/// What a plugin asks of the app, which the app does once the plugin's
/// call returns.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Run a command, whose call the registry checked.
    Run {
        /// The command and its arguments.
        invocation: Invocation,
        /// How deep the command whose call asked it ran, as commands ran
        /// commands: 0 for a call of no command's, an event's or a
        /// render's, and for a command that the user ran.
        depth: usize,
    },
    /// Open a window.
    OpenWindow,
    /// Close a window.
    CloseWindow(WindowId),
    /// Quit.
    Quit,
    /// Give the text a size, in points; with `None`, the config's.
    FontSize(Option<f32>),
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
}

/// What starts the programs of the panes that plugins open, the app's own
/// way, as it starts a window's pane.
pub trait Panes {
    /// A pane of the id `id`, which nothing of the process took, beside
    /// `near`, the pane that it splits or the focused one of the window
    /// that it opens in, if there is one: its program, its directory and
    /// its size as a new pane's beside it.
    fn spawn(&mut self, id: PaneId, near: Option<&Pane>) -> Pane;
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
    /// The window that has the focus.
    pub focused: Option<WindowId>,
    /// The text's size, in points.
    pub font_size: f32,
    /// When a paste with line breaks asks the user first.
    pub confirm_paste: ConfirmPaste,
    /// The keys that run a command with its arguments, as the platform
    /// writes them, as `⌘1`; none without a binding.
    pub keys: &'a dyn Fn(&str, &Args) -> Option<String>,
    /// The language that the app shows, of its own catalogs, which the
    /// plugins' messages take.
    pub language: &'a str,
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

/// A plugin, and what the host keeps of it: what makes it anew, as the
/// config turns it on again, if it was registered.
struct Entry {
    runner: Runner,
    state: State,
    make: Option<fn() -> Box<dyn Plugin + Send>>,
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
    /// Its catalogs, of its commands' titles.
    catalogs: &'static [(&'static str, &'static str)],
    /// The last tree of each of its views that it rendered, `None` for a
    /// view that shows nothing, which stays as it is disabled, with the
    /// render's number, which no other render took.
    views: BTreeMap<ViewId, (u64, Option<Node>)>,
    /// Its open panels, each in its window.
    panels: BTreeMap<PanelId, WindowId>,
    /// The decorations that it laid, by their ids, each of its pane.
    decorations: BTreeMap<u64, host::PaneId>,
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
    /// The veil of the panes that have not the keyboard that it asked for,
    /// and whether it asked that the keyboard follow the pointer.
    dim: f32,
    follows: bool,
    /// Where its tab bar goes in windows of a place of their own, as a
    /// window of a single tab shows its title, which goes as the window
    /// closes.
    window_places: BTreeMap<host::WindowId, host::BarPlace>,
    /// Of a plugin that does not read the processes, the title and the
    /// color of each tab as it last learned them, which a change of the
    /// tab's folder or program, that it cannot read, leaves.
    told_tabs: BTreeMap<host::TabId, Seen>,
}

/// A tab's title and color, as a plugin that does not read the processes
/// sees them.
type Seen = (String, Option<host::Color>);

impl State {
    /// Whether it reads the processes, whose folders and programs a tab's
    /// title tells it.
    fn reads_processes(&self) -> bool {
        self.granted
            .as_ref()
            .is_none_or(|granted| granted.contains(&Capability::ProcessRead))
    }

    /// Learns the title and the color of every tab of `workspace`, as it
    /// starts, a plugin that does not read the processes.
    fn learn_tabs(&mut self, workspace: &Workspace) {
        self.told_tabs.clear();
        if self.reads_processes() {
            return;
        }
        for window in workspace.windows() {
            for &tab in window.tabs() {
                if let Some(shown) = workspace.tab(tab) {
                    let seen = (shown.title().to_owned(), host_color(shown.color()));
                    self.told_tabs.insert(tab.into(), seen);
                }
            }
        }
    }

    /// Whether it hears `event`, of the workspace, of the tabs that
    /// `workspace` has: a tab's change that it cannot read it does not, as
    /// a plugin that does not read the processes, which learns each tab's
    /// title and color.
    fn hears(&mut self, event: &Event, workspace: &Workspace) -> bool {
        if self.reads_processes() {
            return true;
        }
        let seen = |tab: host::TabId| {
            workspace
                .tab(crate::workspace::TabId::of(tab.0))
                .map(|tab| (tab.title().to_owned(), host_color(tab.color())))
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
}

/// The rows that a read takes under the lock at a time: about a frame's
/// copy's time.
const READ_PART: usize = 256;

/// The last id of a read or a decoration, which none takes twice.
static LAST_ID: AtomicU64 = AtomicU64::new(0);

/// An id that no read nor decoration of the process took.
fn new_id() -> u64 {
    LAST_ID.fetch_add(1, Ordering::Relaxed) + 1
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
            entry.make = Some(registration.make);
        }
        Ok(())
    }

    /// Adds `plugin`, which `manifest` describes, by the manifest's name,
    /// to activate, with what the manifest declares: its capabilities, and
    /// the commands of its `[uses]` with those that it registers, of its
    /// own namespace alone; a request that needs another capability, or a
    /// run of another command, fails. One of another plugin's namespace
    /// adds nothing.
    pub fn add_declared(
        &mut self,
        manifest: Manifest,
        plugin: Box<dyn Plugin>,
    ) -> Result<(), Error> {
        let name = manifest.name.clone();
        let granted = (
            manifest.capabilities.clone(),
            Uses::new(manifest.uses.clone()),
        );
        self.insert(&name, Some(manifest), Runner::Main(plugin), Some(granted))
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
            Runner::Worker(_) | Runner::Stopped => &[],
        }))
        .unwrap_or(&[]);
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
                catalogs,
                views: BTreeMap::new(),
                panels: BTreeMap::new(),
                decorations: BTreeMap::new(),
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
            },
        });
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
                    let Some(make) = entry.make else {
                        errors.push(Error::new(
                            Code::Unavailable,
                            format!(
                                "the plugin `{}` starts again at the app's next start",
                                entry.state.name
                            ),
                        ));
                        continue;
                    };
                    let worker = entry
                        .state
                        .manifest
                        .as_ref()
                        .is_some_and(|manifest| manifest.thread == Thread::Worker);
                    entry.runner = if worker {
                        Runner::Unstarted(make())
                    } else {
                        Runner::Main(make())
                    };
                    // A fault of the new one is told again.
                    entry.state.fault = None;
                    self.told_disabled.remove(&entry.state.name);
                }
                entry.state.status = Status::Inactive;
                self.out.written = true;
            }
            if entry.state.status != Status::Inactive {
                continue;
            }
            entry.state.status = Status::Active;
            entry.state.ran = true;
            entry.state.learn_tabs(context.workspace);
            let Entry { runner, state, .. } = entry;
            let plugin = match runner {
                Runner::Main(plugin) => plugin,
                Runner::Unstarted(_) => {
                    let Runner::Unstarted(plugin) = std::mem::replace(runner, Runner::Stopped)
                    else {
                        continue;
                    };
                    let wake = self.wake.clone().unwrap_or_else(|| Arc::new(|| {}));
                    match Worker::start(&state.name, plugin, wake) {
                        Ok(mut worker) => {
                            worker.ask(Job::Activate, context.focused);
                            *runner = Runner::Worker(Box::new(worker));
                        }
                        Err(error) => errors.push(failed_start(
                            state,
                            context.registry,
                            &mut self.declarations_changed,
                            &Error::new(
                                Code::Unavailable,
                                format!("its thread did not start: {error}"),
                            ),
                        )),
                    }
                    continue;
                }
                Runner::Worker(_) | Runner::Stopped => continue,
            };
            let activated = call(
                (plugin.as_mut(), state),
                context,
                &mut self.out,
                &mut self.declarations_changed,
                "activate",
                |plugin, cx| plugin.activate(cx),
            );
            if let Err(error) = activated {
                if state.status == Status::Active {
                    errors.push(failed_start(
                        state,
                        context.registry,
                        &mut self.declarations_changed,
                        &error,
                    ));
                } else {
                    errors.push(error);
                }
            }
        }
        errors
    }

    /// Whether an active plugin registered `command`.
    pub fn owns(&self, command: &str) -> bool {
        self.owner(command).is_some()
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
        self.run_deep(command, args, 0, context)
    }

    /// Runs `command` as [`Plugins::run`] does, which commands ran `depth`
    /// deep: the commands that its call runs, whenever it ends, are
    /// [`Request::Run`]s of that depth.
    pub fn run_deep(
        &mut self,
        command: &str,
        args: Args,
        depth: usize,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        let Some(at) = self.owner(command) else {
            return Err(Error::new(
                Code::NotFound,
                format!("no plugin runs `{command}`"),
            ));
        };
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
                deepen(&mut self.out.requests[before..], depth);
                ran
            }
            // Its failure comes as its call ends.
            Runner::Worker(worker) => {
                worker.ask(
                    Job::Command(command.to_owned(), args, depth),
                    context.focused,
                );
                Ok(())
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
            &mut self.declarations_changed,
            "applies",
            |plugin, cx| Ok(plugin.applies(cx, command, args)),
        )
    }

    /// The bindings of the active plugins, in their order, which the
    /// keymap takes after the app's and before the config's.
    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.active().flat_map(|state| &state.bindings)
    }

    /// The commands of the active plugins, which a registry made anew, as
    /// for another language, takes again.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.active().flat_map(|state| &state.commands)
    }

    /// The commands of the active plugins, as [`Plugins::commands`], titled
    /// in `language` from their plugins' catalogs: by their messages, as
    /// [`title_message`] names them, the language's or English's; or by the
    /// titles that they registered.
    pub fn commands_in(&self, language: &str) -> Vec<Command> {
        let mut commands = Vec::new();
        for state in self.active() {
            let texts = Texts::new(language, state.catalogs);
            commands.extend(state.commands.iter().map(|command| {
                Command {
                    title: texts
                        .get(&title_message(&command.name))
                        .unwrap_or_else(|| command.title.clone()),
                    ..command.clone()
                }
            }));
        }
        commands
    }

    /// The veil of the panes that have not their tab's keyboard, of their
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
            .and_then(|entry| Texts::new(language, entry.state.catalogs).get(text))
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

    /// Takes what the plugins' calls asked of the app since the last take.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.out.requests)
    }

    /// Gives `plugin` the user's answer to its question `id`, the text
    /// given or none, for the next delivery, to it alone.
    pub fn answer(&mut self, plugin: &str, id: u64, text: Option<String>) {
        self.out
            .events
            .push((Some(plugin.to_owned()), Event::Answered { id, text }));
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
                    state.views.retain(|view, _| match view {
                        ViewId::TabBar(shown)
                        | ViewId::WindowStrip(shown)
                        | ViewId::StatusBar(shown) => shown != window,
                        ViewId::PaneHeader(_) | ViewId::Panel(_) => true,
                    });
                }
            }
            Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => {
                for entry in &mut self.entries {
                    entry.state.views.remove(&ViewId::PaneHeader(*pane));
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
                !matches!(state.status, Status::Inactive | Status::Off) && state.fills(slot)
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
                    |plugin, cx| tree_within_depth(plugin.render(cx, view)),
                )?;
                state.views.insert(view.clone(), (new_id(), tree));
            }
            Runner::Worker(worker) => worker.ask(Job::Render(view.clone()), context.focused),
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
                worker.ask(Job::Ui(event.clone()), context.focused);
                Ok(())
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

    /// Takes the reads of rows that ended, as events to their plugins.
    fn take_reads(&mut self) {
        let Outbox { reads, events, .. } = &mut self.out;
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
    /// those that it subscribed to; those that the deliveries' calls emit
    /// wait for the next. Returns the calls' errors, of which a panic's
    /// disables its plugin. A plugin of a thread of its own takes its batch
    /// there, which joins the one that waits for it, if one does.
    pub fn deliver(&mut self, context: &mut Context<'_>) -> Vec<Error> {
        self.take_reads();
        let events = std::mem::take(&mut self.out.events);
        let mut errors = Vec::new();
        if events.is_empty() {
            return errors;
        }
        for entry in &mut self.entries {
            if entry.state.status != Status::Active {
                continue;
            }
            let (name, subscribed) = (&entry.state.name, &entry.state.subscribed);
            // A plugin of a thread of its own that activates subscribes as
            // its requests come, after the activation began: it holds the
            // events that may be its own until its subscriptions are known.
            if let Runner::Worker(worker) = &mut entry.runner
                && worker.activating()
            {
                worker.hold(
                    events
                        .iter()
                        .filter(|(to, _)| to.as_ref().is_none_or(|to| to == name))
                        .cloned(),
                );
                continue;
            }
            let batch: Vec<Event> = events
                .iter()
                .filter(|(to, event)| match to {
                    Some(to) => to == name,
                    None => subscribed.iter().any(|name| name == event.name()),
                })
                .map(|(_, event)| event.clone())
                .collect();
            let batch: Vec<Event> = batch
                .into_iter()
                .filter(|event| entry.state.hears(event, context.workspace))
                .collect();
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
                    worker.ask(Job::Events(batch), context.focused);
                    Ok(())
                }
                Runner::Unstarted(_) | Runner::Stopped => Ok(()),
            };
            if let Err(error) = delivered {
                errors.push(error);
            }
        }
        errors
    }

    /// Whether an active plugin subscribed to the events of `name`, which
    /// the app then tells, as the pointer's, whose moves cost wakeups.
    pub fn subscribes(&self, name: &str) -> bool {
        self.active()
            .any(|state| state.subscribed.iter().any(|subscribed| subscribed == name))
    }

    /// Whether events wait for the next delivery: those of the reads of
    /// rows that ended once [`Plugins::serve`] took them.
    pub fn has_events(&self) -> bool {
        !self.out.events.is_empty()
    }

    /// Whether reads of rows, or calls of plugins on threads of their own,
    /// are under way, whose ends [`Plugins::serve`] takes, as they wake the
    /// main thread.
    pub fn waiting(&self) -> bool {
        !self.out.reads.is_empty()
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
                        let mut host = Host {
                            state,
                            context: &mut *context,
                            out: &mut busy.out,
                            writes: &mut busy.writes,
                            declarations_changed,
                        };
                        let response = host::Host::handle(&mut host, asked);
                        context.focused = now;
                        response
                    }
                    None => {
                        Response::Failed(Error::new(Code::Fault, "a request came outside a call"))
                    }
                };
                worker.answer(response);
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
            depth,
            ..
        } = busy;
        let result = match done {
            Done::Panicked(message) => {
                disable(state, context.registry, declarations_changed);
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
        apply(writes, context, &mut sent, state);
        deepen(&mut sent.requests, depth);
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
        match (&manifest.contributes.config_schema, text) {
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
    let schema = Schema::parse(text).map_err(|error| {
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
    let before = out.requests.len();
    let mut writes = Vec::new();
    let outcome = within(
        (plugin, state),
        context,
        (&mut *out, &mut writes),
        (declarations_changed, callback),
        call,
    );
    match outcome {
        Ok(result) => {
            apply(writes, context, out, state);
            result
        }
        Err(fault) => {
            out.requests.truncate(before);
            fault
        }
    }
}

/// Calls `call` on a plugin of the main thread, as [`call`] does, but
/// drops its writes and its requests, as the call only looks: the panels
/// that it opens or closes, and its decorations, stay as they were.
fn look<T>(
    (plugin, state): (&mut dyn Plugin, &mut State),
    context: &mut Context<'_>,
    declarations_changed: &mut bool,
    callback: &'static str,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let (mut out, mut writes) = (Outbox::default(), Vec::new());
    let (panels, decorations) = (state.panels.clone(), state.decorations.clone());
    let outcome = within(
        (plugin, state),
        context,
        (&mut out, &mut writes),
        (declarations_changed, callback),
        call,
    );
    state.panels = panels;
    state.decorations = decorations;
    outcome.unwrap_or_else(|fault| fault)
}

/// Calls `call` on a plugin of the main thread within `catch_unwind`, its
/// writes and its requests into `queues`: the call's result, or, as it
/// panicked, which disables the plugin, the fault's error. A call of
/// [`SLOW`] or longer is noted, by its `callback`, for the log.
fn within<T>(
    (plugin, state): (&mut dyn Plugin, &mut State),
    context: &mut Context<'_>,
    (out, writes): (&mut Outbox, &mut Vec<Write>),
    (declarations_changed, callback): (&mut bool, &'static str),
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

/// Stops the plugin of `entry`, which its section of the config turns off:
/// its commands and its bindings go, as a disabled plugin's, its panels
/// close, as the app hears, its decorations leave their panes, its views
/// show no more, and its thread, if it has one, ends.
fn stop(
    entry: &mut Entry,
    context: &mut Context<'_>,
    out: &mut Outbox,
    declarations_changed: &mut bool,
) {
    let state = &mut entry.state;
    disable(state, context.registry, declarations_changed);
    *declarations_changed = true;
    for (id, pane) in std::mem::take(&mut state.decorations) {
        if let Some(pane) = context.workspace.pane(PaneId::of(pane.0)) {
            pane.undecorate(id);
        }
    }
    for id in std::mem::take(&mut state.panels).into_keys() {
        out.requests.push(Request::ClosePanel {
            plugin: state.name.clone(),
            id,
        });
    }
    state.views.clear();
    state.subscribed.clear();
    if let Runner::Worker(_) = entry.runner {
        entry.runner = Runner::Stopped;
    }
}

/// Whether `view`, of the plugin of `state`, shows: a slot's of a window
/// or a pane that is there, or one of its open panels.
fn shows(state: &State, view: &ViewId, workspace: &Workspace) -> bool {
    match view {
        ViewId::TabBar(window) | ViewId::WindowStrip(window) | ViewId::StatusBar(window) => {
            workspace.window(WindowId::of(window.0)).is_some()
        }
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
            // A tab or a split of a window or a pane that an earlier write
            // of the call closed starts no program.
            Write::NewTab {
                window,
                index,
                tab,
                pane,
            } => {
                if context.workspace.window(window).is_some() {
                    let near = context.workspace.active_pane(window);
                    let near = near.and_then(|near| context.workspace.pane(near));
                    let new = context.panes.spawn(pane, near);
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
                    let new = context.panes.spawn(new, Some(near));
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
            Asked::SelectAll(id) => {
                if let Some(pane) = pane(id) {
                    pane.select_all();
                }
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

/// Reads `count` rows of `reader`'s terminal from the row numbered
/// `first`, of the rows' `epoch`, the request's, on a thread of its own,
/// [`READ_PART`] rows under the lock at a time, and wakes the main thread
/// once it is done: the rows, and whether the read went to its end, as the
/// rows' epoch stayed from the request to the last part, come through the
/// receiver.
fn read_rows(
    reader: Reader,
    epoch: u64,
    first: i64,
    count: usize,
) -> mpsc::Receiver<(Vec<host::Line>, bool)> {
    let (sender, receiver) = mpsc::channel();
    let read = move || {
        let (mut lines, mut next, mut complete) = (Vec::new(), first, true);
        while lines.len() < count {
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

/// Disables the plugin of `state`: its commands leave `registry`, and its
/// bindings go.
fn disable(state: &mut State, registry: &mut Registry, declarations_changed: &mut bool) {
    state.status = Status::Disabled;
    if !state.commands.is_empty() || !state.bindings.is_empty() {
        *declarations_changed = true;
    }
    for command in state.commands.drain(..) {
        registry.unregister(&command.name);
    }
    state.bindings.clear();
}

/// Gives the commands that `requests` run the depth `depth`, of the
/// command whose call asked them.
fn deepen(requests: &mut [Request], depth: usize) {
    for request in requests {
        if let Request::Run { depth: asked, .. } = request {
            *asked = depth;
        }
    }
}

/// The schemes of the URLs that a plugin can open: none that starts another
/// application on the user's behalf.
const SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

/// What opening `target` opens: a URL of one of [`SCHEMES`], or a path,
/// absolute or from `~`, which becomes the home directory.
fn opening(target: &str) -> Result<String, String> {
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
            Some(file) => match std::fs::read(file) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Data::Nil),
                Err(error) => Err(format!(
                    "it cannot be read from {}: {error}",
                    file.display()
                )),
                Ok(bytes) => serde_json::from_slice::<Value>(&bytes)
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
            ViewId::TabBar(window) | ViewId::WindowStrip(window) | ViewId::StatusBar(window) => {
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
            Anchor::Cell { pane, .. } | Anchor::Cursor(pane) | Anchor::PaneCorner { pane, .. } => {
                pane_window(*pane)
            }
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
        let active = self
            .context
            .workspace
            .window(tab.window())
            .is_some_and(|window| window.active_tab() == tab.id());
        // The folder and the program of its pane, to a plugin that reads
        // the processes' alone.
        let title = if self.state.reads_processes() {
            tab.label()
        } else {
            tab.title()
        };
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
            // What runs, to a plugin that reads the processes' alone.
            running: marks.running && self.state.reads_processes(),
            alternate: marks.alternate && self.state.reads_processes(),
            secure: marks.secure,
            agent: marks.agent,
        }
    }

    /// Whether the plugin may make `request`: it has the capability that
    /// the request needs, if it needs one.
    fn allows(&self, request: &Asked) -> Result<(), Error> {
        let Some(needed) = request.capability() else {
            return Ok(());
        };
        match &self.state.granted {
            Some(granted) if !granted.contains(&needed) => Err(Error::new(
                Code::MissingCapability,
                format!(
                    "the plugin `{}` lacks the capability `{needed}`",
                    self.state.name
                ),
            )),
            _ => Ok(()),
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
        let taken = self.context.registry.in_order().any(|command| {
            command.name.starts_with(&prefix)
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
                let id = new_id();
                self.out.reads.push(Read {
                    id,
                    plugin: self.state.name.clone(),
                    pane,
                    rows: read_rows(reader, epoch, first, count.min(host::MOST_LINES)),
                });
                Response::Reading(id)
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
                    Texts::new(self.context.language, self.state.catalogs).get_with(&id, &args),
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
                        let seen = (info.title.clone(), info.color);
                        self.state.told_tabs.insert(info.id, seen);
                    }
                }
                Response::Tabs(infos)
            }
            Asked::Layout(tab) => Response::Layout(layout(self.tab(tab)?.layout())),
            Asked::PaneGrid(pane) => {
                let grid = self.pane(pane)?.grid();
                Response::Grid {
                    columns: grid.columns,
                    rows: grid.rows,
                }
            }
            Asked::PaneTitle(pane) => {
                let reads = self.state.reads_processes();
                Response::Text(self.pane(pane)?.header_title(reads))
            }
            Asked::Config => Response::Data(self.state.config.clone()),
            Asked::State => Response::Data(self.stored()),
            other => {
                return Err(Error::new(Code::Fault, format!("{other:?} is not a read")));
            }
        })
    }

    /// Checks `request`, a write, and queues it: one to a pane that is not
    /// there, nor opened by the call's writes so far, fails now.
    fn queue(&mut self, request: Asked) -> Result<Response, Error> {
        match &request {
            Asked::SelectAll(pane)
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
            Asked::NewTab { window, index } => {
                self.window(*window)?;
                let (tab, pane) = (TabId::new(), PaneId::new());
                self.writes.push(Write::NewTab {
                    window: WindowId::of(window.0),
                    index: *index,
                    tab,
                    pane,
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

    /// Checks `request`, of the app's, and passes it on, for the app to do
    /// once the call returns.
    fn ask_app(&mut self, request: Asked) -> Result<Response, Error> {
        let asked = match request {
            Asked::RunCommand { name, args } => {
                let registry = &self.context.registry;
                let args = match (&self.state.uses, &self.state.granted) {
                    (Some(uses), Some(granted)) => registry.check_for_plugin(
                        &name,
                        args,
                        &self.state.commands,
                        uses,
                        granted,
                    )?,
                    _ => registry.check(&name, args)?,
                };
                Request::Run {
                    invocation: Invocation {
                        command: name,
                        args,
                    },
                    depth: 0,
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
            Asked::OpenWindow => Request::OpenWindow,
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
                // Forgotten now, so that the same call can open it again: no
                // tree of it stays, a render of it asked for keeps none, and
                // its plugin hears that it closed.
                self.state.views.remove(&ViewId::Panel(id.clone()));
                self.out.events.push((
                    Some(self.state.name.clone()),
                    Event::PanelClosed(id.clone()),
                ));
                Request::ClosePanel {
                    plugin: self.state.name.clone(),
                    id,
                }
            }
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

    /// Takes `request`, a declaration of the plugin's commands or keys, at
    /// once.
    fn declare(&mut self, request: Asked) -> Result<Response, Error> {
        match request {
            Asked::RegisterCommand(command) => {
                if self.state.uses.is_some() {
                    self.of_its_namespace(&command.name)?;
                }
                self.context.registry.register(command.clone())?;
                self.state.commands.push(command);
                *self.declarations_changed = true;
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
                self.state.bindings.push(Binding {
                    keys,
                    invocation: Invocation {
                        command: binding.command,
                        args: binding.args,
                    },
                    when,
                });
                *self.declarations_changed = true;
            }
            Asked::Subscribe(names) => {
                if let Some(name) = names.iter().find(|name| {
                    !WorkspaceEvent::NAMES.contains(&name.as_str())
                        && name.as_str() != POINTER
                        && name.as_str() != KEYS_CHANGED
                        && !commands::is_name(name)
                }) {
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

impl host::Host for Host<'_, '_> {
    fn handle(&mut self, call: Call) -> Response {
        let answered = if call.caller == self.state.name {
            let request = call.request;
            within_depth(request)
                .and_then(|request| self.allows(&request).map(|()| request))
                .and_then(|request| match request.effect() {
                    Effect::Read => self.read(request),
                    Effect::Write => self.queue(request),
                    Effect::App => self.ask_app(request),
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

/// The color of a tab of `color`, as a plugin learns it.
fn host_color(color: Option<lokyr_vt_core::page::Rgb>) -> Option<host::Color> {
    color.map(|color| host::Color::Rgb(color.r, color.g, color.b))
}

/// `tree`, a plugin's render, if it nests [`Node::DEEPEST`] levels at most,
/// as the host takes it; `failed` otherwise, the tree dropped without
/// recursion, where the render returned it, on the plugin's own thread.
pub(super) fn tree_within_depth(tree: Option<Node>) -> Result<Option<Node>, Error> {
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

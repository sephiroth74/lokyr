//! A host of the tests' own, for plugins' unit tests, as the `fake` feature
//! gives it.
//!
//! The test sets what the host holds: panes, each of its window, its tab,
//! its rows' lines, its links, its styles, its selection and its program's
//! directory, the focused pane, the clipboard, the text's size, the
//! targets that are not there, and the plugin's config and state. The host
//! answers the plugin's requests as the app's does: a read with what it
//! holds as it handles it, a request that the plugin's capabilities lack,
//! or of a pane that is not there, with its error; a write and a request of
//! the app's queued, and applied as the call returns, so that a read after
//! a write, in the same call, reads what was before.
//!
//! It keeps what the plugin declared, its commands, its bindings and its
//! subscriptions; what it did, its writes and its requests of the app's, as
//! the requests themselves, in their order, which a test asserts; its
//! decorations and its panels; the events that it is owed, which
//! [`FakeHost::deliver`] gives it; and the trees of its views, which
//! [`snapshot`] writes as text.
//!
//! ```ignore
//! let mut host = FakeHost::new("core-actions");
//! host.pane(PaneId(7)).selection = Some("ls -la".to_owned());
//! host.focus(Some(PaneId(7)));
//! host.command(&mut CoreActions, "pane.copy", Args::new())?;
//! assert_eq!(host.take_done(), [Request::SetClipboardText("ls -la".to_owned())]);
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::command::{Args, Capability, Code, Command, Error};
use crate::host::{
    self, Binding, DecorationLook, Direction, Layout, LineText, PaneId, StyleRun, TabId, TabInfo,
    WindowId, WindowInfo,
};
use crate::message::{Call, Data, Effect, Event, Request, Response};
use crate::plugin::{Cx, Plugin};
use crate::ui::{PanelId, PanelOptions, UiEvent, ViewId};
use crate::view::{Kind, Node};

/// The first id that the host gives a tab, a pane, a decoration or a read
/// of its own, past those that a test gives its panes and its tabs.
pub const FIRST_ID: u64 = 1001;

/// A pane of the fake host: where it is, and what its reads give.
#[derive(Clone, Debug, PartialEq)]
pub struct FakePane {
    /// Its window.
    pub window: WindowId,
    /// Its tab.
    pub tab: TabId,
    /// The logical line of each of its rows, by the row's number, as a read
    /// of a line's text gives it.
    pub lines: BTreeMap<i64, LineText>,
    /// The targets of the OSC 8 links of its cells, by their columns and
    /// their rows' numbers.
    pub links: BTreeMap<(u16, i64), String>,
    /// The styles of each of its rows, by the row's number.
    pub styles: BTreeMap<i64, Vec<StyleRun>>,
    /// Its selection's text.
    pub selection: Option<String>,
    /// The working directory of its program in the foreground.
    pub directory: Option<String>,
}

impl Default for FakePane {
    fn default() -> Self {
        Self {
            window: WindowId(1),
            tab: TabId(1),
            lines: BTreeMap::new(),
            links: BTreeMap::new(),
            styles: BTreeMap::new(),
            selection: None,
            directory: None,
        }
    }
}

/// A decoration that the plugin laid: its pane, its cells and its look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FakeDecoration {
    /// Its id.
    pub id: u64,
    /// Its pane.
    pub pane: PaneId,
    /// Its first cell, by its column and its row's number.
    pub from: (u16, i64),
    /// Its last cell.
    pub to: (u16, i64),
    /// Its look.
    pub look: DecorationLook,
}

/// The host of a plugin's unit tests.
#[derive(Debug)]
pub struct FakeHost {
    caller: String,
    /// The capabilities that the plugin holds; `None` for every one, as a
    /// first-party plugin's.
    granted: Option<Vec<Capability>>,
    panes: BTreeMap<PaneId, FakePane>,
    focused: Option<PaneId>,
    clipboard: Option<String>,
    font_size: f32,
    missing: Vec<String>,
    config: Data,
    state: Data,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    subscribed: Vec<String>,
    done: Vec<Request>,
    decorations: Vec<FakeDecoration>,
    panels: BTreeMap<PanelId, PanelOptions>,
    views: BTreeMap<ViewId, Option<Node>>,
    owed: Vec<Event>,
    last_id: u64,
}

impl FakeHost {
    /// A host of the plugin `caller`, as its calls name it, of every
    /// capability, without panes, of text of 13 points, and of an empty
    /// config.
    pub fn new(caller: &str) -> Self {
        Self {
            caller: caller.to_owned(),
            granted: None,
            panes: BTreeMap::new(),
            focused: None,
            clipboard: None,
            font_size: 13.0,
            missing: Vec::new(),
            config: Data::Map(BTreeMap::new()),
            state: Data::Nil,
            commands: Vec::new(),
            bindings: Vec::new(),
            subscribed: Vec::new(),
            done: Vec::new(),
            decorations: Vec::new(),
            panels: BTreeMap::new(),
            views: BTreeMap::new(),
            owed: Vec::new(),
            last_id: FIRST_ID - 1,
        }
    }

    /// The host, whose plugin holds the capabilities of `granted` alone, as
    /// another than a first-party plugin does.
    pub fn with_capabilities(mut self, granted: Vec<Capability>) -> Self {
        self.granted = Some(granted);
        self
    }

    /// The pane `id`, which the host holds from now, of the window 1 and
    /// the tab 1 if it did not hold it.
    pub fn pane(&mut self, id: PaneId) -> &mut FakePane {
        self.panes.entry(id).or_default()
    }

    /// Gives `pane` the focus, or takes it away with `None`.
    pub fn focus(&mut self, pane: Option<PaneId>) -> &mut Self {
        if let Some(pane) = pane {
            self.panes.entry(pane).or_default();
        }
        self.focused = pane;
        self
    }

    /// Puts `text` on the clipboard, or empties it.
    pub fn set_clipboard(&mut self, text: Option<&str>) -> &mut Self {
        self.clipboard = text.map(str::to_owned);
        self
    }

    /// The clipboard's text.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    /// Gives the text `size`, in points, as its reads tell it.
    pub fn set_font_size(&mut self, size: f32) -> &mut Self {
        self.font_size = size;
        self
    }

    /// Has the targets of `missing` not there, which open nothing.
    pub fn set_missing(&mut self, missing: &[&str]) -> &mut Self {
        self.missing = missing.iter().map(|&target| target.to_owned()).collect();
        self
    }

    /// Gives the plugin the config `config`, a map of its keys' values, of
    /// which an active plugin hears as [`FakeHost::deliver`] goes on.
    pub fn set_config(&mut self, config: Data) -> &mut Self {
        if config != self.config {
            self.config = config;
            self.owed.push(Event::ConfigChanged);
        }
        self
    }

    /// Gives the plugin the state `state`, as an earlier run kept it.
    pub fn set_state(&mut self, state: Data) -> &mut Self {
        self.state = state;
        self
    }

    /// The plugin's state, as it last kept it.
    pub fn state(&self) -> &Data {
        &self.state
    }

    /// Owes the plugin `event`, which [`FakeHost::deliver`] gives it if it
    /// subscribed to its name, or if it is one of those of the plugin
    /// alone.
    pub fn post(&mut self, event: Event) -> &mut Self {
        self.owed.push(event);
        self
    }

    /// The commands that the plugin registered.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// The bindings that it made.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The names of the events that it subscribed to.
    pub fn subscriptions(&self) -> &[String] {
        &self.subscribed
    }

    /// What it did since the last take: its writes and its requests of the
    /// app's, as the requests themselves, in their order.
    pub fn take_done(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.done)
    }

    /// The decorations that it laid and did not take away, in their order.
    pub fn decorations(&self) -> &[FakeDecoration] {
        &self.decorations
    }

    /// Its open panels, by their ids.
    pub fn panels(&self) -> &BTreeMap<PanelId, PanelOptions> {
        &self.panels
    }

    /// The last tree of `view` that it rendered, if it gave one.
    pub fn tree(&self, view: &ViewId) -> Option<&Node> {
        self.views.get(view).and_then(Option::as_ref)
    }

    /// Activates `plugin`.
    pub fn activate(&mut self, plugin: &mut dyn Plugin) -> Result<(), Error> {
        self.call(true, |cx| plugin.activate(cx))
    }

    /// Runs `plugin`'s command `name` with `args`, as the registry passes
    /// them.
    pub fn command(
        &mut self,
        plugin: &mut dyn Plugin,
        name: &str,
        args: Args,
    ) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_command(cx, name, args))
    }

    /// Whether `plugin`'s command `name` would do something with `args`, as
    /// a menu asks: what the call asks is dropped.
    pub fn applies(&mut self, plugin: &mut dyn Plugin, name: &str, args: &Args) -> bool {
        let mut applies = false;
        let _ = self.call(false, |cx| {
            applies = plugin.applies(cx, name, args);
            Ok(())
        });
        applies
    }

    /// Gives `plugin` `events`, as a batch.
    pub fn events(&mut self, plugin: &mut dyn Plugin, events: &[Event]) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_event(cx, events))
    }

    /// Gives `plugin` the events that it is owed, as a batch, if it is owed
    /// any: those of the plugin alone, as a panel's closing, a read's rows
    /// and its config's change, and those that it subscribed to, of the
    /// workspace's and those that it emitted; the others go.
    pub fn deliver(&mut self, plugin: &mut dyn Plugin) -> Result<(), Error> {
        let owed = std::mem::take(&mut self.owed);
        let batch: Vec<Event> = owed
            .into_iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Lines { .. } | Event::PanelClosed(_) | Event::ConfigChanged
                ) || self.subscribed.iter().any(|name| name == event.name())
            })
            .collect();
        if batch.is_empty() {
            return Ok(());
        }
        self.events(plugin, &batch)
    }

    /// Has `plugin` render `view`, whose tree the host keeps.
    pub fn render(&mut self, plugin: &mut dyn Plugin, view: &ViewId) -> Option<Node> {
        let mut tree = None;
        let _ = self.call(true, |cx| {
            tree = plugin.render(cx, view);
            Ok(())
        });
        self.views.insert(view.clone(), tree.clone());
        tree
    }

    /// Gives `plugin` `event`, of a node of one of its views.
    pub fn ui_event(&mut self, plugin: &mut dyn Plugin, event: &UiEvent) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_ui_event(cx, event))
    }

    /// Makes a call of the plugin's, `call`, on the host, and applies its
    /// writes as it returns, with `apply`, or drops them.
    fn call(
        &mut self,
        apply: bool,
        call: impl FnOnce(&mut Cx<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let caller = self.caller.clone();
        let mut calling = Calling {
            host: self,
            queued: Vec::new(),
        };
        let result = call(&mut Cx::new(&mut calling, &caller));
        let queued = calling.queued;
        if apply {
            for request in queued {
                self.apply(request);
            }
        }
        result
    }

    /// A new id of a decoration, a read, a tab or a pane.
    fn next_id(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }

    /// The pane `pane`; `not-found` if the host does not hold it.
    fn held(&self, pane: PaneId) -> Result<&FakePane, Error> {
        self.panes
            .get(&pane)
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// Applies `request`, a write or a request of the app's, as the call
    /// returns.
    fn apply(&mut self, request: Request) {
        match &request {
            Request::SetClipboardText(text) => self.clipboard = Some(text.clone()),
            Request::Undecorate { id, .. } => self.decorations.retain(|laid| laid.id != *id),
            Request::SetState(state) => self.state = state.clone(),
            Request::FocusPane(pane) => self.focused = Some(*pane),
            Request::ClosePane(pane) => {
                self.panes.remove(pane);
                if self.focused == Some(*pane) {
                    self.focused = None;
                }
            }
            Request::Emit {
                name,
                schema,
                value,
            } => self.owed.push(Event::Custom {
                name: name.clone(),
                schema: schema.clone(),
                value: value.clone(),
            }),
            _ => {}
        }
        self.done.push(request);
    }

    /// The windows of the host's panes, each of its tabs, the first its
    /// active one.
    fn windows(&self) -> Vec<WindowInfo> {
        let mut windows: Vec<WindowInfo> = Vec::new();
        for pane in self.panes.values() {
            match windows.iter_mut().find(|window| window.id == pane.window) {
                Some(window) if !window.tabs.contains(&pane.tab) => window.tabs.push(pane.tab),
                Some(_) => {}
                None => windows.push(WindowInfo {
                    id: pane.window,
                    tabs: vec![pane.tab],
                    active: pane.tab,
                }),
            }
        }
        windows
    }

    /// The panes of `tab`, in their order.
    fn panes_of(&self, tab: TabId) -> Vec<PaneId> {
        self.panes
            .iter()
            .filter(|(_, pane)| pane.tab == tab)
            .map(|(&id, _)| id)
            .collect()
    }
}

/// The host of one call, which queues its writes.
struct Calling<'h> {
    host: &'h mut FakeHost,
    queued: Vec<Request>,
}

impl Calling<'_> {
    /// Answers `request`, a read, with what the host holds.
    fn read(&mut self, request: &Request) -> Result<Response, Error> {
        let host = &*self.host;
        Ok(match request {
            Request::ClipboardText => Response::Text(host.clipboard.clone()),
            Request::FocusedWindow => Response::Window(
                host.focused
                    .and_then(|pane| host.panes.get(&pane))
                    .map(|pane| pane.window),
            ),
            Request::FocusedPane => Response::Pane(host.focused),
            Request::SelectionText(pane) => Response::Text(host.held(*pane)?.selection.clone()),
            Request::LineText { pane, row, .. } => {
                Response::Line(host.held(*pane)?.lines.get(row).cloned())
            }
            Request::LinkAt { pane, column, row } => {
                Response::Text(host.held(*pane)?.links.get(&(*column, *row)).cloned())
            }
            Request::WorkingDirectory(pane) => Response::Text(host.held(*pane)?.directory.clone()),
            Request::Styles { pane, row } => {
                Response::Styles(host.held(*pane)?.styles.get(row).cloned())
            }
            Request::ReadLines { pane, first, count } => {
                let lines: Vec<host::Line> = host
                    .held(*pane)?
                    .lines
                    .range(*first..)
                    .take(*count)
                    .map(|(&row, line)| host::Line {
                        row,
                        text: line.text.clone(),
                        wrapped: false,
                    })
                    .collect();
                let (pane, id) = (*pane, self.host.next_id());
                self.host.owed.push(Event::Lines {
                    id,
                    pane,
                    lines,
                    complete: true,
                });
                Response::Reading(id)
            }
            Request::Opens(target) => {
                Response::Boolean(opening(target).is_ok() && !host.missing.contains(target))
            }
            Request::FontSize => Response::FontSize(host.font_size),
            Request::Windows => Response::Windows(host.windows()),
            Request::Tabs(window) => {
                let windows = host.windows();
                let Some(found) = windows.iter().find(|shown| shown.id == *window) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                };
                Response::Tabs(
                    found
                        .tabs
                        .iter()
                        .map(|&tab| {
                            let panes = host.panes_of(tab);
                            TabInfo {
                                id: tab,
                                window: *window,
                                title: String::new(),
                                active: tab == found.active,
                                focused: host
                                    .focused
                                    .filter(|pane| panes.contains(pane))
                                    .or_else(|| panes.first().copied())
                                    .unwrap_or(PaneId(0)),
                                panes,
                                zoomed: None,
                            }
                        })
                        .collect(),
                )
            }
            Request::Layout(tab) => {
                let panes = host.panes_of(*tab);
                let Some((&last, rest)) = panes.split_last() else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no tab {}", tab.0),
                    ));
                };
                // The panes side by side, the first leftmost.
                let layout = rest
                    .iter()
                    .rev()
                    .fold(Layout::Pane(last), |second, &first| Layout::Split {
                        direction: Direction::Horizontal,
                        ratio: None,
                        first: Box::new(Layout::Pane(first)),
                        second: Box::new(second),
                    });
                Response::Layout(layout)
            }
            Request::Config => Response::Data(host.config.clone()),
            Request::State => Response::Data(host.state.clone()),
            other => {
                return Err(Error::new(Code::Fault, format!("{other:?} is not a read")));
            }
        })
    }

    /// Checks `request`, a write or a request of the app's, and queues it:
    /// one of a pane that the host does not hold, or of a target that does
    /// not open, fails now.
    fn queue(&mut self, request: Request) -> Result<Response, Error> {
        let response = match &request {
            Request::SelectAll(pane)
            | Request::ClearScrollback(pane)
            | Request::Clear(pane)
            | Request::Paste { pane, .. }
            | Request::SendText { pane, .. }
            | Request::Scroll { pane, .. }
            | Request::Undecorate { pane, .. }
            | Request::FocusPane(pane)
            | Request::ClosePane(pane) => {
                self.host.held(*pane)?;
                Response::Done
            }
            Request::Decorate {
                pane,
                from,
                to,
                look,
            } => {
                self.host.held(*pane)?;
                let id = self.host.next_id();
                self.host.decorations.push(FakeDecoration {
                    id,
                    pane: *pane,
                    from: *from,
                    to: *to,
                    look: *look,
                });
                Response::Decoration(id)
            }
            Request::Open(target) => {
                opening(target).map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::NewTab { .. } => {
                let (tab, pane) = (TabId(self.host.next_id()), PaneId(self.host.next_id()));
                Response::Tab { tab, pane }
            }
            Request::Split { pane, .. } => {
                self.host.held(*pane)?;
                Response::Pane(Some(PaneId(self.host.next_id())))
            }
            Request::OpenPanel { id, options } => {
                if self.host.panels.contains_key(id) {
                    return Err(Error::new(
                        Code::AlreadyRegistered,
                        format!("the plugin has a panel `{}` open", id.0),
                    ));
                }
                self.host.panels.insert(id.clone(), options.clone());
                Response::Done
            }
            Request::ClosePanel(id) => {
                if self.host.panels.remove(id).is_none() {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no panel `{}` open", id.0),
                    ));
                }
                self.host.owed.push(Event::PanelClosed(id.clone()));
                Response::Done
            }
            Request::Emit { name, .. } => {
                let namespace = self.host.caller.rsplit('/').next().unwrap_or_default();
                let own = name
                    .strip_prefix(namespace)
                    .is_some_and(|rest| rest.starts_with('.'));
                if !own {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!("`{name}` is not an event of the plugin's, `{namespace}.event`"),
                    ));
                }
                Response::Done
            }
            _ => Response::Done,
        };
        self.queued.push(request);
        Ok(response)
    }

    /// Takes `request`, a declaration, at once.
    fn declare(&mut self, request: Request) -> Response {
        match request {
            Request::RegisterCommand(command) => self.host.commands.push(command),
            Request::Bind(binding) => self.host.bindings.push(binding),
            Request::Subscribe(names) => {
                for name in names {
                    if !self.host.subscribed.contains(&name) {
                        self.host.subscribed.push(name);
                    }
                }
            }
            _ => {}
        }
        Response::Done
    }
}

impl host::Host for Calling<'_> {
    fn handle(&mut self, call: Call) -> Response {
        if call.caller != self.host.caller {
            return Response::Failed(Error::new(
                Code::Fault,
                format!(
                    "a call of the plugin `{}` came as `{}`'s",
                    self.host.caller, call.caller
                ),
            ));
        }
        let request = call.request;
        if let (Some(needed), Some(granted)) = (request.capability(), &self.host.granted)
            && !granted.contains(&needed)
        {
            return Response::Failed(Error::new(
                Code::MissingCapability,
                format!(
                    "the plugin `{}` lacks the capability `{needed}`",
                    self.host.caller
                ),
            ));
        }
        let answered = match request.effect() {
            Effect::Read => self.read(&request),
            Effect::Write | Effect::App => self.queue(request),
            Effect::Declaration => Ok(self.declare(request)),
        };
        answered.unwrap_or_else(Response::Failed)
    }
}

/// The schemes of the URLs that open, as the app's host has them.
const SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

/// Whether `target` opens, as the app's host takes it: a URL of one of
/// [`SCHEMES`], or a path from `/` or `~`; the error of what does not.
fn opening(target: &str) -> Result<(), String> {
    if target.starts_with('/') || target == "~" || target.starts_with("~/") {
        return Ok(());
    }
    match target.split_once(':') {
        Some((scheme, _)) if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) => Ok(()),
        Some((scheme, _)) => Err(format!(
            "`{scheme}:` does not open: only {}",
            SCHEMES.join(", ")
        )),
        None => Err(format!(
            "`{target}` is neither a URL nor a path that is absolute or from `~`"
        )),
    }
}

/// `tree` as text: a line of each node, two spaces deeper for each level,
/// of its kind, its text, and its key, label, role, tooltip and focus when
/// it has them; a list of its count and its row's template, and a view of
/// tabs of its tabs and the one chosen. Its layout and its style are not
/// told, as a snapshot tells the tree's content.
pub fn snapshot(tree: &Node) -> String {
    let mut out = String::new();
    write_node(&mut out, tree, 0);
    out
}

fn write_node(out: &mut String, node: &Node, depth: usize) {
    let _ = write!(out, "{}", "  ".repeat(depth));
    let quoted = |text: &str| format!("{text:?}");
    match &node.kind {
        Kind::Container => out.push_str("container"),
        Kind::Text(text) => {
            let _ = write!(out, "text {}", quoted(text));
        }
        Kind::Icon(name) => {
            let _ = write!(out, "icon {}", quoted(name));
        }
        Kind::Spacer => out.push_str("spacer"),
        Kind::Field(field) => {
            let _ = write!(out, "field {}", quoted(&field.placeholder));
        }
        Kind::Button(button) => {
            let _ = write!(out, "button {}", quoted(&button.label));
            if !button.icon.is_empty() {
                let _ = write!(out, " icon={}", quoted(&button.icon));
            }
            if button.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Toggle(toggle) => {
            let _ = write!(out, "toggle {}", quoted(&toggle.label));
            if !toggle.icon.is_empty() {
                let _ = write!(out, " icon={}", quoted(&toggle.icon));
            }
            out.push_str(if toggle.on { " on" } else { " off" });
            if toggle.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Separator => out.push_str("separator"),
        Kind::List(list) => {
            let _ = write!(
                out,
                "list of {}, {} given from {}",
                list.count,
                list.items.len(),
                list.first
            );
        }
        Kind::Scroll => out.push_str("scroll"),
        Kind::Progress(progress) => {
            let _ = write!(out, "progress {:?} {:?}", progress.shape, progress.state);
            if progress.state != crate::view::ProgressState::Indeterminate {
                let _ = write!(out, " {}", progress.value);
            }
        }
        Kind::Tabs(tabs) => {
            let labels: Vec<String> = tabs.tabs.iter().map(|tab| quoted(&tab.label)).collect();
            let _ = write!(out, "tabs {} chosen {}", labels.join(" | "), tabs.chosen);
        }
    }
    if let Some(key) = &node.key {
        let _ = write!(out, " key={}", key.0);
    }
    if let Some(label) = &node.label {
        let _ = write!(out, " label={}", quoted(label));
    }
    if let Some(role) = &node.role {
        let _ = write!(out, " role={role:?}");
    }
    if let Some(tooltip) = &node.tooltip {
        let _ = write!(out, " tooltip={}", quoted(tooltip));
    }
    if node.focusable {
        out.push_str(" focusable");
    }
    out.push('\n');
    if let Kind::List(list) = &node.kind {
        let _ = writeln!(out, "{}row:", "  ".repeat(depth + 1));
        write_node(out, &list.row, depth + 2);
    }
    for child in &node.children {
        write_node(out, child, depth + 1);
    }
}

#[cfg(test)]
mod tests;

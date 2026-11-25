//! Built-in plugin of the automation: what scripts and shortcuts of other
//! apps ask of Lokyr, which the host's bridges of the system hand it as the
//! `automation` event, and which it answers of the requests that a plugin
//! has. It opens windows, tabs and splits, in a folder and running a
//! command line at their shells' start; it sends text, brings a window, a
//! tab or a pane forward and closes it; it runs a command of the registry
//! as the user's own; and it tells the windows, their tabs and their panes,
//! by the host's ids, which stay the same between a script's requests.
//!
//! A pane's text may hold a secret: before an app reads one, the user
//! answers a question about that app, whose answer the plugin keeps in its
//! state, the default button refusing. Like every built-in plugin, it
//! depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};

use lokyr_plugin_sdk::automation::{
    AutomationAnswer, AutomationEdge, AutomationPane, AutomationRequest, AutomationTab,
    AutomationTarget, AutomationWindow, LONGEST_TEXT,
};
use lokyr_plugin_sdk::command::{Args, Code, Error, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{Direction, PaneId, Side, TabId, TabInfo, WindowId};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

lokyr_plugin_sdk::export_plugin!(Automation);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/automation.ftl")),
    ("it", include_str!("../locales/it/automation.ftl")),
];

/// The question's buttons, the first the default.
const DENY: u32 = 0;
const ALLOW: u32 = 1;

/// The key of the state that keeps the apps' answers.
const READERS: &str = "readers";

/// The command that brings a window of a number forward.
const GOTO_WINDOW: &str = "window.goto";

/// The plugin of the automation.
#[derive(Debug, Default)]
pub struct Automation {
    /// The apps' answers to the question of reading a pane's text, by the
    /// apps' names, of its state, once read.
    readers: Option<BTreeMap<String, bool>>,
    /// The question that waits for its answer, of its app.
    question: Option<(u64, String)>,
    /// The reads of a pane's text that wait for an app's answer, in their
    /// order: each request's id, its app, its pane and whether it reads
    /// the selection.
    reads: Vec<Read>,
    /// The windows asked to open, in their order, which take the next
    /// windows that open: each request's id and the command line of its
    /// shell's start.
    opening: VecDeque<(u64, Option<String>)>,
}

/// A read of a pane's text that waits for its app's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Read {
    id: u64,
    client: String,
    pane: Option<PaneId>,
    selection: bool,
}

impl Plugin for Automation {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        // A window that a request opened tells its tab and its pane once
        // it opens.
        cx.subscribe(&["window-opened"])
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            match event {
                Event::Automation {
                    id,
                    client,
                    request,
                } => {
                    let answer = match self.take(cx, *id, client, request.clone()) {
                        Ok(answer) => answer,
                        Err(error) => Some(AutomationAnswer::of_error(&error)),
                    };
                    if let Some(answer) = answer {
                        cx.answer_automation(*id, answer)?;
                    }
                }
                Event::Workspace(WorkspaceEvent::WindowOpened { window }) => {
                    if let Some((id, command)) = self.opening.pop_front() {
                        let answer = opened_window(cx, *window, command.as_deref())
                            .unwrap_or_else(|error| AutomationAnswer::of_error(&error));
                        cx.answer_automation(id, answer)?;
                    }
                }
                Event::Chosen { id, button }
                    if self.question.as_ref().is_some_and(|(asked, _)| asked == id) =>
                {
                    self.chosen(cx, *button)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }
}

impl Automation {
    /// Does what `request` of `client`, of the id `id`, asks: its answer,
    /// or none of an answer that comes later, as a window's that opens or
    /// a read that waits for the user's answer.
    fn take(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        client: &str,
        request: AutomationRequest,
    ) -> Result<Option<AutomationAnswer>, Error> {
        let answer = match request {
            AutomationRequest::Workspace => AutomationAnswer::Workspace(workspace(cx)?),
            AutomationRequest::NewWindow { folder, command } => {
                match folder {
                    Some(folder) => cx.open_window_in(&folder)?,
                    None => cx.open_window()?,
                }
                self.opening.push_back((id, command));
                return Ok(None);
            }
            AutomationRequest::NewTab {
                window,
                folder,
                command,
            } => {
                let window = match window {
                    Some(window) => window,
                    None => front_window(cx)?,
                };
                let (tab, pane) = match folder {
                    Some(folder) => cx.new_tab_in(window, None, &folder)?,
                    None => cx.new_tab(window, None)?,
                };
                start(cx, pane, command.as_deref())?;
                cx.activate_tab(tab)?;
                AutomationAnswer::Opened { window, tab, pane }
            }
            AutomationRequest::Split {
                pane,
                edge,
                folder,
                command,
            } => {
                let split = pane_or_focused(cx, pane)?;
                let (window, tab) = place_of(cx, split)?;
                let (direction, side) = match edge {
                    AutomationEdge::Right => (Direction::Horizontal, Side::Second),
                    AutomationEdge::Left => (Direction::Horizontal, Side::First),
                    AutomationEdge::Down => (Direction::Vertical, Side::Second),
                    AutomationEdge::Up => (Direction::Vertical, Side::First),
                };
                let pane = match folder {
                    Some(folder) => cx.split_in(split, direction, side, &folder)?,
                    None => cx.split(split, direction, side)?,
                };
                start(cx, pane, command.as_deref())?;
                cx.focus_pane(pane)?;
                AutomationAnswer::Opened { window, tab, pane }
            }
            AutomationRequest::SendText { pane, text } => {
                let pane = pane_or_focused(cx, pane)?;
                cx.send_text(pane, &text)?;
                AutomationAnswer::Done
            }
            AutomationRequest::Focus(target) => {
                focus(cx, target)?;
                AutomationAnswer::Done
            }
            AutomationRequest::Close(target) => {
                match target {
                    AutomationTarget::Window(window) => cx.close_window(window)?,
                    AutomationTarget::Tab(tab) => cx.close_tab(tab, true)?,
                    AutomationTarget::Pane(pane) => cx.close_pane(pane, true)?,
                }
                AutomationAnswer::Done
            }
            AutomationRequest::Run { command, args } => {
                cx.run_command(&command, args)?;
                AutomationAnswer::Done
            }
            AutomationRequest::PaneText { pane, selection } => {
                let read = Read {
                    id,
                    client: client.to_owned(),
                    pane,
                    selection,
                };
                return self.read(cx, read);
            }
        };
        Ok(Some(answer))
    }

    /// Reads a pane's text as `read` asks, if its app may: its answer, or
    /// none while it waits for the user's answer, which the question asks.
    fn read(&mut self, cx: &mut Cx<'_>, read: Read) -> Result<Option<AutomationAnswer>, Error> {
        match self.readers(cx).get(&read.client).copied() {
            Some(true) => Ok(Some(AutomationAnswer::Text(text(
                cx,
                read.pane,
                read.selection,
            )?))),
            Some(false) => Err(refused(&read.client)),
            None => {
                let client = read.client.clone();
                self.reads.push(read);
                if self.question.is_none() {
                    self.ask(cx, &client)?;
                }
                Ok(None)
            }
        }
    }

    /// Asks the user whether `client` may read the panes' text.
    fn ask(&mut self, cx: &mut Cx<'_>, client: &str) -> Result<(), Error> {
        let message = cx.message(
            "automation-read-message",
            &[("client", Data::Text(client.to_owned()))],
        )?;
        // On the window in front, or on the first while Lokyr is behind
        // the app that asks.
        let window = front_window(cx)?;
        let id = cx.ask_choice(
            Some(window),
            (&message, "automation-read-detail"),
            &["automation-read-deny", "automation-read-allow"],
        )?;
        self.question = Some((id, client.to_owned()));
        Ok(())
    }

    /// The user's answer to the question: its app's reads answered, its
    /// answer kept unless the question closed of none; then the next app's
    /// question, of the reads that wait.
    fn chosen(&mut self, cx: &mut Cx<'_>, button: Option<u32>) -> Result<(), Error> {
        let Some((_, client)) = self.question.take() else {
            return Ok(());
        };
        let allowed = button == Some(ALLOW);
        if matches!(button, Some(DENY | ALLOW)) {
            self.readers(cx).insert(client.clone(), allowed);
            // An answer that no state keeps, as of a run of no state's
            // folder, holds for this run: the reads are answered all the
            // same.
            let _kept = self.keep_readers(cx);
        }
        let (now, later): (Vec<Read>, Vec<Read>) = std::mem::take(&mut self.reads)
            .into_iter()
            .partition(|read| read.client == client);
        self.reads = later;
        for read in now {
            let answer = if allowed {
                text(cx, read.pane, read.selection).map(AutomationAnswer::Text)
            } else {
                Err(refused(&client))
            };
            cx.answer_automation(
                read.id,
                answer.unwrap_or_else(|error| AutomationAnswer::of_error(&error)),
            )?;
        }
        if let Some(next) = self.reads.first().map(|read| read.client.clone()) {
            // An app answered meanwhile is answered at once.
            let known = self.readers(cx).get(&next).copied();
            match known {
                Some(_) => {
                    let reads = std::mem::take(&mut self.reads);
                    for read in reads {
                        let id = read.id;
                        if let Some(answer) = self
                            .read(cx, read)
                            .unwrap_or_else(|error| Some(AutomationAnswer::of_error(&error)))
                        {
                            cx.answer_automation(id, answer)?;
                        }
                    }
                }
                None => self.ask(cx, &next)?,
            }
        }
        Ok(())
    }

    /// The apps' answers, of the plugin's state, read once.
    fn readers(&mut self, cx: &mut Cx<'_>) -> &mut BTreeMap<String, bool> {
        self.readers.get_or_insert_with(|| {
            let Ok(Data::Map(state)) = cx.state() else {
                return BTreeMap::new();
            };
            let Some(Data::Map(readers)) = state.get(READERS) else {
                return BTreeMap::new();
            };
            readers
                .iter()
                .filter_map(|(client, allowed)| match allowed {
                    Data::Boolean(allowed) => Some((client.clone(), *allowed)),
                    _ => None,
                })
                .collect()
        })
    }

    /// Keeps the apps' answers in the plugin's state.
    fn keep_readers(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let readers = self
            .readers(cx)
            .iter()
            .map(|(client, allowed)| (client.clone(), Data::Boolean(*allowed)))
            .collect();
        cx.set_state(Data::Map(BTreeMap::from([(
            READERS.to_owned(),
            Data::Map(readers),
        )])))
    }
}

/// The windows, each of its tabs and each tab of its panes.
fn workspace(cx: &mut Cx<'_>) -> Result<Vec<AutomationWindow>, Error> {
    let front = cx.focused_window();
    let mut windows = Vec::new();
    for window in cx.windows()? {
        let mut tabs = Vec::new();
        for tab in cx.tabs(window.id)? {
            tabs.push(tab_of(cx, &tab)?);
        }
        windows.push(AutomationWindow {
            id: window.id,
            title: window.title,
            focused: front == Some(window.id),
            tabs,
        });
    }
    Ok(windows)
}

/// The automation's tab of `tab`, of its panes.
fn tab_of(cx: &mut Cx<'_>, tab: &TabInfo) -> Result<AutomationTab, Error> {
    let mut panes = Vec::new();
    for &pane in &tab.panes {
        panes.push(AutomationPane {
            id: pane,
            title: cx.pane_title(pane)?.unwrap_or_default(),
            folder: cx.working_directory(pane)?,
            focused: pane == tab.focused,
        });
    }
    Ok(AutomationTab {
        id: tab.id,
        title: tab.title.clone(),
        active: tab.active,
        panes,
    })
}

/// The answer of the window `window` that a request opened: its tab and
/// its pane, whose shell runs `command` as it starts.
fn opened_window(
    cx: &mut Cx<'_>,
    window: WindowId,
    command: Option<&str>,
) -> Result<AutomationAnswer, Error> {
    let tab = cx.tabs(window)?.into_iter().next().ok_or_else(|| {
        Error::new(
            Code::NotFound,
            format!("the window {} has no tab", window.0),
        )
    })?;
    start(cx, tab.focused, command)?;
    Ok(AutomationAnswer::Opened {
        window,
        tab: tab.id,
        pane: tab.focused,
    })
}

/// Has the shell of the new pane `pane` run `command` as it starts, typed
/// ahead of its prompt.
fn start(cx: &mut Cx<'_>, pane: PaneId, command: Option<&str>) -> Result<(), Error> {
    match command {
        Some(command) if !command.trim().is_empty() => cx.send_text(pane, &format!("{command}\r")),
        _ => Ok(()),
    }
}

/// `pane`, or without one the focused pane, or, while Lokyr is behind the
/// app that asks, the pane with the keyboard of the shown tab of its first
/// window.
fn pane_or_focused(cx: &mut Cx<'_>, pane: Option<PaneId>) -> Result<PaneId, Error> {
    if let Some(pane) = pane.or_else(|| cx.focused_pane()) {
        return Ok(pane);
    }
    let window = front_window(cx)?;
    cx.tabs(window)?
        .into_iter()
        .find(|tab| tab.active)
        .map(|tab| tab.focused)
        .ok_or_else(no_window)
}

/// The window in front, or its first window while Lokyr is behind the app
/// that asks.
fn front_window(cx: &mut Cx<'_>) -> Result<WindowId, Error> {
    if let Some(window) = cx.focused_window() {
        return Ok(window);
    }
    cx.windows()?
        .first()
        .map(|window| window.id)
        .ok_or_else(no_window)
}

/// The window and the tab of `pane`.
fn place_of(cx: &mut Cx<'_>, pane: PaneId) -> Result<(WindowId, TabId), Error> {
    for window in cx.windows()? {
        for tab in cx.tabs(window.id)? {
            if tab.panes.contains(&pane) {
                return Ok((window.id, tab.id));
            }
        }
    }
    Err(Error::new(
        Code::NotFound,
        format!("there is no pane {}", pane.0),
    ))
}

/// Brings `target` forward: its window, by its number, its tab shown and
/// its pane given the keyboard.
fn focus(cx: &mut Cx<'_>, target: AutomationTarget) -> Result<(), Error> {
    let window = match target {
        AutomationTarget::Window(window) => window,
        AutomationTarget::Tab(tab) => {
            let window = window_of_tab(cx, tab)?;
            cx.activate_tab(tab)?;
            window
        }
        AutomationTarget::Pane(pane) => {
            let (window, tab) = place_of(cx, pane)?;
            cx.activate_tab(tab)?;
            cx.focus_pane(pane)?;
            window
        }
    };
    let number = cx
        .windows()?
        .into_iter()
        .find(|shown| shown.id == window)
        .ok_or_else(|| Error::new(Code::NotFound, format!("there is no window {}", window.0)))?
        .number;
    if let Some(number) = number {
        let args = Args::from([("index".to_owned(), Value::Integer(i64::from(number)))]);
        cx.run_command(GOTO_WINDOW, args)?;
    }
    Ok(())
}

/// The window of `tab`.
fn window_of_tab(cx: &mut Cx<'_>, tab: TabId) -> Result<WindowId, Error> {
    for window in cx.windows()? {
        if window.tabs.contains(&tab) {
            return Ok(window.id);
        }
    }
    Err(Error::new(
        Code::NotFound,
        format!("there is no tab {}", tab.0),
    ))
}

/// The text of `pane`, or of the focused pane: its selection, or the rows
/// of its screen, each without the spaces that end it, within
/// [`LONGEST_TEXT`].
fn text(cx: &mut Cx<'_>, pane: Option<PaneId>, selection: bool) -> Result<String, Error> {
    let pane = pane_or_focused(cx, pane)?;
    let mut text = if selection {
        cx.selection_text(pane)?.unwrap_or_default()
    } else {
        let rows: Vec<String> = cx
            .screen_cells(pane)?
            .into_iter()
            .map(|row| row.text.trim_end().to_owned())
            .collect();
        rows.join("\n").trim_end().to_owned()
    };
    if text.len() > LONGEST_TEXT {
        let mut end = LONGEST_TEXT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    Ok(text)
}

/// The error of a read that the user did not allow `client`.
fn refused(client: &str) -> Error {
    Error::new(
        Code::Failed,
        format!("“{client}” may not read the text of Lokyr's panes, as its user answered"),
    )
}

/// The error of a request of no window.
fn no_window() -> Error {
    Error::new(Code::Unavailable, "Lokyr has no window")
}

#[cfg(test)]
mod tests;

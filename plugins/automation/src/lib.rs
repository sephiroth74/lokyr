//! Built-in plugin of the automation: what scripts and shortcuts of other
//! apps ask of Lokyr, which the host's bridges of the system hand it as the
//! `automation` event, and which it answers of the requests that a plugin
//! has. It opens windows, tabs and splits, in a folder and running a
//! command line at their shells' start; it sends text, brings a window, a
//! tab or a pane forward and closes it; it runs a command of the registry
//! as the user's own; it runs a command line at a shell's prompt; it
//! sends a pane a mouse event over a cell; it
//! presses a key in a pane as the user does, its
//! binding's command or the program's; it
//! switches to a session of a name, which it makes
//! of none; and it tells the windows, their tabs and their panes,
//! by the host's ids, which stay the same between a script's requests.
//!
//! Before a client's first request, of any kind, the user answers whether
//! it may control Lokyr, as a client can type, run commands and read the
//! panes, whose text may hold a secret: the answers of apps kept in its
//! state, the default button refusing. Like every built-in plugin, it
//! depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use lokyr_plugin_sdk::automation::{
    ANSWER_WITHIN_MS, AutomationAnswer, AutomationClient, AutomationCommand, AutomationEdge,
    AutomationPane, AutomationRequest, AutomationTab, AutomationTarget, AutomationWindow,
    LONGEST_TEXT, QUESTION_WITHIN_MS,
};
use lokyr_plugin_sdk::command::{Args, Code, Error, Kind, Value};
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

/// The key of the state that keeps the clients' answers.
const CLIENTS: &str = "clients";

/// The command that brings a window of a number forward.
const GOTO_WINDOW: &str = "window.goto";

/// The command that shows the quick terminal.
const SHOW_QUICK_TERMINAL: &str = "window.show-quick-terminal";

/// The most answers of programs in Lokyr's own terminals kept, past which
/// the oldest is forgotten, and its program asked again.
const MOST_TERMINAL_ANSWERS: usize = 256;

/// The most requests that wait for their clients' answers to the question,
/// past which a new one fails at once, as each holds a timer of its own.
const MOST_WAITING: usize = 16;

/// How long a request waits for its client's answer, in milliseconds: a
/// second less than the host waits, past which the host fails it, so that
/// an answer that comes later never does it.
const WAITS_FOR_ANSWER_MS: u64 = QUESTION_WITHIN_MS - 1_000;

/// The plugin of the automation.
#[derive(Debug, Default)]
pub struct Automation {
    /// The clients' answers to the question, by their ids, of its state,
    /// once read.
    answers: Option<BTreeMap<String, bool>>,
    /// The answers of programs in Lokyr's own terminals, by their ids (a
    /// pane's shell and the programs down to the sender), oldest first,
    /// which no state keeps.
    in_terminals: VecDeque<(String, bool)>,
    /// The question that waits for its answer, of its client.
    question: Option<(u64, AutomationClient)>,
    /// The clients that wait to be asked, after the question that shows.
    asking: VecDeque<AutomationClient>,
    /// The requests that wait for their client's answer, in their order,
    /// of the clients that wait for it, as a shortcut.
    waiting: Vec<Waiting>,
    /// The windows asked to open, in their order, which take the next
    /// windows that open, each until its deadline.
    opening: VecDeque<Opening>,
    /// The window in front as the request under way came, which a request
    /// of no window nor pane takes: the one with the focus, or the last
    /// that had it while Lokyr is behind the app that asks.
    front: Option<WindowId>,
}

/// A window that a request asked to open, which waits for the next window
/// that opens: the request's id, the command line of its shell's start,
/// and the timer of its deadline, past which the host failed the request
/// and the next window that opens is none of its.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Opening {
    id: u64,
    command: Option<String>,
    timer: u64,
}

/// A request that waits for its client's answer to the question: its id,
/// its client, what it asks, the window in front as it came, and the timer
/// of its deadline, past which it fails and an answer does it no more.
#[derive(Clone, Debug, PartialEq)]
struct Waiting {
    id: u64,
    client: AutomationClient,
    request: AutomationRequest,
    front: Option<WindowId>,
    timer: u64,
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
                    front,
                } => self.came(cx, *id, client, request, *front),
                Event::Workspace(WorkspaceEvent::WindowOpened { window }) => {
                    if let Some(opening) = self.opening.pop_front() {
                        let _ = cx.cancel_timer(opening.timer);
                        let answer = opened_window(cx, *window, opening.command.as_deref())
                            .unwrap_or_else(|error| AutomationAnswer::of_error(&error));
                        answer_once(cx, opening.id, answer);
                    }
                }
                // A window that never opened, as the host failed its request:
                // the next window that opens is none of its; a request that
                // waited for its client's answer past its deadline fails.
                Event::Timer { id } => {
                    self.opening.retain(|opening| opening.timer != *id);
                    self.late(cx, *id);
                }
                Event::Chosen { id, button }
                    if self.question.as_ref().is_some_and(|(asked, _)| asked == id) =>
                {
                    self.chosen(cx, *button);
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
    /// A request of `client`, of the id `id`, as it comes: done at once of
    /// a client that the user allowed, refused of one that the user
    /// refused; of a client that the user has not answered, the question
    /// asked, and the request waiting for its answer, of a client that
    /// waits, or failed at once, of a script, which runs again once the
    /// user answers.
    fn came(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        client: &AutomationClient,
        request: &AutomationRequest,
        front: Option<WindowId>,
    ) {
        self.front = front;
        match self.answer_of(cx, client) {
            Some(true) => self.run(cx, id, request.clone()),
            Some(false) => answer_once(cx, id, AutomationAnswer::of_error(&refused(client))),
            None => {
                if client.waits {
                    if let Err(error) = self.wait(cx, id, client, request, front) {
                        answer_once(cx, id, AutomationAnswer::of_error(&error));
                        return;
                    }
                } else {
                    answer_once(cx, id, AutomationAnswer::of_error(&asking(client)));
                }
                let asked = self
                    .question
                    .as_ref()
                    .is_some_and(|(_, asked)| asked.id == client.id)
                    || self.asking.iter().any(|queued| queued.id == client.id);
                if !asked {
                    self.asking.push_back(client.clone());
                }
                if self.question.is_none() {
                    self.ask_next(cx);
                }
            }
        }
    }

    /// Keeps `request` of the id `id`, of `client`, waiting for the client's
    /// answer to the question, until its deadline; refused past
    /// [`MOST_WAITING`] requests that wait, or of no timer.
    fn wait(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        client: &AutomationClient,
        request: &AutomationRequest,
        front: Option<WindowId>,
    ) -> Result<(), Error> {
        if self.waiting.len() >= MOST_WAITING {
            return Err(Error::new(
                Code::Unavailable,
                format!(
                    "{MOST_WAITING} requests wait for the answer whether they may control Lokyr: answer, then try again"
                ),
            ));
        }
        let timer = cx.start_timer(Duration::from_millis(WAITS_FOR_ANSWER_MS), false)?;
        self.waiting.push(Waiting {
            id,
            client: client.clone(),
            request: request.clone(),
            front,
            timer,
        });
        Ok(())
    }

    /// Fails the request that waited for its client's answer of the timer
    /// `timer`, its deadline passed: an answer that comes later does it no
    /// more.
    fn late(&mut self, cx: &mut Cx<'_>, timer: u64) {
        let Some(at) = self
            .waiting
            .iter()
            .position(|waiting| waiting.timer == timer)
        else {
            return;
        };
        let waiting = self.waiting.remove(at);
        let error = Error::new(
            Code::Failed,
            format!(
                "“{}” waited too long for the answer whether it may control Lokyr",
                waiting.client.name
            ),
        );
        answer_once(cx, waiting.id, AutomationAnswer::of_error(&error));
    }

    /// Does what `request` of the id `id` asks, of a client that the user
    /// allowed, and answers it, now or once its answer comes.
    fn run(&mut self, cx: &mut Cx<'_>, id: u64, request: AutomationRequest) {
        match self.take(cx, id, request) {
            Ok(Some(answer)) => answer_once(cx, id, answer),
            Ok(None) => {}
            Err(error) => answer_once(cx, id, AutomationAnswer::of_error(&error)),
        }
    }

    /// Does what `request`, of the id `id`, asks: its answer, or none of an
    /// answer that comes later, as a window's that opens.
    fn take(
        &mut self,
        cx: &mut Cx<'_>,
        id: u64,
        request: AutomationRequest,
    ) -> Result<Option<AutomationAnswer>, Error> {
        let answer = match request {
            AutomationRequest::Workspace => AutomationAnswer::Workspace(workspace(cx)?),
            AutomationRequest::NewWindow {
                folder,
                command,
                environment,
            } => {
                cx.open_window_with(folder.as_deref(), &environment)?;
                let timer = cx.start_timer(Duration::from_millis(ANSWER_WITHIN_MS), false)?;
                self.opening.push_back(Opening { id, command, timer });
                return Ok(None);
            }
            AutomationRequest::NewTab {
                window,
                folder,
                command,
                environment,
            } => {
                let window = match window {
                    Some(window) => window,
                    None => front_window(cx, self.front)?,
                };
                let (tab, pane) = cx.new_tab_with(window, None, folder.as_deref(), &environment)?;
                start(cx, pane, command.as_deref())?;
                cx.activate_tab(tab)?;
                AutomationAnswer::Opened { window, tab, pane }
            }
            AutomationRequest::Split {
                pane,
                edge,
                folder,
                command,
                environment,
            } => {
                let split = pane_or_focused(cx, pane, self.front)?;
                let (window, tab) = place_of(cx, split)?;
                let (direction, side) = match edge {
                    AutomationEdge::Right => (Direction::Horizontal, Side::Second),
                    AutomationEdge::Left => (Direction::Horizontal, Side::First),
                    AutomationEdge::Down => (Direction::Vertical, Side::Second),
                    AutomationEdge::Up => (Direction::Vertical, Side::First),
                };
                let pane =
                    cx.split_with(split, direction, side, folder.as_deref(), &environment)?;
                start(cx, pane, command.as_deref())?;
                cx.focus_pane(pane)?;
                AutomationAnswer::Opened { window, tab, pane }
            }
            AutomationRequest::SendText { pane, text } => {
                let pane = pane_or_focused(cx, pane, self.front)?;
                cx.send_text(pane, &text)?;
                AutomationAnswer::Done
            }
            AutomationRequest::SendMouse { pane, input } => {
                let pane = pane_or_focused(cx, pane, self.front)?;
                cx.send_mouse(pane, input)?;
                AutomationAnswer::Done
            }
            AutomationRequest::RunLine { pane, line } => {
                let pane = pane_or_focused(cx, pane, self.front)?;
                cx.run_line(pane, &line)?;
                AutomationAnswer::Done
            }
            AutomationRequest::SendKey { pane, keys } => {
                let pane = pane_or_focused(cx, pane, self.front)?;
                cx.press_key(pane, &keys)?;
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
                let args = typed(cx, &command, args)?;
                cx.run_command(&command, args)?;
                AutomationAnswer::Done
            }
            AutomationRequest::Commands => AutomationAnswer::Commands(palette_commands(cx)?),
            AutomationRequest::PaneText { pane, selection } => {
                AutomationAnswer::Text(text(cx, pane, selection, self.front)?)
            }
            AutomationRequest::SwitchSession { name } => {
                switch_session(cx, &name)?;
                AutomationAnswer::Done
            }
        };
        Ok(Some(answer))
    }

    /// The user's answer for `client`, if the user answered: of a program
    /// in Lokyr's terminals, of this run; of another, of the plugin's
    /// state.
    fn answer_of(&mut self, cx: &mut Cx<'_>, client: &AutomationClient) -> Option<bool> {
        if client.terminal {
            return self
                .in_terminals
                .iter()
                .find(|(id, _)| *id == client.id)
                .map(|&(_, allowed)| allowed);
        }
        self.answers(cx).get(&client.id).copied()
    }

    /// Asks the next client that waits to be asked whether it may control
    /// Lokyr, on the window in front: a client answered meanwhile has its
    /// requests done; a question that cannot show, as of no window, fails
    /// its client's requests, and the next is asked.
    fn ask_next(&mut self, cx: &mut Cx<'_>) {
        while let Some(client) = self.asking.pop_front() {
            if let Some(allowed) = self.answer_of(cx, &client) {
                self.answer_waiting(cx, &client, allowed);
                continue;
            }
            match self.ask(cx, &client) {
                Ok(id) => {
                    self.question = Some((id, client));
                    return;
                }
                Err(error) => self.fail_waiting(cx, &client, &error),
            }
        }
    }

    /// Asks the user whether `client` may control Lokyr: the question's id.
    fn ask(&mut self, cx: &mut Cx<'_>, client: &AutomationClient) -> Result<u64, Error> {
        let message = cx.message(
            "automation-control-message",
            &[("client", Data::Text(client.name.clone()))],
        )?;
        let detail = if client.terminal {
            "automation-control-terminal-detail"
        } else {
            "automation-control-detail"
        };
        let window = front_window(cx, self.front)?;
        cx.ask_choice(
            Some(window),
            (&message, detail),
            &["automation-control-deny", "automation-control-allow"],
        )
    }

    /// The user's answer to the question: kept for its client unless the
    /// question closed of none, a program's in Lokyr's terminals in memory
    /// alone, while its pane's shell runs; its client's requests done or
    /// refused, each answered once; then the next client asked.
    fn chosen(&mut self, cx: &mut Cx<'_>, button: Option<u32>) {
        let Some((_, client)) = self.question.take() else {
            return;
        };
        let allowed = button == Some(ALLOW);
        if matches!(button, Some(DENY | ALLOW)) {
            if client.terminal {
                self.in_terminals.retain(|(id, _)| *id != client.id);
                self.in_terminals.push_back((client.id.clone(), allowed));
                while self.in_terminals.len() > MOST_TERMINAL_ANSWERS {
                    self.in_terminals.pop_front();
                }
            } else {
                self.answers(cx).insert(client.id.clone(), allowed);
                // An answer that no state keeps, as of a run of no state's
                // folder, holds for this run.
                let _kept = self.keep_answers(cx);
            }
        }
        self.answer_waiting(cx, &client, allowed);
        self.ask_next(cx);
    }

    /// Answers the requests of `client` that wait: done of `allowed`,
    /// refused otherwise.
    fn answer_waiting(&mut self, cx: &mut Cx<'_>, client: &AutomationClient, allowed: bool) {
        let (now, later): (Vec<Waiting>, Vec<Waiting>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|waiting| waiting.client.id == client.id);
        self.waiting = later;
        for waiting in now {
            let _ = cx.cancel_timer(waiting.timer);
            if allowed {
                self.front = waiting.front;
                self.run(cx, waiting.id, waiting.request);
            } else {
                answer_once(cx, waiting.id, AutomationAnswer::of_error(&refused(client)));
            }
        }
    }

    /// Fails the requests of `client` that wait, of `error`.
    fn fail_waiting(&mut self, cx: &mut Cx<'_>, client: &AutomationClient, error: &Error) {
        let (now, later): (Vec<Waiting>, Vec<Waiting>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|waiting| waiting.client.id == client.id);
        self.waiting = later;
        for waiting in now {
            let _ = cx.cancel_timer(waiting.timer);
            answer_once(cx, waiting.id, AutomationAnswer::of_error(error));
        }
    }

    /// The clients' answers, of the plugin's state, read once.
    fn answers(&mut self, cx: &mut Cx<'_>) -> &mut BTreeMap<String, bool> {
        self.answers.get_or_insert_with(|| {
            let Ok(Data::Map(state)) = cx.state() else {
                return BTreeMap::new();
            };
            let Some(Data::Map(answers)) = state.get(CLIENTS) else {
                return BTreeMap::new();
            };
            answers
                .iter()
                .filter_map(|(client, allowed)| match allowed {
                    Data::Boolean(allowed) => Some((client.clone(), *allowed)),
                    _ => None,
                })
                .collect()
        })
    }

    /// Keeps the clients' answers in the plugin's state.
    fn keep_answers(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let answers = self
            .answers(cx)
            .iter()
            .map(|(client, allowed)| (client.clone(), Data::Boolean(*allowed)))
            .collect();
        cx.set_state(Data::Map(BTreeMap::from([(
            CLIENTS.to_owned(),
            Data::Map(answers),
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
fn pane_or_focused(
    cx: &mut Cx<'_>,
    pane: Option<PaneId>,
    front: Option<WindowId>,
) -> Result<PaneId, Error> {
    if let Some(pane) = pane.or_else(|| cx.focused_pane()) {
        return Ok(pane);
    }
    let window = front_window(cx, front)?;
    cx.tabs(window)?
        .into_iter()
        .find(|tab| tab.active)
        .map(|tab| tab.focused)
        .ok_or_else(no_window)
}

/// The commands as the command palette lists them: those of no argument
/// that they need, then the menu bar's items of their arguments, each
/// once, as Split Right; an item of a command listed already, of no
/// arguments, is that command. Whether the menus enable them now does not
/// count, as a shortcut runs them later.
fn palette_commands(cx: &mut Cx<'_>) -> Result<Vec<AutomationCommand>, Error> {
    let mut commands: Vec<AutomationCommand> = cx
        .commands()?
        .into_iter()
        .filter(|command| command.params.iter().all(|param| param.default.is_some()))
        .map(|command| AutomationCommand {
            name: command.name,
            title: command.title,
            args: Args::new(),
        })
        .collect();
    for item in cx.menu_bar_items().unwrap_or_default() {
        let listed = commands
            .iter()
            .any(|command| command.name == item.command && command.args == item.args);
        if listed {
            continue;
        }
        commands.push(AutomationCommand {
            name: item.command,
            title: item.title,
            args: item.args,
        });
    }
    Ok(commands)
}

/// `args` of the kinds that `command`'s parameters take: a text of a whole
/// number, a number or a boolean read as the parameter's kind, as a
/// shortcut gives every argument as text; the others as they are, which
/// the command's run checks.
fn typed(cx: &mut Cx<'_>, command: &str, args: Args) -> Result<Args, Error> {
    let Some(info) = cx.commands()?.into_iter().find(|info| info.name == command) else {
        return Ok(args);
    };
    let typed = args
        .into_iter()
        .map(|(name, value)| {
            let kind = info
                .params
                .iter()
                .find(|param| param.name == name)
                .map(|param| param.kind);
            let read = match (&value, kind) {
                (Value::String(text), Some(Kind::Integer)) => {
                    text.trim().parse().ok().map(Value::Integer)
                }
                (Value::String(text), Some(Kind::Number)) => text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|number| number.is_finite())
                    .map(Value::Number),
                (Value::String(text), Some(Kind::Boolean)) => match text.trim() {
                    "true" | "yes" => Some(Value::Boolean(true)),
                    "false" | "no" => Some(Value::Boolean(false)),
                    _ => None,
                },
                _ => None,
            };
            (name, read.unwrap_or(value))
        })
        .collect();
    Ok(typed)
}

/// Switches to the session of `name`, the one used last of several, as the
/// sessions come in the order of their use, or makes it of none of that
/// name, which opens its first window.
fn switch_session(cx: &mut Cx<'_>, name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        return Err(Error::new(
            Code::MissingArgument,
            "a session's switch takes its name",
        ));
    }
    let found = cx
        .sessions()?
        .into_iter()
        .find(|session| session.name == name);
    match found {
        Some(session) if session.active => {}
        Some(session) => cx.switch_session(session.id)?,
        None => {
            cx.new_session(name, None)?;
        }
    }
    Ok(())
}

/// Answers the request `id` of `answer`, once: an answer that the host
/// refuses, of a request that it failed already as its deadline passed,
/// fails nothing else of the call, whose other answers and writes stand.
fn answer_once(cx: &mut Cx<'_>, id: u64, answer: AutomationAnswer) {
    let _ = cx.answer_automation(id, answer);
}

/// The window in front: the one with the focus; else `front`, the window
/// in front as the request came, the one used last while Lokyr is behind
/// the app that asks; else the first window of the session that shows, of
/// no quick terminal: never a window that does not show.
fn front_window(cx: &mut Cx<'_>, front: Option<WindowId>) -> Result<WindowId, Error> {
    if let Some(window) = cx.focused_window() {
        return Ok(window);
    }
    let windows = cx.windows()?;
    let shows = |id: WindowId| {
        windows
            .iter()
            .any(|window| window.id == id && !window.hidden)
    };
    if let Some(front) = front.filter(|&front| shows(front)) {
        return Ok(front);
    }
    let quick = |id: WindowId| windows.iter().any(|window| window.id == id && window.quick);
    cx.sessions()?
        .into_iter()
        .filter(|session| session.active)
        .flat_map(|session| session.windows)
        .find(|&window| shows(window) && !quick(window))
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

/// Brings `target` forward: its tab shown and its pane given the keyboard;
/// its window's session switched to, of a session that is hidden, as a
/// window's number is its session's; then its window, by its number; a
/// quick terminal shown.
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
    let shown = cx
        .windows()?
        .into_iter()
        .find(|shown| shown.id == window)
        .ok_or_else(|| Error::new(Code::NotFound, format!("there is no window {}", window.0)))?;
    if shown.quick {
        return cx.run_command(SHOW_QUICK_TERMINAL, Args::new());
    }
    let hidden = cx
        .sessions()?
        .into_iter()
        .find(|session| !session.active && session.windows.contains(&window));
    if let Some(session) = hidden {
        cx.switch_session(session.id)?;
    }
    if let Some(number) = shown.number {
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
fn text(
    cx: &mut Cx<'_>,
    pane: Option<PaneId>,
    selection: bool,
    front: Option<WindowId>,
) -> Result<String, Error> {
    let pane = pane_or_focused(cx, pane, front)?;
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

/// The error of a request of a client that the user did not allow.
fn refused(client: &AutomationClient) -> Error {
    Error::new(
        Code::Failed,
        format!(
            "“{}” may not control Lokyr, as its user answered",
            client.name
        ),
    )
}

/// The error of a script's request while the user has not answered whether
/// its client may control Lokyr, which the question asks.
fn asking(client: &AutomationClient) -> Error {
    Error::new(
        Code::Unavailable,
        format!(
            "Lokyr asks whether “{}” may control it: answer, then run the script again",
            client.name
        ),
    )
}

/// The error of a request of no window.
fn no_window() -> Error {
    Error::new(Code::Unavailable, "Lokyr has no window")
}

#[cfg(test)]
mod tests;

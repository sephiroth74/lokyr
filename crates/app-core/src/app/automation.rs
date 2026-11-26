//! The automation's requests, of the system's scripts and shortcuts, which
//! the platform's bridges hand the app: each goes to the plugin of the
//! capability `automation` as its event, and the plugin's answer goes back
//! to the platform, or an error past the answer's deadline, or at once
//! while no such plugin runs.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lokyr_kernel::commands::Code;
use lokyr_kernel::commands::{Args, Value};
use lokyr_kernel::plugins::automation::{
    ANSWER_WITHIN_MS, AutomationAnswer, AutomationEdge, AutomationRequest, AutomationTarget,
};
use lokyr_kernel::plugins::host::{PaneId, TabId, WindowId};
use lokyr_platform_core::automation as platform;
use lokyr_platform_core::{Platform, TimerId};

use super::App;
use crate::drawing::Backend;

/// What the answer of a request that no plugin takes tells.
pub(super) const OFF: &str =
    "Lokyr's automation is off: its plugin, lokyr/automation, is turned off in the Plugins page";

/// What the answer of a request that the plugin did not answer in time
/// tells.
pub(super) const LATE: &str = "Lokyr's automation did not answer in time";

/// The requests that wait for the plugin's answers, each of its deadline,
/// and the timer of the first.
#[derive(Debug, Default)]
pub(super) struct Automations {
    waiting: BTreeMap<u64, Instant>,
    timer: Option<TimerId>,
}

impl Automations {
    /// Whether `timer` is its timer.
    pub(super) fn is_timer(&self, timer: TimerId) -> bool {
        self.timer == Some(timer)
    }
}

impl<B: Backend> App<B> {
    /// Hands the automation's request `request` of `client`, of the
    /// platform's id `id`, to the plugin of the capability `automation`,
    /// with the window in front: the one with the focus, or the terminals'
    /// window that had it last, as Lokyr is behind the app that asks, but
    /// a quick terminal that hid; fails it at once while no such plugin
    /// runs, or of a text too long.
    pub(super) fn take_automation(
        &mut self,
        platform: &mut dyn Platform,
        id: u64,
        client: &str,
        request: platform::Request,
    ) {
        let request = request_of(request);
        if let Err(error) = request.check() {
            platform.answer_automation(id, answer_of(AutomationAnswer::of_error(&error)));
            return;
        }
        // A window that shows: neither the settings' window, nor a quick
        // terminal that hid, nor a window of a session that is hidden.
        let shows =
            |window: &lokyr_platform_core::WindowId| -> Option<lokyr_kernel::workspace::WindowId> {
                if self.quick.get(window).is_some_and(|(_, shown)| !shown) {
                    return None;
                }
                let id = self.windows.get(window)?.window;
                let active = self.workspace.active_session();
                let shown = self.workspace.window(id)?;
                (self.workspace.settings_window() != Some(id)
                    && shown.session().is_none_or(|session| session == active))
                .then_some(id)
            };
        let front = platform
            .focused_window()
            .and_then(|window| shows(&window))
            .or_else(|| self.front.and_then(|window| shows(&window)));
        if !self.plugins.automation(id, client, request, front) {
            platform.answer_automation(
                id,
                platform::Answer::Error {
                    code: Code::Unavailable.number(),
                    message: OFF.to_owned(),
                },
            );
            return;
        }
        let deadline = Instant::now() + Duration::from_millis(ANSWER_WITHIN_MS);
        self.automations.waiting.insert(id, deadline);
        self.settle_automation_timer(platform);
    }

    /// Gives the platform the plugin's answer to the request `id`, which
    /// waits no more.
    pub(super) fn answer_automation(
        &mut self,
        platform: &mut dyn Platform,
        id: u64,
        answer: AutomationAnswer,
    ) {
        if self.automations.waiting.remove(&id).is_none() {
            return;
        }
        platform.answer_automation(id, answer_of(answer));
        self.settle_automation_timer(platform);
    }

    /// Fails the requests whose deadlines passed by `now`, of no answer of
    /// the plugin's.
    pub(super) fn automations_late(&mut self, platform: &mut dyn Platform, now: Instant) {
        self.automations.timer = None;
        let late: Vec<u64> = self
            .automations
            .waiting
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(&id, _)| id)
            .collect();
        for id in late {
            self.automations.waiting.remove(&id);
            self.plugins.forget_automation(id);
            platform.answer_automation(
                id,
                platform::Answer::Error {
                    code: Code::Failed.number(),
                    message: LATE.to_owned(),
                },
            );
        }
        self.settle_automation_timer(platform);
    }

    /// Has the timer fire at the first deadline of the requests that wait,
    /// and none without one.
    fn settle_automation_timer(&mut self, platform: &mut dyn Platform) {
        let first = self.automations.waiting.values().min().copied();
        if let Some(timer) = self.automations.timer.take() {
            platform.cancel_timer(timer);
        }
        if let Some(at) = first {
            self.automations.timer = Some(platform.start_timer(at));
        }
    }
}

/// The plugin API's request of the platform's `request`.
fn request_of(request: platform::Request) -> AutomationRequest {
    match request {
        platform::Request::Workspace => AutomationRequest::Workspace,
        platform::Request::NewWindow {
            folder,
            command,
            environment,
        } => AutomationRequest::NewWindow {
            folder,
            command,
            environment: environment.into_iter().collect(),
        },
        platform::Request::NewTab {
            window,
            folder,
            command,
            environment,
        } => AutomationRequest::NewTab {
            window: window.map(WindowId),
            folder,
            command,
            environment: environment.into_iter().collect(),
        },
        platform::Request::Split {
            pane,
            edge,
            folder,
            command,
            environment,
        } => AutomationRequest::Split {
            pane: pane.map(PaneId),
            edge: match edge {
                platform::Edge::Right => AutomationEdge::Right,
                platform::Edge::Left => AutomationEdge::Left,
                platform::Edge::Down => AutomationEdge::Down,
                platform::Edge::Up => AutomationEdge::Up,
            },
            folder,
            command,
            environment: environment.into_iter().collect(),
        },
        platform::Request::SendText { pane, text } => AutomationRequest::SendText {
            pane: pane.map(PaneId),
            text,
        },
        platform::Request::SendKey { pane, keys } => AutomationRequest::SendKey {
            pane: pane.map(PaneId),
            keys,
        },
        platform::Request::RunLine { pane, line } => AutomationRequest::RunLine {
            pane: pane.map(PaneId),
            line,
        },
        platform::Request::SendMouse { pane, mouse } => AutomationRequest::SendMouse {
            pane: pane.map(PaneId),
            input: mouse_input(mouse),
        },
        platform::Request::Focus(target) => AutomationRequest::Focus(target_of(target)),
        platform::Request::Close(target) => AutomationRequest::Close(target_of(target)),
        platform::Request::Run { command, args } => AutomationRequest::Run {
            command,
            args: args
                .into_iter()
                .map(|(name, arg)| {
                    let value = match arg {
                        platform::Arg::Text(text) => Value::String(text),
                        platform::Arg::Integer(number) => Value::Integer(number),
                        platform::Arg::Bool(yes) => Value::Boolean(yes),
                    };
                    (name, value)
                })
                .collect::<Args>(),
        },
        platform::Request::PaneText { pane, selection } => AutomationRequest::PaneText {
            pane: pane.map(PaneId),
            selection,
        },
        platform::Request::SwitchSession { name } => AutomationRequest::SwitchSession { name },
        platform::Request::Commands => AutomationRequest::Commands,
    }
}

/// The plugin API's mouse event of the platform's `mouse`.
fn mouse_input(mouse: platform::Mouse) -> lokyr_kernel::plugins::host::MouseInput {
    use lokyr_kernel::plugins::host::{MouseButton, MouseDoes, MouseInput};
    MouseInput {
        action: match mouse.action {
            platform::MouseAction::Press => MouseDoes::Press,
            platform::MouseAction::Release => MouseDoes::Release,
            platform::MouseAction::Move => MouseDoes::Move,
            platform::MouseAction::ScrollUp => MouseDoes::ScrollUp,
            platform::MouseAction::ScrollDown => MouseDoes::ScrollDown,
            platform::MouseAction::ScrollLeft => MouseDoes::ScrollLeft,
            platform::MouseAction::ScrollRight => MouseDoes::ScrollRight,
        },
        button: mouse.button.map(|button| match button {
            platform::MouseButton::Left => MouseButton::Left,
            platform::MouseButton::Middle => MouseButton::Middle,
            platform::MouseButton::Right => MouseButton::Right,
        }),
        column: mouse.column,
        row: mouse.row,
        shift: mouse.shift,
        control: mouse.control,
        alt: mouse.alt,
        steps: mouse.steps,
    }
}

fn target_of(target: platform::Target) -> AutomationTarget {
    match target {
        platform::Target::Window(id) => AutomationTarget::Window(WindowId(id)),
        platform::Target::Tab(id) => AutomationTarget::Tab(TabId(id)),
        platform::Target::Pane(id) => AutomationTarget::Pane(PaneId(id)),
    }
}

/// The platform's answer of the plugin's `answer`.
fn answer_of(answer: AutomationAnswer) -> platform::Answer {
    match answer {
        AutomationAnswer::Done => platform::Answer::Done,
        AutomationAnswer::Opened { window, tab, pane } => platform::Answer::Opened {
            window: window.0,
            tab: tab.0,
            pane: pane.0,
        },
        AutomationAnswer::Workspace(windows) => platform::Answer::Workspace(
            windows
                .into_iter()
                .map(|window| platform::Window {
                    id: window.id.0,
                    title: window.title,
                    focused: window.focused,
                    tabs: window
                        .tabs
                        .into_iter()
                        .map(|tab| platform::Tab {
                            id: tab.id.0,
                            title: tab.title,
                            active: tab.active,
                            panes: tab
                                .panes
                                .into_iter()
                                .map(|pane| platform::Pane {
                                    id: pane.id.0,
                                    title: pane.title,
                                    folder: pane.folder,
                                    focused: pane.focused,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        ),
        AutomationAnswer::Text(text) => platform::Answer::Text(text),
        AutomationAnswer::Commands(commands) => platform::Answer::Commands(
            commands
                .into_iter()
                .map(|command| platform::Command {
                    name: command.name,
                    title: command.title,
                    args: command
                        .args
                        .into_iter()
                        .map(|(name, value)| {
                            let arg = match value {
                                Value::String(text) => platform::Arg::Text(text),
                                Value::Integer(number) => platform::Arg::Integer(number),
                                Value::Boolean(yes) => platform::Arg::Bool(yes),
                                Value::Number(number) => platform::Arg::Text(number.to_string()),
                            };
                            (name, arg)
                        })
                        .collect(),
                })
                .collect(),
        ),
        AutomationAnswer::Error { code, message } => platform::Answer::Error { code, message },
    }
}

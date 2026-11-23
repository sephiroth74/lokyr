//! The sessions in the app: the windows of the sessions that are not active
//! hidden, drawing nothing, and the active one's shown where they were, the
//! last of them to have had the keyboard taking it again; the Window menu's
//! Sessions, of a new session, the last two toggled, a session renamed and
//! closed, and saved as a layout, which opens as a new session; and a
//! session's name beside the window's number in its titlebar. The quick terminal's window, the settings' and a man page's
//! are of no session, and show whichever is active.

use lokyr_kernel::commands::{Args, Code, Error, Value};
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::layouts::{self, Layout};
use lokyr_kernel::state::windows::{self, Kept};
use lokyr_kernel::workspace::{self, Pane, SessionId};
use lokyr_platform_core::{Alert, AlertId, AlertStyle, Platform, WindowId};

use super::{App, LAST_SESSION};
use crate::drawing::Backend;

/// A question of the app's of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SessionAsk {
    /// Its name.
    Rename,
    /// Whether to close it, as a program runs in one of its windows.
    Close,
    /// The name of the layout that it is saved as.
    SaveLayout,
    /// Whether to replace the layout of a name, which it is saved as.
    ReplaceLayout(String),
}

impl<B: Backend> App<B> {
    /// Makes a session of no name, switches to it, the windows of the one
    /// before hidden, and opens its first window.
    pub(super) fn new_session(&mut self, platform: &mut dyn Platform) {
        if self.workspace.new_session("").is_none() {
            return;
        }
        self.open_window(platform);
    }

    /// Switches to the session used before the active one, if there is one.
    pub(super) fn switch_to_last_session(&mut self, platform: &mut dyn Platform) {
        if self.workspace.switch_to_last_session() {
            self.settle_sessions(platform);
        }
    }

    /// Switches to the session of `args`' `name`, the one used last of
    /// several; `not-found` of none of that name.
    pub(super) fn switch_session_named(
        &mut self,
        platform: &mut dyn Platform,
        args: &Args,
    ) -> Result<(), Error> {
        let Some(Value::String(name)) = args.get("name") else {
            return Err(Error::new(
                Code::MissingArgument,
                "a session's switch takes its `name`",
            ));
        };
        let found = self
            .workspace
            .sessions()
            .into_iter()
            .find(|session| session.name() == name.as_str())
            .map(workspace::Session::id);
        let Some(session) = found else {
            return Err(Error::new(
                Code::NotFound,
                format!("there is no session `{name}`"),
            ));
        };
        self.workspace.switch_session(session);
        self.settle_sessions(platform);
        Ok(())
    }

    /// Asks for the active session's name, on the window in front.
    pub(super) fn ask_session_name(&mut self, platform: &mut dyn Platform) {
        let session = self.workspace.active_session();
        if self
            .session_asks
            .values()
            .any(|(ask, asked, _)| *ask == SessionAsk::Rename && *asked == session)
        {
            return;
        }
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let name = self
            .workspace
            .session(session)
            .map(|shown| shown.name().to_owned())
            .unwrap_or_default();
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text("rename-session-message"),
            detail: self.texts.text("rename-session-detail"),
            buttons: vec![
                self.texts.text("rename-session-rename"),
                self.texts.text("rename-session-cancel"),
            ],
            suppression: None,
            input: Some(name),
        };
        let asked = platform.show_alert(Some(window), &alert);
        self.session_asks
            .insert(asked, (SessionAsk::Rename, session, window));
    }

    /// Closes the active session's windows, once the user confirms it while
    /// a program other than a shell runs in one of them, which closing
    /// ends; the session goes with its last window, unless it is the only
    /// one.
    pub(super) fn ask_close_session(&mut self, platform: &mut dyn Platform) {
        let session = self.workspace.active_session();
        if self
            .session_asks
            .values()
            .any(|(ask, asked, _)| *ask == SessionAsk::Close && *asked == session)
        {
            return;
        }
        let running = self
            .workspace
            .session_windows(session)
            .into_iter()
            .flat_map(|shown| self.workspace.panes_of(shown))
            .find_map(|pane| self.workspace.pane(pane).and_then(Pane::running));
        let Some(program) = running else {
            self.close_session(platform, session);
            return;
        };
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let program = self.program_name(program);
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("close-session-message"),
            detail: self
                .texts
                .text_with("close-session-detail", &[("program", Arg::Text(&program))]),
            buttons: vec![
                self.texts.text("close-session-close"),
                self.texts.text("close-session-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let asked = platform.show_alert(Some(window), &alert);
        self.session_asks
            .insert(asked, (SessionAsk::Close, session, window));
    }

    /// Closes `session`'s windows, as their own closes do, and the session,
    /// which goes with its last window, unless it is the only one.
    fn close_session(&mut self, platform: &mut dyn Platform, session: SessionId) {
        // Of windows kept that never showed, nothing is left.
        self.pending_windows.remove(&session);
        let windows = self.windows_of_session(session);
        for window in windows {
            self.close(platform, window);
        }
        // Of no window to close, it goes all the same.
        self.workspace.close_session(session);
        self.settle_sessions(platform);
    }

    /// Takes the answer to a question of a session's, if `alert` is one:
    /// its `button`, from the first, and the name given.
    pub(super) fn session_answered(
        &mut self,
        platform: &mut dyn Platform,
        alert: AlertId,
        button: usize,
        text: Option<&str>,
    ) {
        let Some((ask, session, window)) = self.session_asks.remove(&alert) else {
            return;
        };
        if button != 0 {
            return;
        }
        match ask {
            SessionAsk::Rename => {
                let name = text.map(str::trim).unwrap_or_default();
                if self.workspace.rename_session(session, name) {
                    self.show_session_names(platform);
                }
            }
            SessionAsk::Close => self.close_session(platform, session),
            SessionAsk::SaveLayout => {
                let name = text.map(str::trim).unwrap_or_default().to_owned();
                self.layout_named(platform, (session, window), name);
            }
            SessionAsk::ReplaceLayout(name) => self.save_layout(platform, session, &name),
        }
    }

    /// Asks for the name of the layout that the active session is saved
    /// as, on the window in front, of the session's name at first.
    pub(super) fn ask_layout_name(&mut self, platform: &mut dyn Platform) {
        let session = self.workspace.active_session();
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let name = self
            .workspace
            .session(session)
            .map(|shown| shown.name().to_owned())
            .unwrap_or_default();
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text("save-layout-message"),
            detail: self.texts.text("save-layout-detail"),
            buttons: vec![
                self.texts.text("save-layout-save"),
                self.texts.text("save-layout-cancel"),
            ],
            suppression: None,
            input: Some(name),
        };
        let asked = platform.show_alert(Some(window), &alert);
        self.session_asks
            .insert(asked, (SessionAsk::SaveLayout, session, window));
    }

    /// Saves `session` as the layout `name`, once the user confirms that it
    /// replaces the one of that name there; a name that is no file's is
    /// told.
    fn layout_named(
        &mut self,
        platform: &mut dyn Platform,
        (session, window): (SessionId, WindowId),
        name: String,
    ) {
        if let Err(error) = layout_name(&name) {
            self.report(platform, Diagnostic::new(Code::Failed, error, About::Other));
            return;
        }
        let Some(folder) = self.layouts_folder() else {
            return;
        };
        if !layouts::path(&folder, &name).exists() {
            self.save_layout(platform, session, &name);
            return;
        }
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self
                .texts
                .text_with("replace-layout-message", &[("name", Arg::Text(&name))]),
            detail: self.texts.text("replace-layout-detail"),
            buttons: vec![
                self.texts.text("replace-layout-replace"),
                self.texts.text("replace-layout-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let asked = platform.show_alert(Some(window), &alert);
        self.session_asks
            .insert(asked, (SessionAsk::ReplaceLayout(name), session, window));
    }

    /// Writes `session`'s windows as the layout `name`: their frames, tabs,
    /// splits and each pane's folder, of `~` for the home directory.
    fn save_layout(&mut self, platform: &mut dyn Platform, session: SessionId, name: &str) {
        let Some(folder) = self.layouts_folder() else {
            return;
        };
        let host = lokyr_pty::host_name();
        let home = std::env::home_dir();
        let shown: Vec<(workspace::WindowId, Option<windows::Frame>)> = self
            .windows_of_session(session)
            .into_iter()
            .filter_map(|window| {
                let open = self.windows.get(&window)?;
                let frame = platform.windowed_frame(window).map(|frame| windows::Frame {
                    x: frame.x,
                    y: frame.y,
                    width: frame.width,
                    height: frame.height,
                });
                Some((open.window, frame))
            })
            .collect();
        let kept = Kept::of(
            &self.workspace,
            &shown,
            None,
            |pane| {
                let path = pane
                    .told_directory(&host)
                    .or_else(|| pane.working_directory())?;
                // Of `~` for the home directory, as a layout goes from one
                // account to another.
                Some(
                    match home
                        .as_deref()
                        .and_then(|home| path.strip_prefix(home).ok())
                    {
                        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
                        Some(rest) => std::path::Path::new("~").join(rest),
                        None => path,
                    },
                )
            },
            |_, _| None,
        );
        let session_folder = self
            .workspace
            .session(session)
            .and_then(workspace::Session::folder);
        let layout = Layout::of(&kept.windows, session_folder);
        let path = layouts::path(&folder, name);
        if let Err(error) = layout.write(&path) {
            let text = format!(
                "the session is not saved as the layout `{name}`, as {} cannot be written: {error}",
                path.display()
            );
            self.report(platform, Diagnostic::new(Code::Failed, text, About::Other));
        }
    }

    /// Opens the layout of `args`' `name` as a new session of its name, of
    /// new programs, its panes' commands run in their shells as they start;
    /// a layout that does not read is told, of its file and its line.
    pub(super) fn open_layout(
        &mut self,
        platform: &mut dyn Platform,
        args: &Args,
    ) -> Result<(), Error> {
        let Some(Value::String(name)) = args.get("name") else {
            return Err(Error::new(
                Code::MissingArgument,
                "a layout's opening takes its `name`",
            ));
        };
        layout_name(name).map_err(|error| Error::new(Code::Failed, error))?;
        let Some(folder) = self.layouts_folder() else {
            return Err(Error::new(Code::NotFound, "there is no folder of layouts"));
        };
        let path = layouts::path(&folder, name);
        let layout = Layout::read(&path).map_err(|error| {
            Error::new(
                Code::Failed,
                format!(
                    "the layout `{name}` does not open: {}: {error}",
                    path.display()
                ),
            )
        })?;
        let Some(session) = self.workspace.new_session(name) else {
            return Ok(());
        };
        let folder = layout
            .folder
            .as_ref()
            .map(|folder| lokyr_kernel::plugins::host::home_expanded(&folder.0));
        self.workspace.set_session_folder(session, folder);
        // The windows of the session left hide first.
        self.settle_sessions(platform);
        let opened = layout
            .windows()
            .iter()
            .filter_map(|window| self.restore_window(platform, window))
            .count();
        if opened == 0 {
            self.workspace.close_session(session);
        }
        self.settle_sessions(platform);
        Ok(())
    }

    /// The folder of the layouts, beside the config file, if the app has
    /// one.
    fn layouts_folder(&self) -> Option<std::path::PathBuf> {
        self.config_file
            .as_ref()
            .map(|file| layouts::folder(&file.path))
    }

    /// Whether a question of a session's is open on `window`.
    pub(super) fn asks_session_on(&self, window: WindowId) -> bool {
        self.session_asks
            .values()
            .any(|(_, _, asked)| *asked == window)
    }

    /// Forgets the questions of sessions on `window`, which closes.
    pub(super) fn forget_session_asks(&mut self, window: WindowId) {
        self.session_asks
            .retain(|_, (_, _, asked)| *asked != window);
        self.session_fronts.retain(|_, front| *front != window);
    }

    /// The name of the session of the workspace's `window`, as its titlebar
    /// shows it beside the window's number: none of a session of no name,
    /// nor of a window of none.
    pub(super) fn session_name_of(&self, window: workspace::WindowId) -> Option<&str> {
        let session = self.workspace.window(window)?.session()?;
        Some(self.workspace.session(session)?.name()).filter(|name| !name.is_empty())
    }

    /// The platform's windows of `session`, in the order they opened.
    fn windows_of_session(&self, session: SessionId) -> Vec<WindowId> {
        let shown = self.workspace.session_windows(session);
        let mut windows: Vec<(usize, WindowId)> = self
            .windows
            .iter()
            .filter_map(|(&window, state)| {
                let at = shown.iter().position(|&id| id == state.window)?;
                Some((at, window))
            })
            .collect();
        windows.sort_unstable();
        windows.into_iter().map(|(_, window)| window).collect()
    }

    /// Follows the keyboard's focus that `window` took: it is its session's
    /// window in front; one of a session that is not active, as a
    /// notification's click or an agent's key brings it forward, switches
    /// to its session. A focus that a window took for a moment, which it
    /// no longer has, as the system gives it to another window of a session
    /// that hides, one at a time, tells nothing.
    pub(super) fn session_focused(&mut self, platform: &dyn Platform, window: WindowId) {
        if platform.focused_window() != Some(window) {
            return;
        }
        let Some(session) = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.window(state.window))
            .and_then(workspace::Window::session)
        else {
            return;
        };
        self.session_fronts.insert(session, window);
        if session != self.workspace.active_session() {
            self.workspace.switch_session(session);
        }
    }

    /// Shows the windows of the active session, and hides those of the
    /// others, as the workspace's sessions say, at a turn's end: the others'
    /// hidden first, as the system shows a window in full screen too while
    /// one of the app's with the keyboard is, which the platform takes out
    /// of it before it hides it; then the active one's shown, where they
    /// were, the last of them to have had the keyboard taking it again, as
    /// a switch brings them. The keyboard that the system gives a window of
    /// the others for a moment on the way tells nothing, as the window no
    /// longer has it as its event comes. Hidden, a window draws nothing.
    /// Nothing changes of no switch, nor of a window that opened in the
    /// session where it shows.
    pub(super) fn settle_sessions(&mut self, platform: &mut dyn Platform) {
        let active = self.workspace.active_session();
        // A session that has not shown since the app started opens its
        // windows, as they were kept, their shells starting now.
        if let Some(pending) = self.pending_windows.remove(&active) {
            for window in &pending {
                self.restore_window(platform, window);
            }
        }
        // The session that the app starts in shows of its own.
        let switched = self.shown_session.is_some_and(|shown| shown != active);
        let mut shown = Vec::new();
        let mut hidden = Vec::new();
        for (&window, state) in &self.windows {
            let shows = self.workspace.in_active_session(state.window);
            if shows == state.session_hidden {
                if shows {
                    shown.push(window);
                } else {
                    hidden.push(window);
                }
            }
        }
        if !switched && shown.is_empty() && hidden.is_empty() && self.shown_session.is_some() {
            return;
        }
        self.shown_session = Some(active);
        for &window in &hidden {
            platform.set_window_hidden(window, true);
            if let Some(state) = self.windows.get_mut(&window) {
                state.session_hidden = true;
            }
        }
        for &window in &shown {
            platform.set_window_hidden(window, false);
            if let Some(state) = self.windows.get_mut(&window) {
                state.session_hidden = false;
            }
        }
        if switched {
            let windows = self.windows_of_session(active);
            let front = self
                .session_fronts
                .get(&active)
                .copied()
                .filter(|front| windows.contains(front))
                .or_else(|| windows.last().copied());
            if let Some(front) = front {
                platform.focus_window(front);
            }
        }
        platform.set_command_enabled(LAST_SESSION, self.workspace.last_session().is_some());
        self.show_session_names(platform);
    }

    /// Shows each window's session's name beside its number, as a session
    /// is renamed or another shows.
    fn show_session_names(&mut self, platform: &mut dyn Platform) {
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        for window in windows {
            self.show_window_keys(platform, window);
        }
    }
}

/// Whether `name` is a layout's: a file's name, of no dot first, whose
/// file the layouts' folder holds of `.toml`; why not otherwise.
fn layout_name(name: &str) -> Result<(), String> {
    lokyr_kernel::plugins::host::check_file_name(name).map_err(|error| error.message)?;
    if name.starts_with('.') {
        return Err("a layout's name does not start with a dot".to_owned());
    }
    Ok(())
}

//! The questions that the app asks before it acts: closing a window whose
//! program runs, quitting, a window's title, a paste of several lines, a
//! link to open, a program's read of the clipboard; and their answers.

use lokyr_kernel::commands::Code;
use lokyr_kernel::config::{ConfirmPaste, OpenFile, OpenUrl};
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::plugins::Closing;
use lokyr_kernel::workspace::{self, Pane, PaneId};
use lokyr_kernel::{config, paste};
use lokyr_platform_core::{
    Alert, AlertId, AlertStyle, Platform, SavePanelId, TargetKind, WindowId,
};
use lokyr_vt_core::terminal::ClipboardRequest;

use super::{App, PREVIEW};
use crate::drawing::Backend;

/// A save that a plugin asked, whose panel shows: the window of the
/// workspace that shows it, the plugin, the question's id and the file's
/// bytes.
pub(super) struct Save {
    pub(super) window: workspace::WindowId,
    pub(super) plugin: String,
    id: u64,
    bytes: Vec<u8>,
}

/// A save's file that its thread wrote where the user chose, or failed to:
/// the plugin, the question's id, the path, and the error of a write that
/// failed.
#[derive(Debug)]
pub(super) struct Written {
    plugin: String,
    id: u64,
    path: std::path::PathBuf,
    error: Option<String>,
}

impl std::fmt::Debug for Save {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Save")
            .field("window", &self.window)
            .field("plugin", &self.plugin)
            .field("id", &self.id)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

impl<B: Backend> App<B> {
    /// The answer to the question of `alert`: its `button`, from the first,
    /// whether the user asked that it be not asked again, and the text
    /// given. A program's read of the clipboard: its second button allows
    /// it.
    pub(super) fn alert_closed(
        &mut self,
        platform: &mut dyn Platform,
        alert: AlertId,
        (button, suppressed): (usize, bool),
        text: Option<&str>,
    ) {
        if let Some(ask) = self.theme_asks.remove(&alert) {
            self.theme_answered(platform, &ask, (button, suppressed));
        }
        if let Some(script) = self.script_asks.remove(&alert) {
            self.script_answered(platform, &script, button);
        }
        if let Some((asked, links)) = self.tools_ask.take() {
            if asked == alert {
                self.tools_answered(platform, links, button);
            } else {
                self.tools_ask = Some((asked, links));
            }
        }
        if let Some((pane, request)) = self.clipboard_asks.remove(&alert)
            && button == 1
            && let Some(pane) = self.workspace.pane(pane)
        {
            pane.reply_clipboard(request, &platform.clipboard_text().unwrap_or_default());
        }
        if let Some(window) = self.close_asks.remove(&alert)
            && button == 0
        {
            self.close(platform, window);
        }
        if let Some((window, closing)) = self.part_asks.remove(&alert)
            && button == 0
            && self.part_in(window, closing)
        {
            self.close_part(platform, window, closing);
        }
        // The capabilities granted to the plugin that activates, or refused.
        if let Some(asked) = self.consent_asks.remove(&alert) {
            self.consent_answered(platform, asked, button == 1);
        }
        // The text given, to the plugin that asked, or none.
        if let Some((_, plugin, id)) = self.text_asks.remove(&alert) {
            let text = (button == 0).then(|| text.unwrap_or_default().to_owned());
            self.plugins.answer(&plugin, id, text);
        }
        // The title given, or the window's own, for none.
        if let Some(window) = self.title_asks.remove(&alert)
            && button == 0
            && let Some(state) = self.windows.get_mut(&window)
        {
            state.user_title = text
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_owned);
            self.show_title(platform, window);
        }
        if self.quit_ask == Some(alert) {
            self.quit_ask = None;
            let system = std::mem::take(&mut self.system_quit);
            if button == 0 {
                self.quit(platform);
            } else if system {
                platform.refuse_quit();
            }
        }
        if let Some((_, asked)) = self.open_asks.remove(&alert) {
            self.answer_open(platform, asked, button, suppressed);
        }
        // Paste, as one line, or cancel.
        if let Some((pane, text)) = self.paste_asks.remove(&alert)
            && button < 2
        {
            if let Some(shown) = self.workspace.pane(pane) {
                match button {
                    0 => shown.paste(&text),
                    _ => shown.paste(&paste::one_line(&text)),
                }
                self.workspace.typed(pane);
            }
            if suppressed {
                self.stop_confirming_pastes();
            }
        }
    }

    /// Whether a question of the app's is open on `window`, or a plugin's
    /// save panel, which takes the keyboard as a sheet of the window.
    pub(super) fn asks_on(&self, window: WindowId) -> bool {
        let pane = self.focused(window);
        let shown = self.windows.get(&window).map(|state| state.window);
        self.saves.values().any(|save| Some(save.window) == shown)
            || self.close_asks.values().any(|&asked| asked == window)
            || self.title_asks.values().any(|&asked| asked == window)
            || self.part_asks.values().any(|(asked, _)| *asked == window)
            || self.text_asks.values().any(|(asked, ..)| *asked == window)
            || self
                .open_asks
                .values()
                .any(|(asked, _)| *asked == Some(window))
            || self
                .clipboard_asks
                .values()
                .any(|(asked, _)| Some(*asked) == pane)
            || self
                .paste_asks
                .values()
                .any(|(asked, _)| Some(*asked) == pane)
    }

    /// Asks the user, with an alert on its window, whether `pane`'s
    /// program can read the clipboard, as `request` asks: the default
    /// button refuses. A read that comes while the pane's ask is open gets
    /// no answer.
    pub(super) fn ask_clipboard_read(
        &mut self,
        platform: &mut dyn Platform,
        pane: PaneId,
        request: ClipboardRequest,
    ) {
        if self
            .clipboard_asks
            .values()
            .any(|(asking, _)| *asking == pane)
        {
            return;
        }
        let Some(window) = self.window_of(pane) else {
            return;
        };
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("clipboard-read-message"),
            detail: self.texts.text("clipboard-read-detail"),
            buttons: vec![
                self.texts.text("clipboard-read-deny"),
                self.texts.text("clipboard-read-allow"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.clipboard_asks.insert(alert, (pane, request));
    }

    /// Pastes `text` into `window`'s program as a paste of the clipboard
    /// does: bracketed where the program asks, and asking first where
    /// `confirm-paste` says so.
    pub(super) fn paste_into(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        text: String,
    ) {
        let Some(id) = self.focused(window) else {
            return;
        };
        let Some(pane) = self.workspace.pane(id) else {
            return;
        };
        let bracketed = pane
            .input_modes()
            .is_some_and(|modes| modes.bracketed_paste);
        let confirm = if self.pastes_unconfirmed {
            ConfirmPaste::Never
        } else {
            self.config.clipboard.confirm_paste
        };
        match paste::decide(&text, bracketed, confirm) {
            paste::Paste::Now(text) => {
                pane.paste(&text);
                self.workspace.typed(id);
            }
            paste::Paste::Ask => self.ask_paste(platform, id, text),
        }
    }

    /// Asks the user to confirm a paste of `text` into `pane`, whose
    /// program takes its line breaks as Return: an alert in its window,
    /// with the text's first lines as they would reach the program. A
    /// paste while one waits for its answer goes.
    pub(super) fn ask_paste(&mut self, platform: &mut dyn Platform, pane: PaneId, text: String) {
        if self.paste_asks.values().any(|(asking, _)| *asking == pane) {
            return;
        }
        let Some(window) = self.window_of(pane) else {
            return;
        };
        let lines = i64::try_from(paste::lines(&text)).unwrap_or(i64::MAX);
        let (most, width) = PREVIEW;
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self
                .texts
                .text_with("paste-lines-message", &[("lines", Arg::Number(lines))]),
            detail: format!(
                "{}\n\n{}",
                self.texts.text("paste-lines-detail"),
                paste::preview(&text, most, width)
            ),
            buttons: vec![
                self.texts.text("paste-lines-paste"),
                self.texts.text("paste-lines-one-line"),
                self.texts.text("paste-lines-cancel"),
            ],
            suppression: Some(self.texts.text("paste-lines-dont-ask")),
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.paste_asks.insert(alert, (pane, text));
    }

    /// Opens `target`, a URL or an absolute path, as a plugin asked, and as
    /// [`opening`] decides of what the platform tells it is: a URL or a
    /// file asks the user first, by default, showing it in full; a
    /// directory opens, and a program shows in the file manager, never
    /// running.
    pub(super) fn open_target(&mut self, platform: &mut dyn Platform, target: String) {
        let resolved = match platform.resolve_target(&target) {
            Ok(resolved) => resolved,
            Err(error) => {
                eprintln!("lokyr: {error}");
                return;
            }
        };
        let done = match opening(resolved.kind, self.links()) {
            Opening::Ask => {
                self.ask_open(platform, target, resolved.kind);
                return;
            }
            Opening::Reveal => platform.reveal(&resolved),
            Opening::Open => platform.open(&resolved),
        };
        if let Err(error) = done {
            eprintln!("lokyr: {error}");
        }
    }

    /// What links do: the config's `[links]`, or what the user chose where
    /// the config could not keep it.
    fn links(&self) -> (OpenUrl, OpenFile) {
        let (urls, files) = self.links_chosen;
        (
            urls.unwrap_or(self.config.links.urls),
            files.unwrap_or(self.config.links.files),
        )
    }

    /// Asks the user whether to open `target`, a URL or a file, of `kind`,
    /// on the window that the commands run in, or the one that has the
    /// focus: a file can show in the file manager instead.
    fn ask_open(&mut self, platform: &mut dyn Platform, target: String, kind: TargetKind) {
        let file = kind == TargetKind::File;
        let (message, dont_ask) = if file {
            ("open-file-message", "open-file-dont-ask")
        } else {
            ("open-url-message", "open-url-dont-ask")
        };
        let mut buttons = vec![self.texts.text("open-open")];
        if file {
            buttons.push(self.texts.text("open-reveal"));
        }
        buttons.push(self.texts.text("open-cancel"));
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text(message),
            detail: shown(&target),
            buttons,
            suppression: Some(self.texts.text(dont_ask)),
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        let alert = platform.show_alert(window, &alert);
        self.open_asks.insert(alert, (window, (target, kind)));
    }

    /// The user answered the question before opening `target`, of `kind`,
    /// with `button`, its checkbox ticked when `suppressed`: it opens, or
    /// shows in the file manager, and, ticked, the config keeps the answer.
    /// What the target is is asked again, as the question's time can have
    /// changed it: one that became a program shows in the file manager,
    /// whatever the answer.
    fn answer_open(
        &mut self,
        platform: &mut dyn Platform,
        (target, kind): (String, TargetKind),
        button: usize,
        suppressed: bool,
    ) {
        let chosen = match (kind, button) {
            (TargetKind::File, 0) => Chosen::File(OpenFile::Open),
            (TargetKind::File, 1) => Chosen::File(OpenFile::Reveal),
            (_, 0) => Chosen::Url(OpenUrl::Open),
            _ => return,
        };
        let done = platform.resolve_target(&target).and_then(|resolved| {
            if resolved.kind == TargetKind::Program || chosen == Chosen::File(OpenFile::Reveal) {
                platform.reveal(&resolved)
            } else {
                platform.open(&resolved)
            }
        });
        if let Err(error) = done {
            eprintln!("lokyr: {error}");
        }
        if suppressed {
            self.remember_opening(chosen);
        }
    }

    /// Keeps `chosen`, what links do, as the user answered without wanting
    /// to be asked again: the config file takes it, or, if it cannot, it
    /// lasts until the app quits.
    fn remember_opening(&mut self, chosen: Chosen) {
        let (key, value) = match chosen {
            Chosen::Url(urls) => {
                self.config.links.urls = urls;
                ("urls", "open")
            }
            Chosen::File(files) => {
                self.config.links.files = files;
                (
                    "files",
                    if files == OpenFile::Reveal {
                        "reveal"
                    } else {
                        "open"
                    },
                )
            }
        };
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "links", key, value)
                .map_err(|error| error.to_string()),
            None => Err("there is no config file, without a home directory".to_owned()),
        };
        if let Err(error) = written {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: links {key} {value} until Lokyr quits");
            match chosen {
                Chosen::Url(urls) => self.links_chosen.0 = Some(urls),
                Chosen::File(files) => self.links_chosen.1 = Some(files),
            }
        }
    }

    /// Asks the user for a title of `window`, which takes the place of its
    /// program's and of its own, in a field that holds the one that the
    /// user gave before; an empty one gives the window's own back. A window
    /// that waits for the answer asks nothing more.
    pub(super) fn ask_title(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.title_asks.values().any(|&asked| asked == window) {
            return;
        }
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text("set-title-message"),
            detail: self.texts.text("set-title-detail"),
            buttons: vec![
                self.texts.text("set-title-set"),
                self.texts.text("set-title-cancel"),
            ],
            suppression: None,
            input: Some(state.user_title.clone().unwrap_or_default()),
        };
        let asked = platform.show_alert(Some(window), &alert);
        self.title_asks.insert(asked, window);
    }

    /// Closes `window`, once the user confirms it while a program other
    /// than a shell runs in it, which closing ends; a window that waits for
    /// the answer asks nothing more.
    pub(super) fn ask_close(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.close_asks.values().any(|&asked| asked == window) {
            return;
        }
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let running = self
            .workspace
            .panes_of(state.window)
            .into_iter()
            .find_map(|pane| self.workspace.pane(pane).and_then(Pane::running));
        let Some(program) = running else {
            self.close(platform, window);
            return;
        };
        let program = self.program_name(program);
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("close-running-message"),
            detail: self
                .texts
                .text_with("close-running-detail", &[("program", Arg::Text(&program))]),
            buttons: vec![
                self.texts.text("close-running-close"),
                self.texts.text("close-running-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.close_asks.insert(alert, window);
    }

    /// Closes `closing`, a tab or a pane that a plugin closes, once the
    /// user confirms it while a program other than a shell runs there,
    /// which closing ends, as for a window, and as the window's close
    /// asks, of the window's last; one that waits for the answer asks
    /// nothing more.
    pub(super) fn ask_close_part(&mut self, platform: &mut dyn Platform, closing: Closing) {
        if self.part_asks.values().any(|(_, asked)| *asked == closing) {
            return;
        }
        let panes = match closing {
            Closing::Tab(tab) => self
                .workspace
                .tab(tab)
                .map(|tab| tab.layout().panes())
                .unwrap_or_default(),
            Closing::Pane(pane) => vec![pane],
        };
        let Some(window) = panes.first().and_then(|&pane| self.window_of(pane)) else {
            return;
        };
        // The window's last tab or pane closes the window, which asks as
        // the window's close does.
        let last = self
            .windows
            .get(&window)
            .is_some_and(|state| self.workspace.panes_of(state.window).len() <= panes.len());
        if last {
            self.ask_close(platform, window);
            return;
        }
        let running = panes
            .iter()
            .find_map(|&pane| self.workspace.pane(pane).and_then(Pane::running));
        let Some(program) = running else {
            self.close_part(platform, window, closing);
            return;
        };
        let program = self.program_name(program);
        let (message, detail) = match closing {
            Closing::Tab(_) => ("close-tab-running-message", "close-tab-running-detail"),
            Closing::Pane(_) => ("close-pane-running-message", "close-pane-running-detail"),
        };
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text(message),
            detail: self
                .texts
                .text_with(detail, &[("program", Arg::Text(&program))]),
            buttons: vec![
                self.texts.text("close-running-close"),
                self.texts.text("close-running-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let alert = platform.show_alert(Some(window), &alert);
        self.part_asks.insert(alert, (window, closing));
    }

    /// Whether `closing`, a tab or a pane, is still open in `window`, as
    /// its question asked of it: a program that ends by itself closes its
    /// own, and a plugin moves one to another window.
    fn part_in(&self, window: WindowId, closing: Closing) -> bool {
        let pane = match closing {
            Closing::Pane(pane) => Some(pane),
            Closing::Tab(tab) => self
                .workspace
                .tab(tab)
                .and_then(|tab| tab.layout().panes().first().copied()),
        };
        pane.and_then(|pane| self.window_of(pane)) == Some(window)
    }

    /// Withdraws the questions of the close of a tab or a pane that is no
    /// longer in its window, which its answer would close nothing of.
    pub(super) fn drop_gone_part_asks(&mut self, platform: &mut dyn Platform) {
        let gone: Vec<AlertId> = self
            .part_asks
            .iter()
            .filter(|(_, (window, closing))| !self.part_in(*window, *closing))
            .map(|(&alert, _)| alert)
            .collect();
        for alert in gone {
            self.part_asks.remove(&alert);
            platform.dismiss_alert(alert);
        }
    }

    /// Closes `closing`, of `window`'s: a tab, the window with its last; a
    /// pane, as the app closes one.
    pub(super) fn close_part(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        closing: Closing,
    ) {
        match closing {
            Closing::Pane(pane) => self.close_pane(platform, window, pane),
            Closing::Tab(tab) => {
                let Some(shown) = self.windows.get(&window).map(|state| state.window) else {
                    return;
                };
                if self
                    .workspace
                    .window(shown)
                    .is_none_or(|shown| shown.tabs().len() <= 1)
                {
                    self.close(platform, window);
                    return;
                }
                let panes = self
                    .workspace
                    .tab(tab)
                    .map(|tab| tab.layout().panes())
                    .unwrap_or_default();
                self.forget_panes(&panes);
                self.workspace.close_tab(tab);
                self.keyboard_moved(platform, window);
                self.show_title(platform, window);
                platform.request_frame(window);
            }
        }
    }

    /// Asks the user for a text, of `alert`, for `plugin`'s question `id`,
    /// on the platform's window that shows `window`, or on the focused
    /// one: without a window, the answer is none.
    pub(super) fn ask_text(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, id): (String, u64),
        window: Option<workspace::WindowId>,
        alert: &Alert,
    ) {
        let shown = match window {
            Some(window) => self
                .windows
                .iter()
                .find(|(_, state)| state.window == window)
                .map(|(&shown, _)| shown),
            None => platform
                .focused_window()
                .filter(|focused| self.windows.contains_key(focused)),
        };
        let Some(shown) = shown else {
            self.plugins.answer(&plugin, id, None);
            return;
        };
        let asked = platform.show_alert(Some(shown), alert);
        self.text_asks.insert(asked, (shown, plugin, id));
    }

    /// Shows the save panel of `plugin`'s save `id` on the platform's window
    /// that shows `window`, of `name` suggested: without one, the save ends
    /// of none, as a panel that the user cancelled.
    pub(super) fn ask_save(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, id): (String, u64),
        window: workspace::WindowId,
        name: &str,
        bytes: Vec<u8>,
    ) {
        let shown = self
            .windows
            .iter()
            .find(|(_, state)| state.window == window)
            .map(|(&shown, _)| shown);
        let Some(shown) = shown else {
            self.plugins.saved(&plugin, id, None, None);
            return;
        };
        // A window shows one panel at a time, though two plugins' threads
        // asked for one each in a turn.
        if self.saves.values().any(|save| save.window == window) {
            let error = lokyr_kernel::plugins::SAVING.to_owned();
            self.plugins.saved(&plugin, id, None, Some(error));
            return;
        }
        let message = self
            .texts
            .text_with("save-panel-message", &[("name", Arg::Text(&plugin))]);
        let panel = platform.show_save_panel(shown, name, &message);
        self.saves.insert(
            panel,
            Save {
                window,
                plugin,
                id,
                bytes,
            },
        );
    }

    /// Writes the file of the save whose panel `panel` closed where the
    /// user chose, `path`, whole, on a thread of its own, marked as come
    /// from elsewhere, as a plugin chose its bytes and its type, so that
    /// the system asks before it opens as a program; its end comes back as
    /// [`App::take_written`] takes it. Of none, its plugin hears that the
    /// user cancelled.
    pub(super) fn save_panel_closed(
        &mut self,
        platform: &mut dyn Platform,
        panel: SavePanelId,
        path: Option<std::path::PathBuf>,
    ) {
        let Some(save) = self.saves.remove(&panel) else {
            return;
        };
        let Some(path) = path else {
            self.plugins.saved(&save.plugin, save.id, None, None);
            return;
        };
        let (written, waker, quarantine) = (
            std::sync::Arc::clone(&self.written),
            platform.waker(),
            platform.quarantine(),
        );
        std::thread::spawn(move || {
            let error = lokyr_pkg_model::write_whole_marked(&path, &save.bytes, quarantine)
                .err()
                .map(|error| error.to_string());
            written
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(Written {
                    plugin: save.plugin,
                    id: save.id,
                    path,
                    error,
                });
            waker.wake();
        });
    }

    /// Tells each plugin the end of its saves that their threads wrote: the
    /// path, or the error of a write that failed, which the user hears in a
    /// notice too, as the plugin may tell nothing of it.
    pub(super) fn take_written(&mut self, platform: &mut dyn Platform) {
        let written = std::mem::take(
            &mut *self
                .written
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for written in written {
            match written.error {
                None => {
                    let path = written.path.to_string_lossy().into_owned();
                    self.plugins
                        .saved(&written.plugin, written.id, Some(path), None);
                }
                Some(error) => {
                    let shown = written.path.display().to_string();
                    let told = self.texts.text_with(
                        "save-failed",
                        &[("path", Arg::Text(&shown)), ("error", Arg::Text(&error))],
                    );
                    self.report(platform, Diagnostic::new(Code::Failed, told, About::Other));
                    let error = format!("{shown} cannot be written: {error}");
                    self.plugins
                        .saved(&written.plugin, written.id, None, Some(error));
                }
            }
        }
    }

    /// Quits, once the user confirms it while programs other than shells
    /// run, which quitting ends: the system's request, when `system`, waits
    /// for the answer, and gives up when the user cancels. A question that
    /// waits for its answer takes the request too.
    pub(super) fn ask_quit(&mut self, platform: &mut dyn Platform, system: bool) {
        self.system_quit |= system;
        if self.quit_ask.is_some() {
            return;
        }
        let mut programs = Vec::new();
        let mut windows = 0;
        for state in self.windows.values() {
            let mut running = false;
            for pane in self.workspace.panes_of(state.window) {
                if let Some(program) = self.workspace.pane(pane).and_then(Pane::running) {
                    running = true;
                    let program = self.program_name(program);
                    if !programs.contains(&program) {
                        programs.push(program);
                    }
                }
            }
            windows += i64::from(running);
        }
        if windows == 0 {
            self.system_quit = false;
            self.quit(platform);
            return;
        }
        let programs = programs.join(", ");
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("quit-running-message"),
            detail: self.texts.text_with(
                "quit-running-detail",
                &[
                    ("windows", Arg::Number(windows)),
                    ("programs", Arg::Text(&programs)),
                ],
            ),
            buttons: vec![
                self.texts.text("quit-running-quit"),
                self.texts.text("quit-running-cancel"),
            ],
            suppression: None,
            input: None,
        };
        self.quit_ask = Some(platform.show_alert(None, &alert));
    }

    /// The name of a program that runs, or "a program" for one whose name
    /// could not be read.
    pub(super) fn program_name(&self, program: String) -> String {
        if program.is_empty() {
            self.texts.text("program-unknown")
        } else {
            program
        }
    }

    /// Pastes no longer ask, as the user said with the question's
    /// checkbox: the config file takes `confirm-paste = "never"`, or, if it
    /// cannot, as the log tells, the app keeps it until it quits.
    fn stop_confirming_pastes(&mut self) {
        self.config.clipboard.confirm_paste = ConfirmPaste::Never;
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "clipboard", "confirm-paste", "never")
                .map_err(|error| error.to_string()),
            None => Err("there is no config file, without a home directory".to_owned()),
        };
        if let Err(error) = written {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: pastes are not confirmed until Lokyr quits");
            self.pastes_unconfirmed = true;
        }
    }
}

/// `target` as a question shows it: each character that shows nothing,
/// or that turns the text's direction, as `\u{202E}`, so that the target
/// reads as it is.
pub(super) fn shown(target: &str) -> String {
    let mut shown = String::with_capacity(target.len());
    for c in target.chars() {
        if invisible(c) {
            shown.push_str(&format!("\\u{{{:X}}}", u32::from(c)));
        } else {
            shown.push(c);
        }
    }
    shown
}

/// Whether `c` shows nothing, or turns the text's direction: the controls,
/// the separators of lines and paragraphs, and Unicode's characters that
/// are ignorable by default, as the bidirectional overrides and isolates,
/// the zero-width ones, the variation selectors and the tags.
pub(super) fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            u32::from(c),
            0xAD | 0x34F
                | 0x61C
                | 0x115F..=0x1160
                | 0x17B4..=0x17B5
                | 0x180B..=0x180F
                | 0x200B..=0x200F
                | 0x2028..=0x202E
                | 0x2060..=0x206F
                | 0x3164
                | 0xFE00..=0xFE0F
                | 0xFEFF
                | 0xFFA0
                | 0xFFF9..=0xFFFB
                | 0x1BCA0..=0x1BCA3
                | 0x1D173..=0x1D17A
                | 0xE0000..=0xE0FFF
        )
}

/// What the user chose that links do, as the config writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chosen {
    /// What a URL does.
    Url(OpenUrl),
    /// What a file does.
    File(OpenFile),
}

/// What the app does with a target that a plugin opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Opening {
    /// Opens it with the system's handler.
    Open,
    /// Shows it in the file manager, selected.
    Reveal,
    /// Asks the user first, whose answer opens it or shows it.
    Ask,
}

/// What the app does with a target of `kind`, as the platform tells it:
/// a program shows in the file manager, and never runs, whatever `urls`
/// and `files` say; a directory opens; a URL and a file do as they say,
/// asking first by default.
pub(super) fn opening(kind: TargetKind, (urls, files): (OpenUrl, OpenFile)) -> Opening {
    match (kind, urls, files) {
        (TargetKind::Program, ..) | (TargetKind::File, _, OpenFile::Reveal) => Opening::Reveal,
        (TargetKind::Url, OpenUrl::Ask, _) | (TargetKind::File, _, OpenFile::Ask) => Opening::Ask,
        (TargetKind::Directory, ..)
        | (TargetKind::Url, OpenUrl::Open, _)
        | (TargetKind::File, _, OpenFile::Open) => Opening::Open,
    }
}

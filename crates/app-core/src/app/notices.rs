//! What the app tells the user apart from the windows' text: the notices
//! of what went wrong, in a bar over the window, and a click on it; the
//! bell; the programs' notifications, the long commands' ends, and a click
//! on a notification.

use std::time::{Duration, Instant};

use lokyr_kernel::commands::Code;
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::plugins::ui::ViewId;
use lokyr_kernel::workspace::PaneId;
use lokyr_platform_core::{Platform, WindowId};

use super::{
    App, BELL_PAUSE, FLASH, MOST_NOTIFIED, NOTIFICATION_BODY, NOTIFICATION_PAUSE,
    NOTIFICATION_TITLE,
};
use crate::drawing::Backend;

impl<B: Backend> App<B> {
    /// Tells `diagnostic`, in the log and, where it is one, in the notice
    /// of every window.
    pub(super) fn report(&mut self, platform: &mut dyn Platform, diagnostic: Diagnostic) {
        if self.diagnostics.report(diagnostic) {
            self.request_frames(platform);
        }
    }

    /// Tells `text`, the error of a file about `about` that does not read,
    /// as its notice, once: a read that finds the same error tells nothing
    /// again, so that a notice that the user closed stays closed through
    /// the reloads; a new error is told.
    pub(super) fn report_unread(
        &mut self,
        platform: &mut dyn Platform,
        about: About,
        text: String,
    ) {
        if self
            .unread_told
            .iter()
            .any(|(told, error)| *told == about && *error == text)
        {
            return;
        }
        self.unread_told.retain(|(told, _)| *told != about);
        self.unread_told.push((about.clone(), text.clone()));
        self.report(platform, Diagnostic::new(Code::Unavailable, text, about));
    }

    /// Takes away the notice of the file about `about` that did not read,
    /// as it reads.
    pub(super) fn resolve_unread(&mut self, platform: &mut dyn Platform, about: &About) {
        self.unread_told.retain(|(told, _)| told != about);
        self.resolve(platform, about);
    }

    /// Rings `pane`'s bell, as `[bell]` says: the system's sound; a flash
    /// of its window with the focus; for one without, a bounce of the
    /// Dock's icon and 🔔 in its title. A bell within [`BELL_PAUSE`] of the
    /// last that acted in the window does nothing.
    pub(super) fn ring(&mut self, platform: &mut dyn Platform, pane: PaneId) {
        let Some(window) = self.window_of(pane) else {
            return;
        };
        let bell = self.config.bell;
        let now = Instant::now();
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if state.rang_at.is_some_and(|at| now < at + BELL_PAUSE) {
            return;
        }
        state.rang_at = Some(now);
        if bell.sound {
            platform.play_alert_sound();
        }
        if state.focused {
            if bell.flash {
                state.flash_until = Some(now + FLASH);
                if let Some(timer) = self.flash_timer.take() {
                    platform.cancel_timer(timer);
                }
                self.flash_timer = Some(platform.start_timer(now + FLASH));
                platform.request_frame(window);
            }
            return;
        }
        if bell.bounce {
            platform.request_attention();
        }
        if bell.mark_title && !state.bell_marked {
            self.mark_bell(platform, window, true);
            self.show_title(platform, window);
        }
    }

    /// Marks `window`'s title of a bell that rang while it had not the
    /// focus, or of none, as it takes it: the workspace keeps the mark for
    /// the plugins, whose views of the window's tab bar render again, as
    /// the title of a single tab, which the system's leaves to them, draws
    /// it.
    pub(super) fn mark_bell(&mut self, platform: &mut dyn Platform, window: WindowId, bell: bool) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if state.bell_marked == bell {
            return;
        }
        state.bell_marked = bell;
        let shown = state.window;
        self.workspace.set_window_bell(shown, bell);
        let bar = ViewId::TabBar(shown.into());
        for (plugin, view) in self.plugins.views() {
            if view != bar {
                continue;
            }
            let result = self.with_plugins(platform, |plugins, context| {
                plugins.render(&plugin, &view, context)
            });
            if let Err(error) = result {
                eprintln!("lokyr: {error}");
            }
            self.rendered_slots = true;
        }
    }

    /// Shows `pane`'s program's notification, of `title`, or, when that is
    /// empty, of the title that the user gave its window, or its window's
    /// name, and `body`, as the system's, while its window has no focus:
    /// one within [`NOTIFICATION_PAUSE`] of the last of the window shows
    /// nothing, and a long title or text shows its start. A click on it
    /// brings `pane` forward.
    pub(super) fn notify(
        &mut self,
        platform: &mut dyn Platform,
        pane: PaneId,
        title: &str,
        body: &str,
    ) {
        let Some(window) = self.window_of(pane) else {
            return;
        };
        let now = Instant::now();
        let title = if title.is_empty() {
            self.windows
                .get(&window)
                .and_then(|state| state.user_title.clone())
                .unwrap_or_else(|| self.window_name(window))
        } else {
            title.to_owned()
        };
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        if state.focused
            || state
                .notified_at
                .is_some_and(|at| now < at + NOTIFICATION_PAUSE)
        {
            return;
        }
        state.notified_at = Some(now);
        let title: String = title.chars().take(NOTIFICATION_TITLE).collect();
        let body: String = body.chars().take(NOTIFICATION_BODY).collect();
        self.notified_last += 1;
        platform.notify(window, self.notified_last, &title, &body);
        if self.notified.len() >= MOST_NOTIFIED {
            self.notified.pop_front();
        }
        self.notified.push_back((self.notified_last, pane));
    }

    /// Tells the end of `pane`'s command `command`, of `status`, that ran
    /// for `took`, in a notification while its window has no focus, if it
    /// ran for `[notifications] long-command` at least: of the command's
    /// line as its title, or the window's once it left the scrollback, and
    /// whether it succeeded or its status, and its time.
    pub(super) fn long_command_ended(
        &mut self,
        platform: &mut dyn Platform,
        pane: PaneId,
        command: u64,
        status: Option<i32>,
        took: Duration,
    ) {
        let Some(long) = self.config.notifications.long_command.duration() else {
            return;
        };
        if took < long {
            return;
        }
        let title = self
            .workspace
            .pane(pane)
            .and_then(|pane| pane.command_line(command))
            .unwrap_or_default();
        let took = self.duration_text(took);
        let body = match status {
            Some(0) => self
                .texts
                .text_with("command-succeeded", &[("took", Arg::Text(&took))]),
            Some(status) => self.texts.text_with(
                "command-failed",
                &[
                    ("status", Arg::Number(i64::from(status))),
                    ("took", Arg::Text(&took)),
                ],
            ),
            None => self
                .texts
                .text_with("command-ended", &[("took", Arg::Text(&took))]),
        };
        self.notify(platform, pane, &title, &body);
    }

    /// `took`, as a command's time tells it: of seconds, of minutes and
    /// seconds past a minute, of hours and minutes past an hour.
    pub(super) fn duration_text(&self, took: Duration) -> String {
        let seconds = took.as_secs();
        let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
        // Fewer seconds than an `i64` holds.
        let number = |value: u64| Arg::Number(value as i64);
        if hours > 0 {
            self.texts.text_with(
                "command-took-hours",
                &[("hours", number(hours)), ("minutes", number(minutes))],
            )
        } else if minutes > 0 {
            self.texts.text_with(
                "command-took-minutes",
                &[("minutes", number(minutes)), ("seconds", number(seconds))],
            )
        } else {
            self.texts
                .text_with("command-took-seconds", &[("seconds", number(seconds))])
        }
    }

    /// A click on the notification `id` of `window`: the pane that told it,
    /// if it is still open, wherever it went since, its tab and its window,
    /// comes forward; the quick terminal, hidden, shows at its place;
    /// another window takes the focus.
    pub(super) fn notification_clicked(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        id: u64,
    ) {
        let told = self
            .notified
            .iter()
            .find(|&&(shown, _)| shown == id)
            .map(|&(_, pane)| pane)
            .filter(|&pane| self.workspace.pane(pane).is_some());
        let window = match told {
            Some(pane) => {
                if let Some(tab) = self.workspace.tab_of(pane) {
                    self.workspace.activate_tab(tab);
                }
                self.workspace.focus_pane(pane);
                self.window_of(pane).unwrap_or(window)
            }
            None => window,
        };
        if let Some(&(place, _)) = self.quick.get(&window) {
            self.show_quick(platform, window, Some(place));
        } else if self.windows.contains_key(&window) {
            platform.focus_window(window);
        }
    }
}

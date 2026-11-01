//! What the app tells the user apart from the windows' text: the notices
//! of what went wrong, in a bar over the window, and a click on it; the
//! bell; the programs' notifications, and a click on one.

use std::time::Instant;

use lokyr_kernel::diagnostics::Diagnostic;
use lokyr_kernel::workspace::PaneId;
use lokyr_platform_core::{Platform, Point, WindowId};

use super::{App, BELL_PAUSE, FLASH, NOTIFICATION_BODY, NOTIFICATION_PAUSE, NOTIFICATION_TITLE};
use crate::drawing::Backend;
use crate::{notice, quick};

impl<B: Backend> App<B> {
    /// Tells `diagnostic`, in the log and, where it is one, in the notice
    /// of every window.
    pub(super) fn report(&mut self, platform: &mut dyn Platform, diagnostic: Diagnostic) {
        if self.diagnostics.report(diagnostic) {
            self.request_frames(platform);
        }
    }

    /// Whether `position`, in points of `window`'s content, is on the bar
    /// of the notice that its last frame drew, and on its close; `None` off
    /// the bar.
    pub(super) fn on_notice(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<bool> {
        let (state, size) = (self.windows.get(&window)?, platform.window_size(window)?);
        let pixels = (position.x * size.scale, position.y * size.scale);
        let rects = state
            .notice
            .filter(|rects| notice::contains(&rects.bar, pixels))?;
        Some(notice::contains(&rects.close, pixels))
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
            state.bell_marked = true;
            self.show_title(platform, window);
        }
    }

    /// Shows `pane`'s program's notification, of `title`, or, when that is
    /// empty, of the title that the user gave its window, or its window's
    /// name, and `body`, as the system's, while its window has no focus:
    /// one within [`NOTIFICATION_PAUSE`] of the last of the window shows
    /// nothing, and a long title or text shows its start.
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
        platform.notify(window, &title, &body);
    }

    /// A click on a notification of `window`: the quick terminal, hidden,
    /// shows at its place; another window takes the focus.
    pub(super) fn notification_clicked(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.quick.is_some_and(|(quick, _)| quick == window) {
            let place = quick::place(&self.config.quick_terminal);
            platform.show_quick_window(window, Some(&place));
            self.quick = Some((window, true));
        } else if self.windows.contains_key(&window) {
            platform.focus_window(window);
        }
    }
}

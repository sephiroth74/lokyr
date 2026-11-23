//! The windows that leave full screen to hide, as their session hides, and
//! what waits for them: macOS shows another window of the app in full
//! screen too while the one with the keyboard is, so that the windows to
//! show, to bring forward and to open wait until none is left leaving.
//!
//! Each window waits of its latest wish alone: a window hidden again drops
//! what waited for it, so that nothing stale shows once the exits end. A
//! window leaves once however many times it is hidden meanwhile, and one
//! whose exit never ends, as AppKit ignores a toggle during a transition,
//! is overdue past [`WATCHDOG`], when the backend hides it of its own.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lokyr_platform_core::WindowId;

/// How long a window may take to leave full screen before the backend hides
/// it of its own: past macOS's animation, some 0.7 s, with room to spare.
pub(crate) const WATCHDOG: Duration = Duration::from_secs(2);

/// What waits for the windows that leave full screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Waiting {
    /// The window shows again, where it was.
    Show,
    /// The window comes forward, with the keyboard, as a new one opens too.
    Focus,
}

/// The windows that leave full screen to hide, each of when it started,
/// and what waits for them, in its order.
#[derive(Debug, Default)]
pub(crate) struct Deferral {
    leaving: BTreeMap<WindowId, Instant>,
    waiting: Vec<(WindowId, Waiting)>,
}

impl Deferral {
    /// Whether a window leaves full screen, so that what shows a window
    /// waits.
    pub(crate) fn holds(&self) -> bool {
        !self.leaving.is_empty()
    }

    /// `window` starts leaving full screen to hide, at `now`: whether it
    /// was not leaving already.
    pub(crate) fn starts_leaving(&mut self, window: WindowId, now: Instant) -> bool {
        if self.leaving.contains_key(&window) {
            return false;
        }
        self.leaving.insert(window, now);
        true
    }

    /// `window` is hidden: what waited for it goes, as its latest wish is
    /// to hide.
    pub(crate) fn hidden(&mut self, window: WindowId) {
        self.waiting.retain(|&(waiting, _)| waiting != window);
    }

    /// `what` waits for the windows that leave full screen, of `window`, in
    /// place of what waited for it of the same kind.
    pub(crate) fn wait(&mut self, window: WindowId, what: Waiting) {
        self.waiting
            .retain(|&(waiting, kind)| !(waiting == window && kind == what));
        self.waiting.push((window, what));
    }

    /// `window` left full screen and hid, or will not: what waited, in its
    /// order, once no window is left leaving; none before.
    pub(crate) fn left(&mut self, window: WindowId) -> Vec<(WindowId, Waiting)> {
        self.leaving.remove(&window);
        if self.holds() {
            return Vec::new();
        }
        std::mem::take(&mut self.waiting)
    }

    /// `window` closed: it leaves no more, and nothing waits for it; what
    /// waited for the others, once none is left leaving.
    pub(crate) fn closed(&mut self, window: WindowId) -> Vec<(WindowId, Waiting)> {
        self.hidden(window);
        if !self.leaving.contains_key(&window) {
            return Vec::new();
        }
        self.left(window)
    }

    /// The windows that have left full screen for [`WATCHDOG`] or more, at
    /// `now`, whose exits never ended.
    pub(crate) fn overdue(&self, now: Instant) -> Vec<WindowId> {
        self.leaving
            .iter()
            .filter(|&(_, &since)| now.saturating_duration_since(since) >= WATCHDOG)
            .map(|(&window, _)| window)
            .collect()
    }

    /// When the first window that leaves is overdue, if one leaves.
    pub(crate) fn next_due(&self) -> Option<Instant> {
        self.leaving.values().min().map(|&since| since + WATCHDOG)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64) -> WindowId {
        WindowId::new(id)
    }

    #[test]
    fn what_waits_runs_once_no_window_leaves() {
        let mut deferral = Deferral::default();
        let now = Instant::now();
        assert!(!deferral.holds());
        assert!(deferral.starts_leaving(window(1), now));
        deferral.wait(window(2), Waiting::Show);
        deferral.wait(window(2), Waiting::Focus);
        assert_eq!(
            deferral.left(window(1)),
            [(window(2), Waiting::Show), (window(2), Waiting::Focus)]
        );
        assert!(!deferral.holds());
    }

    #[test]
    fn a_window_hidden_again_drops_what_waited_for_it() {
        // A→B→A within the exit: B's window waits to show, then hides; A's
        // window, which leaves, waits to show again.
        let mut deferral = Deferral::default();
        let now = Instant::now();
        let (a, b) = (window(1), window(2));
        assert!(deferral.starts_leaving(a, now));
        deferral.wait(b, Waiting::Show);
        deferral.wait(b, Waiting::Focus);
        deferral.hidden(b);
        deferral.wait(a, Waiting::Show);
        deferral.wait(a, Waiting::Focus);
        assert_eq!(deferral.left(a), [(a, Waiting::Show), (a, Waiting::Focus)]);
    }

    #[test]
    fn a_window_leaves_once_however_many_times_it_hides() {
        // A→B→A→B within the exit: A's window is hidden twice, which counts
        // once, so that its one exit frees what waits.
        let mut deferral = Deferral::default();
        let now = Instant::now();
        let (a, b) = (window(1), window(2));
        assert!(deferral.starts_leaving(a, now));
        deferral.wait(b, Waiting::Show);
        deferral.hidden(b);
        deferral.wait(a, Waiting::Show);
        deferral.hidden(a);
        assert!(!deferral.starts_leaving(a, now));
        deferral.wait(b, Waiting::Show);
        assert_eq!(deferral.left(a), [(b, Waiting::Show)]);
        // The same wish twice waits once.
        deferral.wait(b, Waiting::Focus);
        deferral.wait(b, Waiting::Focus);
        assert!(deferral.starts_leaving(a, now));
        assert_eq!(deferral.left(a), [(b, Waiting::Focus)]);
    }

    #[test]
    fn two_windows_leave_and_a_closed_one_leaves_no_more() {
        let mut deferral = Deferral::default();
        let now = Instant::now();
        let (a, b, c) = (window(1), window(2), window(3));
        deferral.starts_leaving(a, now);
        deferral.starts_leaving(b, now);
        deferral.wait(c, Waiting::Focus);
        assert!(deferral.left(a).is_empty());
        // A window that does not leave closes of nothing to free.
        assert!(deferral.closed(c).is_empty());
        deferral.wait(c, Waiting::Show);
        assert_eq!(deferral.closed(b), [(c, Waiting::Show)]);
    }

    #[test]
    fn an_exit_that_never_ends_is_overdue_past_the_watchdog() {
        let mut deferral = Deferral::default();
        let start = Instant::now();
        assert_eq!(deferral.next_due(), None);
        deferral.starts_leaving(window(1), start);
        assert_eq!(deferral.next_due(), Some(start + WATCHDOG));
        assert!(deferral.overdue(start + WATCHDOG / 2).is_empty());
        assert_eq!(deferral.overdue(start + WATCHDOG), [window(1)]);
    }
}

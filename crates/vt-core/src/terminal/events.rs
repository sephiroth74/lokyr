//! The events that wait for the embedder, within limits, as a program can
//! make them faster than the embedder takes them.

use super::osc::Event;

/// The bytes of the events with a payload that wait to be taken, at most,
/// each counted with its own size: past them, such an event is dropped
/// until the next take.
pub(super) const EVENT_BYTES: usize = 1 << 20;

/// The events that wait, oldest first.
///
/// An event that tells of a change that the terminal's state shows, a
/// title's or the colors', waits once: another of its kind adds nothing. An
/// event that carries the new state, the tab's color, the progress or the
/// working directory, takes the place of the one of its kind that waits, so
/// that the latest is told, once.
#[derive(Clone, Debug, Default)]
pub(super) struct Events {
    events: Vec<Event>,
    title: bool,
    colors: bool,
    /// Where the event of each kind that carries a state waits: the tab's
    /// color, the progress, the working directory.
    states: [Option<usize>; 3],
    /// The bytes of the events with a payload that wait.
    bytes: usize,
}

impl Events {
    /// Adds `event`, unless it adds nothing to what waits, or would take the
    /// events with a payload past [`EVENT_BYTES`].
    pub(super) fn push(&mut self, event: Event) {
        let state = match &event {
            Event::TitleChanged => {
                if std::mem::replace(&mut self.title, true) {
                    return;
                }
                None
            }
            Event::ColorsChanged => {
                if std::mem::replace(&mut self.colors, true) {
                    return;
                }
                None
            }
            Event::TabColor(_) => Some(0),
            Event::Progress(_) => Some(1),
            Event::WorkingDirectory(_) => Some(2),
            Event::Notification { title, body } => {
                if !self.fits(title.len() + body.len()) {
                    return;
                }
                None
            }
            Event::ClipboardWrite { data, .. } => {
                if !self.fits(data.len()) {
                    return;
                }
                None
            }
            Event::ClipboardRead(_) => {
                if !self.fits(0) {
                    return;
                }
                None
            }
        };
        if let Some(kind) = state {
            if let Some(at) = self.states[kind] {
                self.events[at] = event;
                return;
            }
            self.states[kind] = Some(self.events.len());
        }
        self.events.push(event);
    }

    /// Whether an event with a payload of `payload` bytes fits, which it
    /// then takes.
    fn fits(&mut self, payload: usize) -> bool {
        let bytes = size_of::<Event>() + payload;
        if self.bytes + bytes > EVENT_BYTES {
            return false;
        }
        self.bytes += bytes;
        true
    }

    /// Takes the events that wait, oldest first.
    pub(super) fn take(&mut self) -> Vec<Event> {
        std::mem::take(self).events
    }
}

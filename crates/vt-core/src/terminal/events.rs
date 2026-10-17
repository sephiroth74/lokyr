//! The events that wait for the embedder, within limits, as a program can
//! make them faster than the embedder takes them.

use super::clipboard::ClipboardTarget;
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
/// that the latest is told, once. A write to the clipboards takes the
/// place of the writes that wait to the same clipboards, or to some of
/// them, made since the last read, which is answered with what was written
/// before it: the clipboards end with the program's last write, unless it
/// does not fit, when it goes with those that it overwrites.
#[derive(Clone, Debug, Default)]
pub(super) struct Events {
    /// The events, and the places of those that a later one took.
    events: Vec<Option<Event>>,
    title: bool,
    colors: bool,
    /// Where the event of each kind that carries a state waits: the tab's
    /// color, the progress, the working directory.
    states: [Option<usize>; 3],
    /// Where the write since the last read to each set of clipboards
    /// waits: the clipboard alone, the primary selection alone, both.
    writes: [Option<usize>; 3],
    /// The bytes of the events with a payload that wait.
    bytes: usize,
}

impl Events {
    /// Adds `event`, unless it adds nothing to what waits, or would take the
    /// events with a payload past [`EVENT_BYTES`].
    pub(super) fn push(&mut self, event: Event) {
        let (state, write) = match &event {
            Event::TitleChanged => {
                if std::mem::replace(&mut self.title, true) {
                    return;
                }
                (None, None)
            }
            Event::ColorsChanged => {
                if std::mem::replace(&mut self.colors, true) {
                    return;
                }
                (None, None)
            }
            Event::TabColor(_) => (Some(0), None),
            Event::Progress(_) => (Some(1), None),
            Event::WorkingDirectory(_) => (Some(2), None),
            Event::Notification { title, body } => {
                if !self.fits(title.len() + body.len()) {
                    return;
                }
                (None, None)
            }
            Event::ClipboardWrite { target, data } => {
                let set = clipboards(*target);
                self.overwrite(set);
                if !self.fits(data.len()) {
                    return;
                }
                (None, (set != 0).then_some(set))
            }
            Event::ClipboardRead(_) => {
                if !self.fits(0) {
                    return;
                }
                // The read is answered with what the writes before it
                // wrote, which the writes after it leave.
                self.writes = [None; 3];
                (None, None)
            }
        };
        if let Some(kind) = state {
            if let Some(at) = self.states[kind] {
                self.events[at] = Some(event);
                return;
            }
            self.states[kind] = Some(self.events.len());
        }
        if let Some(set) = write {
            self.writes[set - 1] = Some(self.events.len());
        }
        self.events.push(Some(event));
    }

    /// Drops the writes that wait to `set`, or to some of its clipboards,
    /// since the last read, as a write to `set` overwrites them.
    fn overwrite(&mut self, set: usize) {
        for other in 1..=3 {
            if other & set != other {
                continue;
            }
            let Some(at) = self.writes[other - 1].take() else {
                continue;
            };
            if let Some(Event::ClipboardWrite { data, .. }) = self.events[at].take() {
                self.bytes -= size_of::<Event>() + data.len();
            }
        }
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
        std::mem::take(self).events.into_iter().flatten().collect()
    }
}

/// The clipboards of `target`, as a set of bits: 1 the clipboard, 2 the
/// primary selection.
fn clipboards(target: ClipboardTarget) -> usize {
    usize::from(target.clipboard) | usize::from(target.primary) << 1
}

//! The search service: a search of a pane's terminal, its scrollback too,
//! on a thread of the host's, a part of its rows at a time under the
//! pane's lock, from the rows of its view outward, whose matches the
//! terminal keeps, of its pins, through new output, scrolls and reflows,
//! and shows over its cells as its plugin asks. While its filter shows, it
//! follows: the rows that new output brings, and the screen's, which a
//! program writes over, searched again as they come, a part at most each
//! look, on the main thread, from a pinned cell, which keeps its row
//! through a resize. A search that a resize, or a reset, cuts short starts
//! again, its plugin told its count alone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};

use lokyr_plugin_sdk::host::{self, MOST_MATCHES};
use lokyr_search::{Case, Query};
use lokyr_session::Reader;
use lokyr_vt_core::terminal::{CellPin, Terminal};

/// The most rows that a part of a search reads under the pane's lock,
/// which the program's output and the frames wait for meanwhile.
pub(super) const PART: usize = 1000;

/// What the engine looks for, of a plugin's `asked` query, or why it
/// cannot look.
pub(super) fn query(asked: &host::SearchQuery) -> Result<Query, String> {
    let case = match asked.case {
        host::SearchCase::Sensitive => Case::Sensitive,
        host::SearchCase::Insensitive => Case::Insensitive,
        host::SearchCase::Smart if asked.text.chars().any(char::is_uppercase) => Case::Sensitive,
        host::SearchCase::Smart => Case::Insensitive,
    };
    let none = || "a search of no text finds nothing".to_owned();
    let query = if asked.regex {
        if asked.text.is_empty() {
            return Err(none());
        }
        Query::regex(&asked.text, case).map_err(|error| error.to_string())?
    } else {
        Query::text(&asked.text, case).ok_or_else(none)?
    };
    Ok(query.whole_word(asked.whole_word))
}

/// The parts of a search of the rows numbered from `first` to `end`, past
/// its last, `part` rows at most each, of a view of the rows from `top`,
/// `shown` of them: the view's first, then a part below it and a part
/// above it in turn, the nearest first, so that the matches near the view
/// come first.
pub(super) fn parts(
    first: i64,
    end: i64,
    (top, shown): (i64, usize),
    part: usize,
) -> Vec<(i64, i64)> {
    let part = i64::try_from(part.max(1)).unwrap_or(i64::MAX);
    let shown = i64::try_from(shown).unwrap_or(i64::MAX);
    let top = top.clamp(first, end);
    let bottom = top.saturating_add(shown).clamp(top, end);
    let mut parts = Vec::new();
    if top < bottom {
        parts.push((top, bottom));
    }
    let (mut below, mut above) = (bottom, top);
    while below < end || above > first {
        if below < end {
            let next = below.saturating_add(part).min(end);
            parts.push((below, next));
            below = next;
        }
        if above > first {
            let next = above.saturating_sub(part).max(first);
            parts.push((next, above));
            above = next;
        }
    }
    parts
}

/// A part of a search, as its thread tells it: its matches, as their cells
/// were found, how many the search found so far, and whether it went to
/// its end.
pub(super) struct Part {
    pub(super) matches: Vec<host::SearchMatch>,
    pub(super) count: usize,
    pub(super) done: bool,
}

/// What a search found, beside its matches, which the terminal keeps by
/// the search's id.
#[derive(Default)]
struct Found {
    /// How many matches it found, those past [`MOST_MATCHES`] too.
    count: usize,
    /// Whether it went to its end.
    done: bool,
    /// Whether its thread stopped short, as the rows were renumbered: a
    /// resize, a reset or the other screen.
    cut: bool,
    /// Whether it ended, its matches let go: a part found after that lets
    /// its own go at once, rather than give them to the terminal.
    ended: bool,
}

/// A search of a plugin's, under way or done, as the host keeps it.
pub(super) struct Search {
    pub(super) id: u64,
    pub(super) pane: host::PaneId,
    cancel: Arc<AtomicBool>,
    found: Arc<Mutex<Found>>,
    /// The parts that its thread tells, until it went to its end.
    parts: Option<mpsc::Receiver<Part>>,
    /// Whether its plugin showed its matches, or its current one.
    shown: bool,
    /// Whether its plugin showed all its matches, as it asked last.
    all: bool,
    /// Whether its pane's view shows its matches' rows alone, as its plugin
    /// asked.
    filtering: bool,
    /// What it follows the rows that new output brings of, once its thread
    /// went to its end.
    follow: Follow,
    /// Whether its plugin was told matches of its parts.
    told: bool,
    /// Whether its thread started again, as a resize or a reset cut the
    /// first short, once its plugin was told matches: its parts tell the
    /// plugin the count alone, the matches staying the host's, as the
    /// plugin knew those before.
    again: bool,
    /// Whether a reset let its matches go, whose showing and filter its
    /// next part sets again, as the plugin asked them.
    lost: bool,
    /// Whether it starts again once the screen that it searched shows
    /// again, as the other screen cut it short.
    pending: bool,
}

/// What a search follows new output of: the pane's reader and the query;
/// the row from which it reads again, the screen's first as it looked
/// last, whose rows a program may have written over since, of a pin of its
/// first cell, which keeps its row through a resize, and of its number, of
/// a list of no pin left; the screen that it follows, the one that showed
/// as it started; the number past the list's last row then, and the slices
/// of output parsed, as output that the screen's empty rows take adds no
/// row; and the rows' epoch, past which their numbers mean nothing.
struct Follow {
    reader: Reader,
    query: Query,
    from: Option<CellPin>,
    from_row: i64,
    alternate: bool,
    end: i64,
    slices: u64,
    epoch: u64,
}

impl Follow {
    /// What a search of `query` follows in `terminal`, locked, which
    /// `reader` reads: from the screen's first row now.
    fn new(reader: Reader, query: Query, terminal: &mut Terminal) -> Self {
        let rows = terminal.screen_rows();
        let (top, total) = (rows.scrollback_rows(), rows.total_rows());
        let from_row = terminal.row_number(top);
        Self {
            from: terminal.pin_cell((0, from_row)),
            from_row,
            alternate: terminal.is_alternate_screen(),
            end: terminal.row_number(total),
            slices: reader.slices(),
            epoch: terminal.origin().epoch,
            reader,
            query,
        }
    }

    /// Lets go of its pin in `terminal`, its pane's, locked.
    fn release(&mut self, terminal: &mut Terminal) {
        if let Some(pin) = self.from.take() {
            terminal.unpin_cell(&pin);
        }
    }

    /// Pins the row numbered `row` of `terminal`, locked, as the one from
    /// which it reads again, its former pin let go.
    fn read_from(&mut self, terminal: &mut Terminal, row: i64) {
        self.release(terminal);
        self.from = terminal.pin_cell((0, row));
        self.from_row = row;
    }

    /// The number of the row from which it reads again in `terminal`,
    /// locked, as its pin is now; of a pin lost, as its row left the
    /// scrollback, or of none, the list's first row, or its number of the
    /// same epoch.
    fn first_row(&self, terminal: &Terminal) -> i64 {
        let first = terminal.row_number(0);
        let row = match &self.from {
            Some(pin) => terminal.pinned_cell(pin).map_or(first, |(_, row)| row),
            None if terminal.origin().epoch == self.epoch => self.from_row,
            None => first,
        };
        row.max(first)
    }
}

/// Starts the thread of the search `id` of `query` in the terminal that
/// `reader` reads, of `cancel` and `found`: the receiver of its parts. A
/// thread that does not start drops the sender, which tells the search
/// done.
fn spawn(
    id: u64,
    reader: Reader,
    query: Query,
    cancel: &Arc<AtomicBool>,
    found: &Arc<Mutex<Found>>,
) -> mpsc::Receiver<Part> {
    let (sender, receiver) = mpsc::channel();
    let (cancel, found) = (Arc::clone(cancel), Arc::clone(found));
    let _ = std::thread::Builder::new()
        .name("lokyr-search".to_owned())
        .spawn(move || search(id, &reader, &query, (&cancel, &found), &sender));
    receiver
}

impl Search {
    /// Starts the search `id` of `query` in the terminal of `pane`, which
    /// `reader` reads, on a thread of its own, which wakes the main thread
    /// at each part. A thread that does not start tells the search done,
    /// of no match.
    pub(super) fn start(id: u64, pane: host::PaneId, reader: Reader, query: Query) -> Self {
        let follow = {
            let mut terminal = reader.lock();
            Follow::new(reader.clone(), query.clone(), &mut terminal)
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let found = Arc::new(Mutex::new(Found::default()));
        let parts = spawn(id, reader, query, &cancel, &found);
        Self {
            id,
            pane,
            cancel,
            found,
            parts: Some(parts),
            shown: false,
            all: false,
            filtering: false,
            follow,
            told: false,
            again: false,
            lost: false,
            pending: false,
        }
    }

    /// Starts it again in `terminal`, its pane's, locked, of a thread of
    /// its own, as a resize or a reset cut the former short: the former
    /// stops, its matches go, the terminal keeping their showing and their
    /// filter, which a reset let go with them and which its first part sets
    /// again; it follows from the screen's first row now.
    fn start_again(&mut self, terminal: &mut Terminal) {
        self.cancel.store(true, Ordering::Relaxed);
        // A part of the former's that its thread finds after this lets its
        // pins go at once.
        lock(&self.found).ended = true;
        self.lost = !terminal.keeps_matches(self.id);
        terminal.drop_matches_from(self.id, i64::MIN);
        self.follow.release(terminal);
        let (reader, query) = (self.follow.reader.clone(), self.follow.query.clone());
        self.follow = Follow::new(reader.clone(), query.clone(), terminal);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.found = Arc::new(Mutex::new(Found::default()));
        self.parts = Some(spawn(self.id, reader, query, &self.cancel, &self.found));
        self.again = self.told;
        self.pending = false;
    }

    /// Shows its matches' rows alone in its pane's view of `terminal`,
    /// locked, or the view as it was, as `filter` says, the rows that new
    /// output brings searched as they come meanwhile. A search of the
    /// alternate screen shows no filter, as the filter shows the primary
    /// screen's rows, which its matches are none of, so that the pane's
    /// view is not emptied as the program ends; off, it leaves another
    /// search's filter as it is. Whether the view changed.
    pub(super) fn filter(&mut self, terminal: &mut Terminal, filter: bool) -> bool {
        if filter && self.follow.alternate {
            return false;
        }
        self.filtering = filter;
        if !filter && terminal.filter() != Some(self.id) {
            return false;
        }
        terminal.set_filter(filter.then_some(self.id))
    }

    /// Searches, while its filter shows and once its thread went to its
    /// end, of output parsed since its last look, the rows that it brought,
    /// and the screen's again, a part at most, under the pane's lock, from
    /// its pinned row, which a resize renumbers: the matches there before
    /// go, as a program may have written over them, and those found now
    /// come. The part found, of the count of the matches that the terminal
    /// keeps and of no match, as its plugin knows the rest of the host's;
    /// none of no new row, or while the other screen shows. A reset, which
    /// let its matches go, starts it again.
    pub(super) fn follow(&mut self) -> Option<Part> {
        if !self.filtering || self.parts.is_some() || self.pending {
            return None;
        }
        let reader = self.follow.reader.clone();
        let mut terminal = reader.lock();
        if terminal.filter() != Some(self.id) {
            // A reset let its matches and its filter go; another search's
            // filter takes the view else.
            if !terminal.keeps_matches(self.id) {
                self.start_again(&mut terminal);
            }
            return None;
        }
        let follow = &mut self.follow;
        if terminal.is_alternate_screen() != follow.alternate {
            return None;
        }
        let total = terminal.screen_rows().total_rows();
        let end = terminal.row_number(total);
        let slices = follow.reader.slices();
        let epoch = terminal.origin().epoch;
        if end == follow.end && slices == follow.slices && epoch == follow.epoch {
            return None;
        }
        let from = follow.first_row(&terminal);
        let part = i64::try_from(PART).unwrap_or(i64::MAX);
        let to = end.min(from.saturating_add(part));
        let start = terminal.row_index(from)?;
        let rows = start
            ..start
                .saturating_add(usize::try_from(to - from).unwrap_or(0))
                .min(total);
        let kept = terminal.drop_matches_from(self.id, from);
        let cancel = AtomicBool::new(false);
        let all = lokyr_search::find_all(
            terminal.screen_rows_mut(),
            &follow.query,
            rows,
            MOST_MATCHES.saturating_sub(kept),
            &cancel,
        )
        .ok()?;
        let pins: Vec<(CellPin, CellPin)> = all
            .pinned
            .into_iter()
            .map(|found| (terminal.cell_pin(found.start), terminal.cell_pin(found.end)))
            .collect();
        terminal.add_matches(self.id, from, pins);
        let count = terminal.current_match(self.id).1;
        // A part's rows past the screen's stay as they are; the screen's
        // are read again at the next look.
        let next = if to < end {
            to
        } else {
            follow.end = end;
            follow.slices = slices;
            let rows = terminal.screen_rows();
            terminal.row_number(rows.scrollback_rows()).min(to)
        };
        follow.read_from(&mut terminal, next);
        follow.epoch = epoch;
        lock(&self.found).count = count;
        Some(Part {
            matches: Vec::new(),
            count,
            done: true,
        })
    }

    /// Whether its plugin showed its matches, or its current one, last.
    pub(super) fn is_shown(&self) -> bool {
        self.shown
    }

    /// Stops it, its matches staying in the terminal, shown as they were,
    /// until [`Search::end`]: its thread finds no more.
    pub(super) fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        lock(&self.found).ended = true;
    }

    /// Whether more of its parts may come, as its thread runs, or will
    /// start again.
    pub(super) fn is_busy(&self) -> bool {
        self.parts.is_some() || self.pending
    }

    /// Whether it follows new output, as its filter shows, which each turn
    /// of the host's looks at.
    pub(super) fn follows(&self) -> bool {
        self.filtering
    }

    /// The parts that came since the last look; the last of them done. A
    /// thread that the rows' renumbering cut short starts again, its last
    /// part not done, once the screen that it searched shows; a part of a
    /// thread started again once its plugin was told matches tells the
    /// count alone, and the first after a reset sets the matches' showing
    /// and filter again.
    pub(super) fn take(&mut self) -> Vec<Part> {
        let mut taken = self.parts();
        if self.again {
            for part in &mut taken {
                part.matches.clear();
            }
        } else if taken.iter().any(|part| !part.matches.is_empty()) {
            self.told = true;
        }
        let cut = self.parts.is_none() && {
            let found = lock(&self.found);
            !found.ended && (self.pending || found.cut)
        };
        if cut {
            let reader = self.follow.reader.clone();
            let mut terminal = reader.lock();
            if terminal.is_alternate_screen() == self.follow.alternate {
                self.start_again(&mut terminal);
            } else {
                self.pending = true;
            }
            if let Some(last) = taken.last_mut() {
                last.done = false;
            }
        }
        if self.lost && !taken.is_empty() {
            let reader = self.follow.reader.clone();
            let mut terminal = reader.lock();
            if terminal.keeps_matches(self.id) {
                self.lost = false;
                if self.shown {
                    terminal.show_matches(self.id, self.all, None);
                }
                if self.filtering {
                    terminal.set_filter(Some(self.id));
                }
            }
        }
        taken
    }

    /// The parts that its thread told since the last look; the last of them
    /// done.
    fn parts(&mut self) -> Vec<Part> {
        let mut taken = Vec::new();
        let Some(parts) = &self.parts else {
            return taken;
        };
        loop {
            match parts.try_recv() {
                Ok(part) => {
                    let done = part.done;
                    taken.push(part);
                    if done {
                        self.parts = None;
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                // Its thread did not start, or ended without its last part.
                Err(mpsc::TryRecvError::Disconnected) => {
                    let count = lock(&self.found).count;
                    taken.push(Part {
                        matches: Vec::new(),
                        count,
                        done: true,
                    });
                    self.parts = None;
                    break;
                }
            }
        }
        taken
    }

    /// Ends the search: its thread stops, and its matches and its pin go
    /// from `terminal`, its pane's, locked, if the pane is still there.
    pub(super) fn end(mut self, terminal: Option<&mut Terminal>) {
        self.cancel.store(true, Ordering::Relaxed);
        lock(&self.found).ended = true;
        if let Some(terminal) = terminal {
            terminal.drop_matches(self.id);
            self.follow.release(terminal);
        }
    }

    /// Shows its matches over the cells of `terminal`, its pane's, locked,
    /// or hides them, as `all` says, and `current` apart, the current
    /// match, if one. Whether the terminal keeps its matches, as its thread
    /// gave it a part.
    pub(super) fn show(
        &mut self,
        terminal: &mut Terminal,
        all: bool,
        current: Option<host::SearchMatch>,
    ) -> bool {
        self.shown = all || current.is_some();
        self.all = all;
        let cells = current.map(|found| (found.first, found.last));
        terminal.show_matches(self.id, all, cells)
    }

    /// Its current match, as its cells are now in `terminal`, its pane's,
    /// locked, and its place among its matches now; and how many of them
    /// keep their text.
    pub(super) fn current(&self, terminal: &Terminal) -> host::Current {
        let (current, kept) = terminal.current_match(self.id);
        host::Current {
            current: current
                .map(|(place, (first, last))| (place, host::SearchMatch { first, last })),
            kept,
        }
    }

    /// Its matches, in the rows' order, from the one at `first`, `count` at
    /// most, as their cells are now in `terminal`, its pane's, locked,
    /// those whose cells left it gone; how many it found; and whether it
    /// went to its end.
    pub(super) fn matches(&self, terminal: &Terminal, first: usize, count: usize) -> host::Found {
        // The terminal's lock first, then the count's, as the search's
        // thread takes them.
        let found = lock(&self.found);
        let matches = terminal
            .matches_of(self.id, first, count)
            .into_iter()
            .map(|(first, last)| host::SearchMatch { first, last })
            .collect();
        host::Found {
            matches,
            count: found.count,
            done: found.done,
        }
    }
}

impl Drop for Search {
    /// A search that the host forgets stops.
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// `found`, locked, whatever a thread that panicked left in it.
fn lock(found: &Mutex<Found>) -> MutexGuard<'_, Found> {
    found.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The match of the cells of `start` and `end`, where they are in
/// `terminal` now, if both are there.
fn told(terminal: &Terminal, start: &CellPin, end: &CellPin) -> Option<host::SearchMatch> {
    Some(host::SearchMatch {
        first: terminal.pinned_cell(start)?,
        last: terminal.pinned_cell(end)?,
    })
}

/// The search `id`'s thread: each of its parts of `reader`'s terminal,
/// under the lock, of the rows that the list holds as it starts, from the
/// view's outward, its matches given to the terminal, counted in `found`
/// and told through `parts`, waking the main thread, until its last part,
/// or a resize, a reset or a change of the screen, which renumbers the
/// rows, cuts it short; `cancel` stops it at once.
fn search(
    id: u64,
    reader: &Reader,
    query: &Query,
    (cancel, found): (&AtomicBool, &Mutex<Found>),
    parts: &mpsc::Sender<Part>,
) {
    let (epoch, order) = {
        let terminal = reader.lock();
        let origin = terminal.origin();
        let total = terminal.screen_rows().total_rows();
        let first = terminal.row_number(0);
        let end = terminal.row_number(total);
        let view = (origin.top, usize::from(terminal.rows()));
        (origin.epoch, self::parts(first, end, view, PART))
    };
    let (mut count, mut kept) = (0, 0);
    let last = order.len().saturating_sub(1);
    for (at, (from, to)) in order.into_iter().enumerate() {
        let done = at == last;
        let matches = {
            let mut terminal = reader.lock();
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            // Renumbered, as by a resize, or of the other screen: the
            // parts left are of no rows that it knows.
            if terminal.origin().epoch != epoch {
                let mut found = lock(found);
                found.done = true;
                found.cut = true;
                drop(found);
                let _ = parts.send(Part {
                    matches: Vec::new(),
                    count,
                    done: true,
                });
                reader.wake();
                return;
            }
            // The part's rows that the list still holds, as the
            // scrollback's limit takes the oldest.
            let total = terminal.screen_rows().total_rows();
            let from = from.max(terminal.row_number(0));
            let rows = match terminal.row_index(from) {
                Some(start) => {
                    let wanted = usize::try_from(to.saturating_sub(from)).unwrap_or(0);
                    start..start.saturating_add(wanted).min(total)
                }
                None => 0..0,
            };
            let Ok(all) = lokyr_search::find_all(
                terminal.screen_rows_mut(),
                query,
                rows,
                MOST_MATCHES - kept,
                cancel,
            ) else {
                return;
            };
            let pins: Vec<(CellPin, CellPin)> = all
                .pinned
                .into_iter()
                .map(|found| (terminal.cell_pin(found.start), terminal.cell_pin(found.end)))
                .collect();
            let matches: Vec<host::SearchMatch> = pins
                .iter()
                .filter_map(|(start, end)| told(&terminal, start, end))
                .collect();
            let mut kept_found = lock(found);
            // Ended meanwhile: its pins go at once, under the lock.
            if kept_found.ended {
                for (start, end) in &pins {
                    terminal.unpin_cell(start);
                    terminal.unpin_cell(end);
                }
                return;
            }
            kept += pins.len();
            count += all.count;
            terminal.add_matches(id, from, pins);
            kept_found.count = count;
            kept_found.done = done;
            matches
        };
        // A receiver that went, as its search's end, takes nothing.
        let _ = parts.send(Part {
            matches,
            count,
            done,
        });
        reader.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_go_from_the_view_outward_below_then_above_each_in_turn() {
        // A view of 10 rows from 50, in rows from 0 to 100, of parts of 20.
        assert_eq!(
            parts(0, 100, (50, 10), 20),
            [(50, 60), (60, 80), (30, 50), (80, 100), (10, 30), (0, 10)]
        );
        // A view at the end, of the screen, the scrollback above it.
        assert_eq!(parts(-30, 24, (0, 24), 20), [(0, 24), (-20, 0), (-30, -20)]);
        // A view past the rows held, at their end.
        assert_eq!(parts(0, 5, (10, 24), 20), [(0, 5)]);
        // No rows, no part.
        assert!(parts(7, 7, (7, 24), 20).is_empty());
    }

    #[test]
    fn a_query_takes_the_case_of_its_text_and_refuses_no_text_and_a_regex_that_does_not_compile() {
        let asked = |text: &str, regex: bool| host::SearchQuery {
            text: text.to_owned(),
            regex,
            ..host::SearchQuery::default()
        };
        assert!(query(&asked("abc", false)).is_ok());
        assert!(query(&asked("a(b", true)).is_err());
        assert!(query(&asked("", false)).is_err());
        assert!(query(&asked("", true)).is_err());
    }
}

//! Literal and regex search over the terminal grid, scrollback included.
//!
//! Soft-wrapped rows are searched as a single logical line, so matches that
//! cross the right margin are still found.

#![forbid(unsafe_code)]

mod line;
mod query;

pub use query::{Case, PatternError, Query};

use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use bolt_vt_core::page::{PageList, PinId, Position};

/// Which way a search goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Towards the newest rows.
    Forward,
    /// Towards the oldest rows.
    Backward,
}

/// A match: its first and its last cell, pinned in the list, so that they
/// follow the cells as output scrolls them, and go when the cells do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    /// The match's first cell.
    pub start: PinId,
    /// Its last cell: the first half of a wide character, if it ends with
    /// one.
    pub end: PinId,
}

impl Match {
    /// Lets go of the match's pins.
    pub fn release(self, list: &mut PageList) {
        list.unpin(self.start);
        list.unpin(self.end);
    }
}

/// A search that stopped, as another thread asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the search was cancelled")
    }
}

impl std::error::Error for Cancelled {}

impl Cancelled {
    /// `Err` once `cancel` is set.
    pub(crate) fn check(cancel: &AtomicBool) -> Result<(), Self> {
        if cancel.load(Ordering::Relaxed) {
            Err(Self)
        } else {
            Ok(())
        }
    }
}

/// Finds `query` in `list`: forward, the first match that starts at `from`
/// or after it; backward, the last one that starts before it.
///
/// The text of a logical line, whose soft-wrapped rows are joined, is
/// searched whole, so that a match can cross the right margin; empty cells
/// read as spaces, and the spaces that end a line go, those that a program
/// printed too, so that `b ` matches nowhere at a line's end. The matches of
/// a line are its leftmost-first matches, found from its start, none of
/// them overlapping another, for literal text and regexes alike, so that
/// both ways go through the same matches. A match starts at a cell and
/// ends with one: one that starts or ends inside a cluster does not count,
/// nor does an empty match.
///
/// Either way, each step reads the whole logical line that it starts in
/// first, which takes time in proportion to the line's length, and goes
/// through the line's matches from its start: a step near the end of a long
/// line takes time in proportion to the matches before it too, backward as
/// forward, and, for a regex whose search scans past its match to the
/// line's end, as `a.*b|a` does, time that grows with the square of the
/// line's length. [`find_all`] goes through a range of rows once.
///
/// The search stops with [`Cancelled`] once `cancel` is set, which another
/// thread can do: it looks at it before each line, and between the matches
/// that it tries in a line, which can be long. It reads cold pages as they
/// are kept, so that it takes no memory from the scrollback. `from` is a
/// cell of the list; a position that is not in it finds nothing.
pub fn find(
    list: &mut PageList,
    query: &Query,
    from: Position,
    direction: Direction,
    cancel: &AtomicBool,
) -> Result<Option<Match>, Cancelled> {
    let Some(index) = list.row_index(from.page, from.row) else {
        return Ok(None);
    };
    if from.col >= list.cols() {
        return Ok(None);
    }
    let found = match direction {
        Direction::Forward => line::find_forward(list, index, query, from, cancel)?,
        Direction::Backward => line::find_backward(list, index, query, from, cancel)?,
    };
    let Some((start, end)) = found else {
        return Ok(None);
    };
    let Some(start) = list.pin(start) else {
        return Ok(None);
    };
    let Some(end) = list.pin(end) else {
        list.unpin(start);
        return Ok(None);
    };
    Ok(Some(Match { start, end }))
}

/// What [`find_all`] found.
#[derive(Debug, PartialEq, Eq)]
pub struct Matches {
    /// The first matches, in order, pinned: as many as the search's limit
    /// at most.
    pub pinned: Vec<Match>,
    /// How many matches there are, those past the limit too.
    pub count: usize,
}

/// The matches of `query` that start in a row of `list` whose index, oldest
/// first, is in `rows`, in order: the matches that [`find`] steps through,
/// found in one pass, for counting them and showing them. The first `limit`
/// are pinned, and the others only counted, as pins cost memory and time to
/// keep. Each logical line that holds one of the rows is read once, whole,
/// and its matches gone through once: in time in proportion to the line's
/// length when each match's search ends near the match, and to its square
/// for a regex that scans to the line's end, as `a.*b|a`, as regex-automata's
/// own iterators do. A range in the middle of a long line reads all of it,
/// so that ranges of a line, searched one after the other, read it each.
///
/// The search stops with [`Cancelled`] as [`find`] does, and reads cold
/// pages as it does. A match whose cells cannot be pinned is left out of
/// the pinned ones.
pub fn find_all(
    list: &mut PageList,
    query: &Query,
    rows: Range<usize>,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<Matches, Cancelled> {
    let total = list.total_rows();
    let rows = rows.start.min(total)..rows.end.min(total);
    let (found, count) = line::find_all(list, rows, query, limit, cancel)?;
    let pinned = found
        .into_iter()
        .filter_map(|(start, end)| {
            let start = list.pin(start)?;
            let Some(end) = list.pin(end) else {
                list.unpin(start);
                return None;
            };
            Some(Match { start, end })
        })
        .collect();
    Ok(Matches { pinned, count })
}

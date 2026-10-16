//! Literal and regex search over the terminal grid, scrollback included.
//!
//! Soft-wrapped rows are searched as a single logical line, so matches that
//! cross the right margin are still found.

#![forbid(unsafe_code)]

mod line;
mod query;

pub use query::{Case, PatternError, Query};

use std::fmt;
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
/// read as spaces, but those that end a line are left out. The matches of
/// a line are its leftmost-first matches, found from its start, none of
/// them overlapping another, for literal text and regexes alike, so that
/// both ways go through the same matches. A match starts at a cell and
/// ends with one: one that starts or ends inside a cluster does not count,
/// nor does an empty match.
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

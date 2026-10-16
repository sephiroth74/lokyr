//! Literal and regex search over the terminal grid, scrollback included.
//!
//! Soft-wrapped rows are searched as a single logical line, so matches that
//! cross the right margin are still found.

#![forbid(unsafe_code)]

mod line;
mod literal;

pub use literal::{Case, Literal};

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

/// Finds `literal` in `list`: forward, the first match that starts at
/// `from` or after it; backward, the last one that starts before it. The
/// text of a logical line, whose soft-wrapped rows are joined, is searched
/// whole, so that a match can cross the right margin; empty cells read as
/// spaces, but those that end a line are left out.
///
/// The pages that it reads are warmed first. `from` is a cell of the list;
/// a position that is not in it finds nothing.
pub fn find(
    list: &mut PageList,
    literal: &Literal,
    from: Position,
    direction: Direction,
) -> Option<Match> {
    let index = list.row_index(from.page, from.row)?;
    list.warm(0..list.total_rows());
    let (start, end) = match direction {
        Direction::Forward => line::find_forward(list, index, literal, from)?,
        Direction::Backward => line::find_backward(list, index, literal, from)?,
    };
    let start = list.pin(start)?;
    let Some(end) = list.pin(end) else {
        list.unpin(start);
        return None;
    };
    Some(Match { start, end })
}

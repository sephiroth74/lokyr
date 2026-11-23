//! A pane's primary screen and its scrollback kept as the bytes that write
//! them again, which a terminal reads back through its parser as though a
//! program printed them:
//! - each row's text, of SGR for its styles and OSC 8 for its links;
//! - a soft wrap as no line feed, so that a terminal of another width
//!   reflows the line;
//! - the commands' marks of OSC 133, their exit status, their line, the
//!   folder where they started of OSC 7, and the times of their start and
//!   end, of Lokyr's own options.
//!
//! The images are left out, as are the shell's prompt where the user types,
//! which a new shell draws again, and the empty rows that end the screen;
//! what its row holds before it, as output of no last line break that
//! bash's prompt follows, is kept.
//! The input that a program hides, as a password, never reaches the grid.

use std::fmt::Write as _;

use super::Terminal;
use super::marks::ArchivedMark;
use super::reports::sgr;
use super::selection::{row_at, style_at};
use crate::page::{Content, ListRow, RowFlag, Style, StyleId, Width};

impl Terminal {
    /// The primary screen's scrollback and screen as the escape sequences
    /// that write them again into a terminal of any width, from the newest
    /// rows back, within `bound` bytes; nothing of a screen of no text. The
    /// bytes end at the last row's end, of the pen reset, the cursor there;
    /// or, where a command's end is marked on the row of the prompt left
    /// out, at the start of the row after, of that end.
    pub fn archive(&self, bound: usize) -> Vec<u8> {
        let rows = &self.primary.rows;
        let marks = self.archived_marks();
        // The rows up to the prompt where the user types, and its row's
        // cells and marks before it, as output of no last line break leaves
        // them there; of no such prompt, every row but the empty ones that
        // end them.
        let live = self.live_prompt();
        let mut end = live.map_or(rows.total_rows(), |(index, _)| index);
        let partial = live.and_then(|(index, col)| {
            let row = row_at(rows, index)?;
            let limit = text_end(row).min(col);
            let mut own = marks
                .iter()
                .filter(|mark| mark.index == index && mark.col < col)
                .peekable();
            if limit == 0 && own.peek().is_none() {
                return None;
            }
            Some((row_bytes(row, own, true, Some(limit)), col, limit))
        });
        if partial.is_none() {
            while end > 0
                && row_at(rows, end - 1).is_some_and(is_blank)
                && !marks.iter().any(|mark| mark.index == end - 1)
            {
                end -= 1;
            }
        }
        // Each row's bytes, the newest first, as many as the bound holds.
        let mut chunks: Vec<String> = Vec::new();
        let mut size = 0;
        let mut first = end;
        let partial = partial.filter(|(chunk, ..)| chunk.len() <= bound);
        if let Some((chunk, ..)) = &partial {
            size += chunk.len();
            chunks.push(chunk.clone());
        }
        for index in (0..end).rev() {
            let Some(row) = row_at(rows, index) else {
                break;
            };
            let at = marks.partition_point(|mark| mark.index < index);
            let own = marks[at..].iter().take_while(|mark| mark.index == index);
            let last = index + 1 == end && partial.is_none();
            let chunk = row_bytes(row, own, last, None);
            if size + chunk.len() > bound {
                break;
            }
            size += chunk.len();
            chunks.push(chunk);
            first = index;
        }
        if chunks.is_empty() {
            return Vec::new();
        }
        let mut bytes = String::with_capacity(size + 64);
        for chunk in chunks.iter().rev() {
            bytes.push_str(chunk);
        }
        // The marks written, of the rows kept and of the prompt's row before
        // it.
        let cut = partial.as_ref().map(|&(_, col, _)| col);
        let written = |mark: &ArchivedMark| {
            (first..end).contains(&mark.index)
                || cut.is_some_and(|col| mark.index == end && mark.col < col)
        };
        // The ends of commands kept whose marks are on the rows left out,
        // as a shell marks a command's end where its next prompt starts.
        let kept = |order: (usize, u8)| {
            marks
                .iter()
                .any(|mark| mark.order.0 == order.0 && written(mark))
        };
        let ends: Vec<&ArchivedMark> = marks
            .iter()
            .filter(|mark| mark.index >= end && !written(mark) && mark.ends() && kept(mark.order))
            .collect();
        if !ends.is_empty() {
            match partial.as_ref().map(|&(_, _, limit)| limit) {
                // At their columns on the prompt's row, after its cells.
                Some(limit) => {
                    let mut at = limit;
                    for mark in ends {
                        if mark.index == end && mark.col > at {
                            let _ = write!(bytes, "\x1b[{}C", mark.col - at);
                            at = mark.col;
                        }
                        bytes.push_str(&mark.bytes);
                    }
                }
                None => {
                    bytes.push_str("\r\n");
                    for mark in ends {
                        bytes.push_str(&mark.bytes);
                    }
                }
            }
        }
        bytes.push_str("\x1b[0m");
        bytes.into_bytes()
    }
}

/// Whether `row` holds nothing: no character, no background and no style
/// of its own in any cell.
fn is_blank(row: ListRow<'_>) -> bool {
    row.cells()
        .all(|cell| cell.is_empty() && cell.style() == StyleId::DEFAULT)
}

/// The column past `row`'s last cell that holds something: a character, a
/// background or a style of its own.
fn text_end(row: ListRow<'_>) -> u16 {
    let mut end = 0;
    for (x, cell) in (0..).zip(row.cells()) {
        if !cell.is_empty() || cell.style() != StyleId::DEFAULT {
            end = x + 1;
        }
    }
    end
}

/// The bytes that write `row` again, of `marks`, its own in the order of
/// their columns, the pen and the link set anew at its start: its cells up
/// to its last that holds something, or all of a row that soft-wraps, so
/// that the next row's text wraps on from it, or up to `limit`, of the
/// prompt's row; the marks past its text at their columns, the cursor
/// moved there; a line feed after a row that does not wrap, but the
/// archive's `last`.
fn row_bytes<'a>(
    row: ListRow<'_>,
    marks: impl Iterator<Item = &'a ArchivedMark>,
    last: bool,
    limit: Option<u16>,
) -> String {
    let cols = row.cols();
    let wraps = row.metadata().has(RowFlag::SoftWrapped);
    let end = match limit {
        Some(limit) => limit,
        None if wraps && !last => cols,
        None => text_end(row),
    };
    let mut bytes = String::new();
    let mut style: Option<Style> = None;
    let mut link: Option<&[u8]> = None;
    let mut marks = marks.peekable();
    for x in 0..end {
        while let Some(mark) = marks.next_if(|mark| mark.col <= x) {
            bytes.push_str(&mark.bytes);
        }
        let cell = row.cell(x);
        // A wide character's second half, and the spacer that one which went
        // on in the next row left at the row's end, which writing it does
        // again.
        match cell.width() {
            Width::SpacerTail => continue,
            Width::SpacerHead if x + 1 == cols => continue,
            _ => {}
        }
        let shown = row.link(x);
        if shown != link {
            bytes.push_str("\x1b]8;;");
            if let Some(uri) = shown {
                bytes.push_str(&String::from_utf8_lossy(uri));
            }
            bytes.push_str("\x1b\\");
            link = shown;
        }
        let pen = style_at(row, x);
        if style != Some(pen) {
            let _ = write!(bytes, "\x1b[{}m", sgr(&pen));
            style = Some(pen);
        }
        match cell.content() {
            Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => bytes.push(' '),
            Content::Char(character) => bytes.push(character),
            Content::Grapheme(first) => {
                bytes.push(first);
                bytes.extend(row.grapheme(x));
            }
        }
    }
    if link.is_some() {
        bytes.push_str("\x1b]8;;\x1b\\");
    }
    // A row that a soft wrap made took the background of its first cell's
    // pen as the terminal scrolled it in, which its empty cells past its
    // text must not keep; a row that a line feed made took the default's,
    // as the pen is reset before each.
    let colored = |x: u16| style_at(row, x).bg != Style::default().bg;
    if end < cols && row.metadata().has(RowFlag::Continuation) && colored(0) {
        bytes.push_str("\x1b[0m\x1b[K");
        style = Some(Style::default());
    }
    // The marks past the cells written, the cursor moved to each over
    // cells that it leaves as they are.
    let mut at = end;
    for mark in marks {
        if mark.col > at {
            let _ = write!(bytes, "\x1b[{}C", mark.col - at);
            at = mark.col;
        }
        bytes.push_str(&mark.bytes);
    }
    if !wraps && !last {
        // The row that the line feed scrolls in takes the pen's background.
        if style.is_some_and(|pen| pen != Style::default()) {
            bytes.push_str("\x1b[0m");
        }
        bytes.push_str("\r\n");
    }
    bytes
}

#[cfg(test)]
mod tests;

//! The moves of the copy mode's cursor through a pane's text, as vi's keys
//! move: by words, through the rows that a line wraps over and from a line
//! to the next; to a line's start, its first character, or its last; and by
//! paragraphs, the runs of lines between blank ones. They read the text by
//! its logical lines, each of every cell of its rows, as a read of a line's
//! text gives it.

use lokyr_plugin_sdk::host::LineText;

/// The rows that a word's move reads around a row at a time: a few, as a
/// line of many rows costs no more, its next rows read as the move gets
/// there.
const REACH: usize = 8;

/// The rows of a line that a move to its start or its end reads around the
/// cursor's, at most.
pub const LINE_REACH: usize = 256;

/// The most lines that a paragraph's move goes through, past which it stops
/// where it got to.
const MOST_LINES: usize = 10_000;

/// Where the text reaches: the rows from the oldest that the pane holds to
/// the screen's last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// The number of the oldest row.
    pub oldest: i64,
    /// The number of the screen's last row.
    pub last: i64,
}

/// What the moves read of a pane's text.
pub trait Text {
    /// The logical line of the row numbered `row`, as far as `reach` rows
    /// before it and after it, each of its characters with its cell; `None`
    /// for a row that the pane does not hold.
    fn line(&mut self, row: i64, reach: usize) -> Option<LineText>;
}

/// What a character is to a word's move: a blank, which parts words, or of
/// a word, of letters, digits and `_`, or of punctuation, each a word of
/// its own; of a big word, any but a blank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Blank,
    Word,
    Punctuation,
}

/// The class of `character`, of a big word's move or not.
fn class(character: char, big: bool) -> Class {
    if character.is_whitespace() || character == '\0' {
        Class::Blank
    } else if big || character.is_alphanumeric() || character == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// A place in the text, of a character, which steps to the one before or
/// after it, reading the rows on as it reaches the edge of those that it
/// read.
struct Walker<'t, T: Text> {
    text: &'t mut T,
    bounds: Bounds,
    /// The characters read, one of each cell: a grapheme's first.
    chars: Vec<char>,
    /// Their cells.
    cells: Vec<(u16, i64)>,
    /// The character that it is at.
    at: usize,
}

impl<'t, T: Text> Walker<'t, T> {
    /// At the cell `from`, or at the character whose cells it is in, as a
    /// wide character's second column.
    fn new(text: &'t mut T, bounds: Bounds, (column, row): (u16, i64)) -> Option<Self> {
        let line = text.line(row, REACH)?;
        let mut walker = Self {
            text,
            bounds,
            chars: Vec::new(),
            cells: Vec::new(),
            at: 0,
        };
        walker.load(line);
        walker.at = walker
            .cells
            .iter()
            .rposition(|&(other, at)| (at, other) <= (row, column))
            .unwrap_or(0);
        (!walker.cells.is_empty()).then_some(walker)
    }

    /// Reads `line` in place of the rows before, a character of each cell.
    fn load(&mut self, line: LineText) {
        self.chars.clear();
        self.cells.clear();
        for (character, cell) in line.text.chars().zip(line.cells) {
            if self.cells.last() != Some(&cell) {
                self.chars.push(character);
                self.cells.push(cell);
            }
        }
    }

    /// The cell of its character.
    fn cell(&self) -> (u16, i64) {
        self.cells[self.at]
    }

    /// Its character's class.
    fn class(&self, big: bool) -> Class {
        class(self.chars[self.at], big)
    }

    /// Steps to the next character, past the rows read to the next row's:
    /// whether it went into another logical line; `None` at the text's end,
    /// where it stays.
    fn forward(&mut self) -> Option<bool> {
        if self.at + 1 < self.cells.len() {
            self.at += 1;
            return Some(false);
        }
        let row = self.cells.last()?.1 + 1;
        if row > self.bounds.last {
            return None;
        }
        let line = self.text.line(row, REACH)?;
        // A row whose line starts above it goes on with the line before.
        let started = line.cells.first().is_some_and(|&(_, first)| first >= row);
        self.load(line);
        self.at = self.cells.iter().position(|&(_, at)| at >= row)?;
        Some(started)
    }

    /// Steps to the character before, past the rows read to the row
    /// before's: whether it went into another logical line; `None` at the
    /// text's start, where it stays.
    fn backward(&mut self) -> Option<bool> {
        if self.at > 0 {
            self.at -= 1;
            return Some(false);
        }
        let row = self.cells.first()?.1 - 1;
        if row < self.bounds.oldest {
            return None;
        }
        let line = self.text.line(row, REACH)?;
        // A row whose line goes on below it is of the line after.
        let ended = line.cells.last().is_some_and(|&(_, last)| last <= row);
        self.load(line);
        self.at = self.cells.iter().rposition(|&(_, at)| at <= row)?;
        Some(ended)
    }
}

/// The start of the next word after `from`, of a big word or not, as vi's
/// `w` and `W` move: past the word that `from` is in and the blanks after
/// it, a line's end parting words; at the text's end, the last word's end.
pub fn word_next(text: &mut impl Text, bounds: Bounds, from: (u16, i64), big: bool) -> (u16, i64) {
    let Some(mut walker) = Walker::new(text, bounds, from) else {
        return from;
    };
    let first = walker.class(big);
    let mut solid = from;
    if first != Class::Blank {
        loop {
            solid = walker.cell();
            match walker.forward() {
                None => return walker.cell(),
                Some(crossed) if crossed || walker.class(big) != first => break,
                Some(_) => {}
            }
        }
    }
    while walker.class(big) == Class::Blank {
        if walker.forward().is_none() {
            return solid;
        }
    }
    walker.cell()
}

/// The start of the word before `from`, or of the word that it is in past
/// its start, of a big word or not, as vi's `b` and `B` move.
pub fn word_previous(
    text: &mut impl Text,
    bounds: Bounds,
    from: (u16, i64),
    big: bool,
) -> (u16, i64) {
    let Some(mut walker) = Walker::new(text, bounds, from) else {
        return from;
    };
    if walker.backward().is_none() {
        return walker.cell();
    }
    while walker.class(big) == Class::Blank {
        if walker.backward().is_none() {
            return walker.cell();
        }
    }
    let class = walker.class(big);
    loop {
        match walker.backward() {
            None => return walker.cell(),
            Some(crossed) if crossed || walker.class(big) != class => {
                walker.forward();
                return walker.cell();
            }
            Some(_) => {}
        }
    }
}

/// The end of the word that `from` is in, past its end, or of the next
/// word, of a big word or not, as vi's `e` and `E` move; at the text's end
/// with no word after it, `from`.
pub fn word_end(text: &mut impl Text, bounds: Bounds, from: (u16, i64), big: bool) -> (u16, i64) {
    let Some(mut walker) = Walker::new(text, bounds, from) else {
        return from;
    };
    if walker.forward().is_none() {
        return from;
    }
    while walker.class(big) == Class::Blank {
        if walker.forward().is_none() {
            return from;
        }
    }
    let class = walker.class(big);
    loop {
        match walker.forward() {
            None => return walker.cell(),
            Some(crossed) if crossed || walker.class(big) != class => {
                walker.backward();
                return walker.cell();
            }
            Some(_) => {}
        }
    }
}

/// Where in the logical line of `from`'s row a move to its edge goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Its first cell, as vi's `0`.
    Start,
    /// Its first character but a blank, as vi's `^`.
    First,
    /// Its last character but a blank, as vi's `$`.
    Last,
}

/// The cell of `edge` of the logical line of `from`'s row, as far as
/// [`LINE_REACH`] rows around it; of a blank line, its first cell.
pub fn line_edge(text: &mut impl Text, from: (u16, i64), edge: Edge) -> (u16, i64) {
    let Some(line) = text.line(from.1, LINE_REACH) else {
        return from;
    };
    let Some(&start) = line.cells.first() else {
        return from;
    };
    let mut solid = line
        .text
        .chars()
        .zip(&line.cells)
        .filter(|(character, _)| class(*character, true) != Class::Blank)
        .map(|(_, &cell)| cell);
    match edge {
        Edge::Start => start,
        Edge::First => solid.next().unwrap_or(start),
        Edge::Last => solid.last().unwrap_or(start),
    }
}

/// Whether `line` holds nothing but blanks.
fn blank(line: &LineText) -> bool {
    line.text
        .chars()
        .all(|character| class(character, true) == Class::Blank)
}

/// The first cell of the next blank line after `from`'s that ends a
/// paragraph, past the blank lines before it, as vi's `}` moves, or of the
/// line before `from`'s, as `{` moves, `up`; the text's last row, or its
/// first, of none.
pub fn paragraph(text: &mut impl Text, bounds: Bounds, from: (u16, i64), up: bool) -> (u16, i64) {
    let edge = if up {
        (0, bounds.oldest)
    } else {
        (0, bounds.last)
    };
    let Some(line) = text.line(from.1, REACH) else {
        return from;
    };
    let mut solid = !blank(&line);
    let mut cells = (line.cells.first().copied(), line.cells.last().copied());
    for _ in 0..MOST_LINES {
        let (Some(first), Some(last)) = cells else {
            return from;
        };
        let row = if up { first.1 - 1 } else { last.1 + 1 };
        if row < bounds.oldest || row > bounds.last {
            return edge;
        }
        let Some(line) = text.line(row, REACH) else {
            return edge;
        };
        let empty = blank(&line);
        if empty && solid {
            return (0, row);
        }
        solid |= !empty;
        cells = (line.cells.first().copied(), line.cells.last().copied());
    }
    cells.0.unwrap_or(from)
}

/// The first cell of the last row that shows something, or of `cursor`'s,
/// the terminal's cursor's row, which is later, as vi's `G` moves: the
/// empty rows below them are left out.
pub fn bottom(text: &mut impl Text, bounds: Bounds, cursor: i64) -> (u16, i64) {
    let cursor = cursor.clamp(bounds.oldest, bounds.last);
    let shown = (cursor + 1..=bounds.last)
        .rev()
        .find(|&row| text.line(row, 0).is_some_and(|line| !blank(&line)));
    (0, shown.unwrap_or(cursor))
}

#[cfg(test)]
mod tests;

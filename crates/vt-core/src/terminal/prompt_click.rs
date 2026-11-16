//! A click in the shell's command line, which moves its line editor's
//! cursor there, as the shell's prompt announces (OSC 133 `A`): kitty's
//! click events, `click_events=1` or `2`, which hand the click itself to
//! the shell, of xterm's SGR encoding
//! (<https://sw.kovidgoyal.net/kitty/shell-integration/>); or the
//! semantic prompts' `cl=`, by which the terminal sends the arrows that
//! move the cursor, a left or a right arrow a character, a wide one too
//! (<https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>).

use super::Terminal;
use crate::page::{Content, RowFlag, Semantic, Width};

/// The most arrows that a click sends.
const MOST_ARROWS: usize = 4096;

/// How the shell's line editor takes a click in its command line, as its
/// prompt announced it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptClicks {
    /// The click itself, of SGR's encoding, of its row on the screen
    /// (`click_events=1`), or from the prompt's first (`click_events=2`).
    Events {
        /// Whether its row counts from the prompt's first.
        relative: bool,
    },
    /// The left and right arrows within the cursor's line, a long one
    /// wrapped over rows too (`cl=line`).
    Line,
    /// The left and right arrows across the command's lines, a line's end
    /// one arrow (`cl=m`, and `v` and `w`, whose up and down arrows leave
    /// a command's first line for the history in some shells).
    Lines,
}

/// What a click in the command line sends the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptClick {
    /// The left arrow, this many times.
    Left(usize),
    /// The right arrow, this many times.
    Right(usize),
    /// The click, at its column and row from 1, as SGR's encoding gives
    /// them.
    Event {
        /// Its column.
        col: u16,
        /// Its row.
        row: u16,
    },
}

/// A step of the cursor through the command line: a character's cell, or
/// the end of a line that does not wrap into the next row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Char { x: u16, y: u16 },
    Break { y: u16 },
}

impl Step {
    /// Where it is, in reading order: a line's end past its row's cells.
    fn place(self) -> (u16, u32) {
        match self {
            Self::Char { x, y } => (y, u32::from(x)),
            Self::Break { y } => (y, u32::MAX),
        }
    }
}

impl Terminal {
    /// How the shell's line editor takes a click in its command line, as
    /// its last prompt announced it; none of one that announced nothing.
    pub fn prompt_clicks(&self) -> Option<PromptClicks> {
        self.prompt_clicks
    }

    /// Takes the options of a prompt's start, `A`, of how its line editor
    /// takes a click: a secondary prompt's, `k=s`, change nothing; click
    /// events before `cl`.
    pub(super) fn take_prompt_options<'a>(&mut self, options: impl Iterator<Item = &'a [u8]>) {
        let mut events = None;
        let mut arrows = None;
        for option in options {
            if option == b"k=s" {
                return;
            }
            if let Some(value) = option.strip_prefix(b"click_events=") {
                events = match value {
                    b"1" => Some(PromptClicks::Events { relative: false }),
                    b"2" => Some(PromptClicks::Events { relative: true }),
                    _ => None,
                };
            } else if let Some(value) = option.strip_prefix(b"cl=") {
                arrows = match value {
                    b"line" => Some(PromptClicks::Line),
                    b"m" | b"v" | b"w" => Some(PromptClicks::Lines),
                    _ => None,
                };
            }
        }
        self.prompt_clicks = events.or(arrows);
    }

    /// What a click of the left button at column `col` of the row numbered
    /// `row` sends the shell, as its prompt announced, while the shell
    /// edits its command line (after `B`, before `C`) on the primary
    /// screen: the click itself, on or below the prompt's first row; or
    /// the arrows from the cursor to the clicked character, within the
    /// command line, which a click past its end takes to its end, of the
    /// cursor's line alone with `cl=line`. None otherwise, nor of a shell
    /// that announced nothing, nor of a click on the prompt.
    pub fn prompt_click(&self, col: u16, row: i64) -> Option<PromptClick> {
        let clicks = self.prompt_clicks?;
        if self.alternate_active || self.zone != Semantic::Input {
            return None;
        }
        let y = self.screen_y(row)?;
        let col = col.min(self.cols().saturating_sub(1));
        let command = self.commands().last().copied();
        let on_screen = |mark: Option<(u16, i64)>| {
            let (x, row) = mark?;
            Some((x, self.screen_y(row)?))
        };
        let cursor = self.cursor();
        let top = on_screen(command.and_then(|command| command.prompt))
            .map(|(_, y)| y)
            .or_else(|| self.prompt_row(cursor.y))?;
        if y < top {
            return None;
        }
        let relative = match clicks {
            PromptClicks::Events { relative } => relative,
            PromptClicks::Line | PromptClicks::Lines => {
                let start =
                    on_screen(command.and_then(|command| command.input)).unwrap_or((0, top));
                return self.arrows(clicks, start, (col, y));
            }
        };
        Some(PromptClick::Event {
            col: col + 1,
            row: if relative { y - top + 1 } else { y + 1 },
        })
    }

    /// The arrows of `clicks` from the cursor to `(x, y)` of the screen,
    /// in the command line that starts at `start`.
    fn arrows(
        &self,
        clicks: PromptClicks,
        (start_x, start_y): (u16, u16),
        (x, y): (u16, u16),
    ) -> Option<PromptClick> {
        if (y, x) < (start_y, start_x) {
            return None;
        }
        let rows = &self.screen().rows;
        let cursor = self.cursor();
        let mut steps = Vec::new();
        for row_y in start_y..self.rows() {
            let (page, row) = rows.screen_row(row_y);
            let from = if row_y == start_y { start_x } else { 0 };
            for (cell_x, cell) in page.cells(row).iter().enumerate().skip(usize::from(from)) {
                if cell.semantic() == Semantic::Input
                    && !matches!(cell.width(), Width::SpacerTail | Width::SpacerHead)
                {
                    steps.push((
                        Step::Char {
                            x: u16::try_from(cell_x).ok()?,
                            y: row_y,
                        },
                        has_character(cell.content()),
                    ));
                }
            }
            if !page.row(row).has(RowFlag::SoftWrapped) {
                steps.push((Step::Break { y: row_y }, false));
            }
        }
        // The command line ends at its last character, or at the cursor
        // past it, as the blank cells that a line's erase leaves show none.
        let cursor_place = (cursor.y, u32::from(cursor.x));
        let before = |place: (u16, u32)| {
            steps
                .iter()
                .filter(|(step, _)| step.place() < place)
                .count()
        };
        let last = steps
            .iter()
            .rposition(|&(_, character)| character)
            .map_or(0, |at| at + 1);
        let end = last.max(before(cursor_place));
        let steps = &steps[..end.min(steps.len())];
        let count_before = |place: (u16, u32)| {
            steps
                .iter()
                .filter(|(step, _)| step.place() < place)
                .count()
        };
        // A click below the command line's last row moves nothing.
        let last_row = steps.last().map_or(start_y, |(step, _)| step.place().0);
        if y > last_row.max(cursor.y) {
            return None;
        }
        // The second half of a wide character is its first's.
        let (page, row) = rows.screen_row(y);
        let x = match page.cells(row).get(usize::from(x)).map(|cell| cell.width()) {
            Some(Width::SpacerTail) => x.saturating_sub(1),
            _ => x,
        };
        if clicks == PromptClicks::Line {
            let line_of = |row_y: u16| {
                steps
                    .iter()
                    .filter(|(step, _)| matches!(step, Step::Break { y } if *y < row_y))
                    .count()
            };
            if line_of(y) != line_of(cursor.y) {
                return None;
            }
        }
        let from = count_before(cursor_place);
        let to = count_before((y, u32::from(x)));
        match to.cmp(&from) {
            std::cmp::Ordering::Greater => Some(PromptClick::Right((to - from).min(MOST_ARROWS))),
            std::cmp::Ordering::Less => Some(PromptClick::Left((from - to).min(MOST_ARROWS))),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// The row of the screen of the row numbered `row`, if it is on the
    /// screen.
    fn screen_y(&self, row: i64) -> Option<u16> {
        let index = self.row_index(row)?;
        let scrollback = self.screen().rows.scrollback_rows();
        u16::try_from(index.checked_sub(scrollback)?)
            .ok()
            .filter(|&y| y < self.rows())
    }

    /// The nearest row at or above `y` where a prompt starts.
    fn prompt_row(&self, y: u16) -> Option<u16> {
        let rows = &self.screen().rows;
        (0..=y).rev().find(|&row_y| {
            let (page, row) = rows.screen_row(row_y);
            page.row(row).has(RowFlag::PromptStart)
        })
    }
}

/// Whether a cell of `content` shows a character, a space too.
fn has_character(content: Content) -> bool {
    matches!(content, Content::Char(c) | Content::Grapheme(c) if c != '\0')
}

#[cfg(test)]
mod tests;

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
        let (start, top) = self.command_line_start()?;
        let y = self.screen_y(row)?;
        let col = col.min(self.cols().saturating_sub(1));
        if y < top {
            return None;
        }
        let relative = match clicks {
            PromptClicks::Events { relative } => relative,
            PromptClicks::Line | PromptClicks::Lines => {
                if (y, col) < (start.1, start.0) {
                    return None;
                }
                let line = self.edited_line(start);
                let cursor = self.cursor();
                // A click below the command line's last row moves nothing.
                if y > line.last_row().unwrap_or(start.1).max(cursor.y) {
                    return None;
                }
                let x = self.character_x(col, y);
                if clicks == PromptClicks::Line && line.line_of(y) != line.line_of(cursor.y) {
                    return None;
                }
                return arrows(
                    line.before((cursor.y, u32::from(cursor.x))),
                    line.before((y, u32::from(x))),
                );
            }
        };
        Some(PromptClick::Event {
            col: col + 1,
            row: if relative { y - top + 1 } else { y + 1 },
        })
    }

    /// What replaces the selection, which a key that deletes or types
    /// takes the place of, where it lies within the command line that the
    /// shell edits, as its prompt announced that its line editor takes the
    /// arrows or the clicks: the arrows from the cursor to the selection's
    /// end, then as many Backspaces as it holds characters; within the
    /// cursor's line but of `cl=` of several. None otherwise, as of a
    /// selection that takes some of the prompt or of a rectangle.
    pub fn command_line_selection(&mut self) -> Option<(Option<PromptClick>, usize)> {
        let clicks = self.prompt_clicks?;
        let selected = self.selection()?;
        if selected.shape != super::SelectionShape::Cells {
            return None;
        }
        let (start, _) = self.command_line_start()?;
        let origin = self.origin();
        let first = (
            selected.start.0,
            self.screen_y(origin.top + selected.start.1)?,
        );
        let last = (selected.end.0, self.screen_y(origin.top + selected.end.1)?);
        if (first.1, first.0) < (start.1, start.0) {
            return None;
        }
        let line = self.edited_line(start);
        let cursor = self.cursor();
        if clicks != PromptClicks::Lines
            && (line.line_of(first.1) != line.line_of(cursor.y)
                || line.line_of(last.1) != line.line_of(cursor.y))
        {
            return None;
        }
        let from = line.before((first.1, u32::from(self.character_x(first.0, first.1))));
        let to = line.before((last.1, u32::from(last.0) + 1));
        let deletes = to.checked_sub(from).filter(|&deletes| deletes > 0)?;
        Some((
            arrows(line.before((cursor.y, u32::from(cursor.x))), to),
            deletes.min(MOST_ARROWS),
        ))
    }

    /// Where the command line that the shell edits starts, `B`'s mark, and
    /// the row where its prompt starts, while it edits it on the primary
    /// screen; none otherwise.
    fn command_line_start(&self) -> Option<((u16, u16), u16)> {
        if self.alternate_active || self.zone != Semantic::Input {
            return None;
        }
        let command = self.commands().last().copied();
        let on_screen = |mark: Option<(u16, i64)>| {
            let (x, row) = mark?;
            Some((x, self.screen_y(row)?))
        };
        let top = on_screen(command.and_then(|command| command.prompt))
            .map(|(_, y)| y)
            .or_else(|| self.prompt_row(self.cursor().y))?;
        let start = on_screen(command.and_then(|command| command.input)).unwrap_or((0, top));
        Some((start, top))
    }

    /// The command line from `start`: its characters, the cells that the
    /// shell printed since `B`, and the ends of its lines that do not wrap
    /// into the next row, to its last character, or to the cursor past it,
    /// as the blank cells that a line's erase leaves show none.
    fn edited_line(&self, (start_x, start_y): (u16, u16)) -> CommandLine {
        let rows = &self.screen().rows;
        let mut steps = Vec::new();
        for row_y in start_y..self.rows() {
            let (page, row) = rows.screen_row(row_y);
            let from = if row_y == start_y { start_x } else { 0 };
            for (cell_x, cell) in page.cells(row).iter().enumerate().skip(usize::from(from)) {
                if cell.semantic() == Semantic::Input
                    && !matches!(cell.width(), Width::SpacerTail | Width::SpacerHead)
                    && let Ok(x) = u16::try_from(cell_x)
                {
                    steps.push((Step::Char { x, y: row_y }, has_character(cell.content())));
                }
            }
            if !page.row(row).has(RowFlag::SoftWrapped) {
                steps.push((Step::Break { y: row_y }, false));
            }
        }
        let cursor = self.cursor();
        let past_cursor = steps
            .iter()
            .filter(|(step, _)| step.place() < (cursor.y, u32::from(cursor.x)))
            .count();
        let last = steps
            .iter()
            .rposition(|&(_, character)| character)
            .map_or(0, |at| at + 1);
        steps.truncate(last.max(past_cursor));
        CommandLine {
            steps: steps.into_iter().map(|(step, _)| step).collect(),
        }
    }

    /// The column of the character at column `x` of the screen's row `y`:
    /// a wide character's second half is its first's.
    fn character_x(&self, x: u16, y: u16) -> u16 {
        let (page, row) = self.screen().rows.screen_row(y);
        match page.cells(row).get(usize::from(x)).map(|cell| cell.width()) {
            Some(Width::SpacerTail) => x.saturating_sub(1),
            _ => x,
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

/// The command line's steps, in reading order.
struct CommandLine {
    steps: Vec<Step>,
}

impl CommandLine {
    /// How many steps are before `place`, in reading order.
    fn before(&self, place: (u16, u32)) -> usize {
        self.steps
            .iter()
            .filter(|step| step.place() < place)
            .count()
    }

    /// The line of the screen's row `y`: how many lines end before it.
    fn line_of(&self, y: u16) -> usize {
        self.steps
            .iter()
            .filter(|step| matches!(step, Step::Break { y: end } if *end < y))
            .count()
    }

    /// The row of its last step.
    fn last_row(&self) -> Option<u16> {
        self.steps.last().map(|step| step.place().0)
    }
}

/// The arrows from the step `from` to the step `to`, of
/// [`MOST_ARROWS`] at most; none between the same steps.
fn arrows(from: usize, to: usize) -> Option<PromptClick> {
    match to.cmp(&from) {
        std::cmp::Ordering::Greater => Some(PromptClick::Right((to - from).min(MOST_ARROWS))),
        std::cmp::Ordering::Less => Some(PromptClick::Left((from - to).min(MOST_ARROWS))),
        std::cmp::Ordering::Equal => None,
    }
}

/// Whether a cell of `content` shows a character, a space too.
fn has_character(content: Content) -> bool {
    matches!(content, Content::Char(c) | Content::Grapheme(c) if c != '\0')
}

#[cfg(test)]
mod tests;

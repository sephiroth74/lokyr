//! A click in the shell's command line, which moves its line editor's
//! cursor there, as the shell's prompt announces (OSC 133 `A`): kitty's
//! click events, `click_events=1` or `2`, which hand the click itself to
//! the shell, of xterm's SGR encoding
//! (<https://sw.kovidgoyal.net/kitty/shell-integration/>); or the
//! semantic prompts' `cl=`, by which the terminal sends the arrows that
//! move the cursor, a left or a right arrow a character, a wide one too
//! (<https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>).
//!
//! A right prompt (`P;k=r`) is no part of the command line: its zone is
//! the prompt's from its start until the input goes on, `B`, or the text
//! printed next comes before it, as the shell's cursor went back to its
//! line, as the proposal's end of the line has it.

use super::Terminal;
use crate::page::{Cell, Content, RowFlag, Semantic, Width};

/// The most arrows that a click sends, and the most arrows and deletes
/// that a selection's replacement sends, past which it sends none.
const MOST_ARROWS: usize = 4096;

/// A right prompt that the shell draws: where it started, on the screen,
/// and the zone before it, which the input's going on gives back.
#[derive(Clone, Copy, Debug)]
pub(super) struct RightPrompt {
    x: u16,
    y: u16,
    zone: Semantic,
}

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

/// A step of the cursor through the command line: a character's cell, of
/// a grapheme of several code points or not, or the end of a line that
/// does not wrap into the next row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Char { x: u16, y: u16, grapheme: bool },
    Break { y: u16 },
}

impl Step {
    /// Where it is, in reading order: a line's end past its row's cells.
    fn place(self) -> (u16, u32) {
        match self {
            Self::Char { x, y, .. } => (y, u32::from(x)),
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

    /// Forgets how the line editor takes a click, as the command's output
    /// or its end comes (`C`, `D`): what reads the keys now is no line
    /// editor that announced it, until the next prompt tells again.
    pub(super) fn forget_prompt_clicks(&mut self) {
        self.prompt_clicks = None;
        self.right_prompt = None;
    }

    /// Starts a right prompt at the cursor (`P;k=r`), of the prompt's zone.
    pub(super) fn start_right_prompt(&mut self) {
        let cursor = self.cursor();
        let zone = self.right_prompt.map_or(self.zone, |right| right.zone);
        self.right_prompt = Some(RightPrompt {
            x: cursor.x,
            y: cursor.y,
            zone,
        });
        self.zone = Semantic::Prompt;
    }

    /// Ends the right prompt, if one, of the zone before it given back:
    /// whether one was.
    pub(super) fn end_right_prompt(&mut self) -> bool {
        let Some(right) = self.right_prompt.take() else {
            return false;
        };
        self.zone = right.zone;
        true
    }

    /// Ends the right prompt as the text printed next comes before it, on
    /// another row or at a column left of its start, as the shell's cursor
    /// went back to its line.
    #[inline]
    pub(super) fn leave_right_prompt(&mut self) {
        if let Some(right) = self.right_prompt {
            let cursor = self.cursor();
            if cursor.y != right.y || cursor.x < right.x {
                self.end_right_prompt();
            }
        }
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
                let (from, to) = (
                    line.before(self.cursor_place()),
                    line.before((y, u32::from(x))),
                );
                // Line editors count a grapheme's code points apart, or
                // not, as each has it: no arrows across one.
                if line.crosses_grapheme(from, to) {
                    return None;
                }
                return arrows(from, to);
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
    /// selection that takes some of the prompt, a right prompt's or other
    /// text not of the input, or a grapheme of several code points, of a
    /// rectangle, or of more than [`MOST_ARROWS`] arrows or deletes.
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
        if self.holds_other_text(start, first, last) {
            return None;
        }
        let from = line.before((first.1, u32::from(self.character_x(first.0, first.1))));
        let to = line.before((last.1, u32::from(last.0) + 1));
        let deletes = to.checked_sub(from).filter(|&deletes| deletes > 0)?;
        let at = line.before(self.cursor_place());
        if deletes > MOST_ARROWS
            || at.abs_diff(to) > MOST_ARROWS
            || line.crosses_grapheme(at.min(from), at.max(to))
        {
            return None;
        }
        Some((arrows(at, to), deletes))
    }

    /// The cursor's place in reading order: past its cell while a wrap is
    /// pending, as the character there is before it.
    fn cursor_place(&self) -> (u16, u32) {
        let cursor = self.cursor();
        (
            cursor.y,
            u32::from(cursor.x) + u32::from(cursor.pending_wrap),
        )
    }

    /// Whether the cells from `first` to `last`, by column and screen's
    /// row, in reading order, of the command line from `start`, hold a
    /// character not of the input: a prompt's, or the input's past the
    /// blank cells that end a row's input, as a right prompt that no mark
    /// tells is; the blank cells after a line's text hold none.
    fn holds_other_text(&self, start: (u16, u16), first: (u16, u16), last: (u16, u16)) -> bool {
        let rows = &self.screen().rows;
        (first.1..=last.1).any(|y| {
            let (page, row) = rows.screen_row(y);
            let cells = page.cells(row);
            let end = if y == start.1 {
                input_end(cells, start.0, false)
            } else {
                input_end(cells, 0, true)
            };
            let from = if y == first.1 { first.0 } else { 0 };
            let to = if y == last.1 { last.0 } else { u16::MAX };
            cells
                .iter()
                .enumerate()
                .skip(usize::from(from))
                .take_while(|&(x, _)| x <= usize::from(to))
                .any(|(x, cell)| {
                    has_character(cell.content())
                        && (cell.semantic() != Semantic::Input || x >= end)
                })
        })
    }

    /// Where the command line that the shell edits starts, `B`'s mark, and
    /// the row where its prompt starts, while it edits it on the primary
    /// screen; none otherwise.
    fn command_line_start(&self) -> Option<((u16, u16), u16)> {
        // A right prompt drawn, of no end told yet, leaves the input.
        let zone = self.right_prompt.map_or(self.zone, |right| right.zone);
        if self.alternate_active || zone != Semantic::Input {
            return None;
        }
        let command = self.last_command();
        let on_screen = |mark: Option<(u16, i64)>| {
            let (x, row) = mark?;
            Some((x, self.screen_y(row)?))
        };
        let top = on_screen(command.as_ref().and_then(|command| command.prompt))
            .map(|(_, y)| y)
            .or_else(|| self.prompt_row(self.cursor().y))?;
        let start =
            on_screen(command.as_ref().and_then(|command| command.input)).unwrap_or((0, top));
        Some((start, top))
    }

    /// The command line from `start`: its characters, the cells that the
    /// shell printed since `B` up to the blank cells that end each row's
    /// input, past which a right prompt that no mark tells is no part of
    /// it, and the ends of its lines that do not wrap into the next row, to
    /// its last character other than a space, or to the cursor past it, as
    /// the blank cells that a line's erase leaves show none and the spaces
    /// that a line editor erases a character with count not.
    fn edited_line(&self, (start_x, start_y): (u16, u16)) -> CommandLine {
        let rows = &self.screen().rows;
        let mut steps = Vec::new();
        for row_y in start_y..self.rows() {
            let (page, row) = rows.screen_row(row_y);
            let from = if row_y == start_y { start_x } else { 0 };
            let cells = page.cells(row);
            let end = input_end(cells, from, row_y != start_y);
            for (cell_x, cell) in cells
                .iter()
                .enumerate()
                .skip(usize::from(from))
                .take(end.saturating_sub(usize::from(from)))
            {
                if cell.semantic() == Semantic::Input
                    && !matches!(cell.width(), Width::SpacerTail | Width::SpacerHead)
                    && let Ok(x) = u16::try_from(cell_x)
                {
                    let grapheme = matches!(cell.content(), Content::Grapheme(_));
                    steps.push((
                        Step::Char {
                            x,
                            y: row_y,
                            grapheme,
                        },
                        shows_text(cell.content()),
                    ));
                }
            }
            if !page.row(row).has(RowFlag::SoftWrapped) {
                steps.push((Step::Break { y: row_y }, false));
            }
        }
        let cursor = self.cursor_place();
        let past_cursor = steps
            .iter()
            .filter(|(step, _)| step.place() < cursor)
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

    /// Whether the steps from the `from`th to the `to`th, either way,
    /// cross a grapheme of several code points.
    fn crosses_grapheme(&self, from: usize, to: usize) -> bool {
        let (low, high) = (from.min(to), from.max(to).min(self.steps.len()));
        self.steps.get(low..high).is_some_and(|passed| {
            passed
                .iter()
                .any(|step| matches!(step, Step::Char { grapheme: true, .. }))
        })
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

/// Whether a cell of `content` shows a character other than a space, as a
/// line editor that erases a character by writing a space over it leaves.
fn shows_text(content: Content) -> bool {
    matches!(content, Content::Char(c) | Content::Grapheme(c) if c != '\0' && c != ' ')
}

/// Where the input of the row of `cells` ends, from the column `from`: at
/// its first blank cell that is no input's nor prompt's, as the semantic
/// prompts' input line holds its input, then a blank area, then a final
/// prompt, which a shell may draw of no mark; the row's end of none. On a
/// row after the command line's first, `continued`, the blank cells
/// before its first input or prompt are none of its end, as a line editor
/// that indents a continuation line by moving the cursor leaves them, as
/// fish does.
fn input_end(cells: &[Cell], from: u16, continued: bool) -> usize {
    let blank = |cell: &Cell| cell.semantic() == Semantic::Output && !has_character(cell.content());
    let first = if continued {
        cells
            .iter()
            .enumerate()
            .skip(usize::from(from))
            .find(|(_, cell)| !blank(cell))
            .map_or(cells.len(), |(x, _)| x)
    } else {
        usize::from(from)
    };
    cells
        .iter()
        .enumerate()
        .skip(first)
        .find(|(_, cell)| blank(cell))
        .map_or(cells.len(), |(x, _)| x)
}

#[cfg(test)]
mod tests;

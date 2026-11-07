//! The commands that the shell marks with OSC 133, its semantic prompts:
//! each command's prompt, where its input starts, where its output starts,
//! where it ended and its exit status, each a cell of the primary screen
//! that a pin follows through output, the scrollback and a reflowing
//! resize, until its row leaves the list; and the view's jumps from prompt
//! to prompt.
//!
//! <https://gitlab.freedesktop.org/Per_Bothner/specifications/-/blob/master/proposals/semantic-prompts.md>

use std::collections::VecDeque;

use super::Terminal;
use super::gutter::GutterKind;
use crate::page::{PageList, PinId, Position};

/// The most commands that a terminal keeps, its oldest going first, so
/// that a program that marks prompts without end costs no more.
pub const MOST_COMMANDS: usize = 10_000;

/// A command's marks, of the cells that their pins follow.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Marked {
    prompt: Option<PinId>,
    input: Option<PinId>,
    output: Option<PinId>,
    end: Option<PinId>,
    status: Option<i32>,
}

/// The kept commands of a terminal, oldest first.
#[derive(Debug, Default)]
pub(super) struct Commands(VecDeque<Marked>);

/// A command that the shell marked, as the terminal keeps it: each mark the
/// cell where it came, by its column and its row's number, of the primary
/// screen's, if its row is in the list still, and the exit status that the
/// shell told as the command ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandMarks {
    /// Where its prompt starts.
    pub prompt: Option<(u16, i64)>,
    /// Where the input that the user types starts, past the prompt.
    pub input: Option<(u16, i64)>,
    /// Where its output starts.
    pub output: Option<(u16, i64)>,
    /// Where it ended.
    pub end: Option<(u16, i64)>,
    /// Its exit status, if it ended and the shell told it.
    pub status: Option<i32>,
}

/// Which of a command's marks OSC 133 gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    /// `A`, the prompt's start, which starts a command.
    Prompt,
    /// `B`, the input's start.
    Input,
    /// `C`, the output's start.
    Output,
    /// `D`, the command's end, of its status if it told one.
    End(Option<i32>),
}

impl Terminal {
    /// Marks the cursor's cell of the primary screen as `mark` of the last
    /// command, a prompt starting a new one; the alternate screen's prompts
    /// are its program's, which no command keeps.
    ///
    /// A prompt lets go of the commands whose prompts are at its row or
    /// below, which a screen cleared in place left on rows written anew,
    /// and of the last command if it has no output's mark, as a shell that
    /// draws its prompt again, after a key's command or of a theme's
    /// redraw, gives the prompt's mark again for the same command; so that
    /// the commands stay in their prompts' order.
    pub(super) fn mark_command(&mut self, mark: Mark) {
        if self.alternate_active {
            return;
        }
        let (x, y) = (self.primary.cursor.x, self.primary.cursor.y);
        let rows = &mut self.primary.rows;
        let (page, row) = rows.screen_row_id(y);
        let pin = rows.pin(Position { page, row, col: x });
        let commands = &mut self.commands.0;
        if mark == Mark::Prompt {
            let at = rows.row_index(page, row);
            let row_of = |rows: &PageList, marked: &Marked| {
                let position = rows.pin_position(marked.prompt?)?;
                rows.row_index(position.page, position.row)
            };
            while let Some(last) = commands.back() {
                let below = at.is_some_and(|at| row_of(rows, last).is_some_and(|row| row >= at));
                if !below && last.output.is_some() {
                    break;
                }
                if let Some(gone) = commands.pop_back() {
                    unpin(rows, &gone);
                }
            }
        }
        if mark == Mark::Prompt || commands.is_empty() {
            // The commands whose prompts left the list go, and the oldest
            // past the most kept.
            while commands.front().is_some_and(|oldest| {
                oldest
                    .prompt
                    .is_none_or(|pin| rows.pin_position(pin).is_none())
            }) || commands.len() >= MOST_COMMANDS
            {
                if let Some(gone) = commands.pop_front() {
                    unpin(rows, &gone);
                }
            }
            commands.push_back(Marked::default());
        }
        let Some(last) = commands.back_mut() else {
            return;
        };
        let place = match mark {
            Mark::Prompt => &mut last.prompt,
            Mark::Input => &mut last.input,
            Mark::Output => &mut last.output,
            Mark::End(status) => {
                last.status = status;
                &mut last.end
            }
        };
        // A mark given again moves.
        if let Some(old) = std::mem::replace(place, pin) {
            rows.unpin(old);
        }
    }

    /// The commands that the shell marked, as the terminal keeps them,
    /// oldest first: those whose prompts left the list but not their output,
    /// with their prompts of `None`.
    pub fn commands(&self) -> Vec<CommandMarks> {
        let rows = &self.primary.rows;
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        let cell = |pin: Option<PinId>| {
            let position = rows.pin_position(pin?)?;
            let index = rows.row_index(position.page, position.row)?;
            Some((position.col, first.saturating_add(index as i64)))
        };
        self.commands
            .0
            .iter()
            .map(|marked| CommandMarks {
                prompt: cell(marked.prompt),
                input: cell(marked.input),
                output: cell(marked.output),
                end: cell(marked.end),
                status: marked.status,
            })
            .filter(|command| {
                command.prompt.is_some() || command.output.is_some() || command.end.is_some()
            })
            .collect()
    }

    /// The prompts of the primary screen's commands that ran, on the rows
    /// numbered from `top` to before `bottom`, oldest first, each of its
    /// row's number and its command's state: the commands of no output's
    /// mark have none.
    pub(super) fn prompts_in(&self, top: i64, bottom: i64) -> Vec<(i64, GutterKind)> {
        let rows = &self.primary.rows;
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        let row_of = |marked: &Marked| {
            let position = rows.pin_position(marked.prompt?)?;
            let index = rows.row_index(position.page, position.row)?;
            Some(first.saturating_add(index as i64))
        };
        // The commands are in their prompts' order, those whose prompts left
        // the list first: a binary search finds the view's first, as each
        // row's number costs a walk of the list's pages.
        let commands = &self.commands.0;
        let from = commands.partition_point(|marked| row_of(marked).is_none_or(|row| row < top));
        commands
            .range(from..)
            .map_while(|marked| Some((row_of(marked).filter(|&row| row < bottom)?, marked)))
            .filter_map(|(row, marked)| {
                // A line of no command, which the shell ends at once
                // without its output's mark, shows none.
                marked.output?;
                let ended = marked
                    .end
                    .is_some_and(|pin| rows.pin_position(pin).is_some());
                let kind = match (ended, marked.status) {
                    (true, Some(0) | None) => GutterKind::Succeeded,
                    (true, Some(_)) => GutterKind::Failed,
                    (false, _) => GutterKind::Running,
                };
                Some((row, kind))
            })
            .collect()
    }

    /// The indexes in the primary screen's list of the kept prompts' rows,
    /// oldest first.
    pub(super) fn prompt_indexes(&self) -> Vec<usize> {
        let rows = &self.primary.rows;
        self.commands
            .0
            .iter()
            .filter_map(|marked| {
                let position = rows.pin_position(marked.prompt?)?;
                rows.row_index(position.page, position.row)
            })
            .collect()
    }
}

/// Lets go of the pins of `marked`.
fn unpin(rows: &mut PageList, marked: &Marked) {
    for pin in [marked.prompt, marked.input, marked.output, marked.end]
        .into_iter()
        .flatten()
    {
        rows.unpin(pin);
    }
}

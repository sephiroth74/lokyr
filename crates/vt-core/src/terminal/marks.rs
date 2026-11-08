//! The commands that the shell marks with OSC 133, its semantic prompts:
//! each command's prompt, where its input starts, where its output starts,
//! where it ended and its exit status, each a cell of the primary screen
//! that a pin follows through output, the scrollback and a reflowing
//! resize, until its row leaves the list; and the view's jumps from prompt
//! to prompt.
//!
//! <https://gitlab.freedesktop.org/Per_Bothner/specifications/-/blob/master/proposals/semantic-prompts.md>

use std::collections::VecDeque;

use super::events::Events;
use super::gutter::GutterKind;
use super::{Event, Terminal};
use crate::page::{PageList, PinId, Position};

/// The most commands that a terminal keeps, its oldest going first, so
/// that a program that marks prompts without end costs no more.
pub const MOST_COMMANDS: usize = 10_000;

/// The most bytes of a command's line that a shell tells, past which the
/// rest goes, so that the lines of the commands kept cost no more.
const MOST_LINE: usize = 2048;

/// A command's marks, of the cells that their pins follow, its id, and its
/// line, as the shell told it, if it did.
#[derive(Clone, Debug, Default)]
pub(super) struct Marked {
    id: u64,
    prompt: Option<PinId>,
    input: Option<PinId>,
    output: Option<PinId>,
    end: Option<PinId>,
    status: Option<i32>,
    line: Option<String>,
}

/// The kept commands of a terminal, oldest first, and the id of the last
/// that came.
#[derive(Debug, Default)]
pub(super) struct Commands {
    list: VecDeque<Marked>,
    pub(super) last: u64,
}

/// A command that the shell marked, as the terminal keeps it: its id, each
/// mark the cell where it came, by its column and its row's number, of the
/// primary screen's, if its row is in the list still, and the exit status
/// that the shell told as the command ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandMarks {
    /// Its id, of the terminal's commands from 1 in the order of their
    /// prompts, which [`Event::CommandStarted`] and
    /// [`Event::CommandEnded`] tell.
    pub id: u64,
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
    ///
    /// A command starts as its output's mark first comes, of no end's
    /// before it, and ends as its end's first comes after it, which
    /// [`Event::CommandStarted`] and [`Event::CommandEnded`] tell: a line of
    /// no command, whose end the shell marks at once, tells nothing; one
    /// that started and that the list lets go of before its end ends of no
    /// status.
    pub(super) fn mark_command(&mut self, mark: Mark) {
        if self.alternate_active {
            return;
        }
        let (x, y) = (self.primary.cursor.x, self.primary.cursor.y);
        let rows = &mut self.primary.rows;
        let (page, row) = rows.screen_row_id(y);
        let pin = rows.pin(Position { page, row, col: x });
        let commands = &mut self.commands;
        if mark == Mark::Prompt {
            let at = rows.row_index(page, row);
            let row_of = |rows: &PageList, marked: &Marked| {
                let position = rows.pin_position(marked.prompt?)?;
                rows.row_index(position.page, position.row)
            };
            while let Some(last) = commands.list.back() {
                let below = at.is_some_and(|at| row_of(rows, last).is_some_and(|row| row >= at));
                if !below && last.output.is_some() {
                    break;
                }
                if let Some(gone) = commands.list.pop_back() {
                    let_go(rows, &mut self.events, &gone);
                }
            }
        }
        if mark == Mark::Prompt || commands.list.is_empty() {
            // The commands whose prompts and ends left the list go, as
            // every row of theirs did, and the oldest past the most kept.
            let gone = |rows: &PageList, pin: Option<PinId>| {
                pin.is_none_or(|pin| rows.pin_position(pin).is_none())
            };
            while commands
                .list
                .front()
                .is_some_and(|oldest| gone(rows, oldest.prompt) && gone(rows, oldest.end))
                || commands.list.len() >= MOST_COMMANDS
            {
                if let Some(gone) = commands.list.pop_front() {
                    let_go(rows, &mut self.events, &gone);
                }
            }
            commands.last += 1;
            commands.list.push_back(Marked {
                id: commands.last,
                ..Marked::default()
            });
        }
        let Some(last) = commands.list.back_mut() else {
            return;
        };
        let told = match mark {
            Mark::Output if last.output.is_none() && last.end.is_none() => {
                Some(Event::CommandStarted(last.id))
            }
            Mark::End(status) if last.output.is_some() && last.end.is_none() => {
                Some(Event::CommandEnded {
                    id: last.id,
                    status,
                })
            }
            _ => None,
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
        if let Some(event) = told {
            self.events.push(event);
        }
    }

    /// The commands that the shell marked, as the terminal keeps them,
    /// oldest first: those whose prompts left the list but not every row,
    /// as their ends did not, with their marks that left of `None`.
    pub fn commands(&self) -> Vec<CommandMarks> {
        self.commands_meeting(i64::MIN, i64::MAX)
    }

    /// The commands that the shell marked whose rows meet the primary
    /// screen's rows numbered from `top` to before `bottom`, as the
    /// terminal keeps them, oldest first: the last whose first row is at
    /// `top` or above it, whose output, or prompt, the row `top` may show,
    /// and those after it whose first rows are above `bottom`. A command's
    /// first row is its prompt's, or, of none, its first mark's; of a
    /// prompt that left the list, the list's first row, but of a command
    /// whose end left it too, as every row of it did, which is none.
    pub fn commands_meeting(&self, top: i64, bottom: i64) -> Vec<CommandMarks> {
        if top >= bottom {
            return Vec::new();
        }
        let rows = &self.primary.rows;
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        let index = rows.row_indexes();
        let cell = |pin: Option<PinId>| {
            let position = rows.pin_position(pin?)?;
            let at = index(position.page, position.row)?;
            Some((position.col, first.saturating_add(at as i64)))
        };
        let first_row = |marked: &Marked| {
            let marks = [marked.prompt, marked.input, marked.output, marked.end];
            match marked.prompt {
                Some(prompt) => {
                    if let Some((_, row)) = cell(Some(prompt)) {
                        return Some(row);
                    }
                }
                None => {
                    if let Some((_, row)) = marks.into_iter().find_map(&cell) {
                        return Some(row);
                    }
                }
            }
            // Its first rows left the list, which its rows start at, unless
            // its end left too, and every row of it.
            let ended = marked
                .end
                .is_some_and(|pin| rows.pin_position(pin).is_none());
            (!ended && marks.iter().any(Option::is_some)).then_some(first)
        };
        // The commands are in their first rows' order, those that left the
        // list first: a binary search finds the first that meets the rows.
        let list = &self.commands.list;
        let after = list.partition_point(|marked| first_row(marked).is_none_or(|row| row <= top));
        let begin = after
            .checked_sub(1)
            .filter(|&at| first_row(&list[at]).is_some())
            .unwrap_or(after);
        list.range(begin..)
            .map_while(|marked| {
                first_row(marked).filter(|&row| row < bottom)?;
                Some(CommandMarks {
                    id: marked.id,
                    prompt: cell(marked.prompt),
                    input: cell(marked.input),
                    output: cell(marked.output),
                    end: cell(marked.end),
                    status: marked.status,
                })
            })
            .collect()
    }

    /// The command of the id `id`, as [`Terminal::commands`] gives it, if
    /// the terminal keeps it.
    pub fn command(&self, id: u64) -> Option<CommandMarks> {
        let list = &self.commands.list;
        let marked = &list[list.binary_search_by_key(&id, |marked| marked.id).ok()?];
        let rows = &self.primary.rows;
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        let cell = |pin: Option<PinId>| {
            let position = rows.pin_position(pin?)?;
            let index = rows.row_index(position.page, position.row)?;
            Some((position.col, first.saturating_add(index as i64)))
        };
        Some(CommandMarks {
            id,
            prompt: cell(marked.prompt),
            input: cell(marked.input),
            output: cell(marked.output),
            end: cell(marked.end),
            status: marked.status,
        })
    }

    /// Gives the last command, whose output's mark came and whose end did
    /// not, the line that the shell told of it, of `bytes`, percent-encoded,
    /// as OSC 133 `C`'s `cmdline_url` tells it: decoded, of UTF-8, 2048
    /// bytes of it at most, its control characters as spaces, and without
    /// the spaces around it.
    pub(super) fn tell_command_line(&mut self, bytes: &[u8]) {
        if self.alternate_active {
            return;
        }
        let Some(last) = self.commands.list.back_mut() else {
            return;
        };
        if last.output.is_none() || last.end.is_some() {
            return;
        }
        let mut decoded = Vec::with_capacity(bytes.len().min(MOST_LINE));
        let mut at = 0;
        while at < bytes.len() && decoded.len() < MOST_LINE {
            let hex = |byte: u8| char::from(byte).to_digit(16);
            let escaped = (bytes[at] == b'%')
                .then(|| Some(hex(*bytes.get(at + 1)?)? * 16 + hex(*bytes.get(at + 2)?)?))
                .flatten();
            match escaped {
                // Two hex digits make a byte.
                Some(byte) => {
                    decoded.push(byte as u8);
                    at += 3;
                }
                None => {
                    decoded.push(bytes[at]);
                    at += 1;
                }
            }
        }
        let line: String = String::from_utf8_lossy(&decoded)
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .collect();
        let line = line.trim();
        last.line = (!line.is_empty()).then(|| line.to_owned());
    }

    /// The command line of the command of the id `id`, as the user typed
    /// it: as the shell told it, if it did, which a right prompt on its row
    /// leaves out; or else the text from its input's mark to its output's,
    /// of the input's logical line, 256 rows of it at most, without the
    /// spaces around it; `None` for a command not kept, of no input's mark
    /// in the list, of no text, or, of a line that the shell did not tell,
    /// while the alternate screen shows.
    pub fn command_line(&self, id: u64) -> Option<String> {
        let list = &self.commands.list;
        let told = list
            .binary_search_by_key(&id, |marked| marked.id)
            .ok()
            .and_then(|at| list[at].line.clone());
        if told.is_some() {
            return told;
        }
        if self.alternate_active {
            return None;
        }
        let command = self.command(id)?;
        let (column, row) = command.input?;
        let (text, cells) = self.line_text(row, 256)?;
        // In reading order: a row's number, then a column.
        let from = (row, column);
        let to = command.output.map(|(column, row)| (row, column));
        let line: String = text
            .chars()
            .zip(cells)
            .filter(|&(_, (column, row))| {
                (row, column) >= from && to.is_none_or(|to| (row, column) < to)
            })
            .map(|(character, _)| character)
            .collect();
        let line = line.trim();
        (!line.is_empty()).then(|| line.to_owned())
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
        let commands = &self.commands.list;
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
            .list
            .iter()
            .filter_map(|marked| {
                let position = rows.pin_position(marked.prompt?)?;
                rows.row_index(position.page, position.row)
            })
            .collect()
    }
}

impl Terminal {
    /// Lets go of the commands, as a reset does, a command that runs
    /// ending of no status, and gives the last command's id, which the
    /// next command's follows.
    pub(super) fn forget_commands(&mut self) -> u64 {
        let commands = std::mem::take(&mut self.commands);
        for gone in &commands.list {
            let_go(&mut self.primary.rows, &mut self.events, gone);
        }
        commands.last
    }
}

/// Lets go of the pins of `marked`, a command that the list lets go of,
/// which ends, of no status, if it started and did not end.
fn let_go(rows: &mut PageList, events: &mut Events, marked: &Marked) {
    for pin in [marked.prompt, marked.input, marked.output, marked.end]
        .into_iter()
        .flatten()
    {
        rows.unpin(pin);
    }
    if marked.output.is_some() && marked.end.is_none() {
        events.push(Event::CommandEnded {
            id: marked.id,
            status: None,
        });
    }
}

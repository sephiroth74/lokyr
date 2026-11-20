//! The commands that the shell marks with OSC 133, its semantic prompts:
//! each command's prompt, where its input starts, where its output starts,
//! where it ended and its exit status, each a cell of the primary screen
//! that a pin follows through output, the scrollback and a reflowing
//! resize, until its row leaves the list; and the view's jumps from prompt
//! to prompt. A command that started keeps the times of the system's
//! clock at its start and its end, and the working directory that OSC 7
//! told last as it started.
//!
//! <https://gitlab.freedesktop.org/Per_Bothner/specifications/-/blob/master/proposals/semantic-prompts.md>

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::SystemTime;

use super::events::Events;
use super::gutter::GutterKind;
use super::{Event, Terminal, WorkingDirectory};
use crate::page::{Content, ListRow, PageList, PinId, Position};

/// The most commands that a terminal keeps, its oldest going first, so
/// that a program that marks prompts without end costs no more.
pub const MOST_COMMANDS: usize = 10_000;

/// The most bytes of a command's line that a shell tells, past which the
/// rest goes, so that the lines of the commands kept cost no more.
const MOST_LINE: usize = 2048;

/// A command's marks, of the cells that their pins follow, its id, its
/// line, as the shell told it, if it did, the times of its start and its
/// end, and the working directory as it started.
#[derive(Clone, Debug, Default)]
pub(super) struct Marked {
    id: u64,
    prompt: Option<PinId>,
    input: Option<PinId>,
    output: Option<PinId>,
    end: Option<PinId>,
    status: Option<i32>,
    line: Option<String>,
    started: Option<SystemTime>,
    ended: Option<SystemTime>,
    folder: Option<Arc<WorkingDirectory>>,
    /// Whether the terminal put a blank row before its prompt, as
    /// [`Terminal::set_block_spacing`] asks.
    spaced: bool,
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
/// primary screen's, if its row is in the list still, the exit status
/// that the shell told as the command ended, the times of its start and
/// its end, and the working directory where it started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// The time of the system's clock as it started, at its output's mark,
    /// if it did.
    pub started: Option<SystemTime>,
    /// The time of the system's clock as it ended, at its end's mark after
    /// its start, if it did.
    pub ended: Option<SystemTime>,
    /// The working directory that OSC 7 told last as it started, if one
    /// was told.
    pub folder: Option<Arc<WorkingDirectory>>,
}

impl CommandMarks {
    /// The command of `marked`, of its marks' cells as `cell` gives them.
    fn of(marked: &Marked, cell: impl Fn(Option<PinId>) -> Option<(u16, i64)>) -> Self {
        Self {
            id: marked.id,
            prompt: cell(marked.prompt),
            input: cell(marked.input),
            output: cell(marked.output),
            end: cell(marked.end),
            status: marked.status,
            started: marked.started,
            ended: marked.ended,
            folder: marked.folder.clone(),
        }
    }
}

/// A command's block, of the primary screen's rows, by their numbers: its
/// command's id, its first row, its prompt's first row of text, if the list holds its
/// prompt, its last row, and its state, none of a command whose output has
/// not started, as the prompt where the user types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Block {
    pub(super) id: u64,
    pub(super) first: i64,
    pub(super) prompt: Option<i64>,
    pub(super) last: i64,
    pub(super) kind: Option<GutterKind>,
    /// Whether the row before its first is the blank row that the terminal
    /// put before its prompt.
    pub(super) room_above: bool,
    /// Whether the row after its last is the blank row that the terminal
    /// put before the next command's prompt.
    pub(super) room_below: bool,
}

/// The first row of `marked`'s block, by its number, as `row_of` gives a
/// mark's, of `rows`, whose first row is numbered `first`: its prompt's,
/// or, of none, its first mark's; of a prompt that left the list, the
/// list's first row, but of a command whose end left it too, as every row
/// of it did, which is none.
fn first_row_of(
    marked: &Marked,
    row_of: impl Fn(Option<PinId>) -> Option<i64>,
    first: i64,
    rows: &PageList,
) -> Option<i64> {
    let marks = [marked.prompt, marked.input, marked.output, marked.end];
    match marked.prompt {
        Some(prompt) => {
            if let Some(row) = row_of(Some(prompt)) {
                return Some(row);
            }
        }
        None => {
            if let Some(row) = marks.into_iter().find_map(&row_of) {
                return Some(row);
            }
        }
    }
    // Its first rows left the list, which its rows start at, unless its end
    // left too, and every row of it.
    let ended = marked
        .end
        .is_some_and(|pin| rows.pin_position(pin).is_none());
    (!ended && marks.iter().any(Option::is_some)).then_some(first)
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
    /// Whether a blank row comes before each prompt that follows a command
    /// that ended, as room between the commands' blocks: as the prompt's
    /// mark comes, of a command whose output and end's marks came before
    /// it, the cursor goes a row down, the region scrolling at its bottom
    /// margin, after a line's end where it is past the first column, so
    /// that the prompt starts a row lower than the shell drew it, the blank
    /// row of no command's block; none before the first prompt, nor of the
    /// cursor at the screen's first cell, as a screen cleared leaves it,
    /// nor on the alternate screen. Shells draw their prompts from where
    /// the cursor is, and move within them of relative moves, so that the
    /// row is no part of what they draw again.
    pub fn set_block_spacing(&mut self, on: bool) {
        self.block_spacing = on;
    }

    /// Whether the prompt's mark that comes now has a blank row before it,
    /// as [`Terminal::set_block_spacing`] tells.
    fn spaces_prompt(&self) -> bool {
        let cursor = &self.primary.cursor;
        self.block_spacing
            && (cursor.x, cursor.y) != (0, 0)
            && self
                .commands
                .list
                .back()
                .is_some_and(|last| last.output.is_some() && last.end.is_some())
    }

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
    /// status. A command keeps the system's time as it starts, and the
    /// working directory told last, and the time as it ends.
    pub(super) fn mark_command(&mut self, mark: Mark) {
        if self.alternate_active {
            return;
        }
        let spaced = mark == Mark::Prompt && self.spaces_prompt();
        if spaced {
            self.moved();
            if self.primary.cursor.x > 0 {
                self.carriage_return();
                self.index();
            }
            self.index();
        }
        let folder = match mark {
            Mark::Output => self.working_directory.clone(),
            _ => None,
        };
        let (x, y) = (self.primary.cursor.x, self.primary.cursor.y);
        let rows = &mut self.primary.rows;
        let (page, row) = rows.screen_row_id(y);
        let pin = rows.pin(Position { page, row, col: x });
        let commands = &mut self.commands;
        // A prompt drawn again at its row, which its shell marks anew as it
        // redraws it, keeps the blank row that the terminal put before it.
        let mut carried = false;
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
                    carried |= gone.spaced
                        && gone.output.is_none()
                        && at.is_some()
                        && row_of(rows, &gone) == at;
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
                spaced: spaced || carried,
                ..Marked::default()
            });
        }
        let Some(last) = commands.list.back_mut() else {
            return;
        };
        let told = match mark {
            Mark::Output if last.output.is_none() && last.end.is_none() => {
                last.started = Some(SystemTime::now());
                last.folder = folder;
                Some(Event::CommandStarted(last.id))
            }
            Mark::End(status) if last.output.is_some() && last.end.is_none() => {
                last.ended = Some(SystemTime::now());
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
        let row_of = |pin: Option<PinId>| cell(pin).map(|(_, row)| row);
        let first_row = |marked: &Marked| first_row_of(marked, row_of, first, rows);
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
                Some(CommandMarks::of(marked, cell))
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
        Some(CommandMarks::of(marked, cell))
    }

    /// The last command that the shell marked, as [`Terminal::command`]
    /// gives it, if the terminal keeps one.
    pub fn last_command(&self) -> Option<CommandMarks> {
        self.command(self.commands.list.back()?.id)
    }

    /// The command whose block holds the primary screen's row numbered
    /// `row`, as [`Terminal::commands`] gives it: its block runs from its
    /// first row, as [`Terminal::commands_meeting`] tells it, to its end's
    /// row, which is its own where the end is past its first column, and
    /// the next prompt's otherwise, or, of a command that has not ended, to
    /// the next command's first row; `None` of a row above every command's,
    /// or past an end before the next command.
    pub fn command_at(&self, row: i64) -> Option<CommandMarks> {
        let command = self.commands_meeting(row, row.saturating_add(1)).pop()?;
        let within = match command.end {
            Some((column, end)) => row < end || (row == end && column > 0),
            None => true,
        };
        within.then_some(command)
    }

    /// Gives the last command, whose output's mark came and whose end did
    /// not, the line that the shell told of it, of `bytes`, percent-encoded,
    /// as OSC 133 `C`'s `cmdline_url` tells it: decoded, of UTF-8, 2048
    /// bytes of it at most, its control characters as spaces but its line
    /// feeds, which part a command of several lines, and without
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
                if character.is_control() && character != '\n' {
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

    /// The blocks of the primary screen's commands whose rows meet the rows
    /// numbered from `top` to before `bottom`, oldest first, as
    /// [`Terminal::commands_meeting`] finds the commands: each of its first
    /// row, its prompt's first row of text, its last row, and its state.
    pub(super) fn blocks_in(&self, top: i64, bottom: i64) -> Vec<Block> {
        if top >= bottom {
            return Vec::new();
        }
        let rows = &self.primary.rows;
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        // The rows' indexes of one walk of the list's pages, for every
        // command of the call.
        let index = rows.row_indexes();
        let cell = |pin: Option<PinId>| {
            let position = rows.pin_position(pin?)?;
            let at = index(position.page, position.row)?;
            Some((position.col, first.saturating_add(at as i64)))
        };
        let row_of = |pin: Option<PinId>| cell(pin).map(|(_, row)| row);
        let first_row = |marked: &Marked| first_row_of(marked, row_of, first, rows);
        // The prompt's first row of text, from its start to its input's row,
        // a few rows at most, as a theme's prompt may print an empty line
        // first, read from the start's own place on; its start's where none
        // holds text.
        let shown_row = |marked: &Marked| {
            let place = marked.prompt.and_then(|pin| rows.pin_position(pin))?;
            let start = index(place.page, place.row)?;
            let row = first.saturating_add(start as i64);
            let Some(input) = marked.input.and_then(|pin| {
                let position = rows.pin_position(pin)?;
                index(position.page, position.row)
            }) else {
                return Some(row);
            };
            let rows_seen = input.saturating_sub(start).min(MOST_PROMPT_ROWS) + 1;
            Some(
                (0..rows_seen)
                    .find(|&at| {
                        // The start's page's rows, of no walk; a row past it
                        // through the list's rows, as a prompt seldom
                        // crosses a page.
                        let shown = u16::try_from(at)
                            .ok()
                            .and_then(|at| rows.row_of(place.page, place.row.checked_add(at)?))
                            .or_else(|| rows.rows_from(start + at).next());
                        shown.is_some_and(holds_text)
                    })
                    .map_or(row, |at| first.saturating_add((start + at) as i64)),
            )
        };
        // The cursor's row, where the output of a command that runs reaches.
        let cursor = (rows.scrolled() as i64).saturating_add(i64::from(self.primary.cursor.y));
        // The commands are in their first rows' order: a binary search finds
        // the last that starts at `top` or above it, whose block the view's
        // top row may show.
        let list = &self.commands.list;
        let after = list.partition_point(|marked| first_row(marked).is_none_or(|row| row <= top));
        let begin = after
            .checked_sub(1)
            .filter(|&at| first_row(&list[at]).is_some())
            .unwrap_or(after);
        (begin..list.len())
            .map_while(|at| Some((first_row(&list[at]).filter(|&row| row < bottom)?, at)))
            .filter_map(|(first, at)| {
                let marked = &list[at];
                let end = cell(marked.end);
                // A line of no command, which the shell ends at once without
                // its output's mark, has no state.
                let kind = marked.output.map(|_| match (end, marked.status) {
                    (Some(_), Some(0) | None) => GutterKind::Succeeded,
                    (Some(_), Some(_)) => GutterKind::Failed,
                    (None, _) => GutterKind::Running,
                });
                let last = match end {
                    // An end at a row's start, as an output's last line feed
                    // leaves it, ends the row before.
                    Some((column, row)) => {
                        if column == 0 {
                            row - 1
                        } else {
                            row
                        }
                    }
                    // A command that runs reaches the cursor; one of no end
                    // that another followed, as a nested shell's outer
                    // command, the next command's first row.
                    None if kind.is_some() => list
                        .get(at + 1)
                        .and_then(first_row)
                        .map_or(cursor, |next| next - 1),
                    None => first,
                }
                .max(first);
                // The blank rows before the prompts, of their commands, as the
                // terminal put them: its own before its prompt's row, and the
                // next command's right after its last row.
                let room_above = marked.spaced && row_of(marked.prompt) == Some(first);
                let room_below = list.get(at + 1).is_some_and(|next| {
                    next.spaced && row_of(next.prompt) == Some(last.saturating_add(2))
                });
                (last >= top).then(|| Block {
                    id: marked.id,
                    first,
                    prompt: shown_row(marked),
                    last,
                    kind,
                    room_above,
                    room_below,
                })
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

/// The most rows below a prompt's start that its mark looks at for its
/// first row of text.
const MOST_PROMPT_ROWS: usize = 8;

/// Whether `row` holds a character other than a space.
fn holds_text(row: ListRow<'_>) -> bool {
    row.cells().any(|cell| match cell.content() {
        Content::Char(char) => char != '\0' && char != ' ',
        Content::Grapheme(_) => true,
        Content::BgPalette(_) | Content::BgRgb(_) => false,
    })
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

#[cfg(test)]
mod spacing_tests;

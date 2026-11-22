//! The text of a command's output, and its selection, of the marks that
//! the shell prints: its cells from its output's mark to its end's, a row
//! that wraps joined to the next, without the mark of a missing newline
//! that a shell prints at its end.
//!
//! The plugins that copy and select a command's output hold this file as a
//! module of their own, by its path, as a built-in plugin depends on the
//! plugin SDK alone.

use lokyr_plugin_sdk::host::{Line, ShellCommand};
use unicode_width::UnicodeWidthStr;

/// The marks that a shell prints of output that ends without a line's end,
/// before its prompt: zsh's `%` of a user, `#` of root, and fish's.
const MISSING_NEWLINE: [char; 4] = ['%', '#', '⏎', '¶'];

/// The first and the last cells of an output, each by its column and its
/// row's number.
pub(crate) type Span = ((u16, i64), (u16, i64));

/// The first and the last cells of `command`'s output, of a grid of
/// `columns`: from its output's mark to the cell before its end's; none of
/// a command that has not ended, nor of no output.
pub(crate) fn output_span(command: &ShellCommand, columns: u16) -> Option<Span> {
    let first = command.output?;
    let (end_column, end_row) = command.end?;
    let last = if end_column == 0 {
        (columns.saturating_sub(1), end_row - 1)
    } else {
        (end_column - 1, end_row)
    };
    (last.1 > first.1 || (last.1 == first.1 && last.0 >= first.0)).then_some((first, last))
}

/// The text of the output of `span` that `lines`, its rows' texts, hold:
/// from its first cell to its last, a row that wraps joined to the next,
/// without the blank rows that start and end it, nor the mark of a missing
/// newline that a shell printed at its end.
pub(crate) fn output_text(
    lines: &[Line],
    ((first_column, first), (last_column, last)): Span,
) -> String {
    let mut text = String::new();
    for line in lines
        .iter()
        .filter(|line| line.row >= first && line.row <= last)
    {
        let shown = if line.row == last {
            unmarked(line)
        } else {
            &line.text
        };
        let start = if line.row == first {
            within(shown, usize::from(first_column))
        } else {
            0
        };
        let end = if line.row == last {
            within(shown, usize::from(last_column) + 1)
        } else {
            shown.len()
        };
        text.push_str(&shown[start.min(end)..end]);
        if !line.wrapped || line.row == last {
            text.push('\n');
        }
    }
    let trimmed = text.trim_end().trim_start_matches(['\n', '\r']);
    trimmed.to_owned()
}

/// The text of `line`, the last row of a command's output, without the
/// mark of a missing newline: a row that wraps into the end's row ends in
/// the shell's mark and the spaces that took the prompt to a row of its
/// own, as zsh's `PROMPT_SP` prints them.
fn unmarked(line: &Line) -> &str {
    if !line.wrapped {
        return &line.text;
    }
    let text = line.text.trim_end_matches(' ');
    text.strip_suffix(MISSING_NEWLINE).unwrap_or(text)
}

/// The end, in bytes, of the longest start of `text` that takes `cells`
/// cells at most, as wide characters take two and the characters of a
/// cluster share theirs.
fn within(text: &str, cells: usize) -> usize {
    let mut end = 0;
    for (at, character) in text.char_indices() {
        let next = at + character.len_utf8();
        if UnicodeWidthStr::width(&text[..next]) > cells {
            break;
        }
        end = next;
    }
    end
}

/// The first and last cells of a selection of the output of `span`, of a
/// grid of `columns`, as `lines`, its last row's text, tells its end: its
/// last row's cells but the mark of a missing newline, or, of none left,
/// the row before's; none of an output left empty.
pub(crate) fn selected(lines: &[Line], (first, last): Span, columns: u16) -> Option<Span> {
    let Some(line) = lines.iter().find(|line| line.row == last.1) else {
        return Some((first, last));
    };
    if !line.wrapped {
        return Some((first, last));
    }
    let cells = UnicodeWidthStr::width(unmarked(line));
    let end = match u16::try_from(cells) {
        Ok(cells) if cells > 0 => (cells.min(columns) - 1, last.1),
        _ => (columns.saturating_sub(1), last.1 - 1),
    };
    (end.1 > first.1 || (end.1 == first.1 && end.0 >= first.0)).then_some((first, end))
}

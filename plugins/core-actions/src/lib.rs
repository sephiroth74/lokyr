//! Built-in plugin with the basic terminal actions.
//!
//! Copy and paste, select all, clearing the scrollback, scrolling and the
//! font's size, whose keys the system's keymap plugin binds; and the
//! output of a command that the shell marked, copied or selected, the last
//! one's, or the one's at a cell of the pane's context menu. Like every
//! built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind, Param, Value,
};
use lokyr_plugin_sdk::host::{Line, PaneId, Scroll, ShellCommand};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use unicode_width::UnicodeWidthStr;

lokyr_plugin_sdk::export_plugin!(CoreActions);

/// Copies the focused pane's selection to the clipboard.
const COPY: &str = "pane.copy";

/// Pastes the clipboard's text into the focused pane.
const PASTE: &str = "pane.paste";

/// Selects the focused pane's scrollback and screen.
const SELECT_ALL: &str = "pane.select-all";

/// Clears the focused pane's scrollback and screen, but its prompt.
const CLEAR: &str = "pane.clear";

/// Makes the text a point larger, in every window.
const FONT_BIGGER: &str = "font.bigger";

/// Makes the text a point smaller.
const FONT_SMALLER: &str = "font.smaller";

/// Gives the text the config's size again.
const FONT_RESET: &str = "font.reset";

/// Scroll the focused pane's view through its scrollback.
const SCROLL_LINE_UP: &str = "pane.scroll-line-up";
const SCROLL_LINE_DOWN: &str = "pane.scroll-line-down";
const SCROLL_PAGE_UP: &str = "pane.scroll-page-up";
const SCROLL_PAGE_DOWN: &str = "pane.scroll-page-down";
const SCROLL_TOP: &str = "pane.scroll-top";
const SCROLL_BOTTOM: &str = "pane.scroll-bottom";

/// Scroll the focused pane's view to the prompt before its top, or after
/// it, of the commands that the shell marks.
const PROMPT_PREVIOUS: &str = "pane.prompt-previous";
const PROMPT_NEXT: &str = "pane.prompt-next";

/// Sends its `text` to the focused pane's program, as it is: for keys that
/// the config binds, as those of a shell's line editing.
const SEND_TEXT: &str = "pane.send-text";

/// Copy, and select, the output of the focused pane's last command that
/// ended, of the commands that the shell marks, or of the command at the
/// cell of its `column` and `row`, as the pane's context menu gives them.
const COPY_OUTPUT: &str = "pane.copy-output";
const SELECT_OUTPUT: &str = "pane.select-output";

/// The rows that a copy of an output reads at most, as a read of rows
/// takes them.
const MOST_OUTPUT_ROWS: usize = lokyr_plugin_sdk::host::MOST_LINES;

/// The rows back from the screen whose commands a look for the last
/// command's output reads first, before every command's.
const RECENT_ROWS: i64 = 10_000;

/// The marks that a shell prints of output that ends without a line's end,
/// before its prompt: zsh's `%` of a user, `#` of root, and fish's.
const MISSING_NEWLINE: [char; 4] = ['%', '#', '⏎', '¶'];

/// The first and the last cells of an output, each by its column and its
/// row's number.
type Span = ((u16, i64), (u16, i64));

/// The plugin's catalogs, of its commands' titles.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/core-actions.ftl")),
    ("it", include_str!("../locales/it/core-actions.ftl")),
];

/// The sizes of the text, in points, that the commands go between.
const SIZES: (f32, f32) = (4.0, 200.0);

/// A command: its name, title, category and capabilities.
type Spec = (&'static str, &'static str, Category, &'static [Capability]);

/// The commands.
const COMMANDS: [Spec; 15] = [
    (
        COPY,
        "Copy",
        Category::Clipboard,
        &[Capability::TerminalRead, Capability::ClipboardWrite],
    ),
    (
        PASTE,
        "Paste",
        Category::Clipboard,
        &[Capability::ClipboardRead, Capability::PtyWrite],
    ),
    (SELECT_ALL, "Select All", Category::Edit, &[]),
    (CLEAR, "Clear", Category::Edit, &[]),
    (FONT_BIGGER, "Bigger", Category::View, &[]),
    (FONT_SMALLER, "Smaller", Category::View, &[]),
    (FONT_RESET, "Default Size", Category::View, &[]),
    (SCROLL_LINE_UP, "Scroll Up a Line", Category::View, &[]),
    (SCROLL_LINE_DOWN, "Scroll Down a Line", Category::View, &[]),
    (SCROLL_PAGE_UP, "Scroll Up a Page", Category::View, &[]),
    (SCROLL_PAGE_DOWN, "Scroll Down a Page", Category::View, &[]),
    (SCROLL_TOP, "Scroll to the Top", Category::View, &[]),
    (SCROLL_BOTTOM, "Scroll to the Bottom", Category::View, &[]),
    (
        PROMPT_PREVIOUS,
        "Jump to Previous Prompt",
        Category::View,
        &[],
    ),
    (PROMPT_NEXT, "Jump to Next Prompt", Category::View, &[]),
];

/// What a read of a command's output's rows is for: its copy, or its
/// selection, which its last row's end tells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reading {
    Copy,
    Select,
}

/// The plugin of the basic terminal actions, and the read of an output's
/// rows under way: the read's id, its pane, the output's cells and what it
/// is for.
#[derive(Debug, Default)]
pub struct CoreActions {
    reading: Option<(u64, PaneId, Span, Reading)>,
}

impl Plugin for CoreActions {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for (name, title, category, capabilities) in COMMANDS {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category,
                capabilities: capabilities.to_vec(),
                params: Vec::new(),
                // Copying takes a selection.
                when: (name == COPY).then(|| "selection".to_owned()),
            })?;
        }
        cx.register_command(Command {
            name: SEND_TEXT.to_owned(),
            title: "Send Text".to_owned(),
            category: Category::Input,
            capabilities: vec![Capability::PtyWrite],
            params: vec![Param {
                name: "text".to_owned(),
                kind: Kind::String,
                default: None,
            }],
            when: None,
        })?;
        // A command's output: its cell's, as the context menu gives it, or
        // of none, the last command's.
        let cell = |name: &str| Param {
            name: name.to_owned(),
            kind: Kind::Integer,
            default: Some(Value::Integer(-1)),
        };
        let outputs = [
            (
                COPY_OUTPUT,
                "Copy Command Output",
                Category::Clipboard,
                vec![
                    Capability::HistoryRead,
                    Capability::TerminalRead,
                    Capability::ClipboardWrite,
                ],
            ),
            (
                SELECT_OUTPUT,
                "Select Command Output",
                Category::Edit,
                vec![Capability::HistoryRead],
            ),
        ];
        for (name, title, category, capabilities) in outputs {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category,
                capabilities,
                params: vec![cell("column"), cell("row")],
                when: None,
            })?;
        }
        Ok(())
    }

    /// A command's output applies where the shell marked one that ended,
    /// of some output.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        match name {
            COPY_OUTPUT | SELECT_OUTPUT => cx
                .focused_pane()
                .is_some_and(|pane| output(cx, pane, args).ok().flatten().is_some()),
            _ => true,
        }
    }

    /// The rows of an output that a copy read, which the clipboard takes.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if let Event::Lines {
                id, pane, lines, ..
            } = event
                && let Some((asked, of, span, reading)) = self.reading
                && asked == *id
                && of == *pane
            {
                self.reading = None;
                match reading {
                    Reading::Copy => {
                        let text = output_text(lines, span);
                        if !text.is_empty() {
                            cx.set_clipboard_text(&text)?;
                        }
                    }
                    Reading::Select => {
                        let (columns, _) = cx.pane_grid(*pane)?;
                        if let Some((first, last)) = selected(lines, span, columns) {
                            cx.select_cells(*pane, first, last)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        // The font's size is every window's; it stays within its sizes.
        let (smallest, largest) = SIZES;
        let size = cx.font_size()?;
        match name {
            FONT_BIGGER if size < largest => {
                return cx.set_font_size(Some((size.floor() + 1.0).min(largest)));
            }
            FONT_SMALLER if size > smallest => {
                return cx.set_font_size(Some((size.ceil() - 1.0).max(smallest)));
            }
            FONT_RESET => return cx.set_font_size(None),
            FONT_BIGGER | FONT_SMALLER => return Ok(()),
            _ => {}
        }
        // The others act on the focused pane; without one, there is nothing
        // to do.
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        let scroll = match name {
            SCROLL_LINE_UP => Some(Scroll::Up(1)),
            SCROLL_LINE_DOWN => Some(Scroll::Down(1)),
            SCROLL_PAGE_UP => Some(Scroll::PageUp),
            SCROLL_PAGE_DOWN => Some(Scroll::PageDown),
            SCROLL_TOP => Some(Scroll::Top),
            SCROLL_BOTTOM => Some(Scroll::Bottom),
            PROMPT_PREVIOUS => Some(Scroll::PreviousPrompt),
            PROMPT_NEXT => Some(Scroll::NextPrompt),
            _ => None,
        };
        if let Some(scroll) = scroll {
            return cx.scroll(pane, scroll);
        }
        if name == COPY_OUTPUT || name == SELECT_OUTPUT {
            let Some(span) = output(cx, pane, &args)? else {
                return Ok(());
            };
            let ((_, first), (_, last)) = span;
            // A selection reads its last row alone, whose end may hold a
            // shell's mark of a missing newline.
            let (from, rows, reading) = if name == SELECT_OUTPUT {
                (last, 1, Reading::Select)
            } else {
                let rows = usize::try_from(last - first + 1)
                    .unwrap_or(0)
                    .min(MOST_OUTPUT_ROWS);
                (first, rows, Reading::Copy)
            };
            let id = cx.read_lines(pane, from, rows)?;
            self.reading = Some((id, pane, span, reading));
            return Ok(());
        }
        if name == SEND_TEXT {
            // The registry checked that the text is there, a string.
            return match args.get("text") {
                Some(Value::String(text)) => cx.send_text(pane, text),
                _ => Ok(()),
            };
        }
        match name {
            COPY => {
                if let Some(text) = cx.selection_text(pane)?
                    && !text.is_empty()
                {
                    cx.set_clipboard_text(&text)?;
                }
                Ok(())
            }
            PASTE => {
                if let Some(text) = cx.clipboard_text()?
                    && !text.is_empty()
                {
                    cx.paste(pane, &text)?;
                }
                Ok(())
            }
            SELECT_ALL => cx.select_all(pane),
            CLEAR => cx.clear(pane),
            _ => Err(Error::new(
                Code::NotFound,
                format!("core-actions has no command `{name}`"),
            )),
        }
    }
}

/// The cells of the output that `args` names in `pane`: of the command
/// whose rows hold the cell of its `column` and `row`, or, of none, of the
/// last command that ended; none of a command that has not ended, nor of
/// one of no output, nor of a pane whose shell marks none.
fn output(cx: &mut Cx<'_>, pane: PaneId, args: &Args) -> Result<Option<Span>, Error> {
    let number = |name: &str| match args.get(name) {
        Some(&Value::Integer(value)) if value >= 0 => Some(value),
        _ => None,
    };
    // The commands' rows are the primary screen's, which the alternate one,
    // as vim's or less's, hides.
    let view = cx.viewport(pane)?;
    if view.alternate {
        return Ok(None);
    }
    let row = number("column").and(number("row"));
    let (columns, _) = cx.pane_grid(pane)?;
    let span = |command: &ShellCommand| output_span(command, columns);
    if let Some(row) = row {
        let commands = cx.shell_commands(pane, row, row.saturating_add(1))?;
        return Ok(commands
            .iter()
            .rev()
            .find(|command| holds(command, row))
            .and_then(span));
    }
    // The recent commands first, as the menus ask at each look.
    let recent = cx.shell_commands(pane, view.screen.saturating_sub(RECENT_ROWS), i64::MAX / 2)?;
    if let Some(found) = recent.iter().rev().find_map(span) {
        return Ok(Some(found));
    }
    let commands = cx.shell_commands(pane, i64::MIN / 2, i64::MAX / 2)?;
    Ok(commands.iter().rev().find_map(span))
}

/// Whether `command`'s rows hold the row numbered `row`: from its first
/// mark's row to its end's, the end's row its own where the end is past
/// its first column, the next prompt's otherwise.
fn holds(command: &ShellCommand, row: i64) -> bool {
    let Some((_, first)) = command.prompt.or(command.input).or(command.output) else {
        return false;
    };
    let Some((end_column, end)) = command.end else {
        return false;
    };
    row >= first && (row < end || (row == end && end_column > 0))
}

/// The first and the last cells of `command`'s output, of a grid of
/// `columns`: from its output's mark to the cell before its end's; none of
/// a command that has not ended, nor of no output.
fn output_span(command: &ShellCommand, columns: u16) -> Option<Span> {
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
fn output_text(lines: &[Line], ((first_column, first), (last_column, last)): Span) -> String {
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
fn selected(lines: &[Line], (first, last): Span, columns: u16) -> Option<Span> {
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

#[cfg(test)]
mod tests;

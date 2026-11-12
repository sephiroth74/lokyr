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

/// The plugin of the basic terminal actions, and the copy of an output
/// whose rows it reads: the read's id, its pane, and the output's cells.
#[derive(Debug, Default)]
pub struct CoreActions {
    copying: Option<(u64, PaneId, Span)>,
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
                && let Some((asked, of, span)) = self.copying
                && asked == *id
                && of == *pane
            {
                self.copying = None;
                let text = output_text(lines, span);
                if !text.is_empty() {
                    cx.set_clipboard_text(&text)?;
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
            if name == SELECT_OUTPUT {
                return cx.select_cells(pane, span.0, span.1);
            }
            let ((_, first), (_, last)) = span;
            let rows = usize::try_from(last - first + 1)
                .unwrap_or(0)
                .min(MOST_OUTPUT_ROWS);
            let id = cx.read_lines(pane, first, rows)?;
            self.copying = Some((id, pane, span));
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
    let row = number("column").and(number("row"));
    let (columns, _) = cx.pane_grid(pane)?;
    let commands = match row {
        Some(row) => cx.shell_commands(pane, row, row.saturating_add(1))?,
        None => cx.shell_commands(pane, i64::MIN / 2, i64::MAX / 2)?,
    };
    let span = |command: &ShellCommand| output_span(command, columns);
    Ok(match row {
        Some(row) => commands
            .iter()
            .rev()
            .find(|command| holds(command, row))
            .and_then(span),
        None => commands.iter().rev().find_map(span),
    })
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
/// without the blank rows that start and end it.
fn output_text(lines: &[Line], ((first_column, first), (last_column, last)): Span) -> String {
    let mut text = String::new();
    for line in lines
        .iter()
        .filter(|line| line.row >= first && line.row <= last)
    {
        let chars: Vec<char> = line.text.chars().collect();
        let start = if line.row == first {
            usize::from(first_column).min(chars.len())
        } else {
            0
        };
        let end = if line.row == last {
            (usize::from(last_column) + 1).min(chars.len())
        } else {
            chars.len()
        };
        text.extend(&chars[start.min(end)..end]);
        if !line.wrapped || line.row == last {
            text.push('\n');
        }
    }
    let trimmed = text.trim_end().trim_start_matches(['\n', '\r']);
    trimmed.to_owned()
}

#[cfg(test)]
mod tests;

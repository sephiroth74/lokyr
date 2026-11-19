//! Built-in plugin of the commands' blocks: each command that the shell
//! marks, its prompt, its line and its output, is a block. While a pane's
//! view shows a command's output and the command's line is above it, the
//! command's header sits at the pane's top, a pill of the line on a
//! hairline across the pane, which a click scrolls to the command's
//! prompt; while the view does not show the block's end, a button beside
//! the pill scrolls there. The header is `lokyr/command-header`'s, of the
//! module that both hold, which this plugin replaces while it runs.
//!
//! Its commands act on a block: the one under a cell, as the pane's
//! context menu gives its `column` and `row`, or else the last command's.
//! They copy its line, or its line and its output, or select it whole.
//! Like every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind, Param, Value,
};
use lokyr_plugin_sdk::host::{PaneId, ShellCommand};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::Node;

// The command's header, of `lokyr/command-header`'s source, and the text of
// a command's output, of `lokyr/core-actions`', as a built-in plugin
// depends on no crate but the SDK.
#[path = "../../command-header/src/header.rs"]
mod header;
#[path = "../../core-actions/src/output.rs"]
mod output;

use header::Headers;
use output::{Span, output_span, output_text, selected};

lokyr_plugin_sdk::export_plugin!(Blocks);

/// The plugin's catalogs, of the header's label and tooltips and of its
/// commands' titles.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/blocks.ftl")),
    ("it", include_str!("../locales/it/blocks.ftl")),
];

/// The prefix of a header's panel's id, of its pane's id.
const PANEL: &str = "block-header-";

/// Copy a block's command line; copy its command line and its output;
/// select it whole, from its prompt to its output's end. Each of the block
/// at the cell of its `column` and `row`, as the pane's context menu gives
/// them, or, of none, of the last command.
const COPY_COMMAND: &str = "blocks.copy-command";
const COPY_BLOCK: &str = "blocks.copy-block";
const SELECT_BLOCK: &str = "blocks.select-block";

/// The context of the commands: a focused pane.
const PANE: &str = "pane.focused";

/// The rows that a copy of an output reads at most, as a read of rows
/// takes them.
const MOST_OUTPUT_ROWS: usize = lokyr_plugin_sdk::host::MOST_LINES;

/// The rows back from the screen whose commands a look for the last
/// command reads first, before every command's.
const RECENT_ROWS: i64 = 10_000;

/// The plugin: the panes' headers, of a button to their blocks' ends, and
/// the read of a block's rows under way.
#[derive(Debug)]
pub struct Blocks {
    headers: Headers,
    reading: Option<Reading>,
}

/// A read of a block's rows under way: the read's id, its pane, and what
/// its rows go to.
#[derive(Debug)]
struct Reading {
    id: u64,
    pane: PaneId,
    to: Destination,
}

/// What a block's rows that a read gives go to.
#[derive(Debug)]
enum Destination {
    /// The clipboard, of the command's line, if it has one, then of its
    /// output's cells.
    Copy { line: Option<String>, output: Span },
    /// A selection of the block's cells, its last row read for the mark of
    /// a missing newline.
    Select { block: Span },
}

impl Default for Blocks {
    fn default() -> Self {
        Self {
            headers: Headers::new(PANEL, true),
            reading: None,
        }
    }
}

impl Plugin for Blocks {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.headers.activate(cx)?;
        // A block: its cell's, as the context menu gives it, or, of none,
        // the last command's.
        let cell = |name: &str| Param {
            name: name.to_owned(),
            kind: Kind::Integer,
            default: Some(Value::Integer(-1)),
        };
        let commands = [
            (
                COPY_COMMAND,
                "Copy Command",
                Category::Clipboard,
                vec![Capability::HistoryRead, Capability::ClipboardWrite],
            ),
            (
                COPY_BLOCK,
                "Copy Command and Output",
                Category::Clipboard,
                vec![
                    Capability::HistoryRead,
                    Capability::TerminalRead,
                    Capability::ClipboardWrite,
                ],
            ),
            (
                SELECT_BLOCK,
                "Select Block",
                Category::Edit,
                vec![Capability::HistoryRead, Capability::TerminalRead],
            ),
        ];
        for (name, title, category, capabilities) in commands {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category,
                capabilities,
                params: vec![cell("column"), cell("row")],
                when: Some(PANE.to_owned()),
            })?;
        }
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("../blocks.schema.json").into())
    }

    /// The headers follow the panes' views and commands; the rows of a
    /// block that a command read go to the clipboard or the selection.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.headers.on_event(cx, events)?;
        for event in events {
            let Event::Lines {
                id, pane, lines, ..
            } = event
            else {
                continue;
            };
            let Some(reading) = self.reading.take_if(|reading| reading.id == *id) else {
                continue;
            };
            if reading.pane != *pane {
                continue;
            }
            match reading.to {
                Destination::Copy { line, output } => {
                    let text = joined(line.as_deref(), &output_text(lines, output));
                    if !text.is_empty() {
                        cx.set_clipboard_text(&text)?;
                    }
                }
                Destination::Select { block } => {
                    let (columns, _) = cx.pane_grid(*pane)?;
                    if let Some((first, last)) = selected(lines, block, columns) {
                        cx.select_cells(*pane, first, last)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        self.headers.render(cx, view)
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.headers.on_ui_event(cx, event)
    }

    /// A command applies where its block has what it takes: a line to
    /// copy, or a command that ended.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        let Some(pane) = cx.focused_pane() else {
            return false;
        };
        match name {
            COPY_COMMAND => block(cx, pane, args, false)
                .ok()
                .flatten()
                .and_then(|command| cx.command_line(pane, command.id).ok().flatten())
                .is_some(),
            COPY_BLOCK | SELECT_BLOCK => block(cx, pane, args, true).ok().flatten().is_some(),
            _ => true,
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        match name {
            COPY_COMMAND => {
                let Some(command) = block(cx, pane, &args, false)? else {
                    return Ok(());
                };
                if let Some(line) = cx.command_line(pane, command.id)? {
                    cx.set_clipboard_text(&line)?;
                }
                Ok(())
            }
            COPY_BLOCK => {
                let Some(command) = block(cx, pane, &args, true)? else {
                    return Ok(());
                };
                let line = cx.command_line(pane, command.id)?;
                let (columns, _) = cx.pane_grid(pane)?;
                // An output whose mark left the scrollback starts at the
                // pane's oldest row.
                let oldest = cx.viewport(pane)?.oldest;
                let held = ShellCommand {
                    output: command.output.or(Some((0, oldest))),
                    ..command
                };
                match output_span(&held, columns) {
                    Some(output) => {
                        let ((_, first), (_, last)) = output;
                        let rows = usize::try_from(last - first + 1)
                            .unwrap_or(0)
                            .min(MOST_OUTPUT_ROWS);
                        let id = cx.read_lines(pane, first, rows)?;
                        self.reading = Some(Reading {
                            id,
                            pane,
                            to: Destination::Copy { line, output },
                        });
                    }
                    // A command of no output: its line alone.
                    None => {
                        if let Some(line) = line {
                            cx.set_clipboard_text(&line)?;
                        }
                    }
                }
                Ok(())
            }
            SELECT_BLOCK => {
                let Some(command) = block(cx, pane, &args, true)? else {
                    return Ok(());
                };
                let (columns, _) = cx.pane_grid(pane)?;
                let oldest = cx.viewport(pane)?.oldest;
                let Some(block) = block_span(&command, columns, oldest) else {
                    return Ok(());
                };
                // Its last row alone, whose end may hold a shell's mark of
                // a missing newline.
                let id = cx.read_lines(pane, block.1.1, 1)?;
                self.reading = Some(Reading {
                    id,
                    pane,
                    to: Destination::Select { block },
                });
                Ok(())
            }
            _ => Err(Error::new(
                Code::NotFound,
                format!("blocks has no command `{name}`"),
            )),
        }
    }
}

/// The block that `args` names in `pane`: of the command whose block holds
/// the cell of its `column` and `row`, or, of none, the last command's,
/// each of an output's mark, as a command that started, and of an end too
/// if `ended`; none on the alternate screen, as vim's or less's, which
/// hides the commands' rows, nor of a pane whose shell marks none.
fn block(
    cx: &mut Cx<'_>,
    pane: PaneId,
    args: &Args,
    ended: bool,
) -> Result<Option<ShellCommand>, Error> {
    let number = |name: &str| match args.get(name) {
        Some(&Value::Integer(value)) if value >= 0 => Some(value),
        _ => None,
    };
    let view = cx.viewport(pane)?;
    if view.alternate {
        return Ok(None);
    }
    let takes = |command: &ShellCommand| started(command) && (!ended || command.end.is_some());
    if let Some(row) = number("column").and(number("row")) {
        return Ok(cx.command_at(pane, row)?.filter(takes));
    }
    // The recent commands first, as the menus ask at each look.
    let recent = cx.shell_commands(pane, view.screen.saturating_sub(RECENT_ROWS), i64::MAX / 2)?;
    if let Some(found) = recent.into_iter().rev().find(takes) {
        return Ok(Some(found));
    }
    let commands = cx.shell_commands(pane, i64::MIN / 2, i64::MAX / 2)?;
    Ok(commands.into_iter().rev().find(takes))
}

/// Whether `command` started: of its output's mark, or of no mark before
/// its end that the pane holds, as its output's mark left the scrollback
/// with them.
fn started(command: &ShellCommand) -> bool {
    command.output.is_some()
        || (command.prompt.is_none() && command.input.is_none() && command.end.is_some())
}

/// The first and the last cells of `command`'s block, of a grid of
/// `columns`, whose oldest row is `oldest`: from its prompt, or its first
/// mark that the pane holds, or the oldest row of a block whose marks before
/// its end left, to the cell before its end's; none of a command that has
/// not ended.
fn block_span(command: &ShellCommand, columns: u16, oldest: i64) -> Option<Span> {
    let first = command
        .prompt
        .or(command.input)
        .or(command.output)
        .unwrap_or((0, oldest));
    let (end_column, end_row) = command.end?;
    let last = if end_column == 0 {
        (columns.saturating_sub(1), end_row - 1)
    } else {
        (end_column - 1, end_row)
    };
    (last.1 > first.1 || (last.1 == first.1 && last.0 >= first.0)).then_some((first, last))
}

/// A block's text: its command's `line`, if it has one, and its `output`,
/// if it has some, a line apart.
fn joined(line: Option<&str>, output: &str) -> String {
    match (line, output.is_empty()) {
        (Some(line), false) => format!("{line}\n{output}"),
        (Some(line), true) => line.to_owned(),
        (None, _) => output.to_owned(),
    }
}

#[cfg(test)]
mod tests;

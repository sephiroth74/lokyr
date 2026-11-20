//! Built-in plugin of the commands' blocks: each command that the shell
//! marks, its prompt, its line and its output, is a block. While a pane's
//! view shows a command's output and the command's line is above it, the
//! command's header sits at the pane's top, a pill of the line on a
//! hairline across the pane, which a click scrolls to the command's
//! prompt; while the view does not show the block's end, a button beside
//! the pill scrolls there. The header is `lokyr/command-header`'s, of the
//! module that both hold, which this plugin replaces while it runs. The
//! host draws the blocks as it asks: the gutter's bar of each command along
//! its block, a divider at each block's top, and a failed block's tint, as
//! its config says.
//!
//! A block is selected of the keyboard: ⌘↑ and ⌘↓ jump to the prompt before
//! or after, as the prompts' jumps do, and select its block, which the host
//! outlines; while one is selected, ↑ and ↓ move the selection and the view
//! to the block before or after, and Escape, a key typed or a click ends
//! it.
//!
//! Its commands act on a block: the one under a cell, as the pane's
//! context menu gives its `column` and `row`, or else the selected one, or
//! else the last command's.
//! They copy its line, or its line and its output, or select it whole, or
//! show its details in a panel: its line, its exit status, its start, its
//! duration and its folder, each copied of a click, or run its line again
//! at the shell's prompt, while the shell waits there of an empty command
//! line. Like every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind, Param, Value,
};
use lokyr_plugin_sdk::host::{BlockLooks, LocalTime, NoticeLevel, PaneId, Scroll, ShellCommand};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{
    Anchor, Corner, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId,
};
use lokyr_plugin_sdk::view::{
    Align, Border, Button, ControlSize, Direction, Edges, FontFamily, Hover, Kind as NodeKind,
    Layout, Length, Node, PointerShape, Role, Shadow, Style, Token, UiEventKind,
};

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

/// Show a block's details in a panel at its cell, of `column` and `row`,
/// or, of none, the last command's at the cursor.
const DETAILS: &str = "blocks.details";

/// Run a block's command line again, at the shell's prompt, the block of
/// a cell, of `column` and `row`, or, of none, the last command's.
const RERUN: &str = "blocks.rerun";

/// Select the block before or after the selected one, or of the prompt
/// before or after the view's top, as the prompts' jumps go, at the view's
/// top; select none.
const SELECT_PREVIOUS: &str = "blocks.select-previous";
const SELECT_NEXT: &str = "blocks.select-next";
const DESELECT: &str = "blocks.deselect";

/// The prefix of a details' panel's id, of a number that each opening
/// takes, so that a panel that opens in place of another is not the one
/// whose closing the host tells.
const DETAILS_PANEL: &str = "block-details-";

/// The width of a details' panel, in points.
const DETAILS_WIDTH: f32 = 340.0;

/// The width of the names of a details' parts, in points.
const NAME_WIDTH: f32 = 72.0;

/// The most lines of a command's line that a details' panel shows.
const LINE_LINES: u16 = 3;

/// The context of the commands: a focused pane.
const PANE: &str = "pane.focused";

/// The rows that a copy of an output reads at most, as a read of rows
/// takes them.
const MOST_OUTPUT_ROWS: usize = lokyr_plugin_sdk::host::MOST_LINES;

/// The rows back from the screen whose commands a look for the last
/// command reads first, before every command's.
const RECENT_ROWS: i64 = 10_000;

/// The plugin: the panes' headers, of a button to their blocks' ends, the
/// read of a block's rows under way, and the details' panel that shows.
#[derive(Debug)]
pub struct Blocks {
    headers: Headers,
    reading: Option<Reading>,
    details: Option<Details>,
    /// The number of the last details' panel that opened.
    opened: u64,
    /// How the host draws the blocks, as the plugin last asked.
    looks: BlockLooks,
}

/// A block's details that a panel shows: its panel, its pane, its command,
/// its line, the system's time as it opened or the command ended, and the
/// part that a click copied last.
#[derive(Debug)]
struct Details {
    panel: PanelId,
    pane: PaneId,
    command: ShellCommand,
    line: Option<String>,
    now: LocalTime,
    copied: Option<Part>,
}

/// A part of a block's details, which a click copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Line,
    Status,
    Started,
    Duration,
    Folder,
}

impl Part {
    /// The key of its row in the panel.
    fn key(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Status => "status",
            Self::Started => "started",
            Self::Duration => "duration",
            Self::Folder => "folder",
        }
    }

    /// The part of a key of its row, or of its copy's button, `copy-` and
    /// its row's key.
    fn of(key: &str) -> Option<Self> {
        let key = key.strip_prefix("copy-").unwrap_or(key);
        [
            Self::Line,
            Self::Status,
            Self::Started,
            Self::Duration,
            Self::Folder,
        ]
        .into_iter()
        .find(|part| part.key() == key)
    }
}

/// A row of a block's details: its part, its name, the text that it shows,
/// an icon of its state, of a token, and the text that a click copies,
/// none of a row that copies nothing, and the label of its copy's button.
#[derive(Debug, PartialEq)]
struct Row {
    part: Part,
    name: String,
    shown: String,
    icon: Option<(&'static str, &'static str)>,
    copied: Option<String>,
    copy: String,
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
            details: None,
            opened: 0,
            looks: BlockLooks::default(),
        }
    }
}

impl Plugin for Blocks {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.headers.activate(cx)?;
        self.looks = looks_of(&cx.config()?);
        cx.set_block_looks(self.looks)?;
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
        let commands = commands.into_iter().chain([
            (
                DETAILS,
                "Block Details",
                Category::Edit,
                vec![
                    Capability::HistoryRead,
                    Capability::ProcessRead,
                    Capability::ClipboardWrite,
                ],
            ),
            (
                RERUN,
                "Run Command Again",
                Category::Edit,
                vec![
                    Capability::HistoryRead,
                    Capability::TerminalRead,
                    Capability::PtyWrite,
                ],
            ),
        ]);
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
        for (name, title) in [
            (SELECT_PREVIOUS, "Select Previous Block"),
            (SELECT_NEXT, "Select Next Block"),
            (DESELECT, "Deselect Block"),
        ] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category: Category::View,
                capabilities: vec![Capability::HistoryRead, Capability::PaneView],
                params: Vec::new(),
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
        self.follow_details(cx, events)?;
        if events.contains(&Event::ConfigChanged) {
            let looks = looks_of(&cx.config()?);
            if looks != self.looks {
                self.looks = looks;
                cx.set_block_looks(looks)?;
            }
        }
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
        match &self.details {
            Some(details) if *view == ViewId::Panel(details.panel.clone()) => {
                let rows = rows(cx, details);
                let line = details.line.clone().unwrap_or_default();
                let label = cx
                    .message("details-label", &[("line", Data::Text(line))])
                    .unwrap_or_default();
                Some(details_tree(&rows, details.copied, &label))
            }
            _ => self.headers.render(cx, view),
        }
    }

    /// A click on a row of the details, or on its button, copies its part.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let Some(details) = &self.details else {
            return self.headers.on_ui_event(cx, event);
        };
        if event.view != ViewId::Panel(details.panel.clone()) {
            return self.headers.on_ui_event(cx, event);
        }
        let (UiEventKind::Click, Some(part)) = (&event.kind, Part::of(&event.key.0)) else {
            return Ok(());
        };
        let Some(text) = rows(cx, details)
            .into_iter()
            .find(|row| row.part == part)
            .and_then(|row| row.copied)
        else {
            return Ok(());
        };
        cx.set_clipboard_text(&text)?;
        let panel = details.panel.clone();
        if let Some(details) = &mut self.details {
            details.copied = Some(part);
        }
        cx.request_render(ViewId::Panel(panel))
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
            DETAILS | RERUN => block(cx, pane, args, false).ok().flatten().is_some(),
            DESELECT => cx.selected_block(pane).ok().flatten().is_some(),
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
            DETAILS => {
                let Some(command) = block(cx, pane, &args, false)? else {
                    return Ok(());
                };
                self.open_details(cx, pane, command, &args)
            }
            RERUN => {
                let Some(command) = block(cx, pane, &args, false)? else {
                    return Ok(());
                };
                rerun(cx, pane, &command)
            }
            SELECT_PREVIOUS => select_step(cx, pane, false),
            SELECT_NEXT => select_step(cx, pane, true),
            DESELECT => cx.select_block(pane, None),
            _ => Err(Error::new(
                Code::NotFound,
                format!("blocks has no command `{name}`"),
            )),
        }
    }
}

impl Blocks {
    /// Opens the details of `command`, of `pane`, in a panel below the cell
    /// of `args`, or, of none, below the cursor while the view shows the
    /// screen, at the pane's top otherwise; in place of the details that
    /// show.
    fn open_details(
        &mut self,
        cx: &mut Cx<'_>,
        pane: PaneId,
        command: ShellCommand,
        args: &Args,
    ) -> Result<(), Error> {
        let line = cx.command_line(pane, command.id)?;
        let now = cx.now()?;
        let anchor = match cell_of(args) {
            Some((column, row)) => Anchor::Cell { pane, column, row },
            None => {
                let view = cx.viewport(pane)?;
                if view.at_screen() {
                    Anchor::Cursor(pane)
                } else {
                    Anchor::PaneCorner {
                        pane,
                        corner: Corner::Top,
                    }
                }
            }
        };
        if let Some(shown) = self.details.take() {
            cx.close_panel(shown.panel)?;
        }
        self.opened += 1;
        let panel = PanelId(format!("{DETAILS_PANEL}{}", self.opened));
        cx.open_panel(
            panel.clone(),
            PanelOptions {
                anchor,
                role: PanelRole::Dialog,
                close_outside: true,
                close_escape: true,
                takes_keys: false,
                close_typing: false,
            },
        )?;
        self.details = Some(Details {
            panel,
            pane,
            command,
            line,
            now,
            copied: None,
        });
        Ok(())
    }

    /// The details follow their command: its end shows in them as it comes.
    /// They go as their panel closes.
    fn follow_details(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            let Some(details) = &self.details else {
                return Ok(());
            };
            match event {
                Event::PanelClosed(id) if *id == details.panel => self.details = None,
                Event::CommandEnded { pane, command, .. }
                    if *pane == details.pane && *command == details.command.id =>
                {
                    let (pane, panel) = (details.pane, details.panel.clone());
                    let Some(ended) = cx.shell_command(pane, *command)? else {
                        continue;
                    };
                    let now = cx.now()?;
                    if let Some(details) = &mut self.details {
                        details.command = ended;
                        details.now = now;
                    }
                    cx.request_render(ViewId::Panel(panel))?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Runs `command`'s line again in `pane`, at the shell's prompt, while
/// the shell edits an empty command line there; a notice of why not
/// otherwise, of nothing sent: of a block of no line, of a shell that is
/// not at its prompt, as a command runs, and of a command line that holds
/// text, which the run would add to.
fn rerun(cx: &mut Cx<'_>, pane: PaneId, command: &ShellCommand) -> Result<(), Error> {
    let why = match cx.command_line(pane, command.id)? {
        None => "rerun-no-line",
        Some(line) => match cx.edited_line(pane)? {
            None => "rerun-busy",
            Some(edited) if !edited.is_empty() => "rerun-typed",
            Some(_) => match cx.run_line(pane, &line) {
                Ok(()) => return Ok(()),
                // The prompt went between the read and the run.
                Err(error) if error.code == Code::Unavailable => "rerun-busy",
                Err(error) => return Err(error),
            },
        },
    };
    let text = cx.message(why, &[])?;
    let title = cx.message("rerun-title", &[])?;
    match cx.notify(&text, NoticeLevel::Warning, Some(&title)) {
        // The notice of a second ago, of the same why, still shows.
        Err(error) if error.code == Code::Unavailable => Ok(()),
        done => done,
    }
}

/// How the host draws the blocks, as `config` says: the gutter's bars along
/// them, their dividers, `dividers`, and a failed block's tint,
/// `failed-tint`, of the schema's defaults where it says nothing.
fn looks_of(config: &Data) -> BlockLooks {
    let switch = |key: &str, default: bool| match config {
        Data::Map(config) => match config.get(key) {
            Some(Data::Boolean(on)) => *on,
            _ => default,
        },
        _ => default,
    };
    BlockLooks {
        bars: true,
        dividers: switch("dividers", true),
        failed_tint: switch("failed-tint", false),
    }
}

/// The cell that `args` names, of its `column` and `row`, if it names one.
fn cell_of(args: &Args) -> Option<(u16, i64)> {
    let column = match args.get("column") {
        Some(&Value::Integer(column)) => u16::try_from(column).ok()?,
        _ => return None,
    };
    match args.get("row") {
        Some(&Value::Integer(row)) if row >= 0 => Some((column, row)),
        _ => None,
    }
}

/// The rows of `details`: its line; its exit status, or that it runs; the
/// time of its start, its time of day of the local time zone, of its date
/// too where it is not today's; its duration, once it ended; and its
/// folder, of its machine where it is another one, as the shell told it.
fn rows(cx: &mut Cx<'_>, details: &Details) -> Vec<Row> {
    let mut text = |id: &str, args: &[(&str, Data)]| cx.message(id, args).unwrap_or_default();
    let command = &details.command;
    let mut rows = Vec::new();
    if let Some(line) = &details.line {
        rows.push(Row {
            part: Part::Line,
            name: text("details-line", &[]),
            shown: line.clone(),
            icon: None,
            copied: Some(line.clone()),
            copy: text("details-copy-line", &[]),
        });
    }
    let status = |status: i32| [("status", Data::Integer(i64::from(status)))];
    let (shown, icon, copied) = match (command.end, command.status) {
        (None, _) => (
            text("details-running", &[]),
            ("circle-filled", "gutter.running"),
            None,
        ),
        (Some(_), Some(0)) => (
            text("details-succeeded", &status(0)),
            ("check", "gutter.succeeded"),
            Some("0".to_owned()),
        ),
        (Some(_), Some(code)) => (
            text("details-failed", &status(code)),
            ("error", "gutter.failed"),
            Some(code.to_string()),
        ),
        (Some(_), None) => (
            text("details-ended", &[]),
            ("circle-filled", "gutter.succeeded"),
            None,
        ),
    };
    rows.push(Row {
        part: Part::Status,
        name: text("details-status", &[]),
        shown,
        icon: Some(icon),
        copied,
        copy: text("details-copy-status", &[]),
    });
    if let Some(started) = command.started {
        let shown = clock_time(details.now, started);
        rows.push(Row {
            part: Part::Started,
            name: text("details-started", &[]),
            shown: shown.clone(),
            icon: None,
            copied: Some(shown),
            copy: text("details-copy-started", &[]),
        });
    }
    if let (Some(started), Some(ended), Some(_)) = (command.started, command.ended, command.end) {
        let shown = duration(&mut text, ended.saturating_sub(started));
        rows.push(Row {
            part: Part::Duration,
            name: text("details-duration", &[]),
            shown: shown.clone(),
            icon: None,
            copied: Some(shown),
            copy: text("details-copy-duration", &[]),
        });
    }
    if let Some(folder) = &command.folder {
        let shown = match &command.host {
            Some(host) => format!("{host}:{folder}"),
            None => folder.clone(),
        };
        rows.push(Row {
            part: Part::Folder,
            name: text("details-folder", &[]),
            shown: shown.clone(),
            icon: None,
            copied: Some(shown),
            copy: text("details-copy-folder", &[]),
        });
    }
    rows
}

/// The local time of day of `time`, in milliseconds since 1970 in UTC, as
/// `24:00:00`, of `now`'s time zone, its date first, as `2026-11-19`, where
/// it is not `now`'s day.
fn clock_time(now: LocalTime, time: u64) -> String {
    let at = now.civil(time);
    let today = now.civil(now.time);
    let clock = format!("{:02}:{:02}:{:02}", at.hour, at.minute, at.second);
    if (at.year, at.month, at.day) == (today.year, today.month, today.day) {
        clock
    } else {
        format!("{:04}-{:02}-{:02} {clock}", at.year, at.month, at.day)
    }
}

/// A duration of `millis` milliseconds, as the catalogs write it: its
/// milliseconds below a second, its seconds and tenths below a minute, its
/// minutes and seconds below an hour, and its hours and minutes past it.
fn duration(text: &mut impl FnMut(&str, &[(&str, Data)]) -> String, millis: u64) -> String {
    let number = |value: u64| Data::Integer(i64::try_from(value).unwrap_or(i64::MAX));
    let seconds = millis / 1000;
    if millis < 1000 {
        text("details-milliseconds", &[("count", number(millis))])
    } else if seconds < 60 {
        let tenths = millis / 100;
        text(
            "details-seconds",
            &[
                ("whole", number(tenths / 10)),
                ("tenths", number(tenths % 10)),
            ],
        )
    } else if seconds < 3600 {
        text(
            "details-minutes",
            &[
                ("minutes", number(seconds / 60)),
                ("seconds", number(seconds % 60)),
            ],
        )
    } else {
        text(
            "details-hours",
            &[
                ("hours", number(seconds / 3600)),
                ("minutes", number(seconds % 3600 / 60)),
            ],
        )
    }
}

/// The details' panel of `rows`, which `label` names to accessibility: a
/// card of the command's line, in the terminal's font, of three lines at
/// most, above a hairline and the other parts' rows, each of its name, its
/// state's icon and its text, and of a button that copies it, as a click
/// on its row does, whose icon is a tick once it copied, as `copied` says.
fn details_tree(rows: &[Row], copied: Option<Part>, label: &str) -> Node {
    let copy_button = |row: &Row| {
        let done = copied == Some(row.part);
        Node::new(NodeKind::Button(Button {
            icon: if done { "check" } else { "copy" }.to_owned(),
            size: ControlSize::Small,
            ..Button::default()
        }))
        .key(format!("copy-{}", row.part.key()).as_str())
        .label(row.copy.as_str())
        .tooltip(row.copy.as_str())
    };
    let row_node = |row: &Row| {
        let line = row.part == Part::Line;
        let mut children = Vec::new();
        if !line {
            children.push(
                Node::text(row.name.as_str())
                    .layout(Layout {
                        width: Length::Points(NAME_WIDTH),
                        ..Layout::default()
                    })
                    .style(Style {
                        color: Some(Token::from("text.secondary")),
                        ..Style::default()
                    }),
            );
        }
        if let Some((icon, color)) = row.icon {
            children.push(Node::icon(icon).style(Style {
                color: Some(Token::from(color)),
                ..Style::default()
            }));
        }
        // Of no least width, so that the layout shrinks it within the card,
        // which the host's ellipsis cuts.
        children.push(
            Node::text(row.shown.as_str())
                .layout(Layout {
                    grow: 1.0,
                    min_width: Length::Points(0.0),
                    lines: line.then_some(LINE_LINES),
                    ..Layout::default()
                })
                .style(Style {
                    font_family: line.then_some(FontFamily::Terminal),
                    ..Style::default()
                }),
        );
        let copies = row.copied.is_some();
        if copies {
            children.push(copy_button(row));
        }
        let node = Node::container(children)
            .layout(Layout {
                direction: Direction::Row,
                gap: 6.0,
                align: if line { Align::Start } else { Align::Center },
                padding: Edges {
                    top: 3.0,
                    right: 4.0,
                    bottom: 3.0,
                    left: 6.0,
                },
                ..Layout::default()
            })
            .style(Style {
                radius: 6.0,
                pointer: copies.then_some(PointerShape::PointingHand),
                hover: copies.then(|| Hover {
                    background: Some(Token::from("button.hover.bg")),
                    ..Hover::default()
                }),
                ..Style::default()
            });
        if copies {
            node.key(row.part.key())
        } else {
            node
        }
    };
    let mut children = Vec::new();
    for row in rows {
        children.push(row_node(row));
        if row.part == Part::Line {
            children.push(
                Node::container(Vec::new())
                    .layout(Layout {
                        height: Length::Points(1.0),
                        ..Layout::default()
                    })
                    .style(Style {
                        background: Some(Token::from("separator")),
                        ..Style::default()
                    }),
            );
        }
    }
    Node::container(children)
        .key("details")
        .role(Role::Group)
        .label(label)
        .layout(Layout {
            direction: Direction::Column,
            gap: 4.0,
            width: Length::Points(DETAILS_WIDTH),
            padding: Edges::all(8.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 10.0,
            shadow: Some(Shadow::default()),
            ..Style::default()
        })
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
    if let Some(selected) = cx.selected_block(pane)? {
        return Ok(Some(selected).filter(takes));
    }
    // The recent commands first, as the menus ask at each look.
    let recent = cx.shell_commands(pane, view.screen.saturating_sub(RECENT_ROWS), i64::MAX / 2)?;
    if let Some(found) = recent.into_iter().rev().find(takes) {
        return Ok(Some(found));
    }
    let commands = cx.shell_commands(pane, i64::MIN / 2, i64::MAX / 2)?;
    Ok(commands.into_iter().rev().find(takes))
}

/// Selects the block after the selected one, `forward`, or before it, and
/// takes the view to its first row; past the last, none, and the view to
/// the screen. Of none selected: before, the last block while the view
/// shows the screen, as the prompt where the user types follows it, else
/// the block of the prompt before the view's top, as the prompts' jumps
/// go; after, the block of the prompt after the view's top, else the
/// screen.
fn select_step(cx: &mut Cx<'_>, pane: PaneId, forward: bool) -> Result<(), Error> {
    let view = cx.viewport(pane)?;
    if view.alternate {
        return Ok(());
    }
    // The ids of the commands that the pane keeps, from the oldest's to the
    // last's, which a walk from one to the next goes through, past the ids
    // of the prompts that a shell drew again, which none keeps; the oldest
    // past the rows before the first prompt, as a login's line.
    let end = view.screen.saturating_add(i64::from(view.rows));
    let oldest = first_from(cx, pane, view.oldest, end)?;
    let newest = cx
        .shell_commands(pane, view.screen, i64::MAX / 2)?
        .last()
        .map(|last| last.id);
    let (Some(first), Some(last)) = (oldest.map(|oldest| oldest.id), newest) else {
        return Ok(());
    };
    let walk = |cx: &mut Cx<'_>,
                from: u64,
                takes: &dyn Fn(&ShellCommand) -> bool|
     -> Result<Option<ShellCommand>, Error> {
        let mut id = from;
        loop {
            id = if forward {
                match id.checked_add(1).filter(|&next| next <= last) {
                    Some(next) => next,
                    None => return Ok(None),
                }
            } else {
                match id.checked_sub(1).filter(|&before| before >= first) {
                    Some(before) => before,
                    None => return Ok(None),
                }
            };
            if let Some(command) = cx.shell_command(pane, id)?
                && takes(&command)
            {
                return Ok(Some(command));
            }
        }
    };
    let found = match cx.selected_block(pane)? {
        Some(selected) => walk(cx, selected.id, &started)?,
        None => {
            // The command whose block holds the view's top row: before it,
            // its own if its prompt is above the top; after it, the next.
            let top = cx.shell_commands(pane, view.top, view.top + 1)?;
            let at = top.first().cloned();
            let above =
                |command: &ShellCommand| first_row(command).is_some_and(|row| row < view.top);
            match (at, forward) {
                (_, false) if view.at_screen() => walk(cx, last.saturating_add(1), &started)?,
                (Some(at), false) if above(&at) && started(&at) => Some(at),
                (Some(at), false) => {
                    walk(cx, at.id, &|command| started(command) && above(command))?
                }
                (Some(at), true) => walk(cx, at.id, &|command| {
                    started(command) && first_row(command).is_some_and(|row| row > view.top)
                })?,
                (None, false) => None,
                // The view's top above the first prompt, as a login's line.
                (None, true) => walk(cx, first.saturating_sub(1), &|command| {
                    started(command) && first_row(command).is_some_and(|row| row > view.top)
                })?,
            }
        }
    };
    match found {
        Some(command) => {
            cx.select_block(pane, Some(command.id))?;
            if let Some(row) = first_row(&command) {
                cx.scroll(pane, Scroll::Row(row))?;
            }
            Ok(())
        }
        // Past the last block, back to the prompt where the user types.
        None if forward => {
            cx.select_block(pane, None)?;
            cx.scroll(pane, Scroll::Bottom)
        }
        None => Ok(()),
    }
}

/// The first command of `pane` whose block starts at the row `from` or
/// below it, before the row `end`, of the rows that a window of a doubling
/// height reads, as rows of no command, as a login's line, may come before
/// the first prompt.
fn first_from(
    cx: &mut Cx<'_>,
    pane: PaneId,
    from: i64,
    end: i64,
) -> Result<Option<ShellCommand>, Error> {
    let mut height: i64 = 64;
    loop {
        let to = from.saturating_add(height).min(end);
        let found = cx
            .shell_commands(pane, from, to)?
            .into_iter()
            .find(|command| first_row(command).is_some_and(|row| row >= from));
        if found.is_some() || to >= end {
            return Ok(found);
        }
        height = height.saturating_mul(2);
    }
}

/// The first row of `command`'s block that the pane holds: its prompt's, or
/// its first mark's.
fn first_row(command: &ShellCommand) -> Option<i64> {
    command
        .prompt
        .or(command.input)
        .or(command.output)
        .or(command.end)
        .map(|(_, row)| row)
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

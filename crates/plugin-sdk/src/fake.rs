//! A host of the tests' own, for plugins' unit tests, as the `fake` feature
//! gives it.
//!
//! The test sets what the host holds: panes, each of its window, its tab,
//! its rows' lines and texts, its links, its styles, its selection and its
//! program's directory, the focused pane, the clipboard, the text's size,
//! the targets that are not there, and the plugin's config and state. The
//! host answers the plugin's requests as the app's does: a read with what it
//! holds as it handles it, a request that the plugin's capabilities lack,
//! or of a pane that is not there, with its error; a write and a request of
//! the app's queued, and applied as the call returns, the writes first,
//! then the app's requests, so that a read after a write, in the same call,
//! reads what was before, and a write to a tab or a pane that an earlier
//! write of the call opens finds it.
//!
//! Its workspace is the app's, of the panes that the test gives: the
//! windows, each of its tabs in their order and the one that it shows; the
//! tabs, each of its layout, the pane of its keyboard and its zoom; the
//! focused window, whose tab and pane the focused pane's are. The plugin's
//! writes change it as the app's workspace changes, a new tab, a split, a
//! close, a move, a focus, a zoom and a split's ratio, the events of each
//! owed to the plugin in the app's order; a drag of a pane's edge,
//! `resize_pane`, changes nothing of it, as it has no sizes.
//!
//! It keeps what the plugin declared, its commands, its bindings and its
//! subscriptions; what it did, its writes and its requests of the app's, as
//! the requests themselves, in their order, which a test asserts; its
//! decorations, its marks of the panes' gutters and its panels; the events
//! that it is owed, which
//! [`FakeHost::deliver`] gives it; and the trees of its views, which
//! [`snapshot`] writes as text.
//!
//! ```ignore
//! let mut host = FakeHost::new("core-actions");
//! host.pane(PaneId(7)).selection = Some("ls -la".to_owned());
//! host.focus(Some(PaneId(7)));
//! host.command(&mut CoreActions, "pane.copy", Args::new())?;
//! assert_eq!(host.take_done(), [Request::SetClipboardText("ls -la".to_owned())]);
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::command::{Args, Capability, Code, Command, Error, Uses, is_name};
use crate::event::WorkspaceEvent;
use crate::host::{
    self, BarPlace, Binding, Color, DecorationLook, Direction, Edge, GroupId, GroupInfo, Layout,
    LineText, MOST_STATE, PaneId, ScreenPoint, SessionFolders, SessionId, SessionInfo, Side,
    StyleRun, TabId, TabInfo, WindowId, WindowInfo,
};
use crate::keys::{When, parse_keys};
use crate::message::{
    Call, DELAY_RANGE, DIM_RANGE, Data, Effect, Event, OWN_PANE, Request, Response, TIMERS_BOUND,
};
use crate::plugin::{Cx, Plugin};
use crate::ui::{Anchor, PanelId, PanelOptions, Slot, UiEvent, ViewId};
use crate::view::{Key, Kind, Node};

/// The first id that the host gives a window, a tab, a pane, a decoration
/// or a read of its own, past those that a test gives its panes and its
/// tabs.
pub const FIRST_ID: u64 = 1001;

/// The pixels of a point of the fake's display, which an image takes when
/// it asks for none.
pub const SCALE: f32 = 2.0;

/// The most levels of a tree that a look for a key goes down, as the app's
/// host's.
const DEEPEST: usize = 64;

/// A pane of the fake host: where it is, and what its reads give.
#[derive(Clone, Debug, PartialEq)]
pub struct FakePane {
    /// Its window.
    pub window: WindowId,
    /// Its tab.
    pub tab: TabId,
    /// The logical line of each of its rows, by the row's number, as a read
    /// of a line's text gives it.
    pub lines: BTreeMap<i64, LineText>,
    /// The text of each of its rows, by the row's number, and whether the
    /// row wraps into the next, as a read of rows gives them: a row that
    /// it holds without one reads empty.
    pub rows: BTreeMap<i64, (String, bool)>,
    /// The targets of the OSC 8 links of its cells, by their columns and
    /// their rows' numbers.
    pub links: BTreeMap<(u16, i64), String>,
    /// The styles of each of its rows, by the row's number.
    pub styles: BTreeMap<i64, Vec<StyleRun>>,
    /// Its selection's text.
    pub selection: Option<String>,
    /// Its selection's cells, as a read of them gives them.
    pub selected_cells: Option<Vec<crate::image::CellRow>>,
    /// The working directory of its program in the foreground.
    pub directory: Option<String>,
    /// The numbers of the rows that it holds, of its scrollback and its
    /// screen, which a decoration's cells and a read of rows are of: a
    /// screen of 24 rows without a scrollback by default.
    pub held: std::ops::Range<i64>,
    /// Its grid, its columns and its rows, 80 by 24 by default.
    pub grid: (u16, u16),
    /// Its title, as a read of a pane's title gives it.
    pub title: Option<String>,
    /// The commands that its shell marked, oldest first, as a read of its
    /// commands gives them: a mark at a row above those held left the
    /// scrollback, as the read tells it of none; a command of no prompt
    /// starts at its first mark, one whose prompt left at the first row
    /// held, and one whose end left too meets no row.
    pub commands: Vec<host::ShellCommand>,
    /// The lines of its commands, by their ids, as a read of a command's
    /// line gives them.
    pub command_lines: BTreeMap<u64, String>,
    /// The command line that its shell edits at its prompt, as a read of
    /// it gives it, and as a run of a line needs it empty: none by
    /// default, as of a shell that marks no prompt.
    pub edited_line: Option<String>,
    /// Its view, as a read of it gives it; of none, its screen, the last
    /// of its rows held, as many as its grid's, of no scrollback above
    /// them.
    pub viewport: Option<host::Viewport>,
    /// Its terminal's cursor's cell, by its column and its row's number:
    /// the first column of its first row held by default.
    pub cursor: (u16, i64),
    /// The cell of the cursor that a plugin's write shows in place of the
    /// terminal's, if one does.
    pub held_cursor: Option<(u16, i64)>,
}

impl FakePane {
    /// The logical line of its row numbered `row`, of its [`rows`](Self::rows)
    /// that wrap into each other, as far as `reach` rows before it and after
    /// it, as the app's host reads it: each row of its grid's columns, a
    /// cell each character and a space past its text; `None` for a row that
    /// it holds no text of.
    pub fn line_of_rows(&self, row: i64, reach: usize) -> Option<LineText> {
        self.rows.get(&row)?;
        let wraps = |at: i64| self.rows.get(&at).is_some_and(|(_, wraps)| *wraps);
        let mut first = row;
        while row - first < reach as i64 && self.rows.contains_key(&(first - 1)) && wraps(first - 1)
        {
            first -= 1;
        }
        let mut last = row;
        while last - row < reach as i64 && wraps(last) && self.rows.contains_key(&(last + 1)) {
            last += 1;
        }
        let mut line = LineText {
            text: String::new(),
            cells: Vec::new(),
        };
        for at in first..=last {
            let text = self.rows.get(&at).map_or("", |(text, _)| text.as_str());
            let mut chars = text.chars();
            for column in 0..self.grid.0 {
                line.text.push(chars.next().unwrap_or(' '));
                line.cells.push((column, at));
            }
        }
        Some(line)
    }

    /// Its commands whose rows meet the rows numbered from `top` to before
    /// `bottom`, as the app's host gives them: the last whose first row is
    /// at `top` or above it, and those after it whose first rows are above
    /// `bottom`. A mark of a row above those held left the scrollback, and
    /// reads as none. A command's first row is its prompt's, or, of none,
    /// its first mark's; of a prompt that left, the first row held, but of
    /// a command whose end left too, as every row of it did, none.
    fn commands_meeting(&self, top: i64, bottom: i64) -> Vec<host::ShellCommand> {
        if top >= bottom {
            return Vec::new();
        }
        let start = self.held.start;
        let held = |mark: Option<(u16, i64)>| mark.filter(|&(_, row)| row >= start);
        let first_row = |command: &host::ShellCommand| {
            let marks = [command.prompt, command.input, command.output, command.end];
            let found = match command.prompt {
                Some(_) => held(command.prompt),
                None => marks.into_iter().find_map(held),
            };
            if let Some((_, row)) = found {
                return Some(row);
            }
            let ended = command.end.is_some_and(|(_, row)| row < start);
            (!ended && marks.iter().any(Option::is_some)).then_some(start)
        };
        let after = self
            .commands
            .partition_point(|command| first_row(command).is_none_or(|row| row <= top));
        let begin = after
            .checked_sub(1)
            .filter(|&at| first_row(&self.commands[at]).is_some())
            .unwrap_or(after);
        self.commands[begin..]
            .iter()
            .map_while(|command| {
                first_row(command).filter(|&row| row < bottom)?;
                Some(host::ShellCommand {
                    prompt: held(command.prompt),
                    input: held(command.input),
                    output: held(command.output),
                    end: held(command.end),
                    ..command.clone()
                })
            })
            .collect()
    }

    /// Its command of the id `id`, as a read of the commands gives it, its
    /// marks of rows above those held read as none.
    fn command(&self, id: u64) -> Option<host::ShellCommand> {
        let start = self.held.start;
        let held = |mark: Option<(u16, i64)>| mark.filter(|&(_, row)| row >= start);
        let command = self.commands.iter().find(|command| command.id == id)?;
        Some(host::ShellCommand {
            prompt: held(command.prompt),
            input: held(command.input),
            output: held(command.output),
            end: held(command.end),
            ..command.clone()
        })
    }

    /// Its command whose block holds the row numbered `row`, as the app's
    /// host gives it: the last whose first row is at `row` or above it,
    /// unless it ended before `row`, its end's row its own where the end is
    /// past its first column.
    fn command_at(&self, row: i64) -> Option<host::ShellCommand> {
        let command = self.commands_meeting(row, row.saturating_add(1)).pop()?;
        let within = match command.end {
            Some((column, end)) => row < end || (row == end && column > 0),
            None => true,
        };
        within.then_some(command)
    }

    /// Its view, as a read of it gives it.
    fn viewport(&self) -> host::Viewport {
        self.viewport.clone().unwrap_or_else(|| {
            let top = self.held.end - i64::from(self.grid.1);
            host::Viewport {
                top,
                rows: self.grid.1,
                screen: top,
                oldest: self.held.start,
                ..host::Viewport::default()
            }
        })
    }

    /// The cells of `count` rows from the row numbered `first`, as many as
    /// the pane holds; none of a first row that it does not hold.
    fn row_cells(&self, first: i64, count: usize) -> Option<Vec<crate::image::CellRow>> {
        if !self.held.contains(&first) {
            return None;
        }
        let last = first
            .saturating_add(i64::try_from(count).unwrap_or(i64::MAX))
            .min(self.held.end);
        Some(
            (first..last)
                .map(|row| crate::image::CellRow {
                    text: self
                        .rows
                        .get(&row)
                        .map(|(text, _)| text.trim_end_matches(' ').to_owned())
                        .unwrap_or_default(),
                    runs: self.styles.get(&row).cloned().unwrap_or_default(),
                })
                .collect(),
        )
    }

    /// The cells of the rows that its view shows, of their texts without
    /// the spaces that end them and their styles, as a read of them gives
    /// them: a fold's placeholder's of none.
    fn view_cells(&self) -> Vec<crate::image::CellRow> {
        let view = self.viewport();
        let cells = |row: i64| crate::image::CellRow {
            text: self
                .rows
                .get(&row)
                .map(|(text, _)| text.trim_end_matches(' ').to_owned())
                .unwrap_or_default(),
            runs: self.styles.get(&row).cloned().unwrap_or_default(),
        };
        let mut shown = Vec::with_capacity(usize::from(view.rows));
        let mut row = view.top;
        let mut folds = view.folds.iter().peekable();
        for _ in 0..view.rows {
            match folds.next_if(|fold| fold.first <= row) {
                Some(fold) => {
                    shown.push(crate::image::CellRow {
                        text: String::new(),
                        runs: Vec::new(),
                    });
                    row = fold.last + 1;
                }
                None => {
                    shown.push(cells(row));
                    row += 1;
                }
            }
        }
        shown
    }
}

impl Default for FakePane {
    fn default() -> Self {
        Self {
            held: 0..24,
            grid: (80, 24),
            window: WindowId(1),
            tab: TabId(1),
            lines: BTreeMap::new(),
            rows: BTreeMap::new(),
            links: BTreeMap::new(),
            styles: BTreeMap::new(),
            selection: None,
            directory: None,
            title: None,
            commands: Vec::new(),
            command_lines: BTreeMap::new(),
            edited_line: None,
            viewport: None,
            cursor: (0, 0),
            held_cursor: None,
            selected_cells: None,
        }
    }
}

/// The matches of `query` in the rows that `pane` holds, as the fake host
/// finds them: literal text alone, in each row apart, of a cell a
/// character, the leftmost of each row first, none overlapping another;
/// the app's host searches regexes and logical lines too. A query of no
/// text fails, as the app's host's does.
fn found_in(pane: &FakePane, query: &host::SearchQuery) -> Result<Vec<host::SearchMatch>, Error> {
    if query.text.is_empty() {
        return Err(Error::new(
            Code::Failed,
            "a search of no text finds nothing",
        ));
    }
    let insensitive = match query.case {
        host::SearchCase::Sensitive => false,
        host::SearchCase::Insensitive => true,
        host::SearchCase::Smart => !query.text.chars().any(char::is_uppercase),
    };
    let fold = |text: &str| -> Vec<char> {
        if insensitive {
            text.chars().flat_map(char::to_lowercase).collect()
        } else {
            text.chars().collect()
        }
    };
    let wanted = fold(&query.text);
    let mut matches = Vec::new();
    for row in pane.held.clone() {
        let text = pane.rows.get(&row).map_or("", |(text, _)| text.as_str());
        let chars = fold(text);
        let mut at = 0;
        while at + wanted.len() <= chars.len() {
            if chars[at..at + wanted.len()] == wanted[..] {
                let column = |at: usize| u16::try_from(at).unwrap_or(u16::MAX);
                matches.push(host::SearchMatch {
                    first: (column(at), row),
                    last: (column(at + wanted.len() - 1), row),
                });
                at += wanted.len();
            } else {
                at += 1;
            }
        }
    }
    Ok(matches)
}

/// A decoration that the plugin laid: its pane, its cells and its look.
#[derive(Clone, Debug, PartialEq)]
pub struct FakeDecoration {
    /// Its id.
    pub id: u64,
    /// Its pane.
    pub pane: PaneId,
    /// Its first cell, by its column and its row's number.
    pub from: (u16, i64),
    /// Its last cell.
    pub to: (u16, i64),
    /// Its look.
    pub look: DecorationLook,
}

/// A popup that the plugin opened: its pane, its tab, its command, its
/// folder and its shares of the tab's room.
#[derive(Clone, Debug, PartialEq)]
pub struct FakePopup {
    /// Its pane.
    pub pane: PaneId,
    /// Its tab.
    pub tab: TabId,
    /// The command that it runs.
    pub command: String,
    /// The folder that it runs in, if the plugin gave one.
    pub folder: Option<String>,
    /// Its share of the tab's room across.
    pub width: f64,
    /// Its share of the tab's room down.
    pub height: f64,
}

/// A popup that the fake host keeps: what the plugin opened, the pane that
/// had its tab's keyboard before it, and its command's status, once the
/// test ended it.
#[derive(Clone, Debug, PartialEq)]
struct KeptPopup {
    popup: FakePopup,
    back: PaneId,
    status: Option<i32>,
}

/// A mark that the plugin laid in a pane's gutter: its pane, its row and
/// its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FakeGutterMark {
    /// Its id.
    pub id: u64,
    /// Its pane.
    pub pane: PaneId,
    /// Its row's number.
    pub row: i64,
    /// Its color, red, green and blue.
    pub color: (u8, u8, u8),
}

/// A window of the fake host: its tabs, in their order, and the one that
/// it shows.
#[derive(Clone, Debug, PartialEq)]
struct FakeWindow {
    tabs: Vec<TabId>,
    active: TabId,
}

/// A tab of the fake host: its window, its layout, the pane that has its
/// keyboard, the one that takes its whole room, if one does, and the title
/// and the color that the plugin gave it.
#[derive(Clone, Debug, PartialEq)]
struct FakeTab {
    window: WindowId,
    layout: Layout,
    active: PaneId,
    zoomed: Option<PaneId>,
    title: Option<String>,
    color: Option<Color>,
    marks: TabMarks,
    group: Option<GroupId>,
}

/// A group of a window's tabs that the fake host keeps, as the app does.
#[derive(Clone, Debug)]
struct FakeGroup {
    window: WindowId,
    name: String,
    color: Option<Color>,
    folded: bool,
}

/// A session of windows that the fake host keeps, as the app does.
#[derive(Clone, Debug)]
struct FakeSession {
    id: SessionId,
    name: String,
    /// The folder that it was opened in, `~` expanded.
    folder: Option<String>,
    /// The count of the sessions' uses as it last became active.
    used: u64,
}

/// The session that the fake host starts in, as the app starts in one of
/// no name, of an id that no other of the host's takes.
const FIRST_SESSION: SessionId = SessionId(0);

/// What a tab bar shows of a tab, whose change `tab-changed` tells: its
/// title, its color and its marks.
type Look = (String, Option<Color>, TabMarks);

/// A tab closed that the fake host keeps to give back: its id, the tab,
/// its panes, its window and its place there.
type ClosedTab = (TabId, FakeTab, Vec<(PaneId, FakePane)>, WindowId, usize);

/// What a tab bar marks a tab of, as a read of the tabs tells it, which a
/// test gives a tab of the fake host.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabMarks {
    /// Output came while it did not show.
    pub activity: bool,
    /// Its bell rang while it did not show.
    pub bell: bool,
    /// The progress that a program of its panes tells.
    pub progress: Option<host::TabProgress>,
    /// Its active pane's program, other than a shell, prints of its own.
    pub printing: bool,
    /// A program other than a shell runs in its foreground.
    pub running: bool,
    /// Its active pane's program shows the alternate screen.
    pub alternate: bool,
    /// The keyboard's protected input is on for it.
    pub secure: bool,
    /// The state of an AI agent that runs in it.
    pub agent: Option<host::AgentState>,
    /// The message by which an agent of it that waits asked for the user.
    pub agent_message: Option<String>,
    /// The status that a program of its panes tells of its session.
    pub status: Option<host::TabStatus>,
    /// Whether it is pinned, at its window's start.
    pub pinned: bool,
}

/// What a tab's leaving did to its window, which is told after the tab's
/// own change.
#[derive(Clone, Copy)]
enum Left {
    /// Nothing that the window shows.
    Kept,
    /// The window shows another tab.
    Shows(WindowId, TabId),
    /// The window closed, as it has no tab left.
    Closed(WindowId),
}

/// The ids that a write's request gave, of what it opens as it applies.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Opened {
    Nothing,
    Tab(TabId, PaneId),
    Pane(PaneId),
    /// A group of tabs.
    Group(GroupId),
    /// A session.
    Session(SessionId),
    /// A quick window, of its tab and its pane.
    Quick(WindowId, TabId, PaneId),
}

/// A timer of the plugin's that the fake host keeps, of its clock.
#[derive(Clone, Debug)]
struct FakeTimer {
    /// When it fires next, in milliseconds of the host's clock; none once
    /// it fired, of one that does not repeat, whose event waits.
    at: Option<u64>,
    /// How long it waits again after it fires, of one that repeats.
    every: Option<u64>,
    /// Whether it fired since the plugin last heard of it.
    due: bool,
}

/// A search of the plugin's that the fake host keeps.
#[derive(Clone, Debug)]
struct FakeSearch {
    /// Its pane.
    pane: PaneId,
    /// Its matches, in the rows' order.
    matches: Vec<host::SearchMatch>,
    /// Its current match, as the plugin showed it.
    current: Option<host::SearchMatch>,
}

/// The host of a plugin's unit tests.
#[derive(Debug)]
pub struct FakeHost {
    caller: String,
    /// The capabilities that the plugin holds; `None` for every one, as a
    /// first-party plugin's.
    granted: Option<Vec<Capability>>,
    /// The commands that its manifest says it runs, of a plugin of the
    /// capabilities that its manifest declares.
    uses: Option<Uses>,
    /// The slots that its manifest fills; `None` for every one.
    slots: Option<Vec<Slot>>,
    /// The ids of the settings' window's pages that its manifest has; `None`
    /// for every one.
    pages: Option<Vec<String>>,
    /// Whether its plugin is another's than Lokyr's, whose fields that
    /// record a shortcut are plain ones.
    third_party: bool,
    /// The commands of the app's and of the other plugins', which its own
    /// join.
    others: Vec<Command>,
    /// Whether it was activated, as the config's changes reach an active
    /// plugin alone.
    activated: bool,
    panes: BTreeMap<PaneId, FakePane>,
    windows: BTreeMap<WindowId, FakeWindow>,
    tabs: BTreeMap<TabId, FakeTab>,
    /// The windows' numbers and the keys that select them, as the test
    /// gave them.
    numbers: BTreeMap<WindowId, (Option<u32>, Option<String>)>,
    /// The windows' titles, as the app gives them to the system's
    /// titlebar.
    titles: BTreeMap<WindowId, String>,
    /// The quick windows that the plugin opened, each whether it is hidden.
    quick: BTreeMap<WindowId, bool>,
    /// The sessions, in the order they were made.
    sessions: Vec<FakeSession>,
    /// The active session, whose windows show.
    active_session: SessionId,
    /// The count of the sessions' uses, the active one's.
    session_uses: u64,
    /// The session of each window but the quick ones.
    window_sessions: BTreeMap<WindowId, SessionId>,
    /// Where each window's typed input goes, of those that broadcast it.
    broadcasts: BTreeMap<WindowId, crate::host::BroadcastTo>,
    /// The floating panes of each tab that has some, as the test gave them.
    floating: BTreeMap<TabId, crate::host::FloatingPanes>,
    /// The popups that the plugin opened, which close as the test ends
    /// them or a close of their pane.
    popups: BTreeMap<PaneId, KeptPopup>,
    /// The panes left out of their windows' broadcasts.
    broadcast_left: std::collections::BTreeSet<PaneId>,
    /// The folders that the sessions' switcher lists, as the test gave
    /// them.
    session_folders: SessionFolders,
    /// The keys of commands of their arguments, as the test gave them.
    keys: Vec<(String, Args, String)>,
    /// The registry's commands of the app's and of other plugins', before
    /// the plugin's own, as the test gave them.
    registry: Vec<host::CommandInfo>,
    /// The themes that the app can show, as the test gave them.
    themes: host::Themes,
    /// The tokens of the themes, as the test gave them, the theme in
    /// force's first.
    theme_tokens: Vec<host::ThemeTokens>,
    /// The id of the plugin's question that waits for its answer, which
    /// the test posts, before which it asks no other.
    question: Option<u64>,
    /// The system's monospaced families that it gives.
    fonts: host::Fonts,
    /// The system's day, whether a notice of the app's shows, and whether
    /// the plugin's state lasts beyond the app's run.
    day: u64,
    notice: bool,
    keeps_state: bool,
    /// The system's time and the local time zone's offset that a read of
    /// the time gives.
    local_time: host::LocalTime,
    /// The folder of the host's temporary files, of none as without a
    /// state's folder, and the temporary files that the plugin wrote, their
    /// bounds' tally and their bytes by their names, kept in memory, and
    /// the clock's time of the last, from which their budget refills.
    temporary_folder: Option<std::path::PathBuf>,
    temporary: host::TemporaryFiles,
    temporary_bytes: BTreeMap<String, Vec<u8>>,
    temporary_written: Option<u64>,
    /// The git branches of the repositories' folders, as the test gave
    /// them, which a folder within one takes.
    git_branches: BTreeMap<String, String>,
    /// The AI agents that a read of the agents gives, as the test gave them.
    agents: Vec<host::AgentInfo>,
    /// The plugins that a read of the plugins gives, as the test gave them,
    /// whose states the plugin's turning them off and on changes.
    plugins: Vec<host::PluginInfo>,
    /// The settings that a read of them gives, as the test gave them.
    settings: host::Settings,
    /// The page of the plugin's that the settings' window shows, as the
    /// plugin's or the test's showing of it opened the tab.
    page: Option<String>,
    /// Where the plugin's moves of tabs to windows of their own put those
    /// windows on the screens.
    places: BTreeMap<WindowId, ScreenPoint>,
    /// The texts of the plugin's messages, of their arguments, as the
    /// host's catalogs would give them.
    messages: Vec<(String, BTreeMap<String, Data>, String)>,
    /// The place of the plugin's tab bar.
    bar_place: BarPlace,
    /// The side of the status bar of the plugin's view: its manifest's,
    /// then the one that it asked for.
    status_bar_side: host::StatusBarSide,
    /// The places of the plugin's tab bar of windows of their own.
    window_bar_places: BTreeMap<WindowId, BarPlace>,
    /// The fade of the panes that have not the keyboard, and whether the
    /// keyboard follows the pointer, as the plugin asked.
    inactive_dim: f32,
    focus_follows_mouse: bool,
    /// The looks of the commands' blocks that the plugin asked for.
    block_looks: host::BlockLooks,
    /// The blocks that the plugin selected, by their panes, of their
    /// commands' ids.
    selected_blocks: BTreeMap<PaneId, u64>,
    /// The window that has the focus.
    focused: Option<WindowId>,
    /// The focus that the test gave last, which the next look at the
    /// panes takes, once the test gave them their tabs.
    given_focus: Option<Option<PaneId>>,
    clipboard: Option<String>,
    /// The PNG on the clipboard, as the plugin put it there.
    clipboard_image: Option<Vec<u8>>,
    /// The saves that the plugin asked, whose panels show, by their ids:
    /// each its window, its file's name and its bytes.
    saves: BTreeMap<u64, (WindowId, String, Vec<u8>)>,
    /// The jobs of the packages' service that the plugin started, which
    /// run, by their ids.
    jobs: BTreeMap<u64, host::Packages>,
    /// The files that its saves wrote, by their paths.
    saved: BTreeMap<String, Vec<u8>>,
    /// The ids of the images that the host keeps of the plugin's, the
    /// oldest first.
    kept: Vec<u64>,
    /// What the plugin's images may still draw, and the host's time when
    /// it last asked for one.
    images: (crate::image::Budget, Option<u64>),
    /// What the plugin keeps from idle sleep.
    awake: host::Awake,
    /// The modifier whose double tap the plugin hears.
    double_tap: Option<host::Modifier>,
    /// The notices that the plugin showed, in their order, and the time of
    /// its last, of the host's clock.
    notices: Vec<host::Notice>,
    noticed_at: Option<u64>,
    /// The default bindings of the settings that the test gave, which the
    /// config's own hide as they name their keys.
    default_bindings: Vec<host::SettingBinding>,
    font_size: f32,
    missing: Vec<String>,
    config: Data,
    state: Data,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    menu_items: Vec<crate::host::MenuItem>,
    subscribed: Vec<String>,
    done: Vec<Request>,
    /// The plugin's reads since the last take, in their order.
    reads: Vec<Request>,
    decorations: Vec<FakeDecoration>,
    gutter_marks: Vec<FakeGutterMark>,
    panels: BTreeMap<PanelId, PanelOptions>,
    /// The window that each open panel shows in.
    panel_windows: BTreeMap<PanelId, WindowId>,
    views: BTreeMap<ViewId, Option<Node>>,
    /// The views whose renders the plugin's calls asked for, as a panel's
    /// opening asks for its own.
    renders: Vec<ViewId>,
    owed: Vec<Event>,
    /// What a plugin that does not read the processes learnt of each tab,
    /// as it activated, read the tabs or heard their changes, which a
    /// change that it cannot read tells it nothing of.
    told_tabs: BTreeMap<TabId, TabInfo>,
    /// When each tab last showed, of the count of uses, as the app orders a
    /// window's tabs by their use.
    tab_uses: BTreeMap<TabId, u64>,
    /// The count of the tabs' uses.
    tab_use_count: u64,
    /// Whether the order of the tabs' use holds, as Control is held.
    order_held: bool,
    /// The tabs closed that it keeps to give back, the last closed last:
    /// each tab, its panes, its window and its place there.
    closed_tabs: Vec<ClosedTab>,
    /// The groups of the windows' tabs, as the app keeps them.
    groups: BTreeMap<GroupId, FakeGroup>,
    /// Of a plugin that does not read the processes, the title that it last
    /// learnt of each pane, its program's, of none at first, which a change
    /// of the pane's folder or program, that it cannot read, leaves.
    told_panes: BTreeMap<PaneId, Option<String>>,
    /// The plugin's searches, by their ids: each's pane, its matches, and
    /// its current match, as the plugin showed it.
    searches: BTreeMap<u64, FakeSearch>,
    /// The plugin's timers, by their ids.
    timers: BTreeMap<u64, FakeTimer>,
    /// The host's time, in milliseconds since it started, which
    /// [`FakeHost::advance`] moves on.
    clock: u64,
    last_id: u64,
}

impl FakeHost {
    /// A host of the plugin `caller`, as its calls name it, of every
    /// capability, without panes, of text of 13 points, and of an empty
    /// config.
    pub fn new(caller: &str) -> Self {
        Self {
            caller: caller.to_owned(),
            granted: None,
            question: None,
            uses: None,
            slots: None,
            pages: None,
            third_party: false,
            others: Vec::new(),
            activated: false,
            panes: BTreeMap::new(),
            windows: BTreeMap::new(),
            tabs: BTreeMap::new(),
            numbers: BTreeMap::new(),
            titles: BTreeMap::new(),
            quick: BTreeMap::new(),
            sessions: vec![FakeSession {
                id: FIRST_SESSION,
                name: String::new(),
                folder: None,
                used: 0,
            }],
            // The home directory, as the app's.
            session_folders: SessionFolders {
                home: std::env::home_dir().map(|home| home.to_string_lossy().into_owned()),
                ..SessionFolders::default()
            },
            active_session: FIRST_SESSION,
            session_uses: 0,
            window_sessions: BTreeMap::new(),
            broadcasts: BTreeMap::new(),
            floating: BTreeMap::new(),
            popups: BTreeMap::new(),
            broadcast_left: std::collections::BTreeSet::new(),
            places: BTreeMap::new(),
            keys: Vec::new(),
            registry: Vec::new(),
            fonts: fonts(),
            day: DAY,
            local_time: LOCAL_TIME,
            temporary_folder: Some(std::path::PathBuf::from(TEMPORARY_FOLDER)),
            temporary: host::TemporaryFiles::default(),
            temporary_bytes: BTreeMap::new(),
            temporary_written: None,
            git_branches: BTreeMap::new(),
            agents: Vec::new(),
            plugins: Vec::new(),
            settings: host::Settings::default(),
            page: None,
            notice: false,
            keeps_state: true,
            theme_tokens: theme_tokens(),
            themes: host::Themes {
                appearance: crate::theme::ThemeVariant::Dark,
                current: "Lokyr Dark".to_owned(),
                themes: vec![
                    host::ThemeInfo {
                        name: "Lokyr Light".to_owned(),
                        variant: crate::theme::ThemeVariant::Light,
                        origin: host::ThemeOrigin::BuiltIn,
                        format: host::ThemeFormat::Lokyr,
                    },
                    host::ThemeInfo {
                        name: "Lokyr Dark".to_owned(),
                        variant: crate::theme::ThemeVariant::Dark,
                        origin: host::ThemeOrigin::BuiltIn,
                        format: host::ThemeFormat::Lokyr,
                    },
                ],
                loading: false,
            },
            messages: Vec::new(),
            bar_place: BarPlace::default(),
            status_bar_side: host::StatusBarSide::Start,
            window_bar_places: BTreeMap::new(),
            inactive_dim: 0.0,
            focus_follows_mouse: false,
            block_looks: host::BlockLooks::default(),
            selected_blocks: BTreeMap::new(),
            focused: None,
            given_focus: None,
            clipboard: None,
            clipboard_image: None,
            saves: BTreeMap::new(),
            jobs: BTreeMap::new(),
            saved: BTreeMap::new(),
            kept: Vec::new(),
            images: (crate::image::Budget::default(), None),
            awake: host::Awake::Off,
            double_tap: None,
            notices: Vec::new(),
            noticed_at: None,
            default_bindings: Vec::new(),
            font_size: 13.0,
            missing: Vec::new(),
            config: Data::Map(BTreeMap::new()),
            state: Data::Nil,
            commands: Vec::new(),
            bindings: Vec::new(),
            menu_items: Vec::new(),
            subscribed: Vec::new(),
            done: Vec::new(),
            reads: Vec::new(),
            decorations: Vec::new(),
            gutter_marks: Vec::new(),
            panels: BTreeMap::new(),
            panel_windows: BTreeMap::new(),
            views: BTreeMap::new(),
            renders: Vec::new(),
            owed: Vec::new(),
            told_tabs: BTreeMap::new(),
            tab_uses: BTreeMap::new(),
            tab_use_count: 0,
            order_held: false,
            closed_tabs: Vec::new(),
            groups: BTreeMap::new(),
            told_panes: BTreeMap::new(),
            searches: BTreeMap::new(),
            timers: BTreeMap::new(),
            clock: 0,
            last_id: FIRST_ID - 1,
        }
    }

    /// The host, whose plugin holds the capabilities of `granted` alone, as
    /// another than a first-party plugin does, of a manifest that declares
    /// them, but those that the host withholds, the network's and the
    /// processes', as it refuses them: its commands are of its namespace,
    /// and it runs those of [`FakeHost::with_uses`] alone, beside its own.
    pub fn with_capabilities(mut self, granted: Vec<Capability>) -> Self {
        self.granted = Some(
            granted
                .into_iter()
                .filter(|capability| !capability.is_withheld())
                .collect(),
        );
        self.uses.get_or_insert_with(Uses::default);
        self
    }

    /// The host, whose plugin holds the capabilities that `manifest`, its
    /// `plugin.toml`, declares in `[capabilities]`, as
    /// [`FakeHost::with_capabilities`] gives them, and runs the commands of
    /// its `[uses]`, as the app's host gives a plugin of that manifest
    /// that is not a first-party one, so that a plugin's tests find a
    /// request that its manifest does not cover. It reads the forms that
    /// the first-party plugins' manifests write them in, `name = true` and
    /// lists of texts of double quotes, a list over lines too, of no
    /// comment after them, and no slot; a name that is no capability
    /// panics, as a manifest that the app would refuse. A manifest of
    /// other forms, as a third party's, is read whole first, as the app
    /// reads it, and given of [`FakeHost::with_capabilities`],
    /// [`FakeHost::with_uses`] and [`FakeHost::with_slots`].
    pub fn with_manifest(self, manifest: &str) -> Self {
        let Declared {
            mut capabilities,
            uses,
            builtin,
            pages,
            status_bar_side,
        } = declared(manifest);
        // Lokyr's own plugins' alone, which another's manifest has refused.
        if !builtin {
            capabilities.retain(|capability| !capability.is_first_party_only());
        }
        let mut host = self.with_capabilities(capabilities).with_pages(pages);
        host.third_party = !builtin;
        host.status_bar_side = status_bar_side;
        match uses {
            Some(patterns) => host.with_uses(patterns),
            None => host,
        }
    }

    /// The host, whose plugin is another's than Lokyr's, as an installed
    /// one: a field of its views that records a shortcut is a plain one, as
    /// the app's host takes it.
    #[must_use]
    pub fn third_party(mut self) -> Self {
        self.third_party = true;
        self
    }

    /// The host, whose plugin's manifest says that it runs the commands of
    /// `patterns`, names and namespaces, as `tabs.*`, which a plugin of
    /// [`FakeHost::with_capabilities`] checks.
    pub fn with_uses(mut self, patterns: Vec<String>) -> Self {
        self.uses = Some(Uses::new(patterns));
        self
    }

    /// The host, whose plugin's manifest fills the slots of `slots` alone,
    /// which its views are of, beside its panels.
    pub fn with_slots(mut self, slots: Vec<Slot>) -> Self {
        self.slots = Some(slots);
        self
    }

    /// The host, whose plugin's manifest has the settings' window's pages of
    /// the ids `pages` alone, which its page views are of.
    pub fn with_pages(mut self, pages: Vec<String>) -> Self {
        self.pages = Some(pages);
        self
    }

    /// The host, of the commands of `commands` beside the plugin's, as the
    /// app's and the other plugins' registered them, which the plugin's
    /// calls run.
    pub fn with_commands(mut self, commands: Vec<Command>) -> Self {
        self.others = commands;
        self
    }

    /// The pane `id`, which the host holds from now, of the window 1 and
    /// the tab 1 if it did not hold it. A pane of a tab of its own, or of a
    /// window of its own, opens them, the tab after the window's others and
    /// the pane after the tab's others, side by side.
    pub fn pane(&mut self, id: PaneId) -> &mut FakePane {
        self.panes.entry(id).or_default()
    }

    /// Gives the plugin's message `id` of `args` its `text`, as the host's
    /// catalogs would in the language shown, which a read of the message
    /// tells; none takes it away, as of a message that they lack.
    pub fn set_message(
        &mut self,
        id: &str,
        args: &[(&str, Data)],
        text: Option<&str>,
    ) -> &mut Self {
        let args: BTreeMap<String, Data> = args
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect();
        self.messages
            .retain(|(known, known_args, _)| !(known == id && *known_args == args));
        if let Some(text) = text {
            self.messages.push((id.to_owned(), args, text.to_owned()));
        }
        self
    }

    /// The pane that has the keyboard of the focused window's shown tab, as
    /// the plugin's writes left it.
    pub fn keyboard(&mut self) -> Option<PaneId> {
        self.settle();
        self.focused_pane()
    }

    /// The focused window and the tab that it shows, as the plugin's writes
    /// left them.
    pub fn focused_tab(&mut self) -> Option<(WindowId, TabId)> {
        self.settle();
        let pane = self.focused_pane()?;
        let held = self.panes.get(&pane)?;
        Some((held.window, held.tab))
    }

    /// Opens a tab of a pane of its own after `window`'s others, as the
    /// user opens one, of new ids of the host's: the tab and its pane,
    /// whose events the plugin hears at the next delivery.
    pub fn add_tab(&mut self, window: WindowId) -> (TabId, PaneId) {
        self.settle();
        let (tab, pane) = (TabId(self.next_id()), PaneId(self.next_id()));
        self.open_tab(window, usize::MAX, tab, pane);
        (tab, pane)
    }

    /// Makes a group of `tabs`, of one window and none pinned, of `name`
    /// and `color`, as the app makes one, of a new id of the host's, whose
    /// events the plugin hears at the next delivery: its id.
    pub fn add_group(&mut self, tabs: &[TabId], name: &str, color: Option<Color>) -> GroupId {
        self.settle();
        let id = GroupId(self.next_id());
        self.new_group(id, tabs, name, color);
        id
    }

    /// The groups of `window`'s tabs, as the plugin's writes left them.
    pub fn groups(&mut self, window: WindowId) -> Vec<GroupInfo> {
        self.settle();
        self.group_infos(window)
    }

    /// Holds the order of the tabs' use, as the app does while Control is
    /// held, or lets it go, as Control is released: the tab that each window
    /// shows then is its tab used last.
    pub fn hold_tab_order(&mut self, held: bool) -> &mut Self {
        self.settle();
        if std::mem::replace(&mut self.order_held, held) && !held {
            let shown: Vec<TabId> = self.windows.values().map(|window| window.active).collect();
            for tab in shown {
                self.shown(tab);
            }
        }
        self
    }

    /// The layout of `tab`, as the plugin's writes left it.
    pub fn tab_layout(&mut self, tab: TabId) -> Option<Layout> {
        self.settle();
        self.tabs.get(&tab).map(|shown| shown.layout.clone())
    }

    /// Gives `tab` the `marks` that a read of the tabs tells, as the host
    /// finds them, which `tab-changed` tells as they change.
    pub fn set_tab_marks(&mut self, tab: TabId, marks: TabMarks) -> &mut Self {
        let before = self.look(tab);
        if let Some(found) = self.tabs.get_mut(&tab) {
            found.marks = marks;
        }
        self.looked(tab, &before);
        self
    }

    /// Gives the app the tokens of themes, `tokens`, which a read of a
    /// theme's tokens gives, of its name, the first of none, as the theme
    /// in force's; the plugin hears `theme-changed` as the first changes.
    pub fn set_theme_tokens(&mut self, tokens: Vec<host::ThemeTokens>) -> &mut Self {
        if self.activated && tokens.first() != self.theme_tokens.first() {
            self.owed.push(Event::ThemeChanged);
        }
        self.theme_tokens = tokens;
        self
    }

    /// Gives the app the themes that it can show, `themes`, which a read of
    /// them gives, the plugin hearing `themes-changed` as they change; the
    /// built-in ones, dark in force, at first.
    pub fn set_themes(&mut self, themes: host::Themes) -> &mut Self {
        if self.activated && themes != self.themes {
            self.owed.push(Event::ThemesChanged);
        }
        self.themes = themes;
        self
    }

    /// Gives the system's monospaced families `fonts`, which a read of the
    /// fonts gives and a preview and a choice of a family check, in place
    /// of [`fonts`]'.
    pub fn set_fonts(&mut self, fonts: host::Fonts) -> &mut Self {
        self.fonts = fonts;
        self
    }

    /// Gives the system's day, in days since 1970-01-01, which a read of
    /// the day gives, in place of [`DAY`].
    pub fn set_day(&mut self, day: u64) -> &mut Self {
        self.day = day;
        self
    }

    /// Gives the system's time and the local time zone's offset, which a
    /// read of the time gives, in place of [`LOCAL_TIME`].
    pub fn set_local_time(&mut self, now: host::LocalTime) -> &mut Self {
        self.local_time = now;
        self
    }

    /// Gives the AI agents that a read of the agents gives.
    pub fn set_agents(&mut self, agents: Vec<host::AgentInfo>) -> &mut Self {
        self.agents = agents;
        self
    }

    /// Gives the plugins that a read of the plugins gives: a plugin that
    /// subscribes to `plugins-changed` hears that they changed, once it
    /// activated.
    pub fn set_plugins(&mut self, plugins: Vec<host::PluginInfo>) -> &mut Self {
        if self.activated && plugins != self.plugins {
            self.owed.push(Event::PluginsChanged);
        }
        self.plugins = plugins;
        self
    }

    /// Tells the plugin, once it activated, that the app took the installed
    /// plugins anew, as their lockfile changed, whatever their list holds:
    /// `plugins-changed`, as of a package that the app refused, which
    /// changes nothing of the list.
    pub fn take_installed_anew(&mut self) -> &mut Self {
        if self.activated {
            self.owed.push(Event::PluginsChanged);
        }
        self
    }

    /// Gives the settings that a read of them gives: a plugin that
    /// subscribes to `settings-changed` hears that they changed, once it
    /// activated.
    pub fn set_settings(&mut self, settings: host::Settings) -> &mut Self {
        if self.activated && settings != self.settings {
            self.owed.push(Event::SettingsChanged);
        }
        self.default_bindings = settings
            .keys
            .iter()
            .filter(|binding| binding.default)
            .cloned()
            .collect();
        self.settings = settings;
        self
    }

    /// The settings, as a read of them gives them, of the writes that the
    /// plugin made.
    pub fn settings(&self) -> &host::Settings {
        &self.settings
    }

    /// The config's binding of `keys` of `value`, as a read of the settings
    /// tells it: a command's, of its title and its category, a table's of
    /// its arguments and its context too, or `false`'s, of the title and the
    /// category of the default of the same keys.
    fn configured(&self, keys: &str, value: &Data) -> host::SettingBinding {
        let field = |name: &str| match value {
            Data::Map(map) => map.get(name),
            _ => None,
        };
        let command = match (value, field("command")) {
            (Data::Text(command), _) | (_, Some(Data::Text(command))) => Some(command.clone()),
            _ => None,
        };
        let args: crate::command::Args = match field("args") {
            Some(Data::Map(args)) => args
                .iter()
                .filter_map(|(name, value)| {
                    let value = match value {
                        Data::Text(text) => crate::command::Value::String(text.clone()),
                        Data::Integer(number) => crate::command::Value::Integer(*number),
                        Data::Number(number) => crate::command::Value::Number(*number),
                        Data::Boolean(on) => crate::command::Value::Boolean(*on),
                        _ => return None,
                    };
                    Some((name.clone(), value))
                })
                .collect(),
            _ => crate::command::Args::new(),
        };
        let when = match field("when") {
            Some(Data::Text(when)) => Some(when.clone()),
            _ => None,
        };
        let (title, category) = match &command {
            Some(command) => self
                .registered(command)
                .map(|command| (command.title.clone(), command.category))
                .unzip(),
            None => self
                .default_bindings
                .iter()
                .find(|binding| binding.keys == keys)
                .map_or((None, None), |binding| {
                    (binding.title.clone(), binding.category)
                }),
        };
        host::SettingBinding {
            keys: keys.to_owned(),
            detailed: !args.is_empty() || when.is_some(),
            command,
            title,
            category,
            args,
            when,
            default: false,
            system: false,
        }
    }

    /// The plugins, as a read of them gives them, of the states that the
    /// plugin's turning them off and on left.
    pub fn plugins(&self) -> &[host::PluginInfo] {
        &self.plugins
    }

    /// Opens the settings' window at the plugin's page `page`, or another of
    /// its pages there, as the user does, whose view renders from now: the
    /// plugin hears the page shown, and the one shown before hidden.
    pub fn show_page(&mut self, page: &str) -> &mut Self {
        self.open_page(page.to_owned());
        self
    }

    /// Hides the plugin's page that shows, as the settings' window closes
    /// or shows another plugin's page: the plugin hears it hidden.
    pub fn hide_page(&mut self) -> &mut Self {
        if let Some(before) = self.page.take() {
            self.owed.push(Event::PageHidden(before));
        }
        self
    }

    /// The plugin's page that the settings' window shows, if it is open at one
    /// of the plugin's.
    pub fn page(&self) -> Option<&str> {
        self.page.as_deref()
    }

    /// Whether the plugin's manifest has the page `page`.
    fn has_page(&self, page: &str) -> bool {
        self.pages
            .as_ref()
            .is_none_or(|pages| pages.iter().any(|own| own == page))
    }

    /// Shows the page `page`, the one before hidden, as the app shows it.
    fn open_page(&mut self, page: String) {
        if self.page.as_ref() == Some(&page) {
            return;
        }
        if let Some(before) = self.page.take() {
            self.owed.push(Event::PageHidden(before));
        }
        self.owed.push(Event::PageShown(page.clone()));
        self.page = Some(page);
    }

    /// Shows a notice of the app's, or none, which a read tells.
    pub fn set_notice_shows(&mut self, shows: bool) -> &mut Self {
        self.notice = shows;
        self
    }

    /// Keeps the host's temporary files in `folder`, [`TEMPORARY_FOLDER`]
    /// by default, which their paths are of, or none, as the app without a
    /// state's folder, when a temporary file fails; the fake keeps their
    /// bytes in memory, which [`FakeHost::temporary_file`] gives.
    pub fn set_temporary_folder(&mut self, folder: Option<std::path::PathBuf>) -> &mut Self {
        self.temporary_folder = folder;
        self
    }

    /// The bytes of the plugin's temporary file `name`, as it wrote them
    /// last; none of a file that it did not write.
    pub fn temporary_file(&self, name: &str) -> Option<&[u8]> {
        self.temporary_bytes.get(name).map(Vec::as_slice)
    }

    /// Gives the folders that the sessions' switcher lists, which a read of
    /// them tells.
    pub fn set_session_folders(&mut self, folders: SessionFolders) -> &mut Self {
        self.session_folders = folders;
        self
    }

    /// Gives `tab` its floating panes, which a read of them tells, as the
    /// app's floating panes would float over its split panes.
    pub fn set_floating_panes(
        &mut self,
        tab: TabId,
        floating: crate::host::FloatingPanes,
    ) -> &mut Self {
        self.floating.insert(tab, floating);
        self
    }

    /// The popups that the plugin opened and that are open, in the order of
    /// their panes' ids.
    pub fn popups(&self) -> Vec<FakePopup> {
        self.popups
            .values()
            .map(|kept| kept.popup.clone())
            .collect()
    }

    /// Ends the command of the popup `pane` of `status`, of 128 and a
    /// signal's number for a signal, or none, and closes the popup, as the
    /// app does as its command exits with 0, or a key after it failed: its
    /// plugin hears that it closed, of the status, owed.
    pub fn close_popup(&mut self, pane: PaneId, status: Option<i32>) -> &mut Self {
        if let Some(kept) = self.popups.get_mut(&pane) {
            kept.status = status;
            self.close_pane(pane);
        }
        self
    }

    /// Sends `window`'s typed input to `to`, as the user's menu does, which
    /// a read of the window's broadcast and of its tabs tells, the change
    /// owed.
    pub fn set_broadcast(&mut self, window: WindowId, to: crate::host::BroadcastTo) -> &mut Self {
        if to == crate::host::BroadcastTo::Off {
            self.broadcasts.remove(&window);
        } else {
            self.broadcasts.insert(window, to);
        }
        self.changed(WorkspaceEvent::BroadcastChanged { window });
        self
    }

    /// Makes the folder of the absolute path `folder` a git repository's
    /// of the branch `branch`, or no repository's of none, which a read of
    /// the branch of a folder within it gives.
    pub fn set_git_branch(&mut self, folder: &str, branch: Option<&str>) -> &mut Self {
        match branch {
            Some(branch) => {
                self.git_branches
                    .insert(folder.to_owned(), branch.to_owned());
            }
            None => {
                self.git_branches.remove(folder);
            }
        }
        self
    }

    /// Keeps the plugin's state between the app's runs, as by default, or
    /// not, as the app without a state's folder, which a read tells; it
    /// holds the state in memory all the same.
    pub fn set_keeps_state(&mut self, keeps: bool) -> &mut Self {
        self.keeps_state = keeps;
        self
    }

    /// Gives the registry `commands`, of the app's and of other plugins',
    /// which a read of the commands gives before the plugin's own, and
    /// which the plugin's calls run, of no capability.
    pub fn set_registry(&mut self, commands: Vec<host::CommandInfo>) -> &mut Self {
        self.others.extend(commands.iter().map(|info| Command {
            name: info.name.clone(),
            title: info.title.clone(),
            category: info.category,
            capabilities: Vec::new(),
            params: info.params.clone(),
            when: info.when.clone(),
        }));
        self.registry = commands;
        self
    }

    /// Gives `command` of `args` the `keys` that run it, as the platform
    /// writes them, which a read of its keys tells; none takes them away.
    pub fn set_keys(&mut self, command: &str, args: Args, keys: Option<&str>) -> &mut Self {
        self.keys
            .retain(|(bound, bound_args, _)| !(bound == command && *bound_args == args));
        if let Some(keys) = keys {
            self.keys.push((command.to_owned(), args, keys.to_owned()));
        }
        self
    }

    /// Gives `window` its `number` and the `keys` that select it, as the
    /// platform writes them, which a read of the windows tells.
    pub fn number_window(
        &mut self,
        window: WindowId,
        number: Option<u32>,
        keys: Option<&str>,
    ) -> &mut Self {
        self.numbers
            .insert(window, (number, keys.map(str::to_owned)));
        self
    }

    /// Gives `window` the title of its titlebar, as the app gives the
    /// system's, which a read of the windows tells.
    pub fn title_window(&mut self, window: WindowId, title: &str) -> &mut Self {
        self.titles.insert(window, title.to_owned());
        self
    }

    /// Gives `pane` the focus, its window's, its tab's in that window and
    /// its own in that tab, as the app's focused pane has it, or takes it
    /// away with `None`.
    pub fn focus(&mut self, pane: Option<PaneId>) -> &mut Self {
        if let Some(pane) = pane {
            self.panes.entry(pane).or_default();
        }
        self.given_focus = Some(pane);
        self
    }

    /// Closes `window`, with its tabs and its panes, as its last program
    /// ended or the user closed it, which the plugin is owed.
    pub fn close_window(&mut self, window: WindowId) -> &mut Self {
        // Its tabs go first, each of its panes' closings, its own and its
        // group's change, then the window, as the app's, then its session,
        // of no window left.
        self.settle();
        let session = self.window_sessions.get(&window).copied();
        let tabs = self
            .windows
            .get(&window)
            .map(|shown| shown.tabs.clone())
            .unwrap_or_default();
        for tab in tabs {
            self.drop_tab(tab);
        }
        self.panes.retain(|_, pane| pane.window != window);
        self.quick.remove(&window);
        if self.focused == Some(window) {
            self.given_focus = Some(None);
        }
        self.settle();
        self.changed(WorkspaceEvent::WindowClosed { window });
        self.window_sessions.remove(&window);
        self.session_lost_window(session);
        self
    }

    /// Puts `text` on the clipboard, or empties it.
    pub fn set_clipboard(&mut self, text: Option<&str>) -> &mut Self {
        self.clipboard = text.map(str::to_owned);
        self
    }

    /// Where a move of a tab to a window of its own put `window` on the
    /// screens, the window of a tab that was alone too.
    pub fn place(&self, window: WindowId) -> Option<ScreenPoint> {
        self.places.get(&window).copied()
    }

    /// The side of the status bar where the plugin's view goes: its
    /// manifest's, or the one that it asked for last.
    pub fn status_bar_side(&self) -> host::StatusBarSide {
        self.status_bar_side
    }

    /// The place of the plugin's tab bar in `window`: the window's own, or
    /// the bar's.
    pub fn bar_place(&self, window: WindowId) -> BarPlace {
        self.window_bar_places
            .get(&window)
            .copied()
            .unwrap_or(self.bar_place)
    }

    /// The fade of the panes that have not the keyboard that the plugin
    /// asked for, and whether the keyboard follows the pointer.
    pub fn pane_looks(&self) -> (f32, bool) {
        (self.inactive_dim, self.focus_follows_mouse)
    }

    /// The looks of the commands' blocks that the plugin asked for.
    pub fn block_looks(&self) -> host::BlockLooks {
        self.block_looks
    }

    /// The id of the command whose block the plugin selected in `pane`, if
    /// it selected one.
    pub fn selected_block(&self, pane: PaneId) -> Option<u64> {
        self.selected_blocks.get(&pane).copied()
    }

    /// Ends the block's selection of `pane`, as a key that its program
    /// takes, a text typed into it or a click in it does.
    pub fn end_block_selection(&mut self, pane: PaneId) -> &mut Self {
        self.selected_blocks.remove(&pane);
        self
    }

    /// The clipboard's text.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    /// The PNG on the clipboard, if the plugin put one there, which takes
    /// the place of its text.
    pub fn clipboard_image(&self) -> Option<&[u8]> {
        self.clipboard_image.as_deref()
    }

    /// The saves that the plugin asked whose panels show, in the order of
    /// their ids: each its id, its window and its file's name.
    pub fn saves(&self) -> Vec<(u64, WindowId, String)> {
        self.saves
            .iter()
            .map(|(&id, (window, name, _))| (id, *window, name.clone()))
            .collect()
    }

    /// Closes the panel of the save `id`, as the user chose `path`, whose
    /// file then holds its bytes, or cancelled, of none: the plugin hears
    /// of it at the next delivery. A save that shows no panel takes
    /// nothing.
    pub fn answer_save(&mut self, id: u64, path: Option<&str>) {
        let Some((_, _, bytes)) = self.saves.remove(&id) else {
            return;
        };
        if let Some(path) = path {
            self.saved.insert(path.to_owned(), bytes);
        }
        self.owed.push(Event::Saved {
            id,
            path: path.map(str::to_owned),
            error: None,
        });
    }

    /// The jobs of the packages' service that the plugin started, which
    /// run, by their ids.
    pub fn packages_jobs(&self) -> &BTreeMap<u64, host::Packages> {
        &self.jobs
    }

    /// Ends the job `id` of the packages' service, as Sleip's command
    /// would, of its lines `lines` and whether it succeeded, `ok`, which
    /// the plugin hears at the next delivery; a job that does not run is
    /// left as it is.
    pub fn answer_packages(&mut self, id: u64, lines: Vec<Data>, ok: bool) {
        if self.jobs.remove(&id).is_none() {
            return;
        }
        self.owed
            .extend(lines.into_iter().map(|line| Event::Package { id, line }));
        self.owed.push(Event::PackagesEnded { id, ok });
    }

    /// The bytes of the file that a save wrote at `path`, if one did.
    pub fn saved_file(&self, path: &str) -> Option<&[u8]> {
        self.saved.get(path).map(Vec::as_slice)
    }

    /// The ids of the images that the host keeps of the plugin's, which
    /// its views' nodes show, the oldest first.
    pub fn kept_images(&self) -> &[u64] {
        &self.kept
    }

    /// What the plugin keeps from idle sleep, as the host would hold it.
    pub fn awake(&self) -> host::Awake {
        self.awake
    }

    /// The modifier whose double tap the plugin hears, as the host would
    /// watch it.
    pub fn double_tap_watched(&self) -> Option<host::Modifier> {
        self.double_tap
    }

    /// Taps `modifier` twice, as the platform would tell it: the plugin
    /// that watches it hears it, as [`FakeHost::deliver`] gives it; one
    /// that watches another, or none, hears nothing.
    pub fn double_tap(&mut self, modifier: host::Modifier) -> &mut Self {
        if self.double_tap == Some(modifier) {
            self.owed.push(Event::DoubleTapped { modifier });
        }
        self
    }

    /// The notices that the plugin showed, in their order, the last the
    /// one that the card shows, each of its title or the plugin's
    /// namespace.
    pub fn notices(&self) -> &[host::Notice] {
        &self.notices
    }

    /// Gives the text `size`, in points, as its reads tell it.
    pub fn set_font_size(&mut self, size: f32) -> &mut Self {
        self.font_size = size;
        self
    }

    /// Has the targets of `missing` not there, which open nothing.
    pub fn set_missing(&mut self, missing: &[&str]) -> &mut Self {
        self.missing = missing.iter().map(|&target| target.to_owned()).collect();
        self
    }

    /// Gives the plugin the config `config`, a map of its keys' values, its
    /// schema's defaults among them as the app gives them, of which an
    /// active plugin hears as [`FakeHost::deliver`] goes on.
    pub fn set_config(&mut self, config: Data) -> &mut Self {
        if config != self.config {
            self.config = config;
            if self.activated {
                self.owed.push(Event::ConfigChanged);
            }
        }
        self
    }

    /// Gives the plugin the state `state`, as an earlier run kept it.
    pub fn set_state(&mut self, state: Data) -> &mut Self {
        self.state = state;
        self
    }

    /// The plugin's state, as it last kept it.
    pub fn state(&self) -> &Data {
        &self.state
    }

    /// Owes the plugin `event`, which [`FakeHost::deliver`] gives it if it
    /// subscribed to its name, or if it is one of those of the plugin
    /// alone.
    ///
    /// # Panics
    ///
    /// Of a timer's event, which the host gives the plugin as one of its
    /// timers fires, as [`FakeHost::advance`] makes it, and no post.
    pub fn post(&mut self, event: Event) -> &mut Self {
        assert!(
            !matches!(event, Event::Timer { .. }),
            "a timer's event is the host's, as a timer fires: `advance` fires them"
        );
        assert!(
            !matches!(event, Event::DoubleTapped { .. }),
            "a double tap's event is the host's, as it watches the modifier: `double_tap` gives it"
        );
        // An answer closes its question, after which the plugin may ask
        // another.
        if let Event::Answered { id, .. } | Event::Chosen { id, .. } = &event
            && self.question == Some(*id)
        {
            self.question = None;
        }
        self.owed.push(event);
        self
    }

    /// The id of the plugin's question of a text or of buttons that waits
    /// for its answer, which the test posts, as [`Event::Answered`] or
    /// [`Event::Chosen`] of its id.
    pub fn question(&self) -> Option<u64> {
        self.question
    }

    /// Whether the plugin holds `config`, as Lokyr's own plugins do.
    fn holds_config(&self) -> bool {
        self.granted
            .as_ref()
            .is_none_or(|granted| granted.contains(&Capability::Config))
    }

    /// The commands that the plugin registered.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// The bindings that it made.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The menus' items that it added, in their order.
    pub fn menu_items(&self) -> &[crate::host::MenuItem] {
        &self.menu_items
    }

    /// The names of the events that it subscribed to.
    pub fn subscriptions(&self) -> &[String] {
        &self.subscribed
    }

    /// What it did since the last take: its writes, in their order, then
    /// its requests of the app's, in theirs, as the requests themselves.
    pub fn take_done(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.done)
    }

    /// The plugin's reads since the last take that its capabilities allow,
    /// in their order, as the requests themselves: what its calls looked
    /// at.
    pub fn take_reads(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.reads)
    }

    /// Cuts short the reads of rows that the plugin asked for and has yet
    /// to hear of, as a resize or the pane's closing does in their middle:
    /// their `lines` events, at the next delivery, tell them incomplete.
    pub fn cut_reads(&mut self) {
        for event in &mut self.owed {
            if let Event::Lines { complete, .. } = event {
                *complete = false;
            }
        }
    }

    /// The ids of the plugin's searches that the host keeps, in their
    /// order, as their ends take them away.
    pub fn searches(&self) -> Vec<u64> {
        self.searches.keys().copied().collect()
    }

    /// Moves the matches of the search `search`, its current one too, as
    /// `to` gives each anew, as a resize's reflow moves their cells, and
    /// drops those that it gives none of, as the scrollback's limit takes
    /// their rows.
    pub fn move_matches(
        &mut self,
        search: u64,
        to: impl Fn(host::SearchMatch) -> Option<host::SearchMatch>,
    ) {
        if let Some(kept) = self.searches.get_mut(&search) {
            kept.matches = kept.matches.iter().filter_map(|&found| to(found)).collect();
            kept.current = kept.current.and_then(&to);
        }
    }

    /// The ids of the plugin's timers, in their order: those that wait, and
    /// those that fired once, whose events it was not given yet.
    pub fn timers(&self) -> Vec<u64> {
        self.timers.keys().copied().collect()
    }

    /// Moves the host's time on by `by`, as the app's runs, to the
    /// millisecond: each timer whose time comes fires, which owes the
    /// plugin its event once however many times it fired, as
    /// [`FakeHost::deliver`] gives it, and one that repeats waits again
    /// from its time, or from the time now if it fell behind by more than
    /// its delay, as the app's host's.
    pub fn advance(&mut self, by: std::time::Duration) -> &mut Self {
        let by = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        let now = self.clock.saturating_add(by);
        self.clock = now;
        for timer in self.timers.values_mut() {
            let Some(at) = timer.at.filter(|&at| at <= now) else {
                continue;
            };
            timer.due = true;
            timer.at = timer.every.map(|every| {
                let next = at.saturating_add(every);
                if next > now {
                    next
                } else {
                    now.saturating_add(every)
                }
            });
        }
        self
    }

    /// The decorations that it laid and did not take away, in their order.
    pub fn decorations(&self) -> &[FakeDecoration] {
        &self.decorations
    }

    /// The marks of the panes' gutters that it laid and did not take away,
    /// in their order.
    pub fn gutter_marks(&self) -> &[FakeGutterMark] {
        &self.gutter_marks
    }

    /// Its open panels, by their ids.
    pub fn panels(&self) -> &BTreeMap<PanelId, PanelOptions> {
        &self.panels
    }

    /// Closes the plugin's panel `id` as the user does, by Escape or a
    /// click outside it, as the app's host does: it forgets the panel and
    /// its tree, and owes the plugin its closing; one not open stays so.
    pub fn close_panel(&mut self, id: &PanelId) -> &mut Self {
        if self.panels.remove(id).is_some() {
            self.panel_windows.remove(id);
            let view = ViewId::Panel(id.clone());
            self.views.remove(&view);
            self.renders.retain(|asked| *asked != view);
            self.owed.push(Event::PanelClosed(id.clone()));
        }
        self
    }

    /// The last tree of `view` that it rendered, if it gave one.
    pub fn tree(&self, view: &ViewId) -> Option<&Node> {
        self.views.get(view).and_then(Option::as_ref)
    }

    /// The views whose renders its calls asked for since the last take, as
    /// `request_render` and a panel's opening ask, in their order, each
    /// once, which a test renders with [`FakeHost::render`].
    pub fn take_renders(&mut self) -> Vec<ViewId> {
        std::mem::take(&mut self.renders)
    }

    /// Activates `plugin`.
    pub fn activate(&mut self, plugin: &mut dyn Plugin) -> Result<(), Error> {
        self.activated = true;
        self.settle();
        self.learn_tabs();
        self.call(true, |cx| plugin.activate(cx))
    }

    /// Runs `plugin`'s command `name` with `args`, as they are; as the
    /// registry runs it, [`FakeHost::run_command`].
    pub fn command(
        &mut self,
        plugin: &mut dyn Plugin,
        name: &str,
        args: Args,
    ) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_command(cx, name, args))
    }

    /// Runs `plugin`'s command `name` with `args` as the app's registry
    /// runs it: of a command that the plugin registered, its arguments
    /// checked and its defaults given; an error of another, or of
    /// arguments that its registration refuses.
    pub fn run_command(
        &mut self,
        plugin: &mut dyn Plugin,
        name: &str,
        args: Args,
    ) -> Result<(), Error> {
        let args = self.checked_own(name, args)?;
        self.command(plugin, name, args)
    }

    /// Whether `plugin`'s command `name` would do something with `args`, as
    /// [`FakeHost::applies`] asks, as the app's registry asks it: never of
    /// a command that the plugin did not register, nor of arguments that
    /// its registration refuses, its defaults given.
    pub fn command_applies(&mut self, plugin: &mut dyn Plugin, name: &str, args: &Args) -> bool {
        match self.checked_own(name, args.clone()) {
            Ok(args) => self.applies(plugin, name, &args),
            Err(_) => false,
        }
    }

    /// Whether `plugin`'s command `name` would do something with `args`, as
    /// a menu asks: what the call asks is dropped, its decorations, its
    /// panels, the events that it would be owed and the ids that it took.
    /// A look that fails, as the plugin tells after it, does not apply, as
    /// a menu dims its item.
    pub fn applies(&mut self, plugin: &mut dyn Plugin, name: &str, args: &Args) -> bool {
        let mut applies = false;
        let looked = self.call(false, |cx| {
            applies = plugin.applies(cx, name, args);
            plugin.take_failure().map_or(Ok(()), Err)
        });
        applies && looked.is_ok()
    }

    /// Gives `plugin` `events`, as a batch.
    pub fn events(&mut self, plugin: &mut dyn Plugin, events: &[Event]) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_event(cx, events))
    }

    /// Gives `plugin` the events that it is owed, as a batch, if it is owed
    /// any: those of the plugin alone, as a panel's closing, a read's rows,
    /// a search's matches, a question's answer, a job's lines and end and
    /// its config's change, and
    /// those that it subscribed to, of the
    /// workspace's, its own writes' among them, and those that it emitted;
    /// the others go. Then those of its timers that fired, in their ids'
    /// order, a timer that does not repeat going with its event.
    pub fn deliver(&mut self, plugin: &mut dyn Plugin) -> Result<(), Error> {
        self.settle();
        let owed = std::mem::take(&mut self.owed);
        let mut batch = Vec::new();
        for event in owed {
            let taken = matches!(
                event,
                Event::Lines { .. }
                    | Event::Searched { .. }
                    | Event::PanelClosed(_)
                    | Event::PageShown(_)
                    | Event::PageHidden(_)
                    | Event::ConfigChanged
                    | Event::Answered { .. }
                    | Event::Chosen { .. }
                    | Event::ColorPicked { .. }
                    | Event::ColorPanelClosed
                    | Event::Saved { .. }
                    | Event::PopupClosed { .. }
                    | Event::Package { .. }
                    | Event::PackagesEnded { .. }
                    | Event::TabBarResized { .. }
                    | Event::DoubleTapped { .. }
            ) || self.subscribed.iter().any(|name| name == event.name());
            if taken && self.hears(&event) {
                batch.push(self.told(event));
            }
        }
        self.timers.retain(|&id, timer| {
            if timer.due {
                timer.due = false;
                batch.push(Event::Timer { id });
            }
            timer.at.is_some()
        });
        if batch.is_empty() {
            return Ok(());
        }
        self.events(plugin, &batch)
    }

    /// Has `plugin` render `view`, whose tree the host keeps: one that
    /// nests past [`Node::DEEPEST`] levels, or a titlebar's button that is
    /// none, it refuses, as the host does, and gives the last, as of a
    /// render that fails, as the plugin tells after it, whose writes go; a
    /// field that records a shortcut, of a [`FakeHost::third_party`]
    /// plugin, it keeps a plain one.
    pub fn render(&mut self, plugin: &mut dyn Plugin, view: &ViewId) -> Option<Node> {
        self.try_render(plugin, view)
            .unwrap_or_else(|_| self.views.get(view).cloned().flatten())
    }

    /// As [`FakeHost::render`], but a render that fails, or a tree that the
    /// host refuses, gives its error, the last tree kept.
    pub fn try_render(
        &mut self,
        plugin: &mut dyn Plugin,
        view: &ViewId,
    ) -> Result<Option<Node>, Error> {
        let mut tree = None;
        let rendered = self.call(true, |cx| {
            tree = plugin.render(cx, view);
            plugin.take_failure().map_or(Ok(()), Err)
        });
        // A tree too deep the host refuses, as it drops it without
        // recursion.
        let refused = |tree: &Node| {
            if !tree.nests_within_deepest() {
                return Err(Error::new(
                    Code::Failed,
                    format!("a plugin's view nests {} levels at most", Node::DEEPEST),
                ));
            }
            match view {
                ViewId::TitleButtons(_) => tree
                    .check_title_button()
                    .map_err(|reason| Error::new(Code::Failed, reason)),
                _ => Ok(()),
            }
        };
        let checked = match (&rendered, &tree) {
            (Err(error), _) => Err(error.clone()),
            (Ok(()), Some(tree)) => refused(tree),
            (Ok(()), None) => Ok(()),
        };
        if let Err(error) = checked {
            if let Some(tree) = tree {
                crate::view::dismantle(tree);
            }
            return Err(error);
        }
        if self.third_party
            && let Some(tree) = tree.as_mut()
        {
            tree.plain_recorders();
        }
        self.views.insert(view.clone(), tree.clone());
        Ok(tree)
    }

    /// Gives `plugin` `event`, of a node of one of its views.
    pub fn ui_event(&mut self, plugin: &mut dyn Plugin, event: &UiEvent) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_ui_event(cx, event))
    }

    /// Makes a call of the plugin's, `call`, on the host, and applies what
    /// it asked as it returns, with `apply`, its writes first, then its
    /// requests of the app's; or drops it, with what queuing it changed.
    fn call(
        &mut self,
        apply: bool,
        call: impl FnOnce(&mut Cx<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.settle();
        let caller = self.caller.clone();
        let kept = (
            (self.decorations.clone(), self.gutter_marks.clone()),
            (self.panels.clone(), self.panel_windows.clone()),
            (self.owed.len(), self.renders.len()),
            (self.last_id, self.timers.clone(), self.searches.clone()),
            self.question,
        );
        let mut calling = Calling {
            host: self,
            writes: Vec::new(),
            asked: Vec::new(),
            closed: Vec::new(),
            looking: !apply,
            searched: 0,
            imaged: 0,
            saving: Vec::new(),
            jobs: Vec::new(),
            cancelled: Vec::new(),
        };
        let result = call(&mut Cx::new(&mut calling, &caller));
        let (writes, asked, closed) = (calling.writes, calling.asked, calling.closed);
        let (saving, jobs, cancelled) = (calling.saving, calling.jobs, calling.cancelled);
        // A look, or a call that failed, leaves things as they were, as the
        // app's host leaves them: its writes and what it asked of the app
        // go, and what its requests took at once comes back; a look takes
        // nothing at all, its ids and the renders that it asked for too.
        if !apply || result.is_err() {
            let (
                (decorations, marks),
                (panels, windows),
                (owed, renders),
                (last_id, timers, searches),
                question,
            ) = kept;
            self.question = question;
            self.decorations = decorations;
            self.gutter_marks = marks;
            self.panels = panels;
            self.panel_windows = windows;
            self.timers = timers;
            // Its searches end, of no event, and those that they ended run
            // on; its reads' rows come to nothing.
            self.searches = searches;
            self.owed.truncate(owed);
            if !apply {
                self.renders.truncate(renders);
                self.last_id = last_id;
            }
            return result;
        }
        for id in closed {
            self.views.remove(&ViewId::Panel(id.clone()));
            self.owed.push(Event::PanelClosed(id));
        }
        for (request, opened) in writes {
            self.apply(request, opened);
        }
        // The settings written, as the app reads its config again once the
        // call returns: a key of no value back to its default.
        let mut settings = self.settings.clone();
        for request in &asked {
            // A binding written or taken away, after the others.
            if let Request::SetSetting {
                section,
                plugin: None,
                key,
                value,
            } = request
                && section == "keys"
            {
                // The config's own of these keys replaced, the defaults that
                // they name hidden, as the app's keymap has them.
                let mut own: Vec<host::SettingBinding> = settings
                    .keys
                    .iter()
                    .filter(|binding| !binding.default && binding.keys != *key)
                    .cloned()
                    .collect();
                if let Some(value) = value {
                    own.push(self.configured(key, value));
                }
                let mut keys: Vec<host::SettingBinding> = self
                    .default_bindings
                    .iter()
                    .filter(|binding| !own.iter().any(|mine| mine.keys == binding.keys))
                    .cloned()
                    .collect();
                keys.extend(own);
                settings.keys = keys;
                continue;
            }
            // A plugin's section turned on or off, as the app reads its
            // config again.
            if let Request::SetSetting {
                section,
                plugin: plugin @ Some(_),
                key,
                value: Some(Data::Boolean(on)),
            } = request
                && key == "enabled"
            {
                if let Some(shown) = settings
                    .sections
                    .iter_mut()
                    .find(|shown| shown.name == *section && shown.plugin == *plugin)
                {
                    shown.enabled = Some(*on);
                }
                continue;
            }
            if let Request::SetSetting {
                section,
                plugin,
                key,
                value,
            } = request
                && let Some(found) = settings
                    .sections
                    .iter_mut()
                    .find(|shown| shown.name == *section && shown.plugin == *plugin)
                    .and_then(|shown| shown.keys.iter_mut().find(|shown| shown.name == *key))
            {
                found.value = value.clone().or_else(|| found.default.clone());
            }
        }
        self.set_settings(settings);
        // The plugins turned off stop, and those turned on wait to start
        // anew, as the app reads its config again once the call returns.
        for request in &asked {
            if let Request::SetPluginEnabled { plugin, enabled } = request
                && let Some(info) = self.plugins.iter_mut().find(|info| &info.name == plugin)
            {
                let state = match (enabled, info.state) {
                    (false, _) => host::PluginState::Off,
                    (true, host::PluginState::Off) => host::PluginState::Waiting,
                    (true, state) => state,
                };
                if state != info.state {
                    info.state = state;
                    self.owed.push(Event::PluginsChanged);
                }
            }
        }
        // The settings' window shows at the page, as the app shows it once the
        // call returns.
        for request in &asked {
            if let Request::ShowPage(page) = request {
                self.open_page(page.clone());
            }
        }
        // The notices show, as the app shows them once the call returns.
        for request in &asked {
            if let Request::Notify { text, level, title } = request {
                let title = title.clone().unwrap_or_else(|| self.namespace().to_owned());
                self.notices.push(host::Notice {
                    level: *level,
                    title,
                    text: text.clone(),
                });
            }
        }
        self.done.extend(asked);
        // The saves' panels show, as the app shows them once the call
        // returns.
        for (id, window, name, bytes) in saving {
            self.saves.insert(id, (window, name, bytes));
        }
        // The jobs of the packages' service start, and those cancelled end
        // of no success, as the app's do once the call returns.
        self.jobs.extend(jobs);
        for id in cancelled {
            if self.jobs.remove(&id).is_some() {
                self.owed.push(Event::PackagesEnded { id, ok: false });
            }
        }
        result
    }

    /// Ends the search `id`, of its events that the plugin did not hear yet,
    /// as the app's host ends one.
    fn end_search(&mut self, id: u64) {
        self.searches.remove(&id);
        self.owed
            .retain(|event| !matches!(event, Event::Searched { id: owed, .. } if *owed == id));
    }

    /// A new id of a window, a tab, a pane, a decoration or a read.
    fn next_id(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }

    /// The pane `pane`; `not-found` if the host does not hold it.
    fn held(&self, pane: PaneId) -> Result<&FakePane, Error> {
        self.panes
            .get(&pane)
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// The window that has the focus, if it is open.
    fn focused_window(&self) -> Option<WindowId> {
        self.focused
            .filter(|window| self.windows.contains_key(window))
    }

    /// The focused pane: that of the keyboard of the tab that the focused
    /// window shows.
    fn focused_pane(&self) -> Option<PaneId> {
        let window = self.windows.get(&self.focused_window()?)?;
        Some(self.tabs.get(&window.active)?.active)
    }

    /// Makes the workspace of the panes as the test gave them: a tab of no
    /// pane goes, and a window of no tab; a pane's tab, or its window, that
    /// the host did not hold opens, the tab after its window's others, the
    /// pane after its tab's others, side by side; a tab takes its first
    /// pane's window, as the test moved it. Then the focus that the test
    /// gave, if it gave one since.
    fn settle(&mut self) {
        let Self {
            panes,
            windows,
            tabs,
            ..
        } = self;
        let placed: Vec<(PaneId, WindowId, TabId)> = panes
            .iter()
            .map(|(&id, pane)| (id, pane.window, pane.tab))
            .collect();
        let held: BTreeSet<TabId> = placed.iter().map(|&(_, _, tab)| tab).collect();
        tabs.retain(|tab, _| held.contains(tab));
        let mut first = BTreeSet::new();
        for &(pane, window, tab) in &placed {
            let entry = tabs.entry(tab).or_insert_with(|| FakeTab {
                window,
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
                marks: TabMarks::default(),
                group: None,
            });
            if first.insert(tab) {
                entry.window = window;
            }
            if !leaves(&entry.layout).contains(&pane) {
                append(&mut entry.layout, pane);
            }
        }
        for (&id, tab) in tabs.iter_mut() {
            let stale: Vec<PaneId> = leaves(&tab.layout)
                .into_iter()
                .filter(|leaf| panes.get(leaf).is_none_or(|pane| pane.tab != id))
                .collect();
            for leaf in stale {
                remove(&mut tab.layout, leaf);
            }
            let kept = leaves(&tab.layout);
            if !kept.contains(&tab.active) {
                tab.active = kept[0];
            }
            if tab.zoomed.is_some_and(|zoomed| !kept.contains(&zoomed)) {
                tab.zoomed = None;
            }
        }
        for (&id, window) in windows.iter_mut() {
            window
                .tabs
                .retain(|tab| tabs.get(tab).is_some_and(|tab| tab.window == id));
        }
        let mut opened = Vec::new();
        for (&id, tab) in tabs.iter() {
            if !windows.contains_key(&tab.window) {
                opened.push(tab.window);
            }
            let window = windows.entry(tab.window).or_insert_with(|| FakeWindow {
                tabs: Vec::new(),
                active: id,
            });
            if !window.tabs.contains(&id) {
                window.tabs.push(id);
            }
        }
        windows.retain(|_, window| !window.tabs.is_empty());
        for window in windows.values_mut() {
            if !window.tabs.contains(&window.active) {
                window.active = window.tabs[0];
            }
        }
        // A window that opens shows its first tab, used last, as the app's.
        let shown: Vec<TabId> = opened
            .iter()
            .filter_map(|window| windows.get(window).map(|window| window.active))
            .collect();
        for tab in shown {
            self.shown(tab);
        }
        // A window opens in the active session, as the app's; a window gone
        // leaves it.
        for window in opened {
            if self.windows.contains_key(&window) && !self.quick.contains_key(&window) {
                self.window_sessions
                    .entry(window)
                    .or_insert(self.active_session);
            }
        }
        let windows = &self.windows;
        self.window_sessions
            .retain(|window, _| windows.contains_key(window));
        match self.given_focus.take() {
            Some(Some(pane)) => {
                if let Some(&FakePane { window, tab, .. }) = self.panes.get(&pane) {
                    self.focused = Some(window);
                    if let Some(shown) = self.windows.get_mut(&window) {
                        shown.active = tab;
                    }
                    if let Some(tab) = self.tabs.get_mut(&tab) {
                        tab.active = pane;
                    }
                }
            }
            Some(None) => self.focused = None,
            None => {}
        }
    }

    /// Applies `request`, a write, which opens what `opened` gave, as the
    /// call returns: the workspace's writes as the app's workspace takes
    /// them, which owe the plugin their events.
    fn apply(&mut self, request: Request, opened: Opened) {
        match &request {
            Request::SetClipboardText(text) => {
                self.clipboard = Some(text.clone());
                self.clipboard_image = None;
            }
            Request::SetClipboardImage(png) => {
                self.clipboard_image = Some(png.0.clone());
                self.clipboard = None;
            }
            Request::DropImage(id) => self.kept.retain(|kept| kept != id),
            Request::KeepAwake(awake) => self.awake = *awake,
            Request::ShowCursor { pane, at } => {
                if let Some(pane) = self.panes.get_mut(pane) {
                    pane.held_cursor = *at;
                }
            }
            Request::Deselect(pane) => {
                if let Some(pane) = self.panes.get_mut(pane) {
                    pane.selection = None;
                    pane.selected_cells = None;
                }
            }
            Request::Undecorate { id, .. } => self.decorations.retain(|laid| laid.id != *id),
            Request::UnmarkGutter { id, .. } => self.gutter_marks.retain(|laid| laid.id != *id),
            Request::RequestRender(view) => self.ask_render(view.clone()),
            Request::SetState(state) => self.state = state.clone(),
            Request::Emit {
                name,
                schema,
                value,
            } => self.owed.push(Event::Custom {
                name: name.clone(),
                schema: schema.clone(),
                value: value.clone(),
            }),
            Request::NewTab { window, index, .. } => {
                if let Opened::Tab(tab, pane) = opened {
                    self.open_tab(*window, index.unwrap_or(usize::MAX), tab, pane);
                }
            }
            Request::Split {
                pane,
                direction,
                side,
            } => {
                if let Opened::Pane(new) = opened {
                    self.split(*pane, *direction, *side, new);
                }
            }
            Request::OpenPopup {
                window,
                command,
                folder,
                width,
                height,
            } => {
                if let Opened::Pane(new) = opened {
                    let window = window.or_else(|| self.focused_window());
                    self.open_popup(
                        window,
                        new,
                        (command.clone(), folder.clone()),
                        (
                            width.unwrap_or(crate::host::POPUP_WIDTH),
                            height.unwrap_or(crate::host::POPUP_HEIGHT),
                        ),
                    );
                }
            }
            // A close that asks closes as the user's confirmation would,
            // which a test reads among the requests done.
            Request::CloseTab { tab, keep: 0, .. } => self.close_tab(*tab),
            Request::CloseTab { tab, .. } => self.close_tab_kept(*tab),
            Request::ReopenTab(window) => self.reopen_tab(*window),
            Request::ClosePane { pane, .. } => self.close_pane(*pane),
            Request::SetTabTitle { tab: id, title } => {
                let before = self.look(*id);
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.title.clone_from(title);
                }
                self.looked(*id, &before);
            }
            Request::SetTabBarPlace(place) => self.bar_place = *place,
            Request::SetStatusBarSide(side) => self.status_bar_side = *side,
            Request::SetInactiveDim(dim) => self.inactive_dim = *dim,
            Request::SetFocusFollowsMouse(on) => self.focus_follows_mouse = *on,
            Request::SetBlockLooks(looks) => self.block_looks = *looks,
            Request::SelectBlock { pane, command } => match command {
                Some(command) => {
                    self.selected_blocks.insert(*pane, *command);
                }
                None => {
                    self.selected_blocks.remove(pane);
                }
            },
            Request::SetWindowTabBarPlace { window, place } => match place {
                Some(place) => {
                    self.window_bar_places.insert(*window, *place);
                }
                None => {
                    self.window_bar_places.remove(window);
                }
            },
            Request::SetTabColor { tab: id, color } => {
                let before = self.look(*id);
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.color = *color;
                }
                self.looked(*id, &before);
            }
            Request::SetTabPinned { tab, pinned } => self.set_tab_pinned(*tab, *pinned),
            Request::NewGroup { tabs, name, color } => {
                if let Opened::Group(id) = opened {
                    self.new_group(id, tabs, name, *color);
                }
            }
            Request::NewSession { name, directory } => {
                if let Opened::Session(id) = opened {
                    self.new_session(id, name, directory.as_deref());
                }
            }
            Request::SwitchSession(session) => self.switch_session(*session),
            Request::SetBroadcast { window, to } => {
                let before = self.broadcasts.get(window).copied().unwrap_or_default();
                if before != *to {
                    if *to == crate::host::BroadcastTo::Off {
                        self.broadcasts.remove(window);
                    } else {
                        self.broadcasts.insert(*window, *to);
                    }
                    self.changed(WorkspaceEvent::BroadcastChanged { window: *window });
                }
            }
            Request::SetPaneBroadcast { pane, left_out } => {
                let changed = if *left_out {
                    self.broadcast_left.insert(*pane)
                } else {
                    self.broadcast_left.remove(pane)
                };
                if changed && let Some(window) = self.panes.get(pane).map(|shown| shown.window) {
                    self.changed(WorkspaceEvent::BroadcastChanged { window });
                }
            }
            Request::AddToGroup { tab, group } => self.add_to_group(*tab, *group),
            Request::RemoveFromGroup(tab) => self.leave_group(*tab),
            Request::RenameGroup { group, name } => {
                if let Some(shown) = self.groups.get_mut(group)
                    && shown.name != *name
                {
                    shown.name.clone_from(name);
                    let window = shown.window;
                    self.group_changed(*group, window);
                }
            }
            Request::SetGroupColor { group, color } => {
                let color = color.filter(|color| matches!(color, Color::Rgb(..)));
                if let Some(shown) = self.groups.get_mut(group)
                    && shown.color != color
                {
                    shown.color = color;
                    let window = shown.window;
                    self.group_changed(*group, window);
                }
            }
            Request::FoldGroup { group, folded } => self.fold_group(*group, *folded),
            Request::Ungroup(group) => {
                if let Some(window) = self.groups.remove(group).map(|shown| shown.window) {
                    for shown in self.tabs.values_mut() {
                        if shown.group == Some(*group) {
                            shown.group = None;
                        }
                    }
                    self.group_changed(*group, window);
                }
            }
            Request::ActivateTab(tab) => self.activate_tab(*tab),
            Request::FocusPane(pane) => self.focus_pane(*pane),
            Request::MoveTab { tab, window, index } => self.move_tab(*tab, *window, *index),
            Request::TabIntoPane { tab, pane, edge } => self.tab_into_pane(*tab, *pane, *edge),
            Request::TabToNewWindow { tab, at } => self.tab_to_new_window(*tab, *at),
            Request::PaneToNewTab {
                pane,
                window,
                index,
            } => self.pane_to_new_tab(*pane, *window, *index),
            Request::PaneToNewWindow(pane) => self.pane_to_new_window(*pane),
            Request::Zoom { tab, pane } => self.zoom(*tab, *pane),
            Request::Equalize(tab) => self.relayout(*tab, equalize),
            Request::SwapPanes { pane, with } => {
                // Both of one tab still, as an earlier write may have moved
                // one, which would hold the other twice.
                let tab_of = |pane: &PaneId| self.panes.get(pane).map(|pane| pane.tab);
                if let Some(tab) = tab_of(pane).filter(|tab| tab_of(with) == Some(*tab)) {
                    self.relayout(tab, |layout| swap_leaves(layout, *pane, *with));
                }
            }
            Request::SetRatio { tab, path, ratio } => {
                let ratio = ratio.map(|ratio| {
                    // A split's share, of ten-thousandths, within 5 % of
                    // its parts' edges, as the app keeps it.
                    (ratio * 10_000.0).round().clamp(500.0, 9_500.0) / 10_000.0
                });
                self.relayout(*tab, |layout| set_ratio(layout, path, ratio));
            }
            // A quick window opens as the app opens it, of a tab of a pane,
            // which takes the focus.
            Request::OpenQuickWindow { .. } => {
                if let Opened::Quick(window, tab, pane) = opened {
                    self.panes.insert(
                        pane,
                        FakePane {
                            window,
                            tab,
                            ..FakePane::default()
                        },
                    );
                    self.quick.insert(window, false);
                    self.given_focus = Some(Some(pane));
                    self.changed(WorkspaceEvent::WindowOpened { window });
                    self.changed(WorkspaceEvent::TabCreated {
                        tab,
                        window,
                        index: 0,
                    });
                    self.changed(WorkspaceEvent::PaneCreated { pane, tab });
                }
            }
            // Shown, it takes the focus; hidden, the app that had it takes
            // it back.
            Request::ShowQuickWindow { window, place } => {
                self.quick.insert(*window, place.is_none());
                self.given_focus = Some(match place {
                    Some(_) => self
                        .panes
                        .iter()
                        .find(|(_, pane)| pane.window == *window)
                        .map(|(&id, _)| id),
                    None => None,
                });
            }
            _ => {}
        }
        self.done.push(request);
    }

    /// Owes the plugin the workspace's `event`.
    fn changed(&mut self, event: WorkspaceEvent) {
        // A tab that comes between two of a group joins it, and the groups
        // settle as their window's tabs change, as the app's.
        let placed = match &event {
            WorkspaceEvent::TabCreated { tab, .. } | WorkspaceEvent::TabMoved { tab, .. } => {
                Some(*tab)
            }
            _ => None,
        };
        let settles = matches!(
            event,
            WorkspaceEvent::TabCreated { .. }
                | WorkspaceEvent::TabClosed { .. }
                | WorkspaceEvent::TabMoved { .. }
                | WorkspaceEvent::WindowClosed { .. }
        );
        self.owed.push(Event::Workspace(event));
        if let Some(tab) = placed {
            self.join_between(tab);
        }
        if settles {
            self.settle_groups();
        }
    }

    /// `event` as the plugin hears it: a command's end of no exit status
    /// to a plugin of no `history-read`, as the app's host tells it.
    fn told(&self, event: Event) -> Event {
        let history = self
            .granted
            .as_ref()
            .is_none_or(|granted| granted.contains(&Capability::HistoryRead));
        if history {
            event
        } else {
            event.without_history()
        }
    }

    /// Whether the plugin reads the processes, as a first-party one does.
    fn reads_processes(&self) -> bool {
        self.granted
            .as_ref()
            .is_none_or(|granted| granted.contains(&Capability::ProcessRead))
    }

    /// The title of `tab`: its own, or its active pane's program's.
    fn tab_title(&self, tab: &FakeTab) -> String {
        tab.title
            .clone()
            .or_else(|| {
                self.panes
                    .get(&tab.active)
                    .and_then(|pane| pane.title.clone())
            })
            .unwrap_or_default()
    }

    /// What a tab bar shows of `id`, whose change `tab-changed` tells: its
    /// title, its color and its marks.
    fn look(&self, id: TabId) -> Option<Look> {
        let tab = self.tabs.get(&id)?;
        Some((self.tab_title(tab), tab.color, tab.marks.clone()))
    }

    /// Owes `tab-changed` of `id` if its look is another than `before`.
    fn looked(&mut self, id: TabId, before: &Option<Look>) {
        if before.is_some() && self.look(id) != *before {
            self.changed(WorkspaceEvent::TabChanged { tab: id });
        }
    }

    /// What a read of the tabs tells of `id`, of the plugin that reads the
    /// processes or not, `reads`, which tells their programs' marks.
    fn tab_info(&self, id: TabId, reads: bool) -> Option<TabInfo> {
        let tab = self.tabs.get(&id)?;
        let window = self.windows.get(&tab.window)?;
        let marks = &tab.marks;
        Some(TabInfo {
            id,
            window: tab.window,
            kind: crate::host::TabKind::Terminal,
            title: self.tab_title(tab),
            color: tab.color,
            active: window.active == id,
            focused: tab.active,
            panes: leaves(&tab.layout),
            zoomed: tab.zoomed,
            activity: marks.activity,
            bell: marks.bell,
            progress: marks.progress,
            printing: marks.printing && reads,
            running: marks.running && reads,
            alternate: marks.alternate && reads,
            secure: marks.secure,
            agent: marks.agent.filter(|_| reads),
            agent_message: marks.agent_message.clone().filter(|_| reads),
            status: marks.status.clone().filter(|_| reads),
            pinned: marks.pinned,
            group: tab.group,
            broadcast: match self.broadcasts.get(&tab.window) {
                Some(crate::host::BroadcastTo::Window) => true,
                Some(crate::host::BroadcastTo::Tab) => window.active == id,
                _ => false,
            },
        })
    }

    /// Learns what it reads of every tab, as the plugin activates, of one
    /// that does not read the processes.
    fn learn_tabs(&mut self) {
        self.told_tabs.clear();
        self.told_panes.clear();
        if self.reads_processes() {
            return;
        }
        let tabs: Vec<TabId> = self.tabs.keys().copied().collect();
        for tab in tabs {
            if let Some(info) = self.tab_info(tab, false) {
                self.told_tabs.insert(tab, info);
            }
        }
    }

    /// Whether the plugin hears `event` as it is delivered: a tab's or a
    /// pane's change that a plugin that does not read the processes cannot
    /// read it does not, as the app's host filters it, of what it learnt of
    /// each tab, its title, its color and its marks, and of each pane, its
    /// program's title.
    fn hears(&mut self, event: &Event) -> bool {
        if self.reads_processes() {
            return true;
        }
        match event {
            Event::Workspace(WorkspaceEvent::TabCreated { tab, .. }) => {
                if let Some(now) = self.tab_info(*tab, false) {
                    self.told_tabs.insert(*tab, now);
                }
                true
            }
            Event::Workspace(WorkspaceEvent::TabClosed { tab, .. }) => {
                self.told_tabs.remove(tab);
                true
            }
            Event::Workspace(WorkspaceEvent::TabChanged { tab }) => {
                match self.tab_info(*tab, false) {
                    Some(now) => self.told_tabs.insert(*tab, now.clone()) != Some(now),
                    None => true,
                }
            }
            Event::Workspace(WorkspaceEvent::PaneChanged { pane, .. }) => {
                match self.panes.get(pane) {
                    Some(found) => {
                        let now = found.title.clone().filter(|title| !title.is_empty());
                        self.told_panes.insert(*pane, now.clone()).flatten() != now
                    }
                    None => true,
                }
            }
            Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => {
                self.told_panes.remove(pane);
                true
            }
            _ => true,
        }
    }

    /// Opens `tab` of `pane` in `window`, at `index` of its tabs or at
    /// their end past them; the window shows the tab that it showed.
    fn open_tab(&mut self, window: WindowId, index: usize, tab: TabId, pane: PaneId) {
        let Some(shown) = self.windows.get_mut(&window) else {
            return;
        };
        let index = index.min(shown.tabs.len());
        shown.tabs.insert(index, tab);
        self.tabs.insert(
            tab,
            FakeTab {
                window,
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
                marks: TabMarks::default(),
                group: None,
            },
        );
        self.panes.insert(
            pane,
            FakePane {
                window,
                tab,
                ..FakePane::default()
            },
        );
        self.changed(WorkspaceEvent::TabCreated { tab, window, index });
        self.changed(WorkspaceEvent::PaneCreated { pane, tab });
    }

    /// Splits `pane`'s part along `direction`, `new` on `side`, which ends
    /// its tab's zoom; the tab's keyboard stays where it was.
    /// Opens the popup `pane` of `command` and `folder` over the active tab
    /// of `window`, of `shares` of its room, with the tab's keyboard, as the
    /// app floats it over the tab's layout.
    fn open_popup(
        &mut self,
        window: Option<WindowId>,
        pane: PaneId,
        (command, folder): (String, Option<String>),
        (width, height): (f64, f64),
    ) {
        let Some(window) = window else {
            return;
        };
        let Some(tab) = self.windows.get(&window).map(|shown| shown.active) else {
            return;
        };
        let Some(back) = self.tabs.get(&tab).map(|shown| shown.active) else {
            return;
        };
        self.panes.insert(
            pane,
            FakePane {
                window,
                tab,
                ..FakePane::default()
            },
        );
        self.popups.insert(
            pane,
            KeptPopup {
                popup: FakePopup {
                    pane,
                    tab,
                    command,
                    folder,
                    width,
                    height,
                },
                back,
                status: None,
            },
        );
        self.changed(WorkspaceEvent::PaneCreated { pane, tab });
        self.changed(WorkspaceEvent::LayoutChanged { tab });
        self.focus_pane(pane);
    }

    fn split(&mut self, pane: PaneId, direction: Direction, side: Side, new: PaneId) {
        let Some(id) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        if !split(&mut tab.layout, pane, direction, new, side) {
            return;
        }
        tab.zoomed = None;
        let window = tab.window;
        self.panes.insert(
            new,
            FakePane {
                window,
                tab: id,
                ..FakePane::default()
            },
        );
        self.changed(WorkspaceEvent::PaneCreated { pane: new, tab: id });
        self.changed(WorkspaceEvent::LayoutChanged { tab: id });
    }

    /// Takes `tab` out of its window's tabs: the window shows the tab after
    /// it, or before it, if it showed this one, and closes if it has none
    /// left. The tab itself stays.
    fn detach_tab(&mut self, tab: TabId) -> Left {
        let Some(id) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return Left::Kept;
        };
        let Some(window) = self.windows.get_mut(&id) else {
            return Left::Kept;
        };
        let Some(index) = window.tabs.iter().position(|&shown| shown == tab) else {
            return Left::Kept;
        };
        window.tabs.remove(index);
        if window.tabs.is_empty() {
            self.windows.remove(&id);
            return Left::Closed(id);
        }
        if window.active != tab {
            return Left::Kept;
        }
        let shown = window.tabs[index.min(window.tabs.len() - 1)];
        window.active = shown;
        Left::Shows(id, shown)
    }

    /// Takes `tab` and its panes away, the panes' closings and the tab's
    /// owed.
    fn drop_tab(&mut self, tab: TabId) {
        let Some(dropped) = self.tabs.remove(&tab) else {
            return;
        };
        for pane in leaves(&dropped.layout) {
            self.panes.remove(&pane);
            self.changed(WorkspaceEvent::PaneClosed { pane, tab });
        }
        self.changed(WorkspaceEvent::TabClosed {
            tab,
            window: dropped.window,
        });
        self.lost_from_group(dropped.group);
    }

    /// Owes that `group`, which a tab that closed was of, lost it, where it
    /// stays, as the app does.
    fn lost_from_group(&mut self, group: Option<GroupId>) {
        if let Some(group) = group
            && let Some(window) = self.groups.get(&group).map(|shown| shown.window)
        {
            self.group_changed(group, window);
        }
    }

    /// Owes what a tab's leaving did to its window.
    fn tell_left(&mut self, left: Left) {
        match left {
            Left::Kept => {}
            Left::Shows(window, tab) => {
                self.changed(WorkspaceEvent::TabActivated { tab, window });
                self.tab_shown(tab);
            }
            Left::Closed(window) => {
                self.changed(WorkspaceEvent::WindowClosed { window });
                let session = self.window_sessions.remove(&window);
                self.session_lost_window(session);
            }
        }
    }

    /// The sessions, in the order of their use, the active one first, as a
    /// read of them tells.
    fn session_infos(&self) -> Vec<SessionInfo> {
        let mut sessions: Vec<&FakeSession> = self.sessions.iter().collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.used));
        sessions
            .into_iter()
            .map(|session| SessionInfo {
                id: session.id,
                name: session.name.clone(),
                active: session.id == self.active_session,
                folder: session.folder.clone(),
                windows: self
                    .windows
                    .keys()
                    .copied()
                    .filter(|window| self.window_sessions.get(window) == Some(&session.id))
                    .collect(),
            })
            .collect()
    }

    /// Makes the session `id` of `name`, of no window, and switches to it,
    /// as the app does, which opens its first window then: the fake host
    /// opens none, as of a request to open a window.
    fn new_session(&mut self, id: SessionId, name: &str, folder: Option<&str>) {
        if self.sessions.iter().any(|session| session.id == id) {
            return;
        }
        self.sessions.push(FakeSession {
            id,
            name: name.to_owned(),
            folder: folder.map(crate::host::home_expanded),
            used: 0,
        });
        self.switch_session(id);
    }

    /// Switches to the session `id`, as the app does, each session's change
    /// owed: the one left, then this one.
    fn switch_session(&mut self, id: SessionId) {
        if self.active_session == id || !self.sessions.iter().any(|session| session.id == id) {
            return;
        }
        let before = std::mem::replace(&mut self.active_session, id);
        self.session_uses += 1;
        let uses = self.session_uses;
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.used = uses;
        }
        self.changed(WorkspaceEvent::SessionChanged { session: before });
        self.changed(WorkspaceEvent::SessionChanged { session: id });
        // The session left goes of no window, as the app's.
        self.session_lost_window(Some(before));
    }

    /// Takes `session`, which lost a window, away, if it has none left and
    /// is not the only one, as the app does: the active one that goes
    /// leaves the one used before it active.
    fn session_lost_window(&mut self, session: Option<SessionId>) {
        let Some(id) = session else {
            return;
        };
        if self.sessions.len() < 2 || self.window_sessions.values().any(|&of| of == id) {
            return;
        }
        if self.active_session == id {
            let last = self.session_infos().get(1).map(|session| session.id);
            if let Some(last) = last {
                self.switch_session(last);
            }
        }
        // Once, as the switch from it may have taken it.
        if self.holds_session(id) {
            self.sessions.retain(|session| session.id != id);
            self.changed(WorkspaceEvent::SessionChanged { session: id });
        }
    }

    /// Whether `session` is held.
    fn holds_session(&self, session: SessionId) -> bool {
        self.sessions.iter().any(|held| held.id == session)
    }

    /// Closes `tab` as [`FakeHost::close_tab`] does, but keeps it to give
    /// back, as the app does, [`host::MOST_CLOSED_TABS`] at most, the
    /// oldest gone for good past them; a window's last tab closes as
    /// before.
    fn close_tab_kept(&mut self, tab: TabId) {
        let Some(window) = self.tabs.get(&tab).map(|shown| shown.window) else {
            return;
        };
        let Some(index) = self
            .windows
            .get(&window)
            .filter(|shown| shown.tabs.len() > 1)
            .and_then(|shown| shown.tabs.iter().position(|&id| id == tab))
        else {
            return self.close_tab(tab);
        };
        let left = self.detach_tab(tab);
        let Some(kept) = self.tabs.remove(&tab) else {
            return;
        };
        let mut panes = Vec::new();
        for pane in leaves(&kept.layout) {
            if let Some(held) = self.panes.remove(&pane) {
                panes.push((pane, held));
            }
            self.changed(WorkspaceEvent::PaneClosed { pane, tab });
        }
        self.changed(WorkspaceEvent::TabClosed { tab, window });
        let group = kept.group;
        self.lost_from_group(group);
        self.closed_tabs.push((tab, kept, panes, window, index));
        let past = self
            .closed_tabs
            .len()
            .saturating_sub(host::MOST_CLOSED_TABS);
        self.closed_tabs.drain(..past);
        self.tell_left(left);
    }

    /// Gives back the tab closed last that it keeps, at its place in its
    /// window, or in `fallback` once its own closed, as the app does.
    fn reopen_tab(&mut self, fallback: WindowId) {
        let Some((tab, mut kept, panes, window, index)) = self.closed_tabs.pop() else {
            return;
        };
        let window = if self.windows.contains_key(&window) {
            window
        } else if self.windows.contains_key(&fallback) {
            fallback
        } else {
            self.closed_tabs.push((tab, kept, panes, window, index));
            return;
        };
        kept.window = window;
        let pinned = kept.marks.pinned;
        let ids: Vec<PaneId> = panes.iter().map(|&(pane, _)| pane).collect();
        for (pane, mut held) in panes {
            held.window = window;
            self.panes.insert(pane, held);
        }
        self.tabs.insert(tab, kept);
        let pinned_count = self.pinned_in(window);
        let mut placed = 0;
        if let Some(shown) = self.windows.get_mut(&window) {
            placed = if pinned {
                index.min(pinned_count)
            } else {
                index.clamp(pinned_count, shown.tabs.len())
            };
            shown.tabs.insert(placed, tab);
        }
        self.changed(WorkspaceEvent::TabCreated {
            tab,
            window,
            index: placed,
        });
        let group = self.tabs.get(&tab).and_then(|shown| shown.group);
        for pane in ids {
            self.changed(WorkspaceEvent::PaneCreated { pane, tab });
        }
        self.lost_from_group(group);
        self.activate_tab(tab);
    }

    /// Closes `tab`, with its panes.
    fn close_tab(&mut self, tab: TabId) {
        if !self.tabs.contains_key(&tab) {
            return;
        }
        let left = self.detach_tab(tab);
        self.drop_tab(tab);
        self.tell_left(left);
    }

    /// Takes `pane` out of `tab`'s layout, its room to its sibling, and the
    /// tab's keyboard and zoom away from it, which the changes tell; a tab
    /// that it leaves without panes leaves its window, what that did to it
    /// returned for the caller to tell after the tab's end.
    fn detach_pane(&mut self, pane: PaneId, id: TabId) -> Option<Left> {
        let before = self.look(id);
        let tab = self.tabs.get_mut(&id)?;
        let Some(nearest) = remove(&mut tab.layout, pane) else {
            return Some(self.detach_tab(id));
        };
        let refocused = tab.active == pane;
        if refocused {
            tab.active = nearest;
        }
        if tab.zoomed == Some(pane) {
            tab.zoomed = None;
        }
        if refocused {
            self.looked(id, &before);
        }
        self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        if refocused {
            self.changed(WorkspaceEvent::PaneFocused {
                pane: nearest,
                tab: id,
            });
        }
        None
    }

    /// Closes `pane`: its room goes to its sibling, and its tab's keyboard,
    /// if it had it, to the sibling's pane nearest to it; a tab that it
    /// leaves without panes closes, and a window without tabs.
    fn close_pane(&mut self, pane: PaneId) {
        let Some(tab) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        self.changed(WorkspaceEvent::PaneClosed { pane, tab });
        // A popup, over its tab's layout, gives the keyboard back to the
        // pane that had it before it, and its plugin hears its close.
        if let Some(kept) = self.popups.remove(&pane) {
            self.panes.remove(&pane);
            self.changed(WorkspaceEvent::LayoutChanged { tab });
            if self
                .tabs
                .get(&tab)
                .is_some_and(|shown| shown.active == pane)
            {
                self.focus_pane(kept.back);
            }
            self.owed.push(Event::PopupClosed {
                pane,
                status: kept.status,
            });
            return;
        }
        let left = self.detach_pane(pane, tab);
        self.panes.remove(&pane);
        if let Some(left) = left {
            if let Some(gone) = self.tabs.remove(&tab) {
                self.changed(WorkspaceEvent::TabClosed {
                    tab,
                    window: gone.window,
                });
                self.lost_from_group(gone.group);
            }
            self.tell_left(left);
        }
    }

    /// Moves the panes of `tab` beside `pane`, of another tab, at its
    /// `edge`, as the workspace does: `tab`'s layout whole in that half,
    /// half and half, the tab gone, its window closing if it has none left,
    /// `pane`'s tab shown, its keyboard at the moved tab's pane that had it.
    fn tab_into_pane(&mut self, tab: TabId, pane: PaneId, edge: Edge) {
        let Some(into) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if into == tab {
            return;
        }
        let (Some(moving), Some(target)) = (self.tabs.get(&tab), self.tabs.get(&into)) else {
            return;
        };
        let mut grafted = target.layout.clone();
        if !graft(&mut grafted, pane, edge, moving.layout.clone()) {
            return;
        }
        let before = self.look(into);
        let left = self.detach_tab(tab);
        let Some(moved) = self.tabs.remove(&tab) else {
            return;
        };
        let Some(target) = self.tabs.get_mut(&into) else {
            return;
        };
        target.layout = grafted;
        target.zoomed = None;
        target.active = moved.active;
        let window = target.window;
        let panes = leaves(&moved.layout);
        for moved_pane in &panes {
            if let Some(shown) = self.panes.get_mut(moved_pane) {
                shown.tab = into;
                shown.window = window;
            }
        }
        for moved_pane in panes {
            self.changed(WorkspaceEvent::PaneMoved {
                pane: moved_pane,
                from: tab,
                tab: into,
            });
        }
        self.changed(WorkspaceEvent::TabClosed {
            tab,
            window: moved.window,
        });
        self.lost_from_group(moved.group);
        // It takes the moved active pane's title, as the app's.
        self.looked(into, &before);
        self.changed(WorkspaceEvent::LayoutChanged { tab: into });
        self.changed(WorkspaceEvent::PaneFocused {
            pane: moved.active,
            tab: into,
        });
        self.tell_left(left);
        self.activate_tab(into);
    }

    /// Has `tab`'s window show it.
    fn activate_tab(&mut self, tab: TabId) {
        let Some(window) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return;
        };
        if let Some(shown) = self.windows.get_mut(&window)
            && shown.active != tab
        {
            shown.active = tab;
            self.changed(WorkspaceEvent::TabActivated { tab, window });
            self.tab_shown(tab);
        }
    }

    /// `tab` shows in its window: its window's tab used last, unless the
    /// order of use holds, and it loses its activity and its bell, as the
    /// app's.
    fn tab_shown(&mut self, tab: TabId) {
        self.shown(tab);
        self.unfold_group_of(tab);
        let before = self.look(tab);
        if let Some(shown) = self.tabs.get_mut(&tab) {
            shown.marks.activity = false;
            shown.marks.bell = false;
        }
        self.looked(tab, &before);
    }

    /// `tab` shows in its window: its window's tab used last, unless the
    /// order of use holds.
    fn shown(&mut self, tab: TabId) {
        if !self.order_held {
            self.tab_use_count += 1;
            self.tab_uses.insert(tab, self.tab_use_count);
        }
    }

    /// The tabs of `window`, in the order of their use, the one that showed
    /// last first, of their own order among those that never showed.
    fn recent_tabs(&self, window: WindowId) -> Option<Vec<TabId>> {
        let mut tabs = self.windows.get(&window)?.tabs.clone();
        tabs.sort_by_key(|tab| std::cmp::Reverse(self.tab_uses.get(tab).copied().unwrap_or(0)));
        Some(tabs)
    }

    /// Gives `pane` its tab's keyboard.
    fn focus_pane(&mut self, pane: PaneId) {
        let Some(id) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        // A zoom of another pane ends, as the app's.
        let unzoomed = self.tabs.get_mut(&id).is_some_and(|tab| {
            let other = tab.zoomed.is_some_and(|zoomed| zoomed != pane);
            if other {
                tab.zoomed = None;
            }
            other
        });
        if self.tabs.get(&id).is_some_and(|tab| tab.active != pane) {
            let before = self.look(id);
            if let Some(tab) = self.tabs.get_mut(&id) {
                tab.active = pane;
            }
            // It takes its new active pane's title, as the app's.
            self.looked(id, &before);
            self.changed(WorkspaceEvent::PaneFocused { pane, tab: id });
        }
        if unzoomed {
            self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        }
    }

    /// Gives `tab`, and its panes, the window `window`.
    fn place_tab(&mut self, tab: TabId, window: WindowId) {
        if let Some(moved) = self.tabs.get_mut(&tab) {
            moved.window = window;
            for pane in leaves(&moved.layout) {
                if let Some(pane) = self.panes.get_mut(&pane) {
                    pane.window = window;
                }
            }
        }
    }

    /// Moves `tab` to `index` of `window`'s tabs, or to their end past
    /// them: the window that it leaves shows the tab after it, or before
    /// it, if it showed this one, and closes if it has none left; the
    /// window that takes it shows the tab that it showed.
    fn move_tab(&mut self, tab: TabId, window: WindowId, index: usize) {
        if !self.tabs.contains_key(&tab) || !self.windows.contains_key(&window) {
            return;
        }
        let from = self.tabs[&tab].window;
        let was = self.windows[&from].tabs.iter().position(|&id| id == tab);
        let left = if from == window {
            if let Some(shown) = self.windows.get_mut(&window) {
                shown.tabs.retain(|&id| id != tab);
            }
            Left::Kept
        } else {
            self.detach_tab(tab)
        };
        self.place_tab(tab, window);
        // A pinned tab among the pinned ones, the others after them.
        let pinned = self.pinned_in(window);
        let held = self.tabs.get(&tab).is_some_and(|shown| shown.marks.pinned);
        let mut placed = 0;
        if let Some(shown) = self.windows.get_mut(&window) {
            placed = if held {
                index.min(pinned)
            } else {
                index.clamp(pinned, shown.tabs.len())
            };
            shown.tabs.insert(placed, tab);
        }
        if from != window || was != Some(placed) {
            self.changed(WorkspaceEvent::TabMoved {
                tab,
                from,
                window,
                index: placed,
            });
        }
        self.tell_left(left);
    }

    /// The tabs of `group`, in its window's order, as the app's.
    fn group_tabs(&self, group: GroupId) -> Vec<TabId> {
        let Some(window) = self
            .groups
            .get(&group)
            .and_then(|shown| self.windows.get(&shown.window))
        else {
            return Vec::new();
        };
        window
            .tabs
            .iter()
            .copied()
            .filter(|tab| {
                self.tabs
                    .get(tab)
                    .is_some_and(|shown| shown.group == Some(group))
            })
            .collect()
    }

    /// The groups of `window`'s tabs, in the order of their tabs, as a read
    /// of them tells.
    fn group_infos(&self, window: WindowId) -> Vec<GroupInfo> {
        let mut found: Vec<GroupInfo> = Vec::new();
        let Some(shown) = self.windows.get(&window) else {
            return found;
        };
        for tab in &shown.tabs {
            let Some(id) = self.tabs.get(tab).and_then(|shown| shown.group) else {
                continue;
            };
            if found.iter().any(|known| known.id == id) {
                continue;
            }
            if let Some(group) = self.groups.get(&id) {
                found.push(GroupInfo {
                    id,
                    window: group.window,
                    name: group.name.clone(),
                    color: group.color,
                    folded: group.folded,
                    tabs: self.group_tabs(id),
                });
            }
        }
        found
    }

    /// Makes the group `id` of `tabs`, as the app does: its tabs next to
    /// one another at the first's place, each out of its group; nothing of
    /// a tab not there or pinned, or of tabs of several windows.
    fn new_group(&mut self, id: GroupId, tabs: &[TabId], name: &str, color: Option<Color>) {
        let Some(window) = tabs
            .first()
            .and_then(|tab| self.tabs.get(tab))
            .map(|shown| shown.window)
        else {
            return;
        };
        let mut members: Vec<TabId> = Vec::new();
        for &tab in tabs {
            let Some(shown) = self.tabs.get(&tab) else {
                return;
            };
            if shown.window != window || shown.marks.pinned {
                return;
            }
            if !members.contains(&tab) {
                members.push(tab);
            }
        }
        if self.groups.contains_key(&id) || !self.windows.contains_key(&window) {
            return;
        }
        // The last first, so that they keep their order, as the app's.
        if let Some(before) = self.windows.get(&window).map(|shown| shown.tabs.clone()) {
            members.sort_by_key(|tab| before.iter().position(|id| id == tab));
        }
        for &tab in members.iter().rev() {
            self.leave_group(tab);
        }
        let Some(order) = self.windows.get(&window).map(|shown| shown.tabs.clone()) else {
            return;
        };
        members.sort_by_key(|tab| order.iter().position(|id| id == tab));
        let Some(at) = order.iter().position(|&tab| tab == members[0]) else {
            return;
        };
        for (k, &tab) in members.iter().enumerate() {
            self.move_tab(tab, window, at + k);
        }
        for tab in &members {
            if let Some(shown) = self.tabs.get_mut(tab) {
                shown.group = Some(id);
            }
        }
        let color = color.filter(|color| matches!(color, Color::Rgb(..)));
        self.groups.insert(
            id,
            FakeGroup {
                window,
                name: name.to_owned(),
                color,
                folded: false,
            },
        );
        self.group_changed(id, window);
    }

    /// Puts `tab` at the end of `group`'s tabs, as the app does.
    fn add_to_group(&mut self, tab: TabId, group: GroupId) {
        let Some(window) = self.groups.get(&group).map(|shown| shown.window) else {
            return;
        };
        let Some(shown) = self.tabs.get(&tab) else {
            return;
        };
        if shown.marks.pinned || shown.group == Some(group) {
            return;
        }
        self.leave_group(tab);
        let Some(&last) = self.group_tabs(group).last() else {
            return;
        };
        let Some(order) = self.windows.get(&window).map(|shown| shown.tabs.clone()) else {
            return;
        };
        let Some(last_at) = order.iter().position(|&id| id == last) else {
            return;
        };
        let index = match order.iter().position(|&id| id == tab) {
            Some(at) if at < last_at => last_at,
            _ => last_at + 1,
        };
        self.move_tab(tab, window, index);
        if let Some(shown) = self.tabs.get_mut(&tab) {
            shown.group = Some(group);
        }
        self.group_changed(group, window);
        self.unfold_if_shown(tab);
    }

    /// Unfolds the group of `tab` where its window shows it, as the app
    /// does.
    fn unfold_if_shown(&mut self, tab: TabId) {
        let shown = self
            .tabs
            .get(&tab)
            .and_then(|shown| self.windows.get(&shown.window))
            .is_some_and(|window| window.active == tab);
        if shown {
            self.unfold_group_of(tab);
        }
    }

    /// Takes `tab` out of its group, right after the group's tabs, as the
    /// app does; a group of no tab left goes.
    fn leave_group(&mut self, tab: TabId) {
        let Some((group, window)) = self
            .tabs
            .get(&tab)
            .and_then(|shown| Some((shown.group?, shown.window)))
        else {
            return;
        };
        let run = self.group_tabs(group);
        let Some(shown) = self.tabs.get_mut(&tab) else {
            return;
        };
        shown.group = None;
        if let Some(&last) = run.last()
            && last != tab
            && run.contains(&tab)
            && let Some(last_at) = self
                .windows
                .get(&window)
                .and_then(|shown| shown.tabs.iter().position(|&id| id == last))
        {
            self.move_tab(tab, window, last_at);
        }
        let window = self.groups.get(&group).map_or(window, |shown| shown.window);
        if self.group_tabs(group).is_empty() {
            self.groups.remove(&group);
        }
        self.group_changed(group, window);
    }

    /// Folds `group`, or unfolds it, as the app does.
    fn fold_group(&mut self, group: GroupId, folded: bool) {
        let Some((window, was)) = self
            .groups
            .get(&group)
            .map(|shown| (shown.window, shown.folded))
        else {
            return;
        };
        if was == folded {
            return;
        }
        if folded {
            let tabs = self.group_tabs(group);
            let Some((order, active)) = self
                .windows
                .get(&window)
                .map(|shown| (shown.tabs.clone(), shown.active))
            else {
                return;
            };
            if tabs.len() >= order.len() {
                return;
            }
            if let Some(shown) = self.groups.get_mut(&group) {
                shown.folded = true;
            }
            if tabs.contains(&active) {
                let position = |tab: &TabId| order.iter().position(|id| id == tab);
                let after = tabs.last().and_then(position).map(|at| at + 1);
                let before = tabs
                    .first()
                    .and_then(position)
                    .and_then(|at| at.checked_sub(1));
                let next = after
                    .and_then(|at| order.get(at))
                    .or_else(|| before.and_then(|at| order.get(at)));
                if let Some(&next) = next {
                    self.activate_tab(next);
                }
            }
        } else if let Some(shown) = self.groups.get_mut(&group) {
            shown.folded = false;
        }
        self.group_changed(group, window);
    }

    /// Unfolds the group of `tab`, which its window shows, if it is folded.
    fn unfold_group_of(&mut self, tab: TabId) {
        let Some(group) = self.tabs.get(&tab).and_then(|shown| shown.group) else {
            return;
        };
        if let Some(shown) = self.groups.get_mut(&group)
            && shown.folded
        {
            shown.folded = false;
            let window = shown.window;
            self.group_changed(group, window);
        }
    }

    /// `tab`, which came to its place, joins the group of the tabs on both
    /// sides of it, as the app's.
    fn join_between(&mut self, tab: TabId) {
        let Some(shown) = self.tabs.get(&tab) else {
            return;
        };
        if shown.marks.pinned {
            return;
        }
        let before = shown.group;
        let Some(order) = self.windows.get(&shown.window).map(|shown| &shown.tabs) else {
            return;
        };
        let Some(at) = order.iter().position(|&id| id == tab) else {
            return;
        };
        let group_at = |index: usize| {
            order
                .get(index)
                .and_then(|id| self.tabs.get(id))
                .and_then(|shown| shown.group)
        };
        let (Some(left), Some(right)) = (at.checked_sub(1).and_then(group_at), group_at(at + 1))
        else {
            return;
        };
        if left != right || before == Some(left) {
            return;
        }
        if let Some(shown) = self.tabs.get_mut(&tab) {
            shown.group = Some(left);
        }
        if let Some(window) = self.groups.get(&left).map(|group| group.window) {
            self.group_changed(left, window);
        }
        self.unfold_if_shown(tab);
        if let Some(before) = before
            && !self.group_tabs(before).is_empty()
            && let Some(window) = self.groups.get(&before).map(|group| group.window)
        {
            self.group_changed(before, window);
        }
    }

    /// Settles the groups as their windows' tabs changed, as the app does.
    fn settle_groups(&mut self) {
        let mut changed: Vec<(GroupId, WindowId)> = Vec::new();
        for shown in self.tabs.values_mut() {
            if let Some(group) = shown.group
                && (shown.marks.pinned || !self.groups.contains_key(&group))
            {
                shown.group = None;
                if let Some(known) = self.groups.get(&group) {
                    changed.push((group, known.window));
                }
            }
        }
        let mut runs: BTreeMap<GroupId, Vec<(WindowId, Vec<TabId>)>> = BTreeMap::new();
        for (&window, shown) in &self.windows {
            let mut last = None;
            for &tab in &shown.tabs {
                let group = self.tabs.get(&tab).and_then(|shown| shown.group);
                if let Some(group) = group {
                    let found = runs.entry(group).or_default();
                    match found.last_mut() {
                        Some((_, run)) if last == Some(group) => run.push(tab),
                        _ => found.push((window, vec![tab])),
                    }
                }
                last = group;
            }
        }
        let ids: Vec<GroupId> = self.groups.keys().copied().collect();
        for id in ids {
            let Some(window) = self.groups.get(&id).map(|group| group.window) else {
                continue;
            };
            let Some(found) = runs.remove(&id) else {
                self.groups.remove(&id);
                changed.push((id, window));
                continue;
            };
            let longest = |own: bool| {
                found
                    .iter()
                    .enumerate()
                    .filter(|(_, (place, _))| !own || *place == window)
                    .max_by_key(|(index, (_, run))| (run.len(), std::cmp::Reverse(*index)))
                    .map(|(index, _)| index)
            };
            let Some(keeper) = longest(true).or_else(|| longest(false)) else {
                continue;
            };
            let mut moved = false;
            for (index, (place, run)) in found.iter().enumerate() {
                if index == keeper {
                    if *place != window
                        && let Some(group) = self.groups.get_mut(&id)
                    {
                        group.window = *place;
                        moved = true;
                    }
                    continue;
                }
                for tab in run {
                    if let Some(shown) = self.tabs.get_mut(tab) {
                        shown.group = None;
                    }
                }
                moved = true;
            }
            if moved && let Some(group) = self.groups.get(&id) {
                changed.push((id, group.window));
            }
        }
        let mut told: Vec<GroupId> = Vec::new();
        for (group, window) in changed {
            if !told.contains(&group) {
                told.push(group);
                self.group_changed(group, window);
            }
        }
    }

    /// Owes the plugin that `group`, of `window`, changed, or went.
    fn group_changed(&mut self, group: GroupId, window: WindowId) {
        self.changed(WorkspaceEvent::GroupChanged { group, window });
    }

    /// How many of `window`'s tabs are pinned, which its first tabs are.
    fn pinned_in(&self, window: WindowId) -> usize {
        self.windows.get(&window).map_or(0, |shown| {
            shown
                .tabs
                .iter()
                .take_while(|tab| self.tabs.get(tab).is_some_and(|tab| tab.marks.pinned))
                .count()
        })
    }

    /// Pins `tab`, at its window's start after the tabs pinned before it,
    /// or unpins it, right after them, as the app does.
    fn set_tab_pinned(&mut self, tab: TabId, pinned: bool) {
        let Some(window) = self.tabs.get(&tab).map(|shown| shown.window) else {
            return;
        };
        if self
            .tabs
            .get(&tab)
            .is_none_or(|shown| shown.marks.pinned == pinned)
        {
            return;
        }
        if pinned {
            self.leave_group(tab);
        }
        let others = self.pinned_in(window).saturating_sub(usize::from(!pinned));
        let before = self.look(tab);
        if let Some(shown) = self.tabs.get_mut(&tab) {
            shown.marks.pinned = pinned;
        }
        self.looked(tab, &before);
        self.move_tab(tab, window, others);
    }

    /// Opens a window whose one tab, which it shows, is `tab`, and returns
    /// it.
    fn window_for(&mut self, tab: TabId) -> WindowId {
        let window = WindowId(self.next_id());
        let from = self.tabs.get(&tab).map(|shown| shown.window);
        let session = from
            .and_then(|from| self.window_sessions.get(&from).copied())
            .unwrap_or(self.active_session);
        self.window_sessions.insert(window, session);
        self.windows.insert(
            window,
            FakeWindow {
                tabs: vec![tab],
                active: tab,
            },
        );
        self.place_tab(tab, window);
        self.changed(WorkspaceEvent::WindowOpened { window });
        self.tab_shown(tab);
        window
    }

    /// Moves `tab` to a window of its own; a tab alone in its window stays.
    fn tab_to_new_window(&mut self, tab: TabId, at: Option<ScreenPoint>) {
        let Some(from) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return;
        };
        let Some(shown) = self.windows.get(&from) else {
            return;
        };
        if shown.tabs.len() == 1 {
            if let Some(at) = at {
                self.places.insert(from, at);
            }
            return;
        }
        let left = self.detach_tab(tab);
        let window = self.window_for(tab);
        if let Some(at) = at {
            self.places.insert(window, at);
        }
        self.changed(WorkspaceEvent::TabMoved {
            tab,
            from,
            window,
            index: 0,
        });
        self.tell_left(left);
    }

    /// Makes a tab of `pane` alone, which left its tab, in `window`, if it
    /// is given, at `index` of its tabs, or in a window of its own, which
    /// opens, and owes its opening and the pane's move from `from`.
    fn new_tab_of(&mut self, pane: PaneId, from: TabId, place: Option<(WindowId, usize)>) {
        let tab = TabId(self.next_id());
        // Of the window that it left, for a window of its own to be of that
        // window's session, as the app's.
        let left = self.tabs.get(&from).map_or(WindowId(0), |from| from.window);
        self.tabs.insert(
            tab,
            FakeTab {
                window: left,
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
                marks: TabMarks::default(),
                group: None,
            },
        );
        if let Some(moved) = self.panes.get_mut(&pane) {
            moved.tab = tab;
        }
        let (window, index) = match place {
            Some((window, index)) => {
                let shown = self.windows.get_mut(&window).expect("an open window");
                let index = index.min(shown.tabs.len());
                shown.tabs.insert(index, tab);
                self.place_tab(tab, window);
                (window, index)
            }
            None => (self.window_for(tab), 0),
        };
        self.changed(WorkspaceEvent::TabCreated { tab, window, index });
        self.changed(WorkspaceEvent::PaneMoved { pane, from, tab });
    }

    /// Moves `pane` out of its tab into a tab of its own at `index` of
    /// `window`'s tabs, or at their end past them; a pane alone in its tab
    /// moves with it.
    fn pane_to_new_tab(&mut self, pane: PaneId, window: WindowId, index: usize) {
        let Some(tab) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if !self.windows.contains_key(&window) {
            return;
        }
        if self
            .tabs
            .get(&tab)
            .is_some_and(|tab| leaves(&tab.layout).len() == 1)
        {
            self.move_tab(tab, window, index);
            return;
        }
        self.detach_pane(pane, tab);
        self.new_tab_of(pane, tab, Some((window, index)));
    }

    /// Moves `pane` out of its tab into a window of its own; a pane alone
    /// in its tab moves with it.
    fn pane_to_new_window(&mut self, pane: PaneId) {
        let Some(tab) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if self
            .tabs
            .get(&tab)
            .is_some_and(|tab| leaves(&tab.layout).len() == 1)
        {
            self.tab_to_new_window(tab, None);
            return;
        }
        self.detach_pane(pane, tab);
        self.new_tab_of(pane, tab, None);
    }

    /// Has `pane` take `tab`'s whole room and its keyboard, or ends its
    /// zoom with `None`.
    fn zoom(&mut self, id: TabId, pane: Option<PaneId>) {
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        if pane.is_some_and(|pane| !leaves(&tab.layout).contains(&pane)) {
            return;
        }
        if tab.zoomed != pane {
            tab.zoomed = pane;
            self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        }
        // The keyboard on the pane that shows, as the app's.
        if let Some(pane) = pane {
            self.focus_pane(pane);
        }
    }

    /// Changes `tab`'s layout with `change`, which owes the change if the
    /// layout is another.
    fn relayout(&mut self, id: TabId, change: impl FnOnce(&mut Layout)) {
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        let before = tab.layout.clone();
        change(&mut tab.layout);
        if tab.layout != before {
            self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        }
    }

    /// Asks for a render of `view`, once.
    fn ask_render(&mut self, view: ViewId) {
        if !self.renders.contains(&view) {
            self.renders.push(view);
        }
    }

    /// The window of `pane`, as its tab's; `not-found` if the host does not
    /// hold it.
    fn pane_window(&self, pane: PaneId) -> Result<WindowId, Error> {
        self.panes
            .get(&pane)
            .and_then(|held| self.tabs.get(&held.tab))
            .map(|tab| tab.window)
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// The window that `view` shows in, if the view is one of the
    /// plugin's: of a slot that its manifest fills, of a window or a pane
    /// that is there, or one of its open panels; `not-found` otherwise.
    fn view_window(&self, view: &ViewId) -> Result<WindowId, Error> {
        let not_found = || Error::new(Code::NotFound, format!("the plugin has no view {view:?}"));
        if let (Some(slot), Some(slots)) = (view.slot(), &self.slots)
            && !slots.contains(&slot)
        {
            return Err(not_found());
        }
        let window = match view {
            ViewId::TabBar(window)
            | ViewId::WindowStrip(window)
            | ViewId::StatusBar(window)
            | ViewId::TitleButtons(window) => {
                Some(*window).filter(|window| self.windows.contains_key(window))
            }
            ViewId::PaneHeader(pane) => self.pane_window(*pane).ok(),
            ViewId::Panel(id) => self.panel_windows.get(id).copied(),
            // A page of its manifest's, while the settings' window is open, in
            // the first window.
            ViewId::Page(page) => self
                .page
                .as_ref()
                .filter(|_| self.has_page(page))
                .and_then(|_| self.windows.keys().next().copied()),
        };
        window.ok_or_else(not_found)
    }

    /// The node of `key` in the last tree of `view`, one of the plugin's;
    /// `not-found` without one.
    fn node(&self, view: &ViewId, key: &Key) -> Result<&Node, Error> {
        self.view_window(view)?;
        self.views
            .get(view)
            .and_then(Option::as_ref)
            .and_then(|tree| keyed(tree, key, 0))
            .ok_or_else(|| {
                Error::new(
                    Code::NotFound,
                    format!("the view {view:?} has no node `{}`", key.0),
                )
            })
    }

    /// The window that a panel at `anchor` shows in: its pane's, its
    /// window, or its node's view's, or its view's.
    fn anchor_window(&self, anchor: &Anchor) -> Result<WindowId, Error> {
        match anchor {
            Anchor::Cell { pane, .. }
            | Anchor::Cursor(pane)
            | Anchor::PaneCorner { pane, .. }
            | Anchor::PaneTop(pane) => self.pane_window(*pane),
            Anchor::Pointer(window) | Anchor::WindowCorner { window, .. } => {
                if self.windows.contains_key(window) {
                    Ok(*window)
                } else {
                    Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ))
                }
            }
            Anchor::Node { view, key } => {
                self.node(view, key)?;
                self.view_window(view)
            }
            Anchor::ViewWindow { view, .. } | Anchor::ViewPointer { view } => {
                self.view_window(view)
            }
        }
    }

    /// The arguments `args` of the plugin's own command `name`, as the
    /// registry checks a call of it, its defaults given; an error of a
    /// command that the plugin did not register.
    fn checked_own(&self, name: &str, args: Args) -> Result<Args, Error> {
        let Some(command) = self.commands.iter().find(|command| command.name == name) else {
            return Err(Error::new(
                Code::NotFound,
                format!("the plugin registers no command `{name}`"),
            ));
        };
        command.check_call(args)
    }

    /// The command `name`, the plugin's or another's, if one is registered.
    fn registered(&self, name: &str) -> Option<&Command> {
        self.commands
            .iter()
            .chain(&self.others)
            .find(|command| command.name == name)
    }

    /// Checks `binding` of a plugin of a manifest that is not a first-party
    /// one, as the app's host checks it: as its menu items, a command that
    /// it may run, of arguments that make a whole call.
    fn check_binding(&self, binding: &Binding) -> Result<(), Error> {
        if self.uses.is_some() {
            self.check_run(&binding.command, &binding.args)?;
        }
        Ok(())
    }

    /// Checks a call of the command `name` with `args` that the plugin
    /// makes, as the app's registry does: of a command that is registered,
    /// of the arguments that its parameters take; of a command that the
    /// plugin's manifest says it runs, and of the capabilities that it
    /// needs, if the plugin is of a manifest that declares them, its own
    /// commands needing neither.
    fn check_run(&self, name: &str, args: &Args) -> Result<(), Error> {
        let Some(command) = self.registered(name) else {
            return Err(Error::new(Code::NotFound, format!("no command `{name}`")));
        };
        command.check_call(args.clone())?;
        let (Some(uses), Some(granted)) = (&self.uses, &self.granted) else {
            return Ok(());
        };
        // Its own, and any as the user's of `user-commands`.
        if self.commands.iter().any(|own| own.name == name)
            || crate::command::grants(granted, &Capability::UserCommands)
        {
            return Ok(());
        }
        if !uses.allows(name) {
            return Err(Error::new(
                Code::Undeclared,
                format!("the plugin runs `{name}`, which its manifest does not declare"),
            ));
        }
        if let Some(missing) = command
            .capabilities
            .iter()
            .find(|capability| !crate::command::grants(granted, capability))
        {
            return Err(Error::new(
                Code::MissingCapability,
                format!("`{name}` needs `{missing}`, which the plugin does not have"),
            ));
        }
        Ok(())
    }

    /// The plugin's namespace, its name's last part.
    fn namespace(&self) -> &str {
        self.caller.rsplit('/').next().unwrap_or_default()
    }

    /// The windows, each of its tabs and the one that it shows, and its
    /// title: the system's to a plugin that reads the processes, its shown
    /// tab's to another, as the app tells them.
    fn windows(&self) -> Vec<WindowInfo> {
        let reads = self.reads_processes();
        self.windows
            .iter()
            .map(|(&id, window)| {
                let (number, keys) = self.numbers.get(&id).cloned().unwrap_or_default();
                let title = if reads {
                    self.titles.get(&id).cloned().unwrap_or_default()
                } else {
                    self.tab_info(window.active, false)
                        .map(|tab| tab.title)
                        .unwrap_or_default()
                };
                WindowInfo {
                    id,
                    tabs: window.tabs.clone(),
                    active: window.active,
                    number,
                    keys,
                    bell: false,
                    title,
                    quick: self.quick.contains_key(&id),
                    hidden: self.quick.get(&id) == Some(&true),
                }
            })
            .collect()
    }
}

/// Whether a key of `kind` takes `value`, as the app's host checks a
/// plugin's section: a boolean, a number of its range, a whole one of a
/// key of integers, a text of its lengths, one of its choices, a table of
/// entries of their names' bounds and of their fields, the required ones
/// among them, each of its kind; a list and another value as they are.
fn setting_takes(kind: &host::SettingKind, value: &Data) -> Result<(), String> {
    use host::SettingKind;
    let number = match value {
        #[allow(clippy::cast_precision_loss)]
        Data::Integer(value) => Some(*value as f64),
        Data::Number(value) => Some(*value),
        _ => None,
    };
    let fine = match kind {
        SettingKind::Boolean => matches!(value, Data::Boolean(_)),
        SettingKind::Number {
            minimum,
            maximum,
            integer,
            ..
        } => number.is_some_and(|number| {
            number.is_finite()
                && (!integer || number.fract() == 0.0)
                && minimum.is_none_or(|least| number >= least)
                && maximum.is_none_or(|most| number <= most)
        }),
        SettingKind::Text {
            min_length,
            max_length,
            ..
        } => match value {
            Data::Text(text) => {
                let count = text.chars().count() as u64;
                min_length.is_none_or(|least| count >= least)
                    && max_length.is_none_or(|most| count <= most)
            }
            _ => false,
        },
        SettingKind::Choice { choices } => choices.contains(value),
        SettingKind::Map { fields } => match value {
            Data::Map(entries) => entries.iter().all(|(name, entry)| {
                let named = !name.is_empty()
                    && name.chars().count() <= host::MOST_ENTRY_NAME
                    && !name
                        .chars()
                        .any(|c| c.is_control() || crate::text::is_hidden(c));
                named
                    && match entry {
                        Data::Map(given) => {
                            fields
                                .iter()
                                .all(|field| !field.required || given.contains_key(&field.name))
                                && given.iter().all(|(field, value)| {
                                    fields.iter().any(|own| {
                                        own.name == *field
                                            && setting_takes(&own.kind, value).is_ok()
                                    })
                                })
                        }
                        _ => false,
                    }
            }),
            _ => false,
        },
        SettingKind::List { .. } | SettingKind::Other => true,
    };
    if fine {
        Ok(())
    } else {
        Err("it does not take the value".to_owned())
    }
}

/// The panes of `layout`, in their order, the first leftmost or topmost.
fn leaves(layout: &Layout) -> Vec<PaneId> {
    match layout {
        Layout::Pane(pane) => vec![*pane],
        Layout::Split { first, second, .. } => {
            let mut panes = leaves(first);
            panes.extend(leaves(second));
            panes
        }
    }
}

/// Puts `grafted` beside the leaf of `pane` in `layout`, at its `edge`,
/// half and half, `grafted` the edge's half; whether `pane` is a leaf of
/// it.
fn graft(layout: &mut Layout, pane: PaneId, edge: Edge, grafted: Layout) -> bool {
    match layout {
        Layout::Pane(leaf) if *leaf == pane => {
            let (direction, first) = match edge {
                Edge::Left => (Direction::Horizontal, true),
                Edge::Right => (Direction::Horizontal, false),
                Edge::Top => (Direction::Vertical, true),
                Edge::Bottom => (Direction::Vertical, false),
            };
            let (first, second) = if first {
                (grafted, Layout::Pane(pane))
            } else {
                (Layout::Pane(pane), grafted)
            };
            *layout = Layout::Split {
                direction,
                ratio: Some(0.5),
                first: Box::new(first),
                second: Box::new(second),
            };
            true
        }
        Layout::Pane(_) => false,
        Layout::Split { first, second, .. } => {
            graft(first, pane, edge, grafted.clone()) || graft(second, pane, edge, grafted)
        }
    }
}

/// Puts `pane` beside the last pane of `layout`, side by side, after it.
fn append(layout: &mut Layout, pane: PaneId) {
    match layout {
        Layout::Pane(last) => {
            *layout = Layout::Split {
                direction: Direction::Horizontal,
                ratio: None,
                first: Box::new(Layout::Pane(*last)),
                second: Box::new(Layout::Pane(pane)),
            };
        }
        Layout::Split { second, .. } => append(second, pane),
    }
}

/// Splits the part of `pane` along `direction`, with `new` on `side`;
/// whether `pane` is one of `layout`'s and `new` is not.
fn split(layout: &mut Layout, pane: PaneId, direction: Direction, new: PaneId, side: Side) -> bool {
    if leaves(layout).contains(&new) {
        return false;
    }
    match layout {
        Layout::Pane(leaf) if *leaf == pane => {
            let (first, second) = match side {
                Side::First => (new, pane),
                Side::Second => (pane, new),
            };
            *layout = Layout::Split {
                direction,
                ratio: None,
                first: Box::new(Layout::Pane(first)),
                second: Box::new(Layout::Pane(second)),
            };
            true
        }
        Layout::Pane(_) => false,
        Layout::Split { first, second, .. } => {
            split(first, pane, direction, new, side) || split(second, pane, direction, new, side)
        }
    }
}

/// Takes `pane`'s part away from `layout`, its split's room to its sibling,
/// which takes the split's place: the sibling's pane nearest to it; `None`
/// if `pane` is no part of a split of `layout`, as its one pane.
fn remove(layout: &mut Layout, pane: PaneId) -> Option<PaneId> {
    let Layout::Split { first, second, .. } = layout else {
        return None;
    };
    let (sibling, nearest) = match (first.as_ref(), second.as_ref()) {
        (Layout::Pane(leaf), sibling) if *leaf == pane => {
            let sibling = sibling.clone();
            let nearest = leaves(&sibling)[0];
            (sibling, nearest)
        }
        (sibling, Layout::Pane(leaf)) if *leaf == pane => {
            let sibling = sibling.clone();
            let nearest = *leaves(&sibling).last().expect("a part has a pane");
            (sibling, nearest)
        }
        _ => return remove(first, pane).or_else(|| remove(second, pane)),
    };
    *layout = sibling;
    Some(nearest)
}

/// `layout` of `pane`'s leaf and `with`'s swapped.
fn swap_leaves(layout: &mut Layout, pane: PaneId, with: PaneId) {
    match layout {
        Layout::Pane(leaf) if *leaf == pane => *leaf = with,
        Layout::Pane(leaf) if *leaf == with => *leaf = pane,
        Layout::Pane(_) => {}
        Layout::Split { first, second, .. } => {
            swap_leaves(first, pane, with);
            swap_leaves(second, pane, with);
        }
    }
}

/// Shares each split of `layout` by its panes again.
fn equalize(layout: &mut Layout) {
    if let Layout::Split {
        ratio,
        first,
        second,
        ..
    } = layout
    {
        *ratio = None;
        equalize(first);
        equalize(second);
    }
}

/// Gives the split at `path` of `layout` the share `to` of its first part,
/// if a split is there.
fn set_ratio(layout: &mut Layout, path: &[Side], to: Option<f64>) {
    let Layout::Split {
        ratio,
        first,
        second,
        ..
    } = layout
    else {
        return;
    };
    match path.split_first() {
        None => *ratio = to,
        Some((Side::First, rest)) => set_ratio(first, rest, to),
        Some((Side::Second, rest)) => set_ratio(second, rest, to),
    }
}

/// The node of `key` in `node`'s tree, `depth` levels down already, if it
/// has one within [`DEEPEST`] levels.
fn keyed<'a>(node: &'a Node, key: &Key, depth: usize) -> Option<&'a Node> {
    if node.key.as_ref() == Some(key) {
        return Some(node);
    }
    if depth >= DEEPEST {
        return None;
    }
    node.children
        .iter()
        .find_map(|child| keyed(child, key, depth + 1))
}

/// The JSON of `data`, as the app keeps a plugin's state; `None` of a
/// number that is not finite, as JSON has none.
fn json(data: &Data) -> Option<serde_json::Value> {
    use serde_json::Value;
    Some(match data {
        Data::Nil => Value::Null,
        Data::Boolean(boolean) => Value::Bool(*boolean),
        Data::Integer(integer) => Value::from(*integer),
        Data::Number(number) => Value::Number(serde_json::Number::from_f64(*number)?),
        Data::Text(text) => Value::String(text.clone()),
        Data::List(items) => Value::Array(items.iter().map(json).collect::<Option<_>>()?),
        Data::Map(map) => Value::Object(
            map.iter()
                .map(|(key, value)| Some((key.clone(), json(value)?)))
                .collect::<Option<_>>()?,
        ),
    })
}

/// The host of one call, which queues its writes, each with the ids that
/// its request gave, and its requests of the app's.
struct Calling<'h> {
    host: &'h mut FakeHost,
    writes: Vec<(Request, Opened)>,
    asked: Vec<Request>,
    /// The panels that the call closed, whose trees go as it returns.
    closed: Vec<PanelId>,
    /// Whether the call only looks, as `applies`, which declares nothing.
    looking: bool,
    /// How many searches the call started.
    searched: usize,
    /// How many images the call rendered.
    imaged: usize,
    /// The saves that the call asked, which show their panels as it
    /// returns: each its id, its window, its file's name and its bytes.
    saving: Vec<(u64, WindowId, String, Vec<u8>)>,
    /// The jobs of the packages' service that the call started, which
    /// start as it returns, and those that it cancelled.
    jobs: Vec<(u64, host::Packages)>,
    cancelled: Vec<u64>,
}

impl Calling<'_> {
    /// A question asked, its id the answer, unless the plugin's last one
    /// waits for its answer.
    fn ask(&mut self) -> Result<Response, Error> {
        if self.host.question.is_some() {
            return Err(Error::new(Code::Unavailable, host::QUESTION_OPEN));
        }
        let id = self.host.next_id();
        self.host.question = Some(id);
        Ok(Response::Asking(id))
    }

    /// Whether the call asked for a render of `view` so far, whose tree the
    /// focus and the selections that it asks after take.
    fn renders(&self, view: &ViewId) -> bool {
        self.writes
            .iter()
            .any(|(request, _)| matches!(request, Request::RequestRender(asked) if asked == view))
    }

    /// Whether the call's writes so far open the pane `pane`.
    fn opens_pane(&self, pane: PaneId) -> bool {
        self.writes.iter().any(|(_, opened)| match opened {
            Opened::Tab(_, opened) | Opened::Pane(opened) => *opened == pane,
            // A quick window's pane opens with the window, as the app opens
            // it after the call.
            Opened::Nothing | Opened::Quick(..) | Opened::Group(_) | Opened::Session(_) => false,
        })
    }

    /// The tab of `pane`, which the call's writes so far open, if they open
    /// it: a new tab's, or that of the pane that a split of theirs splits.
    fn opened_tab_of(&self, pane: PaneId) -> Option<TabId> {
        self.writes
            .iter()
            .find_map(|(request, opened)| match (request, opened) {
                (_, Opened::Tab(tab, opened)) if *opened == pane => Some(*tab),
                (Request::Split { pane: split, .. }, Opened::Pane(opened)) if *opened == pane => {
                    self.host
                        .panes
                        .get(split)
                        .map(|split| split.tab)
                        .or_else(|| self.opened_tab_of(*split))
                }
                (Request::OpenPopup { window, .. }, Opened::Pane(opened)) if *opened == pane => {
                    window
                        .or_else(|| self.host.focused_window())
                        .and_then(|window| self.host.windows.get(&window))
                        .map(|shown| shown.active)
                }
                _ => None,
            })
    }

    /// The pane of the tab `tab` that the call's writes so far open, if
    /// they open it.
    fn opened_tab(&self, tab: TabId) -> Option<PaneId> {
        self.writes.iter().find_map(|(_, opened)| match opened {
            Opened::Tab(opened, pane) if *opened == tab => Some(*pane),
            _ => None,
        })
    }

    /// The window of `tab`, which a group takes: held, or opened by the
    /// call's writes so far, and not pinned.
    fn groupable(&self, tab: TabId) -> Result<WindowId, Error> {
        self.known_tab(tab)?;
        let opened = self
            .writes
            .iter()
            .find_map(|(request, opened)| match (request, opened) {
                (Request::NewTab { window, .. }, Opened::Tab(id, _)) if *id == tab => Some(*window),
                _ => None,
            });
        if let Some(window) = opened {
            return Ok(window);
        }
        let Some(shown) = self.host.tabs.get(&tab) else {
            return Err(Error::new(
                Code::NotFound,
                format!("there is no tab {}", tab.0),
            ));
        };
        if shown.marks.pinned {
            return Err(Error::new(
                Code::Failed,
                format!("the tab {} is pinned, which no group takes", tab.0),
            ));
        }
        Ok(shown.window)
    }

    /// Whether `group` is held, or the call's writes so far make it;
    /// `not-found` otherwise.
    fn known_group(&self, group: GroupId) -> Result<(), Error> {
        let made = self
            .writes
            .iter()
            .any(|(_, opened)| *opened == Opened::Group(group));
        if made || self.host.groups.contains_key(&group) {
            return Ok(());
        }
        Err(Error::new(
            Code::NotFound,
            format!("there is no group {}", group.0),
        ))
    }

    /// Whether `pane` is held, or the call's writes so far open it;
    /// `not-found` otherwise.
    fn known_pane(&self, pane: PaneId) -> Result<(), Error> {
        if self.opens_pane(pane) {
            return Ok(());
        }
        self.host.held(pane).map(|_| ())
    }

    /// That `pane` is no popup, held or that the call's writes so far
    /// open, as a popup stays over its tab; `failed` otherwise.
    fn not_popup(&self, pane: PaneId) -> Result<(), Error> {
        let opened = self.writes.iter().any(|(request, opened)| {
            matches!(request, Request::OpenPopup { .. })
                && matches!(opened, Opened::Pane(new) if *new == pane)
        });
        if opened || self.host.popups.contains_key(&pane) {
            return Err(Error::new(Code::Failed, crate::message::POPUP_STAYS));
        }
        Ok(())
    }

    /// Whether `tab` is held, or the call's writes so far open it;
    /// `not-found` otherwise.
    fn known_tab(&self, tab: TabId) -> Result<(), Error> {
        if self.opened_tab(tab).is_some() || self.host.tabs.contains_key(&tab) {
            return Ok(());
        }
        Err(Error::new(
            Code::NotFound,
            format!("there is no tab {}", tab.0),
        ))
    }

    /// Whether `window` is open; `not-found` otherwise.
    fn known_window(&self, window: WindowId) -> Result<(), Error> {
        if self.host.windows.contains_key(&window) {
            return Ok(());
        }
        Err(Error::new(
            Code::NotFound,
            format!("there is no window {}", window.0),
        ))
    }

    /// Writes the plugin's temporary file `name` of `bytes`, as the app's
    /// host does, in memory: its path in the folder of the host's
    /// temporary files.
    fn temp_file(&mut self, name: &str, bytes: &[u8]) -> Result<Response, Error> {
        if self.looking {
            return Err(Error::new(Code::Failed, host::LOOKING_WRITES_NOTHING));
        }
        let Some(folder) = self.host.temporary_folder.clone() else {
            return Err(Error::new(Code::Failed, host::NO_TEMPORARY_FOLDER));
        };
        let now = self.host.clock;
        if let Some(at) = self.host.temporary_written {
            self.host
                .temporary
                .refill(std::time::Duration::from_millis(now.saturating_sub(at)));
        }
        self.host.temporary_written = Some(now);
        self.host.temporary.check(name, bytes.len())?;
        self.host.temporary.insert(name, bytes.len());
        self.host
            .temporary_bytes
            .insert(name.to_owned(), bytes.to_vec());
        let path = host::temporary_path(&folder, self.host.namespace(), name);
        Ok(Response::Text(Some(path.to_string_lossy().into_owned())))
    }

    /// The title of `pane`: its program's, or, to a plugin that reads the
    /// processes, the name of its folder, as the app's; what a plugin that
    /// does not read them learns of it, of which a change that it cannot
    /// read tells it nothing.
    fn pane_title(&mut self, pane: PaneId) -> Result<Response, Error> {
        let reads = self.host.reads_processes();
        let held = self.host.held(pane)?;
        let title = held.title.clone().filter(|title| !title.is_empty());
        let place = || {
            held.directory.as_deref().map(|directory| {
                directory
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("/")
                    .to_owned()
            })
        };
        let told = title.or_else(|| reads.then(place).flatten());
        if !reads {
            self.host.told_panes.insert(pane, told.clone());
        }
        Ok(Response::Text(told))
    }

    /// Answers `request`, a read, with what the host holds.
    fn read(&mut self, request: &Request) -> Result<Response, Error> {
        if let Request::PaneTitle(pane) = request {
            return self.pane_title(*pane);
        }
        if let Request::TempFile { name, bytes } = request {
            return self.temp_file(name, &bytes.0);
        }
        let host = &*self.host;
        Ok(match request {
            Request::ClipboardText => Response::Text(host.clipboard.clone()),
            Request::FocusedWindow => Response::Window(host.focused_window()),
            Request::FocusedPane => Response::Pane(host.focused_pane()),
            Request::SelectionText(pane) => Response::Text(host.held(*pane)?.selection.clone()),
            Request::LineText { pane, row, reach } => {
                let held = host.held(*pane)?;
                Response::Line(
                    held.lines
                        .get(row)
                        .cloned()
                        .or_else(|| held.line_of_rows(*row, (*reach).min(host::MOST_REACH))),
                )
            }
            Request::LinkAt { pane, column, row } => {
                Response::Text(host.held(*pane)?.links.get(&(*column, *row)).cloned())
            }
            Request::WorkingDirectory(pane) => Response::Text(host.held(*pane)?.directory.clone()),
            Request::Styles { pane, row } => {
                Response::Styles(host.held(*pane)?.styles.get(row).cloned())
            }
            Request::SelectionCells(pane) => Response::Cells(
                host.held(*pane)?
                    .selected_cells
                    .as_ref()
                    .map(|rows| rows.iter().take(crate::image::MOST_ROWS).cloned().collect()),
            ),
            Request::ScreenCells(pane) => Response::Cells(Some(host.held(*pane)?.view_cells())),
            Request::RowCells { pane, first, count } => Response::Cells(
                host.held(*pane)?
                    .row_cells(*first, (*count).min(crate::image::MOST_ROWS)),
            ),
            Request::RenderImage(image) => {
                if self.looking {
                    return Err(Error::new(Code::Failed, crate::image::LOOKING));
                }
                if self.imaged >= crate::image::MOST_IMAGES {
                    return Err(Error::new(Code::Failed, crate::image::IMAGES_BOUND));
                }
                let image = image.clone().bounded();
                image.check()?;
                // Of the plugin's budget of pixels in time, as the app's.
                let (budget, at) = &mut self.host.images;
                let now = self.host.clock;
                if let Some(at) = *at {
                    budget.refill(std::time::Duration::from_millis(now.saturating_sub(at)));
                }
                *at = Some(now);
                budget.check()?;
                self.imaged += 1;
                // A clear pixel, as the fake draws nothing, at the scale
                // asked for or of a display of 2, lowered as the app lowers
                // it past the most; kept, as the app keeps its picture, the
                // oldest let go past the most.
                let drawn = crate::image::nominal(&image, SCALE);
                self.host.images.0.spend(&image, drawn.as_ref().ok());
                let mut rendered = drawn?;
                if image.keep {
                    let id = self.host.next_id();
                    self.host.kept.push(id);
                    if self.host.kept.len() > crate::image::MOST_KEPT {
                        self.host.kept.remove(0);
                    }
                    rendered.id = Some(id);
                }
                Response::Image(rendered)
            }
            Request::Search { pane, query } => {
                if self.looking {
                    return Err(Error::new(
                        Code::Failed,
                        "a call that only looks, as `applies`, starts no search",
                    ));
                }
                if self.searched >= host::MOST_SEARCHES {
                    return Err(Error::new(
                        Code::Failed,
                        format!("a call starts {} searches at most", host::MOST_SEARCHES),
                    ));
                }
                let held = host.held(*pane)?;
                let matches = found_in(held, query)?;
                let (pane, id) = (*pane, self.host.next_id());
                self.searched += 1;
                // The plugin's search of the pane before ends, of no event
                // that it owes, and comes back of a call that fails, as the
                // kept searches do.
                let ended: Vec<u64> = self
                    .host
                    .searches
                    .iter()
                    .filter(|(_, searched)| searched.pane == pane)
                    .map(|(&id, _)| id)
                    .collect();
                for id in ended {
                    self.host.end_search(id);
                }
                self.host.owed.push(Event::Searched {
                    id,
                    pane,
                    matches: matches.clone(),
                    count: matches.len(),
                    done: true,
                });
                self.host.searches.insert(
                    id,
                    FakeSearch {
                        pane,
                        matches,
                        current: None,
                    },
                );
                Response::Searching(id)
            }
            Request::EndSearch(search) => {
                if self.looking {
                    return Err(Error::new(
                        Code::Failed,
                        "a call that only looks, as `applies`, ends no search",
                    ));
                }
                self.host.end_search(*search);
                Response::Done
            }
            Request::SearchMatches {
                search,
                first,
                count,
            } => {
                let Some(FakeSearch { matches, .. }) = host.searches.get(search) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no search {search}"),
                    ));
                };
                Response::Matches(host::Found {
                    matches: matches
                        .iter()
                        .skip(*first)
                        .take((*count).min(host::MOST_LINES))
                        .copied()
                        .collect(),
                    count: matches.len(),
                    done: true,
                })
            }
            Request::CurrentMatch(search) => {
                let Some(kept) = host.searches.get(search) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no search {search}"),
                    ));
                };
                let current = kept.current.and_then(|current| {
                    let place = kept.matches.iter().position(|found| *found == current)?;
                    Some((place, current))
                });
                Response::Current(host::Current {
                    current,
                    kept: kept.matches.len(),
                })
            }
            Request::ReadLines { pane, first, count } => {
                // Every row that it holds from the first, or from its
                // first if that is later, a blank one empty.
                let held = host.held(*pane)?;
                let lines: Vec<host::Line> = ((*first).max(held.held.start)..held.held.end)
                    .take((*count).min(host::MOST_LINES))
                    .map(|row| {
                        let (text, wrapped) = held.rows.get(&row).cloned().unwrap_or_default();
                        host::Line { row, text, wrapped }
                    })
                    .collect();
                let (pane, id) = (*pane, self.host.next_id());
                self.host.owed.push(Event::Lines {
                    id,
                    pane,
                    lines,
                    complete: true,
                });
                Response::Reading(id)
            }
            Request::Opens(target) => {
                Response::Boolean(opening(target).is_ok() && !host.missing.contains(target))
            }
            Request::FontSize => Response::FontSize(host.font_size),
            Request::TabBarPlace => Response::TabBarPlace(host.bar_place),
            Request::Themes => Response::Themes(host.themes.clone()),
            // A file's path to a plugin of `config` alone.
            Request::ThemeTokens(name) => {
                let tokens = match name {
                    Some(name) => host
                        .theme_tokens
                        .iter()
                        .find(|tokens| tokens.theme == *name)
                        .cloned(),
                    None => host.theme_tokens.first().cloned(),
                };
                Response::ThemeTokens(if host.holds_config() {
                    tokens
                } else {
                    tokens.map(host::ThemeTokens::without_folders)
                })
            }
            Request::Fonts => Response::Fonts(host.fonts.clone()),
            Request::FontStyles(family) => Response::FontStyles(font_styles(family)),
            Request::Day => Response::Day(host.day),
            Request::Now => Response::Now(host.local_time),
            Request::GitBranch { folder } => {
                host::check_absolute_folder(folder)?;
                let branch = std::path::Path::new(folder)
                    .ancestors()
                    .find_map(|at| host.git_branches.get(at.to_str()?));
                Response::Text(branch.cloned())
            }
            Request::Agents => Response::Agents(host.agents.clone()),
            // Of no folder that a session was opened in, as the app's.
            Request::SessionFolders => {
                let folders = &host.session_folders;
                let sessions: Vec<&str> = host
                    .sessions
                    .iter()
                    .filter_map(|session| session.folder.as_deref())
                    .collect();
                let (configured, recent) = host::without_sessions(
                    folders.configured.clone(),
                    folders.recent.clone(),
                    &sessions,
                );
                Response::SessionFolders(SessionFolders {
                    configured,
                    recent,
                    ..folders.clone()
                })
            }
            Request::Plugins => Response::Plugins(host.plugins.clone()),
            Request::Settings => Response::Settings(host.settings.clone()),
            Request::NoticeShows => Response::Boolean(host.notice),
            Request::KeepsState => Response::Boolean(host.keeps_state),
            Request::Commands => {
                // The rest of the registry to a plugin of `user-commands`
                // alone, as the app's host gives it.
                let user = host
                    .granted
                    .as_ref()
                    .is_none_or(|granted| granted.contains(&Capability::UserCommands));
                let keys = |name: &str| {
                    host.keys
                        .iter()
                        .find(|(bound, args, _)| bound == name && args.is_empty())
                        .map(|(_, _, keys)| keys.clone())
                };
                let own = host.commands.iter().map(|command| host::CommandInfo {
                    name: command.name.clone(),
                    title: command.title.clone(),
                    category: command.category,
                    params: command.params.clone(),
                    plugin: Some(host.caller.clone()),
                    keys: keys(&command.name),
                    when: command.when.clone(),
                    enabled: Some(true),
                });
                let infos = host
                    .registry
                    .iter()
                    .cloned()
                    .chain(own)
                    .map(|info| {
                        if user {
                            info
                        } else {
                            host::CommandInfo {
                                plugin: None,
                                keys: None,
                                when: None,
                                enabled: None,
                                ..info
                            }
                        }
                    })
                    .collect();
                Response::Commands(infos)
            }
            Request::KeysOf { command, args } => Response::Text(
                host.keys
                    .iter()
                    .find(|(bound, bound_args, _)| bound == command && bound_args == args)
                    .map(|(_, _, keys)| keys.clone()),
            ),
            Request::Message { id, args } => Response::Text(
                host.messages
                    .iter()
                    .find(|(known, known_args, _)| known == id && known_args == args)
                    .map(|(_, _, text)| text.clone()),
            ),
            Request::Windows => Response::Windows(host.windows()),
            Request::Tabs(window) => {
                let Some(found) = host.windows.get(window) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                };
                let reads = host.reads_processes();
                let infos: Vec<TabInfo> = found
                    .tabs
                    .iter()
                    .filter_map(|&id| host.tab_info(id, reads))
                    .collect();
                // What it learns of each tab, of which a change that it
                // cannot read tells it nothing.
                if !reads {
                    for info in &infos {
                        self.host.told_tabs.insert(info.id, info.clone());
                    }
                }
                Response::Tabs(infos)
            }
            Request::FloatingPanes(tab) => {
                if !host.tabs.contains_key(tab) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no tab {}", tab.0),
                    ));
                }
                Response::FloatingPanes(host.floating.get(tab).cloned().unwrap_or(
                    crate::host::FloatingPanes {
                        shown: true,
                        panes: Vec::new(),
                    },
                ))
            }
            Request::Broadcast(window) => {
                if !host.windows.contains_key(window) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                }
                let mut left_out: Vec<PaneId> = host
                    .broadcast_left
                    .iter()
                    .copied()
                    .filter(|pane| {
                        host.panes
                            .get(pane)
                            .is_some_and(|shown| shown.window == *window)
                    })
                    .collect();
                left_out.sort_unstable();
                Response::Broadcast(crate::host::Broadcast {
                    to: host.broadcasts.get(window).copied().unwrap_or_default(),
                    left_out,
                })
            }
            // Their folders, the shells', to a plugin that reads the
            // processes alone.
            Request::Sessions => {
                let reads = host.reads_processes();
                let mut sessions = host.session_infos();
                if !reads {
                    for session in &mut sessions {
                        session.folder = None;
                    }
                }
                Response::Sessions(sessions)
            }
            Request::Groups(window) => {
                if !host.windows.contains_key(window) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                }
                Response::Groups(host.group_infos(*window))
            }
            Request::RecentTabs(window) => {
                let Some(recent) = host.recent_tabs(*window) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                };
                let reads = host.reads_processes();
                let infos: Vec<TabInfo> = recent
                    .into_iter()
                    .filter_map(|id| host.tab_info(id, reads))
                    .collect();
                // What it learns of each tab, of which a change that it
                // cannot read tells it nothing.
                if !reads {
                    for info in &infos {
                        self.host.told_tabs.insert(info.id, info.clone());
                    }
                }
                Response::Tabs(infos)
            }
            Request::PaneGrid(pane) => {
                let (columns, rows) = host.held(*pane)?.grid;
                Response::Grid { columns, rows }
            }
            Request::CommandLine { pane, command } => {
                Response::Text(host.held(*pane)?.command_lines.get(command).cloned())
            }
            Request::EditedLine(pane) => Response::Text(host.held(*pane)?.edited_line.clone()),
            Request::SelectedBlock(pane) => {
                let reads = host.reads_processes();
                let held = host.held(*pane)?;
                let command = host
                    .selected_blocks
                    .get(pane)
                    .and_then(|&id| held.command(id));
                Response::ShellCommand(
                    command
                        .map(|command| {
                            if reads {
                                command
                            } else {
                                command.without_folder()
                            }
                        })
                        .map(Box::new),
                )
            }
            // Their folders, the processes' working directories, to a plugin
            // that reads the processes alone.
            Request::ShellCommands { pane, top, bottom } => {
                let reads = host.reads_processes();
                let commands = host.held(*pane)?.commands_meeting(*top, *bottom);
                Response::ShellCommands(
                    commands
                        .into_iter()
                        .map(|command| {
                            if reads {
                                command
                            } else {
                                command.without_folder()
                            }
                        })
                        .collect(),
                )
            }
            Request::ShellCommand { pane, command } => {
                let reads = host.reads_processes();
                let command = host.held(*pane)?.command(*command);
                Response::ShellCommand(
                    command
                        .map(|command| {
                            if reads {
                                command
                            } else {
                                command.without_folder()
                            }
                        })
                        .map(Box::new),
                )
            }
            Request::CommandAt { pane, row } => {
                let reads = host.reads_processes();
                let command = host.held(*pane)?.command_at(*row);
                Response::ShellCommand(
                    command
                        .map(|command| {
                            if reads {
                                command
                            } else {
                                command.without_folder()
                            }
                        })
                        .map(Box::new),
                )
            }
            Request::Viewport(pane) => Response::Viewport(host.held(*pane)?.viewport()),
            Request::CursorCell(pane) => Response::Cell(Some(host.held(*pane)?.cursor)),
            Request::Layout(tab) => match host.tabs.get(tab) {
                Some(tab) => Response::Layout(tab.layout.clone()),
                None => {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no tab {}", tab.0),
                    ));
                }
            },
            Request::Config => Response::Data(host.config.clone()),
            Request::State => Response::Data(host.state.clone()),
            other => {
                return Err(Error::new(Code::Fault, format!("{other:?} is not a read")));
            }
        })
    }

    /// Checks `request`, a write or a request of the app's, and queues it:
    /// one of a pane, a tab or a window that the host does not hold, nor
    /// the call's writes so far open, or of a target that does not open,
    /// fails now, and so does one past the [`host::MOST_WRITES`] of a call.
    fn queue(&mut self, request: Request) -> Result<Response, Error> {
        if self.writes.len() + self.asked.len() >= host::MOST_WRITES {
            return Err(Error::new(
                Code::Failed,
                format!(
                    "a call writes and asks the app {} times at most, which apply as it returns",
                    host::MOST_WRITES
                ),
            ));
        }
        let mut opened = Opened::Nothing;
        let response = match &request {
            Request::SelectCells { pane, .. }
            | Request::Deselect(pane)
            | Request::ShowCursor { pane, .. }
            | Request::SelectAll(pane)
            | Request::ClearScrollback(pane)
            | Request::Clear(pane)
            | Request::Paste { pane, .. }
            | Request::PasteBroadcast { pane, .. }
            | Request::SendTextBroadcast { pane, .. }
            | Request::SetPaneBroadcast { pane, .. }
            | Request::SendText { pane, .. }
            | Request::Scroll { pane, .. }
            | Request::FocusPane(pane)
            | Request::ClosePane { pane, .. } => {
                self.known_pane(*pane)?;
                Response::Done
            }
            Request::PaneToNewWindow(pane) => {
                self.known_pane(*pane)?;
                self.not_popup(*pane)?;
                Response::Done
            }
            Request::SetBroadcast { window, .. } => {
                self.known_window(*window)?;
                Response::Done
            }
            Request::SetTabTitle { tab, .. } => {
                self.known_tab(*tab)?;
                Response::Done
            }
            Request::SelectBlock { pane, command } => {
                let held = self.host.held(*pane)?;
                if let Some(command) = command
                    && held.command(*command).is_none()
                {
                    return Err(Error::new(
                        Code::NotFound,
                        host::no_command(*pane, *command),
                    ));
                }
                Response::Done
            }
            Request::RunLine { pane, line } => {
                let edited = self.host.held(*pane)?.edited_line.as_deref();
                host::check_run_line(*pane, edited, line)
                    .map_err(|why| Error::new(Code::Unavailable, why))?;
                Response::Done
            }
            Request::SetClipboardImage(png) => {
                png.check_png()?;
                Response::Done
            }
            Request::SetTabBarPlace(place) => {
                place
                    .check()
                    .map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::SetStatusBarSide(_) => Response::Done,
            Request::SetInactiveDim(dim) => {
                if !(0.0..=1.0).contains(dim) {
                    return Err(Error::new(Code::Failed, DIM_RANGE));
                }
                Response::Done
            }
            Request::SetFocusFollowsMouse(_) | Request::SetBlockLooks(_) => Response::Done,
            Request::ShowMatches {
                search, current, ..
            } => {
                let Some(kept) = self.host.searches.get_mut(search) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no search {search}"),
                    ));
                };
                kept.current = *current;
                Response::Done
            }
            Request::SetWindowTabBarPlace { window, place } => {
                self.known_window(*window)?;
                if let Some(place) = place {
                    place
                        .check()
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                Response::Done
            }
            Request::SetTabColor { tab, color } => {
                self.known_tab(*tab)?;
                if matches!(color, Some(Color::Palette(_))) {
                    return Err(Error::new(
                        Code::Failed,
                        "a tab's color is an RGB one, not one of the palette's",
                    ));
                }
                Response::Done
            }
            // One question at a time, of its texts bounded.
            Request::AskText {
                window,
                message,
                detail,
                ..
            } => {
                host::check_question(message, detail)?;
                if let Some(window) = window {
                    self.known_window(*window)?;
                }
                self.ask()?
            }
            Request::AskChoice {
                window,
                message,
                detail,
                buttons,
            } => {
                host::check_buttons(buttons)?;
                host::check_question(message, detail)?;
                if let Some(window) = window {
                    self.known_window(*window)?;
                }
                self.ask()?
            }
            Request::SaveFile {
                window,
                name,
                bytes,
            } => {
                crate::host::check_file_name(name)?;
                bytes.check()?;
                if let Some(window) = window {
                    self.known_window(*window)?;
                }
                let Some(window) = window.or_else(|| self.host.focused_window()) else {
                    return Err(Error::new(Code::Unavailable, crate::message::NO_WINDOW));
                };
                let showing = self
                    .host
                    .saves
                    .values()
                    .map(|(shown, _, _)| *shown)
                    .chain(self.saving.iter().map(|(_, shown, _, _)| *shown))
                    .any(|shown| shown == window);
                if showing {
                    return Err(Error::new(Code::Failed, crate::message::SAVING));
                }
                let id = self.host.next_id();
                self.saving
                    .push((id, window, name.clone(), bytes.0.clone()));
                Response::Asking(id)
            }
            // A notice of its bounds, a second at least after the last.
            Request::Notify { text, title, .. } => {
                host::check_notice(text, title.as_deref())?;
                let clock = self.host.clock;
                let paused = self
                    .host
                    .noticed_at
                    .is_some_and(|at| clock < at.saturating_add(host::NOTICE_PAUSE));
                if paused {
                    return Err(Error::new(Code::Unavailable, crate::message::NOTICE_PAUSED));
                }
                self.host.noticed_at = Some(clock);
                Response::Done
            }
            Request::Packages(call) => {
                call.check()?;
                let id = self.host.next_id();
                self.jobs.push((id, call.clone()));
                Response::Asking(id)
            }
            Request::CancelPackages(id) => {
                self.cancelled.push(*id);
                Response::Done
            }
            // A page of the plugin's manifest.
            Request::ShowPage(page) => {
                if !self.host.has_page(page) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no page `{page}`"),
                    ));
                }
                Response::Done
            }
            // A key of the settings that the test gave, of a value that its
            // kind takes, as the app's host checks it.
            Request::SetSetting {
                section,
                plugin,
                key,
                value,
            } => {
                // Nothing over a file that does not read, as the app's host.
                if let Some(problem) = &self.host.settings.error {
                    return Err(Error::new(
                        Code::InvalidConfig,
                        format!(
                            "the config file does not read, and nothing is written into it until it does: {}",
                            problem.message
                        ),
                    ));
                }
                // A binding of any keys, of a command registered; another
                // key of the settings that the test gave, of a value that
                // its kind takes.
                if plugin.is_none()
                    && section == "keys"
                    && value.is_some()
                    && crate::keys::parse_keys(key)
                        .is_ok_and(|keys| crate::keys::is_system(&keys, None))
                {
                    return Err(Error::new(
                        Code::InvalidConfig,
                        crate::keys::system_error(key),
                    ));
                }
                if plugin.is_none() && section == "keys" {
                    let command = match value {
                        Some(Data::Text(command)) => Some(command),
                        Some(Data::Map(table)) => match table.get("command") {
                            Some(Data::Text(command)) => Some(command),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(command) = command
                        && self.host.registered(command).is_none()
                    {
                        return Err(Error::new(
                            Code::NotFound,
                            format!("there is no command `{command}`"),
                        ));
                    }
                } else if plugin.is_some() && key == "enabled" {
                    // The host's key of a plugin's section, which turns the
                    // plugin on or off, of a boolean alone.
                    let known = self
                        .host
                        .settings
                        .sections
                        .iter()
                        .any(|shown| shown.name == *section && shown.plugin == *plugin);
                    if !known {
                        return Err(Error::new(
                            Code::NotFound,
                            format!("there is no plugin's section `{section}`"),
                        ));
                    }
                    if !matches!(value, Some(Data::Boolean(_))) {
                        return Err(Error::new(
                            Code::InvalidConfig,
                            format!("`enabled` in `[plugins.{section}]` is true or false"),
                        ));
                    }
                } else {
                    let found = self
                        .host
                        .settings
                        .sections
                        .iter()
                        .find(|shown| shown.name == *section && shown.plugin == *plugin)
                        .and_then(|shown| shown.keys.iter().find(|shown| shown.name == *key));
                    let Some(found) = found else {
                        let message = match plugin {
                            None => format!("the settings have no key `{key}` in `[{section}]`"),
                            Some(plugin) => format!(
                                "the plugin `{plugin}` has no key `{key}` in its section `{section}`"
                            ),
                        };
                        return Err(Error::new(Code::NotFound, message));
                    };
                    if let Some(value) = value
                        && let Err(why) = setting_takes(&found.kind, value)
                    {
                        return Err(Error::new(
                            Code::InvalidConfig,
                            format!("`{key}` in `{section}`: {why}"),
                        ));
                    }
                }
                Response::Done
            }
            // A plugin that the host has, as the app's checks it.
            Request::SetPluginEnabled { plugin, .. } => {
                if !self.host.plugins.iter().any(|info| &info.name == plugin) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no plugin `{plugin}`"),
                    ));
                }
                Response::Done
            }
            Request::SwapPanes { pane, with } => {
                self.known_pane(*pane)?;
                self.known_pane(*with)?;
                // Of a pane that the call's writes opened too.
                let tab_of = |pane: &PaneId| {
                    self.host
                        .panes
                        .get(pane)
                        .map(|pane| pane.tab)
                        .or_else(|| self.opened_tab_of(*pane))
                };
                if tab_of(pane) != tab_of(with) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("the panes {} and {} are not of one tab", pane.0, with.0),
                    ));
                }
                Response::Done
            }
            Request::ResizePane { pane, by, .. } => {
                self.known_pane(*pane)?;
                if !(-1.0..=1.0).contains(by) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("a resize of {by} is not a share from -1 to 1"),
                    ));
                }
                Response::Done
            }
            Request::CloseTab { keep, .. } if *keep > host::MOST_KEEP => {
                return Err(Error::new(
                    Code::Failed,
                    format!("a closed tab is kept a day at most, not {keep} seconds"),
                ));
            }
            Request::ReopenTab(window) => {
                self.known_window(*window)?;
                Response::Done
            }
            Request::CloseTab { tab, .. }
            | Request::ActivateTab(tab)
            | Request::SetTabPinned { tab, .. }
            | Request::TabToNewWindow { tab, .. }
            | Request::Equalize(tab) => {
                self.known_tab(*tab)?;
                Response::Done
            }
            Request::SetRatio { tab, ratio, .. } => {
                self.known_tab(*tab)?;
                if ratio.is_some_and(|ratio| !(0.0..=1.0).contains(&ratio)) {
                    return Err(Error::new(
                        Code::Failed,
                        "a split's ratio is a share from 0 to 1",
                    ));
                }
                Response::Done
            }
            Request::OpenWindow {
                directory: Some(directory),
            } => {
                crate::host::check_directory(directory)
                    .map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::OpenQuickWindow { place, directory } => {
                place
                    .check()
                    .map_err(|message| Error::new(Code::Failed, message))?;
                if let Some(directory) = directory {
                    crate::host::check_directory(directory)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                let window = WindowId(self.host.next_id());
                let (tab, pane) = (TabId(self.host.next_id()), PaneId(self.host.next_id()));
                opened = Opened::Quick(window, tab, pane);
                Response::Window(Some(window))
            }
            Request::ShowQuickWindow { window, place } => {
                if let Some(place) = place {
                    place
                        .check()
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                if !self.host.quick.contains_key(window) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no quick window {}", window.0),
                    ));
                }
                Response::Done
            }
            Request::PreviewTheme(Some(name)) | Request::SetTheme { name, .. }
                if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') =>
            {
                return Err(Error::new(
                    Code::Failed,
                    format!("`{name}` is no theme's name"),
                ));
            }
            // A monospaced family that the system has, previewed or chosen.
            Request::PreviewFont(Some(family)) | Request::SetFont(family) => {
                self.host
                    .fonts
                    .check(family)
                    .map_err(|message| Error::new(Code::NotFound, message))?;
                Response::Done
            }
            // A style of the family in force.
            Request::PreviewFontStyle { style, .. } => {
                let family = &self.host.fonts.current;
                host::check_style(family, &font_styles(family), style)
                    .map_err(|message| Error::new(Code::NotFound, message))?;
                Response::Done
            }
            // A choice that the config keeps, of a theme that is there.
            Request::SetTheme { name, .. } => {
                self.host
                    .themes
                    .check(name)
                    .map_err(|message| Error::new(Code::NotFound, message))?;
                Response::Done
            }
            // A copy of a theme that is there, an `.itermcolors` one of the
            // user's folder converted, whose answers the test posts.
            Request::CopyTheme(name) => {
                host::copied_theme(&self.host.themes, name)?;
                Response::Asking(self.host.next_id())
            }
            Request::NewTheme(_) => Response::Asking(self.host.next_id()),
            Request::ConvertTheme(name) => {
                host::converted_theme(&self.host.themes, name)?;
                Response::Asking(self.host.next_id())
            }
            Request::SetThemeValue { theme, name, value } => {
                host::check_theme_value(&self.host.themes, theme, name, value.as_deref())?;
                Response::Done
            }
            // A preview of none ends one, whatever the color.
            Request::PreviewThemeValue { theme, name, value } => {
                if let Some(value) = value {
                    host::check_theme_value(&self.host.themes, theme, name, Some(value))?;
                }
                Response::Done
            }
            Request::ShowColorPanel { .. } | Request::CloseColorPanel => Response::Done,
            Request::SetFontSize(Some(size)) => {
                // The text's sizes, in points, as the app takes them.
                let (smallest, largest) = (4.0_f32, 200.0_f32);
                let points = f64::from(*size);
                if !(f64::from(smallest)..=f64::from(largest)).contains(&points) {
                    return Err(Error::new(
                        Code::Failed,
                        format!(
                            "the size, in points, is from {smallest} to {largest}, not {points}"
                        ),
                    ));
                }
                Response::Done
            }
            Request::SetState(value) => {
                let failed = |message: String| Error::new(Code::Failed, message);
                let text = json(value)
                    .ok_or_else(|| {
                        failed("a plugin's state holds finite numbers alone".to_owned())
                    })?
                    .to_string();
                if text.len() > MOST_STATE {
                    return Err(failed(format!(
                        "a plugin's state is {MOST_STATE} bytes at most as JSON, not {}",
                        text.len()
                    )));
                }
                Response::Done
            }
            Request::RunCommand { name, args } => {
                self.host.check_run(name, args)?;
                Response::Done
            }
            Request::RequestRender(view) => {
                self.host.view_window(view)?;
                Response::Done
            }
            Request::SetText { view, key, .. } => {
                if !matches!(self.host.node(view, key)?.kind, Kind::Field(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a field", key.0),
                    ));
                }
                Response::Done
            }
            Request::Select { view, .. } if self.renders(view) => Response::Done,
            Request::Select { view, key, .. } => {
                if !matches!(self.host.node(view, key)?.kind, Kind::List(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a list", key.0),
                    ));
                }
                Response::Done
            }
            // A node of the tree that the call renders, as the app gives the
            // focus to as it takes the tree, or of the one shown.
            Request::FocusNode { view, key } => {
                if !self.renders(view) {
                    self.host.node(view, key)?;
                }
                Response::Done
            }
            Request::Reveal { view, .. } => {
                if !self.host.views.contains_key(view) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has rendered no view {view:?}"),
                    ));
                }
                Response::Done
            }
            Request::MoveTab { tab, window, .. } => {
                self.known_tab(*tab)?;
                self.known_window(*window)?;
                Response::Done
            }
            Request::TabIntoPane { tab, pane, .. } => {
                self.known_tab(*tab)?;
                self.known_pane(*pane)?;
                if self.host.panes.get(pane).map(|shown| shown.tab) == Some(*tab) {
                    return Err(Error::new(Code::Failed, OWN_PANE));
                }
                Response::Done
            }
            Request::CloseWindow(window) => {
                self.known_window(*window)?;
                Response::Done
            }
            Request::PaneToNewTab { pane, window, .. } => {
                self.known_pane(*pane)?;
                self.not_popup(*pane)?;
                self.known_window(*window)?;
                Response::Done
            }
            Request::Zoom { tab, pane } => {
                let panes = match self.opened_tab(*tab) {
                    Some(opened) => vec![opened],
                    None => match self.host.tabs.get(tab) {
                        Some(found) => leaves(&found.layout),
                        None => {
                            return Err(Error::new(
                                Code::NotFound,
                                format!("there is no tab {}", tab.0),
                            ));
                        }
                    },
                };
                if let Some(pane) = pane
                    && !panes.contains(pane)
                {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the tab {} has no pane {}", tab.0, pane.0),
                    ));
                }
                Response::Done
            }
            Request::Decorate {
                pane,
                from,
                to,
                look,
            } => {
                let held = &self.host.held(*pane)?.held;
                if !held.contains(&from.1)
                    || !held.contains(&to.1)
                    || (to.1, to.0) < (from.1, from.0)
                {
                    return Err(Error::new(
                        Code::Failed,
                        "a decoration's cells are not the pane's, or not in reading order",
                    ));
                }
                if let Some(label) = &look.label {
                    host::check_label(label)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                let id = self.host.next_id();
                self.host.decorations.push(FakeDecoration {
                    id,
                    pane: *pane,
                    from: *from,
                    to: *to,
                    look: look.clone(),
                });
                Response::Decoration(id)
            }
            Request::Undecorate { pane, id } => {
                // A decoration of a pane that closed since is the plugin's
                // still, as the app's host keeps it.
                let own = self
                    .host
                    .decorations
                    .iter()
                    .any(|laid| laid.id == *id && laid.pane == *pane);
                if !own {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no decoration {id} of the pane {}", pane.0),
                    ));
                }
                Response::Done
            }
            Request::MarkGutter { pane, row, color } => {
                if !self.host.held(*pane)?.held.contains(row) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("the pane {} holds no row {row}", pane.0),
                    ));
                }
                let id = self.host.next_id();
                self.host.gutter_marks.push(FakeGutterMark {
                    id,
                    pane: *pane,
                    row: *row,
                    color: *color,
                });
                Response::Decoration(id)
            }
            Request::UnmarkGutter { pane, id } => {
                let own = self
                    .host
                    .gutter_marks
                    .iter()
                    .any(|laid| laid.id == *id && laid.pane == *pane);
                if !own {
                    return Err(Error::new(
                        Code::NotFound,
                        format!(
                            "the plugin has no mark {id} of the pane {}'s gutter",
                            pane.0
                        ),
                    ));
                }
                Response::Done
            }
            Request::Open(target) => {
                opening(target).map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::OpenAt { target, line, .. } => {
                opening_at(target, *line).map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::NewTab {
                window, directory, ..
            } => {
                self.known_window(*window)?;
                if let Some(directory) = directory {
                    crate::host::check_directory(directory)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                let (tab, pane) = (TabId(self.host.next_id()), PaneId(self.host.next_id()));
                opened = Opened::Tab(tab, pane);
                Response::Tab { tab, pane }
            }
            Request::Split { pane, .. } => {
                self.known_pane(*pane)?;
                let new = PaneId(self.host.next_id());
                opened = Opened::Pane(new);
                Response::Pane(Some(new))
            }
            Request::OpenPopup {
                window,
                command,
                folder,
                width,
                height,
            } => {
                crate::host::check_popup(command, folder.as_deref(), *width, *height)
                    .map_err(|message| Error::new(Code::Failed, message))?;
                match window {
                    Some(window) => self.known_window(*window)?,
                    None if self.host.focused_window().is_none() => {
                        return Err(Error::new(
                            Code::NotFound,
                            "no window has the focus, which a popup opens in",
                        ));
                    }
                    None => {}
                }
                let new = PaneId(self.host.next_id());
                opened = Opened::Pane(new);
                Response::Pane(Some(new))
            }
            Request::NewGroup { tabs, name, color } => {
                check_group(name, *color)?;
                // Each tab once, in its order, as the app's.
                let mut seen = BTreeSet::new();
                let tabs: Vec<TabId> = tabs
                    .iter()
                    .copied()
                    .filter(|tab| seen.insert(*tab))
                    .collect();
                let Some(&first) = tabs.first() else {
                    return Err(Error::new(Code::Failed, "a group has one tab at least"));
                };
                let window = self.groupable(first)?;
                for &tab in &tabs[1..] {
                    if self.groupable(tab)? != window {
                        return Err(Error::new(Code::Failed, "a group's tabs are of one window"));
                    }
                }
                let id = GroupId(self.host.next_id());
                opened = Opened::Group(id);
                Response::Group(id)
            }
            Request::AddToGroup { tab, group } => {
                self.groupable(*tab)?;
                self.known_group(*group)?;
                Response::Done
            }
            Request::RemoveFromGroup(tab) => {
                self.known_tab(*tab)?;
                Response::Done
            }
            Request::RenameGroup { group, name } => {
                check_group(name, None)?;
                self.known_group(*group)?;
                Response::Done
            }
            Request::SetGroupColor { group, color } => {
                check_group("", *color)?;
                self.known_group(*group)?;
                Response::Done
            }
            Request::FoldGroup { group, .. } | Request::Ungroup(group) => {
                self.known_group(*group)?;
                Response::Done
            }
            Request::NewSession { name, directory } => {
                crate::host::check_session_name(name)
                    .map_err(|message| Error::new(Code::Failed, message))?;
                if let Some(directory) = directory {
                    crate::host::check_session_directory(directory)
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                let id = SessionId(self.host.next_id());
                opened = Opened::Session(id);
                Response::Session(id)
            }
            Request::SwitchSession(session) => {
                let made = self
                    .writes
                    .iter()
                    .any(|(_, opened)| *opened == Opened::Session(*session));
                if !made && !self.host.holds_session(*session) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no session {}", session.0),
                    ));
                }
                Response::Done
            }
            Request::OpenPanel { id, options } => {
                if id.0.is_empty() {
                    return Err(Error::new(Code::InvalidName, "a panel's id is not empty"));
                }
                if self.host.panels.contains_key(id) {
                    return Err(Error::new(
                        Code::AlreadyRegistered,
                        format!("the plugin has a panel `{}` open", id.0),
                    ));
                }
                let window = self.host.anchor_window(&options.anchor)?;
                self.host.panels.insert(id.clone(), options.clone());
                self.host.panel_windows.insert(id.clone(), window);
                self.host.ask_render(ViewId::Panel(id.clone()));
                Response::Done
            }
            Request::ClosePanel(id) => {
                if self.host.panels.remove(id).is_none() {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no panel `{}` open", id.0),
                    ));
                }
                // Forgotten now, so that the same call can open it again; its
                // tree goes, and its plugin is owed that it closed, as the
                // call returns, unless it fails.
                self.host.panel_windows.remove(id);
                let view = ViewId::Panel(id.clone());
                self.host.renders.retain(|asked| *asked != view);
                self.closed.push(id.clone());
                Response::Done
            }
            Request::Emit { name, schema, .. } => {
                let namespace = self.host.namespace();
                let own = name
                    .strip_prefix(namespace)
                    .is_some_and(|rest| rest.starts_with('.'));
                if !is_name(name) || !own {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!("`{name}` is not an event of the plugin's, `{namespace}.event`"),
                    ));
                }
                if schema.is_empty() {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!("the event `{name}` names no schema"),
                    ));
                }
                Response::Done
            }
            _ => Response::Done,
        };
        // The quick windows' requests change what the fake keeps as the app
        // would, as the writes do.
        let quick = matches!(
            request,
            Request::OpenQuickWindow { .. } | Request::ShowQuickWindow { .. }
        );
        match request.effect() {
            Effect::App if !quick => self.asked.push(request),
            _ => self.writes.push((request, opened)),
        }
        Ok(response)
    }

    /// Checks `request`, a declaration, as the app's host does, and takes
    /// it at once: [`host::MOST_DECLARED`] commands, keys and menu items at
    /// most, and a key or an item declared already changing nothing.
    fn declare(&mut self, request: Request) -> Result<Response, Error> {
        if matches!(
            request,
            Request::RegisterCommand(_) | Request::Bind(_) | Request::AddMenuItem(_)
        ) {
            let host = &*self.host;
            let declared = host.commands.len() + host.bindings.len() + host.menu_items.len();
            if declared >= host::MOST_DECLARED {
                return Err(Error::new(
                    Code::Failed,
                    format!(
                        "a plugin declares {} commands, keys and menu items at most",
                        host::MOST_DECLARED
                    ),
                ));
            }
        }
        match request {
            Request::RegisterCommand(command) => {
                let host = &*self.host;
                if host.uses.is_some() {
                    let namespace = host.namespace();
                    let prefix = format!("{namespace}.");
                    if !command.name.starts_with(&prefix) {
                        return Err(Error::new(
                            Code::InvalidName,
                            format!(
                                "`{}` is not of the plugin's namespace, `{namespace}`",
                                command.name
                            ),
                        ));
                    }
                    if host
                        .others
                        .iter()
                        .any(|other| other.name.starts_with(&prefix))
                    {
                        return Err(Error::new(
                            Code::AlreadyRegistered,
                            format!(
                                "the namespace `{namespace}` has another plugin's commands, or the app's"
                            ),
                        ));
                    }
                }
                if host.registered(&command.name).is_some() {
                    return Err(Error::new(
                        Code::AlreadyRegistered,
                        format!("`{}` is registered already", command.name),
                    ));
                }
                command.check()?;
                self.host.commands.push(command);
            }
            Request::Bind(binding) if binding.keys.starts_with(crate::keys::GLOBAL) => {
                let invalid = |message: String| Error::new(Code::InvalidBinding, message);
                crate::keys::parse_global(&binding.keys[crate::keys::GLOBAL.len()..])
                    .map_err(invalid)?;
                if !is_name(&binding.command) {
                    return Err(invalid(format!(
                        "`{}` is not a command's name, `namespace.action`",
                        binding.command
                    )));
                }
                if binding.when.is_some() {
                    return Err(invalid(format!(
                        "a global key holds everywhere, without `when`, in `{}`",
                        binding.keys
                    )));
                }
                self.host.check_binding(&binding)?;
                if !self.host.bindings.contains(&binding) {
                    self.host.bindings.push(binding);
                }
            }
            Request::Bind(binding) => {
                let invalid = |message: String| Error::new(Code::InvalidBinding, message);
                parse_keys(&binding.keys).map_err(invalid)?;
                if !is_name(&binding.command) {
                    return Err(invalid(format!(
                        "`{}` is not a command's name, `namespace.action`",
                        binding.command
                    )));
                }
                if let Some(when) = &binding.when {
                    When::parse(when).map_err(invalid)?;
                }
                self.host.check_binding(&binding)?;
                if !self.host.bindings.contains(&binding) {
                    self.host.bindings.push(binding);
                }
            }
            Request::AddMenuItem(item) => {
                item.check()?;
                if !is_name(&item.command) {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!(
                            "`{}` is not a command's name, `namespace.action`",
                            item.command
                        ),
                    ));
                }
                self.host.check_run(&item.command, &item.args)?;
                // The same item again, of another tick, changes its tick.
                match self
                    .host
                    .menu_items
                    .iter_mut()
                    .find(|kept| kept.same_as(&item))
                {
                    Some(kept) => kept.ticked = item.ticked,
                    None => self.host.menu_items.push(item),
                }
            }
            Request::Subscribe(names) => {
                if let Some(name) = names
                    .iter()
                    .find(|name| !crate::message::is_event_name(name))
                {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!(
                            "`{name}` is no event: one of the workspace's, as `tab-created`, or a plugin's, `namespace.event`"
                        ),
                    ));
                }
                for name in names {
                    if !self.host.subscribed.contains(&name) {
                        self.host.subscribed.push(name);
                    }
                }
            }
            Request::StartTimer { delay, repeats } => {
                if delay > host::MOST_DELAY {
                    return Err(Error::new(Code::Failed, DELAY_RANGE));
                }
                if self.host.timers.len() >= host::MOST_TIMERS {
                    return Err(Error::new(Code::Unavailable, TIMERS_BOUND));
                }
                let every = delay.max(host::LEAST_DELAY);
                let id = self.host.next_id();
                let at = self.host.clock.saturating_add(every);
                self.host.timers.insert(
                    id,
                    FakeTimer {
                        at: Some(at),
                        every: repeats.then_some(every),
                        due: false,
                    },
                );
                return Ok(Response::Timer(id));
            }
            Request::CancelTimer(id) => {
                self.host.timers.remove(&id);
            }
            Request::WatchDoubleTap(modifier) => self.host.double_tap = modifier,
            other => {
                return Err(Error::new(
                    Code::Fault,
                    format!("{other:?} is not a declaration"),
                ));
            }
        }
        Ok(Response::Done)
    }
}

impl host::Host for Calling<'_> {
    fn handle(&mut self, call: Call) -> Response {
        if call.caller != self.host.caller {
            return Response::Failed(Error::new(
                Code::Fault,
                format!(
                    "a call of the plugin `{}` came as `{}`'s",
                    self.host.caller, call.caller
                ),
            ));
        }
        let request = match crate::message::within_depth(call.request) {
            Ok(request) => request,
            Err(error) => return Response::Failed(error),
        };
        if let Some(granted) = &self.host.granted
            && let Some(needed) = request.lacking(granted)
        {
            return Response::Failed(Error::new(
                Code::MissingCapability,
                format!(
                    "the plugin `{}` lacks the capability `{needed}`",
                    self.host.caller
                ),
            ));
        }
        let answered = match request.effect() {
            Effect::Read => {
                self.host.reads.push(request.clone());
                self.read(&request)
            }
            Effect::Write | Effect::App => self.queue(request),
            Effect::Declaration if self.looking => Err(Error::new(
                Code::Failed,
                "a call that only looks, as `applies`, declares nothing",
            )),
            Effect::Declaration => self.declare(request),
        };
        answered.unwrap_or_else(Response::Failed)
    }
}

/// The name of the machine that the fake host is, which a `file` URL's
/// host names, as the app's host takes its own.
pub const HOST_NAME: &str = "fake-mac";

/// Whether `target` opens, as the app's host takes it: a URL of one of
/// [`OPEN_SCHEMES`](crate::host::OPEN_SCHEMES), a `file` URL of this machine alone, or a path from `/` or
/// `~`; the error of what does not.
fn opening(target: &str) -> Result<(), String> {
    if let Some(host) = crate::host::file_host(target)
        && !crate::host::is_local_host(host, HOST_NAME)
    {
        return Err(format!(
            "a `file:` URL of another machine, `{host}`, does not open"
        ));
    }
    if target.starts_with('/') || target == "~" || target.starts_with("~/") {
        return Ok(());
    }
    match crate::host::url_scheme(target) {
        Some(scheme) if crate::host::scheme_opens(scheme) => Ok(()),
        Some(scheme) => Err(format!(
            "`{scheme}:` does not open: only {}",
            crate::host::OPEN_SCHEMES.join(", ")
        )),
        None => Err(format!(
            "`{target}` is neither a URL nor a path that is absolute or from `~`"
        )),
    }
}

/// Whether a plugin may open `target` at `line`, as the app's host checks
/// it: a file of this machine, a path or a `file` URL, at a line from 1.
fn opening_at(target: &str, line: u32) -> Result<(), String> {
    let file = target.starts_with('/')
        || target == "~"
        || target.starts_with("~/")
        || crate::host::file_host(target).is_some();
    if !file || line == 0 {
        return Err(format!(
            "`{target}` at line {line} is no file's line, which counts from 1"
        ));
    }
    opening(target)
}

/// `tree` as text: a line of each node, two spaces deeper for each level,
/// of its kind, its text, and its key, label, role, tooltip and focus when
/// it has them; a list of its count, its items given, each of its index and
/// its texts, and its row's template, and a view of tabs of its tabs and
/// the one chosen. Its layout and its style are not
/// told, as a snapshot tells the tree's content.
pub fn snapshot(tree: &Node) -> String {
    let mut out = String::new();
    write_node(&mut out, tree, 0);
    out
}

fn write_node(out: &mut String, node: &Node, depth: usize) {
    let _ = write!(out, "{}", "  ".repeat(depth));
    let quoted = |text: &str| format!("{text:?}");
    match &node.kind {
        Kind::Container => out.push_str("container"),
        Kind::Text(text) => {
            let _ = write!(out, "text {}", quoted(text));
        }
        Kind::Icon(name) => {
            let _ = write!(out, "icon {}", quoted(name));
        }
        Kind::Keys(keys) => {
            let _ = write!(out, "keys {}", quoted(keys));
        }
        Kind::Spacer => out.push_str("spacer"),
        Kind::TerminalSpecimen => out.push_str("terminal-specimen"),
        Kind::Themed(name) => {
            let _ = write!(out, "themed {}", quoted(name));
        }
        Kind::Field(field) => {
            let _ = write!(out, "field {}", quoted(&field.placeholder));
            if let Some(text) = &field.text {
                let _ = write!(out, " text={}", quoted(text));
            }
            if field.search {
                out.push_str(" search");
            }
        }
        Kind::Button(button) => {
            let _ = write!(out, "button {}", quoted(&button.label));
            if !button.icon.is_empty() {
                let _ = write!(out, " icon={}", quoted(&button.icon));
            }
            if button.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Toggle(toggle) => {
            let _ = write!(out, "toggle {}", quoted(&toggle.label));
            if !toggle.icon.is_empty() {
                let _ = write!(out, " icon={}", quoted(&toggle.icon));
            }
            out.push_str(if toggle.on { " on" } else { " off" });
            if toggle.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Checkbox(checkbox) => {
            let _ = write!(out, "checkbox {}", quoted(&checkbox.label));
            out.push_str(match (checkbox.mixed, checkbox.on) {
                (true, _) => " mixed",
                (false, true) => " on",
                (false, false) => " off",
            });
            if checkbox.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Radios(radios) => {
            let options: Vec<String> = radios
                .options
                .iter()
                .map(|option| {
                    let label = quoted(&option.label);
                    if option.disabled {
                        format!("{label} disabled")
                    } else {
                        label
                    }
                })
                .collect();
            let _ = write!(out, "radios {}", options.join(" | "));
            if let Some(chosen) = radios.chosen {
                let _ = write!(out, " chosen {chosen}");
            }
            if radios.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::PopUp(pop_up) => {
            let choices: Vec<String> = pop_up.choices.iter().map(|text| quoted(text)).collect();
            let _ = write!(out, "pop-up {}", choices.join(" | "));
            if let Some(chosen) = pop_up.chosen {
                let _ = write!(out, " chosen {chosen}");
            }
            if !pop_up.placeholder.is_empty() {
                let _ = write!(out, " placeholder={}", quoted(&pop_up.placeholder));
            }
            if pop_up.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Switch(switch) => {
            let _ = write!(out, "switch {}", quoted(&switch.label));
            out.push_str(if switch.on { " on" } else { " off" });
            if switch.size == crate::view::ControlSize::Small {
                out.push_str(" small");
            }
            if switch.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Separator => out.push_str("separator"),
        Kind::Group => out.push_str("group"),
        Kind::List(list) => {
            let _ = write!(
                out,
                "list of {}, {} given from {}",
                list.count,
                list.items.len(),
                list.first
            );
        }
        Kind::Scroll => out.push_str("scroll"),
        Kind::Progress(progress) => {
            let _ = write!(out, "progress {:?} {:?}", progress.shape, progress.state);
            if progress.state != crate::view::ProgressState::Indeterminate {
                let _ = write!(out, " {}", progress.value);
            }
        }
        Kind::Tabs(tabs) => {
            let labels: Vec<String> = tabs.tabs.iter().map(|tab| quoted(&tab.label)).collect();
            let _ = write!(out, "tabs {} chosen {}", labels.join(" | "), tabs.chosen);
        }
        Kind::Image(id) => {
            let _ = write!(out, "image {id}");
        }
        Kind::ThemePreview(name) => {
            let _ = write!(out, "theme-preview {}", quoted(name));
        }
    }
    if let Some(key) = &node.key {
        let _ = write!(out, " key={}", key.0);
    }
    if let Some(label) = &node.label {
        let _ = write!(out, " label={}", quoted(label));
    }
    if let Some(role) = &node.role {
        let _ = write!(out, " role={role:?}");
    }
    if let Some(tooltip) = &node.tooltip {
        let _ = write!(out, " tooltip={}", quoted(tooltip));
    }
    if node.focusable {
        out.push_str(" focusable");
    }
    if node.reorderable {
        out.push_str(" reorderable");
    }
    if let Some(tab) = node.carries {
        let _ = write!(out, " carries=tab-{}", tab.0);
    }
    out.push('\n');
    if let Kind::List(list) = &node.kind {
        // Its items given, each of its texts, from its first.
        for (at, item) in list.items.iter().enumerate() {
            let texts: Vec<String> = item.texts.iter().map(|text| quoted(text)).collect();
            let _ = writeln!(
                out,
                "{}item {} {}",
                "  ".repeat(depth + 1),
                list.first + at,
                texts.join(" | ")
            );
        }
        let _ = writeln!(out, "{}row:", "  ".repeat(depth + 1));
        write_node(out, &list.row, depth + 2);
    }
    for child in &node.children {
        write_node(out, child, depth + 1);
    }
}

/// The capabilities that the manifest `text` declares in its
/// `[capabilities]`, the commands of its `[uses]`, if it has them, and its
/// settings' window's pages, as [`FakeHost::with_manifest`] reads them.
fn declared(text: &str) -> Declared {
    let (mut capabilities, mut uses, mut builtin) = (Vec::new(), None, false);
    let mut pages = Vec::new();
    let mut status_bar_side = host::StatusBarSide::Start;
    let mut section = "";
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            section = match name.trim() {
                "plugin" => "plugin",
                "capabilities" => "capabilities",
                "uses" => "uses",
                "contributes" => "contributes",
                "contributes.status-bar" => "status-bar",
                _ => "",
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, mut value) = (key.trim(), value.trim().to_owned());
        // A list over lines, to its end.
        while value.starts_with('[') && !value.contains(']') {
            match lines.next() {
                Some(next) => value.push_str(next.trim()),
                None => break,
            }
        }
        // The texts of a list, between its quotes.
        let texts = || -> Vec<String> {
            value
                .split('"')
                .skip(1)
                .step_by(2)
                .map(str::to_owned)
                .collect()
        };
        match section {
            "capabilities" if value == "true" => capabilities.push(
                Capability::of_name(key)
                    .unwrap_or_else(|| panic!("`{key}` is no capability of a manifest")),
            ),
            "capabilities" if value == "false" => {}
            "capabilities" => capabilities.extend(texts().into_iter().map(|parameter| {
                Capability::of_parameter(key, parameter)
                    .unwrap_or_else(|| panic!("`{key}` is no capability of a list"))
            })),
            "uses" if key == "commands" => uses = Some(texts()),
            "plugin" if key == "kind" => builtin = value == "\"builtin\"",
            // Each inline table's `id`.
            "contributes" if key == "pages" => {
                pages.extend(value.split('{').skip(1).filter_map(|table| {
                    table.split([',', '}']).find_map(|pair| {
                        let (key, value) = pair.split_once('=')?;
                        (key.trim() == "id").then(|| value.trim().trim_matches('"').to_owned())
                    })
                }));
            }
            "status-bar" if key == "side" => {
                status_bar_side = match value.trim_matches('"') {
                    "center" => host::StatusBarSide::Center,
                    "end" => host::StatusBarSide::End,
                    _ => host::StatusBarSide::Start,
                };
            }
            _ => {}
        }
    }
    Declared {
        capabilities,
        uses,
        builtin,
        pages,
        status_bar_side,
    }
}

/// What a plugin's manifest declares, of [`declared`].
struct Declared {
    /// Its capabilities.
    capabilities: Vec<Capability>,
    /// The commands that it runs, if it says.
    uses: Option<Vec<String>>,
    /// Whether it is one of Lokyr's own.
    builtin: bool,
    /// The ids of its settings' window's pages.
    pages: Vec<String>,
    /// The side of the status bar of its view.
    status_bar_side: host::StatusBarSide,
}

/// The system's day that the fake host gives by default, in days since
/// 1970-01-01: 11 November 2026.
pub const DAY: u64 = 20_768;

/// The folder of the host's temporary files that the fake gives by
/// default, which their paths are of; the fake writes nothing there.
pub const TEMPORARY_FOLDER: &str = "/tmp/lokyr-temporary";

/// The system's time that the fake host gives by default: 11 November 2026
/// at noon in UTC, of a time zone an hour east of it, as Central European
/// Time.
pub const LOCAL_TIME: host::LocalTime = host::LocalTime {
    time: (DAY * 86_400 + 12 * 3600) * 1000,
    offset: 3600,
};

/// The tokens of the themes that the fake host gives by default: Lokyr
/// Dark's, in force, of a token of its own, one of a rule and a plugin's.
pub fn theme_tokens() -> Vec<host::ThemeTokens> {
    let token = |name: &str, color: u32, origin: host::TokenOrigin| host::ThemeToken {
        name: name.to_owned(),
        color: crate::theme::ThemeColor::hex(color),
        value: (!matches!(origin, host::TokenOrigin::Rule { .. }))
            .then(|| crate::theme::ThemeColor::hex(color).to_string()),
        origin,
    };
    vec![host::ThemeTokens {
        theme: "Lokyr Dark".to_owned(),
        variant: crate::theme::ThemeVariant::Dark,
        terminal: vec![
            token(
                "background",
                0x1e1e20,
                host::TokenOrigin::Theme {
                    theme: "Lokyr Dark".to_owned(),
                },
            ),
            token(
                "cursor",
                0xe6e6e6,
                host::TokenOrigin::Rule {
                    text: "foreground".to_owned(),
                },
            ),
            token(
                "bright-blue",
                0x86bcff,
                host::TokenOrigin::File {
                    path: "/themes/Lokyr Dark.toml".to_owned(),
                    line: 8,
                    theme: "Lokyr Dark".to_owned(),
                },
            ),
        ],
        tokens: vec![
            token(
                "accent",
                0x4c9bff,
                host::TokenOrigin::Theme {
                    theme: "Lokyr Dark".to_owned(),
                },
            ),
            token(
                "focus.ring",
                0x4c9bff,
                host::TokenOrigin::Rule {
                    text: "accent / 60%".to_owned(),
                },
            ),
            token(
                "command-header.pill",
                0x2a2a2d,
                host::TokenOrigin::File {
                    path: "/themes/Lokyr Dark.toml".to_owned(),
                    line: 3,
                    theme: "Lokyr Dark".to_owned(),
                },
            ),
        ],
    }]
}

/// The tokens of the theme of `name` among [`theme_tokens`], the first's
/// of none.
pub fn theme_tokens_of(name: Option<&str>) -> Option<host::ThemeTokens> {
    let all = theme_tokens();
    match name {
        Some(name) => all.into_iter().find(|tokens| tokens.theme == name),
        None => all.into_iter().next(),
    }
}

/// The system's families that the fake host gives by default: Menlo,
/// Monaco and SF Mono, which is in force, monospaced, and Helvetica and SF
/// Pro beside them.
pub fn fonts() -> host::Fonts {
    host::Fonts {
        current: "SF Mono".to_owned(),
        families: vec![
            "Menlo".to_owned(),
            "Monaco".to_owned(),
            "SF Mono".to_owned(),
        ],
        all: ["Helvetica", "Menlo", "Monaco", "SF Mono", "SF Pro"]
            .map(str::to_owned)
            .to_vec(),
    }
}

/// The styles of `family` that the fake host gives by default, of one of
/// [`fonts`]' families: Regular, Italic, Bold and Bold Italic; none of
/// another.
pub fn font_styles(family: &str) -> Vec<String> {
    if fonts().all.iter().any(|known| known == family) {
        ["Regular", "Italic", "Bold", "Bold Italic"]
            .map(str::to_owned)
            .to_vec()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;

/// Whether a group's `name` and `color` are a group's, as the app checks
/// them.
fn check_group(name: &str, color: Option<Color>) -> Result<(), Error> {
    host::check_group(name, color).map_err(|message| Error::new(Code::Failed, message))
}

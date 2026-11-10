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
    self, BarPlace, Binding, Color, DecorationLook, Direction, Edge, Layout, LineText, MOST_STATE,
    PaneId, ScreenPoint, Side, StyleRun, TabId, TabInfo, WindowId, WindowInfo,
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
                    ..*command
                })
            })
            .collect()
    }

    /// Its view, as a read of it gives it.
    fn viewport(&self) -> host::Viewport {
        self.viewport.unwrap_or_else(|| {
            let top = self.held.end - i64::from(self.grid.1);
            host::Viewport {
                top,
                rows: self.grid.1,
                screen: top,
                oldest: self.held.start,
                epoch: 0,
                alternate: false,
            }
        })
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
            viewport: None,
            cursor: (0, 0),
            held_cursor: None,
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
}

/// What a tab bar shows of a tab, whose change `tab-changed` tells: its
/// title, its color and its marks.
type Look = (String, Option<Color>, TabMarks);

/// What a tab bar marks a tab of, as a read of the tabs tells it, which a
/// test gives a tab of the fake host.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    /// The quick windows that the plugin opened, each whether it is hidden.
    quick: BTreeMap<WindowId, bool>,
    /// The keys of commands of their arguments, as the test gave them.
    keys: Vec<(String, Args, String)>,
    /// The registry's commands of the app's and of other plugins', before
    /// the plugin's own, as the test gave them.
    registry: Vec<host::CommandInfo>,
    /// The themes that the app can show, as the test gave them.
    themes: host::Themes,
    /// Where the plugin's moves of tabs to windows of their own put those
    /// windows on the screens.
    places: BTreeMap<WindowId, ScreenPoint>,
    /// The texts of the plugin's messages, of their arguments, as the
    /// host's catalogs would give them.
    messages: Vec<(String, BTreeMap<String, Data>, String)>,
    /// The place of the plugin's tab bar.
    bar_place: BarPlace,
    /// The places of the plugin's tab bar of windows of their own.
    window_bar_places: BTreeMap<WindowId, BarPlace>,
    /// The fade of the panes that have not the keyboard, and whether the
    /// keyboard follows the pointer, as the plugin asked.
    inactive_dim: f32,
    focus_follows_mouse: bool,
    /// The window that has the focus.
    focused: Option<WindowId>,
    /// The focus that the test gave last, which the next look at the
    /// panes takes, once the test gave them their tabs.
    given_focus: Option<Option<PaneId>>,
    clipboard: Option<String>,
    font_size: f32,
    missing: Vec<String>,
    config: Data,
    state: Data,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    menu_items: Vec<crate::host::MenuItem>,
    subscribed: Vec<String>,
    done: Vec<Request>,
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
            uses: None,
            slots: None,
            others: Vec::new(),
            activated: false,
            panes: BTreeMap::new(),
            windows: BTreeMap::new(),
            tabs: BTreeMap::new(),
            numbers: BTreeMap::new(),
            quick: BTreeMap::new(),
            places: BTreeMap::new(),
            keys: Vec::new(),
            registry: Vec::new(),
            themes: host::Themes {
                appearance: crate::theme::ThemeVariant::Dark,
                current: "Lokyr Dark".to_owned(),
                themes: vec![
                    host::ThemeInfo {
                        name: "Lokyr Light".to_owned(),
                        variant: crate::theme::ThemeVariant::Light,
                        origin: host::ThemeOrigin::BuiltIn,
                    },
                    host::ThemeInfo {
                        name: "Lokyr Dark".to_owned(),
                        variant: crate::theme::ThemeVariant::Dark,
                        origin: host::ThemeOrigin::BuiltIn,
                    },
                ],
            },
            messages: Vec::new(),
            bar_place: BarPlace::default(),
            window_bar_places: BTreeMap::new(),
            inactive_dim: 0.0,
            focus_follows_mouse: false,
            focused: None,
            given_focus: None,
            clipboard: None,
            font_size: 13.0,
            missing: Vec::new(),
            config: Data::Map(BTreeMap::new()),
            state: Data::Nil,
            commands: Vec::new(),
            bindings: Vec::new(),
            menu_items: Vec::new(),
            subscribed: Vec::new(),
            done: Vec::new(),
            decorations: Vec::new(),
            gutter_marks: Vec::new(),
            panels: BTreeMap::new(),
            panel_windows: BTreeMap::new(),
            views: BTreeMap::new(),
            renders: Vec::new(),
            owed: Vec::new(),
            told_tabs: BTreeMap::new(),
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
        let (capabilities, uses) = declared(manifest);
        let host = self.with_capabilities(capabilities);
        match uses {
            Some(patterns) => host.with_uses(patterns),
            None => host,
        }
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

    /// Gives the app the themes that it can show, `themes`, which a read of
    /// them gives; the built-in ones, dark in force, at first.
    pub fn set_themes(&mut self, themes: host::Themes) -> &mut Self {
        self.themes = themes;
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
        self.panes.retain(|_, pane| pane.window != window);
        self.quick.remove(&window);
        if self.focused == Some(window) {
            self.given_focus = Some(None);
        }
        self.changed(WorkspaceEvent::WindowClosed { window });
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

    /// The clipboard's text.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
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
        self.owed.push(event);
        self
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
    /// a search's matches, a question's answer and its config's change, and
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
                    | Event::ConfigChanged
                    | Event::Answered { .. }
                    | Event::TabBarResized { .. }
            ) || self.subscribed.iter().any(|name| name == event.name());
            if taken && self.hears(&event) {
                batch.push(event);
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
    /// nests past [`Node::DEEPEST`] levels it refuses, as the host does,
    /// and gives the last, as of a render that fails, as the plugin tells
    /// after it, whose writes go.
    pub fn render(&mut self, plugin: &mut dyn Plugin, view: &ViewId) -> Option<Node> {
        let mut tree = None;
        let rendered = self.call(true, |cx| {
            tree = plugin.render(cx, view);
            plugin.take_failure().map_or(Ok(()), Err)
        });
        // A tree too deep the host refuses, the last staying, as it drops
        // it without recursion.
        if let Some(deep) = tree.take_if(|tree| rendered.is_err() || !tree.nests_within_deepest()) {
            crate::view::dismantle(deep);
            return self.views.get(view).cloned().flatten();
        }
        if rendered.is_err() {
            return self.views.get(view).cloned().flatten();
        }
        self.views.insert(view.clone(), tree.clone());
        tree
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
        );
        let mut calling = Calling {
            host: self,
            writes: Vec::new(),
            asked: Vec::new(),
            closed: Vec::new(),
            looking: !apply,
            searched: 0,
        };
        let result = call(&mut Cx::new(&mut calling, &caller));
        let (writes, asked, closed) = (calling.writes, calling.asked, calling.closed);
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
            ) = kept;
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
        self.done.extend(asked);
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
        for (&id, tab) in tabs.iter() {
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
            Request::SetClipboardText(text) => self.clipboard = Some(text.clone()),
            Request::ShowCursor { pane, at } => {
                if let Some(pane) = self.panes.get_mut(pane) {
                    pane.held_cursor = *at;
                }
            }
            Request::Deselect(pane) => {
                if let Some(pane) = self.panes.get_mut(pane) {
                    pane.selection = None;
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
            // A close that asks closes as the user's confirmation would,
            // which a test reads among the requests done.
            Request::CloseTab { tab, .. } => self.close_tab(*tab),
            Request::ClosePane { pane, .. } => self.close_pane(*pane),
            Request::SetTabTitle { tab: id, title } => {
                let before = self.look(*id);
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.title.clone_from(title);
                }
                self.looked(*id, &before);
            }
            Request::SetTabBarPlace(place) => self.bar_place = *place,
            Request::SetInactiveDim(dim) => self.inactive_dim = *dim,
            Request::SetFocusFollowsMouse(on) => self.focus_follows_mouse = *on,
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
        self.owed.push(Event::Workspace(event));
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
        Some((self.tab_title(tab), tab.color, tab.marks))
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
        let marks = tab.marks;
        Some(TabInfo {
            id,
            window: tab.window,
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
            agent: marks.agent,
        })
    }

    /// Learns what it reads of every tab, as the plugin activates, of one
    /// that does not read the processes.
    fn learn_tabs(&mut self) {
        self.told_tabs.clear();
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
    }

    /// Owes what a tab's leaving did to its window.
    fn tell_left(&mut self, left: Left) {
        match left {
            Left::Kept => {}
            Left::Shows(window, tab) => self.changed(WorkspaceEvent::TabActivated { tab, window }),
            Left::Closed(window) => self.changed(WorkspaceEvent::WindowClosed { window }),
        }
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
        let left = self.detach_pane(pane, tab);
        self.panes.remove(&pane);
        if let Some(left) = left {
            if let Some(gone) = self.tabs.remove(&tab) {
                self.changed(WorkspaceEvent::TabClosed {
                    tab,
                    window: gone.window,
                });
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
            // Shown, it loses its activity and its bell, as the app's.
            let before = self.look(tab);
            if let Some(shown) = self.tabs.get_mut(&tab) {
                shown.marks.activity = false;
                shown.marks.bell = false;
            }
            self.looked(tab, &before);
        }
    }

    /// Gives `pane` its tab's keyboard.
    fn focus_pane(&mut self, pane: PaneId) {
        let Some(id) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if self.tabs.get(&id).is_some_and(|tab| tab.active != pane) {
            let before = self.look(id);
            if let Some(tab) = self.tabs.get_mut(&id) {
                tab.active = pane;
            }
            // It takes its new active pane's title, as the app's.
            self.looked(id, &before);
            self.changed(WorkspaceEvent::PaneFocused { pane, tab: id });
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
        let mut placed = 0;
        if let Some(shown) = self.windows.get_mut(&window) {
            placed = index.min(shown.tabs.len());
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

    /// Opens a window whose one tab, which it shows, is `tab`, and returns
    /// it.
    fn window_for(&mut self, tab: TabId) -> WindowId {
        let window = WindowId(self.next_id());
        self.windows.insert(
            window,
            FakeWindow {
                tabs: vec![tab],
                active: tab,
            },
        );
        self.place_tab(tab, window);
        self.changed(WorkspaceEvent::WindowOpened { window });
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
        self.tabs.insert(
            tab,
            FakeTab {
                window: WindowId(0),
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
                marks: TabMarks::default(),
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

    /// Has `pane` take `tab`'s whole room, or ends its zoom with `None`.
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
            ViewId::TabBar(window) | ViewId::WindowStrip(window) | ViewId::StatusBar(window) => {
                Some(*window).filter(|window| self.windows.contains_key(window))
            }
            ViewId::PaneHeader(pane) => self.pane_window(*pane).ok(),
            ViewId::Panel(id) => self.panel_windows.get(id).copied(),
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
    /// window, or its node's view's.
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

    /// The windows, each of its tabs and the one that it shows.
    fn windows(&self) -> Vec<WindowInfo> {
        self.windows
            .iter()
            .map(|(&id, window)| {
                let (number, keys) = self.numbers.get(&id).cloned().unwrap_or_default();
                WindowInfo {
                    id,
                    tabs: window.tabs.clone(),
                    active: window.active,
                    number,
                    keys,
                    bell: false,
                    quick: self.quick.contains_key(&id),
                    hidden: self.quick.get(&id) == Some(&true),
                }
            })
            .collect()
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
}

impl Calling<'_> {
    /// Whether the call's writes so far open the pane `pane`.
    fn opens_pane(&self, pane: PaneId) -> bool {
        self.writes.iter().any(|(_, opened)| match opened {
            Opened::Tab(_, opened) | Opened::Pane(opened) => *opened == pane,
            // A quick window's pane opens with the window, as the app opens
            // it after the call.
            Opened::Nothing | Opened::Quick(..) => false,
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

    /// Whether `pane` is held, or the call's writes so far open it;
    /// `not-found` otherwise.
    fn known_pane(&self, pane: PaneId) -> Result<(), Error> {
        if self.opens_pane(pane) {
            return Ok(());
        }
        self.host.held(pane).map(|_| ())
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

    /// Answers `request`, a read, with what the host holds.
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

    fn read(&mut self, request: &Request) -> Result<Response, Error> {
        if let Request::PaneTitle(pane) = request {
            return self.pane_title(*pane);
        }
        let host = &*self.host;
        Ok(match request {
            Request::ClipboardText => Response::Text(host.clipboard.clone()),
            Request::FocusedWindow => Response::Window(host.focused_window()),
            Request::FocusedPane => Response::Pane(host.focused_pane()),
            Request::SelectionText(pane) => Response::Text(host.held(*pane)?.selection.clone()),
            Request::LineText { pane, row, .. } => {
                Response::Line(host.held(*pane)?.lines.get(row).cloned())
            }
            Request::LinkAt { pane, column, row } => {
                Response::Text(host.held(*pane)?.links.get(&(*column, *row)).cloned())
            }
            Request::WorkingDirectory(pane) => Response::Text(host.held(*pane)?.directory.clone()),
            Request::Styles { pane, row } => {
                Response::Styles(host.held(*pane)?.styles.get(row).cloned())
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
            Request::PaneGrid(pane) => {
                let (columns, rows) = host.held(*pane)?.grid;
                Response::Grid { columns, rows }
            }
            Request::CommandLine { pane, command } => {
                Response::Text(host.held(*pane)?.command_lines.get(command).cloned())
            }
            Request::ShellCommands { pane, top, bottom } => {
                Response::ShellCommands(host.held(*pane)?.commands_meeting(*top, *bottom))
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
            | Request::SendText { pane, .. }
            | Request::Scroll { pane, .. }
            | Request::FocusPane(pane)
            | Request::ClosePane { pane, .. }
            | Request::PaneToNewWindow(pane) => {
                self.known_pane(*pane)?;
                Response::Done
            }
            Request::SetTabTitle { tab, .. } => {
                self.known_tab(*tab)?;
                Response::Done
            }
            Request::SetTabBarPlace(place) => {
                place
                    .check()
                    .map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::SetInactiveDim(dim) => {
                if !(0.0..=1.0).contains(dim) {
                    return Err(Error::new(Code::Failed, DIM_RANGE));
                }
                Response::Done
            }
            Request::SetFocusFollowsMouse(_) => Response::Done,
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
            Request::AskText { window, .. } => {
                if let Some(window) = window {
                    self.known_window(*window)?;
                }
                Response::Asking(self.host.next_id())
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
            Request::CloseTab { tab, .. }
            | Request::ActivateTab(tab)
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
            // A choice that the config keeps, of a theme that is there.
            Request::SetTheme { variant, name } => {
                self.host
                    .themes
                    .check(name, *variant)
                    .map_err(|message| Error::new(Code::NotFound, message))?;
                Response::Done
            }
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
            Request::Select { view, key, .. } => {
                if !matches!(self.host.node(view, key)?.kind, Kind::List(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a list", key.0),
                    ));
                }
                Response::Done
            }
            Request::FocusNode { view, key } => {
                self.host.node(view, key)?;
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
                let id = self.host.next_id();
                self.host.decorations.push(FakeDecoration {
                    id,
                    pane: *pane,
                    from: *from,
                    to: *to,
                    look: *look,
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
                if !self.host.menu_items.contains(&item) {
                    self.host.menu_items.push(item);
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
            && let Some(needed) = request
                .capabilities()
                .into_iter()
                .find(|needed| !crate::command::grants(granted, needed))
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
            Effect::Read => self.read(&request),
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

/// The schemes of the URLs that open, as the app's host has them.
const SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

/// The name of the machine that the fake host is, which a `file` URL's
/// host names, as the app's host takes its own.
pub const HOST_NAME: &str = "fake-mac";

/// Whether `target` opens, as the app's host takes it: a URL of one of
/// [`SCHEMES`], a `file` URL of this machine alone, or a path from `/` or
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
    match target.split_once(':') {
        Some((scheme, _)) if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) => Ok(()),
        Some((scheme, _)) => Err(format!(
            "`{scheme}:` does not open: only {}",
            SCHEMES.join(", ")
        )),
        None => Err(format!(
            "`{target}` is neither a URL nor a path that is absolute or from `~`"
        )),
    }
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
        Kind::Spacer => out.push_str("spacer"),
        Kind::Field(field) => {
            let _ = write!(out, "field {}", quoted(&field.placeholder));
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
        Kind::Separator => out.push_str("separator"),
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
/// `[capabilities]`, and the commands of its `[uses]`, if it has them, as
/// [`FakeHost::with_manifest`] reads them.
fn declared(text: &str) -> (Vec<Capability>, Option<Vec<String>>) {
    let (mut capabilities, mut uses) = (Vec::new(), None);
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
                "capabilities" => "capabilities",
                "uses" => "uses",
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
            _ => {}
        }
    }
    (capabilities, uses)
}

#[cfg(test)]
mod tests;

//! The windows that were open as the app quit, which it opens again as it
//! starts: their frames, their tabs, each tab's title and color of its own,
//! its splits and their ratios, the pane that has its keyboard and the one
//! zoomed, and each pane's directory and the file of its scrollback, which
//! [`super::scrollback`] keeps; the groups of their tabs; and the sessions
//! that they are of, in the order of their use, the active one first.
//! Their programs start anew, of the app's program.
//!
//! A file of its own in the state's directory, of JSON, of a format's
//! version, which a file of a newer version, as a later Lokyr writes, or
//! that does not read, is left aside of: the app starts as it would
//! without one, and writes its own as it quits.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lokyr_vt_core::page::Rgb;
use serde::{Deserialize, Serialize};

use super::write_whole;
use crate::workspace::{self, Pane, Ratio, Side, Workspace};

/// The file of the windows, in the state's directory.
const FILE: &str = "windows.json";

/// The version of the file's format, which a later one that it cannot
/// read raises; fields that it does not know are read past, as the groups
/// of tabs, which an earlier Lokyr opens ungrouped.
pub const VERSION: u32 = 1;

/// The most panes that a file opens, of all its windows, past which it is
/// left aside, as no one of them is the user's: each starts a program.
pub const MOST_PANES: usize = 256;

/// The most windows that a file opens, past which it is left aside, as a
/// window of no tab, which opens nothing, counts in no pane.
pub const MOST_WINDOWS: usize = 100;

/// The windows that the app keeps, as it quits, to open again.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Kept {
    /// The format's version, [`VERSION`].
    pub version: u32,
    /// The windows, in the order of their numbers.
    pub windows: Vec<Window>,
    /// The index of the window in front, which takes the focus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<usize>,
    /// The sessions of the windows, in the order of their use, the active
    /// one first, which each window names by its index; none of the one
    /// session of no name nor folder, which an earlier Lokyr reads past,
    /// opening every window in one session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<Session>,
    /// The windows and the panes left out, past [`MOST_WINDOWS`] and
    /// [`MOST_PANES`], which the next start tells.
    #[serde(default, skip_serializing_if = "Left::is_none")]
    pub left: Left,
    /// The session of each window, as the workspace has it, which the
    /// windows appended after take theirs beside.
    #[serde(skip)]
    of_sessions: Vec<Option<workspace::SessionId>>,
}

impl PartialEq for Kept {
    /// The same windows, front and sessions, as the file holds them.
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
            && self.windows == other.windows
            && self.front == other.front
            && self.sessions == other.sessions
            && self.left == other.left
    }
}

/// The windows and the panes that were not kept, past the most that a file
/// opens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Left {
    /// The windows left out.
    pub windows: usize,
    /// The panes of the windows left out.
    pub panes: usize,
}

impl Left {
    /// Whether nothing was left out.
    pub fn is_none(&self) -> bool {
        self.windows == 0
    }
}

/// A session of windows that the app keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Its name, empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The folder that it was opened in, if it was opened in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// A window that the app keeps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    /// Its content's frame on the screens, the titlebar's room too, in
    /// points from the top left corner of the menu bar's screen; out of
    /// full screen, as it was before, for a window in full screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
    /// Its tabs, in their order.
    pub tabs: Vec<Tab>,
    /// The index of the tab that it shows.
    #[serde(default)]
    pub active: usize,
    /// The groups of its tabs, in the order of their tabs, which each tab
    /// names by its index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
    /// The index of its session among the kept ones; none of the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<usize>,
}

/// A group of a window's tabs that the app keeps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// Its name, empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Its color, as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Whether it is folded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folded: bool,
}

/// A rectangle on the screens, in points: its top left corner, from the top
/// left corner of the menu bar's screen, rightwards and downwards, and its
/// size.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    /// From the left, rightwards.
    pub x: f64,
    /// From the top, downwards.
    pub y: f64,
    /// Its width.
    pub width: f64,
    /// Its height.
    pub height: f64,
}

/// A tab that the app keeps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    /// The title of its own, as the user or a plugin gave it, in place of
    /// its program's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The color of its own, as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The layout of its panes.
    pub layout: Layout,
    /// The index, among its panes in the layout's order, of the one that
    /// has the keyboard.
    #[serde(default)]
    pub active: usize,
    /// The index of the one that takes the tab's whole room, if one does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoomed: Option<usize>,
    /// Whether it is pinned, at its window's start.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// The index of its group among its window's, if it is in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<usize>,
    /// Its panes that float over its layout, from the back to the front,
    /// which an earlier Lokyr reads past.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floating: Vec<Floating>,
    /// Whether its floating panes show.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub floating_shown: bool,
}

/// Floating panes show, unless a tab keeps them hidden.
fn shown() -> bool {
    true
}

/// Whether floating panes show, which a file writes not.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's `skip_serializing_if` passes a reference"
)]
fn is_shown(shown: &bool) -> bool {
    *shown
}

/// A pane that floats over its tab's layout, that the app keeps: the pane,
/// as a layout of it alone, and its frame's corner and size, shares of the
/// tab's room.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Floating {
    /// The pane.
    pub pane: Layout,
    /// Its corner's share across.
    pub x: f64,
    /// Its corner's share down.
    pub y: f64,
    /// Its share across.
    pub width: f64,
    /// Its share down.
    pub height: f64,
}

impl Tab {
    /// The number of its panes, its layout's and its floating ones.
    pub fn panes(&self) -> usize {
        self.layout.panes() + self.floating.len()
    }
}

/// A tab's layout that the app keeps: a tree of splits whose leaves are
/// panes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// A pane, of the directory that its program started in.
    Pane {
        /// The directory that its program worked in; none for the home
        /// directory, where the app starts it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        directory: Option<String>,
        /// The name of the file of its scrollback in the state's folder of
        /// the panes' scrollback, if it kept one, which an earlier Lokyr
        /// reads past.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scrollback: Option<String>,
        /// The command line that its shell runs as it starts, of a layout's
        /// pane, which the file of the windows neither writes nor reads, so
        /// that no command comes of it.
        #[serde(skip)]
        command: Option<String>,
    },
    /// Two parts that share the room.
    Split {
        /// How the parts lie.
        direction: Direction,
        /// The share of the first part, from 0 to 1, as the user gave it;
        /// none to share the room by the parts' panes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ratio: Option<f64>,
        /// The left or top part.
        first: Box<Layout>,
        /// The right or bottom part.
        second: Box<Layout>,
    },
}

/// A pane of a kept layout, as its spawn takes it: the directory that its
/// program worked in, and the name of its scrollback's file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeptPane<'a> {
    /// The directory that its program worked in; none for the home
    /// directory.
    pub directory: Option<&'a Path>,
    /// The name of its scrollback's file in the state's folder of the
    /// panes' scrollback, if it kept one.
    pub scrollback: Option<&'a str>,
    /// The command line that its shell runs as it starts, of a layout's
    /// pane.
    pub command: Option<&'a str>,
}

/// How a kept split lays its two parts out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// Side by side.
    Horizontal,
    /// One over the other.
    Vertical,
}

/// Why the file of the windows is left aside.
#[derive(Debug)]
pub enum Unread {
    /// It is of a newer format's version, which a later Lokyr wrote.
    Newer(u64),
    /// It does not read, or holds more than [`MOST_PANES`] panes or
    /// [`MOST_WINDOWS`] windows.
    Invalid(String),
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Newer(version) => write!(
                f,
                "it is of the format's version {version}, of a later Lokyr, which this one, of {VERSION}, cannot read"
            ),
            Self::Invalid(error) => write!(f, "it does not read: {error}"),
        }
    }
}

/// The windows that `state` keeps, if it keeps any; or why its file is
/// left aside.
pub fn read(state: &Path) -> Result<Option<Kept>, Unread> {
    let text = match fs::read_to_string(state.join(FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Unread::Invalid(error.to_string())),
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| Unread::Invalid(error.to_string()))?;
    // Its version first, as a newer format's other fields may not read.
    let version = value.get("version").and_then(serde_json::Value::as_u64);
    match version {
        Some(version) if version > u64::from(VERSION) => return Err(Unread::Newer(version)),
        Some(_) => {}
        None => return Err(Unread::Invalid("it has no version".to_owned())),
    }
    let kept: Kept =
        serde_json::from_value(value).map_err(|error| Unread::Invalid(error.to_string()))?;
    let panes: usize = kept
        .windows
        .iter()
        .flat_map(|window| &window.tabs)
        .map(Tab::panes)
        .sum();
    if kept.windows.len() > MOST_WINDOWS {
        return Err(Unread::Invalid(format!(
            "it holds {} windows, past the {MOST_WINDOWS} that it may open",
            kept.windows.len()
        )));
    }
    if panes > MOST_PANES {
        return Err(Unread::Invalid(format!(
            "it holds {panes} panes, past the {MOST_PANES} that it may open"
        )));
    }
    // Each session of a window at least.
    if kept.sessions.len() > MOST_WINDOWS {
        return Err(Unread::Invalid(format!(
            "it holds {} sessions, past the {MOST_WINDOWS} windows that it may open",
            kept.sessions.len()
        )));
    }
    Ok(Some(kept))
}

/// Keeps `kept` in `state`, which it makes, in place of the windows that it
/// kept: a file written whole.
pub fn write(state: &Path, kept: &Kept) -> io::Result<()> {
    let text = serde_json::to_string_pretty(kept).map_err(io::Error::other)?;
    write_whole(&state.join(FILE), text.as_bytes())
}

/// Forgets the windows that `state` keeps, if it keeps any.
pub fn forget(state: &Path) -> io::Result<()> {
    match fs::remove_file(state.join(FILE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Of `shown`, the windows that a file keeps within [`MOST_WINDOWS`] and
/// [`MOST_PANES`], in their order, and what is left out: the active
/// session's first, then the others' in the order of their use, each
/// session's in their order, a window whose panes are past the room left
/// giving way to the next ones that fit. Each window counts the panes of
/// its tabs that are kept, the settings tab of none.
fn within_bounds(
    workspace: &Workspace,
    shown: &[(workspace::WindowId, Option<Frame>)],
) -> (Vec<(workspace::WindowId, Option<Frame>)>, Left) {
    let order: Vec<workspace::SessionId> = workspace
        .sessions()
        .iter()
        .map(|session| session.id())
        .collect();
    let mut ranked: Vec<(usize, usize, usize)> = shown
        .iter()
        .enumerate()
        .filter_map(|(at, &(id, _))| {
            let window = workspace.window(id)?;
            let rank = window
                .session()
                .and_then(|session| order.iter().position(|&of| of == session))
                .unwrap_or(usize::MAX);
            let panes = window
                .tabs()
                .iter()
                .filter_map(|&tab| workspace.tab(tab))
                .map(|tab| tab.panes().len())
                .sum();
            Some((rank, at, panes))
        })
        .collect();
    ranked.sort_unstable();
    let (mut chosen, mut panes, mut left) = (Vec::new(), 0, Left::default());
    for (_, at, more) in ranked {
        if chosen.len() >= MOST_WINDOWS || panes + more > MOST_PANES {
            left.windows += 1;
            left.panes += more;
            continue;
        }
        panes += more;
        chosen.push(at);
    }
    chosen.sort_unstable();
    (chosen.into_iter().map(|at| shown[at]).collect(), left)
}

impl Kept {
    /// The windows `shown` of `workspace`, in their order, each of its
    /// frame, of `front` the one in front, and of each pane's directory,
    /// as `directory` finds it, and the name of its scrollback's file, as
    /// `scrollback` keeps it, of the pane and its index among the panes
    /// kept, in their order. A window that the workspace does not hold is
    /// left out, and so are the windows past [`MOST_WINDOWS`] and
    /// [`MOST_PANES`], which the next start would not open: the active
    /// session's kept first, then the others' in the order of their use,
    /// each session's in their order.
    pub fn of(
        workspace: &Workspace,
        shown: &[(workspace::WindowId, Option<Frame>)],
        front: Option<workspace::WindowId>,
        directory: impl Fn(&Pane) -> Option<PathBuf>,
        mut scrollback: impl FnMut(&Pane, usize) -> Option<String>,
    ) -> Self {
        let mut index = 0;
        let mut scrollback = |pane: &Pane| {
            index += 1;
            scrollback(pane, index - 1)
        };
        let mut windows = Vec::new();
        let mut of_sessions: Vec<Option<workspace::SessionId>> = Vec::new();
        let mut front_index = None;
        let (shown, left) = within_bounds(workspace, shown);
        for (id, frame) in shown {
            let Some(window) = workspace.window(id) else {
                continue;
            };
            let groups: Vec<&workspace::Group> = workspace.groups_of(id);
            let tabs: Vec<Tab> = window
                .tabs()
                .iter()
                .filter_map(|&tab| workspace.tab(tab))
                .map(|tab| Tab {
                    group: tab
                        .group()
                        .and_then(|group| groups.iter().position(|kept| kept.id() == group)),
                    ..Tab::of(workspace, tab, &directory, &mut scrollback)
                })
                .collect();
            if tabs.is_empty() {
                continue;
            }
            if front == Some(id) {
                front_index = Some(windows.len());
            }
            // Of the tabs kept, the settings tab not among them: the one
            // that the window shows, or, as it shows the settings tab, the
            // one before it.
            let kept = window
                .tabs()
                .iter()
                .take_while(|&&tab| tab != window.active_tab())
                .filter(|&&tab| workspace.tab(tab).is_some())
                .count();
            let active = if workspace.tab(window.active_tab()).is_some() {
                kept
            } else {
                kept.saturating_sub(1)
            };
            let groups = groups
                .iter()
                .map(|group| Group {
                    name: group.name().to_owned(),
                    color: group.color().map(hex),
                    folded: group.folded(),
                })
                .collect();
            windows.push(Window {
                frame,
                tabs,
                active,
                groups,
                session: None,
            });
            of_sessions.push(window.session());
        }
        let mut kept = Self {
            version: VERSION,
            windows,
            front: front_index,
            sessions: Vec::new(),
            left,
            of_sessions,
        };
        kept.keep_sessions(workspace);
        kept
    }

    /// The windows `windows`, of `front` the one in front, of their one
    /// session, of the format's version.
    pub fn new(windows: Vec<Window>, front: Option<usize>) -> Self {
        Self {
            version: VERSION,
            windows,
            front,
            sessions: Vec::new(),
            left: Left::default(),
            of_sessions: Vec::new(),
        }
    }

    /// Appends `windows`, each of its session, which the workspace holds of
    /// no window yet, as a session that never showed since the app
    /// started keeps its windows as they were kept, after the others, in
    /// their order, while they fit within [`MOST_WINDOWS`] and
    /// [`MOST_PANES`]; each appended as `kept` takes it first, as the names
    /// of its panes' files are its new place's.
    pub fn append(
        &mut self,
        workspace: &Workspace,
        windows: Vec<(workspace::SessionId, Window)>,
        mut kept: impl FnMut(&mut Window),
    ) {
        let mut panes = self.panes();
        for (session, mut window) in windows {
            let more = window.tabs.iter().map(Tab::panes).sum::<usize>();
            if self.windows.len() >= MOST_WINDOWS || panes + more > MOST_PANES {
                self.left.windows += 1;
                self.left.panes += more;
                continue;
            }
            panes += more;
            kept(&mut window);
            self.windows.push(window);
            self.of_sessions.push(Some(session));
        }
        self.keep_sessions(workspace);
    }

    /// The panes that its windows hold, of all their tabs.
    pub fn panes(&self) -> usize {
        self.windows
            .iter()
            .flat_map(|window| &window.tabs)
            .map(Tab::panes)
            .sum()
    }

    /// Names each window's session, of `of_sessions`, each window's in
    /// their order, among the sessions that the windows are of, in the
    /// order of their use, the active one first; none of the one session
    /// of no name nor folder, as a run of no sessions of its own keeps.
    fn keep_sessions(&mut self, workspace: &Workspace) {
        let of_sessions = &self.of_sessions;
        let used: Vec<&workspace::Session> = workspace
            .sessions()
            .into_iter()
            .filter(|session| of_sessions.contains(&Some(session.id())))
            .collect();
        let plain =
            |session: &workspace::Session| session.name().is_empty() && session.folder().is_none();
        if used.len() < 2 && used.iter().all(|session| plain(session)) {
            self.sessions.clear();
            for window in &mut self.windows {
                window.session = None;
            }
            return;
        }
        self.sessions = used
            .iter()
            .map(|session| Session {
                name: session.name().to_owned(),
                folder: session.folder().map(str::to_owned),
            })
            .collect();
        for (window, of) in self.windows.iter_mut().zip(&self.of_sessions) {
            window.session = of
                .and_then(|of| used.iter().position(|session| session.id() == of))
                .filter(|&index| index > 0);
        }
    }

    /// The sessions as they open: the kept ones, the first one's of none
    /// kept, each name that the file holds as no plugin could write it
    /// emptied.
    pub fn sessions_to_open(&self) -> Vec<Session> {
        let mut sessions = self.sessions.clone();
        if sessions.is_empty() {
            sessions.push(Session::default());
        }
        for session in &mut sessions {
            if lokyr_plugin_sdk::host::check_session_name(&session.name).is_err() {
                session.name.clear();
            }
        }
        sessions
    }
}

impl Window {
    /// The index of its session among those that open, of `count`: the
    /// first's of none, or of an index past them.
    pub fn session_index(&self, count: usize) -> usize {
        self.session.filter(|&index| index < count).unwrap_or(0)
    }

    /// Gives each of its panes the name of its scrollback's file that
    /// `rename` gives of the one that it had, in the layouts' order, as a
    /// window kept anew takes the files' new names.
    pub fn rename_scrollbacks(&mut self, mut rename: impl FnMut(Option<String>) -> Option<String>) {
        for tab in &mut self.tabs {
            tab.layout.rename_scrollbacks(&mut rename);
            for floating in &mut tab.floating {
                floating.pane.rename_scrollbacks(&mut rename);
            }
        }
    }
}

impl Window {
    /// Opens the window in `workspace`, of its tabs, the one that it showed
    /// shown, each pane of `spawn`, of the id that the workspace gives it,
    /// of the directory and the scrollback that it kept; `None` for a
    /// window of no tab.
    pub fn open(
        &self,
        workspace: &mut Workspace,
        mut spawn: impl FnMut(KeptPane<'_>) -> Pane,
    ) -> Option<workspace::WindowId> {
        let (first, rest) = self.tabs.split_first()?;
        let window = workspace.open_window(spawn(first.layout.first_pane()));
        let tab = workspace.window(window)?.active_tab();
        first.lay_out(workspace, tab, &mut spawn);
        let mut tabs = vec![(tab, first.group)];
        for (index, kept) in rest.iter().enumerate() {
            let pane = spawn(kept.layout.first_pane());
            if let Some(tab) = workspace.open_tab(window, pane, index + 1) {
                kept.lay_out(workspace, tab, &mut spawn);
                tabs.push((tab, kept.group));
            }
        }
        // The groups of the tabs that name them, folded as they were, before
        // the tab shown, which unfolds its own.
        for (index, group) in self.groups.iter().enumerate() {
            let members: Vec<workspace::TabId> = tabs
                .iter()
                .filter(|(_, of)| *of == Some(index))
                .map(|&(tab, _)| tab)
                .collect();
            let color = group.color.as_deref().and_then(rgb);
            // A name that the file holds as no plugin could write it goes.
            let name = if lokyr_plugin_sdk::host::check_group(&group.name, None).is_ok() {
                group.name.as_str()
            } else {
                ""
            };
            if let Some(made) = workspace.new_group(&members, name, color)
                && group.folded
            {
                workspace.fold_group(made, true);
            }
        }
        if let Some(&(active, _)) = tabs.get(self.active) {
            workspace.activate_tab(active);
        }
        Some(window)
    }
}

impl Tab {
    /// The tab `tab` of `workspace`, of each pane's directory as
    /// `directory` finds it.
    fn of(
        workspace: &Workspace,
        tab: &workspace::Tab,
        directory: &impl Fn(&Pane) -> Option<PathBuf>,
        scrollback: &mut impl FnMut(&Pane) -> Option<String>,
    ) -> Self {
        // Its layout's panes, then its floating ones, as the keyboard's
        // index counts them and their files are named.
        let panes = tab.panes();
        let index = |pane| panes.iter().position(|&id| id == pane);
        let layout = Layout::of(workspace, tab.layout(), directory, scrollback);
        let floating = tab
            .floating()
            .iter()
            .map(|floating| Floating {
                pane: Layout::of(
                    workspace,
                    &workspace::Layout::Leaf(floating.pane),
                    directory,
                    scrollback,
                ),
                x: floating.frame.x(),
                y: floating.frame.y(),
                width: floating.frame.width(),
                height: floating.frame.height(),
            })
            .collect();
        Self {
            title: tab.own_title().map(str::to_owned),
            color: tab.own_color().map(hex),
            layout,
            active: index(tab.active_pane()).unwrap_or(0),
            zoomed: tab.zoomed().and_then(index),
            pinned: tab.marks().pinned,
            group: None,
            floating,
            floating_shown: tab.floating_shown(),
        }
    }

    /// Lays `tab`, a tab of `workspace` whose one pane is its layout's
    /// first, out as it is kept: its splits, of the other panes, of
    /// `spawn`, their ratios, the pane that has the keyboard and the one
    /// zoomed, its title, its color and its pin.
    fn lay_out(
        &self,
        workspace: &mut Workspace,
        tab: workspace::TabId,
        spawn: &mut impl FnMut(KeptPane<'_>) -> Pane,
    ) {
        let Some(first) = workspace.tab(tab).map(workspace::Tab::active_pane) else {
            return;
        };
        self.layout
            .grow(workspace, tab, first, &mut Vec::new(), spawn);
        // Its floating panes, over the layout, from the back to the front,
        // shown or hidden as they were.
        for floating in &self.floating {
            let pane = spawn(floating.pane.first_pane());
            let frame = workspace::FloatingFrame::new(
                floating.x,
                floating.y,
                floating.width,
                floating.height,
            );
            workspace.float_pane(tab, pane, frame);
        }
        if !self.floating.is_empty() {
            workspace.set_floating_shown(tab, self.floating_shown);
        }
        let panes = workspace
            .tab(tab)
            .map(workspace::Tab::panes)
            .unwrap_or_default();
        if let Some(&active) = panes.get(self.active) {
            workspace.focus_pane(active);
        }
        if let Some(&zoomed) = self.zoomed.and_then(|index| panes.get(index)) {
            workspace.zoom(tab, Some(zoomed));
        }
        if let Some(title) = &self.title {
            workspace.set_tab_title(tab, Some(title.clone()));
        }
        if let Some(color) = self.color.as_deref().and_then(rgb) {
            workspace.set_tab_color(tab, Some(color));
        }
        if self.pinned {
            workspace.set_tab_pinned(tab, true);
        }
    }
}

impl Layout {
    /// The layout `layout` of `workspace`, of each pane's directory as
    /// `directory` finds it and its scrollback's file as `scrollback`
    /// keeps it.
    fn of(
        workspace: &Workspace,
        layout: &workspace::Layout,
        directory: &impl Fn(&Pane) -> Option<PathBuf>,
        scrollback: &mut impl FnMut(&Pane) -> Option<String>,
    ) -> Self {
        match layout {
            workspace::Layout::Leaf(pane) => Self::Pane {
                // A path that is not text, as a path's bytes may be on
                // Unix, is kept of its characters that are; one that is
                // then not a directory starts in the home directory.
                directory: workspace
                    .pane(*pane)
                    .and_then(directory)
                    .map(|path| path.to_string_lossy().into_owned()),
                scrollback: workspace.pane(*pane).and_then(&mut *scrollback),
                command: None,
            },
            workspace::Layout::Split {
                direction,
                ratio,
                first,
                second,
            } => Self::Split {
                direction: match direction {
                    workspace::Direction::Horizontal => Direction::Horizontal,
                    workspace::Direction::Vertical => Direction::Vertical,
                },
                ratio: ratio.map(Ratio::get),
                first: Box::new(Self::of(workspace, first, directory, scrollback)),
                second: Box::new(Self::of(workspace, second, directory, scrollback)),
            },
        }
    }

    /// Gives each of its panes, in their order, the scrollback's file's
    /// name that `rename` gives of the one that it had.
    fn rename_scrollbacks(&mut self, rename: &mut impl FnMut(Option<String>) -> Option<String>) {
        match self {
            Self::Pane { scrollback, .. } => *scrollback = rename(scrollback.take()),
            Self::Split { first, second, .. } => {
                first.rename_scrollbacks(rename);
                second.rename_scrollbacks(rename);
            }
        }
    }

    /// The number of its panes.
    pub fn panes(&self) -> usize {
        match self {
            Self::Pane { .. } => 1,
            Self::Split { first, second, .. } => first.panes() + second.panes(),
        }
    }

    /// Its first pane, the left or top one.
    pub fn first_pane(&self) -> KeptPane<'_> {
        match self {
            Self::Pane {
                directory,
                scrollback,
                command,
            } => KeptPane {
                directory: directory.as_deref().map(Path::new),
                scrollback: scrollback.as_deref(),
                command: command.as_deref(),
            },
            Self::Split { first, .. } => first.first_pane(),
        }
    }

    /// Grows the leaf of `pane`, at `path` of `tab`'s layout, which is
    /// this layout's first pane, into this layout: each split splits the
    /// leaf of its first part's first pane, of its second part's first
    /// pane, of `spawn`, and takes its ratio.
    fn grow(
        &self,
        workspace: &mut Workspace,
        tab: workspace::TabId,
        pane: workspace::PaneId,
        path: &mut Vec<Side>,
        spawn: &mut impl FnMut(KeptPane<'_>) -> Pane,
    ) {
        let Self::Split {
            direction,
            ratio,
            first,
            second,
        } = self
        else {
            return;
        };
        let direction = match direction {
            Direction::Horizontal => workspace::Direction::Horizontal,
            Direction::Vertical => workspace::Direction::Vertical,
        };
        let new = spawn(second.first_pane());
        let Some(new) = workspace.split(pane, direction, new, Side::Second) else {
            return;
        };
        if let Some(ratio) = ratio.filter(|ratio| ratio.is_finite()) {
            workspace.set_ratio(tab, path, Some(Ratio::new(ratio)));
        }
        path.push(Side::First);
        first.grow(workspace, tab, pane, path, spawn);
        path.pop();
        path.push(Side::Second);
        second.grow(workspace, tab, new, path, spawn);
        path.pop();
    }
}

/// `color` as `#rrggbb`.
fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// The color of `text`, `#rrggbb`; `None` for any other text.
fn rgb(text: &str) -> Option<Rgb> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let part = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some(Rgb {
        r: part(0)?,
        g: part(2)?,
        b: part(4)?,
    })
}

#[cfg(test)]
mod tests;

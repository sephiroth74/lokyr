//! The windows that were open as the app quit, which it opens again as it
//! starts: their frames, their tabs, each tab's title and color of its own,
//! its splits and their ratios, the pane that has its keyboard and the one
//! zoomed, and each pane's directory. Their programs start anew, of the
//! app's program; what the panes held is not kept.
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
/// read raises.
pub const VERSION: u32 = 1;

/// The most panes that a file opens, of all its windows, past which it is
/// left aside, as no one of them is the user's: each starts a program.
pub const MOST_PANES: usize = 256;

/// The most windows that a file opens, past which it is left aside, as a
/// window of no tab, which opens nothing, counts in no pane.
pub const MOST_WINDOWS: usize = 100;

/// The windows that the app keeps, as it quits, to open again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    /// The format's version, [`VERSION`].
    pub version: u32,
    /// The windows, in the order of their numbers.
    pub windows: Vec<Window>,
    /// The index of the window in front, which takes the focus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<usize>,
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
        .map(|tab| tab.layout.panes())
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

impl Kept {
    /// The windows `shown` of `workspace`, in their order, each of its
    /// frame, of `front` the one in front, and of each pane's directory,
    /// as `directory` finds it. A window that the workspace does not hold
    /// is left out.
    pub fn of(
        workspace: &Workspace,
        shown: &[(workspace::WindowId, Option<Frame>)],
        front: Option<workspace::WindowId>,
        directory: impl Fn(&Pane) -> Option<PathBuf>,
    ) -> Self {
        let mut windows = Vec::new();
        let mut front_index = None;
        for &(id, frame) in shown {
            let Some(window) = workspace.window(id) else {
                continue;
            };
            let tabs: Vec<Tab> = window
                .tabs()
                .iter()
                .filter_map(|&tab| workspace.tab(tab))
                .map(|tab| Tab::of(workspace, tab, &directory))
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
            windows.push(Window {
                frame,
                tabs,
                active,
            });
        }
        Self {
            version: VERSION,
            windows,
            front: front_index,
        }
    }
}

impl Window {
    /// Opens the window in `workspace`, of its tabs, the one that it showed
    /// shown, each pane of `spawn`, of the id that the workspace gives it,
    /// in the directory that it kept; `None` for a window of no tab.
    pub fn open(
        &self,
        workspace: &mut Workspace,
        mut spawn: impl FnMut(Option<&Path>) -> Pane,
    ) -> Option<workspace::WindowId> {
        let (first, rest) = self.tabs.split_first()?;
        let window = workspace.open_window(spawn(first.layout.first_directory()));
        let tab = workspace.window(window)?.active_tab();
        first.lay_out(workspace, tab, &mut spawn);
        let mut tabs = vec![tab];
        for (index, kept) in rest.iter().enumerate() {
            let pane = spawn(kept.layout.first_directory());
            if let Some(tab) = workspace.open_tab(window, pane, index + 1) {
                kept.lay_out(workspace, tab, &mut spawn);
                tabs.push(tab);
            }
        }
        if let Some(&active) = tabs.get(self.active) {
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
    ) -> Self {
        let panes = tab.layout().panes();
        let index = |pane| panes.iter().position(|&id| id == pane);
        Self {
            title: tab.own_title().map(str::to_owned),
            color: tab.own_color().map(hex),
            layout: Layout::of(workspace, tab.layout(), directory),
            active: index(tab.active_pane()).unwrap_or(0),
            zoomed: tab.zoomed().and_then(index),
        }
    }

    /// Lays `tab`, a tab of `workspace` whose one pane is its layout's
    /// first, out as it is kept: its splits, of the other panes, of
    /// `spawn`, their ratios, the pane that has the keyboard and the one
    /// zoomed, its title and its color.
    fn lay_out(
        &self,
        workspace: &mut Workspace,
        tab: workspace::TabId,
        spawn: &mut impl FnMut(Option<&Path>) -> Pane,
    ) {
        let Some(first) = workspace.tab(tab).map(workspace::Tab::active_pane) else {
            return;
        };
        self.layout
            .grow(workspace, tab, first, &mut Vec::new(), spawn);
        let panes = workspace
            .tab(tab)
            .map(|tab| tab.layout().panes())
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
    }
}

impl Layout {
    /// The layout `layout` of `workspace`, of each pane's directory as
    /// `directory` finds it.
    fn of(
        workspace: &Workspace,
        layout: &workspace::Layout,
        directory: &impl Fn(&Pane) -> Option<PathBuf>,
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
                first: Box::new(Self::of(workspace, first, directory)),
                second: Box::new(Self::of(workspace, second, directory)),
            },
        }
    }

    /// The number of its panes.
    pub fn panes(&self) -> usize {
        match self {
            Self::Pane { .. } => 1,
            Self::Split { first, second, .. } => first.panes() + second.panes(),
        }
    }

    /// The directory of its first pane, the left or top one.
    pub fn first_directory(&self) -> Option<&Path> {
        match self {
            Self::Pane { directory } => directory.as_deref().map(Path::new),
            Self::Split { first, .. } => first.first_directory(),
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
        spawn: &mut impl FnMut(Option<&Path>) -> Pane,
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
        let new = spawn(second.first_directory());
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

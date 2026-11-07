//! The host's side: what carries a plugin's calls to the host, the
//! handles of the windows, the tabs and the panes that it reaches, and the
//! types that the requests carry.
//!
//! Only data crosses, never references into the host's state. What
//! changes, the kernel's state or the app, comes once the plugin's call
//! returns, as the host never calls into a plugin that is running.

use serde::{Deserialize, Serialize};

use crate::command::Args;
use crate::message::{Call, Response};

/// A window, by the host's id, which no other window, tab or pane of the
/// process has had.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

/// A tab, by the host's id, likewise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

/// A pane, by the host's id, likewise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u64);

/// A point on the screens, in points, from the top left corner of the menu
/// bar's screen, rightwards and downwards, as the screens lie on one plane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScreenPoint {
    /// From the left, rightwards.
    pub x: f64,
    /// From the top, downwards.
    pub y: f64,
}

/// A window, as a read of the windows gives it: its tabs, in their order,
/// the one that it shows, and its number with the keys that select it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// The window.
    pub id: WindowId,
    /// Its tabs, from the first.
    pub tabs: Vec<TabId>,
    /// The tab that it shows.
    pub active: TabId,
    /// Its number, from 1, the lowest that no other open window had as it
    /// opened; `None` for a window of none, as the quick terminal's.
    #[serde(default)]
    pub number: Option<u32>,
    /// The keys that bring it forward, as the platform writes them, as
    /// `⌥⌘1`, for its titlebar to show, a bar of tabs there too; `None`
    /// while no binding has them, or while the config hides them from the
    /// titlebar.
    #[serde(default)]
    pub keys: Option<String>,
    /// Whether its bell rang while it had not the focus, until it takes
    /// it, as `[bell] mark-title` marks its title: of a window whose title
    /// a plugin draws, as a single tab's, to draw.
    #[serde(default)]
    pub bell: bool,
}

/// A tab, as a read of a window's tabs gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabInfo {
    /// The tab.
    pub id: TabId,
    /// Its window.
    pub window: WindowId,
    /// Its title: its own, as a plugin gave it, or its active pane's
    /// program's, or, to a plugin of `process-read`, its active pane's
    /// folder and program in the foreground, as `src — vim`; empty for
    /// none.
    pub title: String,
    /// Its color, its own as a plugin gave it, or its active pane's
    /// program's, an RGB one; `None` for none.
    #[serde(default)]
    pub color: Option<Color>,
    /// Whether its window shows it.
    pub active: bool,
    /// Its panes, in its layout's order: a split's first part before its
    /// second.
    pub panes: Vec<PaneId>,
    /// The pane that has its keyboard.
    pub focused: PaneId,
    /// The pane that takes its whole room, if one does.
    pub zoomed: Option<PaneId>,
    /// Whether output came in it since its window last showed it, while it
    /// did not show.
    #[serde(default)]
    pub activity: bool,
    /// Whether its bell rang since its window last showed it, while it did
    /// not show.
    #[serde(default)]
    pub bell: bool,
    /// The progress that a program of its panes tells, OSC 9;4's: its
    /// active pane's, or another's.
    #[serde(default)]
    pub progress: Option<TabProgress>,
    /// Whether the program of its active pane, other than a shell, as
    /// `running` tells, prints of its own: output that came within the last
    /// few seconds, apart from the echo of the user's input and the redraw
    /// of a new size, as a build's or a download's, rather than ssh, a REPL
    /// or an agent waiting for the user; to a plugin of `process-read`,
    /// `false` to the others.
    #[serde(default)]
    pub printing: bool,
    /// Whether a program other than a shell runs in its active pane's
    /// foreground, to a plugin of `process-read`; `false` to the others.
    #[serde(default)]
    pub running: bool,
    /// Whether the program of its active pane shows the alternate screen,
    /// as a full-screen one, vim or htop, does, to a plugin of
    /// `process-read`; `false` to the others.
    #[serde(default)]
    pub alternate: bool,
    /// Whether the keyboard's protected input is on for it, as Secure
    /// Keyboard Entry: while its window has the focus and shows it.
    #[serde(default)]
    pub secure: bool,
    /// The state of an AI agent that runs in it, if one does.
    #[serde(default)]
    pub agent: Option<AgentState>,
}

/// The progress of a task that a program tells, as OSC 9;4 does.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabProgress {
    /// Its state.
    pub state: crate::view::ProgressState,
    /// How far along, from 0 to 1, if it tells: none of an indeterminate
    /// one, nor of a failed or a paused one that tells no value.
    #[serde(default)]
    pub value: Option<f32>,
}

/// The state of an AI agent that runs in a tab, as its notifications tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentState {
    /// At work.
    Working,
    /// Waiting for the user.
    Waiting,
    /// Done with what it was asked.
    Done,
}

/// How a split lays its two parts out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// Side by side: the first part on the left, the second on the right.
    Horizontal,
    /// One over the other: the first part above, the second below.
    Vertical,
}

/// A part of a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// The first: the left one, or the top one.
    First,
    /// The second: the right one, or the bottom one.
    Second,
}

/// An edge of a pane, which a resize moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    /// Its left edge.
    Left,
    /// Its right edge.
    Right,
    /// Its top edge.
    Top,
    /// Its bottom edge.
    Bottom,
}

/// A tab's layout: a pane, or a split of two layouts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layout {
    /// A pane, which takes the whole of its part.
    Pane(PaneId),
    /// Two parts that share the room.
    Split {
        /// How the parts lie.
        direction: Direction,
        /// The share of the first part, from 0 to 1, as a divider's drag
        /// gave it; `None` shares the room by the parts' panes along the
        /// direction.
        ratio: Option<f64>,
        /// The left or top part.
        first: Box<Layout>,
        /// The right or bottom part.
        second: Box<Layout>,
    },
}

/// A binding of keys to a command, as a plugin asks for it. The user's
/// bindings, in the config, come after the plugins', and win over them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// The keys, as the config writes them: `cmd+c`, or a sequence, as
    /// `ctrl+a>c`.
    pub keys: String,
    /// The command's name.
    pub command: String,
    /// Its arguments.
    pub args: Args,
    /// The context in which it holds, as `pane.focused && !selection`;
    /// every context without one.
    pub when: Option<String>,
}

/// The text of a logical line of a pane's view, its rows that soft-wrap
/// joined, with the cell of each of its characters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineText {
    /// The text: a cell without a character is a space.
    pub text: String,
    /// The cell of each of the text's characters, in order: its column,
    /// and its row's number.
    pub cells: Vec<(u16, i64)>,
}

/// The most rows before a row and after it that a read of its logical line
/// takes, so that a line of many rows costs no more.
pub const MOST_REACH: usize = 256;

/// The most rows that a read of rows takes.
pub const MOST_LINES: usize = 100_000;

/// The most bytes of a plugin's state, as JSON.
pub const MOST_STATE: usize = 1024 * 1024;

/// The most matches of a search that the host keeps, past which it counts
/// them alone.
pub const MOST_MATCHES: usize = 100_000;

/// What a search of a pane's terminal looks for, through its scrollback
/// too: the text of each logical line, its soft-wrapped rows joined, of
/// spaces for the cells without a character, without the spaces that end
/// it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SearchQuery {
    /// The text, as it is, or the regex, of the syntax of Rust's `regex`.
    pub text: String,
    /// Whether the text is a regex.
    #[serde(default)]
    pub regex: bool,
    /// Whether case counts.
    #[serde(default)]
    pub case: SearchCase,
    /// Whether a match takes whole words alone: the cells right before it
    /// and right after it start with neither a letter, a digit nor `_`.
    #[serde(default)]
    pub whole_word: bool,
}

/// Whether case counts in a search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchCase {
    /// Case counts of a text of a capital letter, and not otherwise.
    #[default]
    Smart,
    /// `a` finds `a` alone.
    Sensitive,
    /// `a` finds `a` and `A`, of Unicode's simple case.
    Insensitive,
}

/// A match of a search: its first cell and its last, by their columns and
/// their rows' numbers, the last of a wide character its first half.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    /// The first cell.
    pub first: (u16, i64),
    /// The last cell.
    pub last: (u16, i64),
}

/// Matches of a search, as a read of them gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    /// The matches read, in the rows' order, as their cells are now.
    pub matches: Vec<SearchMatch>,
    /// How many matches the search found, those past [`MOST_MATCHES`] too.
    pub count: usize,
    /// Whether the search went to its end.
    pub done: bool,
}

/// The current match of a search, as a read of it gives it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Current {
    /// The current match, as its cells are now, and its place among the
    /// search's matches now, from 0, those that left the scrollback not
    /// counted; `None` of none, or of one that left the scrollback.
    pub current: Option<(usize, SearchMatch)>,
    /// How many of the search's matches the host keeps now, those that
    /// left the scrollback not counted.
    pub kept: usize,
}

/// A row's text, as a read of rows gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// The row's number.
    pub row: i64,
    /// Its text: a cell without a character a space, a cluster whole, a
    /// wide character once; without the spaces that end it, but where it
    /// wraps into the next.
    pub text: String,
    /// Whether it soft-wraps into the next row, its logical line going on
    /// there.
    pub wrapped: bool,
}

/// Where a plugin's tab bar goes in the windows that show it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BarPlace {
    /// In the titlebar, past the window's buttons.
    #[default]
    Titlebar,
    /// Under a titlebar of the window's title, across the window.
    BelowTitlebar,
    /// Across the window's bottom, below the status bar.
    Bottom,
    /// Down a column at the window's left, from below the titlebar to the
    /// window's bottom.
    Left {
        /// The column's width, in points.
        width: f32,
    },
    /// Down a column at the window's right, likewise.
    Right {
        /// The column's width, in points.
        width: f32,
    },
    /// Down a column at the window's left from its top, through the
    /// titlebar, as macOS's sidebars: the window's buttons over the
    /// column's top, which leaves them its room of
    /// [`Length::Titlebar`](crate::view::Length::Titlebar) and
    /// [`Length::Buttons`](crate::view::Length::Buttons), and the panes
    /// beside it below the titlebar, the window's title hidden.
    Sidebar {
        /// The column's width, in points.
        width: f32,
    },
    /// Across the whole titlebar, of its height, in place of the window's
    /// own title, which the window keeps for its menu and the system's
    /// lists of windows, as a window of a single tab shows its title and
    /// its marks: the titlebar's presses stay the window's; none in full
    /// screen, where the window has no titlebar.
    Title,
}

/// The narrowest and the widest that a tab bar's column is, in points.
pub const COLUMN_WIDTHS: (f32, f32) = (60.0, 600.0);

impl BarPlace {
    /// Whether the place is one that a host takes: a column's width from
    /// [`COLUMN_WIDTHS`]'s narrowest to its widest; why not otherwise.
    pub fn check(self) -> Result<(), String> {
        let (narrowest, widest) = COLUMN_WIDTHS;
        match self {
            Self::Left { width } | Self::Right { width } | Self::Sidebar { width }
                if !(narrowest..=widest).contains(&width) =>
            {
                Err(format!(
                    "a tab bar's column is from {narrowest} to {widest} points wide, not {width}"
                ))
            }
            _ => Ok(()),
        }
    }

    /// The column's width, in points, of a place on a side.
    pub fn width(self) -> Option<f32> {
        match self {
            Self::Left { width } | Self::Right { width } | Self::Sidebar { width } => Some(width),
            _ => None,
        }
    }
}

/// A color: one of the 256's palette, or an RGB one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Color {
    /// An entry of the 256-color palette.
    Palette(u8),
    /// Red, green and blue.
    Rgb(u8, u8, u8),
}

/// A kind of underline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Underline {
    /// One line.
    Single,
    /// Two lines.
    Double,
    /// A wave.
    Curly,
    /// Dots.
    Dotted,
    /// Dashes.
    Dashed,
}

/// The style of a run of cells, as the program gave it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextStyle {
    /// The text's color; the default one without.
    pub foreground: Option<Color>,
    /// The background; the default one without.
    pub background: Option<Color>,
    /// The underline, if any.
    pub underline: Option<Underline>,
    /// The underline's color; the text's without.
    pub underline_color: Option<Color>,
    /// Bold.
    pub bold: bool,
    /// Faint.
    pub faint: bool,
    /// Italic.
    pub italic: bool,
    /// Blinking.
    pub blink: bool,
    /// The text's and the background's colors swapped.
    pub inverse: bool,
    /// Hidden.
    pub invisible: bool,
    /// Struck through.
    pub strikethrough: bool,
    /// Overlined.
    pub overline: bool,
}

/// The cells of a row from a column to another, both included, of one
/// style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRun {
    /// The first column.
    pub first: u16,
    /// The last column.
    pub last: u16,
    /// Their style.
    pub style: TextStyle,
}

/// How a decoration looks over its cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecorationLook {
    /// The line under the cells, if any.
    pub underline: Option<Underline>,
    /// The underline's color, red, green and blue; the text's without.
    pub underline_color: Option<(u8, u8, u8)>,
    /// The cells' background, in place of the text's, if any.
    pub background: Option<(u8, u8, u8)>,
    /// Whether the pointer over the cells is a pointing hand, as over a
    /// link that a click opens.
    #[serde(default)]
    pub pointing: bool,
}

/// How a pane's view scrolls through its scrollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scroll {
    /// Up into the scrollback, by rows.
    Up(u32),
    /// Down toward the screen, by rows.
    Down(u32),
    /// Up by the screen's rows.
    PageUp,
    /// Down by the screen's rows.
    PageDown,
    /// To the scrollback's first row.
    Top,
    /// To the screen, which the view then follows.
    Bottom,
}

/// What carries a plugin's calls to the host, and the host's answers
/// back: the host itself, for a plugin on the main thread, or a channel's
/// end, for one on a thread of its own or in a VM, whose calls cross as
/// data.
pub trait Host {
    /// Answers `call`: a read with the host's state as it handles the call,
    /// a write once it queued it, which it applies as the plugin's call
    /// returns, a request that the caller may not make, or of a window, a
    /// tab or a pane that is not there, with its error.
    fn handle(&mut self, call: Call) -> Response;
}

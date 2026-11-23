//! A session saved as a layout: a file of TOML in the config's folder,
//! `layouts/<name>.toml`, of the session's windows, each of its frame and
//! its tabs, each tab of its title, its color and its panes, split as they
//! were, each pane of its folder and, written by hand, a command that its
//! shell runs as it starts. The sessions' switcher lists the layouts, and
//! opens one as a new session of new programs.
//!
//! ```toml
//! folder = "~/src/api"
//!
//! [[window]]
//! frame = { x = 80, y = 60, width = 1200, height = 800 }
//!
//! [[window.tab]]
//! title = "server"
//! [window.tab.split]
//! direction = "right"
//! ratio = 0.6
//! [window.tab.split.first.pane]
//! folder = "~/src/api"
//! command = "cargo run"
//! [window.tab.split.second.pane]
//! folder = "~/src/api"
//! focused = true
//! ```
//!
//! What does not read is told of its line, as the config's errors are.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::Position;
use crate::state::windows;

/// The folder of the layouts, in the config's folder.
pub const FOLDER: &str = "layouts";

/// The extension of a layout's file.
const EXTENSION: &str = "toml";

/// The most bytes of a layout's file, past which it is left aside.
pub const MOST_BYTES: u64 = 1 << 20;

/// The most layouts that the switcher lists.
pub const MOST_LAYOUTS: usize = 200;

/// The most characters of a pane's command.
pub const MOST_COMMAND: usize = 4096;

/// The folder of the layouts, beside the config file `config`.
pub fn folder(config: &Path) -> PathBuf {
    config.parent().unwrap_or(Path::new(".")).join(FOLDER)
}

/// The path of the layout `name` in `folder`.
pub fn path(folder: &Path, name: &str) -> PathBuf {
    folder.join(format!("{name}.{EXTENSION}"))
}

/// The names of the layouts of `folder`, its files of `.toml`, in their
/// names' order, [`MOST_LAYOUTS`] at most; none of no folder.
pub fn names(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(&format!(".{EXTENSION}"))?;
            (!stem.is_empty() && !stem.starts_with('.') && entry.path().is_file())
                .then(|| stem.to_owned())
        })
        .collect();
    names.sort();
    names.truncate(MOST_LAYOUTS);
    names
}

/// A layout's file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Layout {
    /// The folder of the session that it opens, a path from `/` or `~`, as
    /// the switcher finds a session of a folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<Folder>,
    /// Its windows, in their order.
    #[serde(rename = "window")]
    pub windows: Vec<Window>,
}

/// A window of a layout.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Window {
    /// Its content's frame on the screens, in points from the top left
    /// corner of the menu bar's screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<Frame>,
    /// Its tabs, in their order, one at least.
    #[serde(rename = "tab")]
    pub tabs: Tabs,
}

/// A window's frame, as a layout writes it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// A window's tabs, one at least.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<Tab>", into = "Vec<Tab>")]
pub struct Tabs(pub Vec<Tab>);

impl TryFrom<Vec<Tab>> for Tabs {
    type Error = String;

    fn try_from(tabs: Vec<Tab>) -> Result<Self, String> {
        if tabs.is_empty() {
            return Err("a window has one `[[window.tab]]` at least".to_owned());
        }
        Ok(Self(tabs))
    }
}

impl From<Tabs> for Vec<Tab> {
    fn from(tabs: Tabs) -> Self {
        tabs.0
    }
}

/// A tab of a layout: its title and color of its own, whether its window
/// shows it, whether it is pinned, and its panes, a pane or a split.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawTab", into = "RawTab")]
pub struct Tab {
    /// Its title of its own, in place of its program's.
    pub title: Option<Title>,
    /// Its color of its own, as `#rrggbb`.
    pub color: Option<HexColor>,
    /// Whether its window shows it, rather than its first tab.
    pub active: bool,
    /// Whether it is pinned, at its window's start.
    pub pinned: bool,
    /// Its panes.
    pub part: Part,
}

/// A tab as the file writes it, of a table `pane` or a table `split`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawTab {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<Title>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<HexColor>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    active: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pinned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pane: Option<Pane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    split: Option<Box<Split>>,
}

impl TryFrom<RawTab> for Tab {
    type Error = String;

    fn try_from(raw: RawTab) -> Result<Self, String> {
        let part = Part::try_from(RawPart {
            pane: raw.pane,
            split: raw.split,
        })?;
        Ok(Self {
            title: raw.title,
            color: raw.color,
            active: raw.active,
            pinned: raw.pinned,
            part,
        })
    }
}

impl From<Tab> for RawTab {
    fn from(tab: Tab) -> Self {
        let part = RawPart::from(tab.part);
        Self {
            title: tab.title,
            color: tab.color,
            active: tab.active,
            pinned: tab.pinned,
            pane: part.pane,
            split: part.split,
        }
    }
}

/// A layout's part: a pane, or a split of two parts, one of them alone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPart", into = "RawPart")]
pub enum Part {
    /// A pane.
    Pane(Pane),
    /// Two parts that share the room.
    Split(Box<Split>),
}

/// A part as the file writes it, of a table `pane` or a table `split`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawPart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pane: Option<Pane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    split: Option<Box<Split>>,
}

impl TryFrom<RawPart> for Part {
    type Error = String;

    fn try_from(raw: RawPart) -> Result<Self, String> {
        match (raw.pane, raw.split) {
            (Some(pane), None) => Ok(Self::Pane(pane)),
            (None, Some(split)) => Ok(Self::Split(split)),
            _ => Err(
                "a tab, and each part of a split, has a `pane` or a `split`, one of them alone"
                    .to_owned(),
            ),
        }
    }
}

impl From<Part> for RawPart {
    fn from(part: Part) -> Self {
        match part {
            Part::Pane(pane) => Self {
                pane: Some(pane),
                split: None,
            },
            Part::Split(split) => Self {
                pane: None,
                split: Some(split),
            },
        }
    }
}

/// A pane of a layout: the folder that its shell starts in, the command
/// that its shell runs as it starts, and whether it has its tab's keyboard
/// or takes its tab's whole room.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Pane {
    /// The folder, a path from `/` or `~`; the home directory of none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<Folder>,
    /// The command line that its shell runs as it starts, as though typed,
    /// its shell staying once it ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandLine>,
    /// Whether it has its tab's keyboard, rather than its tab's first pane.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
    /// Whether it takes its tab's whole room.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub zoomed: bool,
}

/// A split of a layout: how its parts lie, the first's share, and the
/// parts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Split {
    /// Where the second part lies of the first.
    pub direction: SplitDirection,
    /// The first part's share, more than 0 and less than 1; half of none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<Share>,
    /// The left or top part.
    pub first: Part,
    /// The right or bottom part.
    pub second: Part,
}

/// Where a split's second part lies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SplitDirection {
    /// Right of the first, side by side.
    Right,
    /// Below the first.
    Down,
}

/// A split's share, more than 0 and less than 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Share(f64);

impl TryFrom<f64> for Share {
    type Error = String;

    fn try_from(share: f64) -> Result<Self, String> {
        if share > 0.0 && share < 1.0 {
            Ok(Self(share))
        } else {
            Err(format!(
                "a split's `ratio` is more than 0 and less than 1, not {share}"
            ))
        }
    }
}

impl From<Share> for f64 {
    fn from(share: Share) -> Self {
        share.0
    }
}

/// A folder of a layout, a path from `/` or `~`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Folder(pub String);

impl TryFrom<String> for Folder {
    type Error = String;

    fn try_from(folder: String) -> Result<Self, String> {
        lokyr_plugin_sdk::host::check_directory(&folder)?;
        if folder.len() > 1024 {
            return Err("a folder's path is 1024 bytes at most".to_owned());
        }
        Ok(Self(folder))
    }
}

impl From<Folder> for String {
    fn from(folder: Folder) -> Self {
        folder.0
    }
}

/// A tab's title, as a plugin could give it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Title(pub String);

impl TryFrom<String> for Title {
    type Error = String;

    fn try_from(title: String) -> Result<Self, String> {
        if title.chars().count() > 256 || title.chars().any(char::is_control) {
            return Err(
                "a tab's `title` is 256 characters at most, of no control character".to_owned(),
            );
        }
        Ok(Self(title))
    }
}

impl From<Title> for String {
    fn from(title: Title) -> Self {
        title.0
    }
}

/// A color, as `#rrggbb`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor(pub String);

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(color: String) -> Result<Self, String> {
        let digits = color.strip_prefix('#').unwrap_or_default();
        if digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(color))
        } else {
            Err(format!("a tab's `color` is `#rrggbb`, not `{color}`"))
        }
    }
}

impl From<HexColor> for String {
    fn from(color: HexColor) -> Self {
        color.0
    }
}

/// A pane's command line, of [`MOST_COMMAND`] characters at most, of one
/// line, of no control character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommandLine(pub String);

impl TryFrom<String> for CommandLine {
    type Error = String;

    fn try_from(command: String) -> Result<Self, String> {
        // What shows of the file is what the shell runs: no control
        // character, nor one that shows nothing or changes the direction.
        let hidden = |c: char| {
            c.is_control()
                || lokyr_plugin_sdk::text::is_hidden(c)
                || matches!(c, '\u{200E}' | '\u{200F}' | '\u{061C}')
        };
        if command.chars().count() > MOST_COMMAND || command.chars().any(hidden) {
            return Err(format!(
                "a pane's `command` is one line of {MOST_COMMAND} characters at most, of no control character, nor one that shows nothing or changes the direction"
            ));
        }
        Ok(Self(command))
    }
}

impl From<CommandLine> for String {
    fn from(command: CommandLine) -> Self {
        command.0
    }
}

/// Why a layout does not read: its error, at its line where it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unread {
    /// The place of the error in the file, if it has one.
    pub position: Option<Position>,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.position {
            Some(position) => write!(f, "line {}: {}", position.line, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl Layout {
    /// The layout of `text`, a file's; or why it does not read, at its line:
    /// a layout of no window, or of more than
    /// [`windows::MOST_PANES`] panes, too.
    pub fn parse(text: &str) -> Result<Self, Unread> {
        let layout: Self = toml_edit::de::from_str(text).map_err(|error| Unread {
            position: error.span().map(|span| Position::at(text, span.start)),
            message: error.message().trim_end().to_owned(),
        })?;
        if layout.windows.is_empty() {
            return Err(Unread {
                position: None,
                message: "a layout has one `[[window]]` at least".to_owned(),
            });
        }
        let panes: usize = layout
            .windows
            .iter()
            .flat_map(|window| &window.tabs.0)
            .map(|tab| tab.part.panes())
            .sum();
        if panes > windows::MOST_PANES {
            return Err(Unread {
                position: None,
                message: format!(
                    "it holds {panes} panes, past the {} that a layout may open",
                    windows::MOST_PANES
                ),
            });
        }
        Ok(layout)
    }

    /// The layout of the file at `path`; or why it does not read.
    pub fn read(path: &Path) -> Result<Self, Unread> {
        let unread = |message: String| Unread {
            position: None,
            message,
        };
        let size = fs::metadata(path)
            .map_err(|error| unread(error.to_string()))?
            .len();
        if size > MOST_BYTES {
            return Err(unread(format!(
                "it holds {size} bytes, past the {MOST_BYTES} that a layout may"
            )));
        }
        let text = fs::read_to_string(path).map_err(|error| unread(error.to_string()))?;
        Self::parse(&text)
    }

    /// The layout as its file writes it, of a line that tells what it is.
    pub fn text(&self) -> String {
        let body = toml_edit::ser::to_string_pretty(self).unwrap_or_default();
        format!(
            "# A layout of Lokyr's: a session's windows, which the sessions' switcher\n\
             # opens as a new session. A pane's `command`, which you add, runs in its\n\
             # shell as it starts.\n\n{body}"
        )
    }

    /// Writes the layout in the file at `path`, whose folder it makes: a
    /// file written whole.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        crate::state::write_whole(path, self.text().as_bytes())
    }

    /// The layout of the windows `kept`, as the app keeps them, of the
    /// session's `folder`.
    pub fn of(kept: &[windows::Window], folder: Option<&str>) -> Self {
        Self {
            folder: folder.map(|folder| Folder(folder.to_owned())),
            windows: kept
                .iter()
                .filter(|window| !window.tabs.is_empty())
                .map(Window::of)
                .collect(),
        }
    }

    /// Its windows as the app opens them, as it opens the windows that it
    /// kept, each pane of its folder and command.
    pub fn windows(&self) -> Vec<windows::Window> {
        self.windows.iter().map(Window::kept).collect()
    }
}

impl Window {
    /// The window `kept`.
    fn of(kept: &windows::Window) -> Self {
        let tabs = kept
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| Tab::of(tab, index == kept.active && index > 0))
            .collect();
        Self {
            frame: kept.frame.map(|frame| Frame {
                x: frame.x,
                y: frame.y,
                width: frame.width,
                height: frame.height,
            }),
            tabs: Tabs(tabs),
        }
    }

    /// The window as the app opens it.
    fn kept(&self) -> windows::Window {
        windows::Window {
            frame: self.frame.map(|frame| windows::Frame {
                x: frame.x,
                y: frame.y,
                width: frame.width,
                height: frame.height,
            }),
            tabs: self.tabs.0.iter().map(Tab::kept).collect(),
            active: self.tabs.0.iter().position(|tab| tab.active).unwrap_or(0),
            groups: Vec::new(),
            session: None,
        }
    }
}

impl Tab {
    /// The tab `kept`, which its window shows where `active`, rather than
    /// its first.
    fn of(kept: &windows::Tab, active: bool) -> Self {
        let mut index = 0;
        Self {
            title: kept.title.clone().map(Title),
            color: kept.color.clone().map(HexColor),
            active,
            pinned: kept.pinned,
            part: Part::of(&kept.layout, kept.active, kept.zoomed, &mut index),
        }
    }

    /// The tab as the app opens it: the first pane focused, or zoomed, of
    /// several, takes the keyboard, or the tab's room.
    fn kept(&self) -> windows::Tab {
        let mut panes = Vec::new();
        self.part.panes_in(&mut panes);
        windows::Tab {
            title: self.title.clone().map(|title| title.0),
            color: self.color.clone().map(|color| color.0),
            layout: self.part.kept(),
            active: panes.iter().position(|pane| pane.focused).unwrap_or(0),
            zoomed: panes.iter().position(|pane| pane.zoomed),
            pinned: self.pinned,
            group: None,
            floating: Vec::new(),
            floating_shown: true,
        }
    }
}

impl Part {
    /// The part of `kept`, its panes counted by `index` in the layout's
    /// order, the one of `active` focused and the one of `zoomed` zoomed.
    fn of(kept: &windows::Layout, active: usize, zoomed: Option<usize>, index: &mut usize) -> Self {
        match kept {
            windows::Layout::Pane { directory, .. } => {
                let at = *index;
                *index += 1;
                Self::Pane(Pane {
                    folder: directory.clone().map(Folder),
                    command: None,
                    focused: at == active && at > 0,
                    zoomed: zoomed == Some(at),
                })
            }
            windows::Layout::Split {
                direction,
                ratio,
                first,
                second,
            } => Self::Split(Box::new(Split {
                direction: match direction {
                    windows::Direction::Horizontal => SplitDirection::Right,
                    windows::Direction::Vertical => SplitDirection::Down,
                },
                ratio: ratio.and_then(|ratio| Share::try_from(ratio).ok()),
                first: Self::of(first, active, zoomed, index),
                second: Self::of(second, active, zoomed, index),
            })),
        }
    }

    /// The number of its panes.
    pub fn panes(&self) -> usize {
        match self {
            Self::Pane(_) => 1,
            Self::Split(split) => split.first.panes() + split.second.panes(),
        }
    }

    /// Its panes, in the layout's order, after those of `panes`.
    fn panes_in<'a>(&'a self, panes: &mut Vec<&'a Pane>) {
        match self {
            Self::Pane(pane) => panes.push(pane),
            Self::Split(split) => {
                split.first.panes_in(panes);
                split.second.panes_in(panes);
            }
        }
    }

    /// The part as the app opens it.
    fn kept(&self) -> windows::Layout {
        match self {
            // A folder of `~`, of the home directory, as the app enters it.
            Self::Pane(pane) => windows::Layout::Pane {
                directory: pane
                    .folder
                    .as_ref()
                    .map(|folder| lokyr_plugin_sdk::host::home_expanded(&folder.0)),
                scrollback: None,
                command: pane.command.clone().map(|command| command.0),
            },
            Self::Split(split) => windows::Layout::Split {
                direction: match split.direction {
                    SplitDirection::Right => windows::Direction::Horizontal,
                    SplitDirection::Down => windows::Direction::Vertical,
                },
                ratio: split.ratio.map(f64::from),
                first: Box::new(split.first.kept()),
                second: Box::new(split.second.kept()),
            },
        }
    }
}

#[cfg(test)]
mod tests;

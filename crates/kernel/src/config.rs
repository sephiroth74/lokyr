//! The config file: where it is, and its sections, read whole or not at all.
//!
//! ```toml
//! config_version = 1   # the version of the file's format
//!
//! [font]
//! family = "CaskaydiaCove NFM"
//! style = "SemiBold"
//! size = 22
//!
//! [text]
//! bold = true
//!
//! [cursor]
//! shape = "bar"
//!
//! [window]
//! close-on-exit = "clean"   # clean | always | never
//! padding-x = 4             # points, at the left and the right
//! padding-y = 4             # points, at the top and the bottom
//! background-opacity = 1    # from 0 to 1
//! background-blur = 0       # points
//!
//! [terminal]
//! term = "xterm-256color"   # the TERM that programs see
//! unicode-widths = "current" # current | 8
//! ambiguous-width = 1       # 1 | 2
//! normalization = "none"    # none | nfc | nfd | hfs+
//!
//! [scrollback]
//! scroll-on-output = false  # whether output brings the view back down
//! limit = 10                # MiB of a window's scrollback
//! budget = 100              # MiB that the windows' scrollbacks share
//!
//! [keyboard]
//! option-as-meta = "none"   # none | left | right | both
//!
//! [clipboard]
//! read = "never"            # never | ask: a program's read of the clipboard
//!
//! [bell]
//! sound = false             # the system's alert sound
//! flash = true              # a short flash of the window that has the focus
//! bounce = true             # the Dock's icon bounces for one in the background
//! mark-title = true         # 🔔 in its title, until it takes the focus
//!
//! [app]
//! icon = "jormungandr"      # the icon's variant
//!
//! [keys]
//! "cmd+shift+]" = "tabs.next"                    # a command
//! "ctrl+a>c" = { command = "tabs.new", when = "pane.focused" }
//! "cmd+k" = false                                # takes a default away
//! ```
//!
//! Each section documents its keys: `[font]` is [`FontConfig`], `[text]`
//! [`TextConfig`], `[cursor]` [`CursorConfig`], `[window]` [`WindowConfig`],
//! `[terminal]` [`TerminalConfig`], `[scrollback]` [`ScrollbackConfig`],
//! `[keyboard]` [`KeyboardConfig`],
//! `[clipboard]` [`ClipboardConfig`], `[bell]` [`BellConfig`], `[app]`
//! [`AppConfig`], and `[keys]` [`KeysConfig`]. Every key and every section can be
//! left out, for its default. An error, of the
//! syntax, of a key or of a value, tells its place in the file, and a key
//! that no section has tells the nearest that it has.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use lokyr_text_core::FontConfig;
use lokyr_ui_core::cursor::CursorConfig;
use lokyr_ui_core::text::TextConfig;
use lokyr_vt_core::unicode::{Normalization, Tables, Widths};
use serde::Deserialize;
use serde::de::{self, Visitor};
use toml_edit::{Document, Item, TableLike};

/// The version of the file's format that the app reads.
pub const VERSION: i64 = 1;

/// The app's directory in the user's config directory.
const DIRECTORY: &str = "lokyr";

/// The config file's name.
const FILE: &str = "config.toml";

/// The config: its sections, each with the defaults of what the file leaves
/// out.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// The version of the file's format, which can only be the app's.
    #[serde(rename = "config_version")]
    version: Version,
    /// The `[font]` section.
    pub font: FontConfig,
    /// The `[text]` section.
    pub text: TextConfig,
    /// The `[cursor]` section.
    pub cursor: CursorConfig,
    /// The `[window]` section.
    pub window: WindowConfig,
    /// The `[terminal]` section.
    pub terminal: TerminalConfig,
    /// The `[scrollback]` section.
    pub scrollback: ScrollbackConfig,
    /// The `[keyboard]` section.
    pub keyboard: KeyboardConfig,
    /// The `[clipboard]` section.
    pub clipboard: ClipboardConfig,
    /// The `[bell]` section.
    pub bell: BellConfig,
    /// The `[app]` section.
    pub app: AppConfig,
    /// The `[keys]` section.
    pub keys: KeysConfig,
}

/// The `[keys]` section: bindings of keys to commands over the defaults, by
/// their keys, as [`crate::keymap`] writes them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct KeysConfig {
    /// The bindings, by their keys.
    pub bindings: BTreeMap<String, KeyValue>,
}

/// What keys take: a command, a binding, several in their contexts, or
/// `false`, which takes the defaults of the keys away.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum KeyValue {
    /// A command, without arguments, in every context.
    Command(String),
    /// `false`: the keys take nothing.
    Unbound(bool),
    /// A command, with its arguments and its context.
    Binding(KeyBinding),
    /// Bindings in their contexts, of which the last that holds wins.
    Bindings(Vec<KeyBinding>),
}

/// A binding of keys: its command, its arguments and its context.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    /// The command's name.
    pub command: String,
    /// Its arguments.
    #[serde(default)]
    pub args: BTreeMap<String, ArgValue>,
    /// The context in which it holds, as `pane.focused && !selection`.
    #[serde(default)]
    pub when: Option<String>,
}

/// An argument's value, as TOML writes it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    /// True or false.
    Boolean(bool),
    /// A whole number.
    Integer(i64),
    /// A number.
    Number(f64),
    /// Text.
    String(String),
}

/// The `[app]` section: the application's own looks and language.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AppConfig {
    /// The variant of the icon that shows while the app runs, by its name
    /// without `lokyr-`, which the app looks for among its icons.
    pub icon: String,
    /// The language of the texts that the app shows: `system`, the first
    /// of the system's preferred languages that the app has, or one, as
    /// `en` or `it`.
    pub language: String,
}

impl AppConfig {
    /// The icon's variant that the app shows when the config names none,
    /// which is the bundle's own.
    pub const DEFAULT_ICON: &str = "jormungandr";
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            icon: Self::DEFAULT_ICON.to_owned(),
            language: "system".to_owned(),
        }
    }
}

/// The `[terminal]` section: what programs are told of the terminal, and
/// how it counts and keeps their text, which must match how they count it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TerminalConfig {
    /// The `TERM` that programs see, which changes only what they are
    /// told, never the emulation, which stays xterm's.
    pub term: Term,
    /// Which width tables the terminal counts characters by.
    pub unicode_widths: UnicodeWidths,
    /// The cells of an East Asian Ambiguous character.
    pub ambiguous_width: AmbiguousWidth,
    /// The normal form that printed text takes.
    pub normalization: TextNormalization,
}

impl TerminalConfig {
    /// The terminal's widths, by clusters as programs start with.
    pub fn widths(&self) -> Widths {
        Widths {
            tables: self.unicode_widths.tables(),
            ambiguous_wide: self.ambiguous_width.is_wide(),
            ..Widths::default()
        }
    }
}

/// A `TERM`: letters, digits, `.`, `_`, `+` and `-`, 64 at most, as
/// terminfo names entries.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Term(String);

impl Term {
    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Term {
    fn default() -> Self {
        Self("xterm-256color".to_owned())
    }
}

impl TryFrom<String> for Term {
    type Error = String;

    fn try_from(term: String) -> Result<Self, Self::Error> {
        let named =
            |character: char| character.is_ascii_alphanumeric() || "._+-".contains(character);
        if (1..=64).contains(&term.len()) && term.chars().all(named) {
            Ok(Self(term))
        } else {
            Err(format!(
                "`{term}` is no TERM: letters, digits, `.`, `_`, `+` and `-`, 64 at most"
            ))
        }
    }
}

/// Which width tables the terminal counts characters by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum UnicodeWidths {
    /// Those of the Unicode version of the terminal's tables.
    #[default]
    #[serde(rename = "current")]
    Current,
    /// Those of Unicode 8, for programs that predate Unicode 9, which gave
    /// most emoji two cells.
    #[serde(rename = "8")]
    Unicode8,
}

impl UnicodeWidths {
    /// The terminal's tables.
    pub fn tables(self) -> Tables {
        match self {
            Self::Current => Tables::Current,
            Self::Unicode8 => Tables::Unicode8,
        }
    }
}

/// The cells of an East Asian Ambiguous character, as `α` and `→`: 1, as
/// most programs count them, or 2, as some fonts and locales of East Asia
/// draw them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "i64")]
pub struct AmbiguousWidth(u8);

impl AmbiguousWidth {
    /// Whether they take two cells.
    pub fn is_wide(self) -> bool {
        self.0 == 2
    }
}

impl Default for AmbiguousWidth {
    fn default() -> Self {
        Self(1)
    }
}

impl TryFrom<i64> for AmbiguousWidth {
    type Error = String;

    fn try_from(cells: i64) -> Result<Self, Self::Error> {
        match cells {
            1 => Ok(Self(1)),
            2 => Ok(Self(2)),
            _ => Err(format!(
                "an ambiguous character takes 1 or 2 cells, not {cells}"
            )),
        }
    }
}

/// The normal form that printed text takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub enum TextNormalization {
    /// As programs write it.
    #[default]
    #[serde(rename = "none")]
    None,
    /// Composed: `e` and U+0301 become `é`.
    #[serde(rename = "nfc")]
    Nfc,
    /// Decomposed, the reverse.
    #[serde(rename = "nfd")]
    Nfd,
    /// Decomposed as Apple's HFS+ keeps file names.
    #[serde(rename = "hfs+")]
    HfsPlus,
}

impl TextNormalization {
    /// The terminal's form.
    pub fn form(self) -> Normalization {
        match self {
            Self::None => Normalization::None,
            Self::Nfc => Normalization::Nfc,
            Self::Nfd => Normalization::Nfd,
            Self::HfsPlus => Normalization::HfsPlus,
        }
    }
}

/// The `[scrollback]` section: the rows above the screen, the view that
/// scrolls through them, and the memory that they take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ScrollbackConfig {
    /// Whether a program's output brings a view scrolled up back down to
    /// the screen, as typing always does.
    pub scroll_on_output: bool,
    /// The memory of a window's scrollback, its screen's included.
    pub limit: Memory<10>,
    /// The memory that the windows' scrollbacks share: a window that needs
    /// more gives back its own oldest rows.
    pub budget: Memory<100>,
}

/// An amount of memory, in MiB, from 1 to 16,384, whole or not, by default
/// `DEFAULT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(try_from = "f64")]
pub struct Memory<const DEFAULT: u32>(usize);

impl<const DEFAULT: u32> Memory<DEFAULT> {
    /// The most, in MiB.
    pub const MAX: f64 = 16_384.0;

    /// The memory in bytes.
    pub fn bytes(self) -> usize {
        self.0
    }
}

impl<const DEFAULT: u32> Default for Memory<DEFAULT> {
    fn default() -> Self {
        Self(DEFAULT as usize * MIB)
    }
}

impl<const DEFAULT: u32> TryFrom<f64> for Memory<DEFAULT> {
    type Error = String;

    fn try_from(mebibytes: f64) -> Result<Self, Self::Error> {
        if (1.0..=Self::MAX).contains(&mebibytes) {
            // Within the range, the bytes fit a `usize`.
            Ok(Self((mebibytes * MIB as f64) as usize))
        } else {
            Err(format!(
                "a scrollback's memory, in MiB, is from 1 to {}, not {mebibytes}",
                Self::MAX
            ))
        }
    }
}

/// The bytes of a MiB.
const MIB: usize = 1024 * 1024;

/// The `[keyboard]` section: how keys reach programs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct KeyboardConfig {
    /// Which Option keys act as Meta, sending an escape before the key,
    /// rather than typing the characters that the layout gives them.
    pub option_as_meta: OptionAsMeta,
}

/// The `[clipboard]` section: what programs do with the clipboard. Their
/// writes, with OSC 52, always reach it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ClipboardConfig {
    /// Whether a program can read the clipboard, with OSC 52.
    pub read: ClipboardRead,
}

/// Whether a program can read the clipboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClipboardRead {
    /// Never: a read gets no answer, as a program that runs elsewhere, over
    /// ssh, could read what the user copied.
    #[default]
    Never,
    /// Each read asks the user first.
    Ask,
}

/// The `[bell]` section: what a program's bell does, BEL, as the agents
/// that run in the terminal ring it when they wait for the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct BellConfig {
    /// Whether it plays the system's alert sound.
    pub sound: bool,
    /// Whether the window, when it has the focus, flashes.
    pub flash: bool,
    /// Whether the Dock's icon bounces once, for a window without the
    /// focus, while Lokyr is not the active application.
    pub bounce: bool,
    /// Whether a window without the focus shows 🔔 before its title, until
    /// it takes the focus.
    pub mark_title: bool,
}

impl Default for BellConfig {
    fn default() -> Self {
        Self {
            sound: false,
            flash: true,
            bounce: true,
            mark_title: true,
        }
    }
}

/// Which Option keys act as Meta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptionAsMeta {
    /// Neither, as Option types `@ # [ ] { } ~` on many layouts, the
    /// Italian one among them.
    #[default]
    None,
    /// The left one.
    Left,
    /// The right one.
    Right,
    /// Both.
    Both,
}

impl OptionAsMeta {
    /// Whether the left Option, and the right one, act as Meta.
    pub fn sides(self) -> (bool, bool) {
        match self {
            Self::None => (false, false),
            Self::Left => (true, false),
            Self::Right => (false, true),
            Self::Both => (true, true),
        }
    }
}

/// The `[window]` section: what a window does, the room around its grid,
/// and how much of what is behind it shows through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowConfig {
    /// What a window does when its program ends.
    pub close_on_exit: CloseOnExit,
    /// The room between the grid and the window's left and right edges.
    pub padding_x: Padding,
    /// The room between the grid and the window's top and bottom edges.
    pub padding_y: Padding,
    /// The opacity of the default background.
    pub background_opacity: Opacity,
    /// The blur of what shows through the default background.
    pub background_blur: Blur,
}

/// The opacity of the window's default background, from 0, clear, to 1,
/// opaque: what is behind the window shows through. The cells whose
/// background a program colors stay opaque, as does the text.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(try_from = "f64")]
pub struct Opacity(f32);

impl Opacity {
    /// The opacity, from 0 to 1.
    pub fn value(self) -> f32 {
        self.0
    }

    /// Whether the background lets what is behind it show.
    pub fn is_translucent(self) -> bool {
        self.0 < 1.0
    }
}

impl Default for Opacity {
    fn default() -> Self {
        Self(1.0)
    }
}

impl TryFrom<f64> for Opacity {
    type Error = String;

    fn try_from(opacity: f64) -> Result<Self, Self::Error> {
        if (0.0..=1.0).contains(&opacity) {
            // Within the range, the value fits an `f32`.
            Ok(Self(opacity as f32))
        } else {
            Err(format!(
                "the window's background opacity is from 0 to 1, not {opacity}"
            ))
        }
    }
}

/// The radius of the blur of what shows through the window's background,
/// in points; 0 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(try_from = "f64")]
pub struct Blur(f32);

impl Blur {
    /// The most.
    pub const MAX: f32 = 100.0;

    /// The radius in points.
    pub fn points(self) -> f32 {
        self.0
    }
}

impl TryFrom<f64> for Blur {
    type Error = String;

    fn try_from(points: f64) -> Result<Self, Self::Error> {
        if (0.0..=f64::from(Self::MAX)).contains(&points) {
            // Within the range, the value fits an `f32`.
            Ok(Self(points as f32))
        } else {
            Err(format!(
                "the window's background blur, in points, is from 0 to {}, not {points}",
                Self::MAX
            ))
        }
    }
}

/// The room between the grid and two edges of the window, each, in points,
/// so that it keeps its look on a Retina display. The pixels that the cells
/// leave over go to the right and the bottom, so that the grid stays where
/// it is as the window's size changes.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(try_from = "f64")]
pub struct Padding(f32);

impl Padding {
    /// The least.
    pub const MIN: f32 = 0.0;
    /// The most.
    pub const MAX: f32 = 100.0;

    /// The padding in points.
    pub fn points(self) -> f32 {
        self.0
    }

    /// The padding in whole pixels, on a display of `scale`.
    pub fn pixels(self, scale: f64) -> u32 {
        // A cast from a float saturates.
        (f64::from(self.0) * scale).round() as u32
    }
}

impl Default for Padding {
    fn default() -> Self {
        Self(4.0)
    }
}

impl TryFrom<f64> for Padding {
    type Error = String;

    fn try_from(points: f64) -> Result<Self, Self::Error> {
        if (f64::from(Self::MIN)..=f64::from(Self::MAX)).contains(&points) {
            // Within the range, the value fits an `f32`.
            Ok(Self(points as f32))
        } else {
            Err(format!(
                "the window's padding, in points, is from {} to {}, not {points}",
                Self::MIN,
                Self::MAX
            ))
        }
    }
}

/// What a window does when its program ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloseOnExit {
    /// It closes when the program exits with 0, and otherwise keeps its
    /// screen, with a line that tells how the program ended, until a key
    /// closes it.
    #[default]
    Clean,
    /// It closes.
    Always,
    /// It keeps its screen, with the line, until a key closes it.
    Never,
}

/// The version of the file's format: [`VERSION`], the only one so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(try_from = "i64")]
struct Version;

impl TryFrom<i64> for Version {
    type Error = String;

    fn try_from(version: i64) -> Result<Self, Self::Error> {
        if version == VERSION {
            Ok(Self)
        } else {
            Err(format!(
                "the config's version is {version}, and the app reads version {VERSION}"
            ))
        }
    }
}

impl Config {
    /// Reads the config file at `path`; the defaults when there is none.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(Error {
                    path: path.to_owned(),
                    position: None,
                    message: format!("it cannot be read: {error}"),
                });
            }
        };
        let text = String::from_utf8(bytes).map_err(|error| {
            let valid = &error.as_bytes()[..error.utf8_error().valid_up_to()];
            let valid = std::str::from_utf8(valid).expect("the bytes before the error are valid");
            Error {
                path: path.to_owned(),
                position: Some(Position::at(valid, valid.len())),
                message: "the file is not UTF-8 text".to_owned(),
            }
        })?;
        Self::parse(&text, path)
    }

    /// Reads the text of a config file, which `path` names in its errors.
    pub fn parse(text: &str, path: &Path) -> Result<Self, Error> {
        let error = |span: Option<std::ops::Range<usize>>, message: &str| Error {
            path: path.to_owned(),
            position: span.map(|span| Position::at(text, span.start)),
            message: message.to_owned(),
        };
        let document = Document::parse(text).map_err(|e| error(e.span(), e.message()))?;
        check_keys(document.as_table()).map_err(|(span, message)| error(span, &message))?;
        check_bindings(document.as_table()).map_err(|(span, message)| error(span, &message))?;
        Self::deserialize(toml_edit::de::Deserializer::from(document))
            .map_err(|e| error(e.span(), e.message()))
    }
}

/// The config file's path: in `$XDG_CONFIG_HOME` when it is set, or else in
/// `~/.config`, on macOS too; `None` without a home directory.
pub fn path() -> Option<PathBuf> {
    path_in(std::env::var_os("XDG_CONFIG_HOME"), std::env::home_dir())
}

fn path_in(config_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    // The XDG Base Directory Specification ignores a path that is not
    // absolute: https://specifications.freedesktop.org/basedir-spec/latest/
    let absolute = |path: &PathBuf| path.is_absolute();
    let base = config_home
        .map(PathBuf::from)
        .filter(absolute)
        .or_else(|| home.filter(absolute).map(|home| home.join(".config")))?;
    Some(base.join(DIRECTORY).join(FILE))
}

/// An error of the config file, at its place in the file when it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The file.
    pub path: PathBuf,
    /// Where the error is in the file.
    pub position: Option<Position>,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path.display())?;
        if let Some(Position { line, column }) = self.position {
            write!(f, ":{line}:{column}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for Error {}

/// A place in a file: its line and its column, in characters, both from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// The line.
    pub line: usize,
    /// The column, in characters.
    pub column: usize,
}

impl Position {
    /// The place of the byte at `offset` in `text`.
    fn at(text: &str, offset: usize) -> Self {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let before = &text[..offset];
        let start = before.rfind('\n').map_or(0, |newline| newline + 1);
        Self {
            line: before.matches('\n').count() + 1,
            column: before[start..].chars().count() + 1,
        }
    }
}

/// The sections of the file, with the keys of each.
fn sections() -> [(&'static str, &'static [&'static str]); 10] {
    [
        ("font", keys::<FontConfig>()),
        ("text", keys::<TextConfig>()),
        ("cursor", keys::<CursorConfig>()),
        ("window", keys::<WindowConfig>()),
        ("terminal", keys::<TerminalConfig>()),
        ("scrollback", keys::<ScrollbackConfig>()),
        ("keyboard", keys::<KeyboardConfig>()),
        ("clipboard", keys::<ClipboardConfig>()),
        ("bell", keys::<BellConfig>()),
        ("app", keys::<AppConfig>()),
    ]
}

/// A span of the config's text, which tells an error's line.
type Span = Option<std::ops::Range<usize>>;

/// Fails at the first binding of `[keys]` that is not one, at its own line:
/// keys that are not a combination or a sequence, or that another entry
/// names otherwise, as `opt+x` and `alt+x`; a command's name that is not
/// one, `true`, a binding without its command or with another field,
/// arguments that are not plain values, and a `when` that is not an
/// expression of the contexts.
fn check_bindings(root: &dyn TableLike) -> Result<(), (Span, String)> {
    let Some(item) = root.get("keys") else {
        return Ok(());
    };
    let Some(table) = item.as_table_like() else {
        return Err((item.span(), "`keys` is a section, not a value".to_owned()));
    };
    let mut named: Vec<(Vec<crate::keymap::Combo>, &str)> = Vec::new();
    for (keys, value) in table.iter() {
        let span = table.key(keys).and_then(toml_edit::Key::span);
        let combos = crate::keymap::parse_keys(keys).map_err(|message| (span.clone(), message))?;
        if let Some((_, other)) = named.iter().find(|(named, _)| *named == combos) {
            return Err((
                span,
                format!("`{keys}` and `{other}` are the same keys, which one entry binds"),
            ));
        }
        named.push((combos, keys));
        let at = value.span().or(span.clone());
        let fail = |message: String| (at.clone(), message);
        if let Some(command) = value.as_str() {
            check_command(command).map_err(fail)?;
        } else if let Some(bound) = value.as_bool() {
            if bound {
                return Err(fail(format!(
                    "`{keys}` takes a command, a binding or `false`, not `true`"
                )));
            }
        } else if let Some(binding) = value.as_table_like() {
            check_binding(binding, at)?;
        } else if let Some(bindings) = value.as_array() {
            for binding in bindings {
                let Some(binding) = binding.as_inline_table() else {
                    let at = binding.span().or(at);
                    return Err((at, format!("the bindings of `{keys}` are tables")));
                };
                check_binding(binding, binding.span().or(at.clone()))?;
            }
        } else if let Some(bindings) = value.as_array_of_tables() {
            for binding in bindings {
                check_binding(binding, binding.span().or(at.clone()))?;
            }
        } else {
            return Err(fail(format!(
                "`{keys}` takes a command, a binding or `false`"
            )));
        }
    }
    Ok(())
}

/// Fails for a command's name that is not one.
fn check_command(command: &str) -> Result<(), String> {
    if crate::commands::is_name(command) {
        Ok(())
    } else {
        Err(format!(
            "`{command}` is not a command's name, `namespace.action`"
        ))
    }
}

/// Fails for a binding without its command, with another field, with
/// arguments that are not plain values, or with a `when` that is not one:
/// at the field's line, or at `at`, the binding's.
fn check_binding(binding: &dyn TableLike, at: Span) -> Result<(), (Span, String)> {
    for (field, value) in binding.iter() {
        let here = value
            .span()
            .or_else(|| binding.key(field).and_then(toml_edit::Key::span))
            .or(at.clone());
        check_field(field, value).map_err(|message| (here, message))?;
    }
    if binding.get("command").is_none() {
        return Err((at, "a binding needs its `command`".to_owned()));
    }
    Ok(())
}

/// Fails for a field of a binding that is not one.
fn check_field(field: &str, value: &toml_edit::Item) -> Result<(), String> {
    match field {
        "command" => match value.as_str() {
            Some(command) => check_command(command),
            None => Err("a binding's `command` is a command's name".to_owned()),
        },
        "args" => {
            let Some(args) = value.as_table_like() else {
                return Err("a binding's `args` are a table".to_owned());
            };
            for (name, value) in args.iter() {
                let plain = value.as_str().is_some()
                    || value.as_integer().is_some()
                    || value.as_float().is_some()
                    || value.as_bool().is_some();
                if !plain {
                    return Err(format!(
                        "the argument `{name}` is a string, an integer, a number or a boolean"
                    ));
                }
            }
            Ok(())
        }
        "when" => match value.as_str() {
            Some(when) => crate::keymap::When::parse(when).map(|_| ()),
            None => Err("a binding's `when` is an expression, in a string".to_owned()),
        },
        other => Err(format!(
            "a binding has `command`, `args` and `when`, not `{other}`"
        )),
    }
}

/// Fails at the first key that the file's sections do not have, with the
/// nearest that they have, or at a section given as a value.
fn check_keys(root: &dyn TableLike) -> Result<(), (Option<std::ops::Range<usize>>, String)> {
    let top = keys::<Config>();
    let sections = sections();
    for (name, item) in root.iter() {
        let key = root.key(name).expect("a key of the table");
        if !top.contains(&name) {
            let message = if matches!(item, Item::Table(_)) {
                format!("unknown section `[{name}]`{}", nearest(name, top, "[", "]"))
            } else {
                format!("unknown key `{name}`{}", nearest(name, top, "", ""))
            };
            return Err((key.span(), message));
        }
        let Some((_, known)) = sections.iter().find(|(section, _)| *section == name) else {
            continue;
        };
        let Some(section) = item.as_table_like() else {
            return Err((item.span(), format!("`{name}` is a section, not a value")));
        };
        for (inner, _) in section.iter() {
            if !known.contains(&inner) {
                let key = section.key(inner).expect("a key of the table");
                let message = format!(
                    "unknown key `{inner}` in `[{name}]`{}",
                    nearest(inner, known, "", "")
                );
                return Err((key.span(), message));
            }
        }
    }
    Ok(())
}

/// `; did you mean …?`, with the known name nearest to `name`, when one is
/// near enough to be a typing error; nothing otherwise.
fn nearest(name: &str, known: &[&str], open: &str, close: &str) -> String {
    let limit = (name.chars().count() / 3).max(2);
    known
        .iter()
        .map(|candidate| (distance(name, candidate), candidate))
        .filter(|&(distance, _)| distance <= limit)
        .min_by_key(|&(distance, _)| distance)
        .map(|(_, candidate)| format!("; did you mean `{open}{candidate}{close}`?"))
        .unwrap_or_default()
}

/// The edit distance of `a` and `b`, in characters: Levenshtein's.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, a) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &b) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(a != b))
                .min(above + 1)
                .min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The keys of a section, as serde names them: the fields that a derived
/// `Deserialize` hands to `deserialize_struct`.
fn keys<T: for<'de> Deserialize<'de>>() -> &'static [&'static str] {
    struct Fields<'a>(&'a mut &'static [&'static str]);

    impl<'de> de::Deserializer<'de> for Fields<'_> {
        type Error = de::value::Error;

        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
            Err(de::Error::custom("not a struct"))
        }

        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            *self.0 = fields;
            Err(de::Error::custom("only its fields"))
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
            enum identifier ignored_any
        }
    }

    let mut fields: &'static [&'static str] = &[];
    let _ = T::deserialize(Fields(&mut fields));
    fields
}

#[cfg(test)]
mod tests {
    use lokyr_text_core::{Factor, Name, Size};
    use lokyr_ui_core::cursor::{Shape, Thickness};

    use super::*;

    fn parse(text: &str) -> Result<Config, Error> {
        Config::parse(text, Path::new("config.toml"))
    }

    /// The error of `text`, with its line and column, and its message.
    fn error(text: &str) -> (Option<(usize, usize)>, String) {
        let error = parse(text).expect_err("an invalid config");
        let position = error
            .position
            .map(|position| (position.line, position.column));
        (position, error.message)
    }

    fn name(name: &str) -> Option<Name> {
        Some(Name::try_from(name.to_owned()).unwrap())
    }

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(parse(""), Ok(Config::default()));
        assert_eq!(parse("# nothing\n\n[font]\n"), Ok(Config::default()));
    }

    #[test]
    fn a_full_config_reads() {
        let config = parse(
            r#"
            config_version = 1

            [font]
            family = "CaskaydiaCove NFM"
            size = 22
            style = "SemiBold"
            bold-style = "Bold"
            italic-style = "Italic"
            bold-italic-style = "Bold Italic"
            non-ascii-family = "Hiragino Sans"
            line-height = 1.2
            cell-width = 1.05
            ligatures = false

            [cursor]
            shape = "bar"
            blink = false
            thickness = 1.5

            [window]
            close-on-exit = "never"
            padding-x = 10
            padding-y = 2.5
            background-opacity = 0.85
            background-blur = 20

            [terminal]
            term = "xterm-lokyr"
            unicode-widths = "8"
            ambiguous-width = 2
            normalization = "hfs+"

            [scrollback]
            scroll-on-output = true
            limit = 2.5
            budget = 64

            [keyboard]
            option-as-meta = "left"

            [clipboard]
            read = "ask"

            [bell]
            sound = true
            mark-title = false

            [app]
            icon = "laguz"
            language = "it"
            "#,
        )
        .unwrap();
        assert_eq!(config.clipboard.read, ClipboardRead::Ask);
        let terminal = &config.terminal;
        assert_eq!(terminal.term.as_str(), "xterm-lokyr");
        assert_eq!(
            (terminal.widths().tables, terminal.widths().ambiguous_wide),
            (Tables::Unicode8, true)
        );
        assert_eq!(terminal.normalization.form(), Normalization::HfsPlus);
        assert_eq!(config.scrollback.limit.bytes(), 5 * MIB / 2);
        assert_eq!(config.scrollback.budget.bytes(), 64 * MIB);
        let defaults = Config::default();
        assert_eq!(defaults.terminal.term.as_str(), "xterm-256color");
        assert_eq!(defaults.terminal.widths(), Widths::default());
        assert_eq!(defaults.terminal.normalization.form(), Normalization::None);
        assert_eq!(
            (
                defaults.scrollback.limit.bytes(),
                defaults.scrollback.budget.bytes()
            ),
            (10 * MIB, 100 * MIB)
        );
        assert_eq!(
            config.bell,
            BellConfig {
                sound: true,
                flash: true,
                bounce: true,
                mark_title: false,
            }
        );
        let bell = Config::default().bell;
        assert!(!bell.sound && bell.flash && bell.bounce && bell.mark_title);
        assert_eq!(Config::default().clipboard.read, ClipboardRead::Never);
        assert_eq!(config.app.language, "it");
        assert_eq!(Config::default().app.language, "system");
        assert!(config.scrollback.scroll_on_output);
        assert!(!Config::default().scrollback.scroll_on_output);
        assert_eq!(config.app.icon, "laguz");
        assert_eq!(Config::default().app.icon, AppConfig::DEFAULT_ICON);
        assert_eq!(config.window.close_on_exit, CloseOnExit::Never);
        assert_eq!(config.window.padding_x.points(), 10.0);
        assert_eq!(config.window.padding_y.points(), 2.5);
        assert_eq!(config.window.background_opacity.value(), 0.85);
        assert_eq!(config.window.background_blur.points(), 20.0);
        assert_eq!(config.keyboard.option_as_meta.sides(), (true, false));
        assert_eq!(
            config.cursor,
            CursorConfig {
                shape: Shape::Bar,
                blink: false,
                thickness: Some(Thickness::try_from(1.5).unwrap()),
            }
        );
        let font = config.font;
        assert_eq!(font.family, name("CaskaydiaCove NFM"));
        assert_eq!(font.size, Size::try_from(22.0).unwrap());
        assert_eq!(font.style, name("SemiBold"));
        assert_eq!(font.bold_style, name("Bold"));
        assert_eq!(font.italic_style, name("Italic"));
        assert_eq!(font.bold_italic_style, name("Bold Italic"));
        assert_eq!(font.non_ascii_family, name("Hiragino Sans"));
        assert_eq!(font.line_height, Factor::try_from(1.2).unwrap());
        assert_eq!(font.cell_width, Factor::try_from(1.05).unwrap());
        assert!(!font.ligatures);
    }

    #[test]
    fn the_keys_read_as_bindings() {
        let config = parse(
            r#"
            [keys]
            "cmd+shift+]" = "tabs.next"
            "cmd+1" = { command = "tabs.goto", args = { index = 1, wrap = true, speed = 1.5, label = "one" } }
            "ctrl+a>c" = { command = "tabs.new", when = "pane.focused" }
            "cmd+k" = false
            "cmd+d" = [
                { command = "panes.split", when = "!alternate-screen" },
                { command = "view.clear", when = "alternate-screen" },
            ]
            "#,
        )
        .unwrap();
        let bindings = &config.keys.bindings;
        assert_eq!(bindings.len(), 5);
        assert_eq!(
            bindings["cmd+shift+]"],
            KeyValue::Command("tabs.next".to_owned())
        );
        assert_eq!(bindings["cmd+k"], KeyValue::Unbound(false));
        let KeyValue::Binding(goto) = &bindings["cmd+1"] else {
            panic!("a binding: {:?}", bindings["cmd+1"]);
        };
        assert_eq!(goto.args["index"], ArgValue::Integer(1));
        assert_eq!(goto.args["wrap"], ArgValue::Boolean(true));
        assert_eq!(goto.args["speed"], ArgValue::Number(1.5));
        assert_eq!(goto.args["label"], ArgValue::String("one".to_owned()));
        let KeyValue::Bindings(split) = &bindings["cmd+d"] else {
            panic!("bindings: {:?}", bindings["cmd+d"]);
        };
        assert_eq!(split.len(), 2);
    }

    #[test]
    fn a_binding_that_is_not_one_tells_its_place() {
        let cases = [
            (
                "[keys]\n\"cmd+foo\" = \"tabs.new\"\n",
                2,
                "`foo` is not a key",
            ),
            ("[keys]\n\"cmd+t\" = \"new\"\n", 2, "not a command's name"),
            ("[keys]\n\"cmd+t\" = true\n", 2, "not `true`"),
            ("[keys]\n\"cmd+t\" = 3\n", 2, "takes a command"),
            (
                "[keys]\n\"cmd+t\" = { when = \"selection\" }\n",
                2,
                "needs its `command`",
            ),
            (
                "[keys]\n\"cmd+t\" = { command = \"tabs.new\", key = 1 }\n",
                2,
                "not `key`",
            ),
            (
                "[keys]\n\"cmd+t\" = { command = \"tabs.new\", when = \"focus\" }\n",
                2,
                "no context",
            ),
            (
                "[keys]\n\"cmd+t\" = { command = \"tabs.new\", args = { a = [1] } }\n",
                2,
                "the argument `a`",
            ),
            (
                "[keys]\n\"ctrl+a>wheel-up\" = \"font.increase\"\n",
                2,
                "takes no sequence",
            ),
            ("keys = 3\n", 1, "is a section"),
            // A binding over several lines tells the line of what is wrong
            // in it: an array's element, a field of a table of its own, of
            // dotted keys, and of an array of tables.
            (
                "[keys]\n\"cmd+d\" = [\n  { command = \"pane.split\" },\n  { command = \"split\" },\n]\n",
                4,
                "not a command's name",
            ),
            (
                "[keys]\n\"cmd+d\" = [\n  \"pane.split\",\n]\n",
                3,
                "are tables",
            ),
            (
                "[keys]\n\"cmd+d\" = [\n  { command = \"pane.split\" },\n  { when = \"selection\" },\n]\n",
                4,
                "needs its `command`",
            ),
            (
                "[keys.\"cmd+d\"]\ncommand = \"pane.split\"\nwhen = \"focus\"\n",
                3,
                "no context",
            ),
            (
                "[keys]\n\"cmd+d\".command = \"pane.split\"\n\"cmd+d\".args = { a = [1] }\n",
                3,
                "the argument `a`",
            ),
            (
                "[[keys.\"cmd+d\"]]\ncommand = \"pane.split\"\n[[keys.\"cmd+d\"]]\nwhen = \"selection\"\n",
                3,
                "needs its `command`",
            ),
            // The same keys in two spellings.
            (
                "[keys]\n\"alt+x\" = \"tabs.new\"\n\"opt+x\" = \"tabs.next\"\n",
                3,
                "the same keys",
            ),
            (
                "[keys]\n\"cmd+shift+n\" = \"tabs.new\"\n\"shift+cmd+n\" = false\n",
                3,
                "the same keys",
            ),
        ];
        for (text, line, message) in cases {
            let (position, error) = error(text);
            assert_eq!(
                position.map(|(line, _)| line),
                Some(line),
                "{text}: {error}"
            );
            assert!(error.contains(message), "{text}: {error}");
        }
    }

    #[test]
    fn dotted_keys_and_inline_tables_read_as_sections() {
        let mut expected = Config::default();
        expected.font.size = Size::try_from(15.0).unwrap();
        assert_eq!(parse("font.size = 15"), Ok(expected.clone()));
        assert_eq!(parse("font = { size = 15 }"), Ok(expected));
    }

    #[test]
    fn a_syntax_error_tells_its_place() {
        let (position, _) = error("[font]\nsize = = 3\n");
        assert_eq!(position, Some((2, 8)));
        let (position, _) = error("[font\n");
        assert_eq!(position.map(|(line, _)| line), Some(1));
    }

    #[test]
    fn an_unknown_key_tells_the_nearest_known_one() {
        assert_eq!(
            error("[font]\nfamily = \"Menlo\"\n  sise = 3\n"),
            (
                Some((3, 3)),
                "unknown key `sise` in `[font]`; did you mean `size`?".to_owned()
            )
        );
        assert_eq!(
            error("[font]\nbold_style = \"Bold\"\n").1,
            "unknown key `bold_style` in `[font]`; did you mean `bold-style`?"
        );
        assert_eq!(
            error("[font]\nweight = 600\n").1,
            "unknown key `weight` in `[font]`"
        );
        assert_eq!(
            error("[fonts]\nsize = 3\n"),
            (
                Some((1, 2)),
                "unknown section `[fonts]`; did you mean `[font]`?".to_owned()
            )
        );
        assert_eq!(
            error("config-version = 1\n").1,
            "unknown key `config-version`; did you mean `config_version`?"
        );
        // In a dotted key and in an inline table.
        assert_eq!(
            error("font.sise = 3\n"),
            (
                Some((1, 6)),
                "unknown key `sise` in `[font]`; did you mean `size`?".to_owned()
            )
        );
        assert_eq!(error("font = { sise = 3 }\n").0, Some((1, 10)));
    }

    #[test]
    fn a_wrong_value_tells_its_place() {
        assert_eq!(
            error("[font]\nsize = 3.5\n"),
            (
                Some((2, 8)),
                "the size, in points, is from 4 to 200, not 3.5".to_owned()
            )
        );
        let (position, message) = error("[font]\nsize = \"big\"\n");
        assert_eq!(position, Some((2, 8)));
        assert!(message.contains("string"), "{message}");
        let (position, _) = error("[font]\nfamily = \"  \"\n");
        assert_eq!(position, Some((2, 10)));
        assert_eq!(
            error("font = 3\n"),
            (Some((1, 8)), "`font` is a section, not a value".to_owned())
        );
    }

    #[test]
    fn the_cursor_s_shape_is_one_of_four() {
        for (name, shape) in [
            ("block", Shape::Block),
            ("bar", Shape::Bar),
            ("underline", Shape::Underline),
            ("hollow", Shape::Hollow),
        ] {
            let config = parse(&format!("[cursor]\nshape = \"{name}\"")).unwrap();
            assert_eq!(config.cursor.shape, shape);
        }
        let (position, message) = error("[cursor]\nshape = \"beam\"\n");
        assert_eq!(position, Some((2, 9)));
        assert!(message.contains("`block`, `bar`, `underline`"), "{message}");
        assert_eq!(
            error("[cursor]\nthickness = 30\n"),
            (
                Some((2, 13)),
                "the cursor's thickness, in points, is from 0.5 to 20, not 30".to_owned()
            )
        );
        assert_eq!(
            error("[cursor]\nblnk = true\n").1,
            "unknown key `blnk` in `[cursor]`; did you mean `blink`?"
        );
    }

    #[test]
    fn the_background_s_opacity_and_blur_are_within_their_ranges() {
        let window = Config::default().window;
        assert_eq!(window.background_opacity.value(), 1.0);
        assert!(!window.background_opacity.is_translucent());
        assert_eq!(window.background_blur.points(), 0.0);
        let config = parse("[window]\nbackground-opacity = 0\nbackground-blur = 100\n").unwrap();
        assert!(config.window.background_opacity.is_translucent());
        assert_eq!(config.window.background_blur.points(), 100.0);
        assert_eq!(
            error("[window]\nbackground-opacity = 1.5\n"),
            (
                Some((2, 22)),
                "the window's background opacity is from 0 to 1, not 1.5".to_owned()
            )
        );
        assert_eq!(
            error("[window]\nbackground-blur = -2\n").1,
            "the window's background blur, in points, is from 0 to 100, not -2"
        );
        assert!(Opacity::try_from(f64::NAN).is_err());
        assert!(Blur::try_from(f64::NAN).is_err());
    }

    #[test]
    fn the_terminal_s_values_are_within_their_ranges() {
        for (text, message) in [
            ("[terminal]\nterm = \"\"\n", "is no TERM"),
            ("[terminal]\nterm = \"xterm 256\"\n", "is no TERM"),
            ("[terminal]\nterm = \"a/b\"\n", "is no TERM"),
            ("[terminal]\nunicode-widths = \"9\"\n", "unknown variant"),
            ("[terminal]\nambiguous-width = 3\n", "1 or 2 cells, not 3"),
            ("[terminal]\nnormalization = \"nfkc\"\n", "unknown variant"),
            ("[scrollback]\nlimit = 0.5\n", "from 1 to 16384, not 0.5"),
            (
                "[scrollback]\nbudget = 20000\n",
                "from 1 to 16384, not 20000",
            ),
        ] {
            let (position, error) = error(text);
            assert_eq!(position.map(|(line, _)| line), Some(2), "{text}");
            assert!(error.contains(message), "{text}: {error}");
        }
        assert!(Term::try_from("x".repeat(65)).is_err());
        assert!(Term::try_from("xterm-256color".to_owned()).is_ok());
        assert!(Memory::<1>::try_from(f64::NAN).is_err());
    }

    #[test]
    fn the_padding_is_in_points_within_its_range() {
        assert_eq!(Config::default().window.padding_x.points(), 4.0);
        let config = parse("[window]\npadding-x = 0\npadding-y = 100\n").unwrap();
        assert_eq!(config.window.padding_x.pixels(2.0), 0);
        assert_eq!(config.window.padding_y.pixels(2.0), 200);
        assert_eq!(Padding::try_from(2.3).unwrap().pixels(2.0), 5);
        assert_eq!(
            error("[window]\npadding-y = -1\n"),
            (
                Some((2, 13)),
                "the window's padding, in points, is from 0 to 100, not -1".to_owned()
            )
        );
        assert!(Padding::try_from(100.5).is_err());
        assert!(Padding::try_from(f64::NAN).is_err());
        assert_eq!(
            error("[window]\npading-x = 2\n").1,
            "unknown key `pading-x` in `[window]`; did you mean `padding-x`?"
        );
    }

    #[test]
    fn only_the_app_s_version_reads() {
        assert_eq!(parse("config_version = 1"), Ok(Config::default()));
        assert_eq!(
            error("\nconfig_version = 2\n"),
            (
                Some((2, 18)),
                "the config's version is 2, and the app reads version 1".to_owned()
            )
        );
    }

    #[test]
    fn columns_count_characters() {
        let (position, _) = error("[font]\nfamily = \"Écrire\" size = 3\n");
        assert_eq!(position, Some((2, 19)));
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_an_unreadable_one_an_error() {
        let directory = std::env::temp_dir().join(format!("lokyr-config-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        assert_eq!(
            Config::read(&directory.join("missing.toml")),
            Ok(Config::default())
        );
        // A directory cannot be read as a file.
        let error = Config::read(&directory).unwrap_err();
        assert_eq!(error.position, None);
        assert!(error.message.starts_with("it cannot be read"));

        let file = directory.join("config.toml");
        fs::write(&file, b"[font]\nfamily = \"a\xffb\"\n").unwrap();
        let error = Config::read(&file).unwrap_err();
        assert_eq!(error.path, file);
        assert_eq!(
            error.position,
            Some(Position {
                line: 2,
                column: 12
            })
        );
        assert_eq!(error.message, "the file is not UTF-8 text");
        assert_eq!(
            error.to_string(),
            format!("{}:2:12: the file is not UTF-8 text", file.display())
        );

        fs::write(&file, "[font]\nsize = 18\n").unwrap();
        assert_eq!(
            Config::read(&file).unwrap().font.size,
            Size::try_from(18.0).unwrap()
        );
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn the_file_is_in_the_xdg_config_home_or_in_dot_config() {
        let home = Some(PathBuf::from("/Users/me"));
        assert_eq!(
            path_in(None, home.clone()),
            Some(PathBuf::from("/Users/me/.config/lokyr/config.toml"))
        );
        assert_eq!(
            path_in(Some("/tmp/xdg".into()), home.clone()),
            Some(PathBuf::from("/tmp/xdg/lokyr/config.toml"))
        );
        // A relative or empty one is ignored.
        assert_eq!(
            path_in(Some("xdg".into()), home.clone()),
            path_in(None, home.clone())
        );
        assert_eq!(path_in(Some("".into()), home.clone()), path_in(None, home));
        assert_eq!(path_in(None, None), None);
        assert_eq!(path_in(None, Some(PathBuf::new())), None);
    }

    #[test]
    fn the_sections_keys_are_serde_s() {
        assert_eq!(
            keys::<Config>(),
            [
                "config_version",
                "font",
                "text",
                "cursor",
                "window",
                "terminal",
                "scrollback",
                "keyboard",
                "clipboard",
                "bell",
                "app",
                "keys"
            ]
        );
        assert_eq!(
            keys::<ScrollbackConfig>(),
            ["scroll-on-output", "limit", "budget"]
        );
        assert_eq!(
            keys::<TerminalConfig>(),
            ["term", "unicode-widths", "ambiguous-width", "normalization"]
        );
        assert_eq!(keys::<ClipboardConfig>(), ["read"]);
        assert_eq!(
            keys::<BellConfig>(),
            ["sound", "flash", "bounce", "mark-title"]
        );
        assert_eq!(keys::<KeyboardConfig>(), ["option-as-meta"]);
        assert_eq!(keys::<AppConfig>(), ["icon", "language"]);
        assert_eq!(OptionAsMeta::default().sides(), (false, false));
        assert_eq!(OptionAsMeta::Both.sides(), (true, true));
        assert_eq!(
            keys::<WindowConfig>(),
            [
                "close-on-exit",
                "padding-x",
                "padding-y",
                "background-opacity",
                "background-blur"
            ]
        );
        assert_eq!(keys::<CursorConfig>(), ["shape", "blink", "thickness"]);
        let font = keys::<FontConfig>();
        assert_eq!(font.len(), 14);
        assert_eq!(
            keys::<TextConfig>(),
            ["bold", "italic", "ansi-colors", "bold-is-bright", "blink"]
        );
        assert!(font.contains(&"bold-italic-style"));
        assert!(font.contains(&"non-ascii-family"));
        assert!(font.contains(&"builtin-powerline"));
    }

    /// The complete example of the configuration's guide reads, and holds
    /// every key that the file can have, so that the example keeps up with
    /// the keys.
    #[test]
    fn the_guide_s_complete_example_reads_and_has_every_key() {
        let guide = include_str!("../../../docs/CONFIGURATION.md");
        let example = guide
            .split_once("### A complete example")
            .and_then(|(_, rest)| rest.split_once("```toml\n"))
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(example, _)| example)
            .expect("the guide has a complete example");
        parse(example).expect("the complete example reads");
        let document: Document<String> = example.parse().unwrap();
        let root = document.as_table();
        for key in keys::<Config>() {
            assert!(root.contains_key(key), "`{key}` is in the complete example");
        }
        for (section, keys) in sections() {
            let table = root[section].as_table().expect("a section");
            for key in keys {
                assert!(
                    table.contains_key(key),
                    "`{key}` of `[{section}]` is in the complete example"
                );
            }
        }
    }

    #[test]
    fn the_nearest_name_is_near_enough() {
        assert_eq!(distance("sise", "size"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(
            nearest("fmaily", &["family", "size"], "", ""),
            "; did you mean `family`?"
        );
        assert_eq!(nearest("colour", &["family", "size"], "", ""), "");
    }
}

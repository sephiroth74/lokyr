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
//! columns = 100             # a new window's size, in cells
//! rows = 30
//! remember-frame = true     # the first window where the last one was
//! close-on-exit = "clean"   # clean | always | never
//! padding-x = 4             # points, at the left and the right
//! padding-y = 4             # points, at the top and the bottom
//! background-opacity = 1    # from 0 to 1
//! background-blur = 0       # points
//! title-size = true         # the grid's size in the title
//! title-shortcut = true     # ⌥⌘1 at the titlebar's right
//! opaque-titlebar = false   # the titlebar opaque over a translucent window
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
//! [links]
//! urls = "ask"              # ask | open: a URL that a click opens
//! files = "ask"             # ask | open | reveal: a file that a click opens
//!
//! [bell]
//! sound = false             # the system's alert sound
//! flash = true              # a short flash of the window that has the focus
//! bounce = true             # the Dock's icon bounces for one in the background
//! mark-title = true         # 🔔 in its title, until it takes the focus
//!
//! [app]
//! icon = "jormungandr"      # the icon's variant
//! appearance = "system"     # system | light | dark
//! quit-after-last-window-closed = false  # stays open without windows
//!
//! [keys]
//! "cmd+shift+]" = "tabs.next"                    # a command
//! "ctrl+a>c" = { command = "tabs.new", when = "pane.focused" }
//! "cmd+k" = false                                # takes a default away
//! ```
//!
//! Each section documents its keys: `[font]` is [`FontConfig`], `[text]`
//! [`TextConfig`], `[cursor]` [`CursorConfig`], `[window]` [`WindowConfig`],
//! `[interface]` [`InterfaceConfig`], `[slots]` [`SlotsConfig`], `[terminal]` [`TerminalConfig`], `[scrollback]` [`ScrollbackConfig`],
//! `[keyboard]` [`KeyboardConfig`],
//! `[clipboard]` [`ClipboardConfig`], `[links]` [`LinksConfig`], `[bell]`
//! [`BellConfig`], `[theme]` [`ThemeConfig`], `[app]`
//! [`AppConfig`], `[updates]` [`UpdatesConfig`], `[keys]` [`KeysConfig`],
//! and the plugins' own, `[plugins.<namespace>]`, [`PluginsConfig`], which
//! their schemas check. Every key and every section can be
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
use lokyr_text_core::as_written;
use lokyr_ui_core::cursor::CursorConfig;
use lokyr_ui_core::interface::InterfaceConfig;
use lokyr_ui_core::text::TextConfig;
use lokyr_vt_core::unicode::{Normalization, Tables, Widths};
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use toml_edit::{Document, DocumentMut, Item, TableLike};

mod plugins;
mod written;

pub use plugins::{PluginSection, PluginsConfig};

/// The version of the file's format that the app reads.
pub const VERSION: i64 = 1;

/// The app's directory in the user's config directory.
const DIRECTORY: &str = "lokyr";

/// The config file's name.
const FILE: &str = "config.toml";

/// The config: its sections, each with the defaults of what the file leaves
/// out.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
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
    /// The quick terminal's section before it was its plugin's,
    /// `[quick-terminal]`, whose keys the file's reading ignores, telling
    /// it in [`Config::moved`].
    #[serde(rename = "quick-terminal", skip_serializing)]
    old_quick_terminal: Ignored,
    /// The `[interface]` section.
    pub interface: InterfaceConfig,
    /// The `[slots]` section.
    pub slots: SlotsConfig,
    /// The `[terminal]` section.
    pub terminal: TerminalConfig,
    /// The `[scrollback]` section.
    pub scrollback: ScrollbackConfig,
    /// The `[keyboard]` section.
    pub keyboard: KeyboardConfig,
    /// The `[clipboard]` section.
    pub clipboard: ClipboardConfig,
    /// The `[links]` section.
    pub links: LinksConfig,
    /// The `[bell]` section.
    pub bell: BellConfig,
    /// The `[theme]` section.
    pub theme: ThemeConfig,
    /// The `[app]` section.
    pub app: AppConfig,
    /// The `[updates]` section.
    pub updates: UpdatesConfig,
    /// The `[debug]` section.
    pub debug: DebugConfig,
    /// The `[keys]` section.
    pub keys: KeysConfig,
    /// The plugins' sections, `[plugins.<namespace>]`, which their schemas
    /// check once the plugins are there.
    #[serde(skip_serializing)]
    pub plugins: PluginsConfig,
    /// The sections of the file that are plugins' now, each with the
    /// namespace of the plugin whose section it is, and what tells it, at
    /// its place: their keys are ignored.
    #[serde(skip)]
    pub moved: Vec<(String, Error)>,
}

/// A section that the file's reading ignores, whatever it holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Ignored;

impl<'de> Deserialize<'de> for Ignored {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self)
    }
}

/// The sections that are the built-in plugins' now, each with the
/// namespace of its plugin.
const MOVED: [(&str, &str); 1] = [("quick-terminal", "quick-terminal")];

/// The `[keys]` section: bindings of keys to commands over the defaults, by
/// their keys, as [`crate::keymap`] writes them.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct KeysConfig {
    /// The bindings, by their keys.
    pub bindings: BTreeMap<String, KeyValue>,
}

/// What keys take: a command, a binding, several in their contexts, or
/// `false`, which takes the defaults of the keys away.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    /// The command's name.
    pub command: String,
    /// Its arguments.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, ArgValue>,
    /// The context in which it holds, as `pane.focused && !selection`.
    #[serde(default)]
    pub when: Option<String>,
}

/// An argument's value, as TOML writes it.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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

/// The `[app]` section: the application's own looks and language, and
/// whether it quits as its last window closes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AppConfig {
    /// The variant of the icon that shows while the app runs, by its name
    /// without `lokyr-`, which the app looks for among its icons.
    pub icon: String,
    /// The language of the texts that the app shows: `system`, the first
    /// of the system's preferred languages that the app has, or one, as
    /// `en` or `it`.
    pub language: String,
    /// The appearance of the app and of its theme: the system's, or light
    /// or dark, whatever the system's is.
    pub appearance: AppearanceMode,
    /// Whether the app quits as its last window closes, the hidden quick
    /// terminal's aside, which goes with it: otherwise it stays open
    /// without windows, until its icon in the Dock or a command opens one.
    pub quit_after_last_window_closed: bool,
}

/// The appearance that the app takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppearanceMode {
    /// The system's, as it changes.
    #[default]
    System,
    /// Light.
    Light,
    /// Dark.
    Dark,
}

impl AppearanceMode {
    /// Its value in the config file.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
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
            appearance: AppearanceMode::System,
            quit_after_last_window_closed: false,
        }
    }
}

/// The `[terminal]` section: what programs are told of the terminal, and
/// how it counts and keeps their text, which must match how they count it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Term(String);

impl From<Term> for String {
    fn from(term: Term) -> Self {
        term.0
    }
}

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct AmbiguousWidth(u8);

impl From<AmbiguousWidth> for i64 {
    fn from(width: AmbiguousWidth) -> Self {
        i64::from(width.0)
    }
}

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Memory<const DEFAULT: u32>(usize);

/// In MiB, as the file writes it.
impl<const DEFAULT: u32> From<Memory<DEFAULT>> for f64 {
    fn from(memory: Memory<DEFAULT>) -> Self {
        // Fewer bytes than an `f64` holds exactly.
        memory.0 as f64 / MIB as f64
    }
}

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct KeyboardConfig {
    /// Which Option keys act as Meta, sending an escape before the key,
    /// rather than typing the characters that the layout gives them.
    pub option_as_meta: OptionAsMeta,
}

/// The `[clipboard]` section: what programs do with the clipboard, whose
/// writes, with OSC 52, always reach it, and when a paste asks first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ClipboardConfig {
    /// Whether a program can read the clipboard, with OSC 52.
    pub read: ClipboardRead,
    /// When a paste with line breaks asks the user first.
    pub confirm_paste: ConfirmPaste,
    /// Whether a selection by the mouse copies its text as it ends.
    pub copy_on_select: bool,
}

/// When a paste with line breaks, which a program that does not ask for
/// bracketed paste takes as Return, asks the user first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfirmPaste {
    /// When the program does not ask for bracketed paste and the text has
    /// several lines; a line with a line break after it pastes without it.
    #[default]
    Auto,
    /// Whenever the text has a line break.
    Always,
    /// Never.
    Never,
}

/// The `[links]` section: what a click on a link does, as the links'
/// plugin opens it, or another plugin; a directory always opens. And the
/// engine that searches the web for the selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct LinksConfig {
    /// What a URL does.
    pub urls: OpenUrl,
    /// What a file does.
    pub files: OpenFile,
    /// The engine that searches the web.
    pub search_engine: SearchEngine,
}

/// The engine that searches the web for a text: one of a name, or a URL
/// of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum SearchEngine {
    /// Google's, `google`.
    #[default]
    Google,
    /// DuckDuckGo's, `duckduckgo`.
    DuckDuckGo,
    /// Bing's, `bing`.
    Bing,
    /// Ecosia's, `ecosia`.
    Ecosia,
    /// Kagi's, `kagi`.
    Kagi,
    /// Startpage's, `startpage`.
    Startpage,
    /// A URL from `https://` or `http://`, whose `{query}` the text takes.
    Url(String),
}

/// The engines of a name, each with its URL of the text's `{query}`.
const SEARCH_ENGINES: [(&str, &str); 6] = [
    ("google", "https://www.google.com/search?q={query}"),
    ("duckduckgo", "https://duckduckgo.com/?q={query}"),
    ("bing", "https://www.bing.com/search?q={query}"),
    ("ecosia", "https://www.ecosia.org/search?q={query}"),
    ("kagi", "https://kagi.com/search?q={query}"),
    ("startpage", "https://www.startpage.com/do/search?q={query}"),
];

impl SearchEngine {
    /// The engines of a name, in the order of [`SEARCH_ENGINES`].
    const NAMED: [Self; 6] = [
        Self::Google,
        Self::DuckDuckGo,
        Self::Bing,
        Self::Ecosia,
        Self::Kagi,
        Self::Startpage,
    ];

    /// The URL that searches for `text`, its characters but letters,
    /// digits and `-._~` escaped, as a URL's query takes them.
    pub fn url(&self, text: &str) -> String {
        let template = match self {
            Self::Url(url) => url.as_str(),
            named => Self::NAMED
                .iter()
                .position(|engine| engine == named)
                .map_or(SEARCH_ENGINES[0].1, |at| SEARCH_ENGINES[at].1),
        };
        let mut query = String::new();
        for byte in text.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                query.push(char::from(byte));
            } else {
                query.push_str(&format!("%{byte:02X}"));
            }
        }
        template.replace("{query}", &query)
    }
}

impl From<SearchEngine> for String {
    fn from(engine: SearchEngine) -> Self {
        match engine {
            SearchEngine::Url(url) => url,
            named => SearchEngine::NAMED
                .iter()
                .position(|engine| *engine == named)
                .map_or_else(String::new, |at| SEARCH_ENGINES[at].0.to_owned()),
        }
    }
}

impl TryFrom<String> for SearchEngine {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if let Some(at) = SEARCH_ENGINES.iter().position(|(name, _)| *name == text) {
            return Ok(Self::NAMED[at].clone());
        }
        let web = text.starts_with("https://") || text.starts_with("http://");
        if web && text.contains("{query}") {
            return Ok(Self::Url(text));
        }
        let names: Vec<String> = SEARCH_ENGINES
            .iter()
            .map(|(name, _)| format!("`\"{name}\"`"))
            .collect();
        Err(format!(
            "`{text}` is no search engine: {}, or a URL from `https://` or `http://` that holds `{{query}}`",
            names.join(", ")
        ))
    }
}

/// What opening a URL does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenUrl {
    /// It asks the user first, showing the URL.
    #[default]
    Ask,
    /// It opens.
    Open,
}

/// What opening a file does; a file that would run, rather than open,
/// shows in the file manager.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenFile {
    /// It asks the user first, showing the path, whether to open the file
    /// or show it in the file manager.
    #[default]
    Ask,
    /// It opens in its application.
    Open,
    /// It shows in the file manager, selected.
    Reveal,
}

/// Whether a program can read the clipboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
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

/// The `[theme]` section: the themes of the light and the dark appearance,
/// by their names, a file's of the themes' folder or a built-in one's, and
/// the least contrast of the terminal's text to its background.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct ThemeConfig {
    /// The light appearance's theme.
    pub light: String,
    /// The dark appearance's theme.
    pub dark: String,
    /// The least contrast ratio of the terminal's text to its background.
    pub minimum_contrast: Contrast,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            light: crate::themes::LIGHT.to_owned(),
            dark: crate::themes::DARK.to_owned(),
            minimum_contrast: Contrast::default(),
        }
    }
}

/// A contrast ratio, as WCAG 2 defines it, from 1, none, to 21, black on
/// white.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Contrast(f32);

impl Contrast {
    /// The ratio.
    pub fn value(self) -> f32 {
        self.0
    }
}

impl Default for Contrast {
    fn default() -> Self {
        Self(1.0)
    }
}

impl From<Contrast> for f64 {
    fn from(value: Contrast) -> Self {
        as_written(value.0)
    }
}

impl TryFrom<f64> for Contrast {
    type Error = String;

    fn try_from(ratio: f64) -> Result<Self, Self::Error> {
        if (1.0..=21.0).contains(&ratio) {
            // Within the range, the value fits an `f32`.
            Ok(Self(ratio as f32))
        } else {
            Err(format!("a contrast ratio is from 1 to 21, not {ratio}"))
        }
    }
}

/// The `[slots]` section: the plugin whose view fills each slot of one
/// plugin's, by its name, as `lokyr/tabs`; without one, or of a plugin that
/// does not fill it, the first of those that fill it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct SlotsConfig {
    /// The tab bar's.
    pub tab_bar: Option<String>,
    /// The panes' headers'.
    pub pane_header: Option<String>,
}

/// The `[updates]` section: the app's own updates, where it can update
/// itself, as its bundle holds an updater on the Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct UpdatesConfig {
    /// Whether the app checks for a newer version now and then, a day
    /// apart at least; the menu's item checks whatever it says.
    pub check: bool,
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self { check: true }
    }
}

/// The `[debug]` section: what Lokyr shows of its own work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DebugConfig {
    /// Whether a panel of the performance shows over each window.
    pub performance_overlay: bool,
    /// The window's corner that the panel takes.
    pub performance_corner: Corner,
}

/// A corner of a window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    /// The top right one, under the titlebar.
    #[default]
    TopRight,
    /// The top left one, under the titlebar.
    TopLeft,
    /// The bottom right one.
    BottomRight,
    /// The bottom left one.
    BottomLeft,
}

/// Which Option keys act as Meta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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

/// The `[window]` section: what a window does, the directory that its shell
/// starts in, the room around its grid, how much of what is behind it shows
/// through, and its titlebar.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowConfig {
    /// A new window's columns.
    pub columns: Cells,
    /// A new window's rows.
    pub rows: Cells,
    /// Whether the first window, as the app starts, opens where the last
    /// window that the user moved or resized was, of its size.
    pub remember_frame: bool,
    /// What a window does when its program ends.
    pub close_on_exit: CloseOnExit,
    /// The directory that the first window's shell starts in, as the app
    /// starts.
    pub start_directory: StartDirectory,
    /// The directory that a new window's shell starts in.
    pub new_directory: NewDirectory,
    /// The room between the grid and the window's left and right edges.
    pub padding_x: Padding,
    /// The room between the grid and the window's top and bottom edges.
    pub padding_y: Padding,
    /// The opacity of the default background.
    pub background_opacity: Opacity,
    /// The blur of what shows through the default background.
    pub background_blur: Blur,
    /// Whether the title ends with the grid's size, as `100×33`.
    pub title_size: bool,
    /// Whether the titlebar shows the keys that bring the window forward,
    /// as `⌥⌘1`, at its right.
    pub title_shortcut: bool,
    /// Whether the titlebar stays opaque over a translucent window, in the
    /// default background's color.
    pub opaque_titlebar: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            columns: Cells(100),
            rows: Cells(30),
            remember_frame: true,
            close_on_exit: CloseOnExit::default(),
            start_directory: StartDirectory::default(),
            new_directory: NewDirectory::default(),
            padding_x: Padding::default(),
            padding_y: Padding::default(),
            background_opacity: Opacity::default(),
            background_blur: Blur::default(),
            title_size: true,
            title_shortcut: true,
            opaque_titlebar: false,
        }
    }
}

/// The opacity of the window's default background, from 0, clear, to 1,
/// opaque: what is behind the window shows through. The cells whose
/// background a program colors stay opaque, as does the text.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Opacity(f32);

impl From<Opacity> for f64 {
    fn from(value: Opacity) -> Self {
        as_written(value.0)
    }
}

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

/// A count of a new window's cells, its columns or its rows, from 1 to
/// [`Cells::MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "i64", into = "i64")]
pub struct Cells(u16);

impl From<Cells> for i64 {
    fn from(value: Cells) -> Self {
        value.0.into()
    }
}

impl Cells {
    /// The most.
    pub const MAX: u16 = 1000;

    /// The count.
    pub fn count(self) -> u16 {
        self.0
    }
}

impl TryFrom<i64> for Cells {
    type Error = String;

    fn try_from(count: i64) -> Result<Self, Self::Error> {
        u16::try_from(count)
            .ok()
            .filter(|count| (1..=Self::MAX).contains(count))
            .map(Self)
            .ok_or_else(|| {
                format!(
                    "a window's columns and rows are from 1 to {}, not {count}",
                    Self::MAX
                )
            })
    }
}

/// The radius of the blur of what shows through the window's background,
/// in points; 0 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Blur(f32);

impl From<Blur> for f64 {
    fn from(value: Blur) -> Self {
        as_written(value.0)
    }
}

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
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Padding(f32);

impl From<Padding> for f64 {
    fn from(value: Padding) -> Self {
        as_written(value.0)
    }
}

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

/// The directory that the first window's shell starts in, as the app
/// starts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum StartDirectory {
    /// The home directory.
    #[default]
    Home,
    /// The directory of the last window that closed, as the app keeps it.
    Last,
    /// A directory, as the file writes it, from `/` or from `~`.
    Path(String),
}

impl From<StartDirectory> for String {
    fn from(directory: StartDirectory) -> Self {
        match directory {
            StartDirectory::Home => "home".to_owned(),
            StartDirectory::Last => "last".to_owned(),
            StartDirectory::Path(path) => path,
        }
    }
}

impl TryFrom<String> for StartDirectory {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        match text.as_str() {
            "home" => Ok(Self::Home),
            "last" => Ok(Self::Last),
            _ if is_directory_path(&text) => Ok(Self::Path(text)),
            _ => Err(format!(
                "`{text}` is no start directory: `\"home\"`, `\"last\"`, or a path from `/` or `~`"
            )),
        }
    }
}

/// The directory that a new window's shell starts in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub enum NewDirectory {
    /// The directory of the program in the window that has the focus.
    #[default]
    Inherit,
    /// The home directory.
    Home,
    /// A directory, as the file writes it, from `/` or from `~`.
    Path(String),
}

impl From<NewDirectory> for String {
    fn from(directory: NewDirectory) -> Self {
        match directory {
            NewDirectory::Inherit => "inherit".to_owned(),
            NewDirectory::Home => "home".to_owned(),
            NewDirectory::Path(path) => path,
        }
    }
}

impl TryFrom<String> for NewDirectory {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        match text.as_str() {
            "inherit" => Ok(Self::Inherit),
            "home" => Ok(Self::Home),
            _ if is_directory_path(&text) => Ok(Self::Path(text)),
            _ => Err(format!(
                "`{text}` is no new directory: `\"inherit\"`, `\"home\"`, or a path from `/` or `~`"
            )),
        }
    }
}

/// Whether `text` is a path that a directory's key takes: from `/`, or
/// from `~`, the home directory.
fn is_directory_path(text: &str) -> bool {
    text.starts_with('/') || text == "~" || text.starts_with("~/")
}

/// The directory that `path`, of a directory's key, names: from `~`, of
/// `home`; `None` from `~` without a home directory.
pub fn expanded(path: &str, home: Option<&Path>) -> Option<PathBuf> {
    let Some(rest) = path.strip_prefix('~') else {
        return Some(PathBuf::from(path));
    };
    let (home, rest) = (home?, rest.trim_start_matches('/'));
    Some(if rest.is_empty() {
        home.to_owned()
    } else {
        home.join(rest)
    })
}

/// What a window does when its program ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "i64", into = "i64")]
struct Version;

impl From<Version> for i64 {
    fn from(_: Version) -> Self {
        VERSION
    }
}

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
        let plugins = PluginsConfig::read(document.as_table(), text, path)
            .map_err(|(span, message)| error(span, &message))?;
        let moved: Vec<(String, Error)> = MOVED
            .iter()
            .filter_map(|&(section, namespace)| {
                let key = document.as_table().key(section)?;
                let message = format!(
                    "`[{section}]` is now `[plugins.{namespace}]`, its plugin's section: move its keys there; here they are ignored"
                );
                Some((namespace.to_owned(), error(key.span(), &message)))
            })
            .collect();
        let mut config = Self::deserialize(toml_edit::de::Deserializer::from(document))
            .map_err(|e| error(e.span(), e.message()))?;
        config.plugins = plugins;
        config.moved = moved;
        Ok(config)
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

/// Sets the key `key` of the section `[section]` of the config file at
/// `path` to the string `value`, keeping the rest of the file, its
/// comments too: in the file that a link points to, which stays a link, or
/// in a new file, with its directory, when there is none. A section that
/// the file lacks goes at its end, after its last comments, as those of a
/// file whose keys are all commented out. A file that does not read as a
/// config is left as it is, and its error returned.
pub fn set_string(path: &Path, section: &str, key: &str, value: &str) -> Result<(), Error> {
    let failed = |path: &Path, message: String| Error {
        path: path.to_owned(),
        position: None,
        message,
    };
    let target = link_target(path);
    let text = match fs::read_to_string(&target) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            format!("config_version = {VERSION}\n")
        }
        Err(error) => return Err(failed(&target, format!("it cannot be read: {error}"))),
    };
    Config::parse(&text, &target)?;
    let mut document: DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| failed(&target, error.message().to_owned()))?;
    // The comments and the blank lines after the last section are the
    // document's, which a new section would come before.
    let after = (!document.contains_key(section))
        .then(|| document.trailing().as_str().map(str::to_owned))
        .flatten()
        .filter(|after| !after.is_empty());
    if let Some(after) = after {
        document.set_trailing("");
        let comments = after.trim_end();
        let mut table = toml_edit::Table::new();
        table.decor_mut().set_prefix(if comments.is_empty() {
            "\n".to_owned()
        } else {
            format!("{comments}\n\n")
        });
        document.insert(section, toml_edit::Item::Table(table));
    }
    let table = document
        .entry(section)
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or_else(|| failed(&target, format!("`{section}` is a section, not a value")))?;
    // A key that the file has keeps its comments, those above its line,
    // which are the key's, and the one after its value, which is the
    // value's.
    match table.get_mut(key) {
        Some(toml_edit::Item::Value(old)) => {
            let decor = old.decor().clone();
            *old = toml_edit::Value::from(value);
            *old.decor_mut() = decor;
        }
        _ => {
            table.insert(key, toml_edit::value(value));
        }
    }
    write_whole(&target, &document.to_string())
        .map_err(|error| failed(&target, format!("it cannot be written: {error}")))
}

/// Makes the config file at `path`, in the file that a link points to,
/// which stays a link, with its directory, when there is none: of the
/// [`Config::template`], every key commented out. Whether it made one; a
/// file that is there stays as it is.
pub fn make(path: &Path) -> Result<bool, Error> {
    let target = link_target(path);
    if fs::symlink_metadata(&target).is_ok() {
        return Ok(false);
    }
    write_whole(&target, &Config::template()).map_err(|error| Error {
        path: target.clone(),
        position: None,
        message: format!("it cannot be written: {error}"),
    })?;
    Ok(true)
}

/// How many links a config's path can lead through, each to the next.
const LINKS: usize = 32;

/// The file that `path` leads to, through its symbolic links: its target
/// when it is one that exists, and otherwise the last link's target, which
/// a write makes, so that a link stays a link.
fn link_target(path: &Path) -> PathBuf {
    if let Ok(target) = fs::canonicalize(path) {
        return target;
    }
    let mut path = path.to_owned();
    for _ in 0..LINKS {
        let Ok(target) = fs::read_link(&path) else {
            break;
        };
        // A relative target is the link's directory's.
        path = match path.parent() {
            Some(directory) => directory.join(target),
            None => target,
        };
    }
    path
}

/// Writes `text` to `path` whole: into a file of its own beside it, with
/// its permissions, on the disk, renamed over it, so that a reader never
/// sees half of it; a failure leaves no file of its own behind.
fn write_whole(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;

    let directory = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(directory)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = directory.join(format!(".{name}.{}.new", std::process::id()));
    let written = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        if let Ok(metadata) = fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
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
    pub(crate) fn at(text: &str, offset: usize) -> Self {
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
fn sections() -> [(&'static str, &'static [&'static str]); 16] {
    [
        ("font", keys::<FontConfig>()),
        ("text", keys::<TextConfig>()),
        ("cursor", keys::<CursorConfig>()),
        ("window", keys::<WindowConfig>()),
        ("interface", keys::<InterfaceConfig>()),
        ("slots", keys::<SlotsConfig>()),
        ("terminal", keys::<TerminalConfig>()),
        ("scrollback", keys::<ScrollbackConfig>()),
        ("keyboard", keys::<KeyboardConfig>()),
        ("clipboard", keys::<ClipboardConfig>()),
        ("links", keys::<LinksConfig>()),
        ("bell", keys::<BellConfig>()),
        ("theme", keys::<ThemeConfig>()),
        ("app", keys::<AppConfig>()),
        ("updates", keys::<UpdatesConfig>()),
        ("debug", keys::<DebugConfig>()),
    ]
}

/// A span of the config's text, which tells an error's line.
type Span = Option<std::ops::Range<usize>>;

/// Fails at the first binding of `[keys]` that is not one, at its own line:
/// keys that are not a combination or a sequence, or that another entry
/// names otherwise, as `opt+x` and `alt+x`; a command's name that is not
/// one, `true`, a binding without its command or with another field,
/// arguments that are not plain values, and a `when` that is not an
/// expression of the contexts; and a global key that is not one, or that
/// takes a `when`, `false` or several bindings.
fn check_bindings(root: &dyn TableLike) -> Result<(), (Span, String)> {
    let Some(item) = root.get("keys") else {
        return Ok(());
    };
    let Some(table) = item.as_table_like() else {
        return Err((item.span(), "`keys` is a section, not a value".to_owned()));
    };
    let mut named: Vec<(bool, Vec<crate::keymap::Combo>, &str)> = Vec::new();
    for (keys, value) in table.iter() {
        let span = table.key(keys).and_then(toml_edit::Key::span);
        let global = keys.strip_prefix(crate::keymap::GLOBAL);
        let combos = match global {
            Some(global) => crate::keymap::parse_global(global).map(|combo| vec![combo]),
            None => crate::keymap::parse_keys(keys),
        }
        .map_err(|message| (span.clone(), message))?;
        if let Some((.., other)) = named
            .iter()
            .find(|(of_all, named, _)| *of_all == global.is_some() && *named == combos)
        {
            return Err((
                span,
                format!("`{keys}` and `{other}` are the same keys, which one entry binds"),
            ));
        }
        named.push((global.is_some(), combos, keys));
        let at = value.span().or(span.clone());
        let fail = |message: String| (at.clone(), message);
        if global.is_some() {
            if let Some(command) = value.as_str() {
                check_command(command).map_err(fail)?;
            } else if let Some(binding) = value.as_table_like() {
                if binding.get("when").is_some() {
                    return Err(fail(format!(
                        "`{keys}` holds in every app, without a `when`"
                    )));
                }
                check_binding(binding, at)?;
            } else {
                return Err(fail(format!(
                    "`{keys}` takes a command or a binding: a global key has no default to take away, nor contexts"
                )));
            }
            continue;
        }
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
pub(crate) fn nearest(name: &str, known: &[&str], open: &str, close: &str) -> String {
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
    fn the_quick_terminal_s_old_section_is_ignored_and_told_at_its_line() {
        let config =
            parse("[font]\nsize = 14\n\n[quick-terminal]\nposition = \"left\"\nwhatever = 1\n")
                .unwrap();
        assert_eq!(config.font.size.points(), 14.0);
        let [(namespace, told)] = &config.moved[..] else {
            panic!("{:?}", config.moved);
        };
        assert_eq!(namespace, "quick-terminal");
        assert_eq!(told.position.map(|at| at.line), Some(4));
        assert!(
            told.message
                .contains("`[quick-terminal]` is now `[plugins.quick-terminal]`"),
            "{}",
            told.message
        );
        // A config without it tells nothing.
        assert!(parse("[font]\nsize = 14\n").unwrap().moved.is_empty());
    }

    #[test]
    fn a_window_s_directories_take_their_words_or_a_path() {
        let window = |text: &str| parse(&format!("[window]\n{text}\n")).unwrap().window;
        let read = window("start-directory = \"last\"\nnew-directory = \"home\"");
        assert_eq!(
            (read.start_directory, read.new_directory),
            (StartDirectory::Last, NewDirectory::Home)
        );
        let read = window("start-directory = \"~/a b\"\nnew-directory = \"/tmp\"");
        assert_eq!(
            read.start_directory,
            StartDirectory::Path("~/a b".to_owned())
        );
        assert_eq!(read.new_directory, NewDirectory::Path("/tmp".to_owned()));
        let default = WindowConfig::default();
        assert_eq!(
            (default.start_directory, default.new_directory),
            (StartDirectory::Home, NewDirectory::Inherit)
        );
        // A relative path is no directory's, nor another word.
        let (at, message) = error("[window]\nstart-directory = \"Projects\"\n");
        assert_eq!(at, Some((2, 19)));
        assert!(
            message.contains("`Projects` is no start directory"),
            "{message}"
        );
        let (_, message) = error("[window]\nnew-directory = \"last\"\n");
        assert!(message.contains("`last` is no new directory"), "{message}");
        // `~` is the home directory.
        let home = Path::new("/Users/me");
        assert_eq!(expanded("~", Some(home)), Some(home.to_owned()));
        assert_eq!(expanded("~/a", Some(home)), Some(home.join("a")));
        assert_eq!(expanded("/tmp", None), Some(PathBuf::from("/tmp")));
        assert_eq!(expanded("~/a", None), None);
    }

    #[test]
    fn the_web_s_search_engine_is_a_name_or_a_url_of_the_query() {
        let links = |text: &str| parse(&format!("[links]\n{text}\n")).map(|config| config.links);
        assert_eq!(
            links("").unwrap().search_engine.url("a b"),
            "https://www.google.com/search?q=a%20b"
        );
        let engine = links("search-engine = \"duckduckgo\"")
            .unwrap()
            .search_engine;
        assert_eq!(engine, SearchEngine::DuckDuckGo);
        // Every byte but a letter's, a digit's and `-._~`, escaped.
        assert_eq!(
            engine.url("ß & c++/x?~"),
            "https://duckduckgo.com/?q=%C3%9F%20%26%20c%2B%2B%2Fx%3F~"
        );
        for (name, engine) in ["google", "bing", "ecosia", "kagi", "startpage"]
            .into_iter()
            .zip(
                SearchEngine::NAMED
                    .into_iter()
                    .filter(|e| *e != SearchEngine::DuckDuckGo),
            )
        {
            let read = links(&format!("search-engine = \"{name}\"")).unwrap();
            assert_eq!(read.search_engine, engine);
            assert_eq!(String::from(engine), name);
        }
        let own = "https://example.org/find?q={query}&safe=1";
        let read = links(&format!("search-engine = \"{own}\"")).unwrap();
        assert_eq!(
            read.search_engine.url("x y"),
            "https://example.org/find?q=x%20y&safe=1"
        );
        assert_eq!(String::from(read.search_engine), own);
        // A URL without its query, of another scheme, and another word.
        for wrong in ["https://example.org/", "ftp://x/{query}", "yahoo"] {
            let (at, message) = error(&format!("[links]\nsearch-engine = \"{wrong}\"\n"));
            assert_eq!(at.map(|(line, _)| line), Some(2));
            assert!(message.contains("is no search engine"), "{message}");
        }
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
            columns = 80
            rows = 24
            remember-frame = false
            close-on-exit = "never"
            padding-x = 10
            padding-y = 2.5
            background-opacity = 0.85
            background-blur = 20
            title-size = false
            title-shortcut = false
            opaque-titlebar = true

            [interface]
            font-family = "Helvetica Neue"
            font-size = 15

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
            appearance = "dark"
            quit-after-last-window-closed = true
            "#,
        )
        .unwrap();
        assert_eq!(config.clipboard.read, ClipboardRead::Ask);
        assert_eq!(config.interface.font_family, name("Helvetica Neue"));
        assert_eq!(config.interface.font_size, Size::try_from(15.0).unwrap());
        assert_eq!(Config::default().interface.font_family, None);
        assert_eq!(Config::default().interface.font_size, Size::DEFAULT);
        let message = error("[interface]\nfont-size = 300\n").1;
        assert!(message.contains("from 4 to 200, not 300"), "{message}");
        // The slots' plugins, by their names, or none for the first.
        let slots = parse("[slots]\ntab-bar = \"lokyr/tabs\"\n").unwrap().slots;
        assert_eq!(slots.tab_bar.as_deref(), Some("lokyr/tabs"));
        assert_eq!(slots.pane_header, None);
        assert_eq!(Config::default().slots, SlotsConfig::default());
        let message = error("[slots]\ntab-bars = \"lokyr/tabs\"\n").1;
        assert!(message.contains("tab-bar"), "{message}");
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
        assert_eq!(config.app.appearance, AppearanceMode::Dark);
        assert_eq!(Config::default().app.appearance, AppearanceMode::System);
        assert!(config.app.quit_after_last_window_closed);
        assert!(!Config::default().app.quit_after_last_window_closed);
        for mode in [
            AppearanceMode::System,
            AppearanceMode::Light,
            AppearanceMode::Dark,
        ] {
            let text = format!("[app]\nappearance = \"{}\"\n", mode.name());
            let read = Config::parse(&text, Path::new("config.toml")).unwrap();
            assert_eq!(read.app.appearance, mode);
        }
        assert!(config.scrollback.scroll_on_output);
        assert!(!Config::default().scrollback.scroll_on_output);
        assert_eq!(config.app.icon, "laguz");
        assert_eq!(Config::default().app.icon, AppConfig::DEFAULT_ICON);
        assert_eq!(config.window.close_on_exit, CloseOnExit::Never);
        assert_eq!(config.window.padding_x.points(), 10.0);
        assert_eq!(config.window.padding_y.points(), 2.5);
        assert_eq!(config.window.background_opacity.value(), 0.85);
        assert_eq!(config.window.background_blur.points(), 20.0);
        assert!(!config.window.title_size && config.window.opaque_titlebar);
        assert!(!config.window.title_shortcut);
        assert_eq!(
            (config.window.columns.count(), config.window.rows.count()),
            (80, 24)
        );
        assert!(!config.window.remember_frame);
        let window = Config::default().window;
        assert!(window.title_size && window.title_shortcut && !window.opaque_titlebar);
        assert_eq!((window.columns.count(), window.rows.count()), (100, 30));
        assert!(window.remember_frame);
        for (text, wanted) in [
            ("columns = 0", "from 1 to 1000, not 0"),
            ("rows = 1001", "from 1 to 1000, not 1001"),
            ("columns = -3", "from 1 to 1000, not -3"),
        ] {
            let message = error(&format!("[window]\n{text}\n")).1;
            assert!(message.contains(wanted), "{text}: {message}");
        }
        // The blink's keys, out of their ranges and of their names.
        for (text, wanted) in [
            ("blink-interval = 50", "from 100 to 5000, not 50"),
            ("blink-timeout = 4000", "from 0 to 3600, not 4000"),
            ("fade-in = 20000", "from 0 to 10000, not 20000"),
            ("smooth-blink = \"pulse\"", "pulse"),
            ("fade-out-curve = \"bounce\"", "bounce"),
        ] {
            let message = error(&format!("[cursor]\n{text}\n")).1;
            assert!(message.contains(wanted), "{text}: {message}");
        }
        assert_eq!(config.keyboard.option_as_meta.sides(), (true, false));
        assert_eq!(
            config.cursor,
            CursorConfig {
                shape: Shape::Bar,
                blink: false,
                thickness: Some(Thickness::try_from(1.5).unwrap()),
                ..CursorConfig::default()
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
            // A global key of one key with a modifier, of a command alone.
            (
                "[keys]\n\"global:a\" = \"window.toggle-quick-terminal\"\n",
                2,
                "takes `cmd`, `ctrl` or `alt`",
            ),
            (
                "[keys]\n\"global:ctrl+a>b\" = \"window.toggle-quick-terminal\"\n",
                2,
                "is not a key",
            ),
            (
                "[keys]\n\"global:ctrl+~\" = \"window.toggle-quick-terminal\"\n",
                2,
                "takes Shift on the US layout",
            ),
            (
                "[keys]\n\"global:ctrl+`\" = false\n",
                2,
                "no default to take away",
            ),
            (
                "[keys]\n\"global:ctrl+`\" = { command = \"window.new\", when = \"selection\" }\n",
                2,
                "without a `when`",
            ),
            (
                "[keys]\n\"global:alt+x\" = \"tabs.new\"\n\"global:opt+x\" = \"tabs.next\"\n",
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
    fn the_plugins_sections_read_with_their_places_and_write_back() {
        let text = "[font]\nsize = 14\n\n[plugins.tabs]\nposition = \"top\"\ncolors = { active = \"red\", sizes = [1, 2.5] }\nwhen = 1979-05-27\n\n[plugins.links.look]\nunderline = true\n\n[[plugins.other.rows]]\nname = \"a\"\n";
        let config = parse(text).unwrap();
        let sections: Vec<(&str, serde_json::Value)> = config
            .plugins
            .sections()
            .map(|(namespace, section)| {
                (
                    namespace,
                    serde_json::Value::Object(section.values().clone()),
                )
            })
            .collect();
        assert_eq!(
            sections,
            [
                (
                    "links",
                    serde_json::json!({ "look": { "underline": true } })
                ),
                ("other", serde_json::json!({ "rows": [{ "name": "a" }] })),
                (
                    "tabs",
                    serde_json::json!({
                        "position": "top",
                        "colors": { "active": "red", "sizes": [1, 2.5] },
                        "when": "1979-05-27",
                    })
                ),
            ]
        );
        let tabs = config.plugins.get("tabs").unwrap();
        let place = |path: &[&str]| {
            let path: Vec<String> = path.iter().map(|&key| key.to_owned()).collect();
            tabs.place(&path).map(|place| (place.line, place.column))
        };
        assert_eq!(place(&[]), Some((4, 1)));
        assert_eq!(place(&["position"]), Some((5, 1)));
        assert_eq!(place(&["colors", "active"]), Some((6, 12)));
        assert_eq!(place(&["colors", "sizes", "1"]), Some((6, 40)));
        // A value without a place of its own tells its nearest key's.
        assert_eq!(place(&["colors", "missing"]), Some((6, 1)));
        assert_eq!(tabs.at(&["position".to_owned()]), "config.toml:5:1");
        // Written whole, the sections read back as the same, of other
        // places.
        let written = config.written(false);
        assert!(written.ends_with("\n[plugins.tabs]\ncolors = { active = \"red\", sizes = [1, 2.5] }\nposition = \"top\"\nwhen = \"1979-05-27\"\n"), "{written}");
        let again = parse(&written).unwrap();
        assert_eq!(again, config);
        assert_ne!(
            again.plugins.get("tabs").unwrap().place(&[]),
            tabs.place(&[])
        );
    }

    #[test]
    fn the_plugins_sections_are_tables_of_values_that_json_holds() {
        for (text, message) in [
            (
                "plugins = 1\n",
                "config.toml:1:11: `plugins` is a section, not a value",
            ),
            (
                "[plugins]\ntabs = 1\n",
                "config.toml:2:8: `plugins.tabs` is a plugin's section, of its keys, not a value",
            ),
            (
                "[plugins.tabs]\nfade = nan\n",
                "config.toml:2:8: `fade` is not a finite number",
            ),
            (
                "[plugins.tabs]\nsizes = [1, inf]\n",
                "config.toml:2:13: `sizes.1` is not a finite number",
            ),
        ] {
            assert_eq!(parse(text).unwrap_err().to_string(), message, "{text}");
        }
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
                "quick-terminal",
                "interface",
                "slots",
                "terminal",
                "scrollback",
                "keyboard",
                "clipboard",
                "links",
                "bell",
                "theme",
                "app",
                "updates",
                "debug",
                "keys",
                "plugins"
            ]
        );
        assert_eq!(keys::<ThemeConfig>(), ["light", "dark", "minimum-contrast"]);
        assert_eq!(keys::<LinksConfig>(), ["urls", "files", "search-engine"]);
        assert_eq!(
            keys::<ScrollbackConfig>(),
            ["scroll-on-output", "limit", "budget"]
        );
        assert_eq!(
            keys::<TerminalConfig>(),
            ["term", "unicode-widths", "ambiguous-width", "normalization"]
        );
        assert_eq!(
            keys::<ClipboardConfig>(),
            ["read", "confirm-paste", "copy-on-select"]
        );
        assert_eq!(
            keys::<BellConfig>(),
            ["sound", "flash", "bounce", "mark-title"]
        );
        assert_eq!(keys::<KeyboardConfig>(), ["option-as-meta"]);
        assert_eq!(
            keys::<AppConfig>(),
            [
                "icon",
                "language",
                "appearance",
                "quit-after-last-window-closed"
            ]
        );
        assert_eq!(OptionAsMeta::default().sides(), (false, false));
        assert_eq!(OptionAsMeta::Both.sides(), (true, true));
        assert_eq!(
            keys::<WindowConfig>(),
            [
                "columns",
                "rows",
                "remember-frame",
                "close-on-exit",
                "start-directory",
                "new-directory",
                "padding-x",
                "padding-y",
                "background-opacity",
                "background-blur",
                "title-size",
                "title-shortcut",
                "opaque-titlebar"
            ]
        );
        assert_eq!(
            keys::<CursorConfig>(),
            [
                "shape",
                "blink",
                "thickness",
                "blink-interval",
                "blink-timeout",
                "smooth-blink",
                "fade-in",
                "fade-in-curve",
                "visible",
                "fade-out",
                "fade-out-curve",
                "hidden"
            ]
        );
        let font = keys::<FontConfig>();
        assert_eq!(font.len(), 15);
        assert_eq!(
            keys::<TextConfig>(),
            ["bold", "italic", "ansi-colors", "bold-is-bright", "blink"]
        );
        assert_eq!(keys::<InterfaceConfig>(), ["font-family", "font-size"]);
        assert!(font.contains(&"bold-italic-style"));
        assert!(font.contains(&"non-ascii-family"));
        assert!(font.contains(&"builtin-powerline"));
        assert!(font.contains(&"system-weight"));
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
        // The plugins' sections hold their plugins' keys, which no built-in
        // plugin has yet: the example shows their form in a comment.
        // The sections that are plugins' now, which the example has in
        // theirs.
        let moved = |key: &str| MOVED.iter().any(|(section, _)| *section == key);
        for key in keys::<Config>()
            .iter()
            .filter(|key| **key != "plugins" && !moved(key))
        {
            assert!(root.contains_key(key), "`{key}` is in the complete example");
        }
        assert!(example.contains("\n# [plugins.<namespace>]\n"));
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
    fn a_key_set_in_the_file_keeps_the_rest_of_it() {
        let root = std::env::temp_dir().join(format!("lokyr-set-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        // Into a section that the file has, its comments kept.
        let file = root.join("config.toml");
        let text = "# mine\n[clipboard]\nread = \"ask\"   # asks\n\n[font]\nsize = 14\n";
        fs::write(&file, text).unwrap();
        set_string(&file, "clipboard", "confirm-paste", "never").unwrap();
        let written = fs::read_to_string(&file).unwrap();
        assert!(
            written.starts_with("# mine\n[clipboard]\nread = \"ask\"   # asks\n"),
            "{written}"
        );
        let config = Config::read(&file).unwrap();
        assert_eq!(config.clipboard.confirm_paste, ConfirmPaste::Never);
        assert_eq!(config.font.size.points(), 14.0);
        // Again, in place.
        set_string(&file, "clipboard", "confirm-paste", "always").unwrap();
        assert_eq!(
            fs::read_to_string(&file)
                .unwrap()
                .matches("confirm-paste")
                .count(),
            1
        );
        // Into a section that it lacks, after the comments at its end, and
        // an inline one.
        for (text, written) in [
            ("# mine\n", "# mine\n\n[links]\nurls = \"open\"\n"),
            (
                "[font]\nsize = 14\n\n# [bell]\n# sound = true\n",
                "[font]\nsize = 14\n\n# [bell]\n# sound = true\n\n[links]\nurls = \"open\"\n",
            ),
            ("\n\n", "\n[links]\nurls = \"open\"\n"),
            (
                "[font]\nsize = 14\n\n\n",
                "[font]\nsize = 14\n\n[links]\nurls = \"open\"\n",
            ),
        ] {
            fs::write(&file, text).unwrap();
            set_string(&file, "links", "urls", "open").unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), written, "{text:?}");
        }
        fs::write(&file, "[font]\nsize = 14\n").unwrap();
        set_string(&file, "clipboard", "confirm-paste", "never").unwrap();
        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .ends_with("[clipboard]\nconfirm-paste = \"never\"\n")
        );
        fs::write(&file, "clipboard = { read = \"ask\" }\n").unwrap();
        set_string(&file, "clipboard", "confirm-paste", "never").unwrap();
        let config = Config::read(&file).unwrap();
        assert_eq!(
            (config.clipboard.read, config.clipboard.confirm_paste),
            (ClipboardRead::Ask, ConfirmPaste::Never)
        );
        // A file that is not there, with its directory.
        let new = root.join("dir").join("config.toml");
        set_string(&new, "clipboard", "confirm-paste", "never").unwrap();
        assert_eq!(
            fs::read_to_string(&new).unwrap(),
            "config_version = 1\n\n[clipboard]\nconfirm-paste = \"never\"\n"
        );
        // Through a link, which stays one.
        let target = root.join("dotfiles.toml");
        fs::write(&target, "[font]\nsize = 14\n").unwrap();
        let link = root.join("linked.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        set_string(&link, "clipboard", "confirm-paste", "never").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::read_to_string(&target)
                .unwrap()
                .contains("confirm-paste")
        );
        // The permissions of the file are kept.
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        set_string(&link, "clipboard", "read", "ask").unwrap();
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // A file with an error is left as it is.
        fs::write(&file, "[font]\nsise = 14\n").unwrap();
        let error = set_string(&file, "clipboard", "confirm-paste", "never").unwrap_err();
        assert_eq!(error.position, Some(Position { line: 2, column: 1 }));
        assert_eq!(fs::read_to_string(&file).unwrap(), "[font]\nsise = 14\n");
        // And no file of its own is left beside it.
        let left: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().into_string().ok()?;
                name.ends_with(".new").then_some(name)
            })
            .collect();
        assert!(left.is_empty(), "{left:?}");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_key_set_again_keeps_its_comments() {
        let root = std::env::temp_dir().join(format!("lokyr-set-kept-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("config.toml");
        // The comments above the key, and the one after its value, as the
        // guide's example and the written config's documents have them.
        let text = concat!(
            "[clipboard]\n",
            "# Whether a paste with line breaks asks first.\n",
            "# auto | always | never\n",
            "confirm-paste = \"always\"   # I want to see them\n",
            "read = \"ask\"\n",
        );
        fs::write(&file, text).unwrap();
        set_string(&file, "clipboard", "confirm-paste", "never").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            text.replace("\"always\"", "\"never\"")
        );
        // In an inline table too.
        fs::write(
            &file,
            "clipboard = { read = \"ask\", confirm-paste = \"auto\" }\n",
        )
        .unwrap();
        set_string(&file, "clipboard", "confirm-paste", "never").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "clipboard = { read = \"ask\", confirm-paste = \"never\" }\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_link_to_a_config_not_there_yet_stays_a_link() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("lokyr-set-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        // An absolute link, and a relative one, to a file of a directory
        // that does not exist yet, as dotfiles not cloned yet.
        let absolute = root.join("absolute.toml");
        symlink(root.join("dotfiles/lokyr/config.toml"), &absolute).unwrap();
        let relative = root.join("relative.toml");
        symlink("other/config.toml", &relative).unwrap();
        for (link, target) in [
            (&absolute, root.join("dotfiles/lokyr/config.toml")),
            (&relative, root.join("other/config.toml")),
        ] {
            set_string(link, "links", "urls", "open").unwrap();
            assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
            assert!(
                fs::read_to_string(&target)
                    .unwrap()
                    .contains("urls = \"open\"")
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_config_is_made_of_the_template_where_there_is_none() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("lokyr-make-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        // With its directory, which is not there yet.
        let path = root.join("lokyr/config.toml");
        assert_eq!(make(&path), Ok(true));
        assert_eq!(fs::read_to_string(&path).unwrap(), Config::template());
        // A file that is there stays as it is.
        fs::write(&path, "[font]\nsize = 14\n").unwrap();
        assert_eq!(make(&path), Ok(false));
        assert_eq!(fs::read_to_string(&path).unwrap(), "[font]\nsize = 14\n");
        // Through a link to a file not there yet, which stays a link.
        let link = root.join("linked.toml");
        symlink(root.join("dotfiles/config.toml"), &link).unwrap();
        assert_eq!(make(&link), Ok(true));
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(root.join("dotfiles/config.toml")).unwrap(),
            Config::template()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_write_that_fails_leaves_no_file_of_its_own() {
        let root = std::env::temp_dir().join(format!("lokyr-set-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        // A directory with a file in it, which no file can be renamed
        // over.
        fs::create_dir_all(root.join("config.toml")).unwrap();
        fs::write(root.join("config.toml/inside"), "").unwrap();
        assert!(write_whole(&root.join("config.toml"), "x").is_err());
        let names: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["config.toml"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn links_read_what_a_url_and_a_file_do() {
        for (urls, url) in [("ask", OpenUrl::Ask), ("open", OpenUrl::Open)] {
            for (files, file) in [
                ("ask", OpenFile::Ask),
                ("open", OpenFile::Open),
                ("reveal", OpenFile::Reveal),
            ] {
                let config = parse(&format!(
                    "[links]\nurls = \"{urls}\"\nfiles = \"{files}\"\n"
                ))
                .unwrap();
                assert_eq!((config.links.urls, config.links.files), (url, file));
            }
        }
        let links = Config::default().links;
        assert_eq!((links.urls, links.files), (OpenUrl::Ask, OpenFile::Ask));
        let error = parse("[links]\nurls = \"reveal\"\n").unwrap_err();
        assert_eq!(error.position, Some(Position { line: 2, column: 8 }));
    }

    #[test]
    fn confirm_paste_reads_its_three_values() {
        for (value, confirm) in [
            ("auto", ConfirmPaste::Auto),
            ("always", ConfirmPaste::Always),
            ("never", ConfirmPaste::Never),
        ] {
            let config = parse(&format!("[clipboard]\nconfirm-paste = \"{value}\"\n")).unwrap();
            assert_eq!(config.clipboard.confirm_paste, confirm);
        }
        assert_eq!(
            Config::default().clipboard.confirm_paste,
            ConfirmPaste::Auto
        );
        let error = parse("[clipboard]\nconfirm-paste = \"ask\"\n").unwrap_err();
        assert_eq!(
            error.position,
            Some(Position {
                line: 2,
                column: 17
            })
        );
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

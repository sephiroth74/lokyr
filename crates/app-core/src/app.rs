//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Args, Category, Code, Command, Error, Kind, Param, Registry, Value};
use lokyr_kernel::config::{
    self, AppConfig, AppearanceMode, ClipboardRead, CloseOnExit, Config, ConfirmPaste,
    NewDirectory, OpenFile, OpenUrl, StartDirectory, Term,
};
use lokyr_kernel::diagnostics::{About, Diagnostic, Diagnostics};
use lokyr_kernel::keymap::{self, Binding, Context, Invocation, Keymap, When};
use lokyr_kernel::l10n::{self, Arg, Texts};
use lokyr_kernel::plugins::{self, Plugins, Request, WorkspaceEvent, host};
use lokyr_kernel::state;
use lokyr_kernel::themes;
#[cfg(test)]
use lokyr_kernel::workspace::RESIZE_PACE;
#[cfg(test)]
use lokyr_kernel::workspace::TOLD_PACE;
use lokyr_kernel::workspace::{
    self, Environment, Event, Exited, Grid, Pane, PaneId, Settings, TITLE_DASH, Workspace,
    directory_name, place_label,
};
use lokyr_platform_core::Layer;
use lokyr_platform_core::{
    AboutLine, AboutPanel, Alert, AlertId, AlertStyle, AppEvent, Appearance, GlobalKey, Handler,
    Platform, ScreenPoint, Size, TargetKind, TimerId, WindowAttributes, WindowEvent, WindowId,
    WindowSize,
};
use lokyr_pty::Program;
use lokyr_render_core::{Renderer, Surface as _};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::theme::{self as ui_theme, Theme};
use lokyr_vt_core::page::Budget;
use lokyr_vt_core::terminal::{self as vt, ClipboardRequest, Scroll};

use crate::drawing::{self, Backend, Drawing};
use crate::environment;
use crate::icon::Icons;
use crate::latency::{self, LatencyBench};
use crate::output_bench::{self, OutputBench};
use crate::performance::{self, Performance};
use crate::quick;
use crate::size_label;

use crate::watch::Watcher;
use crate::window::{Surfaces, Window};

mod access;
pub(crate) use access::TreeCursor;
mod asks;
pub(crate) mod chrome;
mod consent;
pub(crate) mod dividers;
mod folders;
pub(crate) mod frame;
mod input;
mod installed;
mod installs;
mod menus;
mod notices;
mod opens;
mod panels;
mod pickers;
mod restore;
pub(crate) mod scrollbars;
mod spawn;
mod surfaces;
mod tab_drag;
#[cfg(test)]
mod tests;
mod tools;

use input::{show_selection, shown_top};

/// The command that quits.
const QUIT: &str = "app.quit";

/// The command that opens the config file in the text editor, made first
/// when it is not there.
const SETTINGS: &str = "app.settings";

/// The command that opens the About panel: the app's version and build,
/// its copyright and its credits.
const ABOUT: &str = "app.about";

/// The command that checks for a newer version of the app, where it can
/// update itself.
const CHECK_UPDATES: &str = "app.check-for-updates";

/// The command that opens a new issue of the repository in the browser.
const REPORT_ISSUE: &str = "app.report-issue";

/// The repository, whose issues take the reports of problems.
const REPOSITORY: &str = "https://github.com/sephiroth74/lokyr";

/// The command that shows the performance's panel over the windows, or
/// hides it.
const PERFORMANCE: &str = "app.performance";

/// The command that turns Secure Keyboard Entry on for good, until the app
/// quits, or back to its own way, on at a password's prompt.
const SECURE_INPUT: &str = "app.secure-keyboard-entry";

/// The command that asks the system to make Lokyr its default terminal,
/// which its item, ticked and dimmed while it is, shows.
const DEFAULT_TERMINAL: &str = "app.make-default-terminal";

/// The command that links the command line tools, `lokyr` and `sleip`,
/// into a folder of the shells' `PATH`.
const INSTALL_TOOLS: &str = "app.install-command-line-tools";

/// What stands before the title of the window whose keys Secure Keyboard
/// Entry keeps from other apps.
const SECURE_MARK: &str = "🔒 ";

/// The command that opens a window.
const NEW_WINDOW: &str = "window.new";

/// The command that closes the focused window.
const CLOSE_WINDOW: &str = "window.close";

/// The command that asks for a title of the user's for the focused window.
const SET_TITLE: &str = "window.set-title";

/// Brings forward the window whose number is its `index`, from 1.
const GOTO_WINDOW: &str = "window.goto";

/// The command that keeps the window above the other applications'
/// windows, or puts it back among them, which the Window menu's item ticks.
const FLOAT_ON_TOP: &str = "window.toggle-float-on-top";

/// The command that shows the system's definition of the selection, of
/// one line, which the context menu alone shows, naming it.
const LOOK_UP: &str = "pane.look-up";

/// The characters of the selection that Look Up's item names, past which
/// it shortens it.
const LOOK_UP_TITLE: usize = 30;

/// The command that opens a window whose pane shows the man page that the
/// selection names, which the context menu alone shows, naming it.
const MAN_PAGE: &str = "pane.man-page";

/// The longest name of a man page, and of its section, that the man page's
/// command takes.
const MAN_PAGE_NAME: usize = 100;
const MAN_SECTION: usize = 8;

/// The command that opens a window whose pane lists the man pages that the
/// selection's words describe, as `man -k` finds them, which the context
/// menu alone shows, naming them.
const MAN_SEARCH: &str = "pane.man-search";

/// The most words of the selection that the man pages' search takes.
const MAN_KEYWORDS: usize = 8;

/// The command that searches the web for the selection, with the engine of
/// the config, which the context menu alone shows, naming it.
const SEARCH_WEB: &str = "pane.search-web";

/// The commands that the context menu alone shows, as they name the
/// selection.
const CONTEXT_ONLY: [&str; 4] = [LOOK_UP, MAN_PAGE, MAN_SEARCH, SEARCH_WEB];

/// The commands that give the app an appearance, as the View menu's
/// Appearance chooses it, each with its own and its title's message.
const APPEARANCES: [(&str, AppearanceMode, &str); 3] = [
    (
        "app.appearance-system",
        AppearanceMode::System,
        "appearance-system",
    ),
    (
        "app.appearance-light",
        AppearanceMode::Light,
        "appearance-light",
    ),
    (
        "app.appearance-dark",
        AppearanceMode::Dark,
        "appearance-dark",
    ),
];

/// The lines of a pasted text that its confirmation shows, and the
/// characters of each.
const PREVIEW: (usize, usize) = (3, 60);

/// The share of a refresh within which a step of a live resize that drew
/// does not draw again: steps a refresh apart draw each, and steps of a
/// display of twice the events' rate every other one.
const STEP_SHARE: f64 = 0.5;

/// How deep the commands that plugins run from their commands go, past
/// which the next does not run, as a plugin that runs its own command
/// would go on forever.
const COMMANDS_DEEP: usize = 16;

/// How many commands one chain of commands that ran commands runs at most,
/// past the first, as a plugin's command that runs itself twice would run
/// twice as often at each step, 2^16 times within the depth.
const COMMANDS_WIDE: usize = 256;

/// How many chains of commands the app keeps the runs of, the newest, as a
/// worker's chain goes on over turns; and those under way, whatever their
/// age.
const CHAINS_KEPT: usize = 64;

/// The catalogs of the texts that the app shows, the kernel's and its own,
/// by language.
pub(crate) const CATALOGS: [(&str, &str); 4] = [
    l10n::KERNEL[0],
    l10n::KERNEL[1],
    ("en", include_str!("../resources/locales/en/app.ftl")),
    ("it", include_str!("../resources/locales/it/app.ftl")),
];

/// The size of a new window's content, in points.
const WINDOW_SIZE: Size = Size {
    width: 800.0,
    height: 500.0,
};

/// How long after a window's last frame of output its title looks once
/// more at the program in its foreground, for one that started without a
/// word.
const TITLE_LOOK: Duration = Duration::from_millis(250);

/// The columns and the rows of a man page's window: `man` lays its pages
/// out for 80 columns.
const MAN_PAGE_CELLS: (u32, u32) = (80, 48);

/// How long the app waits after the last change of the config file, for
/// the changes that come with it, as an editor's write and rename do, before
/// it reads the file once.
const SETTLE: Duration = Duration::from_millis(100);

/// How often the app looks for the status of a program whose output ended.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// The cell of a window that the app does not draw, in pixels.
const UNDRAWN_CELL: (u32, u32) = (8, 16);

/// How long a window's flash for its bell lasts.
const FLASH: Duration = Duration::from_millis(100);

/// How often the view scrolls under a selection that the pointer drags past
/// the grid's top or bottom.
const SELECTION_SCROLL: Duration = Duration::from_millis(50);

/// The most rows that the view scrolls a step under a selection, as the
/// pointer goes farther past the grid.
const SELECTION_SCROLL_ROWS: usize = 8;

/// The opacity of the default text's color over a window as it flashes.
const FLASH_OPACITY: f32 = 0.2;

/// How long after a bell that acted another in the same window does
/// nothing, as a program can ring many at once.
const BELL_PAUSE: Duration = Duration::from_millis(200);

/// How long after a notification that showed another of the same window
/// shows nothing, as a program can send many at once.
const NOTIFICATION_PAUSE: Duration = Duration::from_secs(2);

/// The characters of a notification's title and of its text that show.
const NOTIFICATION_TITLE: usize = 256;
const NOTIFICATION_BODY: usize = 2048;

/// The `TERM` that a window takes in place of the terminal's own when the
/// app has no entry of it to give its programs.
const FALLBACK_TERM: &str = "xterm-256color";

/// The title of a window whose program gives none, and what stands before
/// the title of a window whose bell rang in the background.
const TITLE: &str = "Lokyr";
const BELL_MARK: &str = "🔔 ";

/// How long the startup's and the latency's benches wait for a frame on
/// the display, as one that sleeps, or a machine without one, shows none.
const BENCH_DEADLINE: Duration = Duration::from_secs(15);

/// How long a start that the system made to open files, folders or links
/// waits for what it opens to open a window, before it opens a new one: a
/// theme's file opens none. A start for a service is a default one, whose
/// new window opens beside the service's own.
const UNTITLED_WAIT: Duration = Duration::from_secs(1);

/// How long a frame starts looking up the faces of characters that it has
/// not met, and how long it draws the glyphs that the atlas lacks, on the
/// font's threads: a frame of new glyphs takes up to about 10 ms, within a
/// refresh at 60 Hz, and a page of them shows whole in a few frames.
const GLYPHS_BUDGET: (Duration, Duration) = (Duration::from_millis(2), Duration::from_millis(2));

/// The folder of the bundle's resources of the themes that the app holds.
const HELD_THEMES: &str = "themes";

/// Checks `config` as the app takes it, without applying it: its font,
/// whose failure fails the whole config, and its icon and its language,
/// which the log tells of when they are not Lokyr's, and whose defaults
/// then show.
pub fn check<B: Backend>(config: &Config) -> Result<(), String> {
    drawing::font::<B>(config.font.clone())?;
    texts(&config.app.language, &[]);
    let resources = environment::resources();
    let own = Environment {
        terminfo: environment::terminfo(resources.as_deref()),
        ..Environment::default()
    };
    if let Some(text) = undescribed(&config.terminal.term, &own, &programs_terminfo_dirs()) {
        eprintln!("lokyr: {text}");
    }
    if let Some(icons) = Icons::find() {
        variant(&icons.variants(), &config.app.icon);
    }
    Ok(())
}

/// Where the programs that the app starts look for terminfo entries, of
/// the app's home and `TERMINFO_DIRS`, which they inherit.
fn programs_terminfo_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    environment::terminfo_dirs(
        home.as_deref(),
        std::env::var_os("TERMINFO_DIRS").as_deref(),
    )
}

/// What is wrong with `term`, if no terminfo entry describes it where the
/// programs look, of `dirs`, nor the terminal's own entry, which
/// `environment` names: its programs take a terminal that they know
/// nothing of for a dumb one, whose line editors draw a line wrongly.
fn undescribed(term: &Term, environment: &Environment, dirs: &[PathBuf]) -> Option<String> {
    let term = term.as_str();
    let own = matches!(term, "xterm-lokyr" | "lokyr") && environment.terminfo.is_some();
    (!own && !environment::describes(dirs, term)).then(|| {
        format!(
            "no terminfo entry describes `{term}`, the `term` of `[terminal]`, where programs look for one, so they take the terminal for a dumb one, whose line editing draws wrongly: `xterm-256color`, the default, is described everywhere"
        )
    })
}

/// The variant of `variants` that `wanted` names; for a name that is none
/// of theirs, the default one, as the log tells.
fn variant<'a>(variants: &[String], wanted: &'a str) -> &'a str {
    if variants.iter().any(|variant| variant == wanted) {
        wanted
    } else {
        eprintln!(
            "lokyr: the icon `{wanted}` is not one of Lokyr's: {}; the default one shows",
            variants.join(", ")
        );
        AppConfig::DEFAULT_ICON
    }
}

/// What a pane's session wakes the main thread for: that it changed, which
/// the main thread takes, and how many times it woke it.
#[derive(Debug, Default)]
struct PaneWake {
    woken: AtomicBool,
    wakes: AtomicU64,
}

impl PaneWake {
    /// The session changed, from its threads.
    fn wake(&self) {
        self.woken.store(true, Ordering::Release);
        self.wakes.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether it changed since the last take.
    fn take(&self) -> bool {
        self.woken.swap(false, Ordering::AcqRel)
    }
}

/// A file of the config's folder, the config file or the decisions on the
/// plugins' capabilities, and its watch.
#[derive(Debug)]
struct ConfigFile {
    path: PathBuf,
    watcher: Option<Watcher>,
    /// Whether the file may have changed, which the watch's thread tells.
    changed: Arc<AtomicBool>,
    /// The timer of the reading, which waits for the changes that come
    /// together.
    reading: Option<TimerId>,
}

impl ConfigFile {
    /// Watches the file, or its directory's parent while its directory does
    /// not exist, so that its changes wake `platform`.
    fn watch(&mut self, platform: &dyn Platform) {
        let (changed, waker) = (Arc::clone(&self.changed), platform.waker());
        self.watcher = Watcher::new(&self.path, move || {
            changed.store(true, Ordering::Release);
            waker.wake();
        })
        .inspect_err(|error| eprintln!("lokyr: {error}"))
        .ok();
    }
}

/// The files of the themes in force, and those that their names looked
/// for, there or not, whose changes load the themes again.
#[derive(Debug, Default)]
struct ThemeFiles {
    paths: Vec<PathBuf>,
    /// The watches of the paths, each with its path.
    watchers: Vec<(PathBuf, Watcher)>,
    /// Whether a file may have changed, which the watches' threads tell.
    changed: Arc<AtomicBool>,
    /// The timer of the loading, which waits for the changes that come
    /// together.
    reading: Option<TimerId>,
}

impl ThemeFiles {
    /// Watches `paths`, unless it watches them already, so that their
    /// changes wake `platform`: anew where a watch went stale, as of a
    /// folder that came since, or of a link to another file.
    fn watch(&mut self, platform: &dyn Platform, paths: Vec<PathBuf>) {
        let stale = self
            .watchers
            .iter()
            .any(|(path, watcher)| watcher.is_stale(path));
        if paths == self.paths && !stale {
            return;
        }
        self.watchers = paths
            .iter()
            .filter_map(|path| {
                let (changed, waker) = (Arc::clone(&self.changed), platform.waker());
                Watcher::new(path, move || {
                    changed.store(true, Ordering::Release);
                    waker.wake();
                })
                .inspect_err(|error| eprintln!("lokyr: {error}"))
                .ok()
                .map(|watcher| (path.clone(), watcher))
            })
            .collect();
        self.paths = paths;
    }
}

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug)]
pub struct App<B: Backend> {
    drawing: Option<Drawing<B>>,
    theme: Theme,
    /// The files of the themes, which their changes load again.
    theme_files: ThemeFiles,
    /// The folder of the themes that the app holds, its bundle's
    /// `Resources/themes`, when it runs from its bundle.
    held_themes: Option<PathBuf>,
    /// The themes that it holds, as a picker lists them, read once.
    held_theme_list: std::cell::OnceCell<Vec<lokyr_kernel::plugins::host::ThemeInfo>>,
    /// The theme that a plugin's picker previews in place of the
    /// appearance in force's, if one does.
    theme_preview: Option<String>,
    /// The program that a new window runs.
    program: Program,
    /// The program that shows man pages, in place of the login shell's
    /// `man`, as the tests give one.
    man: Option<PathBuf>,
    /// The program that an `ssh` link runs, `ssh` as the system finds it,
    /// or the tests' own.
    ssh: PathBuf,
    /// The config in force.
    config: Config,
    /// The file of the config, if it has one.
    config_file: Option<ConfigFile>,
    /// The texts that the app shows, in the language shown.
    texts: Texts,
    /// The user's languages, as the system has them, from the first.
    preferred: Vec<String>,
    /// The commands, which the app and the plugins run.
    commands: Registry,
    /// The built-in plugins.
    plugins: Plugins,
    /// The bindings of keys to commands: the app's, the plugins', and the
    /// config's.
    keymap: Keymap,
    /// The size of the text that a plugin gave, in points, in place of the
    /// config's.
    font_size: Option<f32>,
    /// The windows, their tabs and panes.
    workspace: Workspace,
    /// The platform's windows, each with what the app keeps of it.
    windows: BTreeMap<WindowId, Window<B>>,
    /// The timer of the next change of a blinking cursor, and its time.
    blink_timer: Option<(TimerId, Instant)>,
    /// The timer of the end of the first label of a grid's size that ends,
    /// and its time.
    label_timer: Option<(TimerId, Instant)>,
    /// The timer of the soonest tooltip of a slot's view that waits to
    /// show, and its time.
    tooltip_timer: Option<(TimerId, Instant)>,
    /// The timer of the turn that delivers the plugins' events that their
    /// calls left.
    deliver_timer: Option<TimerId>,
    /// The timer of the next look for the status of a program whose output
    /// ended.
    exit_timer: Option<TimerId>,
    /// The timer past which the startup's and the latency's benches give up
    /// waiting for a frame on the display.
    bench_deadline: Option<TimerId>,
    /// The questions of the scripts that came from the internet, which run
    /// once the user says so, by their alerts.
    script_asks: BTreeMap<AlertId, PathBuf>,
    /// The question of the command line tools' links that replace what is
    /// in their folder, and the links, until the user answers.
    tools_ask: Option<(AlertId, Vec<lokyr_platform_core::ToolLink>)>,
    /// Where the command line tools' links go, and where their binaries
    /// are.
    tools_places: tools::Places,
    /// The panes of the scripts that the system opened with the app, which
    /// keep their screens as their programs end, whatever `close-on-exit`
    /// says.
    scripts: BTreeSet<PaneId>,
    /// The timer past which a start that the system made to open files,
    /// folders or links opens a new window, if what it opened opened none.
    untitled_timer: Option<TimerId>,
    /// The timer of the next plugin that faulted to start again, and its
    /// time.
    restart_timer: Option<(TimerId, Instant)>,
    /// Where the plugins that Sleip installed are, which the app adds as it
    /// starts; none for an app of the built-in plugins alone.
    installed: Option<installed::Installed>,
    /// What the app keeps of the installed plugins as they change: their
    /// watches, those loaded and refused, and the reload's timer.
    reloads: installed::Reloads,
    /// The user's decisions on their capabilities, as last read.
    decisions: lokyr_pkg_model::permissions::Permissions,
    /// The file of those decisions, and its watch.
    permissions_file: Option<ConfigFile>,
    /// The questions of the capabilities of the plugins that activate, each
    /// plugin and its capabilities, by their alerts.
    consent_asks: BTreeMap<AlertId, (String, Vec<lokyr_kernel::commands::Capability>)>,
    /// The timer of the next of the plugins' timers to fire, and its time:
    /// none while no plugin's timer waits.
    plugin_timer: Option<(TimerId, Instant)>,
    /// The timer of the turn of the panes whose sessions told something
    /// within the pace of their last look, and its time.
    told_timer: Option<(TimerId, Instant)>,
    /// What each pane's session wakes the main thread for, which tells the
    /// panes that changed.
    pane_wakes: BTreeMap<PaneId, Arc<PaneWake>>,
    /// The events of the slots' views' nodes that their layouts made, as a
    /// list's rows ask for their items, for their plugins as the turn ends.
    ui_events: Vec<(String, plugins::ui::UiEvent)>,
    /// Whether a frame asked plugins for their slots' views, whose calls'
    /// requests the turn's end takes.
    rendered_slots: bool,
    /// The timer of the next size that a program learns, at its pace, and
    /// its time.
    resize_timer: Option<(TimerId, Instant)>,
    /// The timer of the end of the windows' flashes.
    flash_timer: Option<TimerId>,
    /// The timer of the windows' accessibility trees, while one waits.
    access_timer: Option<TimerId>,
    /// The timer of the next step of the views that scroll under a
    /// selection dragged past their grid.
    scroll_timer: Option<TimerId>,
    /// The timer of the start of the next fade of the panes' scroll bars,
    /// and when it fires, while one waits.
    bar_timer: Option<(TimerId, Instant)>,
    /// Whether Command is held, as the last key told, while which the
    /// pointer's moves go to the plugins that subscribed to them.
    command_held: bool,
    /// What the plugins were told last of the pointer, and the modifiers
    /// held then.
    pointer_told: Option<PointerTold>,
    pointer_mods: lokyr_platform_core::Modifiers,
    /// Whether the frame drawn changed the text under the pointer that the
    /// plugins were told of, as its row changed or its pane scrolled under
    /// it, of which they are told again.
    pointer_text_changed: bool,
    /// Whether a window opened since the app started, after which none
    /// restores the frame that a window kept, nor starts in the first
    /// window's directory.
    opened_one: bool,
    /// The directory of the app's state, which keeps the directory of the
    /// last window that closed, and the windows open as the app quits;
    /// `None` keeps nothing, as in the tests.
    state: Option<PathBuf>,
    /// The window that took the focus last, which is the one in front as
    /// the app quits, though the app is not active.
    front: Option<WindowId>,
    /// What went wrong that the user is to learn of: the log, and the
    /// notices that the windows show.
    diagnostics: Diagnostics,
    /// Whether the user turned Secure Keyboard Entry on for good, from the
    /// menu, and whether it is on, as the platform has it.
    secure_chosen: bool,
    secure_on: bool,
    /// The timer of the windows' titles' last look at the programs in
    /// their foreground, after the frames of a burst of output, with the
    /// time that it is for; and how long after a window's last frame of
    /// output that look comes, `None` for none.
    title_timer: Option<(TimerId, Instant)>,
    title_look: Option<Duration>,
    /// The icon's variants, where the app finds them.
    icons: Option<Icons>,
    /// The variant that the app made the icon, once it made one.
    icon_shown: Option<String>,
    /// The programs' reads of the clipboard that the user is asked about,
    /// by their alerts.
    clipboard_asks: BTreeMap<AlertId, (PaneId, ClipboardRequest)>,
    /// The pastes that the user is asked to confirm, by their alerts.
    paste_asks: BTreeMap<AlertId, (PaneId, String)>,
    /// Whether the user asked that pastes be not confirmed, where the
    /// config could not keep it: until the app quits.
    pastes_unconfirmed: bool,
    /// The links that the user is asked about before they open, by their
    /// alerts: the window that the question is on, the URL or the path,
    /// and what it is.
    open_asks: BTreeMap<AlertId, (Option<WindowId>, (String, TargetKind))>,
    /// The themes' files that the user is asked to install, by their
    /// alerts.
    theme_asks: BTreeMap<AlertId, installs::ThemeInstall>,
    /// The contexts in which the menus' commands that tell one can run, and
    /// whether the platform has each one's items enabled, as the focused
    /// pane's last frame had it.
    menu_contexts: Vec<(String, When)>,
    menu_enabled: BTreeMap<String, bool>,
    /// The plugins' items of the menus, titled, which the menus' items name
    /// by their places here.
    menu_items: Vec<lokyr_kernel::plugins::MenuEntry>,
    /// The keymap that the plugins last heard of, as `keys-changed` told
    /// them, which a keymap made anew of the same bindings does not tell
    /// again.
    keys_told: Option<Keymap>,
    /// The window that the last context menu opened on, whose pane marks
    /// the cell that it opened on, which its commands of a cell take.
    context_window: Option<WindowId>,
    /// The window whose pane the commands run in, in place of the focused
    /// one, while a context menu's do.
    acting: Option<WindowId>,
    /// What the user chose that links do, where the config could not keep
    /// it: until the app quits.
    links_chosen: (Option<OpenUrl>, Option<OpenFile>),
    /// The appearance that the user chose from the menu, where the config
    /// could not keep it: until the app quits, or the config's key changes.
    appearance_chosen: Option<AppearanceMode>,
    /// The performance's panel, while it shows, with what it measures; and
    /// whether the user chose from the menu to show it, until the app quits
    /// or the config's key changes.
    performance: Option<Performance>,
    performance_chosen: Option<bool>,
    /// The runs of each chain of commands that ran commands, of the newest
    /// chains, by their ids, and the chains whose commands run now, one
    /// within another.
    chains: (BTreeMap<u64, usize>, Vec<u64>),
    /// Whether the cursors and the text that blinks blink as on layers of
    /// their own without them, as the tests' fake platform, which draws
    /// nothing, shows them.
    layers: bool,
    /// The windows whose closing the user is asked to confirm, by their
    /// alerts.
    close_asks: BTreeMap<AlertId, WindowId>,
    /// The windows whose title the user is asked for, by their alerts.
    title_asks: BTreeMap<AlertId, WindowId>,
    /// The tabs and the panes, of their windows, that plugins close, whose
    /// closing the user is asked to confirm, by their alerts.
    part_asks: BTreeMap<AlertId, (WindowId, plugins::Closing)>,
    /// The texts that plugins ask the user for, on their windows, with the
    /// plugin and the question's id, by their alerts.
    text_asks: BTreeMap<AlertId, (WindowId, String, u64)>,
    /// Where the windows of the workspace that a plugin's move of a tab
    /// opened go on the screens, until they show in the platform's.
    placing: BTreeMap<workspace::WindowId, ScreenPoint>,
    /// The size of the content of the window that a tab dropped out of
    /// every window opens, as its window's was, below the titlebar, until
    /// the window shows.
    placing_size: Option<(workspace::TabId, Size)>,
    /// A tab dragged out of its bar, or back in it, until its drop.
    tab_drag: Option<tab_drag::TabDrag<B>>,
    /// The question before quitting, while it waits for its answer.
    quit_ask: Option<AlertId>,
    /// Whether the config in force is the defaults, as the start's file did
    /// not apply.
    config_fell_back: bool,
    /// The updates' checks now and then, on or off, as the platform has
    /// them.
    update_checks: bool,
    /// Whether the system asked to quit, and waits for the answer.
    system_quit: bool,
    /// The environment of the programs that the app starts, which the
    /// start completes with the user's locale.
    environment: Environment,
    /// Where the programs look for the terminfo entry of their `TERM`.
    terminfo_dirs: Vec<PathBuf>,
    /// The memory that the windows' scrollbacks share, which the config
    /// sizes.
    budget: Arc<Budget>,
    /// The quick windows, each with the place that it showed at last, and
    /// whether it shows.
    quick: BTreeMap<WindowId, (lokyr_platform_core::QuickPlace, bool)>,
    /// The config's global keys that the Mac's keyboards have, by the
    /// indices that their presses tell, with the commands that they run,
    /// those that another app holds too.
    global_keys: Vec<(keymap::Combo, Invocation)>,
    /// The measure of the startup, with `--bench startup`: when the first
    /// window's first frame showed, once it did, which the app prints
    /// before it quits.
    startup_bench: Option<Arc<Mutex<StartupBench>>>,
    /// The latency's bench, with `--bench latency`.
    latency_bench: Option<LatencyBench>,
    /// Whether the app was started as a bench, of its startup, its
    /// latency or its output, whatever a bench's end empties: a bench's app
    /// neither keeps nor restores the user's windows, takes no global key,
    /// checks for no update, and quits as its last window closes.
    benched: bool,
    /// The output's bench, with `--bench output`.
    output_bench: Option<OutputBench>,
    /// The size of the output bench's window's content, in points, that
    /// `--window` gives.
    bench_size: Option<Size>,
}

/// The measure of the startup.
#[derive(Debug, Default)]
struct StartupBench {
    /// Whether a frame will tell when it shows.
    asked: bool,
    /// When the first frame showed.
    shown: Option<std::time::SystemTime>,
}

/// What the plugins were told of the pointer: its pane, its cell there,
/// if any, and whether Command was held.
type PointerTold = (PaneId, Option<(u16, i64)>, bool);

impl<B: Backend> App<B> {
    /// An application that draws its windows with `drawing`, or nothing
    /// without it, with `config`, which it reads again from `path`, its
    /// file, when that changes.
    pub fn new(mut drawing: Option<Drawing<B>>, config: Config, path: Option<PathBuf>) -> Self {
        // The interface's font, of the plugins' views.
        if let Some(drawing) = &mut drawing {
            drawing.set_interface(&config.interface);
        }
        let integrated = config.terminal.shell_integration;
        let mut plugins = Plugins::new();
        // The distribution's plugins, whose manifests read, as the build
        // checked them.
        for registration in registrations() {
            if let Err(error) = plugins.add_registered(registration) {
                eprintln!("lokyr: a built-in plugin's manifest does not read: {error}");
            }
        }
        // A view in every slot, for a build of tests.
        #[cfg(feature = "sample-slots")]
        if let Err(error) = plugins.add_builtin(
            crate::sample::MANIFEST,
            Box::new(crate::sample::SampleSlots::default()),
        ) {
            eprintln!("lokyr: a built-in plugin's manifest does not read: {error}");
        }
        // A plugin on a thread of its own that renders without pause, for a
        // build of tests.
        #[cfg(feature = "sample-worker")]
        if let Err(error) = plugins.add_builtin_send(
            crate::sample_worker::MANIFEST,
            Box::new(crate::sample_worker::SampleWorker::default()),
        ) {
            eprintln!("lokyr: a built-in plugin's manifest does not read: {error}");
        }
        let commands = commands(&Texts::new(l10n::SOURCE, &CATALOGS), &plugins);
        let keymap = Keymap::with_config(bindings(&plugins, &commands), &config.keys);
        let budget = Arc::new(Budget::new(config.scrollback.budget.bytes()));
        Self {
            drawing,
            theme: Theme::builtin(),
            theme_files: ThemeFiles::default(),
            held_themes: environment::resources().map(|resources| resources.join(HELD_THEMES)),
            held_theme_list: std::cell::OnceCell::new(),
            theme_preview: None,
            program: Program::LoginShell,
            man: None,
            ssh: PathBuf::from("ssh"),
            config,
            config_file: path.map(|path| ConfigFile {
                path,
                watcher: None,
                changed: Arc::new(AtomicBool::new(false)),
                reading: None,
            }),
            commands,
            texts: Texts::new(l10n::SOURCE, &CATALOGS),
            preferred: Vec::new(),
            plugins,
            keymap,
            font_size: None,
            workspace: Workspace::new(),
            windows: BTreeMap::new(),
            blink_timer: None,
            label_timer: None,
            tooltip_timer: None,
            deliver_timer: None,
            exit_timer: None,
            bench_deadline: None,
            untitled_timer: None,
            restart_timer: None,
            installed: None,
            reloads: installed::Reloads::default(),
            decisions: lokyr_pkg_model::permissions::Permissions::default(),
            permissions_file: None,
            consent_asks: BTreeMap::new(),
            plugin_timer: None,
            scripts: BTreeSet::new(),
            script_asks: BTreeMap::new(),
            tools_ask: None,
            tools_places: tools::Places::default(),
            told_timer: None,
            pane_wakes: BTreeMap::new(),
            ui_events: Vec::new(),
            rendered_slots: false,
            resize_timer: None,
            flash_timer: None,
            access_timer: None,
            scroll_timer: None,
            bar_timer: None,
            command_held: false,
            pointer_told: None,
            pointer_mods: lokyr_platform_core::Modifiers::NONE,
            pointer_text_changed: false,
            title_timer: None,
            title_look: Some(TITLE_LOOK),
            opened_one: false,
            state: None,
            front: None,
            diagnostics: Diagnostics::default(),
            secure_chosen: false,
            secure_on: false,
            icons: Icons::find(),
            icon_shown: None,
            clipboard_asks: BTreeMap::new(),
            paste_asks: BTreeMap::new(),
            pastes_unconfirmed: false,
            open_asks: BTreeMap::new(),
            theme_asks: BTreeMap::new(),
            links_chosen: (None, None),
            appearance_chosen: None,
            performance: None,
            performance_chosen: None,
            chains: (BTreeMap::new(), Vec::new()),
            layers: false,
            menu_contexts: Vec::new(),
            menu_enabled: BTreeMap::new(),
            menu_items: Vec::new(),
            keys_told: None,
            context_window: None,
            acting: None,
            close_asks: BTreeMap::new(),
            title_asks: BTreeMap::new(),
            part_asks: BTreeMap::new(),
            text_asks: BTreeMap::new(),
            placing: BTreeMap::new(),
            placing_size: None,
            tab_drag: None,
            quit_ask: None,
            config_fell_back: false,
            update_checks: false,
            system_quit: false,
            environment: {
                let resources = environment::resources();
                let scripts = environment::shell_integration(resources.as_deref());
                Environment {
                    terminfo: environment::terminfo(resources.as_deref()),
                    resources,
                    binaries: environment::binaries(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    shell_integration: scripts.filter(|_| integrated),
                    ..Environment::default()
                }
            },
            terminfo_dirs: programs_terminfo_dirs(),
            budget,
            quick: BTreeMap::new(),
            global_keys: Vec::new(),
            startup_bench: None,
            latency_bench: None,
            benched: false,
            output_bench: None,
            bench_size: None,
        }
    }

    /// The app whose programs learn `version` as Lokyr's, in place of the
    /// crate's: the app's own, with the date of its commit.
    pub fn with_version(mut self, version: &str) -> Self {
        self.environment.version = version.to_owned();
        self
    }

    /// The app that measures its startup, from the process's start to its
    /// first window's first frame on the display, prints it as JSON, and
    /// quits.
    pub fn with_startup_bench(mut self) -> Self {
        self.startup_bench = Some(Arc::default());
        self.benched = true;
        self
    }

    /// The app that measures the latency of its keys, from each press to
    /// the frame that shows what it typed, in a window whose program
    /// echoes them, as a shell does, and prints it as JSON as it quits:
    /// of `keys` that it types itself, and then quits, or of the user's;
    /// with `flood`, beside a window that floods its output.
    pub fn with_latency_bench(mut self, keys: usize, flood: bool) -> Self {
        self.program = Program::command("/bin/sh", ["-c", "stty raw -echo; exec cat"]);
        self.latency_bench = Some(LatencyBench {
            keys,
            flood,
            ..LatencyBench::default()
        });
        self.benched = true;
        self
    }

    /// The app that measures its frames' time on the CPU, by their phases,
    /// as `program`'s output draws in a window of `size` points, or of the
    /// config's columns and rows without one, and prints it as JSON, and
    /// quits, as the program ends.
    pub fn with_output_bench(mut self, program: Program, size: Option<Size>) -> Self {
        self.program = program;
        self.output_bench = Some(OutputBench::default());
        self.bench_size = size;
        self.benched = true;
        self
    }

    /// Ends the output's bench, and the app, failing, when its program
    /// could not start, which then tells no exit.
    fn end_output_bench_unstarted(&mut self, platform: &mut dyn Platform) {
        if self.output_bench.is_none() {
            return;
        }
        let started = self.windows.values().any(|state| {
            self.workspace
                .focused(state.window)
                .is_some_and(lokyr_kernel::workspace::Pane::started)
        });
        if !started {
            eprintln!("lokyr: the output's program cannot start, and so no frames to measure");
            crate::BENCH_FAILED.store(true, Ordering::Relaxed);
            self.output_bench = None;
            platform.quit();
        }
    }

    /// Prints the output's report, of `window`'s pixels, once.
    fn end_output_bench(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(bench) = self.output_bench.take() else {
            return;
        };
        let pixels = platform
            .window_size(window)
            .map_or((0, 0), |size| (size.pixels.width, size.pixels.height));
        println!("{}", output_bench::report(&bench.frames, pixels));
    }

    /// Starts the latency's bench, as its window opened: the window that
    /// floods, if it has one, and the first key a moment after, for the
    /// programs to start.
    fn start_latency_bench(&mut self, platform: &mut dyn Platform) {
        let measured = self.windows.keys().next().copied();
        let Some(bench) = &mut self.latency_bench else {
            return;
        };
        bench.window = measured;
        let flood = bench.flood;
        if flood {
            let program = Program::command("/bin/sh", ["-c", "exec yes 'flood flood flood flood'"]);
            self.open_window_running(platform, &program, None, None);
            if let Some(measured) = measured {
                platform.focus_window(measured);
            }
        }
        if let Some(bench) = &mut self.latency_bench
            && bench.keys > 0
        {
            bench.timer = Some(platform.start_timer(Instant::now() + Duration::from_millis(1500)));
        }
    }

    /// The bench's next key, when the last one showed or was given up; or
    /// its end, once it typed them all.
    fn latency_step(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let Some(bench) = &mut self.latency_bench else {
            return;
        };
        bench.timer = None;
        if bench
            .typed_at
            .is_some_and(|typed| now.duration_since(typed) > latency::GIVE_UP)
        {
            bench.pending = None;
            bench.typed_at = None;
        }
        let waiting = bench.pending.is_some() || bench.typed_at.is_some();
        if bench.sent < bench.keys {
            if !waiting && let Some(window) = bench.window {
                let (code, letter) = latency::key_of(bench.sent);
                platform.post_key(window, code, &letter.to_string());
                bench.typed_at = Some(now);
                bench.sent += 1;
            }
            let pause = latency::pause(bench.sent);
            bench.timer = Some(platform.start_timer(now + pause));
            return;
        }
        if waiting {
            bench.timer = Some(platform.start_timer(now + Duration::from_millis(50)));
            return;
        }
        self.end_latency_bench(platform);
        platform.quit();
    }

    /// Prints the latency's report, once; of keys that it typed and none
    /// showed in a frame whose time the display told, as on a display that
    /// tells none, it says so.
    fn end_latency_bench(&mut self, platform: &mut dyn Platform) {
        let Some(bench) = self.latency_bench.take() else {
            return;
        };
        let refresh = bench
            .window
            .and_then(|window| platform.refresh_interval(window))
            .unwrap_or(Duration::from_micros(16_667));
        let samples = bench
            .samples
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if bench.keys > 0 && samples.is_empty() {
            eprintln!("lokyr: no frame showed on the display, and so no latency to measure");
        }
        println!(
            "{}",
            latency::report(&samples, refresh, bench.flood, bench.keys)
        );
    }

    /// Ends the startup's bench, and the latency's of the keys that it types,
    /// as the display showed no frame of them in [`BENCH_DEADLINE`], as on
    /// a display asleep or a machine without one: each says so, the
    /// latency's report of no samples, and the app quits. The latency of
    /// the user's own keys, or of keys that showed, goes on.
    fn give_up_benches(&mut self, platform: &mut dyn Platform) {
        let seconds = BENCH_DEADLINE.as_secs();
        if self.startup_bench.take().is_some() {
            eprintln!(
                "lokyr: no frame showed on the display in {seconds} s, and so no startup to measure"
            );
            platform.quit();
            return;
        }
        let shown = self.latency_bench.as_ref().is_none_or(|bench| {
            bench.keys == 0
                || !bench
                    .samples
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .is_empty()
        });
        if !shown {
            eprintln!("lokyr: no frame showed on the display in {seconds} s");
            self.end_latency_bench(platform);
            platform.quit();
        }
    }

    /// Prints the startup's measure and quits, once the first frame
    /// showed; quits at once without a drawing, which shows no frame.
    fn end_startup_bench(&mut self, platform: &mut dyn Platform) {
        let Some(bench) = &self.startup_bench else {
            return;
        };
        if self.drawing.is_none() {
            eprintln!("lokyr: no frame shows without a GPU, and so no startup to measure");
            self.startup_bench = None;
            platform.quit();
            return;
        }
        let (asked, shown) = {
            let bench = bench.lock().unwrap_or_else(PoisonError::into_inner);
            (bench.asked, bench.shown)
        };
        let Some(shown) = shown else {
            // The display skipped the frame: the windows draw again, whole.
            if !asked {
                for (&window, state) in &mut self.windows {
                    state.shown = None;
                    platform.request_frame(window);
                }
            }
            return;
        };
        let started = platform.process_started();
        let Some(elapsed) = started.and_then(|started| shown.duration_since(started).ok()) else {
            eprintln!("lokyr: the process's start cannot be read, and so the startup");
            self.startup_bench = None;
            platform.quit();
            return;
        };
        println!(
            "{{\"pid\":{},\"first_frame_ms\":{:.1}}}",
            std::process::id(),
            elapsed.as_secs_f64() * 1000.0
        );
        self.startup_bench = None;
        platform.quit();
    }

    /// The app, with `plugin` of `manifest`, as a built-in one, after the
    /// others.
    #[cfg(test)]
    fn with_builtin(mut self, manifest: &str, plugin: Box<dyn plugins::Plugin>) -> Self {
        let before = self.tab_bar_providers();
        if let Err(error) = self.plugins.add_builtin(manifest, plugin) {
            eprintln!("lokyr: {error}");
        }
        self.take_tab_bar(&before);
        self
    }

    /// The plugins' names.
    #[cfg(test)]
    fn tab_bar_providers(&self) -> Vec<String> {
        self.plugins.names().map(str::to_owned).collect()
    }

    /// A test's plugin of a tab bar, added since `before`, of the plugins'
    /// names, shows it, as the config would choose it, unless the config
    /// chooses another.
    #[cfg(test)]
    fn take_tab_bar(&mut self, before: &[String]) {
        let added = self.plugins.names().find(|name| {
            !before.iter().any(|known| known == name)
                && self.plugins.manifest(name).is_some_and(|manifest| {
                    manifest
                        .contributes
                        .slots
                        .iter()
                        .any(|slot| slot == "tab-bar")
                })
        });
        if self.config.slots.tab_bar.is_none() {
            self.config.slots.tab_bar = added.map(str::to_owned);
        }
    }

    /// The app, with the plugin of `registration`, as a built-in one of a
    /// distribution, after the others.
    #[cfg(test)]
    fn with_registration(mut self, registration: plugins::Registration) -> Self {
        if let Err(error) = self.plugins.add_registered(registration) {
            eprintln!("lokyr: {error}");
        }
        self
    }

    /// The app, with `plugin` of `manifest`, as a built-in one of the
    /// thread that its manifest asks for, after the others.
    #[cfg(test)]
    fn with_builtin_send(
        mut self,
        manifest: &str,
        plugin: Box<dyn plugins::Plugin + Send>,
    ) -> Self {
        let before = self.tab_bar_providers();
        if let Err(error) = self.plugins.add_builtin_send(manifest, plugin) {
            eprintln!("lokyr: {error}");
        }
        self.take_tab_bar(&before);
        self
    }

    /// The app, with `plugin`, by its `name`, after the built-in ones.
    #[cfg(test)]
    fn with_plugin(mut self, name: &str, plugin: Box<dyn plugins::Plugin>) -> Self {
        if let Err(error) = self.plugins.add(name, plugin) {
            eprintln!("lokyr: {error}");
        }
        self
    }

    /// The app, whose notices start with those of `diagnostics`, as a
    /// config that does not read as the app starts.
    pub fn with_diagnostics(mut self, diagnostics: Diagnostics) -> Self {
        // A notice of the config at the start tells that the config in
        // force is the defaults, of a file that did not apply.
        self.config_fell_back = diagnostics.tells(&About::Config);
        self.diagnostics = diagnostics;
        self
    }

    /// The app, which keeps the directory of the last window that closed in
    /// `state`, its state's directory, for the next start.
    pub fn with_state(mut self, state: Option<PathBuf>) -> Self {
        self.state = state;
        self
    }

    /// The app, which adds the plugins that Sleip installed as it starts:
    /// those of the lockfile `lock`, of their packages in `store`.
    pub fn with_installed(mut self, lock: PathBuf, store: lokyr_pkg_model::store::Store) -> Self {
        let permissions = lock.with_file_name(lokyr_pkg_model::permissions::FILE);
        self.installed = Some(installed::Installed {
            lock,
            store,
            permissions,
        });
        self
    }

    /// The app, whose windows run `program` rather than the login shell;
    /// its titles take no last look after a burst of output, whose timer
    /// the tests of the other timers would count, but where a test gives
    /// them one.
    #[cfg(test)]
    fn with_program(mut self, program: Program) -> Self {
        self.program = program;
        self.title_look = None;
        self
    }

    /// The app, whose programs find its resources in `resources`.
    #[cfg(test)]
    fn with_resources(mut self, resources: Option<PathBuf>) -> Self {
        self.environment.resources = resources;
        self
    }

    /// The pane of `window`.
    #[cfg(test)]
    fn pane(&self, window: WindowId) -> &Pane {
        let shown = self.windows[&window].window;
        self.workspace.focused(shown).expect("the window's pane")
    }

    /// The pane that has `window`'s keyboard: its active tab's active one.
    fn focused(&self, window: WindowId) -> Option<PaneId> {
        self.workspace
            .active_pane(self.windows.get(&window)?.window)
    }

    /// The app, whose icon's variants are `icons`.
    #[cfg(test)]
    fn with_icons(mut self, icons: Option<Icons>) -> Self {
        self.icons = icons;
        self
    }

    /// Reads the config file again: a valid config applies whole, and an
    /// invalid one changes nothing, as the log tells.
    fn reload(&mut self, platform: &mut dyn Platform) {
        let Some(file) = &mut self.config_file else {
            return;
        };
        if file
            .watcher
            .as_ref()
            .is_none_or(|watcher| watcher.is_stale(&file.path))
        {
            file.watch(platform);
        }
        match Config::read(&file.path) {
            Ok(config) => self.apply(platform, config),
            Err(error) => {
                let text =
                    format!("{error}; none of the config applies, and the one in force stays");
                let diagnostic = Diagnostic::new(Code::InvalidConfig, text, About::Config);
                self.report(platform, diagnostic);
            }
        }
    }

    /// Checks the plugins' sections of the config against their schemas,
    /// which tell what they refused and ignored in notices of their own,
    /// in place of those of the config before.
    fn configure_plugins(&mut self, platform: &mut dyn Platform) {
        let notices = self.plugins.configure(&self.config.plugins);
        if self
            .diagnostics
            .resolve_all(|about| matches!(about, About::PluginConfig(_)))
        {
            self.request_frames(platform);
        }
        for notice in notices {
            let diagnostic = Diagnostic::new(
                Code::InvalidConfig,
                notice.text,
                About::PluginConfig(notice.namespace),
            );
            self.report(platform, diagnostic);
        }
        // The sections that are plugins' now, which the file still has.
        for (namespace, told) in self.config.moved.clone() {
            let diagnostic = Diagnostic::new(
                Code::InvalidConfig,
                told.to_string(),
                About::PluginConfig(namespace),
            );
            self.report(platform, diagnostic);
        }
    }

    /// Takes the notice about `about` away, as a fix does.
    fn resolve(&mut self, platform: &mut dyn Platform, about: &About) {
        if self.diagnostics.resolve(about) {
            self.request_frames(platform);
        }
    }

    /// Loads the themes that `[theme]` names, of the themes' folder beside
    /// the config file or built in: they take the place of those in force,
    /// whose notice goes, the panes and the views taking their colors; an
    /// error tells itself, with its line, and the themes in force stay. The
    /// files looked for are watched, those of an error too, so that the
    /// file that fixes it loads.
    fn load_themes(&mut self, platform: &mut dyn Platform) {
        let directory = self
            .config_file
            .as_ref()
            .and_then(|file| themes::directory(&file.path));
        let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
        let config = &self.config.theme;
        let mut paths = Vec::new();
        let mut resolved = Vec::new();
        let mut errors = Vec::new();
        // A picker's preview in place of the appearance in force's.
        let mut names = [&config.light, &config.dark];
        if let Some(preview) = &self.theme_preview {
            match platform.appearance() {
                Appearance::Light => names[0] = preview,
                Appearance::Dark => names[1] = preview,
            }
        }
        for name in names {
            match themes::resolve(
                name,
                directory.as_deref(),
                self.held_themes.as_deref(),
                &builtins,
            ) {
                Ok(theme) => {
                    paths.extend(theme.files);
                    resolved.push(theme.theme);
                }
                Err(error) => {
                    // The file of the name, there or not, and the one of the
                    // error; none of a name that names no file of the
                    // folder.
                    if let Some(directory) = directory.as_ref().filter(|_| themes::is_name(name)) {
                        paths.push(directory.join(format!("{name}.toml")));
                        paths.push(directory.join(format!("{name}.itermcolors")));
                        if error.path.starts_with(directory) {
                            paths.push(error.path.clone());
                        }
                    }
                    errors.push(error);
                }
            }
        }
        paths.sort();
        paths.dedup();
        self.theme_files.watch(platform, paths);
        if let Some(error) = errors.into_iter().next() {
            let text = format!("{error}; the themes in force stay");
            let diagnostic = Diagnostic::new(Code::InvalidConfig, text, About::Theme);
            self.report(platform, diagnostic);
            return;
        }
        self.resolve(platform, &About::Theme);
        let theme = Theme::of(&resolved[0], &resolved[1]);
        if theme != self.theme {
            self.theme = theme;
            let appearance = platform.appearance();
            self.appearance_changed(platform, appearance);
        }
    }

    /// Tells what the font lacks of the config's, or takes the notice of
    /// what it lacked away.
    fn tell_font(&mut self, platform: &mut dyn Platform) {
        let Some(drawing) = &self.drawing else {
            return;
        };
        match drawing.missing() {
            Some(text) => self.report(
                platform,
                Diagnostic::new(Code::Unavailable, text, About::Font),
            ),
            None => self.resolve(platform, &About::Font),
        }
    }

    /// Tells that no terminfo entry describes the `TERM` of the config,
    /// where the programs look, or takes the notice of it away.
    fn tell_term(&mut self, platform: &mut dyn Platform) {
        match undescribed(
            &self.config.terminal.term,
            &self.environment,
            &self.terminfo_dirs,
        ) {
            Some(text) => {
                self.report(
                    platform,
                    Diagnostic::new(Code::Unavailable, text, About::Term),
                );
            }
            None => self.resolve(platform, &About::Term),
        }
    }

    /// Takes `config` in place of the one in force, whole, in every window;
    /// or nothing of it, if its font fails.
    fn apply(&mut self, platform: &mut dyn Platform, config: Config) {
        if config == self.config {
            // The plugins' sections of the same values moved, as lines above
            // them came or went: their notices tell their new places.
            let check = config.updates.check;
            let moved = !config.plugins.same_places(&self.config.plugins);
            self.config.plugins = config.plugins;
            if moved {
                self.configure_plugins(platform);
            }
            self.resolve(platform, &About::Config);
            self.settle_update_checks(platform, check);
            return;
        }
        let font_changed = config.font != self.config.font;
        if font_changed
            && let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(sized(&config.font, self.font_size))
        {
            let text = format!("{error}; none of the config applies, and the one in force stays");
            let diagnostic = Diagnostic::new(Code::InvalidConfig, text, About::Config);
            self.report(platform, diagnostic);
            return;
        }
        self.resolve(platform, &About::Config);
        if font_changed {
            self.tell_font(platform);
        }
        if config.interface != self.config.interface
            && let Some(drawing) = &mut self.drawing
        {
            drawing.set_interface(&config.interface);
            for &window in self.windows.keys() {
                platform.request_frame(window);
            }
        }
        let icon_changed = config.app.icon != self.config.app.icon;
        let language_changed = config.app.language != self.config.app.language;
        let appearance_changed = config.app.appearance != self.config.app.appearance;
        let keys_changed = config.keys != self.config.keys;
        let shortcut_changed = config.window.title_shortcut != self.config.window.title_shortcut;
        let remember_changed = config.window.remember_frame != self.config.window.remember_frame;
        let performance_changed =
            config.debug.performance_overlay != self.config.debug.performance_overlay;
        let term_changed = config.terminal.term != self.config.terminal.term;
        let themes_changed = config.theme.light != self.config.theme.light
            || config.theme.dark != self.config.theme.dark;
        self.settle_update_checks(platform, config.updates.check);
        if keys_changed {
            self.keymap =
                Keymap::with_config(bindings(&self.plugins, &self.commands), &config.keys);
        }
        let plugins_changed =
            config.plugins != self.config.plugins || config.moved != self.config.moved;
        // The shells' integration, as the config has it, of the panes that
        // start from now.
        if config.terminal.shell_integration != self.config.terminal.shell_integration {
            self.environment.shell_integration = config
                .terminal
                .shell_integration
                .then(|| environment::shell_integration(self.environment.resources.as_deref()))
                .flatten();
        }
        self.config = config;
        // The plugins take their sections, and hear of their changes; those
        // that their sections turn off stop, and those turned on again
        // start.
        if plugins_changed {
            self.configure_plugins(platform);
            for error in self.with_plugins(platform, Plugins::activate) {
                eprintln!("lokyr: {error}");
            }
            self.after_plugins(platform);
        }
        if term_changed {
            self.tell_term(platform);
        }
        if themes_changed {
            self.load_themes(platform);
        }
        // The open windows keep their frames, or not, as the user moves or
        // resizes them from now; a man page's never does.
        if remember_changed {
            for (&window, state) in &self.windows {
                if self.quick.contains_key(&window) {
                    continue;
                }
                let remember = self.config.window.remember_frame && state.page.is_none();
                platform.set_window_remember(window, remember);
            }
        }
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        // The open windows take the settings for their output to come;
        // their programs keep the `TERM` that they started with.
        let terminal = &self.config.terminal;
        let widths = terminal.widths();
        self.budget.set_limit(self.config.scrollback.budget.bytes());
        for pane in self.workspace.panes() {
            pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
            pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
            pane.set_width_tables(widths.tables, widths.ambiguous_wide);
            pane.set_normalization(terminal.normalization.form());
            pane.set_scrollback_limit(self.config.scrollback.limit.bytes());
        }
        if icon_changed {
            self.show_icon(platform);
        }
        if appearance_changed {
            self.appearance_chosen = None;
            self.show_appearance(platform);
        }
        if performance_changed {
            self.performance_chosen = None;
            self.show_performance(platform);
        }
        if language_changed {
            self.texts = texts(&self.config.app.language, &self.preferred);
            self.commands = commands(&self.texts, &self.plugins);
            self.set_menu(platform);
        } else if keys_changed || shortcut_changed {
            // The windows' keys too, which the titlebar shows as the config
            // says.
            self.set_menu(platform);
        }
        if keys_changed {
            self.take_global_keys(platform);
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// Opens the About panel, the platform's own of the app's version, build
    /// and copyright, with its credits: the license, the repository, and,
    /// in a bundle, the notices of the licenses of what it includes.
    fn show_about(&mut self, platform: &mut dyn Platform) {
        let mut credits = vec![
            AboutLine {
                text: self.texts.text("about-license"),
                link: None,
            },
            AboutLine {
                text: self.texts.text("about-repository"),
                link: platform.resolve_target(REPOSITORY).ok(),
            },
        ];
        let licenses = self
            .environment
            .resources
            .as_ref()
            .map(|resources| resources.join("licenses"))
            .filter(|licenses| licenses.is_dir());
        if let Some(licenses) = licenses {
            credits.push(AboutLine {
                text: self.texts.text("about-licenses"),
                link: platform.resolve_target(&licenses.to_string_lossy()).ok(),
            });
        }
        platform.show_about(&AboutPanel { credits });
    }

    /// Opens a new issue of the repository in the browser, which tells
    /// the app's version and the system's, and asks what happened.
    fn report_issue(&mut self, platform: &mut dyn Platform) {
        let url = issue_url(&self.environment.version, &platform.system_version());
        let opened = platform
            .resolve_target(&url)
            .and_then(|target| platform.open(&target));
        if let Err(error) = opened {
            eprintln!("lokyr: the page of a new issue does not open, {error}");
        }
    }

    /// Starts the app's updates, where it can update itself, with the
    /// config's checks now and then, once the first window has opened, as
    /// the updater loads out of its way; the menu's item is dimmed where it
    /// cannot. The benches leave the updater alone, as its checks would
    /// reach the network while they measure.
    fn start_updates(&mut self, platform: &mut dyn Platform) {
        let benched = self.benched;
        // Off where the config in force is the defaults of a file that did
        // not apply, which may have turned them off.
        self.update_checks = self.config.updates.check && !self.config_fell_back;
        let can = !benched && platform.start_updates(self.update_checks);
        platform.set_command_enabled(CHECK_UPDATES, can);
    }

    /// Has the updates' checks now and then follow `check`, of a config
    /// that applied, if they do not.
    fn settle_update_checks(&mut self, platform: &mut dyn Platform, check: bool) {
        if self.update_checks != check {
            self.update_checks = check;
            platform.set_update_checks(check);
        }
    }

    /// Shows the icon's variant that the config names, or the default one
    /// for a name that is no variant's, as the log tells; nothing when that
    /// variant shows already, as the bundle's own does at the start, or
    /// when the app found no variants, which the log tells too.
    fn show_icon(&mut self, platform: &mut dyn Platform) {
        let Some(icons) = &self.icons else {
            eprintln!(
                "lokyr: no icon's variants found, neither in the bundle nor in the source tree; the icon `{}` cannot show",
                self.config.app.icon
            );
            return;
        };
        let variant = variant(&icons.variants(), &self.config.app.icon);
        let shown = self
            .icon_shown
            .as_deref()
            .or_else(|| icons.bundled().then_some(AppConfig::DEFAULT_ICON));
        if shown == Some(variant) {
            return;
        }
        match platform.set_app_icon(icons.file(variant).as_deref()) {
            Ok(()) => self.icon_shown = Some(variant.to_owned()),
            Err(error) => eprintln!("lokyr: {error}"),
        }
    }

    /// Shows the performance's panel over every window, or hides it, as the
    /// menu chose, else as the config says, and ticks the menu's item: while
    /// it shows, its timer measures each second, which wakes the app, and
    /// draws every window once more.
    fn show_performance(&mut self, platform: &mut dyn Platform) {
        let shown = self
            .performance_chosen
            .unwrap_or(self.config.debug.performance_overlay);
        platform.set_command_checked(PERFORMANCE, shown);
        match (shown, self.performance.take()) {
            (true, None) => {
                let now = Instant::now();
                // The GPU's times from before it shows are not its own.
                if let Some(drawing) = &mut self.drawing {
                    drawing.renderer.take_gpu_times();
                }
                let windows: Vec<WindowId> = self.windows.keys().copied().collect();
                let bytes = self.output_bytes();
                let mut shown = Performance::new(now, &windows, platform.usage(), bytes);
                shown.timer = Some(platform.start_timer(now + performance::INTERVAL));
                self.performance = Some(shown);
            }
            (true, Some(shown)) => self.performance = Some(shown),
            (false, Some(hidden)) => {
                if let Some(timer) = hidden.timer {
                    platform.cancel_timer(timer);
                }
            }
            (false, None) => {}
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// The performance's timer fired: it measures the second that went, a
    /// window's lines each, and asks every window for the frame that shows
    /// them.
    fn measure_performance(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let bytes = self.output_bytes();
        let gpu = self
            .drawing
            .as_mut()
            .map(|drawing| drawing.renderer.take_gpu_times())
            .unwrap_or_default();
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        let Some(shown) = &mut self.performance else {
            return;
        };
        let measures = shown.measure(now, &windows, gpu, platform.usage(), bytes);
        shown.keep(&windows);
        for (window, measure) in measures {
            eprintln!("{}", performance::log_line(window.get(), &measure));
            shown.set_lines(window, performance::lines(&measure, &self.texts));
            platform.request_frame(window);
        }
        shown.timer = Some(platform.start_timer(now + performance::INTERVAL));
    }

    /// The bytes that each pane's program wrote, by the pane's number.
    fn output_bytes(&self) -> BTreeMap<u64, u64> {
        self.workspace
            .panes()
            .map(|pane| (pane.id().get(), pane.output_bytes()))
            .collect()
    }

    /// Opens the config file in the system's text editor, made first, when
    /// it is not there, of every key commented out with its default and its
    /// text; an alert tells why it cannot, as without a home directory,
    /// which gives it no path.
    fn open_settings(&mut self, platform: &mut dyn Platform) {
        let failed = match &self.config_file {
            None => Some(self.texts.text("settings-no-home")),
            Some(file) => config::make(&file.path)
                .err()
                .map(|error| error.to_string())
                .or_else(|| {
                    platform
                        .open_in_text_editor(&file.path)
                        .err()
                        .map(|error| error.to_string())
                }),
        };
        if let Some(why) = failed {
            self.tell_settings_failed(platform, why);
        }
    }

    /// Tells, in an alert of the window that the commands run in, or of the
    /// one that has the focus, that Settings… opens nothing, and `why`, as
    /// the log does.
    fn tell_settings_failed(&mut self, platform: &mut dyn Platform, why: String) {
        eprintln!("lokyr: {why}");
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("settings-failed-message"),
            detail: why,
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        platform.show_alert(window, &alert);
    }

    fn open_window(&mut self, platform: &mut dyn Platform) {
        let program = self.program.clone();
        self.open_window_running(platform, &program, None, None);
    }

    /// Opens a window, whose pane runs `program`, and quits if it cannot
    /// while no other window is open: of the man page of the title `page`,
    /// as `man ls`, of its size and colors, when it shows one; in the
    /// folder `directory`, if given, or a new window's. The window, if it
    /// opened.
    fn open_window_running(
        &mut self,
        platform: &mut dyn Platform,
        program: &Program,
        page: Option<String>,
        directory: Option<PathBuf>,
    ) -> Option<WindowId> {
        // Of its cells, at the scale of the screen that it opens on; a man
        // page's of its own, which it neither keeps nor restores.
        let window_config = &self.config.window;
        let cells = match page {
            Some(_) => MAN_PAGE_CELLS,
            None => (
                u32::from(window_config.columns.count()),
                u32::from(window_config.rows.count()),
            ),
        };
        // The output bench's window takes its own size, or the config's
        // grid, never the frame that the last window left.
        let remember =
            window_config.remember_frame && page.is_none() && self.output_bench.is_none();
        let size = self
            .bench_size
            .unwrap_or_else(|| self.size_of(platform.screen_scale(), cells));
        let environment = Environment {
            directory: directory.or_else(|| self.start_directory(platform)),
            ..self.environment.clone()
        };
        let attributes = WindowAttributes {
            title: page.clone().unwrap_or_else(|| TITLE.to_owned()),
            size,
            remember,
            // The first window as the app starts; the others take the size
            // that a window kept, cascaded.
            restore: remember && !self.opened_one,
            restore_size: remember && self.opened_one,
            frame: None,
        };
        self.opened_one = true;
        match platform.create_window(&attributes) {
            Ok(window) => {
                self.run_in(platform, window, (program, page), &environment, None);
                Some(window)
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                if self.windows.is_empty() && self.quits_without_windows() {
                    platform.quit();
                }
                None
            }
        }
    }

    /// Gives the platform's new `window` a pane that runs `program` in
    /// `environment`, of the man page of the title `page`, as `man ls`, of
    /// its colors, when it shows one; and draws it. A quick window is the
    /// workspace's of the id `quick`.
    fn run_in(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (program, page): (&Program, Option<String>),
        environment: &Environment,
        quick: Option<workspace::WindowId>,
    ) {
        let size = platform
            .window_size(window)
            .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
        let grid = self.grid(size);
        let colors = match page {
            Some(_) => self.theme.man_page.colors.clone(),
            None => self.colors(platform.appearance()),
        };
        let pane = self.spawn_pane(platform, program, environment, grid, &colors);
        let shown = match quick {
            Some(id) => self.workspace.open_quick_window(pane, id),
            None => self.workspace.open_window(pane),
        };
        self.attach(platform, window, shown, page);
    }

    /// Has the platform's new `window` show `shown`, a window of the
    /// workspace, of the man page of the title `page` when it shows one;
    /// and draws it.
    fn attach(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        shown: workspace::WindowId,
        page: Option<String>,
    ) {
        let surfaces = Surfaces {
            window: self.surface(platform, window),
            text: self.layer_surface(platform, window, Layer::BlinkingText),
            cursor: self.layer_surface(platform, window, Layer::Cursor),
        };
        let focused = platform.focused_window() == Some(window);
        let size = platform
            .window_size(window)
            .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
        let mut state = Window::new(surfaces, shown, focused, Instant::now());
        state.page = page;
        state.keyboard = self.workspace.focused(shown).map(Pane::id);
        // The lowest number that no other open window has; the quick
        // terminal's has none.
        let quick = self.quick.contains_key(&window);
        let number = (!quick).then(|| self.free_number());
        self.workspace.number_window(shown, number, None);
        self.windows.insert(window, state);
        self.window_keys(platform);
        // Its tab's folder and program, which its program, started, has
        // already, for the first frame's title to be the one that stays: as
        // the frame looks after its output, of the program alone while its
        // folder cannot be read, as `login`'s, which runs as root. A label
        // that changed after the frame drew the slots' views would draw
        // them again a refresh later, as the window shows, which puts its
        // showing off by a refresh.
        self.look_at_place(window);
        self.draw(platform, window, size);
    }

    /// The lowest number, from 1, that no open window has.
    fn free_number(&self) -> u32 {
        let taken: BTreeSet<u32> = self
            .windows
            .values()
            .filter_map(|state| self.workspace.window(state.window)?.number())
            .collect();
        (1..).find(|number| !taken.contains(number)).unwrap_or(1)
    }

    /// Gives each numbered window the keys that bring it forward, as the
    /// keymap binds `window.goto` of its number and the platform writes
    /// them, which the plugins read and its titlebar shows, a bar of tabs
    /// there too; none while the config hides them from the titlebar.
    fn window_keys(&mut self, platform: &mut dyn Platform) {
        let shown = self.config.window.title_shortcut;
        let mut changed = Vec::new();
        for (&window, state) in &self.windows {
            let Some(number) = self
                .workspace
                .window(state.window)
                .and_then(workspace::Window::number)
            else {
                continue;
            };
            let args = Args::from([("index".to_owned(), Value::Integer(i64::from(number)))]);
            let keys = self
                .keymap
                .shortcut_with(GOTO_WINDOW, &args)
                .and_then(menus::shortcut)
                .filter(|_| shown)
                .map(|shortcut| platform.shortcut_text(shortcut));
            let told = self
                .workspace
                .window(state.window)
                .and_then(workspace::Window::keys);
            if told != keys.as_deref() {
                self.workspace
                    .number_window(state.window, Some(number), keys);
                changed.push(window);
            }
        }
        for window in changed {
            self.show_window_keys(platform, window);
        }
    }

    /// Shows the keys that bring `window` forward at the right of its
    /// titlebar of its title, unless the config hides them; a titlebar of
    /// tabs leaves them to the bar's plugin. It takes no frame.
    pub(super) fn show_window_keys(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let keys = self
            .workspace
            .window(state.window)
            .and_then(workspace::Window::keys)
            .filter(|_| state.titlebar != lokyr_platform_core::Titlebar::Tabs);
        if keys != state.accessory.as_deref() {
            platform.set_title_accessory(window, keys);
            state.accessory = keys.map(str::to_owned);
        }
    }

    /// Brings forward the window whose number is `args`' `index`, if one
    /// is open.
    fn goto_window(&mut self, platform: &mut dyn Platform, args: &Args) {
        let Some(&Value::Integer(index)) = args.get("index") else {
            return;
        };
        let found = self.windows.iter().find(|(_, state)| {
            self.workspace
                .window(state.window)
                .and_then(workspace::Window::number)
                .is_some_and(|number| i64::from(number) == index)
        });
        if let Some((&window, _)) = found {
            platform.focus_window(window);
        }
    }

    /// Shows `shown`, a window of the workspace that a plugin's move made,
    /// as a tab's or a pane's out to a window of its own, in a window of
    /// the platform's, of its focused pane's cells; a window that does not
    /// open closes `shown`, with its panes, as the log tells.
    fn show_window(&mut self, platform: &mut dyn Platform, shown: workspace::WindowId) {
        let Some(grid) = self.workspace.focused(shown).map(Pane::grid) else {
            return;
        };
        let cells = (u32::from(grid.columns), u32::from(grid.rows));
        // A tab dropped out of every window takes its window's size.
        let dropped = self
            .workspace
            .window(shown)
            .map(workspace::Window::active_tab)
            .and_then(|tab| self.placing_size.take_if(|(placed, _)| *placed == tab))
            .map(|(_, size)| size);
        let attributes = WindowAttributes {
            title: TITLE.to_owned(),
            size: dropped.unwrap_or_else(|| self.size_of(platform.screen_scale(), cells)),
            remember: false,
            restore: false,
            restore_size: false,
            frame: None,
        };
        match platform.create_window(&attributes) {
            Ok(window) => {
                if let Some(at) = self.placing.remove(&shown) {
                    platform.set_window_origin(window, at);
                }
                self.attach(platform, window, shown, None);
                self.show_title(platform, window);
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                let panes = self.workspace.panes_of(shown);
                self.forget_panes(&panes);
                self.workspace.close_window(shown);
            }
        }
    }

    /// Puts the content's top left corner of the platform's window of
    /// `shown` at `at` on the screens, or, of a window of the workspace
    /// that has yet to show, as it shows.
    fn place_window(
        &mut self,
        platform: &mut dyn Platform,
        shown: workspace::WindowId,
        at: ScreenPoint,
    ) {
        let window = self
            .windows
            .iter()
            .find(|(_, state)| state.window == shown)
            .map(|(&window, _)| window);
        match window {
            Some(window) => platform.set_window_origin(window, at),
            None if self.workspace.window(shown).is_some() => {
                self.placing.insert(shown, at);
            }
            None => {}
        }
    }

    /// A pane that runs `program` in `environment`, of `grid` and `colors`,
    /// whose session wakes the app as it tells something, of the config's
    /// scrolling and clipboard's reads.
    fn spawn_pane(
        &mut self,
        platform: &dyn Platform,
        program: &Program,
        environment: &Environment,
        grid: Grid,
        colors: &lokyr_vt_core::terminal::Colors,
    ) -> Pane {
        let settings = self.settings();
        spawn::Start {
            program,
            environment,
            settings: &settings,
            colors,
            texts: &self.texts,
            config: &self.config,
            waker: platform.waker(),
            wakes: &mut self.pane_wakes,
        }
        .pane(PaneId::new(), grid)
    }

    /// Opens a quick window of a new pane, the workspace's of the id
    /// `shown`, which shows at `place` and takes the focus; its pane starts
    /// in `directory`, or as a new window's.
    fn open_quick(
        &mut self,
        platform: &mut dyn Platform,
        shown: workspace::WindowId,
        place: lokyr_platform_core::QuickPlace,
        directory: Option<PathBuf>,
    ) {
        let environment = Environment {
            directory: directory.or_else(|| self.start_directory(platform)),
            ..self.environment.clone()
        };
        match platform.create_quick_window(TITLE) {
            Ok(window) => {
                // Shown first, so that its pane starts of its size, and
                // known as a quick one before it draws.
                platform.show_quick_window(window, Some(&place));
                self.quick.insert(window, (place, true));
                let program = self.program.clone();
                self.run_in(
                    platform,
                    window,
                    (&program, None),
                    &environment,
                    Some(shown),
                );
            }
            Err(error) => eprintln!("lokyr: {error}"),
        }
    }

    /// Shows the quick window `window` at `place`, which takes the focus,
    /// or hides it, of `None`, the app that had the focus taking it back.
    fn show_quick(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        place: Option<lokyr_platform_core::QuickPlace>,
    ) {
        let Some((kept, shown)) = self.quick.get_mut(&window) else {
            return;
        };
        platform.show_quick_window(window, place.as_ref());
        if let Some(place) = place {
            *kept = place;
        }
        *shown = place.is_some();
        if let Some(state) = self.windows.get(&window) {
            self.workspace
                .set_window_hidden(state.window, place.is_none());
        }
    }

    /// Takes the config's global keys from every app, in place of those
    /// before, and tells those that the platform refused, as another app
    /// takes them, and those of a character that the US layout has no key
    /// for, by which they go.
    fn take_global_keys(&mut self, platform: &mut dyn Platform) {
        let mut refused = Vec::new();
        let mut keys: Vec<GlobalKey> = Vec::new();
        self.global_keys.clear();
        // The config's, then the plugins' of other keys.
        let mut bindings = keymap::global_bindings(&self.config.keys);
        for (combo, invocation) in self.plugins.global_bindings() {
            if bindings.iter().all(|(taken, _)| taken != combo) {
                bindings.push((*combo, invocation.clone()));
            }
        }
        for (combo, invocation) in bindings {
            match quick::global_key(combo) {
                Some(key) => {
                    keys.push(key);
                    self.global_keys.push((combo, invocation));
                }
                None => refused.push(combo),
            }
        }
        let refused_keys = platform.set_global_keys(&keys);
        for index in refused_keys {
            if let Some(&(combo, _)) = self.global_keys.get(index) {
                refused.push(combo);
            }
        }
        if refused.is_empty() {
            self.resolve(platform, &About::GlobalKeys);
            return;
        }
        let named: Vec<String> = refused
            .iter()
            .map(|combo| format!("`{}{combo}`", keymap::GLOBAL))
            .collect();
        let text = format!(
            "{} cannot be taken from every app: another app holds it, or the Mac's keyboards have no such key",
            named.join(", ")
        );
        let diagnostic = Diagnostic::new(Code::Unavailable, text, About::GlobalKeys);
        self.report(platform, diagnostic);
    }

    /// The directory that a window that opens now starts in: the first
    /// window's, as the app starts, `start-directory` says; another's,
    /// `new-directory`, of the window that the commands run in, or the one
    /// that has the focus, or, while the app is not active, the one that
    /// had it last. `None` for the home directory, as a directory that is
    /// not there, or cannot be read, takes too, which the log tells.
    fn start_directory(&self, platform: &dyn Platform) -> Option<PathBuf> {
        let home = std::env::home_dir();
        let (key, chosen) = if self.opened_one {
            let chosen = match &self.config.window.new_directory {
                NewDirectory::Home => None,
                NewDirectory::Inherit => self
                    .acting
                    .or_else(|| platform.focused_window())
                    .or(self.front)
                    .and_then(|window| self.directory_of(window)),
                NewDirectory::Path(path) => config::expanded(path, home.as_deref()),
            };
            ("new-directory", chosen)
        } else {
            let chosen = match &self.config.window.start_directory {
                StartDirectory::Home => None,
                StartDirectory::Last => self.state.as_deref().and_then(state::last_directory),
                StartDirectory::Path(path) => config::expanded(path, home.as_deref()),
            };
            ("start-directory", chosen)
        };
        let chosen = chosen?;
        if !chosen.is_dir() {
            eprintln!(
                "lokyr: the window starts in the home directory, as `{key}`'s {} is not a directory that it can enter",
                chosen.display()
            );
            return None;
        }
        Some(chosen)
    }

    /// The directory of the program in `window`'s foreground, or, once it
    /// ended or where it cannot be read, the one that its title saw last.
    fn directory_of(&self, window: WindowId) -> Option<PathBuf> {
        let state = self.windows.get(&window)?;
        self.workspace
            .focused(state.window)
            .and_then(Pane::working_directory)
            .or_else(|| state.directory.clone())
    }

    /// Keeps the directory of `window`, unless it shows a man page, as the
    /// last window's, for a next start in it.
    fn keep_directory(&self, window: WindowId) {
        let Some(kept) = &self.state else {
            return;
        };
        if self
            .windows
            .get(&window)
            .is_none_or(|shown| shown.page.is_some())
        {
            return;
        }
        if let Some(directory) = self.directory_of(window)
            && let Err(error) = state::keep_last_directory(kept, &directory)
        {
            eprintln!("lokyr: the last window's directory cannot be kept: {error}");
        }
    }

    /// Quits, as the directory of the window in front is kept as the last
    /// window's, and the windows for the next start, as the config says.
    fn quit(&mut self, platform: &mut dyn Platform) {
        if let Some(front) = self.front.or_else(|| platform.focused_window()) {
            self.keep_directory(front);
        }
        self.keep_windows(platform);
        self.end_latency_bench(platform);
        platform.quit();
    }

    /// Whether the app quits as its last window closes: as the config
    /// says, and always for a bench, whose measure ends with the app.
    fn quits_without_windows(&self) -> bool {
        self.config.app.quit_after_last_window_closed || self.benched
    }

    /// Closes `window`, and quits with the last one, if the config says so.
    fn close(&mut self, platform: &mut dyn Platform, window: WindowId) {
        // A tab dragged out of its bar ends its drag with its window.
        if self.tab_dragged_of(window) {
            self.end_tab_drag(platform, true);
        }
        self.keep_directory(window);
        if self.front == Some(window) {
            self.front = None;
        }
        // The surface and the session go before the window, and the asks of
        // its program's reads with it.
        if let Some(state) = self.windows.remove(&window) {
            drop(state.surface);
            let panes = self.workspace.panes_of(state.window);
            self.forget_panes(&panes);
            self.open_asks
                .retain(|_, (asked, _)| *asked != Some(window));
            self.close_asks.retain(|_, asked| *asked != window);
            self.title_asks.retain(|_, asked| *asked != window);
            self.part_asks.retain(|_, (asked, _)| *asked != window);
            // A question whose window closed has no answer.
            let unanswered: Vec<AlertId> = self
                .text_asks
                .iter()
                .filter(|(_, (asked, ..))| *asked == window)
                .map(|(&alert, _)| alert)
                .collect();
            for alert in unanswered {
                if let Some((_, plugin, id)) = self.text_asks.remove(&alert) {
                    self.plugins.answer(&plugin, id, None);
                }
            }
            self.workspace.close_window(state.window);
        }
        self.quick.remove(&window);
        platform.close_window(window);
        // The secure entry goes with the window that had the focus, whose
        // loss of it the platform tells no more, unless another takes it.
        self.settle_secure_input(platform);
        // The app stays open without windows, its icon in the Dock or a
        // command opening one, and a global key showing the quick terminal;
        // unless the config quits with the last window, the hidden quick
        // terminal with it, after the question for a program in it.
        if !self.quits_without_windows() {
            return;
        }
        if self.windows.is_empty() {
            // The latency's bench of the user's keys tells them as its
            // window closes; the windows kept, none now, forgotten.
            self.end_latency_bench(platform);
            self.keep_windows(platform);
            platform.quit();
        } else if self
            .windows
            .keys()
            .all(|window| self.quick.get(window).is_some_and(|&(_, shown)| !shown))
        {
            self.ask_quit(platform, false);
        }
    }

    /// Gives `pane`, one of `window`'s, the window's keyboard, as
    /// [`App::keyboard_moved`] tells, and the title takes its program's.
    pub(super) fn focus_pane(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        pane: PaneId,
    ) {
        if self.window_of(pane) != Some(window) || !self.workspace.focus_pane(pane) {
            return;
        }
        self.keyboard_moved(platform, window);
        self.show_title(platform, window);
    }

    /// The pane that has `window`'s keyboard, if it is another than the one
    /// that had it, as the app or a plugin moved it: while the window has
    /// the focus, and the pane rather than a slot's view, the programs that
    /// asked learn of it, the one that it leaves first; a composition under
    /// way ends, which the input method forgets, so that its commit goes to
    /// no other pane, and so does a selection being made; the cursor starts
    /// blinking, on; the window looks at its new place, its program and its
    /// directory, and draws again; the protected input and its lock follow
    /// the keyboard.
    fn keyboard_moved(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let now = self.workspace.focused(state.window).map(Pane::id);
        if now == state.keyboard {
            return;
        }
        if state.focused && state.pane_has_focus() {
            // A pane that closed tells nothing.
            for (id, focused) in [(state.keyboard, false), (now, true)] {
                if let Some(pane) = id.and_then(|id| self.workspace.pane(id)) {
                    pane.focus(focused);
                }
            }
        }
        state.keyboard = now;
        if state.preedit.take().is_some() {
            platform.end_composition(window);
        }
        state.selecting = None;
        state.blink.restart(Instant::now());
        self.look_at_place(window);
        platform.request_frame(window);
        // The protected input is the pane's that has the keyboard now, as
        // another tab shows, and the lock its tab's.
        self.settle_secure_input(platform);
    }

    /// Closes `pane` of `window`, whose program is hung up: its room goes
    /// to its sibling, and the keyboard, if it had it, to the sibling's
    /// pane nearest to it; a tab that it leaves without panes closes. The
    /// window's last pane closes the window, as [`App::close`] does.
    pub(super) fn close_pane(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        pane: PaneId,
    ) {
        let Some(shown) = self.windows.get(&window).map(|state| state.window) else {
            return;
        };
        if self.workspace.panes_of(shown).len() <= 1 {
            self.close(platform, window);
            return;
        }
        self.forget_panes(&[pane]);
        self.workspace.close_pane(pane);
        self.keyboard_moved(platform, window);
        self.show_title(platform, window);
        platform.request_frame(window);
    }

    /// Forgets what the app keeps of `panes`, which close: their programs'
    /// reads of the clipboard and their pastes that the user is asked
    /// about, their wakes, and the mouse's buttons that they held.
    fn forget_panes(&mut self, panes: &[PaneId]) {
        self.clipboard_asks
            .retain(|_, (pane, _)| !panes.contains(pane));
        self.paste_asks.retain(|_, (pane, _)| !panes.contains(pane));
        for &pane in panes {
            self.pane_wakes.remove(&pane);
            self.scripts.remove(&pane);
            for state in self.windows.values_mut() {
                state.mouse.forget(pane);
            }
        }
    }

    /// Does what `close-on-exit` says with the windows whose program has
    /// ended, and looks again soon for the status of those whose output
    /// ended before it.
    fn exits(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let close = self.config.window.close_on_exit;
        let mut closing = Vec::new();
        let mut ended = Vec::new();
        let mut waiting = false;
        let panes: Vec<(WindowId, PaneId, bool)> = self
            .windows
            .iter()
            .flat_map(|(&window, state)| {
                let page = state.page.is_some();
                self.workspace
                    .panes_of(state.window)
                    .into_iter()
                    .map(move |pane| (window, pane, page))
            })
            .collect();
        for (window, id, page) in panes {
            let Some(pane) = self.workspace.pane_mut(id) else {
                continue;
            };
            // A man page's window keeps the page, from its top, and a
            // script's pane what it printed.
            let close = if page || self.scripts.contains(&id) {
                CloseOnExit::Never
            } else {
                close
            };
            match pane.exited(close, now, &self.texts) {
                Some(Exited::Close) => {
                    closing.push((window, id));
                    ended.push(window);
                }
                Some(Exited::Keep) => {
                    if page {
                        pane.scroll(Scroll::Top);
                    }
                    platform.request_frame(window);
                    ended.push(window);
                }
                None => waiting |= pane.is_waiting(),
            }
        }
        // The output's bench ends with its program.
        if self.output_bench.is_some()
            && let Some(&window) = ended.first()
        {
            self.end_output_bench(platform, window);
            platform.quit();
            return;
        }
        for (window, pane) in closing {
            self.close_pane(platform, window, pane);
        }
        if waiting && self.exit_timer.is_none() {
            self.exit_timer = Some(platform.start_timer(now + EXIT_POLL));
        }
    }

    /// Runs the command `name` with `args`, as the menus and the keys ask,
    /// once the registry took them: in the plugin that registered it, or
    /// in the app; an error goes to the log.
    fn run(&mut self, platform: &mut dyn Platform, name: &str, args: Args) {
        // Copy, paste and select all, of the Edit menu or of their keys, are
        // the field's that has the keyboard's focus.
        if self.field_command(platform, name) {
            return;
        }
        self.run_deep(platform, name, args, plugins::new_chain());
    }

    /// Counts a run of `chain`, which commands asked: whether it may run,
    /// within the chain's depth and its runs.
    fn may_run(&mut self, chain: plugins::Chain, command: &str) -> bool {
        if chain.depth >= COMMANDS_DEEP {
            eprintln!(
                "lokyr: `{command}` does not run, as commands that ran commands took it {COMMANDS_DEEP} deep"
            );
            return false;
        }
        let (chains, under_way) = &mut self.chains;
        let runs = *chains
            .entry(chain.id)
            .and_modify(|runs| *runs += 1)
            .or_insert(1);
        // The oldest go, but those under way, whose runs a chain's
        // commands' calls go on asking, so that none counts anew.
        while chains.len() > CHAINS_KEPT {
            let Some(old) = chains
                .keys()
                .copied()
                .find(|id| *id != chain.id && !under_way.contains(id))
            else {
                break;
            };
            chains.remove(&old);
        }
        if runs > COMMANDS_WIDE {
            // Told once a chain.
            if runs == COMMANDS_WIDE + 1 {
                eprintln!(
                    "lokyr: `{command}` does not run, as commands that ran commands ran {COMMANDS_WIDE} of one chain"
                );
            }
            return false;
        }
        true
    }

    /// Runs `name`, of the chain `chain` of commands of plugins that ran
    /// it.
    fn run_deep(
        &mut self,
        platform: &mut dyn Platform,
        name: &str,
        args: Args,
        chain: plugins::Chain,
    ) {
        let ran = match self.commands.check(name, args) {
            Err(error) => Err(error),
            // A command that the registry gives to a plugin, never the
            // app's own, whatever a plugin's activation names.
            Ok(args) if self.commands.owner(name).is_some() && self.plugins.owns(name) => {
                let owner = self.plugins.owner_of(name).map(str::to_owned);
                let ran = self.with_plugins(platform, |plugins, context| {
                    plugins.run_deep(name, args, chain, context)
                });
                self.after_plugins(platform);
                // A fault that stopped its plugin shows as the plugins that
                // stopped do, once.
                let stopped = owner.is_some_and(|owner| {
                    self.plugins.status(&owner) == Some(plugins::Status::Disabled)
                });
                if stopped {
                    return;
                }
                ran
            }
            Ok(args) => match name {
                QUIT => {
                    self.ask_quit(platform, false);
                    Ok(())
                }
                SETTINGS => {
                    self.open_settings(platform);
                    Ok(())
                }
                ABOUT => {
                    self.show_about(platform);
                    Ok(())
                }
                CHECK_UPDATES => {
                    platform.check_for_updates();
                    Ok(())
                }
                REPORT_ISSUE => {
                    self.report_issue(platform);
                    Ok(())
                }
                PERFORMANCE => {
                    self.performance_chosen = Some(self.performance.is_none());
                    self.show_performance(platform);
                    Ok(())
                }
                SECURE_INPUT => {
                    self.secure_chosen = !self.secure_chosen;
                    platform.set_command_checked(SECURE_INPUT, self.secure_chosen);
                    self.settle_secure_input(platform);
                    Ok(())
                }
                DEFAULT_TERMINAL => {
                    self.make_default_terminal(platform);
                    Ok(())
                }
                INSTALL_TOOLS => {
                    self.install_tools(platform);
                    Ok(())
                }
                FLOAT_ON_TOP => {
                    if let Some(window) = self.acting.or_else(|| platform.focused_window())
                        && !self.quick.contains_key(&window)
                        && let Some(state) = self.windows.get_mut(&window)
                    {
                        state.floating = !state.floating;
                        platform.set_window_floating(window, state.floating);
                        self.show_floating(platform, Some(window));
                    }
                    Ok(())
                }
                NEW_WINDOW => {
                    self.open_window(platform);
                    Ok(())
                }
                CLOSE_WINDOW => {
                    if let Some(window) = platform.focused_window() {
                        self.ask_close(platform, window);
                    }
                    Ok(())
                }
                SET_TITLE => {
                    if let Some(window) = self.acting.or_else(|| platform.focused_window()) {
                        self.ask_title(platform, window);
                    }
                    Ok(())
                }
                GOTO_WINDOW => {
                    self.goto_window(platform, &args);
                    Ok(())
                }
                LOOK_UP => {
                    self.look_up(platform);
                    Ok(())
                }
                MAN_PAGE => {
                    self.open_man_page(platform);
                    Ok(())
                }
                MAN_SEARCH => {
                    self.search_man_pages(platform);
                    Ok(())
                }
                SEARCH_WEB => {
                    self.search_web(platform);
                    Ok(())
                }
                _ if let Some(&(_, mode, _)) =
                    APPEARANCES.iter().find(|(command, ..)| *command == name) =>
                {
                    self.choose_appearance(platform, mode);
                    Ok(())
                }
                _ => Err(Error::new(
                    Code::Failed,
                    format!("`{name}` has no action in the app"),
                )),
            },
        };
        if let Err(error) = ran {
            self.report(
                platform,
                Diagnostic::new(error.code, error.message, About::Other),
            );
        }
    }

    /// Activates the plugins, which register their commands and bind their
    /// keys; one that fails is disabled, as the log tells.
    fn activate_plugins(&mut self, platform: &mut dyn Platform) {
        // The plugins on threads of their own wake the main thread as they
        // send it messages, which the turns take.
        let waker = platform.waker();
        self.plugins.set_wake(move || waker.wake());
        // Their states and their sections of the config, which their
        // activation reads, the installed plugins' too.
        self.plugins.set_state_directory(self.state.clone());
        self.add_installed(platform);
        self.configure_plugins(platform);
        // A plugin whose activation failed is disabled, which the notice of
        // the plugins that stopped tells.
        let _failed = self.with_plugins(platform, Plugins::activate);
        self.after_plugins(platform);
    }

    /// Calls `call` with the plugins and what their host reaches of the
    /// app: the registry, the workspace, the platform's clipboard, the
    /// focused window, or the one that a context menu's commands run in,
    /// and the text's size.
    fn with_plugins<T>(
        &mut self,
        platform: &mut dyn Platform,
        call: impl FnOnce(&mut Plugins, &mut plugins::Context<'_>) -> T,
    ) -> T {
        let focused = self
            .acting
            .or_else(|| platform.focused_window())
            .and_then(|window| self.windows.get(&window))
            .map(|state| state.window);
        let font_size = self.font_size.unwrap_or(self.config.font.size.points());
        let confirm_paste = if self.pastes_unconfirmed {
            ConfirmPaste::Never
        } else {
            self.config.clipboard.confirm_paste
        };
        // The plugins' new panes start as a window's, beside the pane that
        // they split or the window's focused one.
        let dark = platform.appearance() == Appearance::Dark;
        let mut panes = spawn::Spawner {
            program: &self.program,
            environment: &self.environment,
            budget: &self.budget,
            colors: &self.theme.variant(dark).colors,
            texts: &self.texts,
            config: &self.config,
            waker: platform.waker(),
            wakes: &mut self.pane_wakes,
        };
        // The keys of a command, of the keymap, as the platform writes them.
        let writer = platform.shortcut_writer();
        let keymap = &self.keymap;
        let keys = move |command: &str, args: &Args| {
            keymap
                .shortcut_with(command, args)
                .and_then(menus::shortcut)
                .map(writer)
        };
        // The themes that a picker lists, read as a plugin asks.
        let theme_folder = self
            .config_file
            .as_ref()
            .and_then(|file| themes::directory(&file.path));
        let (held_themes, held_list) = (self.held_themes.as_deref(), &self.held_theme_list);
        let (appearance, theme_config) = (platform.appearance(), &self.config.theme);
        let theme_list = || {
            pickers::theme_list(
                (theme_folder.as_deref(), held_themes),
                held_list,
                appearance,
                theme_config,
            )
        };
        // Whether the menus enable a command, as the last frame of the
        // focused pane settled them.
        let menu_enabled = &self.menu_enabled;
        let enabled = |command: &str| menu_enabled.get(command).copied().unwrap_or(true);
        let mut clipboard = PlatformClipboard(platform);
        let mut context = plugins::Context {
            registry: &mut self.commands,
            workspace: &mut self.workspace,
            panes: &mut panes,
            clipboard: &mut clipboard,
            focused,
            font_size,
            confirm_paste,
            keys: &keys,
            enabled: &enabled,
            themes: &theme_list,
            language: self.texts.language(),
            now: Instant::now(),
        };
        call(&mut self.plugins, &mut context)
    }

    /// Takes what the plugins' calls changed and asked, once they
    /// returned: their commands into the registry, titled from their
    /// catalogs, and their bindings into the keymap, which the menu
    /// shows; and their requests, of which a command runs one deeper than
    /// the command whose call asked it; the windows draw again, as a call
    /// can change what a pane shows.
    fn after_plugins(&mut self, platform: &mut dyn Platform) {
        if self.plugins.take_declarations_changed() {
            self.commands = commands(&self.texts, &self.plugins);
            self.keymap =
                Keymap::with_config(bindings(&self.plugins, &self.commands), &self.config.keys);
            self.set_menu(platform);
            // The plugins' global keys, which the benches leave to the
            // user's Lokyr.
            if !self.benched {
                self.take_global_keys(platform);
            }
        }
        for request in self.plugins.take_requests() {
            match request {
                Request::Run { invocation, chain } => {
                    // A call of no command's starts a chain, which the
                    // kernel gives its runs.
                    let chain = match chain.id {
                        0 => plugins::new_chain(),
                        _ => chain,
                    };
                    if self.may_run(chain, &invocation.command) {
                        let deeper = plugins::Chain {
                            depth: chain.depth + 1,
                            ..chain
                        };
                        self.chains.1.push(chain.id);
                        self.run_deep(platform, &invocation.command, invocation.args, deeper);
                        self.chains.1.pop();
                    }
                }
                Request::OpenWindow(None) => self.open_window(platform),
                Request::OpenQuickWindow {
                    window,
                    place,
                    directory,
                } => {
                    let folder = directory.as_deref().and_then(spawn::folder_of);
                    self.open_quick(platform, window, quick::host_place(&place), folder);
                }
                Request::ShowQuickWindow { window, place } => {
                    let shown = self
                        .windows
                        .iter()
                        .find(|(_, state)| state.window == window)
                        .map(|(&shown, _)| shown);
                    if let Some(shown) = shown {
                        let place = place.as_ref().map(quick::host_place);
                        self.show_quick(platform, shown, place);
                    }
                }
                // A window in the folder that the plugin asked for, or a
                // new window's of a path that is not there.
                Request::OpenWindow(Some(directory)) => {
                    let program = self.program.clone();
                    let folder = spawn::folder_of(&directory);
                    self.open_window_running(platform, &program, None, folder);
                }
                Request::CloseWindow(window) => {
                    let shown = self
                        .windows
                        .iter()
                        .find(|(_, state)| state.window == window)
                        .map(|(&shown, _)| shown);
                    if let Some(shown) = shown {
                        self.ask_close(platform, shown);
                    }
                }
                Request::Quit => self.ask_quit(platform, false),
                Request::FontSize(size) => self.set_font_size(platform, size),
                Request::PreviewTheme(name) => self.preview_theme(platform, name),
                Request::SetTheme { variant, name } => self.set_theme(platform, variant, &name),
                Request::ConfirmPaste { pane, text } => self.ask_paste(platform, pane, text),
                Request::Open(target) => self.open_target(platform, target),
                // A plugin's panel over its window; one that its plugin
                // closed, the plugins' host forgot already.
                Request::OpenPanel {
                    plugin,
                    id,
                    window,
                    options,
                } => self.open_panel(platform, (plugin, id), window, options),
                Request::ClosePanel { plugin, id } => self.panel_gone(platform, &plugin, &id),
                Request::CloseAsked(closing) => self.ask_close_part(platform, closing),
                Request::AskConsent {
                    plugin,
                    capabilities,
                } => self.ask_consent(platform, plugin, capabilities),
                Request::AskText {
                    plugin,
                    id,
                    window,
                    message,
                    detail,
                    text,
                } => {
                    // Of the plugin's catalogs, in the language shown.
                    let language = self.texts.language().to_owned();
                    let alert = Alert {
                        style: AlertStyle::Informational,
                        message: self.plugins.text_of(&plugin, &language, &message),
                        detail: self.plugins.text_of(&plugin, &language, &detail),
                        buttons: vec![
                            self.texts.text("ask-text-ok"),
                            self.texts.text("ask-text-cancel"),
                        ],
                        suppression: None,
                        input: Some(text),
                    };
                    self.ask_text(platform, (plugin, id), window, &alert);
                }
                // A field's text and a list's selection in a slot's view.
                Request::SetText {
                    plugin,
                    view,
                    key,
                    text,
                    revision,
                } => self.set_field_text(platform, (&plugin, &view, &key), &text, revision),
                Request::Select {
                    plugin,
                    view,
                    key,
                    item,
                } => self.select_item(platform, (&plugin, &view, &key), item),
                Request::Reveal { plugin, view, key } => {
                    self.reveal_node(platform, (&plugin, &view), key);
                }
                Request::FocusNode { plugin, view, key } => {
                    self.focus_node(platform, (&plugin, &view), &key);
                }
                Request::PlaceWindow { window, at } => {
                    self.place_window(platform, window, ScreenPoint { x: at.x, y: at.y });
                }
            }
        }
        // The views whose renders the calls asked for, and their panels'
        // first, are kept for the windows.
        for (plugin, view) in self.plugins.take_renders() {
            if let Err(error) = self.with_plugins(platform, |plugins, context| {
                plugins.render(&plugin, &view, context)
            }) {
                eprintln!("lokyr: {error}");
            }
        }
        // The quick windows of the plugins that stopped, which none can show
        // again, close, shown first if hidden, so that the close's question
        // of a program that runs shows.
        for window in self.plugins.take_stopped_quick_windows() {
            let shown = self
                .windows
                .iter()
                .find(|(_, state)| state.window == window)
                .map(|(&shown, _)| shown);
            let Some(shown) = shown else {
                continue;
            };
            if let Some(&(place, false)) = self.quick.get(&shown) {
                self.show_quick(platform, shown, Some(place));
            }
            self.ask_close(platform, shown);
        }
        // A window that shows a view rendered anew draws it.
        self.frame_rendered(platform);
        // A plugin's state that could not be written goes to the log.
        for fault in self.plugins.take_faults() {
            eprintln!("lokyr: {fault}");
        }
        // A call of the main thread's that took long goes to the log.
        for slow in self.plugins.take_slow() {
            eprintln!(
                "lokyr: the plugin `{}` took {:.1} ms in `{}` on the main thread",
                slow.plugin,
                slow.took.as_secs_f64() * 1000.0,
                slow.callback
            );
        }
        // A plugin that stopped, whose views show as they were and take no
        // event, tells it in a notice.
        // A capability that no plugin has yet, which a manifest declared,
        // refused: the plugin runs without it, as a notice tells.
        for (plugin, capability) in self.plugins.take_withheld() {
            let capability = capability.to_string();
            let text = self.texts.text_with(
                "plugin-capability-withheld",
                &[
                    ("name", Arg::Text(&plugin)),
                    ("capability", Arg::Text(&capability)),
                ],
            );
            self.report(
                platform,
                Diagnostic::new(Code::Unavailable, text, About::Other),
            );
        }
        // The plugins that faulted start again after a while, which the
        // notice tells, or are turned off, after three faults in a minute.
        self.plugins.schedule_restarts(Instant::now());
        match self.plugins.next_restart() {
            Some(at) if self.restart_timer.is_none_or(|(_, armed)| armed != at) => {
                if let Some((timer, _)) = self.restart_timer.take() {
                    platform.cancel_timer(timer);
                }
                self.restart_timer = Some((platform.start_timer(at), at));
            }
            None => {
                if let Some((timer, _)) = self.restart_timer.take() {
                    platform.cancel_timer(timer);
                }
            }
            Some(_) => {}
        }
        self.arm_plugin_timer(platform);
        for (plugin, fault) in self.plugins.take_disabled() {
            let text = self.texts.text_with(
                "plugin-stopped",
                &[("name", Arg::Text(&plugin)), ("fault", Arg::Text(&fault))],
            );
            self.report(platform, Diagnostic::new(Code::Fault, text, About::Other));
        }
        // What the windows show changed only through the calls' writes; a
        // turn whose plugins wrote nothing, as the start's, whose events
        // they only take, draws no frame more.
        if self.plugins.take_written() {
            for &window in self.windows.keys() {
                platform.request_frame(window);
            }
        }
    }

    /// Arms the platform's timer at the time of the next of the plugins'
    /// timers, as their calls started and cancelled them and as they
    /// fired; none while no plugin's timer waits, which costs nothing.
    fn arm_plugin_timer(&mut self, platform: &mut dyn Platform) {
        match self.plugins.next_timer() {
            Some(at) if self.plugin_timer.is_none_or(|(_, armed)| armed != at) => {
                if let Some((timer, _)) = self.plugin_timer.take() {
                    platform.cancel_timer(timer);
                }
                self.plugin_timer = Some((platform.start_timer(at), at));
            }
            None => {
                if let Some((timer, _)) = self.plugin_timer.take() {
                    platform.cancel_timer(timer);
                }
            }
            Some(_) => {}
        }
    }

    /// Gives the text `size`, in points, or the config's with `None`, in
    /// every window; a font that fails leaves the one in force, as the log
    /// tells.
    fn set_font_size(&mut self, platform: &mut dyn Platform, size: Option<f32>) {
        let font = sized(&self.config.font, size);
        if let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(font)
        {
            eprintln!("lokyr: {error}");
            return;
        }
        self.font_size = size;
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    /// Gathers what the sessions of `panes` told, and of those that waited
    /// for their turn, and acts on it; a timer brings the turn of those
    /// that wait still.
    fn collect(&mut self, platform: &mut dyn Platform, panes: &[PaneId]) {
        let due = self.workspace.collect(panes, Instant::now());
        // The tabs of the panes whose output came take their places anew,
        // and their headers.
        for &pane in panes {
            self.look_at_tab_place(pane);
        }
        self.workspace.look_at_headers(panes);
        self.events(platform);
        if let Some(due) = due
            && self.told_timer.is_none_or(|(_, at)| at > due)
        {
            if let Some((timer, _)) = self.told_timer.take() {
                platform.cancel_timer(timer);
            }
            self.told_timer = Some((platform.start_timer(due), due));
        }
    }

    /// Ends a turn of the main thread: the workspace's events, whose
    /// changes the plugins take; what the plugins' threads and the reads of
    /// rows sent since, which a command's failure among tells the user;
    /// then the plugins' events delivered, a batch to each plugin of those
    /// that it subscribed to, whose calls' requests the app then takes, and
    /// whose writes' changes of the workspace the windows follow. A turn
    /// without events costs nothing.
    fn end_turn(&mut self, platform: &mut dyn Platform) {
        self.events(platform);
        let served = if self.plugins.waiting() {
            self.with_plugins(platform, Plugins::serve)
        } else {
            plugins::Served::default()
        };
        for error in served.commands {
            self.report(
                platform,
                Diagnostic::new(error.code, error.message, About::Other),
            );
        }
        for error in &served.errors {
            eprintln!("lokyr: {error}");
        }
        let ui_events = std::mem::take(&mut self.ui_events);
        let delivers = self.plugins.has_events();
        let rendered = std::mem::take(&mut self.rendered_slots);
        if ui_events.is_empty() && !delivers && !rendered && !served.any {
            return;
        }
        for (plugin, event) in &ui_events {
            let given = self.with_plugins(platform, |plugins, context| {
                plugins.ui_event(plugin, event, context)
            });
            if let Err(error) = given {
                eprintln!("lokyr: {error}");
            }
        }
        if delivers {
            for error in self.with_plugins(platform, Plugins::deliver) {
                eprintln!("lokyr: {error}");
            }
        }
        self.after_plugins(platform);
        // What the calls' writes changed of the workspace shows now, and
        // waits for the next delivery, which a turn of its own brings, as
        // no event of the platform's may come soon.
        self.events(platform);
        if self.plugins.has_events() && self.deliver_timer.is_none() {
            self.deliver_timer = Some(platform.start_timer(Instant::now()));
        }
    }

    /// Takes the workspace's events, whose changes the plugins take. A
    /// program's write to the clipboard puts its text there, and the
    /// primary selection, which macOS lacks, takes nothing; its read asks
    /// the user, once at a time for each pane.
    /// A window takes the title of its active tab, which its program's
    /// gives; a bell does what `[bell]` says, and a program's notification
    /// shows as the system's. The tabs keep their programs' colors, which
    /// the windows do not show, and a program's end waits for its status,
    /// which the exits take.
    fn events(&mut self, platform: &mut dyn Platform) {
        for event in self.workspace.take_events() {
            match event {
                Event::ClipboardWrite { target, data, .. } => {
                    if target.clipboard {
                        platform.set_clipboard_text(&String::from_utf8_lossy(&data));
                    }
                }
                Event::Terminal {
                    pane,
                    event: vt::Event::ClipboardRead(request),
                } => self.ask_clipboard_read(platform, pane, request),
                Event::TitleChanged(pane) => {
                    if let Some(window) = self.window_of(pane) {
                        self.show_title(platform, window);
                    }
                }
                Event::Bell(pane) => self.ring(platform, pane),
                Event::Terminal {
                    pane,
                    event: vt::Event::Notification { title, body },
                } => self.notify(platform, pane, &title, &body),
                Event::Changed(change) => {
                    self.follow(platform, &change);
                    self.plugins.post(plugins::Event::Workspace(change));
                }
                // The commands that the shell marked, for the plugins that
                // follow them.
                Event::Terminal {
                    pane,
                    event: vt::Event::CommandStarted(command),
                } => self.plugins.post(plugins::Event::CommandStarted {
                    pane: host::PaneId(pane.get()),
                    command,
                }),
                Event::CommandEnded {
                    pane,
                    command,
                    status,
                    took,
                } => {
                    self.plugins.post(plugins::Event::CommandEnded {
                        pane: host::PaneId(pane.get()),
                        command,
                        status,
                    });
                    if let Some(took) = took {
                        self.long_command_ended(platform, pane, command, status, took);
                    }
                }
                Event::Ended(_) | Event::Terminal { .. } => {}
            }
        }
        if !self.part_asks.is_empty() {
            self.drop_gone_part_asks(platform);
        }
    }

    /// Follows `change`, of the workspace, as a plugin's write made it,
    /// which the app's own operations have followed already: a window
    /// that opened without one of the platform's shows in one, and one
    /// that closed with one closes it; a pane that closed is forgotten; and
    /// the windows whose tabs, panes or layouts changed take their active
    /// tab's title. They draw again as the plugins' writes ask, and the
    /// app's own operations ask for their own frames.
    fn follow(&mut self, platform: &mut dyn Platform, change: &WorkspaceEvent) {
        let showing = |app: &Self, shown: workspace::WindowId| {
            app.windows
                .iter()
                .find(|(_, state)| state.window == shown)
                .map(|(&window, _)| window)
        };
        let of_tab = |app: &Self, tab: host::TabId| {
            app.workspace
                .tab(workspace::TabId::of(tab.0))
                .and_then(|tab| showing(app, tab.window()))
        };
        let changed = match *change {
            WorkspaceEvent::WindowOpened { window } => {
                let shown = workspace::WindowId::of(window.0);
                if showing(self, shown).is_none() {
                    self.show_window(platform, shown);
                }
                return;
            }
            WorkspaceEvent::WindowClosed { window } => {
                let shown = workspace::WindowId::of(window.0);
                self.placing.remove(&shown);
                if let Some(window) = showing(self, shown) {
                    self.close(platform, window);
                }
                return;
            }
            WorkspaceEvent::PaneClosed { pane, tab } => {
                self.forget_panes(&[PaneId::of(pane.0)]);
                self.close_strayed_panels(platform);
                vec![of_tab(self, tab)]
            }
            WorkspaceEvent::TabCreated { window, .. }
            | WorkspaceEvent::TabClosed { window, .. }
            | WorkspaceEvent::TabActivated { window, .. } => {
                vec![showing(self, workspace::WindowId::of(window.0))]
            }
            WorkspaceEvent::TabMoved { from, window, .. } => {
                self.close_strayed_panels(platform);
                vec![
                    showing(self, workspace::WindowId::of(from.0)),
                    showing(self, workspace::WindowId::of(window.0)),
                ]
            }
            WorkspaceEvent::PaneCreated { tab, .. }
            | WorkspaceEvent::PaneFocused { tab, .. }
            | WorkspaceEvent::LayoutChanged { tab } => vec![of_tab(self, tab)],
            // A tab's label and color, which the plugins' views show.
            WorkspaceEvent::TabChanged { .. } | WorkspaceEvent::PaneChanged { .. } => return,
            WorkspaceEvent::PaneMoved { from, tab, .. } => {
                self.close_strayed_panels(platform);
                vec![of_tab(self, from), of_tab(self, tab)]
            }
        };
        // A plugin's moves do what the app's own do.
        for window in changed.into_iter().flatten() {
            self.keyboard_moved(platform, window);
            self.show_title(platform, window);
        }
    }

    /// Whether `window`, in a live resize, drew a frame within `share` of
    /// this refresh of its display. Its frames then show with Core
    /// Animation's transaction, which lets their drawables go once it is
    /// committed: the display's frame, which comes within the commit of a
    /// step's transaction, would wait for a drawable that the commit, which
    /// waits for it in turn, lets go, until the drawables' wait gives up a
    /// second later, the window held, its content stretched; it waits for
    /// the next refresh instead, past a frame within the whole of it. A
    /// step draws in its own transaction, past a frame within
    /// [`STEP_SHARE`] of the refresh: a step that waited for a frame within
    /// the whole of it, as the steps come a refresh apart, waited for the
    /// display's next frame nearly every time, which showed in a
    /// transaction of its own, so that the resize drew at half the
    /// display's rate.
    fn drew_within(&self, platform: &dyn Platform, window: WindowId, share: f64) -> bool {
        let refresh = platform.refresh_interval(window).unwrap_or_default();
        self.windows.get(&window).is_some_and(|state| {
            state.live_resize
                && state
                    .drawn_at
                    .is_some_and(|drawn| drawn.elapsed() < refresh.mul_f64(share))
        })
    }

    /// What a new window's terminal takes of the config: its `TERM`, its
    /// widths and normal form, and its scrollback's limit, within the
    /// budget that the windows share. The terminal's own `TERM`, without
    /// its entry, as outside the bundle before `cargo xtask terminfo`,
    /// gives way to `xterm-256color`, as the log tells, rather than leave
    /// the programs a terminal that they cannot know.
    fn settings(&self) -> Settings {
        spawn::settings(&self.config, &self.environment, &self.budget)
    }

    /// The platform's window that shows `pane`, in one of its tabs.
    fn window_of(&self, pane: PaneId) -> Option<WindowId> {
        let tab = self.workspace.tab(self.workspace.tab_of(pane)?)?;
        self.windows
            .iter()
            .find(|(_, state)| state.window == tab.window())
            .map(|(&window, _)| window)
    }

    /// Ticks the Window menu's Float on Top as `window`, the one in front,
    /// floats, dimmed for the quick terminal's, which floats of its own,
    /// and without a window.
    fn show_floating(&self, platform: &mut dyn Platform, window: Option<WindowId>) {
        let state = window
            .filter(|window| !self.quick.contains_key(window))
            .and_then(|window| self.windows.get(&window));
        platform.set_command_checked(FLOAT_ON_TOP, state.is_some_and(|state| state.floating));
        platform.set_command_enabled(FLOAT_ON_TOP, state.is_some());
    }

    /// Gives `window` the title of its active tab, or Lokyr's when its
    /// program gives none, after 🔔 when its bell rang while it had no
    /// focus.
    fn show_title(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let title = self.title_of(window);
        let secure = self.secure_on;
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        // Its tab bar marks the protected input of its tab, where one
        // shows; a title in its place, of a single tab, leaves the window's
        // own the lock for its menu and the system's lists of windows.
        let titled = chrome::tab_bar_place(&state.chrome, &self.plugins)
            == lokyr_kernel::plugins::host::BarPlace::Title;
        let lock = if secure && state.focused && (!chrome::has_tab_bar(&state.chrome) || titled) {
            SECURE_MARK
        } else {
            ""
        };
        let mark = if state.bell_marked { BELL_MARK } else { "" };
        let title = format!("{lock}{mark}{title}");
        if state.title_shown.as_ref() != Some(&title) {
            platform.set_title(window, &title);
            state.title_shown = Some(title);
        }
    }

    /// Turns Secure Keyboard Entry on while the user chose it, or while the
    /// program in the window that has the focus hides what is typed, as at
    /// a password's prompt, which its terminal tells as output comes and
    /// before each key, never by a timer; off otherwise, and without a
    /// window that has the focus, as while another app is active. The
    /// focused window's title shows a lock while it is on.
    fn settle_secure_input(&mut self, platform: &mut dyn Platform) {
        let focused = self
            .windows
            .iter()
            .find(|(_, state)| state.focused)
            .map(|(&window, state)| (window, state.window));
        let hides = focused
            .and_then(|(_, shown)| self.workspace.focused(shown))
            .is_some_and(Pane::hides_input);
        let on = focused.is_some() && (self.secure_chosen || hides);
        if on != self.secure_on {
            platform.set_secure_input(on);
            self.secure_on = on;
            let windows: Vec<WindowId> = self.windows.keys().copied().collect();
            for window in windows {
                self.show_title(platform, window);
            }
        }
        // The tab that shows in the window with the focus is marked of it.
        let marked = focused
            .filter(|_| on)
            .and_then(|(_, shown)| self.workspace.window(shown))
            .map(workspace::Window::active_tab);
        let tabs: Vec<workspace::TabId> = self
            .windows
            .values()
            .filter_map(|state| self.workspace.window(state.window))
            .flat_map(|shown| shown.tabs().to_vec())
            .collect();
        for tab in tabs {
            self.workspace.set_tab_secure(tab, Some(tab) == marked);
        }
    }

    /// The title of `window`, as [`App::base_title`] gives it, then the
    /// grid's size, as `— 100×33`, unless the config leaves it out.
    fn title_of(&self, window: WindowId) -> String {
        let title = self.base_title(window);
        let grid = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.focused(state.window))
            .map(Pane::grid);
        match grid {
            Some(grid) if self.config.window.title_size => {
                format!("{title}{TITLE_DASH}{}×{}", grid.columns, grid.rows)
            }
            _ => title,
        }
    }

    /// The title of `window` before its size: its name, as
    /// [`App::window_name`] gives it, then the program in its foreground,
    /// as `lokyr — vim`, which a shell's title of its own, set at its
    /// prompt, would otherwise hide; but a man page's, and a program whose
    /// name is the title's.
    fn base_title(&self, window: WindowId) -> String {
        // The user's, in place of the rest.
        if let Some(title) = self
            .windows
            .get(&window)
            .and_then(|state| state.user_title.clone())
        {
            return title;
        }
        let name = self.window_name(window);
        let program = self
            .windows
            .get(&window)
            .filter(|state| state.page.is_none())
            .map(|state| state.place.1.as_str())
            .filter(|program| !program.is_empty() && *program != name);
        match program {
            Some(program) => format!("{name}{TITLE_DASH}{program}"),
            None => name,
        }
    }

    /// The name of `window`: its active tab's title, which its program
    /// gives; or its man page's; or the last part of its program's
    /// directory, as `lokyr`; or Lokyr's.
    fn window_name(&self, window: WindowId) -> String {
        let state = self.windows.get(&window);
        state
            .and_then(|state| self.workspace.window(state.window))
            .and_then(|shown| self.workspace.tab(shown.active_tab()))
            .map(workspace::Tab::title)
            .filter(|title| !title.is_empty())
            .or_else(|| state.and_then(|state| state.page.as_deref()))
            .or_else(|| {
                state
                    .map(|state| state.place.0.as_str())
                    .filter(|directory| !directory.is_empty())
            })
            .unwrap_or(TITLE)
            .to_owned()
    }

    /// Shows `window`'s title, if it changed, after a frame, which brought
    /// `output` or not: one of output looks again at the directory and the
    /// program in its foreground, while its program runs; a look reads the
    /// terminal's foreground group and its first process's path and
    /// directory, some microseconds. A frame of output comes after what a
    /// program wrote last, as `cd`'s prompt; the titles' timer looks once
    /// more [`TITLE_LOOK`] after a window's last one, for a program that
    /// started without a word after the echo of its command, as `sleep`:
    /// a wakeup or a few a burst, and none for the cursor's blinking.
    fn refresh_title(&mut self, platform: &mut dyn Platform, window: WindowId, output: bool) {
        if output && self.look_at_place(window) {
            let now = Instant::now();
            if let Some(state) = self.windows.get_mut(&window) {
                state.output_at = Some(now);
            }
            if let Some(after) = self.title_look
                && self.title_timer.is_none()
            {
                self.title_timer = Some((platform.start_timer(now + after), now + after));
            }
        }
        self.show_title(platform, window);
    }

    /// Looks at the directory and the program in `window`'s foreground,
    /// while its program runs: whether it did.
    fn look_at_place(&mut self, window: WindowId) -> bool {
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        let Some(pane) = self.workspace.focused(state.window) else {
            return false;
        };
        if pane.is_kept() {
            return false;
        }
        // A directory that cannot be read, as a program's of another user,
        // as `sudo`, keeps the one before.
        if let Some(directory) = pane.working_directory() {
            state.place.0 = directory_name(&directory, std::env::home_dir().as_deref());
            state.directory = Some(directory);
        }
        state.place.1 = pane.foreground_program().unwrap_or_default();
        let (id, place) = (pane.id(), place_label(&state.place.0, &state.place.1));
        let running = runs_other_than_shell(&state.place.1);
        let alternate = pane.is_alternate_screen();
        if let Some(tab) = self.workspace.tab_of(id)
            && self
                .workspace
                .tab(tab)
                .is_some_and(|tab| tab.active_pane() == id)
        {
            self.workspace.set_tab_place(tab, Some(place));
            self.workspace.set_tab_running(tab, running, alternate);
        }
        true
    }

    /// Gives `pane`'s tab, while the pane has its keyboard, its folder and
    /// its program in the foreground, which the tab's label takes without
    /// a title: a look at the terminal's foreground group and its first
    /// process's path and directory, some microseconds, as the pane's
    /// output comes.
    fn look_at_tab_place(&mut self, pane: PaneId) {
        let Some(tab) = self.workspace.tab_of(pane) else {
            return;
        };
        if self
            .workspace
            .tab(tab)
            .is_none_or(|tab| tab.active_pane() != pane)
        {
            return;
        }
        let Some(shown) = self.workspace.pane(pane).filter(|shown| !shown.is_kept()) else {
            return;
        };
        let home = std::env::home_dir();
        let Some(directory) = shown.working_directory() else {
            return;
        };
        let name = directory_name(&directory, home.as_deref());
        let program = shown.foreground_program().unwrap_or_default();
        let alternate = shown.is_alternate_screen();
        self.workspace
            .set_tab_place(tab, Some(place_label(&name, &program)));
        self.workspace
            .set_tab_running(tab, runs_other_than_shell(&program), alternate);
    }

    /// The titles' timer fired: the windows whose last frame of output came
    /// [`TITLE_LOOK`] or more before the time that it was for look once
    /// more; the timer starts again for the others, at the first of their
    /// times.
    fn look_at_places_again(&mut self, platform: &mut dyn Platform) {
        let (Some((_, at)), Some(after)) = (self.title_timer.take(), self.title_look) else {
            return;
        };
        let mut next: Option<Instant> = None;
        let mut due = Vec::new();
        for (&window, state) in &mut self.windows {
            let Some(output) = state.output_at else {
                continue;
            };
            if output + after <= at {
                state.output_at = None;
                due.push(window);
            } else {
                next = Some(next.map_or(output + after, |next| next.min(output + after)));
            }
        }
        for window in due {
            self.look_at_place(window);
            self.show_title(platform, window);
            // The panes' headers, of a program that started without a word.
            if let Some(shown) = self.windows.get(&window).map(|state| state.window) {
                let panes = self.workspace.panes_of(shown);
                self.workspace.look_at_headers(&panes);
            }
        }
        if let Some(next) = next {
            self.title_timer = Some((platform.start_timer(next), next));
        }
    }

    /// Gives the app the appearance `mode`, as the user chose it from the
    /// menu: the config file takes it, or, if it cannot, as the log tells,
    /// the app keeps it until it quits, or until the config's key changes.
    fn choose_appearance(&mut self, platform: &mut dyn Platform, mode: AppearanceMode) {
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "app", "appearance", mode.name())
                .map_err(|error| error.to_string()),
            None => Err("there is no config file, without a home directory".to_owned()),
        };
        match written {
            Ok(()) => {
                self.config.app.appearance = mode;
                self.appearance_chosen = None;
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                eprintln!("lokyr: the appearance is {} until Lokyr quits", mode.name());
                self.appearance_chosen = Some(mode);
            }
        }
        self.show_appearance(platform);
    }

    /// Gives the platform the appearance that the user chose, or the
    /// system's, ticks its item in the View menu, and draws the panes in
    /// its theme's variant.
    fn show_appearance(&mut self, platform: &mut dyn Platform) {
        let mode = self.appearance_chosen.unwrap_or(self.config.app.appearance);
        platform.set_appearance(match mode {
            AppearanceMode::System => None,
            AppearanceMode::Light => Some(Appearance::Light),
            AppearanceMode::Dark => Some(Appearance::Dark),
        });
        for (command, choice, _) in APPEARANCES {
            platform.set_command_checked(command, choice == mode);
        }
        let appearance = platform.appearance();
        self.appearance_changed(platform, appearance);
    }

    /// The app's appearance is `appearance`: the panes take its theme's
    /// variant, every pane of every tab, and the windows draw again.
    fn appearance_changed(&mut self, platform: &mut dyn Platform, appearance: Appearance) {
        let colors = self.colors(appearance);
        for state in self.windows.values().filter(|state| state.page.is_none()) {
            for id in self.workspace.panes_of(state.window) {
                if let Some(pane) = self.workspace.pane(id) {
                    pane.set_colors(&colors);
                }
            }
        }
        // The plugins' views render again, of the theme's other variant.
        for (plugin, view) in self.plugins.views() {
            let result = self.with_plugins(platform, |plugins, context| {
                plugins.render(&plugin, &view, context)
            });
            if let Err(error) = result {
                eprintln!("lokyr: {error}");
            }
            self.rendered_slots = true;
        }
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        for window in windows {
            if let Some(size) = platform.window_size(window) {
                self.draw(platform, window, size);
            }
        }
    }

    /// Keeps the timer of the programs' sizes at `at`, unless it comes
    /// sooner.
    fn pace_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self.resize_timer.is_some_and(|(_, current)| current <= at) {
            return;
        }
        if let Some((timer, _)) = self.resize_timer.take() {
            platform.cancel_timer(timer);
        }
        self.resize_timer = Some((platform.start_timer(at), at));
    }
}

/// The platform's calls, each a turn of the main thread.
impl<B: Backend> App<B> {
    fn start(&mut self, platform: &mut dyn Platform) {
        self.preferred = platform.locales();
        self.texts = texts(&self.config.app.language, &self.preferred);
        self.commands = commands(&self.texts, &self.plugins);
        // The plugins' commands, registered in English, take their
        // catalogs' titles in the language shown as their activations end.
        self.activate_plugins(platform);
        // The themes before the first window, whose panes take their colors.
        self.load_themes(platform);
        self.set_menu(platform);
        self.show_appearance(platform);
        self.show_performance(platform);
        self.tell_font(platform);
        self.tell_term(platform);
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        self.show_icon(platform);
        // The benches leave the user's global keys to the user's Lokyr.
        if !self.benched {
            self.take_global_keys(platform);
        }
        self.environment.locale =
            environment::utf8_locale(&self.preferred, &environment::system_locales());
        if let Some(file) = &mut self.config_file {
            if let Err(error) = crate::watch::make_directory(&file.path) {
                eprintln!(
                    "lokyr: the config's directory cannot be made, for {}: {error}",
                    file.path.display()
                );
            }
            file.watch(platform);
        }
        // The windows kept as the app last quit, or a new one; none yet
        // where the system started the app to open files, folders or
        // links, whose events came before the start and follow it, which
        // open their own windows.
        if !self.restore_windows(platform) {
            if platform.default_launch() || self.benched {
                self.open_window(platform);
            } else {
                self.untitled_timer = Some(platform.start_timer(Instant::now() + UNTITLED_WAIT));
            }
        }
        self.start_updates(platform);
        self.end_output_bench_unstarted(platform);
        self.end_startup_bench(platform);
        self.start_latency_bench(platform);
        if self.startup_bench.is_some() || self.latency_bench.is_some() {
            self.bench_deadline = Some(platform.start_timer(Instant::now() + BENCH_DEADLINE));
        }
    }

    fn take_app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        match event {
            AppEvent::Command(command) => match self.menu_item(&command) {
                Some(invocation) => self.run(platform, &invocation.command, invocation.args),
                None => self.run(platform, &command, Args::new()),
            },
            AppEvent::GlobalKey(index) => {
                if let Some((_, invocation)) = self.global_keys.get(index).cloned() {
                    self.run(platform, &invocation.command, invocation.args);
                }
            }
            AppEvent::Reopen => self.open_window(platform),
            AppEvent::QuitRequested => self.ask_quit(platform, true),
            // What woke the application changed what a window shows, ended
            // its program, or changed the config file.
            AppEvent::Wake => {
                self.end_startup_bench(platform);
                // The panes whose sessions woke it, alone, and their
                // windows' frames.
                let changed: Vec<PaneId> = self
                    .pane_wakes
                    .iter()
                    .filter(|(_, wake)| wake.take())
                    .map(|(&pane, _)| pane)
                    .collect();
                self.collect(platform, &changed);
                self.exits(platform);
                // The reading waits for the changes to settle: each change
                // puts it off.
                if let Some(file) = &mut self.config_file
                    && file.changed.swap(false, Ordering::AcqRel)
                {
                    if let Some(timer) = file.reading.take() {
                        platform.cancel_timer(timer);
                    }
                    file.reading = Some(platform.start_timer(Instant::now() + SETTLE));
                }
                if self.reloads.changed.swap(false, Ordering::AcqRel) {
                    if let Some(timer) = self.reloads.timer.take() {
                        platform.cancel_timer(timer);
                    }
                    self.reloads.timer =
                        Some(platform.start_timer(Instant::now() + installed::RELOAD_SETTLE));
                }
                if let Some(file) = &mut self.permissions_file
                    && file.changed.swap(false, Ordering::AcqRel)
                {
                    if let Some(timer) = file.reading.take() {
                        platform.cancel_timer(timer);
                    }
                    file.reading = Some(platform.start_timer(Instant::now() + SETTLE));
                }
                let themes = &mut self.theme_files;
                if themes.changed.swap(false, Ordering::AcqRel) {
                    if let Some(timer) = themes.reading.take() {
                        platform.cancel_timer(timer);
                    }
                    themes.reading = Some(platform.start_timer(Instant::now() + SETTLE));
                }
                for pane in changed {
                    if let Some(window) = self.window_of(pane) {
                        platform.request_frame(window);
                    }
                }
            }
            AppEvent::Timer(timer)
                if self
                    .latency_bench
                    .as_ref()
                    .is_some_and(|bench| bench.timer == Some(timer)) =>
            {
                self.latency_step(platform);
            }
            AppEvent::Timer(timer) if self.bench_deadline == Some(timer) => {
                self.bench_deadline = None;
                self.give_up_benches(platform);
            }
            // The plugins' timers whose time came fire, whose events the
            // turn's end delivers.
            AppEvent::Timer(timer)
                if self.plugin_timer.is_some_and(|(armed, _)| armed == timer) =>
            {
                self.plugin_timer = None;
                self.plugins.timers_due(Instant::now());
                self.arm_plugin_timer(platform);
            }
            AppEvent::Timer(timer)
                if self
                    .restart_timer
                    .is_some_and(|(restart, _)| restart == timer) =>
            {
                self.restart_timer = None;
                let now = Instant::now();
                for error in self.with_plugins(platform, |plugins, context| {
                    plugins.restart_due(now, context)
                }) {
                    eprintln!("lokyr: {error}");
                }
                self.after_plugins(platform);
            }
            AppEvent::Timer(timer) if self.untitled_timer == Some(timer) => {
                self.untitled_timer = None;
                if self
                    .windows
                    .keys()
                    .all(|window| self.quick.contains_key(window))
                {
                    self.open_window(platform);
                }
            }
            AppEvent::Timer(timer) if self.told_timer.is_some_and(|(told, _)| told == timer) => {
                self.told_timer = None;
                self.collect(platform, &[]);
            }
            AppEvent::Timer(timer) if self.theme_files.reading == Some(timer) => {
                self.theme_files.reading = None;
                self.load_themes(platform);
            }
            AppEvent::Timer(timer) if self.reloads.timer == Some(timer) => {
                self.reloads.timer = None;
                self.reload_installed(platform);
            }
            AppEvent::Timer(timer)
                if self
                    .permissions_file
                    .as_ref()
                    .is_some_and(|file| file.reading == Some(timer)) =>
            {
                if let Some(file) = &mut self.permissions_file {
                    file.reading = None;
                }
                self.watch_permissions(platform);
                self.permissions_changed(platform);
            }
            AppEvent::Timer(timer)
                if self
                    .config_file
                    .as_ref()
                    .is_some_and(|file| file.reading == Some(timer)) =>
            {
                if let Some(file) = &mut self.config_file {
                    file.reading = None;
                }
                self.reload(platform);
            }
            // The programs whose grid changed within their pace learn it.
            AppEvent::Timer(timer)
                if self.resize_timer.is_some_and(|(resize, _)| resize == timer) =>
            {
                let Some((_, at)) = self.resize_timer.take() else {
                    return;
                };
                // The timer fires once its time has come, or later.
                let now = Instant::now().max(at);
                let next = self
                    .workspace
                    .panes_mut()
                    .filter_map(|pane| pane.pace(now))
                    .min();
                if let Some(next) = next {
                    self.pace_at(platform, next);
                }
                // The screens that waited for their programs' drawing at a
                // new size show, whether it came or not.
                self.request_frames(platform);
            }
            // The performance's panel measures again.
            AppEvent::Timer(timer)
                if self
                    .performance
                    .as_ref()
                    .is_some_and(|shown| shown.timer == Some(timer)) =>
            {
                self.measure_performance(platform);
            }
            // The titles look once more after a burst of output.
            AppEvent::Timer(timer) if self.title_timer.is_some_and(|(title, _)| title == timer) => {
                self.look_at_places_again(platform);
            }
            // The panes' scroll bars that showed a while fade.
            AppEvent::Timer(timer) if self.bar_timer.is_some_and(|(bar, _)| bar == timer) => {
                self.bar_timer_fired(platform);
            }
            // A selection dragged past a window's edge scrolls its view.
            AppEvent::Timer(timer) if self.scroll_timer == Some(timer) => {
                self.scroll_selections(platform);
            }
            // The windows' flashes end.
            AppEvent::Timer(timer) if self.flash_timer == Some(timer) => {
                self.flash_timer = None;
                for (&window, state) in &mut self.windows {
                    if state.flash_until.take().is_some() {
                        platform.request_frame(window);
                    }
                }
            }
            AppEvent::Timer(timer) if self.access_timer == Some(timer) => {
                self.accessibility_due(platform);
            }
            // A turn for the plugins' events, which its end delivers.
            AppEvent::Timer(timer) if self.deliver_timer == Some(timer) => {
                self.deliver_timer = None;
            }
            // A tooltip of a slot's view shows.
            AppEvent::Timer(timer) if self.tooltip_timer.is_some_and(|(due, _)| due == timer) => {
                self.tooltips_due(platform);
            }
            AppEvent::Timer(timer) if self.exit_timer == Some(timer) => {
                self.exit_timer = None;
                self.exits(platform);
            }
            // The labels of the grids' sizes that end go.
            AppEvent::Timer(timer) if self.label_timer.is_some_and(|(label, _)| label == timer) => {
                // Its time, which has come, ends the labels of that time.
                let now = self
                    .label_timer
                    .take()
                    .map_or_else(Instant::now, |(_, at)| at.max(Instant::now()));
                let mut next = None;
                for (&window, state) in &mut self.windows {
                    match state.size_label {
                        // The end of its live resize starts its time.
                        Some(_) if state.live_resize => {}
                        Some(until) if until <= now => {
                            state.size_label = None;
                            platform.request_frame(window);
                        }
                        Some(until) => {
                            next = Some(next.map_or(until, |next: Instant| next.min(until)));
                        }
                        None => {}
                    }
                }
                if let Some(next) = next {
                    self.label_at(platform, next);
                }
            }
            // A blinking cursor changes.
            AppEvent::Timer(timer) if self.blink_timer.is_some_and(|(blink, _)| blink == timer) => {
                self.blink_timer = None;
                for (&window, state) in &self.windows {
                    if state.blinking {
                        platform.request_frame(window);
                    }
                }
            }
            // A click on a program's notification brings its window forward.
            // The text editor did not open the config file, as the system
            // told once it had taken the open on.
            AppEvent::OpenFailed { target, error } => {
                let config = self
                    .config_file
                    .as_ref()
                    .is_some_and(|file| file.path.display().to_string() == target);
                if config {
                    self.tell_settings_failed(platform, error);
                } else {
                    eprintln!("lokyr: {error}");
                }
            }
            AppEvent::NotificationClicked(window) => self.notification_clicked(platform, window),
            AppEvent::AlertClosed {
                alert,
                button,
                suppressed,
                text,
            } => self.alert_closed(platform, alert, (button, suppressed), text.as_deref()),
            AppEvent::AppearanceChanged(appearance) => {
                self.appearance_changed(platform, appearance);
            }
            AppEvent::OpenFiles(paths) => self.open_files(platform, paths),
            AppEvent::OpenFolders { folders, tabs } => self.open_folders(platform, folders, tabs),
            AppEvent::OpenUrls(links) => self.open_links(platform, &links),
            AppEvent::DefaultTerminalMade { failed } => {
                self.default_terminal_made(platform, &failed);
            }
            AppEvent::DefaultTerminalKnown => self.show_default_terminal(platform),
            AppEvent::ToolsLinked(outcome) => self.tools_linked(platform, &outcome),
            _ => {}
        }
    }

    fn take_window_event(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        event: WindowEvent,
    ) {
        match event {
            // A step of a live resize draws at once, so that the content
            // follows the edge, but not within half a refresh of the last
            // frame, the display's frames not within a whole one: see
            // `drew_within`.
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => {
                if self.drew_within(platform, window, STEP_SHARE) {
                    platform.request_frame(window);
                } else {
                    self.draw(platform, window, size);
                }
            }
            // A live resize presents its frames with Core Animation's
            // transaction, which moves the window's edge; its label of the
            // grid's size stays a moment after it ends.
            WindowEvent::LiveResize(resizing) => {
                // Its end tells the programs their last grids, within the
                // pace of their last signals, as its steps do.
                if !resizing {
                    let shown = self.windows.get(&window).map(|state| state.window);
                    let now = Instant::now();
                    let panes = shown.map(|shown| self.workspace.panes_of(shown));
                    let due = panes
                        .into_iter()
                        .flatten()
                        .filter_map(|pane| self.workspace.pane_mut(pane)?.end_live_resize(now))
                        .min();
                    if let Some(due) = due {
                        self.pace_at(platform, due);
                    }
                }
                if let Some(state) = self.windows.get_mut(&window) {
                    state.live_resize = resizing;
                    if let Some(surface) = &state.surface {
                        surface.set_presents_with_transaction(resizing);
                    }
                    if !resizing && state.size_label.is_some() {
                        let until = Instant::now() + size_label::HOLD;
                        state.size_label = Some(until);
                        self.label_at(platform, until);
                    }
                    platform.request_frame(window);
                }
            }
            WindowEvent::Frame => {
                if self.drew_within(platform, window, 1.0) {
                    platform.request_frame(window);
                } else if let Some(size) = platform.window_size(window) {
                    self.draw(platform, window, size);
                }
            }
            // The focus, and a key, start the cursor blinking again, on.
            // A window that loses the focus forgets the keys that the keymap
            // took, whose releases go elsewhere.
            // A window that takes the focus loses its bell's mark.
            WindowEvent::Focused(focused) => {
                // Command's release, out of the window, comes to another.
                if !focused {
                    self.command_held = false;
                    self.tell_pointer(platform, window, None, lokyr_platform_core::Modifiers::NONE);
                    self.surface_cancel(platform, window);
                }
                if let Some(state) = self.windows.get_mut(&window) {
                    // A program that asked learns of it, while its pane has
                    // the window's keyboard rather than a slot's view.
                    if let Some(pane) = self.workspace.focused(state.window)
                        && state.pane_has_focus()
                    {
                        pane.focus(focused);
                    }
                    state.focused = focused;
                    if focused {
                        self.front = Some(window);
                        state.blink.restart(Instant::now());
                    } else {
                        state.taken.clear();
                    }
                    platform.request_frame(window);
                    // Its title takes the lock, or leaves it, and its bell's
                    // mark goes with the focus.
                    if focused {
                        self.mark_bell(platform, window, false);
                    }
                    self.settle_secure_input(platform);
                    self.show_title(platform, window);
                }
                if focused {
                    self.show_floating(platform, Some(window));
                }
                // The plugins hear it, but of a question on the window, which
                // takes the focus as a part of it.
                if let Some(state) = self.windows.get(&window)
                    && (focused || !self.asks_on(window))
                {
                    let shown = state.window;
                    self.plugins
                        .post(lokyr_kernel::plugins::Event::WindowFocus {
                            window: shown.into(),
                            focused,
                        });
                }
            }
            // A key goes to the program, and starts the cursor blinking
            // again, on; a key that types or acts, not a modifier, a lock,
            // Fn or a Command key that no menu took, closes a window whose
            // program ended.
            WindowEvent::Key(key) => self.key(platform, window, &key),
            WindowEvent::Ime(ime) => self.ime(platform, window, ime),
            WindowEvent::MouseButton {
                button,
                pressed,
                position,
                mods,
                clicks,
            } => self.mouse_button(platform, window, (button, pressed, clicks), position, mods),
            WindowEvent::MouseMoved { position, mods } => {
                self.mouse_moved(platform, window, position, mods);
            }
            WindowEvent::MouseExited => {
                self.bar_hovered(platform, window, None);
                self.surface_exited(platform, window);
            }
            WindowEvent::Scroll {
                delta,
                phase,
                position,
                mods,
                ..
            } => self.scroll(platform, window, (delta, phase), position, mods),
            WindowEvent::CloseRequested => self.ask_close(platform, window),
            // An accessibility client asked for the window's tree, or for
            // an action on one of its nodes.
            WindowEvent::AccessibilityAsked => self.give_accessibility(platform, window),
            WindowEvent::AccessibilityAction(request) => {
                self.accessibility_action(platform, window, &request);
            }
            WindowEvent::Dropped(dropped) => self.dropped(platform, window, dropped),
            // A choice of a native menu of a plugin's panel, or of the
            // window's own.
            WindowEvent::ContextCommand(command) => {
                if !self.native_chosen(window, &command) {
                    self.context_command(platform, window, &command);
                }
            }
            WindowEvent::ContextMenuClosed => self.native_closed(platform, window),
            // A window that does not show lets its drawables go, and draws
            // again as it shows.
            WindowEvent::Occluded(occluded) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.occluded = occluded;
                    if occluded {
                        // The next frame draws whole, as the surface lets
                        // its drawables go.
                        state.shown = None;
                        let surfaces = [
                            &state.surface,
                            &state.text_layer.surface,
                            &state.cursor_layer.surface,
                        ];
                        for surface in surfaces.into_iter().flatten() {
                            surface.release();
                        }
                    } else {
                        platform.request_frame(window);
                    }
                }
            }
        }
    }
}

/// Each call of the platform's ends a turn of the main thread, which
/// delivers the plugins' events.
impl<B: Backend> Handler for App<B> {
    fn started(&mut self, platform: &mut dyn Platform) {
        self.start(platform);
        self.end_turn(platform);
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        self.take_app_event(platform, event);
        self.end_turn(platform);
    }

    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent) {
        self.take_window_event(platform, window, event);
        self.end_turn(platform);
    }
}

/// The context of the keys in a window of `pane`: its pane has the
/// keyboard, and holds the pane's contexts, the alternate screen and the
/// selection.
fn context(pane: Option<&Pane>) -> Context {
    let mut names = vec!["pane.focused"];
    names.extend(pane.map(Pane::contexts).unwrap_or_default());
    Context::new(names)
}

/// The clipboard of the platform, for the plugins' host.
struct PlatformClipboard<'a>(&'a mut dyn Platform);

impl plugins::Clipboard for PlatformClipboard<'_> {
    fn text(&self) -> Option<String> {
        self.0.clipboard_text()
    }

    fn set_text(&mut self, text: &str) {
        self.0.set_clipboard_text(text);
    }
}

/// `font`, at `size` in points when it is one, rather than its own.
fn sized(font: &FontConfig, size: Option<f32>) -> FontConfig {
    let size = size.and_then(|size| lokyr_text_core::Size::try_from(f64::from(size)).ok());
    FontConfig {
        size: size.unwrap_or(font.size),
        ..font.clone()
    }
}

/// The default bindings, the active plugins', of the commands that
/// `commands` holds: a plugin's binding of a command that no plugin
/// registers, as one of a plugin turned off, takes no key.
fn bindings(plugins: &Plugins, commands: &Registry) -> Vec<Binding> {
    plugins
        .bindings()
        .filter(|binding| commands.get(&binding.invocation.command).is_some())
        .cloned()
        .collect()
}

/// The registrations of the build's plugins, its distribution's; a test
/// of another system than the Mac takes the Mac's default shortcuts first,
/// as the tests press them.
fn registrations() -> Vec<plugins::Registration> {
    #[cfg_attr(any(not(test), target_os = "macos"), expect(unused_mut))]
    let mut registrations = crate::dist::plugins();
    #[cfg(all(test, not(target_os = "macos")))]
    registrations.insert(0, lokyr_keymap_macos::registration());
    registrations
}

/// The address of a new issue of the repository, its text telling the
/// app's `version` and the `system`, and asking what happened, in English,
/// as the issues are.
fn issue_url(version: &str, system: &str) -> String {
    let body = format!(
        "**Lokyr**: {version}\n**System**: {system}\n\n### What happened\n\n\n### What you expected\n\n\n### How to make it happen again\n\n"
    );
    format!("{REPOSITORY}/issues/new?body={}", query_encoded(&body))
}

/// `text` as a URL's query takes it: every byte but the unreserved ones of
/// RFC 3986 percent-encoded.
fn query_encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The texts of the language `chosen`, as the config names it, or of the
/// first of the user's `preferred` languages that the catalogs have; a
/// language that they lack goes to the log, with those that they have.
fn texts(chosen: &str, preferred: &[String]) -> Texts {
    let available = l10n::languages(&CATALOGS);
    let (language, unknown) = l10n::choose(chosen, preferred, &available);
    if unknown {
        eprintln!(
            "lokyr: the language `{chosen}` is not one of Lokyr's: {}; the system's shows",
            available.join(", ")
        );
    }
    Texts::new(&language, &CATALOGS)
}

/// The app's commands, which it runs itself, with their titles in `texts`,
/// and the active plugins'.
fn commands(texts: &Texts, plugins: &Plugins) -> Registry {
    let mut registry = Registry::new();
    let commands = [
        (ABOUT, "app-about", Category::App),
        (CHECK_UPDATES, "app-check-for-updates", Category::App),
        (REPORT_ISSUE, "app-report-issue", Category::App),
        (SETTINGS, "app-settings", Category::App),
        (SECURE_INPUT, "app-secure-keyboard-entry", Category::App),
        (DEFAULT_TERMINAL, "app-make-default-terminal", Category::App),
        (
            INSTALL_TOOLS,
            "app-install-command-line-tools",
            Category::App,
        ),
        (PERFORMANCE, "app-performance", Category::View),
        (NEW_WINDOW, "window-new", Category::Windows),
        (CLOSE_WINDOW, "window-close", Category::Windows),
        (SET_TITLE, "window-set-title", Category::Windows),
        (GOTO_WINDOW, "window-goto", Category::Windows),
        (
            FLOAT_ON_TOP,
            "window-toggle-float-on-top",
            Category::Windows,
        ),
        (QUIT, "app-quit", Category::App),
        (LOOK_UP, "pane-look-up", Category::Edit),
        (MAN_PAGE, "pane-man-page", Category::Windows),
        (MAN_SEARCH, "pane-man-search", Category::Windows),
        (SEARCH_WEB, "pane-search-web", Category::Edit),
    ]
    .into_iter()
    .chain(
        APPEARANCES
            .into_iter()
            .map(|(name, _, title)| (name, title, Category::View)),
    );
    for (name, title, category) in commands {
        // Bringing a window forward takes its number.
        let params = if name == GOTO_WINDOW {
            vec![Param {
                name: "index".to_owned(),
                kind: Kind::Integer,
                default: None,
            }]
        } else {
            Vec::new()
        };
        let command = Command {
            name: name.to_owned(),
            title: texts.text(&format!("command-{title}")),
            category,
            capabilities: Vec::new(),
            params,
            when: CONTEXT_ONLY.contains(&name).then(|| "selection".to_owned()),
        };
        if let Err(error) = registry.register(command) {
            eprintln!("lokyr: {error}");
        }
    }
    // Each of its plugin, which the plugins' reads of the commands tell.
    for (command, plugin, waits) in plugins.commands_in(texts.language()) {
        let registered = if waits {
            registry.register_waiting(command, &plugin)
        } else {
            registry.register_for(command, &plugin)
        };
        if let Err(error) = registered {
            eprintln!("lokyr: {error}");
        }
    }
    registry
}

/// Whether `program`, by its name, as the terminal's foreground tells it,
/// is one other than a shell at its prompt: none of no name.
fn runs_other_than_shell(program: &str) -> bool {
    !program.is_empty() && !workspace::is_shell(program)
}

//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use lokyr_kernel::commands::{Args, Category, Code, Command, Error, Registry};
use lokyr_kernel::config::{
    self, AppConfig, AppearanceMode, ClipboardRead, CloseOnExit, Config, ConfirmPaste,
    NewDirectory, OpenFile, OpenUrl, StartDirectory, Term,
};
use lokyr_kernel::diagnostics::{About, Diagnostic, Diagnostics};
use lokyr_kernel::keymap::{self, Binding, Context, Invocation, Keymap, When};
use lokyr_kernel::l10n::{self, Texts};
use lokyr_kernel::plugins::{self, Plugins, Request};
use lokyr_kernel::state;
#[cfg(test)]
use lokyr_kernel::workspace::RESIZE_PACE;
#[cfg(test)]
use lokyr_kernel::workspace::TOLD_PACE;
use lokyr_kernel::workspace::{
    self, Environment, Event, Exited, Grid, Pane, PaneId, Settings, Workspace,
};
use lokyr_platform_core::Layer;
use lokyr_platform_core::{
    Alert, AlertId, AlertStyle, AppEvent, Appearance, GlobalKey, Handler, Platform, Size,
    TargetKind, TimerId, WindowAttributes, WindowEvent, WindowId, WindowSize,
};
use lokyr_pty::Program;
use lokyr_render_core::{Renderer, Surface as _};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::theme::Theme;
use lokyr_vt_core::page::{Budget, Scrollback};
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

mod asks;
mod frame;
mod input;
mod menus;
mod notices;
#[cfg(test)]
mod tests;

use input::{show_selection, shown_top};

/// The command that quits.
const QUIT: &str = "app.quit";

/// The command that opens the config file in the text editor, made first
/// when it is not there.
const SETTINGS: &str = "app.settings";

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

/// What stands before the title of the window whose keys Secure Keyboard
/// Entry keeps from other apps.
const SECURE_MARK: &str = "🔒 ";

/// The command that opens a window.
const NEW_WINDOW: &str = "window.new";

/// The command that closes the focused window.
const CLOSE_WINDOW: &str = "window.close";

/// The command that asks for a title of the user's for the focused window.
const SET_TITLE: &str = "window.set-title";

/// The command that shows the quick terminal, or hides it.
const TOGGLE_QUICK: &str = "window.toggle-quick-terminal";

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

/// The commands that the context menu alone shows, as they name the
/// selection.
const CONTEXT_ONLY: [&str; 2] = [LOOK_UP, MAN_PAGE];

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

/// How deep the commands that plugins run from their commands go, past
/// which the next does not run, as a plugin that runs its own command
/// would go on forever.
const COMMANDS_DEEP: usize = 16;

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

/// What parts a title's parts.
const TITLE_DASH: &str = " — ";

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

/// How long a frame starts looking up the faces of characters that it has
/// not met, and how long it draws the glyphs that the atlas lacks, on the
/// font's threads: a frame of new glyphs takes up to about 10 ms, within a
/// refresh at 60 Hz, and a page of them shows whole in a few frames.
const GLYPHS_BUDGET: (Duration, Duration) = (Duration::from_millis(2), Duration::from_millis(2));

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

/// The config file, and its watch.
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

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug)]
pub struct App<B: Backend> {
    drawing: Option<Drawing<B>>,
    theme: Theme,
    /// The program that a new window runs.
    program: Program,
    /// The program that shows man pages, in place of the login shell's
    /// `man`, as the tests give one.
    man: Option<PathBuf>,
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
    /// The timer of the next look for the status of a program whose output
    /// ended.
    exit_timer: Option<TimerId>,
    /// The timer past which the startup's and the latency's benches give up
    /// waiting for a frame on the display.
    bench_deadline: Option<TimerId>,
    /// The timer of the turn of the panes whose sessions told something
    /// within the pace of their last look, and its time.
    told_timer: Option<(TimerId, Instant)>,
    /// What each pane's session wakes the main thread for, which tells the
    /// panes that changed.
    pane_wakes: BTreeMap<PaneId, Arc<PaneWake>>,
    /// The timer of the next size that a program learns, at its pace, and
    /// its time.
    resize_timer: Option<(TimerId, Instant)>,
    /// The timer of the end of the windows' flashes.
    flash_timer: Option<TimerId>,
    /// The timer of the next step of the views that scroll under a
    /// selection dragged past their grid.
    scroll_timer: Option<TimerId>,
    /// Whether Command is held, as the last key told, while which the
    /// pointer's moves go to the plugins that subscribed to them.
    command_held: bool,
    /// What the plugins were told last of the pointer.
    pointer_told: Option<PointerTold>,
    /// Whether a window opened since the app started, after which none
    /// restores the frame that a window kept, nor starts in the first
    /// window's directory.
    opened_one: bool,
    /// The directory of the app's state, which keeps the directory of the
    /// last window that closed; `None` keeps nothing, as in the tests.
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
    /// The contexts in which the menus' commands that tell one can run, and
    /// whether the platform has each one's items enabled, as the focused
    /// pane's last frame had it.
    menu_contexts: Vec<(String, When)>,
    menu_enabled: BTreeMap<String, bool>,
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
    /// Whether the cursors and the text that blinks blink as on layers of
    /// their own without them, as the tests' fake platform, which draws
    /// nothing, shows them.
    layers: bool,
    /// The windows whose closing the user is asked to confirm, by their
    /// alerts.
    close_asks: BTreeMap<AlertId, WindowId>,
    /// The windows whose title the user is asked for, by their alerts.
    title_asks: BTreeMap<AlertId, WindowId>,
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
    /// The quick terminal's window, once it opened, and whether it shows.
    quick: Option<(WindowId, bool)>,
    /// The config's global keys that the Mac's keyboards have, by the
    /// indices that their presses tell, with the commands that they run,
    /// those that another app holds too.
    global_keys: Vec<(keymap::Combo, Invocation)>,
    /// Whether the system gave the app one of the global keys at least,
    /// which keeps the app open without windows.
    global_held: bool,
    /// The measure of the startup, with `--bench startup`: when the first
    /// window's first frame showed, once it did, which the app prints
    /// before it quits.
    startup_bench: Option<Arc<Mutex<StartupBench>>>,
    /// The latency's bench, with `--bench latency`.
    latency_bench: Option<LatencyBench>,
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
    pub fn new(drawing: Option<Drawing<B>>, config: Config, path: Option<PathBuf>) -> Self {
        let mut plugins = Plugins::new();
        // Their manifests read, as the build checked them.
        let builtins: [(&str, Box<dyn plugins::Plugin>); 2] = [
            (
                lokyr_core_actions::MANIFEST,
                Box::new(lokyr_core_actions::CoreActions),
            ),
            (
                lokyr_links::MANIFEST,
                Box::new(lokyr_links::Links::default()),
            ),
        ];
        for (manifest, plugin) in builtins {
            if let Err(error) = plugins.add_builtin(manifest, plugin) {
                eprintln!("lokyr: a built-in plugin's manifest does not read: {error}");
            }
        }
        let keymap = Keymap::with_config(bindings(&plugins), &config.keys);
        let budget = Arc::new(Budget::new(config.scrollback.budget.bytes()));
        Self {
            drawing,
            theme: Theme::builtin(),
            program: Program::LoginShell,
            man: None,
            config,
            config_file: path.map(|path| ConfigFile {
                path,
                watcher: None,
                changed: Arc::new(AtomicBool::new(false)),
                reading: None,
            }),
            commands: commands(&Texts::new(l10n::SOURCE, &CATALOGS), &plugins),
            texts: Texts::new(l10n::SOURCE, &CATALOGS),
            preferred: Vec::new(),
            plugins,
            keymap,
            font_size: None,
            workspace: Workspace::new(),
            windows: BTreeMap::new(),
            blink_timer: None,
            label_timer: None,
            exit_timer: None,
            bench_deadline: None,
            told_timer: None,
            pane_wakes: BTreeMap::new(),
            resize_timer: None,
            flash_timer: None,
            scroll_timer: None,
            command_held: false,
            pointer_told: None,
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
            links_chosen: (None, None),
            appearance_chosen: None,
            performance: None,
            performance_chosen: None,
            layers: false,
            menu_contexts: Vec::new(),
            menu_enabled: BTreeMap::new(),
            context_window: None,
            acting: None,
            close_asks: BTreeMap::new(),
            title_asks: BTreeMap::new(),
            quit_ask: None,
            config_fell_back: false,
            update_checks: false,
            system_quit: false,
            environment: {
                let resources = environment::resources();
                Environment {
                    terminfo: environment::terminfo(resources.as_deref()),
                    resources,
                    binaries: environment::binaries(),
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    ..Environment::default()
                }
            },
            terminfo_dirs: programs_terminfo_dirs(),
            budget,
            quick: None,
            global_keys: Vec::new(),
            global_held: false,
            startup_bench: None,
            latency_bench: None,
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
            self.open_window_running(platform, &program, None);
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

    /// Takes the notice about `about` away, as a fix does.
    fn resolve(&mut self, platform: &mut dyn Platform, about: &About) {
        if self.diagnostics.resolve(about) {
            self.request_frames(platform);
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
            self.resolve(platform, &About::Config);
            self.settle_update_checks(platform, config.updates.check);
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
        let icon_changed = config.app.icon != self.config.app.icon;
        let language_changed = config.app.language != self.config.app.language;
        let appearance_changed = config.app.appearance != self.config.app.appearance;
        let keys_changed = config.keys != self.config.keys;
        let remember_changed = config.window.remember_frame != self.config.window.remember_frame;
        let performance_changed =
            config.debug.performance_overlay != self.config.debug.performance_overlay;
        let term_changed = config.terminal.term != self.config.terminal.term;
        self.settle_update_checks(platform, config.updates.check);
        if keys_changed {
            self.keymap = Keymap::with_config(bindings(&self.plugins), &config.keys);
        }
        self.config = config;
        if term_changed {
            self.tell_term(platform);
        }
        // The open windows keep their frames, or not, as the user moves or
        // resizes them from now; a man page's never does.
        if remember_changed {
            for (&window, state) in &self.windows {
                if self.quick.is_some_and(|(quick, _)| quick == window) {
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
        } else if keys_changed {
            self.set_menu(platform);
        }
        if keys_changed {
            self.take_global_keys(platform);
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
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
        let benched = self.startup_bench.is_some()
            || self.latency_bench.is_some()
            || self.output_bench.is_some();
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
        self.open_window_running(platform, &program, None);
    }

    /// Opens a window, whose pane runs `program`, and quits if it cannot
    /// while no other window is open: of the man page of the title `page`,
    /// as `man ls`, of its size and colors, when it shows one.
    fn open_window_running(
        &mut self,
        platform: &mut dyn Platform,
        program: &Program,
        page: Option<String>,
    ) {
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
            directory: self.start_directory(platform),
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
        };
        self.opened_one = true;
        match platform.create_window(&attributes) {
            Ok(window) => self.run_in(platform, window, (program, page), &environment),
            Err(error) => {
                eprintln!("lokyr: {error}");
                if self.windows.is_empty() && !self.global_held {
                    platform.quit();
                }
            }
        }
    }

    /// Gives the platform's new `window` a pane that runs `program` in
    /// `environment`, of the man page of the title `page`, as `man ls`, of
    /// its colors, when it shows one; and draws it.
    fn run_in(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        (program, page): (&Program, Option<String>),
        environment: &Environment,
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
        let grid = self.grid(size);
        let colors = match page {
            Some(_) => self.theme.man_page.colors.clone(),
            None => self.colors(platform.appearance()),
        };
        let pane = self.spawn_pane(platform, program, environment, grid, &colors);
        let shown = self.workspace.open_window(pane);
        let mut state = Window::new(surfaces, shown, focused, Instant::now());
        state.page = page;
        self.windows.insert(window, state);
        self.draw(platform, window, size);
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
        let waker = platform.waker();
        let wake = Arc::new(PaneWake::default());
        let woken = Arc::clone(&wake);
        let pane = Pane::spawn(
            program,
            environment,
            &self.settings(),
            grid,
            colors,
            &self.texts,
            move || {
                woken.wake();
                waker.wake();
            },
        );
        self.pane_wakes.insert(pane.id(), wake);
        pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
        pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
        pane
    }

    /// Shows the quick terminal, at the place that the config gives, and
    /// opens it first, in the directory that `new-directory` gives; or
    /// hides it, if it shows with the focus. Shown without the focus, it
    /// takes the focus.
    fn toggle_quick(&mut self, platform: &mut dyn Platform) {
        let place = quick::place(&self.config.quick_terminal);
        match self.quick {
            Some((window, true))
                if self.windows.get(&window).is_some_and(|state| state.focused) =>
            {
                platform.show_quick_window(window, None);
                self.quick = Some((window, false));
            }
            Some((window, _)) => {
                platform.show_quick_window(window, Some(&place));
                self.quick = Some((window, true));
            }
            None => {
                let environment = Environment {
                    directory: self.start_directory(platform),
                    ..self.environment.clone()
                };
                match platform.create_quick_window(TITLE) {
                    Ok(window) => {
                        // Shown first, so that its pane starts of its size.
                        platform.show_quick_window(window, Some(&place));
                        let program = self.program.clone();
                        self.run_in(platform, window, (&program, None), &environment);
                        self.quick = Some((window, true));
                    }
                    Err(error) => eprintln!("lokyr: {error}"),
                }
            }
        }
    }

    /// Hides the quick terminal as `window`, which it is, lost the focus,
    /// if `autohide` says so: not for a question on it, which takes the
    /// focus.
    fn autohide(&mut self, platform: &mut dyn Platform, window: WindowId) {
        if self.quick != Some((window, true))
            || !self.config.quick_terminal.autohide
            || self.asks_on(window)
        {
            return;
        }
        platform.show_quick_window(window, None);
        self.quick = Some((window, false));
    }

    /// Takes the config's global keys from every app, in place of those
    /// before, and tells those that the platform refused, as another app
    /// takes them, and those of a character that the US layout has no key
    /// for, by which they go.
    fn take_global_keys(&mut self, platform: &mut dyn Platform) {
        let mut refused = Vec::new();
        let mut keys: Vec<GlobalKey> = Vec::new();
        self.global_keys.clear();
        for (combo, invocation) in keymap::global_bindings(&self.config.keys) {
            match quick::global_key(combo) {
                Some(key) => {
                    keys.push(key);
                    self.global_keys.push((combo, invocation));
                }
                None => refused.push(combo),
            }
        }
        let refused_keys = platform.set_global_keys(&keys);
        self.global_held = refused_keys.len() < keys.len();
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
    /// window's.
    fn quit(&mut self, platform: &mut dyn Platform) {
        if let Some(front) = self.front.or_else(|| platform.focused_window()) {
            self.keep_directory(front);
        }
        self.end_latency_bench(platform);
        platform.quit();
    }

    /// Closes `window`, and quits with the last one.
    fn close(&mut self, platform: &mut dyn Platform, window: WindowId) {
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
            self.workspace.close_window(state.window);
        }
        if self.quick.is_some_and(|(quick, _)| quick == window) {
            self.quick = None;
        }
        platform.close_window(window);
        // The secure entry goes with the window that had the focus, whose
        // loss of it the platform tells no more, unless another takes it.
        self.settle_secure_input(platform);
        // A global key goes on showing the quick terminal, which needs the
        // app. Without one, the last window quits, the hidden quick
        // terminal with it, after the question for a program in it.
        if self.global_held {
            return;
        }
        if self.windows.is_empty() {
            // The latency's bench of the user's keys tells them as its
            // window closes.
            self.end_latency_bench(platform);
            platform.quit();
        } else if self.windows.len() == 1 && self.quick.is_some_and(|(_, shown)| !shown) {
            self.ask_quit(platform, false);
        }
    }

    /// Gives `pane`, one of `window`'s, the window's keyboard: the title
    /// takes its program's, its cursor starts blinking, on, and the window
    /// draws again. A composition under way, of the pane that had it, ends.
    pub(super) fn focus_pane(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        pane: PaneId,
    ) {
        if self.window_of(pane) != Some(window) || !self.workspace.focus_pane(pane) {
            return;
        }
        if let Some(state) = self.windows.get_mut(&window) {
            state.preedit = None;
            state.blink.restart(Instant::now());
        }
        self.look_at_place(window);
        self.show_title(platform, window);
        platform.request_frame(window);
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
        if let Some(state) = self.windows.get_mut(&window) {
            state.selecting = None;
        }
        self.look_at_place(window);
        self.show_title(platform, window);
        platform.request_frame(window);
    }

    /// Forgets what the app keeps of `panes`, which close: their programs'
    /// reads of the clipboard and their pastes that the user is asked
    /// about, and their wakes.
    fn forget_panes(&mut self, panes: &[PaneId]) {
        self.clipboard_asks
            .retain(|_, (pane, _)| !panes.contains(pane));
        self.paste_asks.retain(|_, (pane, _)| !panes.contains(pane));
        for pane in panes {
            self.pane_wakes.remove(pane);
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
            // A man page's window keeps the page, from its top.
            let close = if page { CloseOnExit::Never } else { close };
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
        self.run_deep(platform, name, args, 0);
    }

    /// Runs `name`, which commands of plugins `depth` deep ran.
    fn run_deep(&mut self, platform: &mut dyn Platform, name: &str, args: Args, depth: usize) {
        let ran = match self.commands.check(name, args) {
            Err(error) => Err(error),
            Ok(args) if self.plugins.owns(name) => {
                let ran = self.with_plugins(platform, |plugins, context| {
                    plugins.run(name, args, context)
                });
                self.after_plugins(platform, depth);
                ran
            }
            Ok(_) => match name {
                QUIT => {
                    self.ask_quit(platform, false);
                    Ok(())
                }
                SETTINGS => {
                    self.open_settings(platform);
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
                NEW_WINDOW => {
                    self.open_window(platform);
                    Ok(())
                }
                TOGGLE_QUICK => {
                    self.toggle_quick(platform);
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
                LOOK_UP => {
                    self.look_up(platform);
                    Ok(())
                }
                MAN_PAGE => {
                    self.open_man_page(platform);
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
        let errors = self.with_plugins(platform, Plugins::activate);
        for error in errors {
            self.report(
                platform,
                Diagnostic::new(error.code, error.message, About::Other),
            );
        }
        self.after_plugins(platform, 0);
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
        let mut clipboard = PlatformClipboard(platform);
        let mut context = plugins::Context {
            registry: &mut self.commands,
            workspace: &mut self.workspace,
            clipboard: &mut clipboard,
            focused,
            font_size,
            confirm_paste,
        };
        call(&mut self.plugins, &mut context)
    }

    /// Takes what the plugins' calls changed and asked, once they
    /// returned: their bindings into the keymap, and their requests, of
    /// which a command runs `depth` deep; the windows draw again, as a
    /// call can change what a pane shows.
    fn after_plugins(&mut self, platform: &mut dyn Platform, depth: usize) {
        if self.plugins.take_bindings_changed() {
            self.keymap = Keymap::with_config(bindings(&self.plugins), &self.config.keys);
            self.set_menu(platform);
        }
        for request in self.plugins.take_requests() {
            match request {
                Request::Run(invocation) if depth < COMMANDS_DEEP => {
                    self.run_deep(platform, &invocation.command, invocation.args, depth + 1);
                }
                Request::Run(invocation) => eprintln!(
                    "lokyr: `{}` does not run, as commands that ran commands took it {COMMANDS_DEEP} deep",
                    invocation.command
                ),
                Request::OpenWindow => self.open_window(platform),
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
                Request::ConfirmPaste { pane, text } => self.ask_paste(platform, pane, text),
                Request::Open(target) => self.open_target(platform, target),
            }
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
    /// changes the plugins take, then the plugins' events delivered, a
    /// batch to each plugin of those that it subscribed to, whose calls'
    /// requests the app then takes. A turn without events costs nothing.
    fn end_turn(&mut self, platform: &mut dyn Platform) {
        self.events(platform);
        if !self.plugins.has_events() {
            return;
        }
        for error in self.with_plugins(platform, Plugins::deliver) {
            eprintln!("lokyr: {error}");
        }
        self.after_plugins(platform, 0);
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
                Event::Changed(change) => self.plugins.post(plugins::Event::Workspace(change)),
                Event::Ended(_) | Event::Terminal { .. } => {}
            }
        }
    }

    /// What a new window's terminal takes of the config: its `TERM`, its
    /// widths and normal form, and its scrollback's limit, within the
    /// budget that the windows share. The terminal's own `TERM`, without
    /// its entry, as outside the bundle before `cargo xtask terminfo`,
    /// gives way to `xterm-256color`, as the log tells, rather than leave
    /// the programs a terminal that they cannot know.
    fn settings(&self) -> Settings {
        let terminal = &self.config.terminal;
        let mut term = terminal.term.as_str();
        if matches!(term, "xterm-lokyr" | "lokyr") && self.environment.terminfo.is_none() {
            eprintln!(
                "lokyr: `{term}` has no terminfo entry here, outside the bundle: the window takes `{FALLBACK_TERM}`; `cargo xtask terminfo` compiles the entry for `cargo run`"
            );
            term = FALLBACK_TERM;
        }
        Settings {
            term: term.to_owned(),
            widths: terminal.widths(),
            normalization: terminal.normalization.form(),
            scrollback: Scrollback {
                limit: self.config.scrollback.limit.bytes(),
                budget: Some(Arc::clone(&self.budget)),
            },
        }
    }

    /// The platform's window that shows `pane`, in one of its tabs.
    fn window_of(&self, pane: PaneId) -> Option<WindowId> {
        let tab = self.workspace.tab(self.workspace.tab_of(pane)?)?;
        self.windows
            .iter()
            .find(|(_, state)| state.window == tab.window())
            .map(|(&window, _)| window)
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
        let lock = if secure && state.focused {
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
        if on == self.secure_on {
            return;
        }
        platform.set_secure_input(on);
        self.secure_on = on;
        let windows: Vec<WindowId> = self.windows.keys().copied().collect();
        for window in windows {
            self.show_title(platform, window);
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
        true
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
    /// variant, and the windows draw again.
    fn appearance_changed(&mut self, platform: &mut dyn Platform, appearance: Appearance) {
        let colors = self.colors(appearance);
        for state in self.windows.values().filter(|state| state.page.is_none()) {
            if let Some(pane) = self.workspace.focused(state.window) {
                pane.set_colors(&colors);
            }
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
        self.activate_plugins(platform);
        // The plugins' commands registered in English, which their catalogs
        // title in the language shown.
        self.commands = commands(&self.texts, &self.plugins);
        self.set_menu(platform);
        self.show_appearance(platform);
        self.show_performance(platform);
        self.tell_font(platform);
        self.tell_term(platform);
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        self.show_icon(platform);
        // The benches leave the user's global keys to the user's Lokyr.
        if self.startup_bench.is_none()
            && self.latency_bench.is_none()
            && self.output_bench.is_none()
        {
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
        self.open_window(platform);
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
            AppEvent::Command(command) => self.run(platform, &command, Args::new()),
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
            AppEvent::Timer(timer) if self.told_timer.is_some_and(|(told, _)| told == timer) => {
                self.told_timer = None;
                self.collect(platform, &[]);
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
            // follows the edge, but once a refresh: a second draw within
            // one would wait a refresh for a drawable, which the display's
            // next frame draws instead.
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => {
                let refresh = platform.refresh_interval(window).unwrap_or_default();
                let drew_within = self.windows.get(&window).is_some_and(|state| {
                    state.live_resize
                        && state
                            .drawn_at
                            .is_some_and(|drawn| drawn.elapsed() < refresh)
                });
                if drew_within {
                    platform.request_frame(window);
                } else {
                    self.draw(platform, window, size);
                }
            }
            // A live resize presents its frames with Core Animation's
            // transaction, which moves the window's edge; its label of the
            // grid's size stays a moment after it ends.
            WindowEvent::LiveResize(resizing) => {
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
                if let Some(size) = platform.window_size(window) {
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
                }
                if let Some(state) = self.windows.get_mut(&window) {
                    // A program that asked learns of it.
                    if let Some(pane) = self.workspace.focused(state.window) {
                        pane.focus(focused);
                    }
                    state.focused = focused;
                    if focused {
                        self.front = Some(window);
                        state.blink.restart(Instant::now());
                    } else {
                        state.taken.clear();
                    }
                    if focused {
                        state.bell_marked = false;
                    }
                    platform.request_frame(window);
                    // Its title takes the lock, or leaves it, and its bell's
                    // mark goes with the focus.
                    self.settle_secure_input(platform);
                    self.show_title(platform, window);
                }
                if !focused {
                    self.autohide(platform, window);
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
            WindowEvent::Scroll {
                delta,
                phase,
                position,
                mods,
                ..
            } => self.scroll(platform, window, (delta, phase), position, mods),
            WindowEvent::CloseRequested => self.ask_close(platform, window),
            WindowEvent::Dropped(dropped) => self.dropped(platform, window, dropped),
            WindowEvent::ContextCommand(command) => {
                self.context_command(platform, window, &command);
            }
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

/// A title's name of `directory`: its last part, `~` for `home`, and `/`
/// for the root.
fn directory_name(directory: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if Some(directory) == home {
        return "~".to_owned();
    }
    directory.file_name().map_or_else(
        || "/".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
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

/// The default bindings: the app's, then the active plugins'.
fn bindings(plugins: &Plugins) -> Vec<Binding> {
    let mut bindings = default_bindings();
    bindings.extend(plugins.bindings().cloned());
    bindings
}

/// The default bindings of the app's commands.
fn default_bindings() -> Vec<Binding> {
    [
        ("cmd+,", SETTINGS),
        ("cmd+n", NEW_WINDOW),
        ("cmd+w", CLOSE_WINDOW),
        ("cmd+shift+i", SET_TITLE),
        ("cmd+q", QUIT),
    ]
    .into_iter()
    .filter_map(|(keys, command)| {
        Some(Binding {
            keys: keymap::parse_keys(keys).ok()?,
            invocation: Invocation {
                command: command.to_owned(),
                args: Args::new(),
            },
            when: None,
        })
    })
    .collect()
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
        (CHECK_UPDATES, "app-check-for-updates", Category::App),
        (REPORT_ISSUE, "app-report-issue", Category::App),
        (SETTINGS, "app-settings", Category::App),
        (SECURE_INPUT, "app-secure-keyboard-entry", Category::App),
        (PERFORMANCE, "app-performance", Category::View),
        (NEW_WINDOW, "window-new", Category::Windows),
        (CLOSE_WINDOW, "window-close", Category::Windows),
        (SET_TITLE, "window-set-title", Category::Windows),
        (
            TOGGLE_QUICK,
            "window-toggle-quick-terminal",
            Category::Windows,
        ),
        (QUIT, "app-quit", Category::App),
        (LOOK_UP, "pane-look-up", Category::Edit),
        (MAN_PAGE, "pane-man-page", Category::Windows),
    ]
    .into_iter()
    .chain(
        APPEARANCES
            .into_iter()
            .map(|(name, _, title)| (name, title, Category::View)),
    );
    for (name, title, category) in commands {
        let command = Command {
            name: name.to_owned(),
            title: texts.text(&format!("command-{title}")),
            category,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: CONTEXT_ONLY.contains(&name).then(|| "selection".to_owned()),
        };
        if let Err(error) = registry.register(command) {
            eprintln!("lokyr: {error}");
        }
    }
    for command in plugins.commands_in(texts.language()) {
        if let Err(error) = registry.register(command) {
            eprintln!("lokyr: {error}");
        }
    }
    registry
}

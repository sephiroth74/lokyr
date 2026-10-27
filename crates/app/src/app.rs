//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lokyr_font_coretext::CoreText;
use lokyr_kernel::config::Config;
use lokyr_platform_core::{
    AppEvent, Appearance, Backdrop, Handler, Ime, KeyState, MainMenu, Menu, MenuItem, Platform,
    Point, Shortcut, ShortcutKey, Size, TimerId, WindowAttributes, WindowEvent, WindowId,
    WindowSize,
};
use lokyr_pty::Program;
use lokyr_render_core::{Color, CursorShape, DisplayList, Item, Renderer};
use lokyr_render_metal::{Gpu, MetalRenderer, Surface};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::cursor::{self, Blink};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::pane::{self as cells, CursorLook, Preedit};
use lokyr_ui_core::theme::Theme;

use crate::keys;
#[cfg(test)]
use crate::pane::RESIZE_PACE;
use crate::pane::{Exited, Grid, Pane};
use crate::watch::Watcher;

/// The command that quits.
const QUIT: &str = "quit";

/// The size of a new window's content, in points.
const WINDOW_SIZE: Size = Size {
    width: 800.0,
    height: 500.0,
};

/// How long the app waits after the last change of the config file, for
/// the changes that come with it, as an editor's write and rename do, before
/// it reads the file once.
const SETTLE: Duration = Duration::from_millis(100);

/// How often the app looks for the status of a program whose output ended.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// The cell of a window that the app does not draw, in pixels.
const UNDRAWN_CELL: (u32, u32) = (8, 16);

/// What draws the windows: the GPU, its renderer, and the font of the text.
#[derive(Debug)]
pub struct Drawing {
    gpu: Gpu,
    renderer: MetalRenderer,
    font: Font<CoreText>,
}

impl Drawing {
    /// The system's GPU, and the font that `config` gives, which tells of a
    /// family that is not installed.
    pub fn new(config: FontConfig) -> Result<Self, String> {
        let gpu = Gpu::new().map_err(|error| error.to_string())?;
        let font = font(config)?;
        let (coverage, color) = font.texture_sizes();
        let renderer =
            MetalRenderer::new(&gpu, coverage, color).map_err(|error| error.to_string())?;
        Ok(Self {
            gpu,
            renderer,
            font,
        })
    }

    /// Takes the font that `config` gives, with an atlas of its own, and a
    /// renderer of the atlas's textures, in place of those before; or, if
    /// it fails, keeps them.
    fn set_font(&mut self, config: FontConfig) -> Result<(), String> {
        let font = font(config)?;
        let (coverage, color) = font.texture_sizes();
        let renderer =
            MetalRenderer::new(&self.gpu, coverage, color).map_err(|error| error.to_string())?;
        self.font = font;
        self.renderer = renderer;
        Ok(())
    }
}

/// The font that `config` gives, which tells of a family that is not
/// installed.
fn font(config: FontConfig) -> Result<Font<CoreText>, String> {
    let font = Font::new(CoreText::new(), config, 1.0).map_err(|error| error.to_string())?;
    if let Some(family) = font.missing_family() {
        eprintln!(
            "lokyr: the font family `{family}` is not installed; the text takes `{}`",
            font.family()
        );
    }
    if let Some(family) = font.missing_non_ascii_family() {
        eprintln!(
            "lokyr: the font family `{family}` is not installed; the text past ASCII takes `{}`",
            font.family()
        );
    }
    Ok(font)
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

/// A window that the app opened.
#[derive(Debug)]
struct Window {
    /// What it draws into, once the app draws and the platform has a layer.
    surface: Option<Surface>,
    /// Its program, and what it shows.
    pane: Pane,
    /// The text that the input method composes, shown at the cursor.
    preedit: Option<Preedit>,
    /// Whether it has the keyboard's focus.
    focused: bool,
    /// The clock of its cursor's blinking.
    blink: Blink,
    /// Whether its cursor blinked at its last frame.
    blinking: bool,
    /// The backdrop that the platform has for it, while it is translucent.
    backdrop: Option<Backdrop>,
}

impl Window {
    fn new(surface: Option<Surface>, pane: Pane, focused: bool, now: Instant) -> Self {
        Self {
            surface,
            pane,
            preedit: None,
            focused,
            blink: Blink::new(now),
            blinking: false,
            backdrop: None,
        }
    }
}

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug)]
pub struct App {
    drawing: Option<Drawing>,
    theme: Theme,
    /// The program that a new window runs.
    program: Program,
    /// The config in force.
    config: Config,
    /// The file of the config, if it has one.
    config_file: Option<ConfigFile>,
    windows: BTreeMap<WindowId, Window>,
    /// The timer of the next change of a blinking cursor, and its time.
    blink_timer: Option<(TimerId, Instant)>,
    /// The timer of the next look for the status of a program whose output
    /// ended.
    exit_timer: Option<TimerId>,
    /// The timer of the next size that a program learns, at its pace, and
    /// its time.
    resize_timer: Option<(TimerId, Instant)>,
}

impl App {
    /// An application that draws its windows with `drawing`, or nothing
    /// without it, with `config`, which it reads again from `path`, its
    /// file, when that changes.
    pub fn new(drawing: Option<Drawing>, config: Config, path: Option<PathBuf>) -> Self {
        Self {
            drawing,
            theme: Theme::builtin(),
            program: Program::LoginShell,
            config,
            config_file: path.map(|path| ConfigFile {
                path,
                watcher: None,
                changed: Arc::new(AtomicBool::new(false)),
                reading: None,
            }),
            windows: BTreeMap::new(),
            blink_timer: None,
            exit_timer: None,
            resize_timer: None,
        }
    }

    /// The app, whose windows run `program` rather than the login shell.
    #[cfg(test)]
    fn with_program(mut self, program: Program) -> Self {
        self.program = program;
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
                eprintln!("lokyr: {error}");
                eprintln!("lokyr: none of the config applies, and the one in force stays");
            }
        }
    }

    /// Takes `config` in place of the one in force, whole, in every window;
    /// or nothing of it, if its font fails.
    fn apply(&mut self, platform: &mut dyn Platform, config: Config) {
        if config == self.config {
            return;
        }
        if config.font != self.config.font
            && let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(config.font.clone())
        {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: none of the config applies, and the one in force stays");
            return;
        }
        self.config = config;
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }

    fn open_window(&mut self, platform: &mut dyn Platform) {
        let attributes = WindowAttributes {
            title: "Lokyr".to_owned(),
            size: WINDOW_SIZE,
        };
        match platform.create_window(&attributes) {
            Ok(window) => {
                let surface = self.surface(platform, window);
                let focused = platform.focused_window() == Some(window);
                let size = platform
                    .window_size(window)
                    .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
                let grid = self.grid(size);
                let colors = &self.colors(platform.appearance());
                let pane = Pane::spawn(&self.program, grid, colors, platform.waker());
                self.windows
                    .insert(window, Window::new(surface, pane, focused, Instant::now()));
                self.draw(platform, window, size);
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
        }
    }

    /// Closes `window`, and quits with the last one.
    fn close(&mut self, platform: &mut dyn Platform, window: WindowId) {
        // The surface and the session go before the window.
        self.windows.remove(&window);
        platform.close_window(window);
        if self.windows.is_empty() {
            platform.quit();
        }
    }

    /// Does what `close-on-exit` says with the windows whose program has
    /// ended, and looks again soon for the status of those whose output
    /// ended before it.
    fn exits(&mut self, platform: &mut dyn Platform) {
        let now = Instant::now();
        let close = self.config.window.close_on_exit;
        let mut closing = Vec::new();
        let mut waiting = false;
        for (&window, state) in &mut self.windows {
            match state.pane.exited(close, now) {
                Some(Exited::Close) => closing.push(window),
                Some(Exited::Keep) => platform.request_frame(window),
                None => waiting |= state.pane.is_waiting(),
            }
        }
        for window in closing {
            self.close(platform, window);
        }
        if waiting && self.exit_timer.is_none() {
            self.exit_timer = Some(platform.start_timer(now + EXIT_POLL));
        }
    }

    /// The theme's colors in `appearance`.
    fn colors(&self, appearance: Appearance) -> lokyr_vt_core::terminal::Colors {
        self.theme
            .variant(appearance == Appearance::Dark)
            .colors
            .clone()
    }

    /// The grid that fits a window of `size`, less its padding, in the
    /// cells of the font at its display's scale.
    fn grid(&mut self, size: WindowSize) -> Grid {
        let (x, y) = self.padding(size.scale);
        let (width, height) = match &mut self.drawing {
            Some(drawing) => {
                drawing.font.set_scale(size.scale as f32);
                let cell = drawing.font.cell();
                (cell.width, cell.height)
            }
            None => UNDRAWN_CELL,
        };
        let fit = |pixels: u32, padding: u32, cell: u32| {
            (pixels.saturating_sub(2 * padding) / cell.max(1)).clamp(1, u32::from(u16::MAX)) as u16
        };
        let pixels = |cell: u32| u16::try_from(cell).unwrap_or(u16::MAX);
        Grid {
            columns: fit(size.pixels.width, x, width),
            rows: fit(size.pixels.height, y, height),
            cell_width: pixels(width),
            cell_height: pixels(height),
        }
    }

    /// The backdrop of a window whose terminal's default background is
    /// `background`, as the config's opacity and blur make it: `None` for
    /// an opaque one.
    fn backdrop(&self, background: Color) -> Option<Backdrop> {
        let window = &self.config.window;
        window
            .background_opacity
            .is_translucent()
            .then(|| Backdrop {
                color: [
                    f64::from(background.red),
                    f64::from(background.green),
                    f64::from(background.blue),
                    f64::from(window.background_opacity.value()),
                ],
                blur: f64::from(window.background_blur.points()),
            })
    }

    /// The room between the grid and the window's edges, in pixels on a
    /// display of `scale`: at the left and the right, and at the top and
    /// the bottom.
    fn padding(&self, scale: f64) -> (u32, u32) {
        let window = &self.config.window;
        (
            window.padding_x.pixels(scale),
            window.padding_y.pixels(scale),
        )
    }

    /// The surface of `window`, if the app draws and the platform has a
    /// layer for it.
    fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.window_handle(window)?;
        Surface::new(&drawing.gpu, handle)
            .inspect_err(|error| eprintln!("lokyr: {error}"))
            .ok()
    }

    /// The shape that the cursor of `window` takes at `now`, or `None` while
    /// its blinking hides it; a cursor that blinks keeps the timer of its
    /// next change running.
    fn cursor(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        now: Instant,
    ) -> Option<CursorShape> {
        let state = self.windows.get_mut(&window)?;
        let program = state.pane.snapshot().cursor();
        let config = &self.config.cursor;
        let shape = cursor::shape(&program, config, state.focused);
        let (on, next) = if cursor::blinks(&program, config, state.focused) {
            state.blink.at(now)
        } else {
            (true, None)
        };
        // A cursor that stopped blinking stays on, and takes no timer.
        state.blinking = next.is_some();
        if let Some(next) = next {
            self.blink_at(platform, next);
        }
        on.then_some(shape)
    }

    /// Keeps the timer of the blinking cursors at `at`.
    fn blink_at(&mut self, platform: &mut dyn Platform, at: Instant) {
        if self.blink_timer.is_some_and(|(_, current)| current == at) {
            return;
        }
        if let Some((timer, _)) = self.blink_timer.take() {
            platform.cancel_timer(timer);
        }
        self.blink_timer = Some((platform.start_timer(at), at));
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

    /// Draws `window`, of `size`: its program's screen, brought up to date,
    /// in the grid that fits its content within the padding, with the
    /// cursor.
    fn draw(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        let grid = self.grid(size);
        let (left, top) = self.padding(size.scale);
        let now = Instant::now();
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let due = state.pane.resize(grid, now);
        // Output starts the cursor blinking again, on, as a key does.
        if state.pane.update() {
            state.blink.restart(now);
        }
        // The input method's candidates go at the cursor's cell.
        let at = state.pane.snapshot().cursor().position;
        let (width, height) = (f64::from(grid.cell_width), f64::from(grid.cell_height));
        platform.set_text_area(
            window,
            Point {
                x: (f64::from(left) + f64::from(at.x) * width) / size.scale,
                y: (f64::from(top) + f64::from(at.y) * height) / size.scale,
            },
            Size {
                width: width / size.scale,
                height: height / size.scale,
            },
        );
        // A translucent window's backdrop is the terminal's default
        // background, under the whole window, the titlebar too, and follows
        // it, as the theme, the program or reverse video change it.
        let background = cells::background(state.pane.snapshot());
        let sent = state.backdrop;
        let backdrop = self.backdrop(background);
        if backdrop != sent {
            platform.set_translucent(window, backdrop);
            if let Some(state) = self.windows.get_mut(&window) {
                state.backdrop = backdrop;
            }
        }
        if let Some(due) = due {
            self.pace_at(platform, due);
        }
        let shape = self.cursor(platform, window, Instant::now());
        let config = self.config.cursor;
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        if state.surface.is_none() {
            return;
        }
        drawing.font.begin_frame();
        let cell = *drawing.font.cell();
        let cursor = shape.map(|shape| CursorLook {
            shape,
            thickness: config.thickness(&cell, size.scale as f32),
        });
        let snapshot = state.pane.snapshot();
        let font = &mut drawing.font;
        let mut glyph = |shown: &cells::CellText<'_>| font.glyph(shown);
        let (x, y) = (
            i32::try_from(left).unwrap_or(i32::MAX),
            i32::try_from(top).unwrap_or(i32::MAX),
        );
        let mut batch = cells::cells(snapshot, &cell, x, y, None, cursor, &mut glyph);
        if let Some(preedit) = &state.preedit {
            cells::preedit(&mut batch, snapshot, &cell, preedit, &mut glyph);
        }
        // Over a backdrop, what the cells leave shows it.
        let opacity = if backdrop.is_some() { 0.0 } else { 1.0 };
        let list = DisplayList {
            background: background.with_alpha(opacity),
            items: vec![Item::Cells(batch)],
        };
        let updates = drawing.font.texture_updates();
        let Some(surface) = &state.surface else {
            return;
        };
        surface.resize(size.pixels.width, size.pixels.height, size.scale);
        let drawn = drawing
            .renderer
            .update_textures(&updates)
            .and_then(|()| drawing.renderer.draw(surface, &list));
        if let Err(error) = drawn {
            eprintln!("lokyr: {error}");
        }
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        platform.set_main_menu(&main_menu());
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
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
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        match event {
            AppEvent::Command(command) if command == QUIT => platform.quit(),
            AppEvent::QuitRequested => platform.quit(),
            // What woke the application changed what a window shows, ended
            // its program, or changed the config file.
            AppEvent::Wake => {
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
                for &window in self.windows.keys() {
                    platform.request_frame(window);
                }
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
                    .windows
                    .values_mut()
                    .filter_map(|state| state.pane.pace(now))
                    .min();
                if let Some(next) = next {
                    self.pace_at(platform, next);
                }
            }
            AppEvent::Timer(timer) if self.exit_timer == Some(timer) => {
                self.exit_timer = None;
                self.exits(platform);
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
            AppEvent::AppearanceChanged(appearance) => {
                let colors = self.colors(appearance);
                for state in self.windows.values() {
                    state.pane.set_colors(&colors);
                }
                let windows: Vec<WindowId> = self.windows.keys().copied().collect();
                for window in windows {
                    if let Some(size) = platform.window_size(window) {
                        self.draw(platform, window, size);
                    }
                }
            }
            _ => {}
        }
    }

    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(size) | WindowEvent::ScaleChanged(size) => {
                self.draw(platform, window, size);
            }
            WindowEvent::Frame => {
                if let Some(size) = platform.window_size(window) {
                    self.draw(platform, window, size);
                }
            }
            // The focus, and a key, start the cursor blinking again, on.
            WindowEvent::Focused(focused) => {
                if let Some(state) = self.windows.get_mut(&window) {
                    state.focused = focused;
                    if focused {
                        state.blink.restart(Instant::now());
                    }
                    platform.request_frame(window);
                }
            }
            // A key goes to the program, and starts the cursor blinking
            // again, on; a key that types or acts, not a modifier, a lock,
            // Fn or a Command key that no menu took, closes a window whose
            // program ended.
            WindowEvent::Key(key) => {
                let options = keys::options(self.config.keyboard.option_as_meta);
                let Some(state) = self.windows.get_mut(&window) else {
                    return;
                };
                let pressed = key.state != KeyState::Released;
                if state.pane.is_kept() {
                    if pressed && closes(&key) {
                        self.close(platform, window);
                    }
                    return;
                }
                if let Some(event) = keys::terminal_key(&key) {
                    state.pane.key(&event, options);
                }
                if pressed {
                    state.blink.restart(Instant::now());
                    platform.request_frame(window);
                }
            }
            // The input method's text: composed, shown at the cursor, or
            // typed, sent to the program.
            WindowEvent::Ime(ime) => {
                let Some(state) = self.windows.get_mut(&window) else {
                    return;
                };
                // On a window whose program ended, what the input method
                // composes does not show, and its commit closes it, as a key
                // does.
                if state.pane.is_kept() {
                    if matches!(ime, Ime::Commit(_)) {
                        self.close(platform, window);
                    }
                    return;
                }
                match ime {
                    Ime::Preedit { text, selection } => {
                        state.preedit = (!text.is_empty()).then_some(Preedit {
                            text,
                            cursor: selection.start,
                        });
                    }
                    Ime::Commit(text) => {
                        state.preedit = None;
                        state.pane.text(&text);
                        state.blink.restart(Instant::now());
                    }
                }
                platform.request_frame(window);
            }
            WindowEvent::CloseRequested => self.close(platform, window),
            _ => {}
        }
    }
}

/// Whether `key` closes a window whose program ended: a key that types or
/// acts, not a modifier, a lock, Fn, nor a key with Command, which the
/// app's shortcuts have.
fn closes(key: &lokyr_platform_core::KeyEvent) -> bool {
    use lokyr_vt_core::input::Key;
    !key.mods.super_key
        && keys::terminal_key(key).is_some_and(|key| {
            !matches!(
                key.key,
                Key::Modifier(_) | Key::CapsLock | Key::NumLock | Key::ScrollLock
            )
        })
}

/// The main menu: the application's menu, with Quit.
fn main_menu() -> MainMenu {
    MainMenu {
        menus: vec![Menu {
            title: "Lokyr".to_owned(),
            items: vec![MenuItem::Command {
                title: "Quit Lokyr".to_owned(),
                command: QUIT.to_owned(),
                shortcut: Some(Shortcut {
                    key: ShortcutKey::Char('q'),
                    command: true,
                    shift: false,
                    alt: false,
                    control: false,
                }),
            }],
        }],
    }
}

#[cfg(test)]
mod tests {
    use lokyr_kernel::config::CloseOnExit;
    use lokyr_platform_core::fake::Fake;
    use lokyr_ui_core::cursor::CursorConfig;

    use super::*;

    /// A program that runs until its terminal hangs up, and writes nothing.
    fn quiet() -> Program {
        Program::command("/bin/cat", [] as [&str; 0])
    }

    /// The app, of `config`, whose windows run `program`.
    fn app(config: Config, program: Program) -> App {
        App::new(None, config, None).with_program(program)
    }

    fn started() -> (Fake, App) {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), quiet());
        fake.start(&mut app);
        (fake, app)
    }

    #[test]
    fn starting_opens_a_window_under_a_menu_with_quit() {
        let (fake, _) = started();
        let windows: Vec<_> = fake.windows().collect();
        assert_eq!(windows.len(), 1);
        assert_eq!(fake.window(windows[0]).unwrap().size.points, WINDOW_SIZE);
        let menu = fake.main_menu().unwrap();
        assert!(matches!(
            &menu.menus[0].items[0],
            MenuItem::Command { command, shortcut: Some(Shortcut { key: ShortcutKey::Char('q'), command: true, .. }), .. }
                if command == QUIT
        ));
    }

    #[test]
    fn quit_from_the_menu_or_from_the_system_quits() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command(QUIT.to_owned()));
        assert!(fake.has_quit());

        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::QuitRequested);
        assert!(fake.has_quit());
    }

    #[test]
    fn closing_the_last_window_quits() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::CloseRequested);
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());
    }

    #[test]
    fn a_window_without_a_layer_draws_nothing() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        assert!(app.windows[&window].surface.is_none());
        let size = WindowSize::new(
            Size {
                width: 900.0,
                height: 600.0,
            },
            1.0,
        );
        assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
        assert!(fake.send(&mut app, window, WindowEvent::ScaleChanged(size)));
        fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
        assert_eq!(fake.windows().count(), 1);
    }

    #[test]
    fn a_wake_asks_each_window_for_a_frame() {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send_app(&mut app, AppEvent::Wake);
        assert!(fake.window(window).unwrap().frame_requested);
        assert!(fake.send(&mut app, window, WindowEvent::Frame));
        assert!(!fake.window(window).unwrap().frame_requested);
    }

    /// The app started, its window focused and drawn once.
    fn focused() -> (Fake, App, WindowId) {
        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        assert!(fake.send(&mut app, window, WindowEvent::Focused(true)));
        assert!(fake.window(window).unwrap().frame_requested);
        assert!(fake.send(&mut app, window, WindowEvent::Frame));
        (fake, app, window)
    }

    #[test]
    fn the_focused_window_s_cursor_blinks_on_its_timer() {
        let (mut fake, mut app, window) = focused();
        let timers: Vec<_> = fake.timers().collect();
        assert_eq!(timers.len(), 1);
        let (timer, at) = timers[0];
        assert_eq!(app.blink_timer, Some((timer, at)));
        // The timer asks the window for the frame of the change.
        assert_eq!(fake.fire_timers(&mut app, at), 1);
        assert_eq!(app.blink_timer, None);
        assert!(fake.window(window).unwrap().frame_requested);
        // Off, then on, as the clock goes.
        let now = at;
        assert_eq!(app.cursor(&mut fake, window, now), None);
        assert_eq!(
            app.cursor(&mut fake, window, now + cursor::BLINK_INTERVAL),
            Some(CursorShape::Block)
        );
        assert_eq!(fake.timers().count(), 1);
    }

    #[test]
    fn blinking_stops_on_after_its_timeout() {
        let (mut fake, mut app, window) = focused();
        let later = Instant::now() + cursor::BLINK_TIMEOUT;
        assert_eq!(
            app.cursor(&mut fake, window, later),
            Some(CursorShape::Block)
        );
        assert!(!app.windows[&window].blinking);
        // Its last timer asks for no frame.
        let fired = fake.fire_timers(&mut app, later + cursor::BLINK_TIMEOUT);
        assert_eq!(fired, 1);
        assert!(!fake.window(window).unwrap().frame_requested);
        assert_eq!(fake.timers().count(), 0);
    }

    #[test]
    fn a_window_without_the_focus_shows_a_hollow_block_that_does_not_blink() {
        let (mut fake, mut app, window) = focused();
        assert!(fake.send(&mut app, window, WindowEvent::Focused(false)));
        let now = Instant::now();
        for millis in [0, 600, 1200] {
            let at = now + std::time::Duration::from_millis(millis);
            assert_eq!(app.cursor(&mut fake, window, at), Some(CursorShape::Hollow));
        }
        assert!(!app.windows[&window].blinking);
    }

    #[test]
    fn the_configuration_gives_the_cursor_s_shape_and_blinking() {
        let mut fake = Fake::default();
        let mut config = Config::default();
        config.cursor = CursorConfig {
            shape: cursor::Shape::Bar,
            blink: false,
            thickness: None,
        };
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        let half = Instant::now() + cursor::BLINK_INTERVAL + cursor::BLINK_INTERVAL / 2;
        assert_eq!(app.cursor(&mut fake, window, half), Some(CursorShape::Bar));
        assert_eq!(fake.timers().count(), 0);
    }

    /// An app started with its config file, in a directory of its own for
    /// `test`, which holds `text`.
    fn with_file(test: &str, text: &str) -> (Fake, App, PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("lokyr-app-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        std::fs::write(&path, text).unwrap();
        let config = Config::read(&path).unwrap();
        let mut fake = Fake::default();
        let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
        fake.start(&mut app);
        assert!(app.config_file.as_ref().unwrap().watcher.is_some());
        (fake, app, path)
    }

    /// Tells the app that its file changed, as its watch does, and lets
    /// the changes settle.
    fn change(fake: &mut Fake, app: &mut App) {
        let file = app.config_file.as_ref().unwrap();
        file.changed.store(true, Ordering::Release);
        fake.send_app(app, AppEvent::Wake);
        // Another change meanwhile reads the file once.
        app.config_file
            .as_ref()
            .unwrap()
            .changed
            .store(true, Ordering::Release);
        fake.send_app(app, AppEvent::Wake);
        let reading = app.config_file.as_ref().unwrap().reading;
        assert!(reading.is_some());
        // The one timer, the reading's, as the window has no focus to blink.
        let timers: Vec<_> = fake.timers().collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(Some(timers[0].0), reading);
        fake.fire_timers(app, timers[0].1);
        assert_eq!(app.config_file.as_ref().unwrap().reading, None);
    }

    #[test]
    fn a_changed_config_applies_whole_once_its_changes_settle() {
        let (mut fake, mut app, path) = with_file("valid", "[cursor]\nshape = \"bar\"\n");
        assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        assert_eq!(fake.option_as_meta(), (false, false));
        std::fs::write(
            &path,
            "[cursor]\nshape = \"hollow\"\nblink = false\n[keyboard]\noption-as-meta = \"right\"\n",
        )
        .unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.config.cursor.shape, cursor::Shape::Hollow);
        assert!(!app.config.cursor.blink);
        assert!(fake.window(window).unwrap().frame_requested);
        // The platform learns which Option acts as Meta.
        assert_eq!(fake.option_as_meta(), (false, true));
        // A file that goes gives the defaults.
        std::fs::remove_file(&path).unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.config, Config::default());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_saved_config_reloads_through_the_watch() {
        let (mut fake, mut app, path) = with_file("watched", "[cursor]\nshape = \"bar\"\n");
        std::fs::write(&path, "[cursor]\nshape = \"underline\"\n").unwrap();
        // The watch's thread wakes the app, and the reading follows.
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Underline
        });
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_config_s_directory_is_made_at_the_start_and_watched() {
        let home = std::env::temp_dir().join(format!("lokyr-app-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        // A home without `.config`.
        let path = home.join(".config").join("lokyr").join("config.toml");
        let mut fake = Fake::default();
        let mut app = App::new(None, Config::default(), Some(path.clone())).with_program(quiet());
        fake.start(&mut app);
        assert!(path.parent().unwrap().is_dir());
        assert!(app.config_file.as_ref().unwrap().watcher.is_some());
        std::fs::write(&path, "[cursor]\nshape = \"hollow\"\n").unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Hollow
        });
        // A directory that goes gives the defaults.
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config == Config::default()
        });
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn each_change_puts_the_reading_off() {
        let (mut fake, mut app, path) = with_file("settle", "");
        let changed = |fake: &mut Fake, app: &mut App| {
            let file = app.config_file.as_ref().unwrap();
            file.changed.store(true, Ordering::Release);
            fake.send_app(app, AppEvent::Wake);
            app.config_file.as_ref().unwrap().reading.unwrap()
        };
        let first = changed(&mut fake, &mut app);
        let second = changed(&mut fake, &mut app);
        // The first timer is gone, and the one left counts from the last.
        assert_ne!(first, second);
        let timers: Vec<_> = fake.timers().map(|(timer, _)| timer).collect();
        assert_eq!(timers, [second]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn an_invalid_config_changes_nothing() {
        let (mut fake, mut app, path) = with_file("invalid", "[cursor]\nshape = \"bar\"\n");
        std::fs::write(&path, "[cursor]\nshape = \"beam\"\n").unwrap();
        change(&mut fake, &mut app);
        assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// Runs the app, as the main thread's loop would, handing it the wakes
    /// of its programs, the timers that come due and the frames that it asks
    /// for, until `done` holds; fails after a while.
    fn run_until(fake: &mut Fake, app: &mut App, mut done: impl FnMut(&Fake, &App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(fake, app) {
            assert!(Instant::now() < deadline, "not done in time");
            fake.deliver_wake(app);
            fake.fire_timers(app, Instant::now());
            let asked: Vec<WindowId> = fake
                .windows()
                .filter(|&window| fake.window(window).unwrap().frame_requested)
                .collect();
            for window in asked {
                fake.send(app, window, WindowEvent::Frame);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The text of row `row` of a window's screen.
    fn row(app: &App, window: WindowId, row: u16) -> String {
        let snapshot = app.windows[&window].pane.snapshot();
        let line = snapshot.line(row);
        line.cells()
            .iter()
            // A wide character's second cell holds nothing of it.
            .filter(|cell| cell.width() != lokyr_vt_core::page::Width::SpacerTail)
            .map(|cell| match cell.content() {
                lokyr_vt_core::page::Content::Char(c)
                | lokyr_vt_core::page::Content::Grapheme(c) => c,
                _ => ' ',
            })
            .map(|c| if c == '\0' { ' ' } else { c })
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn shell(script: &str) -> Program {
        Program::command("/bin/sh", ["-c", script])
    }

    #[test]
    fn a_window_shows_its_program_s_output() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("printf 'hello'; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "hello");
        // At the window's grid: 1600 by 1000 pixels, less 8 of padding at
        // each edge, of cells of 8 by 16.
        let snapshot = app.windows[&window].pane.snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (198, 61));
    }

    /// The config of a window without padding.
    fn unpadded() -> Config {
        let mut config = Config::default();
        config.window.padding_x = 0.0.try_into().unwrap();
        config.window.padding_y = 0.0.try_into().unwrap();
        config
    }

    /// A size of a window's content, in points, at a scale of 2: cells of 8
    /// by 16 pixels make a column of 4 points and a row of 8.
    fn points(width: f64, height: f64) -> WindowSize {
        WindowSize::new(Size { width, height }, 2.0)
    }

    /// The text of every row of a window's screen.
    fn screen(app: &App, window: WindowId) -> Vec<String> {
        let rows = app.windows[&window].pane.snapshot().rows();
        (0..rows).map(|at| row(app, window, at)).collect()
    }

    #[test]
    fn the_grid_fits_the_window_less_its_padding() {
        let (mut fake, mut app, path) = with_file("padding", "[window]\npadding-x = 10\n");
        // 1600 by 1000 pixels, less 20 at the left and the right and 8 at
        // the top and the bottom.
        let grid = app.grid(points(800.0, 500.0));
        assert_eq!((grid.columns, grid.rows), (195, 61));
        // A window smaller than its padding keeps a cell.
        let grid = app.grid(points(15.0, 5.0));
        assert_eq!((grid.columns, grid.rows), (1, 1));
        // The input method's candidates go at the cursor's cell, within the
        // padding.
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        let (origin, _) = fake.window(window).unwrap().text_area.unwrap();
        assert_eq!((origin.x, origin.y), (10.0, 4.0));
        // A reloaded padding gives the grid that it leaves.
        std::fs::write(&path, "[window]\npadding-x = 0\npadding-y = 0\n").unwrap();
        change(&mut fake, &mut app);
        assert!(fake.window(window).unwrap().frame_requested);
        fake.send(&mut app, window, WindowEvent::Frame);
        let snapshot = app.windows[&window].pane.snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (200, 62));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_translucent_background_makes_the_window_translucent() {
        let (mut fake, mut reloaded, path) = with_file("translucent", "");
        let window = fake.windows().next().unwrap();
        let backdrop = |fake: &mut Fake, app: &mut App| {
            fake.send(app, window, WindowEvent::Frame);
            fake.window(window)
                .unwrap()
                .translucent
                .map(|backdrop| (backdrop.color, backdrop.blur))
        };
        assert_eq!(backdrop(&mut fake, &mut reloaded), None);
        std::fs::write(
            &path,
            "[window]\nbackground-opacity = 0.9\nbackground-blur = 20\n",
        )
        .unwrap();
        change(&mut fake, &mut reloaded);
        // The terminal's default background, of the light variant, with the
        // opacity.
        let light = reloaded.colors(Appearance::Light).background;
        let rgb = |rgb: lokyr_vt_core::page::Rgb| {
            [rgb.r, rgb.g, rgb.b].map(|component| f64::from(f32::from(component) / 255.0))
        };
        let [red, green, blue] = rgb(light);
        let opacity = f64::from(0.9_f32);
        assert_eq!(
            backdrop(&mut fake, &mut reloaded),
            Some(([red, green, blue, opacity], 20.0))
        );
        // The dark variant's background takes its place.
        fake.send_app(&mut reloaded, AppEvent::AppearanceChanged(Appearance::Dark));
        let [red, green, blue] = rgb(reloaded.colors(Appearance::Dark).background);
        assert_eq!(
            backdrop(&mut fake, &mut reloaded),
            Some(([red, green, blue, opacity], 20.0))
        );
        // A blur alone, of an opaque background, leaves the window opaque.
        std::fs::write(&path, "[window]\nbackground-blur = 20\n").unwrap();
        change(&mut fake, &mut reloaded);
        assert_eq!(backdrop(&mut fake, &mut reloaded), None);
        std::fs::write(&path, "[window]\nbackground-opacity = 0.5\n").unwrap();
        change(&mut fake, &mut reloaded);
        let blur = backdrop(&mut fake, &mut reloaded).map(|(_, blur)| blur);
        assert_eq!(blur, Some(0.0));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        // A window opens as the config says.
        let mut config = Config::default();
        config.window.background_opacity = 0.8.try_into().unwrap();
        config.window.background_blur = 12.0.try_into().unwrap();
        let mut fake = Fake::default();
        let mut app = app(config, quiet());
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let opened = fake.window(window).unwrap().translucent;
        assert_eq!(opened.map(|backdrop| backdrop.blur), Some(12.0));
    }

    #[test]
    fn a_resize_storm_tells_the_program_its_size_at_its_pace() {
        let mut fake = Fake::default();
        // The program prints its size at each SIGWINCH.
        let script = "trap 'stty size' WINCH; echo ready; while :; do sleep 0.02; done";
        let mut app = app(unpadded(), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "ready");
        // A live resize's steps, as fast as they come.
        let start = Instant::now();
        let mut told = Vec::new();
        let grids: Vec<Grid> = (0..50)
            .map(|step| {
                let size = points(400.0 + f64::from(step) * 4.0, 300.0);
                assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
                told.push(app.windows[&window].pane.told());
                app.grid(size)
            })
            .collect();
        let elapsed = start.elapsed();
        let last = grids[49];
        assert_eq!((last.columns, last.rows), (149, 37));
        // The terminal takes each step at once, and the program the first,
        // then one every 50 ms at most.
        let pane = &app.windows[&window].pane;
        assert_eq!(pane.snapshot().cols(), last.columns);
        assert_eq!(told[0], grids[0]);
        told.dedup();
        let paced = elapsed.as_millis() / RESIZE_PACE.as_millis() + 1;
        assert!(
            told.len() as u128 <= paced,
            "{} sizes in {elapsed:?}",
            told.len()
        );
        // Then the last step, once, as its timer comes: the timer of a size
        // told since it started comes first, and starts it again.
        for _ in 0..2 {
            let Some((timer, at)) = app.resize_timer else {
                break;
            };
            assert_eq!(fake.timers().collect::<Vec<_>>(), [(timer, at)]);
            assert_eq!(fake.fire_timers(&mut app, at), 1);
        }
        assert_eq!(app.resize_timer, None);
        assert_eq!(app.windows[&window].pane.told(), last);
        // The program saw few sizes, and the last one.
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window).iter().any(|line| line == "37 149")
        });
        let sizes: Vec<String> = screen(&app, window)
            .into_iter()
            .skip(1)
            .filter(|line| !line.is_empty())
            .collect();
        assert!(sizes.len() <= told.len() + 1, "{sizes:?}");
        assert_eq!(sizes.last().map(String::as_str), Some("37 149"));
    }

    #[test]
    fn the_screen_and_its_scrollback_reflow_with_the_window() {
        let mut fake = Fake::default();
        let script = "seq 1 3000; printf '%150s' '' | tr ' ' x; sleep 10";
        let mut app = app(unpadded(), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let line = "x".repeat(150);
        run_until(&mut fake, &mut app, |_, app| row(app, window, 61) == line);
        let before = screen(&app, window);
        assert_eq!(before[60], "3000");
        // Half as wide, the line takes two rows, and the screen keeps its
        // height.
        fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 500.0)));
        let narrow = screen(&app, window);
        assert_eq!(narrow.len(), 62);
        assert_eq!(narrow[59..], ["3000", &line[..100], &line[100..]]);
        // Through other sizes, taller and shorter too, and back, the screen
        // is as it was: no row lost, none doubled.
        for (width, height) in [
            (52.0, 500.0),
            (244.0, 200.0),
            (1000.0, 900.0),
            (148.0, 500.0),
        ] {
            fake.send(
                &mut app,
                window,
                WindowEvent::Resized(points(width, height)),
            );
        }
        fake.send(&mut app, window, WindowEvent::Resized(points(800.0, 500.0)));
        assert_eq!(screen(&app, window), before);
    }

    #[test]
    fn a_synchronized_update_shows_whole() {
        let mut fake = Fake::default();
        let script = "printf 'before\\r\\n'; sleep 0.3; printf '\\033[?2026hhalf'; sleep 0.5; \
                      printf ' done\\033[?2026l'; sleep 10";
        let mut app = app(Config::default(), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let mut seen = Vec::new();
        run_until(&mut fake, &mut app, |_, app| {
            let shown = row(app, window, 1);
            if seen.last() != Some(&shown) {
                seen.push(shown.clone());
            }
            shown == "half done"
        });
        assert!(!seen.iter().any(|shown| shown == "half"), "{seen:?}");
        assert_eq!(row(&app, window, 0), "before");
    }

    /// The app with `close-on-exit` as `close`, whose window runs `script`,
    /// once its program has ended and the window has done what `close`
    /// says.
    fn ended(close: CloseOnExit, script: &str) -> (Fake, App, WindowId) {
        let mut config = Config::default();
        config.window.close_on_exit = close;
        let mut fake = Fake::default();
        let mut app = app(config, shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |fake, app| {
            fake.windows().count() == 0 || app.windows[&window].pane.is_kept()
        });
        (fake, app, window)
    }

    fn key() -> WindowEvent {
        typed(lokyr_platform_core::KeyCode::KeyA, "a", false)
    }

    /// A key, of `code`, that types `text`, with Command held or not.
    fn typed(code: lokyr_platform_core::KeyCode, text: &str, command: bool) -> WindowEvent {
        WindowEvent::Key(lokyr_platform_core::KeyEvent {
            code,
            state: KeyState::Pressed,
            mods: lokyr_platform_core::Modifiers {
                super_key: command,
                ..lokyr_platform_core::Modifiers::default()
            },
            text: text.to_owned(),
            unmodified: text.to_owned(),
            plain: text.to_owned(),
        })
    }

    #[test]
    fn keys_reach_the_program_but_command_s_do_not() {
        use lokyr_platform_core::KeyCode;

        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        // `cat`, which the terminal echoes, and which writes back each line.
        fake.send(&mut app, window, typed(KeyCode::KeyH, "h", true));
        fake.send(&mut app, window, typed(KeyCode::KeyO, "o", false));
        fake.send(&mut app, window, typed(KeyCode::KeyK, "k", false));
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == "ok");
        assert_eq!(row(&app, window, 0), "ok");
    }

    #[test]
    fn a_clean_exit_closes_the_window_and_another_keeps_it_until_a_key() {
        let (fake, _, _) = ended(CloseOnExit::Clean, "exit 0");
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());

        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "printf 'oops'; exit 3");
        assert_eq!(fake.windows().count(), 1);
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
        });
        assert_eq!(row(&app, window, 0), "oops");
        fake.send(&mut app, window, key());
        assert_eq!(fake.windows().count(), 0);
        assert!(fake.has_quit());
    }

    #[test]
    fn a_kept_window_closes_with_a_key_that_types_or_acts_alone() {
        use lokyr_platform_core::KeyCode;

        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "exit 3");
        // A lock, Fn, a modifier and a Command key leave it open.
        for key in [
            typed(KeyCode::CapsLock, "", false),
            typed(KeyCode::Fn, "", false),
            typed(KeyCode::ShiftLeft, "", false),
            typed(KeyCode::KeyH, "h", true),
        ] {
            fake.send(&mut app, window, key);
            assert_eq!(fake.windows().count(), 1);
        }
        // What an input method composes does not show; its commit closes.
        let composing = Ime::Preedit {
            text: "に".to_owned(),
            selection: 3..3,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        assert_eq!(app.windows[&window].preedit, None);
        fake.send(
            &mut app,
            window,
            WindowEvent::Ime(Ime::Commit("日".to_owned())),
        );
        assert_eq!(fake.windows().count(), 0);
    }

    #[test]
    fn a_program_that_cannot_start_leaves_a_window_that_a_key_closes() {
        let mut fake = Fake::default();
        let missing = Program::command("/nonexistent/lokyr-program", [] as [&str; 0]);
        let mut app = app(Config::default(), missing);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        assert!(row(&app, window, 0).starts_with("[the program cannot start"));
        fake.send(&mut app, window, key());
        assert_eq!(fake.windows().count(), 0);
    }

    #[test]
    fn the_line_of_a_program_that_ended_reads_whatever_it_left_set() {
        // The line drawing character set, insert mode and no autowrap.
        let script = "printf '\\033(0\\033[4h\\033[?7l'; exit 3";
        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, script);
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
        });
    }

    #[test]
    fn always_closes_and_never_keeps() {
        let (fake, _, _) = ended(CloseOnExit::Always, "exit 3");
        assert_eq!(fake.windows().count(), 0);
        let (fake, app, window) = ended(CloseOnExit::Never, "exit 0");
        assert_eq!(fake.windows().count(), 1);
        assert!(app.windows[&window].pane.is_kept());
        // A program that a signal ends.
        let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "kill -9 $$");
        run_until(&mut fake, &mut app, |_, app| {
            row(app, window, 1) == "[the program ended by signal 9; a key closes the window]"
        });
    }

    #[test]
    fn the_input_method_s_text_shows_then_reaches_the_program() {
        use lokyr_platform_core::KeyCode;

        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        // The candidates' place: the cursor's cell, at the top left within
        // the padding of 4 points, in the points of cells of 8 by 16 pixels
        // at a scale of 2.
        let (origin, size) = fake.window(window).unwrap().text_area.unwrap();
        assert_eq!((origin.x, origin.y), (4.0, 4.0));
        assert_eq!((size.width, size.height), (4.0, 8.0));
        let composing = Ime::Preedit {
            text: "にほ".to_owned(),
            selection: 6..6,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        assert_eq!(
            app.windows[&window].preedit,
            Some(Preedit {
                text: "にほ".to_owned(),
                cursor: 6
            })
        );
        assert!(fake.window(window).unwrap().frame_requested);
        fake.send(
            &mut app,
            window,
            WindowEvent::Ime(Ime::Commit("日本".to_owned())),
        );
        assert_eq!(app.windows[&window].preedit, None);
        fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
        // `cat` writes back the line.
        run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == "日本");
        // An empty preedit ends the composition with nothing typed.
        let composing = Ime::Preedit {
            text: "k".to_owned(),
            selection: 1..1,
        };
        fake.send(&mut app, window, WindowEvent::Ime(composing));
        let ended = Ime::Preedit {
            text: String::new(),
            selection: 0..0,
        };
        fake.send(&mut app, window, WindowEvent::Ime(ended));
        assert_eq!(app.windows[&window].preedit, None);
    }

    #[test]
    fn output_starts_the_blinking_again() {
        let mut fake = Fake::default();
        let mut app = app(Config::default(), shell("sleep 0.3; printf x; sleep 10"));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let focused = Instant::now();
        fake.send(&mut app, window, WindowEvent::Focused(true));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "x");
        // The output came at least 0.3 s after the focus: 15.1 s after the
        // focus, the cursor still blinks, as its timeout counts from it.
        let later = focused + cursor::BLINK_TIMEOUT + Duration::from_millis(100);
        app.cursor(&mut fake, window, later);
        assert!(app.windows[&window].blinking);
    }

    /// A directory of the test's own, made empty.
    fn directory(test: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("lokyr-app-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    /// The startup lines of a shell whose prompt is as oh-my-posh draws it
    /// with its shell integration: three lines of output above it, then a
    /// first line computed once, as wide as the terminal, `left` and
    /// `right` at its edges, and the input line, marked with OSC 133.
    const PROMPT_OUTPUT: &str = "for n in 1 2 3; do echo \"output $n\"; done\n\
        cols=$(stty size | cut -d' ' -f2)\n\
        first=\"left$(printf '%*s' $((cols - 9)) '')right\"\n";

    /// Drags the window of `app` through `widths`, in points, a step every
    /// 8 ms, as a live resize does.
    fn drag(fake: &mut Fake, app: &mut App, window: WindowId, widths: &[f64]) {
        for pair in widths.windows(2) {
            for step in 0..=20 {
                let width = pair[0] + (pair[1] - pair[0]) * f64::from(step) / 20.0;
                fake.send(app, window, WindowEvent::Resized(points(width, 500.0)));
                let until = Instant::now() + Duration::from_millis(8);
                run_until(fake, app, |_, _| Instant::now() > until);
            }
        }
    }

    /// Runs the shell that `command` starts through a drag from 200
    /// columns to 75, 175 and 100, and waits for its screen to show
    /// `expected`, its rows without their spaces at either end, and to keep
    /// it a while.
    fn prompt_through_a_drag(command: &str, expected: &[&str]) {
        let mut fake = Fake::default();
        let mut app = app(unpadded(), shell(command));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        let trimmed = |app: &App| -> Vec<String> {
            screen(app, window)
                .iter()
                .take(expected.len())
                .map(|line| line.trim().to_owned())
                .collect()
        };
        run_until(&mut fake, &mut app, |_, app| {
            screen(app, window).iter().any(|line| line.starts_with('>'))
        });
        drag(&mut fake, &mut app, window, &[800.0, 300.0, 700.0, 400.0]);
        run_until(&mut fake, &mut app, |_, app| trimmed(app) == expected);
        let until = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > until);
        assert_eq!(trimmed(&app), expected);
    }

    #[test]
    fn a_zsh_prompt_draws_once_through_a_drag() {
        let directory = directory("zsh-prompt");
        let rc = format!(
            r#"{PROMPT_OUTPUT}PROMPT=$'%{{\e]133;D;0\a\e]133;A\a%}}'"$first"$'\n> %{{\e]133;B\a%}}'
preexec() {{ print -n '\e]133;C\a' }}
print -z 'echo typed'
"#
        );
        std::fs::write(directory.join(".zshrc"), rc).unwrap();
        let zsh = format!("ZDOTDIR={} exec /bin/zsh -d -i", directory.display());
        // The first line, of 200 columns, takes two rows at 100; zsh draws
        // the prompt again, with what was typed, where the cleared one was.
        let expected = [
            "output 1",
            "output 2",
            "output 3",
            "left",
            "right",
            "> echo typed",
            "",
        ];
        prompt_through_a_drag(&zsh, &expected);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn a_bash_prompt_keeps_the_lines_that_bash_does_not_draw_again() {
        let directory = directory("bash-prompt");
        let rc = directory.join("rc");
        let prompt = format!(
            r#"{PROMPT_OUTPUT}PS1='\[\e]133;D;0\a\e]133;A;redraw=last\a\]'"$first"'\n> \[\e]133;B\a\]'
"#
        );
        std::fs::write(&rc, prompt).unwrap();
        let bash = format!(
            "BASH_SILENCE_DEPRECATION_WARNING=1 exec /bin/bash --noprofile --rcfile {} -i",
            rc.display()
        );
        // bash draws the last line again, and the first reflows.
        let expected = ["output 1", "output 2", "output 3", "left", "right", ">", ""];
        prompt_through_a_drag(&bash, &expected);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn other_commands_do_nothing() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
        assert!(!fake.has_quit());
        assert_eq!(fake.windows().count(), 1);
    }
}

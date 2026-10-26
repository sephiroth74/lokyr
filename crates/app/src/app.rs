//! The application: its windows, each with the pane of its program, what
//! they draw, its main menu, and its config, which it reads again when the
//! file changes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bolt_font_coretext::CoreText;
use bolt_kernel::config::Config;
use bolt_platform_core::{
    AppEvent, Appearance, Handler, Ime, KeyState, MainMenu, Menu, MenuItem, Platform, Point,
    Shortcut, ShortcutKey, Size, TimerId, WindowAttributes, WindowEvent, WindowId, WindowSize,
};
use bolt_pty::Program;
use bolt_render_core::{CursorShape, DisplayList, Item, Renderer};
use bolt_render_metal::{Gpu, MetalRenderer, Surface};
use bolt_text_core::FontConfig;
use bolt_ui_core::cursor::{self, Blink};
use bolt_ui_core::font::Font;
use bolt_ui_core::pane::{self as cells, CursorLook, Preedit};
use bolt_ui_core::theme::Theme;

use crate::keys;
use crate::pane::{Exited, Grid, Pane};
use crate::watch::Watcher;

/// The command that quits.
const QUIT: &str = "quit";

/// The size of a new window's content, in points.
const WINDOW_SIZE: Size = Size {
    width: 800.0,
    height: 500.0,
};

/// How long the app waits, after a change of the config file, for the
/// changes that come with it, as an editor's write and rename do, before it
/// reads the file once.
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
            "bolt: the font family `{family}` is not installed; the text takes `{}`",
            font.family()
        );
    }
    if let Some(family) = font.missing_non_ascii_family() {
        eprintln!(
            "bolt: the font family `{family}` is not installed; the text past ASCII takes `{}`",
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
        .inspect_err(|error| eprintln!("bolt: {error}"))
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
                eprintln!("bolt: {error}");
                eprintln!("bolt: none of the config applies, and the one in force stays");
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
            eprintln!("bolt: {error}");
            eprintln!("bolt: none of the config applies, and the one in force stays");
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
            title: "Bolt".to_owned(),
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
                eprintln!("bolt: {error}");
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
    fn colors(&self, appearance: Appearance) -> bolt_vt_core::terminal::Colors {
        self.theme
            .variant(appearance == Appearance::Dark)
            .colors
            .clone()
    }

    /// The grid that fits a window of `size`, in the cells of the font at
    /// its display's scale.
    fn grid(&mut self, size: WindowSize) -> Grid {
        let (width, height) = match &mut self.drawing {
            Some(drawing) => {
                drawing.font.set_scale(size.scale as f32);
                let cell = drawing.font.cell();
                (cell.width, cell.height)
            }
            None => UNDRAWN_CELL,
        };
        let fit =
            |pixels: u32, cell: u32| (pixels / cell.max(1)).clamp(1, u32::from(u16::MAX)) as u16;
        let pixels = |cell: u32| u16::try_from(cell).unwrap_or(u16::MAX);
        Grid {
            columns: fit(size.pixels.width, width),
            rows: fit(size.pixels.height, height),
            cell_width: pixels(width),
            cell_height: pixels(height),
        }
    }

    /// The surface of `window`, if the app draws and the platform has a
    /// layer for it.
    fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<Surface> {
        let drawing = self.drawing.as_ref()?;
        let handle = platform.window_handle(window)?;
        Surface::new(&drawing.gpu, handle)
            .inspect_err(|error| eprintln!("bolt: {error}"))
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

    /// Draws `window`, of `size`: its program's screen, brought up to date,
    /// in the grid that fits its content, with the cursor.
    fn draw(&mut self, platform: &mut dyn Platform, window: WindowId, size: WindowSize) {
        let grid = self.grid(size);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        state.pane.resize(grid);
        state.pane.update();
        // The input method's candidates go at the cursor's cell.
        let at = state.pane.snapshot().cursor().position;
        let (width, height) = (f64::from(grid.cell_width), f64::from(grid.cell_height));
        platform.set_text_area(
            window,
            Point {
                x: f64::from(at.x) * width / size.scale,
                y: f64::from(at.y) * height / size.scale,
            },
            Size {
                width: width / size.scale,
                height: height / size.scale,
            },
        );
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
        let mut batch = cells::cells(snapshot, &cell, 0, 0, None, cursor, &mut glyph);
        if let Some(preedit) = &state.preedit {
            cells::preedit(&mut batch, snapshot, &cell, preedit, &mut glyph);
        }
        let list = DisplayList {
            background: cells::background(snapshot),
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
            eprintln!("bolt: {error}");
        }
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        platform.set_main_menu(&main_menu());
        let (left, right) = self.config.keyboard.option_as_meta.sides();
        platform.set_option_as_meta(left, right);
        if let Some(file) = &mut self.config_file {
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
                if let Some(file) = &mut self.config_file
                    && file.changed.swap(false, Ordering::AcqRel)
                    && file.reading.is_none()
                {
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
            // again, on; a key, not a modifier alone, closes a window whose
            // program ended.
            WindowEvent::Key(key) => {
                let options = keys::options(self.config.keyboard.option_as_meta);
                let Some(state) = self.windows.get_mut(&window) else {
                    return;
                };
                let pressed = key.state != KeyState::Released;
                let modifier = matches!(
                    keys::terminal_key(&key).map(|key| key.key),
                    Some(bolt_vt_core::input::Key::Modifier(_))
                );
                if state.pane.is_kept() {
                    if pressed && !modifier {
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
                match ime {
                    Ime::Preedit { text, selection } => {
                        state.preedit = (!text.is_empty()).then_some(Preedit {
                            text,
                            cursor: selection.start,
                        });
                    }
                    Ime::Commit(text) => {
                        state.preedit = None;
                        if !state.pane.is_kept() {
                            state.pane.text(&text);
                        }
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

/// The main menu: the application's menu, with Quit.
fn main_menu() -> MainMenu {
    MainMenu {
        menus: vec![Menu {
            title: "Bolt".to_owned(),
            items: vec![MenuItem::Command {
                title: "Quit Bolt".to_owned(),
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
    use bolt_kernel::config::CloseOnExit;
    use bolt_platform_core::fake::Fake;
    use bolt_ui_core::cursor::CursorConfig;

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
            std::env::temp_dir().join(format!("bolt-app-{test}-{}", std::process::id()));
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
            .filter(|cell| cell.width() != bolt_vt_core::page::Width::SpacerTail)
            .map(|cell| match cell.content() {
                bolt_vt_core::page::Content::Char(c) | bolt_vt_core::page::Content::Grapheme(c) => {
                    c
                }
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
        // At the window's grid: 1600 by 1000 pixels, of cells of 8 by 16.
        let snapshot = app.windows[&window].pane.snapshot();
        assert_eq!((snapshot.cols(), snapshot.rows()), (200, 62));
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
        typed(bolt_platform_core::KeyCode::KeyA, "a", false)
    }

    /// A key, of `code`, that types `text`, with Command held or not.
    fn typed(code: bolt_platform_core::KeyCode, text: &str, command: bool) -> WindowEvent {
        WindowEvent::Key(bolt_platform_core::KeyEvent {
            code,
            state: KeyState::Pressed,
            mods: bolt_platform_core::Modifiers {
                super_key: command,
                ..bolt_platform_core::Modifiers::default()
            },
            text: text.to_owned(),
            unmodified: text.to_owned(),
            plain: text.to_owned(),
        })
    }

    #[test]
    fn keys_reach_the_program_but_command_s_do_not() {
        use bolt_platform_core::KeyCode;

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
        use bolt_platform_core::KeyCode;

        let (mut fake, mut app) = started();
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Frame);
        // The candidates' place: the cursor's cell, at the top left, in the
        // points of cells of 8 by 16 pixels at a scale of 2.
        let (origin, size) = fake.window(window).unwrap().text_area.unwrap();
        assert_eq!((origin.x, origin.y), (0.0, 0.0));
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
    fn other_commands_do_nothing() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
        assert!(!fake.has_quit());
        assert_eq!(fake.windows().count(), 1);
    }
}

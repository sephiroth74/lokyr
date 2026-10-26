//! The application: its windows, what they draw, and its main menu.

use std::collections::BTreeMap;

use bolt_platform_core::{
    AppEvent, Appearance, Handler, MainMenu, Menu, MenuItem, Platform, Shortcut, ShortcutKey, Size,
    WindowAttributes, WindowEvent, WindowId, WindowSize,
};
use bolt_render_core::Color;
use bolt_render_metal::{Gpu, Surface};

/// The command that quits.
const QUIT: &str = "quit";

/// The size of a new window's content, in points.
const WINDOW_SIZE: Size = Size {
    width: 800.0,
    height: 500.0,
};

/// The application, which opens a window when it starts, and quits with its
/// last window or from its menu.
#[derive(Debug, Default)]
pub struct App {
    gpu: Option<Gpu>,
    /// The open windows, each with its surface, which a window has once the
    /// app has a GPU and the platform a layer to draw into.
    windows: BTreeMap<WindowId, Option<Surface>>,
}

impl App {
    /// An application that draws its windows with `gpu`, or nothing without
    /// one.
    pub fn new(gpu: Option<Gpu>) -> Self {
        Self {
            gpu,
            windows: BTreeMap::new(),
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
                self.windows.insert(window, surface);
                if let Some(size) = platform.window_size(window) {
                    self.draw(platform, window, size);
                }
            }
            Err(error) => {
                eprintln!("bolt: {error}");
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
        }
    }

    /// The surface of `window`, if the app has a GPU and the platform a
    /// layer for it.
    fn surface(&self, platform: &dyn Platform, window: WindowId) -> Option<Surface> {
        let gpu = self.gpu.as_ref()?;
        let handle = platform.window_handle(window)?;
        Surface::new(gpu, handle)
            .inspect_err(|error| eprintln!("bolt: {error}"))
            .ok()
    }

    /// Sizes the surface of `window` to its content, of `size`, and draws
    /// the background.
    fn draw(&self, platform: &dyn Platform, window: WindowId, size: WindowSize) {
        let Some(Some(surface)) = self.windows.get(&window) else {
            return;
        };
        surface.resize(size.pixels.width, size.pixels.height, size.scale);
        if let Err(error) = surface.clear(background(platform.appearance())) {
            eprintln!("bolt: {error}");
        }
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        platform.set_main_menu(&main_menu());
        self.open_window(platform);
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        match event {
            AppEvent::Command(command) if command == QUIT => platform.quit(),
            AppEvent::QuitRequested => platform.quit(),
            // What woke the application changed what a window shows.
            AppEvent::Wake => {
                for &window in self.windows.keys() {
                    platform.request_frame(window);
                }
            }
            AppEvent::AppearanceChanged(_) => {
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
            WindowEvent::CloseRequested => {
                // The surface goes before its window.
                self.windows.remove(&window);
                platform.close_window(window);
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
            _ => {}
        }
    }
}

/// The background of a window's content, which follows the system's
/// appearance.
fn background(appearance: Appearance) -> Color {
    match appearance {
        Appearance::Light => Color::from_rgb8(0xfa, 0xfa, 0xfa),
        Appearance::Dark => Color::from_rgb8(0x1e, 0x1e, 0x20),
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
    use bolt_platform_core::fake::Fake;

    use super::*;

    fn started() -> (Fake, App) {
        let mut fake = Fake::default();
        let mut app = App::default();
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
        assert!(app.windows[&window].is_none());
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

    #[test]
    fn the_background_follows_the_appearance() {
        assert_ne!(background(Appearance::Light), background(Appearance::Dark));
    }

    #[test]
    fn other_commands_do_nothing() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
        assert!(!fake.has_quit());
        assert_eq!(fake.windows().count(), 1);
    }
}

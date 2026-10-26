//! The application: its windows and its main menu.

use bolt_platform_core::{
    AppEvent, Handler, MainMenu, Menu, MenuItem, Platform, Shortcut, ShortcutKey, Size,
    WindowAttributes, WindowEvent, WindowId,
};

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
    windows: Vec<WindowId>,
}

impl App {
    fn open_window(&mut self, platform: &mut dyn Platform) {
        let attributes = WindowAttributes {
            title: "Bolt".to_owned(),
            size: WINDOW_SIZE,
        };
        match platform.create_window(&attributes) {
            Ok(window) => self.windows.push(window),
            Err(error) => {
                eprintln!("bolt: {error}");
                if self.windows.is_empty() {
                    platform.quit();
                }
            }
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
            _ => {}
        }
    }

    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            platform.close_window(window);
            self.windows.retain(|&open| open != window);
            if self.windows.is_empty() {
                platform.quit();
            }
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
    fn other_commands_do_nothing() {
        let (mut fake, mut app) = started();
        fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
        assert!(!fake.has_quit());
        assert_eq!(fake.windows().count(), 1);
    }
}

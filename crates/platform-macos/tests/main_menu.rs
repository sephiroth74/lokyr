//! The main menu's items: a command's runs it as the app's event, a dimmed
//! one is disabled, and a ticked one shows its tick, which a new menu keeps.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, MainMenu, Menu, MenuItem, Platform, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSControlStateValueOn, NSMenu};

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// Each item's state, enabled and ticked, as the menu showed it.
        states: Vec<Vec<(bool, bool)>>,
        commands: Vec<String>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    fn main_menu() -> MainMenu {
        let item = |command: &str| MenuItem::Command {
            title: command.to_owned(),
            command: command.to_owned(),
            shortcut: None,
        };
        MainMenu {
            menus: vec![
                Menu {
                    title: "Probe".to_owned(),
                    items: Vec::new(),
                    role: None,
                },
                Menu {
                    title: "View".to_owned(),
                    items: vec![
                        item("test.light"),
                        MenuItem::Submenu(Menu {
                            title: "More".to_owned(),
                            items: vec![item("test.dark")],
                            role: None,
                        }),
                    ],
                    role: None,
                },
            ],
        }
    }

    /// The probe's View menu, and its submenu.
    fn menus() -> (Retained<NSMenu>, Retained<NSMenu>) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let bar = NSApplication::sharedApplication(mtm)
            .mainMenu()
            .expect("the main menu");
        let view = bar
            .itemAtIndex(1)
            .and_then(|item| item.submenu())
            .expect("the View menu");
        let more = view
            .itemAtIndex(1)
            .and_then(|item| item.submenu())
            .expect("the submenu");
        (view, more)
    }

    /// The items' states, as the menus validate them.
    fn states() -> Vec<(bool, bool)> {
        let (view, more) = menus();
        view.update();
        more.update();
        [(&view, 0), (&more, 0)]
            .into_iter()
            .map(|(menu, index)| {
                let item = menu.itemAtIndex(index).expect("an item");
                (item.isEnabled(), item.state() == NSControlStateValueOn)
            })
            .collect()
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform.set_main_menu(&main_menu());
            let mut seen = self.0.borrow_mut();
            seen.states.push(states());
            platform.set_command_checked("test.dark", true);
            platform.set_command_enabled("test.light", false);
            seen.states.push(states());
            // A new menu keeps the ticks and the dimmed items.
            platform.set_main_menu(&main_menu());
            seen.states.push(states());
            platform.set_command_checked("test.dark", false);
            platform.set_command_checked("test.light", true);
            platform.set_command_enabled("test.light", true);
            seen.states.push(states());
            let (_, more) = menus();
            more.performActionForItemAtIndex(0);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if let AppEvent::Command(command) = event {
                self.0.borrow_mut().commands.push(command);
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("main_menu", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(
            seen.states,
            [
                [(true, false), (true, false)],
                [(false, false), (true, true)],
                [(false, false), (true, true)],
                [(true, true), (true, false)],
            ],
            "{seen:?}"
        );
        assert_eq!(seen.commands, ["test.dark"], "{seen:?}");
        println!("main_menu: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

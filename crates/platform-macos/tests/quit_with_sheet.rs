//! Quitting while an alert is a window's sheet: `run` returns, and the
//! sheet's answer, which AppKit gives as the window closes, is dropped.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        wakes: usize,
        answered: bool,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "sheet".to_owned(),
                    size: Size {
                        width: 400.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("the window opens");
            platform.show_alert(Some(window), &common::alert());
            common::wake_after(platform.waker(), 500);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            let mut seen = self.0.borrow_mut();
            match event {
                AppEvent::Wake => {
                    seen.wakes += 1;
                    platform.quit();
                }
                AppEvent::AlertClosed { .. } => seen.answered = true,
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("quit_with_sheet", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.wakes, 1, "{seen:?}");
        assert!(!seen.answered, "{seen:?}");
        println!("quit_with_sheet: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

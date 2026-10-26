//! Quitting while an alert without a window runs its own loop: the alert's
//! loop ends, then the application's, and `run` returns, with nothing handed
//! on after the quit.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use bolt_platform_core::{AppEvent, Handler, Platform, WindowEvent, WindowId};

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        wakes: usize,
        answered: bool,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform.show_alert(None, &common::alert());
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
        common::deadline("quit_in_modal", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        bolt_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.wakes, 1, "{seen:?}");
        assert!(!seen.answered, "{seen:?}");
        println!("quit_in_modal: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

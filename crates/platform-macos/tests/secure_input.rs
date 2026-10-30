//! Secure Keyboard Entry: the app turns it on, the system tells it on, and
//! the app turns it off, its calls balanced, twice over.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{AppEvent, Handler, Platform, WindowEvent, WindowId};

    use crate::common;

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn IsSecureEventInputEnabled() -> u8;
    }

    /// Whether any app has the secure entry on, as the system tells it.
    fn enabled() -> bool {
        // SAFETY: the system's call, of no arguments, which only reads.
        unsafe { IsSecureEventInputEnabled() != 0 }
    }

    /// Whether the system told it on before, and after each of the app's
    /// calls: on, on again, off, off again, on and off.
    #[derive(Debug, Default)]
    struct Seen(Vec<bool>);

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let mut seen = self.0.borrow_mut();
            seen.0.push(enabled());
            for on in [true, true, false, false, true, false] {
                platform.set_secure_input(on);
                seen.0.push(enabled());
            }
            common::wake_after(platform.waker(), 100);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event == AppEvent::Wake {
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("secure_input", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.0[1..3], [true, true], "{seen:?}");
        assert!(seen.0[5], "{seen:?}");
        // Off, where no other app has it on, as a password's field may.
        if !seen.0[0] {
            assert_eq!([seen.0[3], seen.0[4], seen.0[6]], [false; 3], "{seen:?}");
        }
        println!("secure_input: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

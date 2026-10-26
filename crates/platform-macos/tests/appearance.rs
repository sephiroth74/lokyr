//! The application's appearance: a change, whether of the system's or of the
//! application's own, comes as an event, once.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use bolt_platform_core::{AppEvent, Appearance, Handler, Platform, WindowEvent, WindowId};
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    };

    use crate::common;

    /// Gives the application the appearance `appearance`, or the system's.
    fn set(appearance: Option<Appearance>) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        // SAFETY: AppKit's constants.
        let name = appearance.map(|appearance| unsafe {
            match appearance {
                Appearance::Light => NSAppearanceNameAqua,
                Appearance::Dark => NSAppearanceNameDarkAqua,
            }
        });
        let appearance = name.and_then(NSAppearance::appearanceNamed);
        NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
    }

    fn other(appearance: Appearance) -> Appearance {
        match appearance {
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Light,
        }
    }

    #[derive(Debug, Default)]
    struct Seen {
        first: Option<Appearance>,
        changes: Vec<Appearance>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let first = platform.appearance();
            self.0.borrow_mut().first = Some(first);
            set(Some(other(first)));
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            let AppEvent::AppearanceChanged(appearance) = event else {
                return;
            };
            let mut seen = self.0.borrow_mut();
            seen.changes.push(appearance);
            assert_eq!(platform.appearance(), appearance);
            if seen.changes.len() == 1 {
                // The same appearance again changes nothing; then back.
                set(Some(appearance));
                set(seen.first);
            } else {
                set(None);
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("appearance", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        bolt_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let first = seen.first.expect("the application started");
        assert_eq!(seen.changes, [other(first), first], "{seen:?}");
        println!("appearance: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

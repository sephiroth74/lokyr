//! The application's appearance: a change, whether of the system's or of the
//! application's own, which the platform sets, comes as an event, once, and
//! the platform tells the appearance at once. A window of its own
//! appearance keeps it, whatever the application's is.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Appearance, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2_app_kit::{
        NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    };
    use objc2_foundation::NSArray;

    use crate::common;

    /// Gives the application the appearance `appearance`, which the platform
    /// tells at once, or the system's.
    fn set(platform: &mut dyn Platform, appearance: Option<Appearance>) {
        platform.set_appearance(appearance);
        if let Some(appearance) = appearance {
            assert_eq!(platform.appearance(), appearance);
        }
    }

    fn other(appearance: Appearance) -> Appearance {
        match appearance {
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::Light,
        }
    }

    /// The appearance of the application's window.
    fn window_appearance() -> Appearance {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window");
        // SAFETY: AppKit's constants.
        let (aqua, dark) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
        let names = NSArray::from_slice(&[aqua, dark]);
        match window
            .effectiveAppearance()
            .bestMatchFromAppearancesWithNames(&names)
        {
            Some(name) if &*name == dark => Appearance::Dark,
            _ => Appearance::Light,
        }
    }

    #[derive(Debug, Default)]
    struct Seen {
        first: Option<Appearance>,
        changes: Vec<Appearance>,
        /// The window's, as each change came.
        window: Vec<Appearance>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let first = platform.appearance();
            self.0.borrow_mut().first = Some(first);
            // A window of the first appearance, its own.
            let window = platform
                .create_window(&WindowAttributes {
                    title: "An appearance".to_owned(),
                    size: Size {
                        width: 200.0,
                        height: 100.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("a window");
            platform.set_window_appearance(window, Some(first));
            set(platform, Some(other(first)));
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            let AppEvent::AppearanceChanged(appearance) = event else {
                return;
            };
            let mut seen = self.0.borrow_mut();
            seen.changes.push(appearance);
            seen.window.push(window_appearance());
            assert_eq!(platform.appearance(), appearance);
            if seen.changes.len() == 1 {
                // The same appearance again changes nothing; then back.
                set(platform, Some(appearance));
                set(platform, seen.first);
            } else {
                set(platform, None);
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("appearance", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let first = seen.first.expect("the application started");
        assert_eq!(seen.changes, [other(first), first], "{seen:?}");
        // The window kept its own, the first, through both.
        assert_eq!(seen.window, [first, first], "{seen:?}");
        println!("appearance: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

//! A window opened at a frame: its content's frame is that one, out of full
//! screen too; one asked for where no screen is opens on the nearest, of
//! its size, within the screen's area for windows. The system's setting of
//! the windows kept reads as the preferences have it.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{Handler, Platform, ScreenRect, Size, WindowAttributes};
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;
    use objc2_foundation::{NSString, NSUserDefaults};

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// The frame of the window at `AT`, as it tells it and out of full
        /// screen, and of the one asked for far away.
        at: Option<ScreenRect>,
        windowed: Option<ScreenRect>,
        far: Option<ScreenRect>,
        /// The screens' areas for windows, on the screens' plane.
        areas: Vec<ScreenRect>,
        /// The setting as the platform tells it, and the preferences.
        keeps: Option<(bool, bool)>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    // Near the main screen's top left corner, its titlebar's room too.
    const AT: ScreenRect = ScreenRect {
        x: 60.0,
        y: 90.0,
        width: 480.0,
        height: 320.0,
    };

    const FAR: ScreenRect = ScreenRect {
        x: 1.0e5,
        y: 1.0e5,
        width: 480.0,
        height: 320.0,
    };

    fn open(platform: &mut dyn Platform, frame: ScreenRect) -> lokyr_platform_core::WindowId {
        platform
            .create_window(&WindowAttributes {
                title: "An opened frame".to_owned(),
                size: Size {
                    width: 200.0,
                    height: 100.0,
                },
                remember: false,
                restore: true,
                restore_size: true,
                frame: Some(frame),
            })
            .expect("a window")
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let at = open(platform, AT);
            let far = open(platform, FAR);
            let mtm = MainThreadMarker::new().expect("the main thread");
            let screens = NSScreen::screens(mtm);
            let top = screens.firstObject().map_or(0.0, |first| {
                let frame = first.frame();
                frame.origin.y + frame.size.height
            });
            let mut seen = self.0.borrow_mut();
            seen.areas = (0..screens.count())
                .map(|index| {
                    let area = screens.objectAtIndex(index).visibleFrame();
                    ScreenRect {
                        x: area.origin.x,
                        y: top - area.origin.y - area.size.height,
                        width: area.size.width,
                        height: area.size.height,
                    }
                })
                .collect();
            seen.at = platform.window_frame(at);
            seen.windowed = platform.windowed_frame(at);
            seen.far = platform.window_frame(far);
            let preferences = NSUserDefaults::standardUserDefaults()
                .boolForKey(&NSString::from_str("NSQuitAlwaysKeepsWindows"));
            seen.keeps = Some((platform.keeps_windows(), preferences));
            platform.quit();
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: lokyr_platform_core::AppEvent) {}

        fn window_event(
            &mut self,
            _: &mut dyn Platform,
            _: lokyr_platform_core::WindowId,
            _: lokyr_platform_core::WindowEvent,
        ) {
        }
    }

    pub fn main() {
        common::deadline("opened_frame", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.at, Some(AT), "{seen:?}");
        assert_eq!(seen.windowed, Some(AT), "{seen:?}");
        let far = seen.far.expect("the far window's frame");
        assert_eq!((far.width, far.height), (FAR.width, FAR.height), "{seen:?}");
        // Within a screen's area: the nearest's, the last of the plane.
        assert!(
            seen.areas.iter().any(|area| far.x >= area.x
                && far.y >= area.y
                && far.x + far.width <= area.x + area.width
                && far.y + far.height <= area.y + area.height),
            "{seen:?}"
        );
        let (told, preferences) = seen.keeps.expect("the setting");
        assert_eq!(told, preferences, "{seen:?}");
        println!("opened_frame: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

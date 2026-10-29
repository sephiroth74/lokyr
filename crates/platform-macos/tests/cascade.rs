//! A new window opens down and to the right of the window that has the
//! focus, as the system cascades windows; the first one, in the screen's
//! centre.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSRect;

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        first: Option<WindowId>,
        /// The windows' frames, the first's and the second's, as the second
        /// opened.
        frames: Vec<NSRect>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    fn open(platform: &mut dyn Platform) -> WindowId {
        platform
            .create_window(&WindowAttributes {
                title: "A cascade".to_owned(),
                size: Size {
                    width: 320.0,
                    height: 200.0,
                },
            })
            .expect("a window")
    }

    /// The frames of the application's windows, in their order.
    fn frames() -> Vec<NSRect> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        (0..windows.count())
            .map(|index| windows.objectAtIndex(index).frame())
            .collect()
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            self.0.borrow_mut().first = Some(open(platform));
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: AppEvent) {}

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            let mut seen = self.0.borrow_mut();
            // The second, once the first has the focus.
            if event != WindowEvent::Focused(true)
                || seen.first != Some(window)
                || !seen.frames.is_empty()
            {
                return;
            }
            assert_eq!(platform.focused_window(), Some(window));
            open(platform);
            seen.frames = frames();
            platform.quit();
        }
    }

    pub fn main() {
        common::deadline("cascade", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let [first, second] = seen.frames[..] else {
            panic!("two windows: {seen:?}");
        };
        let top = |frame: NSRect| frame.origin.y + frame.size.height;
        assert!(second.origin.x > first.origin.x, "{seen:?}");
        assert!(top(second) < top(first), "{seen:?}");
        assert_eq!(second.size, first.size, "{seen:?}");
        println!("cascade: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

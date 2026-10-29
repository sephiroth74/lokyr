//! The titlebar's points are the window's frame's, which drags the window
//! from them and zooms it at a double click, though the content runs under
//! the bar: the content view's hit test gives them up, and keeps those
//! below the bar.

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
    use objc2_foundation::NSPoint;

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// Whether a point of the bar, and one below it, hit the content.
        bar: Option<bool>,
        below: Option<bool>,
        height: f64,
        bar_height: f64,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "The titlebar".to_owned(),
                    size: Size {
                        width: 400.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                })
                .expect("the window opens");
            let bar = platform.window_size(window).expect("the window's size").bar;
            let mtm = MainThreadMarker::new().expect("the main thread");
            let ns_window = NSApplication::sharedApplication(mtm)
                .windows()
                .firstObject()
                .expect("the window");
            let content = ns_window.contentView().expect("the content view");
            let height = content.bounds().size.height;
            // In the window's coordinates, from its bottom left corner.
            let hits = |y: f64| {
                let point = content.convertPoint_toView(NSPoint::new(100.0, y), None);
                content.hitTest(point).is_some_and(|hit| hit == content)
            };
            let mut seen = self.0.borrow_mut();
            seen.bar = Some(hits(height - bar / 2.0));
            seen.below = Some(hits(20.0));
            (seen.height, seen.bar_height) = (height, bar);
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
        common::deadline("titlebar", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert!(seen.bar_height > 10.0, "{seen:?}");
        assert_eq!(
            (seen.bar, seen.below),
            (Some(false), Some(true)),
            "{seen:?}"
        );
        println!("titlebar: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

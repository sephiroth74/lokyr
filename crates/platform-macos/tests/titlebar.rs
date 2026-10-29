//! The titlebar, which the content runs under: a double click on it, past
//! its title, does what the system's settings say, as the window's frame
//! does over the title, and a single click tells the content nothing.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView};
    use objc2_foundation::{NSPoint, NSString, NSUserDefaults};

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// The system's setting of a double click on a titlebar.
        action: Option<String>,
        /// Whether the window was zoomed, or minimized, after the double
        /// click.
        zoomed: bool,
        minimized: bool,
        /// The buttons that the content learned of.
        buttons: usize,
    }

    struct Probe(Rc<RefCell<Seen>>);

    /// Sends the content view `view` a press of the left button of `clicks`
    /// at `y` points from its bottom.
    fn press(view: &NSView, y: f64, clicks: isize) {
        let window = view.window().expect("the view's window");
        let location = view.convertPoint_toView(NSPoint::new(120.0, y), None);
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDown,
            location,
            NSEventModifierFlags::empty(),
            0.0,
            window.windowNumber(),
            None,
            0,
            clicks,
            1.0,
        )
        .expect("a press");
        // SAFETY: the view's action of a press, of this signature.
        let () = unsafe { msg_send![view, mouseDown: &*event] };
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform
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
            common::wake_after(platform.waker(), 300);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let mtm = MainThreadMarker::new().expect("the main thread");
            let window = NSApplication::sharedApplication(mtm)
                .windows()
                .firstObject()
                .expect("the window");
            let content: Retained<NSView> = window.contentView().expect("the content view");
            let top = content.bounds().size.height - 10.0;
            let mut seen = self.0.borrow_mut();
            seen.action = NSUserDefaults::standardUserDefaults()
                .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
                .map(|action| action.to_string());
            // Past the title, in the bar's left part.
            press(&content, top, 2);
            seen.zoomed = window.isZoomed();
            seen.minimized = window.isMiniaturized();
            platform.quit();
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            if let WindowEvent::MouseButton { .. } = event {
                self.0.borrow_mut().buttons += 1;
            }
        }
    }

    pub fn main() {
        common::deadline("titlebar", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        match seen.action.as_deref() {
            Some("Minimize") => assert!(seen.minimized, "{seen:?}"),
            Some("None") => assert!(!seen.zoomed && !seen.minimized, "{seen:?}"),
            _ => assert!(seen.zoomed, "{seen:?}"),
        }
        assert_eq!(seen.buttons, 0, "{seen:?}");
        println!("titlebar: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

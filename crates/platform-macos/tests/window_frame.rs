//! A window's frame: one that keeps it saves it as it moves, and a window
//! that restores it opens there, of its size; a change of the backing's
//! properties that leaves the scale tells nothing.

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
    use objc2_app_kit::{NSApplication, NSWindow, NSWindowDidChangeBackingPropertiesNotification};
    use objc2_foundation::{NSNotification, NSPoint, NSRect, NSString, NSUserDefaults};

    use crate::common;

    /// The preferences' key of the frame that the windows keep.
    const KEY: &str = "NSWindow Frame Lokyr Window";

    #[derive(Debug, Default)]
    struct Seen {
        /// The frame of the window that kept it, and of the one that
        /// restored it.
        frames: Vec<NSRect>,
        scale_changes: usize,
    }

    struct Probe(Rc<RefCell<Seen>>);

    fn open(platform: &mut dyn Platform, remember: bool, restore: bool) -> WindowId {
        platform
            .create_window(&WindowAttributes {
                title: "A frame".to_owned(),
                size: Size {
                    width: 320.0,
                    height: 200.0,
                },
                remember,
                restore,
            })
            .expect("a window")
    }

    /// The application's windows, from the first opened.
    fn windows() -> Vec<Retained<NSWindow>> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        (0..windows.count())
            .map(|index| windows.objectAtIndex(index))
            .collect()
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            NSUserDefaults::standardUserDefaults().removeObjectForKey(&NSString::from_str(KEY));
            open(platform, true, false);
            let kept = windows().pop().expect("the first window");
            // Moved, it keeps its frame.
            kept.setFrameOrigin(NSPoint::new(123.0, 234.0));
            // The same scale, through the notification of the backing's
            // properties, tells nothing.
            // SAFETY: AppKit's name, and the window that it is of.
            let notification = unsafe {
                NSNotification::notificationWithName_object(
                    NSWindowDidChangeBackingPropertiesNotification,
                    Some(&kept),
                )
            };
            let delegate = kept.delegate().expect("the window's delegate");
            // SAFETY: the delegate's method, which takes a notification.
            let () =
                unsafe { msg_send![&*delegate, windowDidChangeBackingProperties: &*notification] };
            open(platform, false, true);
            let mut seen = self.0.borrow_mut();
            seen.frames = windows().iter().map(|window| window.frame()).collect();
            common::wake_after(platform.waker(), 200);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event == AppEvent::Wake {
                NSUserDefaults::standardUserDefaults().removeObjectForKey(&NSString::from_str(KEY));
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            if let WindowEvent::ScaleChanged(_) = event {
                self.0.borrow_mut().scale_changes += 1;
            }
        }
    }

    pub fn main() {
        common::deadline("window_frame", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let [kept, restored] = seen.frames[..] else {
            panic!("two windows: {seen:?}");
        };
        assert_eq!((kept.origin.x, kept.origin.y), (123.0, 234.0), "{seen:?}");
        assert_eq!(restored, kept, "{seen:?}");
        assert_eq!(seen.scale_changes, 0, "{seen:?}");
        println!("window_frame: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

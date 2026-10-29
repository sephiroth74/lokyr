//! A window's frame: one that keeps it saves it as it moves, and a window
//! that restores it opens there, of its size; a window that the backend
//! places, cascaded, saves nothing, and one opened while a sheet has the
//! focus cascades from the sheet's window; a window keeps its frame from
//! when the app says so; a change of the backing's properties that leaves
//! the scale tells nothing.

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
        /// The frame saved after the first window moved, after a cascaded
        /// window opened, and once the backend's events came.
        saved: Vec<Option<String>>,
        /// The frames of the window with the sheet, and of the one opened
        /// then.
        cascade: Vec<NSRect>,
        /// The frame saved after a window that no longer keeps it moved,
        /// then after one that keeps it now.
        kept_from_now: Vec<Option<String>>,
    }

    struct Probe(Rc<RefCell<Seen>>, Vec<WindowId>);

    /// The frame that the windows keep, as the preferences have it.
    fn saved() -> Option<String> {
        NSUserDefaults::standardUserDefaults()
            .stringForKey(&NSString::from_str(KEY))
            .map(|frame| frame.to_string())
    }

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

    /// The probe's windows, of their title, from the first opened: not the
    /// alert's.
    fn windows() -> Vec<Retained<NSWindow>> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        (0..windows.count())
            .map(|index| windows.objectAtIndex(index))
            .filter(|window| window.title().to_string() == "A frame")
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
            let moved = saved();
            open(platform, false, true);
            let mut seen = self.0.borrow_mut();
            seen.frames = windows().iter().map(|window| window.frame()).collect();
            // A cascaded window that keeps its frame keeps nothing yet.
            let cascaded = open(platform, true, false);
            seen.saved = vec![moved, saved()];
            // A sheet takes the focus from its window.
            platform.show_alert(Some(cascaded), &common::alert());
            self.1.push(cascaded);
            common::wake_after(platform.waker(), 300);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let mut seen = self.0.borrow_mut();
            if self.1.len() == 1 {
                seen.saved.push(saved());
                // A window opened while the sheet has the focus.
                let with_sheet = self.1[0];
                let opened = open(platform, false, false);
                let [.., sheet_s, last] = &windows()[..] else {
                    panic!("the windows");
                };
                seen.cascade = vec![sheet_s.frame(), last.frame()];
                // The window with the sheet keeps its frame no more, and
                // the last one does from now.
                platform.set_window_remember(with_sheet, false);
                platform.set_window_remember(opened, true);
                sheet_s.setFrameOrigin(NSPoint::new(150.0, 160.0));
                seen.kept_from_now.push(saved());
                last.setFrameOrigin(NSPoint::new(170.0, 180.0));
                seen.kept_from_now.push(saved());
                self.1.push(opened);
                common::wake_after(platform.waker(), 200);
            } else {
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
        lokyr_platform_macos::run(Probe(Rc::clone(&seen), Vec::new()))
            .expect("the application runs");
        let seen = seen.borrow();
        let [kept, restored] = seen.frames[..] else {
            panic!("two windows: {seen:?}");
        };
        assert_eq!((kept.origin.x, kept.origin.y), (123.0, 234.0), "{seen:?}");
        assert_eq!(restored, kept, "{seen:?}");
        assert_eq!(seen.scale_changes, 0, "{seen:?}");
        // The cascaded window kept nothing, nor did its events after.
        let moved = seen.saved[0].clone();
        assert!(moved.is_some(), "{seen:?}");
        assert_eq!(
            seen.saved,
            [moved.clone(), moved.clone(), moved.clone()],
            "{seen:?}"
        );
        // Down and to the right of the sheet's window, not in the centre.
        let [with_sheet, opened] = seen.cascade[..] else {
            panic!("two frames: {seen:?}");
        };
        let top = |frame: NSRect| frame.origin.y + frame.size.height;
        let right = opened.origin.x - with_sheet.origin.x;
        assert!(right > 0.0 && right < 60.0, "{seen:?}");
        assert!(top(opened) < top(with_sheet), "{seen:?}");
        // The window that no longer keeps it kept nothing; the other did.
        assert_eq!(seen.kept_from_now[0], moved, "{seen:?}");
        let kept = seen.kept_from_now[1].clone().unwrap_or_default();
        assert!(kept.starts_with("170 180 "), "{seen:?}");
        println!("window_frame: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

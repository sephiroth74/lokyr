//! The titlebar, which the content runs under: a double click on it, past
//! its title, does what the system's settings say, as the window's frame
//! does over the title, and a single click tells the content nothing. A
//! titlebar of tabs is taller, of no title, its buttons' room told; a press
//! over its parts that are the content's comes to the content, as the
//! pointer's moves over it do; the title's titlebar again is as before.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, ContentRect, Handler, Platform, Point, Size, Titlebar, WindowAttributes,
        WindowEvent, WindowId, WindowSize,
    };
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::{
        NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindowTitleVisibility,
    };
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
        /// The buttons that the content learned of, and the moves.
        buttons: usize,
        moves: usize,
        /// The sizes that came, of the title's titlebar, of the tabs', and
        /// of the title's again.
        sizes: Vec<WindowSize>,
        /// Whether the title showed with the tabs' titlebar.
        title_shown: bool,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        step: usize,
    }

    /// Sends the content view `view` a press of the left button of `clicks`
    /// at `x` points from its left and `y` from its bottom.
    fn press(view: &NSView, (x, y): (f64, f64), clicks: isize) {
        send(view, NSEventType::LeftMouseDown, (x, y), clicks);
    }

    /// Sends the content view `view` an event of the mouse of `kind` at `x`
    /// points from its left and `y` from its bottom.
    fn send(view: &NSView, kind: NSEventType, (x, y): (f64, f64), clicks: isize) {
        let window = view.window().expect("the view's window");
        let location = view.convertPoint_toView(NSPoint::new(x, y), None);
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            location,
            NSEventModifierFlags::empty(),
            0.0,
            window.windowNumber(),
            None,
            0,
            clicks,
            1.0,
        )
        .expect("an event");
        if kind == NSEventType::MouseMoved {
            // SAFETY: the view's action of a move, of this signature.
            let () = unsafe { msg_send![view, mouseMoved: &*event] };
        } else {
            // SAFETY: the view's action of a press, of this signature.
            let () = unsafe { msg_send![view, mouseDown: &*event] };
        }
    }

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
                    restore_size: false,
                })
                .expect("the window opens");
            self.window = Some(window);
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
            let id = self.window.expect("the probe's window");
            let mut seen = self.seen.borrow_mut();
            self.step += 1;
            match self.step {
                1 => {
                    seen.action = NSUserDefaults::standardUserDefaults()
                        .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
                        .map(|action| action.to_string());
                    // Past the title, in the bar's left part.
                    press(&content, (120.0, top), 2);
                    seen.zoomed = window.isZoomed();
                    seen.minimized = window.isMiniaturized();
                    seen.sizes.clear();
                    platform.set_titlebar(id, Titlebar::Tabs);
                    common::wake_after(platform.waker(), 300);
                }
                2 => {
                    seen.title_shown = window.titleVisibility() == NSWindowTitleVisibility::Visible;
                    // A tab's part of the bar, 200 to 300 points across.
                    platform.set_titlebar_areas(
                        id,
                        &[ContentRect {
                            origin: Point { x: 200.0, y: 0.0 },
                            size: Size {
                                width: 100.0,
                                height: 20.0,
                            },
                        }],
                    );
                    press(&content, (250.0, top), 1);
                    platform.track_mouse_moves(id, true);
                    send(&content, NSEventType::MouseMoved, (120.0, top), 0);
                    platform.set_titlebar(id, Titlebar::Titled);
                    common::wake_after(platform.waker(), 300);
                }
                _ => platform.quit(),
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            let mut seen = self.seen.borrow_mut();
            match event {
                WindowEvent::MouseButton { .. } => seen.buttons += 1,
                WindowEvent::MouseMoved { .. } => seen.moves += 1,
                WindowEvent::Resized(size) => seen.sizes.push(size),
                _ => {}
            }
        }
    }

    pub fn main() {
        common::deadline("titlebar", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            step: 0,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        match seen.action.as_deref() {
            Some("Minimize") => assert!(seen.minimized, "{seen:?}"),
            Some("None") => assert!(!seen.zoomed && !seen.minimized, "{seen:?}"),
            _ => assert!(seen.zoomed, "{seen:?}"),
        }
        // The press over the tab's part of the bar alone, and the move.
        assert_eq!((seen.buttons, seen.moves), (1, 1), "{seen:?}");
        assert!(!seen.title_shown, "{seen:?}");
        let tabs = seen.sizes.first().expect("the tabs' titlebar's size");
        let titled = seen.sizes.last().expect("the title's titlebar's size");
        // A compact toolbar's titlebar, taller than the title's alone, its
        // buttons past a margin.
        assert!(tabs.bar > titled.bar && tabs.bar >= 36.0, "{seen:?}");
        assert!((50.0..=140.0).contains(&tabs.buttons), "{seen:?}");
        assert!(titled.bar >= 24.0 && titled.buttons > 0.0, "{seen:?}");
        println!("titlebar: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

//! The titlebar, which the content runs under: a double click on it, past
//! its title, does what the system's settings say, as the window's frame
//! does over the title, and a single click tells the content nothing. A
//! titlebar of tabs is taller, of no title, its buttons' room told; a press
//! over its parts that are the content's comes to the content, as the
//! pointer's moves over it do, and the first click there of a window that
//! is not key too; the title's titlebar again is as before. The parts of the
//! content where the moves come are tracking areas at their places from
//! the view's top, as its height changes too, and the pointer's leaving
//! them comes. The view lets AppKit drag the window from no press over
//! it, translucent too, as it drags the window itself.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Backdrop, ContentRect, Handler, Platform, Point, Size, Titlebar,
        WindowAttributes, WindowEvent, WindowId, WindowSize,
    };
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::{
        NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWindowTitleVisibility,
    };
    use objc2_foundation::{NSPoint, NSSize, NSString, NSUserDefaults};

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
        /// The sizes that came, of the zoom's frames, of the tabs'
        /// titlebar, and of the title's again.
        sizes: Vec<WindowSize>,
        /// The last size before the second step, the tabs' titlebar's, as
        /// the zoom's frames came before it.
        tabs: Option<WindowSize>,
        /// Whether the title showed with the tabs' titlebar.
        title_shown: bool,
        /// Whether the content takes a first click over a tab's part of the
        /// bar, over the rest of it, and below it.
        first_mouse: Vec<bool>,
        /// The view's tracking areas, of their rectangles in its own
        /// coordinates, and its height, before and after a resize; and the
        /// pointer's leavings.
        tracked: Vec<(f64, Vec<[f64; 4]>)>,
        exits: usize,
        /// Whether the view is opaque, and lets AppKit drag the window from
        /// a press over it, of an opaque window and of a translucent one.
        moves_window: Vec<(bool, bool)>,
        /// Of the titlebar of no title, whether the title shows, whether a
        /// toolbar is there, and the window's title; and how many sizes had
        /// come as it came, and once it was there.
        untitled: Option<(bool, bool, String)>,
        sizes_untitled: (usize, usize),
    }

    /// The rectangles of `view`'s tracking areas, and its height.
    fn tracked(view: &NSView) -> (f64, Vec<[f64; 4]>) {
        let areas = view.trackingAreas();
        let rects = (0..areas.count())
            .map(|index| areas.objectAtIndex(index))
            .map(|area| {
                let rect = area.rect();
                [
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                ]
            })
            .collect();
        (view.bounds().size.height, rects)
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

    /// An event of the mouse of `kind` at `x` points from the content view
    /// `view`'s left and `y` from its bottom.
    fn mouse_event(
        view: &NSView,
        kind: NSEventType,
        (x, y): (f64, f64),
        clicks: isize,
    ) -> Retained<NSEvent> {
        let window = view.window().expect("the view's window");
        let location = view.convertPoint_toView(NSPoint::new(x, y), None);
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
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
        .expect("an event")
    }

    /// Sends the content view `view` an event of the mouse of `kind` at `x`
    /// points from its left and `y` from its bottom.
    fn send(view: &NSView, kind: NSEventType, (x, y): (f64, f64), clicks: isize) {
        let event = mouse_event(view, kind, (x, y), clicks);
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
                    platform.set_titlebar(id, Titlebar::Tabs);
                    common::wake_after(platform.waker(), 300);
                }
                2 => {
                    seen.tabs = seen.sizes.last().copied();
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
                    for at in [(250.0, top), (120.0, top), (120.0, 50.0)] {
                        let down = mouse_event(&content, NSEventType::LeftMouseDown, at, 1);
                        // SAFETY: the view's answer, of this signature.
                        let takes: bool =
                            unsafe { msg_send![&*content, acceptsFirstMouse: &*down] };
                        seen.first_mouse.push(takes);
                    }
                    press(&content, (250.0, top), 1);
                    // The view drags the window itself, from the bar's parts
                    // that are not the content's, translucent too, as a
                    // tab's drag would move it otherwise.
                    for backdrop in [
                        None,
                        Some(Backdrop {
                            color: [0.2, 0.4, 0.6, 0.8],
                            blur: 20.0,
                        }),
                    ] {
                        platform.set_translucent(id, backdrop);
                        // SAFETY: the view's answer, of this signature.
                        let opaque: bool = unsafe { msg_send![&*content, isOpaque] };
                        // SAFETY: the view's answer, of this signature.
                        let moves: bool = unsafe { msg_send![&*content, mouseDownCanMoveWindow] };
                        seen.moves_window.push((opaque, moves));
                    }
                    platform.set_translucent(id, None);
                    platform.track_mouse_moves(id, true);
                    send(&content, NSEventType::MouseMoved, (120.0, top), 0);
                    platform.set_titlebar(id, Titlebar::Titled);
                    // The moves over a part of the content, 200 to 300 points
                    // across at its top, and the pointer's leaving it.
                    platform.set_move_areas(
                        id,
                        &[ContentRect {
                            origin: Point { x: 200.0, y: 0.0 },
                            size: Size {
                                width: 100.0,
                                height: 20.0,
                            },
                        }],
                    );
                    seen.tracked.push(tracked(&content));
                    let event = mouse_event(&content, NSEventType::MouseMoved, (250.0, top), 0);
                    // SAFETY: the view's action of the pointer's leaving, of
                    // this signature, which reads nothing of its event.
                    let () = unsafe { msg_send![&*content, mouseExited: &*event] };
                    window.setContentSize(NSSize::new(400.0, 360.0));
                    common::wake_after(platform.waker(), 300);
                }
                3 => {
                    seen.tracked.push(tracked(&content));
                    // The system's titlebar of no title, which the content
                    // draws, the window keeping its title.
                    seen.sizes_untitled.0 = seen.sizes.len();
                    platform.set_titlebar(id, Titlebar::Untitled);
                    seen.untitled = Some((
                        window.titleVisibility() == NSWindowTitleVisibility::Visible,
                        window.toolbar().is_some(),
                        window.title().to_string(),
                    ));
                    common::wake_after(platform.waker(), 300);
                }
                4 => {
                    seen.sizes_untitled.1 = seen.sizes.len();
                    platform.quit();
                }
                _ => platform.quit(),
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            let mut seen = self.seen.borrow_mut();
            match event {
                WindowEvent::MouseButton { .. } => seen.buttons += 1,
                WindowEvent::MouseMoved { .. } => seen.moves += 1,
                WindowEvent::MouseExited => seen.exits += 1,
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
        assert_eq!(seen.first_mouse, [true, false, false], "{seen:?}");
        let tabs = seen.tabs.as_ref().expect("the tabs' titlebar's size");
        let titled = seen.sizes.last().expect("the title's titlebar's size");
        // A compact toolbar's titlebar, taller than the title's alone, its
        // buttons past a margin.
        assert!(tabs.bar > titled.bar && tabs.bar >= 36.0, "{seen:?}");
        assert!((50.0..=140.0).contains(&tabs.buttons), "{seen:?}");
        assert!(titled.bar >= 24.0 && titled.buttons > 0.0, "{seen:?}");
        // Of no title, of the title's titlebar's size, which comes not anew.
        assert_eq!(
            seen.untitled,
            Some((false, false, "The titlebar".to_owned())),
            "{seen:?}"
        );
        let (before, after) = seen.sizes_untitled;
        assert_eq!(before, after, "no new size: {seen:?}");
        // The tracking area 20 points below the view's top, before and after
        // the resize, and the pointer's leaving it.
        assert_eq!(seen.tracked.len(), 2, "{seen:?}");
        for (height, rects) in &seen.tracked {
            assert_eq!(rects, &[[200.0, height - 20.0, 100.0, 20.0]], "{seen:?}");
        }
        assert_ne!(seen.tracked[1].0, seen.tracked[0].0, "{seen:?}");
        assert_eq!(seen.exits, 1, "{seen:?}");
        assert_eq!(
            seen.moves_window,
            [(true, false), (false, false)],
            "{seen:?}"
        );
        println!("titlebar: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

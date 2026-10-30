//! The mouse in a window's view: a button's press and release, with the
//! clicks that it makes, a drag, and the wheel's steps and a trackpad's
//! points, where the pointer is from the content's top left corner; and the
//! moves without a button, which the window takes only while the app asks
//! for them. The events go to the view's methods, as the window hands them
//! on, so that the test needs no focus.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Modifiers, MouseButton, Platform, Point, ScrollDelta, ScrollPhase, Size,
        WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::encode::{Encoding, RefEncode};
    use objc2::rc::Retained;
    use objc2::{ClassType, MainThreadMarker, msg_send};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView};
    use objc2_foundation::NSPoint;

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        events: Vec<WindowEvent>,
        accepts_moves: Vec<bool>,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        sent: bool,
    }

    /// Core Graphics' event, which AppKit's wraps.
    #[repr(C)]
    struct CGEvent {
        _opaque: [u8; 0],
    }

    // SAFETY: the encoding of `CGEventRef`, a pointer to the struct.
    unsafe impl RefEncode for CGEvent {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CGEvent", &[]));
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        /// A scroll of the wheel, by lines or by pixels, of one axis.
        fn CGEventCreateScrollWheelEvent2(
            source: *const c_void,
            units: u32,
            wheels: u32,
            wheel1: i32,
            wheel2: i32,
            wheel3: i32,
        ) -> *mut CGEvent;
        fn CFRelease(object: *const c_void);
    }

    /// The window's view.
    fn view() -> Retained<NSView> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window")
            .contentView()
            .expect("the window's view")
    }

    /// Hands the view a mouse event of `kind` through its method `method`,
    /// `x` points from the content's left edge and `y` from its top one,
    /// which AppKit counts from the window's bottom: the content, which
    /// runs under the titlebar, is the whole window.
    fn send(
        method: &str,
        kind: NSEventType,
        (x, y): (f64, f64),
        clicks: isize,
        flags: NSEventModifierFlags,
    ) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window");
        let height = view().bounds().size.height;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            NSPoint::new(x, height - y),
            flags,
            0.0,
            window.windowNumber(),
            None,
            0,
            clicks,
            1.0,
        )
        .expect("a mouse event");
        let view = view();
        let view = &*view;
        let event = &*event;
        // SAFETY: the view's methods, each of which takes an event.
        unsafe {
            let () = match method {
                "mouseDown:" => msg_send![view, mouseDown: event],
                "mouseDragged:" => msg_send![view, mouseDragged: event],
                "mouseUp:" => msg_send![view, mouseUp: event],
                "rightMouseDown:" => msg_send![view, rightMouseDown: event],
                "rightMouseUp:" => msg_send![view, rightMouseUp: event],
                "mouseMoved:" => msg_send![view, mouseMoved: event],
                _ => unreachable!("{method}"),
            };
        }
    }

    /// Hands the view a scroll of `lines` lines, or of pixels when not
    /// `lines`, of `amount` up.
    fn scroll(lines: bool, amount: i32) {
        // SAFETY: Core Graphics makes an event, which AppKit's takes in,
        // and which is released once it has.
        let event: Retained<NSEvent> = unsafe {
            let cg =
                CGEventCreateScrollWheelEvent2(std::ptr::null(), u32::from(lines), 1, amount, 0, 0);
            assert!(!cg.is_null(), "a scroll event");
            let event: Option<Retained<NSEvent>> =
                msg_send![NSEvent::class(), eventWithCGEvent: cg];
            CFRelease(cg.cast());
            event.expect("an AppKit event")
        };
        let view = view();
        // SAFETY: the view's method, which takes an event.
        let _: () = unsafe { msg_send![&*view, scrollWheel: &*event] };
    }

    fn accepts_moves() -> bool {
        let mtm = MainThreadMarker::new().expect("the main thread");
        NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window")
            .acceptsMouseMovedEvents()
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "mouse".to_owned(),
                    size: Size {
                        width: 300.0,
                        height: 200.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                })
                .expect("the window opens");
            let mtm = MainThreadMarker::new().expect("the main thread");
            NSApplication::sharedApplication(mtm).hide(None);
            self.window = Some(window);
            common::wake_after(platform.waker(), 200);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            if self.sent {
                platform.quit();
                return;
            }
            self.sent = true;
            let window = self.window.expect("the window");
            {
                let mut seen = self.seen.borrow_mut();
                seen.accepts_moves.push(accepts_moves());
                platform.track_mouse_moves(window, true);
                seen.accepts_moves.push(accepts_moves());
                platform.track_mouse_moves(window, false);
                seen.accepts_moves.push(accepts_moves());
            }
            let (shift, none) = (NSEventModifierFlags::Shift, NSEventModifierFlags(0));
            send(
                "mouseDown:",
                NSEventType::LeftMouseDown,
                (10.0, 50.0),
                2,
                shift,
            );
            send(
                "mouseDragged:",
                NSEventType::LeftMouseDragged,
                (30.0, 90.0),
                1,
                none,
            );
            send("mouseUp:", NSEventType::LeftMouseUp, (30.0, 90.0), 1, none);
            send(
                "rightMouseDown:",
                NSEventType::RightMouseDown,
                (100.0, 140.0),
                1,
                none,
            );
            send(
                "rightMouseUp:",
                NSEventType::RightMouseUp,
                (100.0, 140.0),
                1,
                none,
            );
            send(
                "mouseMoved:",
                NSEventType::MouseMoved,
                (5.0, 195.0),
                0,
                none,
            );
            // Over the titlebar, 10 points from the top, the window's: a
            // click, a drag that goes below it, the right button and a move.
            send(
                "mouseDown:",
                NSEventType::LeftMouseDown,
                (10.0, 10.0),
                1,
                none,
            );
            send(
                "mouseDragged:",
                NSEventType::LeftMouseDragged,
                (10.0, 100.0),
                1,
                none,
            );
            send("mouseUp:", NSEventType::LeftMouseUp, (10.0, 100.0), 1, none);
            send(
                "rightMouseDown:",
                NSEventType::RightMouseDown,
                (10.0, 10.0),
                1,
                none,
            );
            send(
                "rightMouseUp:",
                NSEventType::RightMouseUp,
                (10.0, 10.0),
                1,
                none,
            );
            send("mouseMoved:", NSEventType::MouseMoved, (5.0, 5.0), 0, none);
            scroll(true, 3);
            scroll(false, -20);
            common::wake_after(platform.waker(), 200);
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            if matches!(
                event,
                WindowEvent::MouseButton { .. }
                    | WindowEvent::MouseMoved { .. }
                    | WindowEvent::Scroll { .. }
            ) {
                self.seen.borrow_mut().events.push(event);
            }
        }
    }

    pub fn main() {
        common::deadline("mouse", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            sent: false,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.accepts_moves, [false, true, false]);
        let none = Modifiers::NONE;
        let shift = Modifiers {
            shift: true,
            ..none
        };
        let button = |button, pressed, (x, y), mods, clicks| WindowEvent::MouseButton {
            button,
            pressed,
            position: Point { x, y },
            mods,
            clicks,
        };
        let moved = |(x, y)| WindowEvent::MouseMoved {
            position: Point { x, y },
            mods: none,
        };
        // From the top left corner of the content, which runs under the
        // titlebar, below it.
        assert_eq!(
            seen.events[..6],
            [
                button(MouseButton::Left, true, (10.0, 50.0), shift, 2),
                moved((30.0, 90.0)),
                button(MouseButton::Left, false, (30.0, 90.0), none, 1),
                button(MouseButton::Right, true, (100.0, 140.0), none, 1),
                button(MouseButton::Right, false, (100.0, 140.0), none, 1),
                moved((5.0, 195.0)),
            ],
            "{seen:#?}"
        );
        // Nothing of the titlebar's: the scrolls come next.
        assert!(
            seen.events[6..]
                .iter()
                .all(|event| matches!(event, WindowEvent::Scroll { .. })),
            "{seen:#?}"
        );
        // A wheel's steps, and a smooth scroll's points, without a phase.
        let scrolls: Vec<(ScrollDelta, Option<ScrollPhase>, bool)> = seen.events[6..]
            .iter()
            .filter_map(|event| match event {
                WindowEvent::Scroll {
                    delta,
                    phase,
                    momentum,
                    ..
                } => Some((*delta, *phase, *momentum)),
                _ => None,
            })
            .collect();
        assert_eq!(
            scrolls,
            [
                (ScrollDelta::Lines { x: 0.0, y: 3.0 }, None, false),
                (ScrollDelta::Points { x: 0.0, y: -20.0 }, None, false),
            ]
        );
        println!("mouse: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

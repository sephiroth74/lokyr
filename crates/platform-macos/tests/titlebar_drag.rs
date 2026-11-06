//! The titlebar's drags and double clicks, of the real pointer, which the
//! probe moves through the system: so it runs alone where
//! `LOKYR_REAL_POINTER=1` asks for it, and the process may post events, as
//! it takes the user's pointer for some seconds. Over the titlebar of the
//! window's title, on the title and past it, and over a titlebar of tabs,
//! by the window's buttons and past them, a drag moves the window and a
//! double click zooms it, opaque or translucent; over the parts of either
//! that are the content's, and below the titlebar, the content takes the
//! press, and the window stays.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::rc::Rc;
    use std::time::Duration;

    use lokyr_platform_core::{
        AppEvent, Backdrop, ContentRect, Handler, Platform, Point, Size, Titlebar,
        WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSScreen, NSWindow};
    use objc2_foundation::NSPoint;

    use crate::common;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    /// Core Graphics' types of mouse events, and its field of a click's
    /// count.
    const MOVED: u32 = 5;
    const DOWN: u32 = 1;
    const UP: u32 = 2;
    const DRAGGED: u32 = 6;
    const CLICK_STATE: u32 = 1;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightPostEventAccess() -> bool;
        fn CGEventCreateMouseEvent(
            source: *const c_void,
            kind: u32,
            at: CGPoint,
            button: u32,
        ) -> *mut c_void;
        fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
        fn CGEventPost(tap: u32, event: *mut c_void);
        fn CFRelease(object: *const c_void);
    }

    /// Posts a mouse event of the left button, of `kind`, at `(x, y)` of
    /// the screens' coordinates from the main one's top, of its `clicks`.
    fn post(kind: u32, (x, y): (f64, f64), clicks: i64) {
        // SAFETY: Core Graphics makes the event, which is posted, then
        // released.
        unsafe {
            let event = CGEventCreateMouseEvent(std::ptr::null(), kind, CGPoint { x, y }, 0);
            assert!(!event.is_null(), "a mouse event");
            CGEventSetIntegerValueField(event, CLICK_STATE, clicks);
            CGEventPost(0, event);
            CFRelease(event);
        }
    }

    /// Drags 90 points right from `at`, or clicks twice there, on a thread
    /// of its own, as the run loop takes the events.
    fn gesture(at: (f64, f64), double: bool) {
        std::thread::spawn(move || {
            let pause = |ms| std::thread::sleep(Duration::from_millis(ms));
            post(MOVED, at, 0);
            pause(80);
            if double {
                for clicks in [1, 2] {
                    post(DOWN, at, clicks);
                    pause(40);
                    post(UP, at, clicks);
                    pause(60);
                }
                return;
            }
            post(DOWN, at, 1);
            pause(120);
            for step in 1..=6 {
                post(DRAGGED, (at.0 + 15.0 * f64::from(step), at.1), 1);
                pause(40);
            }
            post(UP, (at.0 + 90.0, at.1), 1);
        });
    }

    fn window() -> Retained<NSWindow> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window")
    }

    /// The window's top left corner, of the screens' coordinates from the
    /// main one's top.
    fn top_left() -> (f64, f64) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let screen = NSScreen::mainScreen(mtm).expect("a screen").frame();
        let frame = window().frame();
        (
            frame.origin.x,
            screen.size.height - (frame.origin.y + frame.size.height),
        )
    }

    /// A case: its titlebar, its translucency, its part of the titlebar
    /// that is the content's, from 200 to 400 points across, where it has
    /// one, the gesture's place from the content's top left corner, and
    /// whether it is a double click; and whether the window moves, or
    /// zooms.
    struct Case {
        name: &'static str,
        titlebar: Titlebar,
        translucent: bool,
        content_part: bool,
        at: (f64, f64),
        double: bool,
        moves: bool,
    }

    /// The window's title, which macOS draws past its buttons, at its left.
    const TITLE: &str = "drags";

    const CASES: [Case; 12] = [
        case(
            "the title's titlebar, on the title",
            Titlebar::Titled,
            true,
            false,
            (90.0, 12.0),
            false,
            true,
        ),
        case(
            "an opaque one, on the title",
            Titlebar::Titled,
            false,
            false,
            (90.0, 12.0),
            false,
            true,
        ),
        case(
            "the title's titlebar, past the title",
            Titlebar::Titled,
            true,
            false,
            (180.0, 12.0),
            false,
            true,
        ),
        case(
            "the title's titlebar, its content's part",
            Titlebar::Titled,
            true,
            true,
            (300.0, 12.0),
            false,
            false,
        ),
        case(
            "a double click on the title",
            Titlebar::Titled,
            true,
            false,
            (90.0, 12.0),
            true,
            true,
        ),
        case(
            "a double click past the title",
            Titlebar::Titled,
            true,
            false,
            (180.0, 12.0),
            true,
            true,
        ),
        case(
            "below the titlebar",
            Titlebar::Titled,
            true,
            false,
            (180.0, 150.0),
            false,
            false,
        ),
        case(
            "the tabs' titlebar, by the buttons",
            Titlebar::Tabs,
            true,
            true,
            (95.0, 12.0),
            false,
            true,
        ),
        case(
            "the tabs' titlebar, past the buttons",
            Titlebar::Tabs,
            true,
            true,
            (150.0, 12.0),
            false,
            true,
        ),
        case(
            "the tabs' titlebar, its content's part",
            Titlebar::Tabs,
            true,
            true,
            (300.0, 12.0),
            false,
            false,
        ),
        case(
            "an opaque one, its content's part",
            Titlebar::Tabs,
            false,
            true,
            (300.0, 12.0),
            false,
            false,
        ),
        case(
            "a double click on the tabs' titlebar",
            Titlebar::Tabs,
            true,
            true,
            (150.0, 12.0),
            true,
            true,
        ),
    ];

    #[allow(clippy::too_many_arguments)]
    const fn case(
        name: &'static str,
        titlebar: Titlebar,
        translucent: bool,
        content_part: bool,
        at: (f64, f64),
        double: bool,
        moves: bool,
    ) -> Case {
        Case {
            name,
            titlebar,
            translucent,
            content_part,
            at,
            double,
            moves,
        }
    }

    /// What a case did: whether the window moved, or zoomed, and the
    /// presses that the content took.
    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    struct Done {
        moved: bool,
        zoomed: bool,
        presses: usize,
    }

    struct Probe {
        window: Option<WindowId>,
        step: usize,
        before: (f64, f64),
        presses: usize,
        done: Rc<RefCell<Vec<Done>>>,
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: TITLE.to_owned(),
                    size: Size {
                        width: 500.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                })
                .expect("the window opens");
            self.window = Some(window);
            common::wake_after(platform.waker(), 500);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let id = self.window.expect("the probe's window");
            let Some(case) = CASES.get(self.step / 2) else {
                platform.quit();
                return;
            };
            let window = window();
            if self.step.is_multiple_of(2) {
                platform.set_titlebar(id, case.titlebar);
                platform.set_translucent(
                    id,
                    case.translucent.then_some(Backdrop {
                        color: [0.1, 0.1, 0.1, 0.9],
                        blur: 0.0,
                    }),
                );
                let part = ContentRect {
                    origin: Point { x: 200.0, y: 0.0 },
                    size: Size {
                        width: 200.0,
                        height: 40.0,
                    },
                };
                let parts: &[ContentRect] = if case.content_part { &[part] } else { &[] };
                platform.set_titlebar_areas(id, parts);
                // At the same place for each case, in front.
                let mtm = MainThreadMarker::new().expect("the main thread");
                let screen = NSScreen::mainScreen(mtm).expect("a screen").frame();
                window.setFrameTopLeftPoint(NSPoint::new(300.0, screen.size.height - 200.0));
                window.makeKeyAndOrderFront(None);
                self.presses = 0;
                self.before = top_left();
                let at = (self.before.0 + case.at.0, self.before.1 + case.at.1);
                gesture(at, case.double);
                common::wake_after(platform.waker(), 1200);
            } else {
                let after = top_left();
                self.done.borrow_mut().push(Done {
                    moved: after != self.before,
                    zoomed: window.isZoomed(),
                    presses: self.presses,
                });
                if window.isZoomed() {
                    window.zoom(None);
                }
                common::wake_after(platform.waker(), 300);
            }
            self.step += 1;
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            if let WindowEvent::MouseButton { pressed: true, .. } = event {
                self.presses += 1;
            }
        }
    }

    pub fn main() {
        if std::env::var_os("LOKYR_REAL_POINTER").is_none_or(|value| value != "1") {
            println!(
                "titlebar_drag: skipped, as it moves the pointer: LOKYR_REAL_POINTER=1 runs it"
            );
            return;
        }
        // SAFETY: Core Graphics' question, of no argument.
        if !unsafe { CGPreflightPostEventAccess() } {
            println!("titlebar_drag: skipped, as the process may not post events");
            return;
        }
        common::deadline("titlebar_drag", 60);
        let done = Rc::new(RefCell::new(Vec::new()));
        let probe = Probe {
            window: None,
            step: 0,
            before: (0.0, 0.0),
            presses: 0,
            done: Rc::clone(&done),
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let done = done.borrow();
        assert_eq!(done.len(), CASES.len(), "{done:?}");
        for (case, done) in CASES.iter().zip(done.iter()) {
            let expected = Done {
                // A zoom moves the window too.
                moved: case.moves,
                zoomed: case.double,
                presses: if case.moves { 0 } else { 1 },
            };
            assert_eq!(*done, expected, "{}", case.name);
        }
        println!("titlebar_drag: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

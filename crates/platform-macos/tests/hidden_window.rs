//! A window hidden, as the windows of a session that is not active hide,
//! leaves the screen and the Window menu and draws nothing, the frame that
//! it asks for meanwhile held; shown again, it is where it was, in the
//! Window menu, and the held frame comes.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, ScreenRect, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    use crate::common;

    /// What the window was, at a step: whether it showed, whether the
    /// Window menu left it out, and its frame.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Step {
        visible: bool,
        excluded: bool,
        frame: Option<ScreenRect>,
    }

    #[derive(Debug, Default)]
    struct Seen {
        steps: Vec<Step>,
        /// The occlusions told, in their order.
        occluded: Vec<bool>,
        /// The frames that came while it was hidden, and after it showed.
        frames_hidden: usize,
        frames_shown: usize,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        wakes: usize,
        hidden: bool,
    }

    /// The application's one window, as AppKit has it.
    fn step(platform: &dyn Platform, window: WindowId) -> Step {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        let shown = windows.firstObject().expect("a window");
        Step {
            visible: shown.isVisible(),
            excluded: shown.isExcludedFromWindowsMenu(),
            frame: platform.window_frame(window),
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "hidden".to_owned(),
                    size: Size {
                        width: 400.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("the window opens");
            self.window = Some(window);
            platform.focus_window(window);
            common::wake_after(platform.waker(), 400);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let Some(window) = self.window else {
                return;
            };
            self.wakes += 1;
            let mut seen = self.seen.borrow_mut();
            match self.wakes {
                // Shown, then hidden, a frame asked for at once.
                1 => {
                    seen.steps.push(step(platform, window));
                    platform.set_window_hidden(window, true);
                    self.hidden = true;
                    seen.steps.push(step(platform, window));
                    platform.request_frame(window);
                    common::wake_after(platform.waker(), 400);
                }
                // Shown again: the frame held comes.
                2 => {
                    self.hidden = false;
                    platform.set_window_hidden(window, false);
                    seen.steps.push(step(platform, window));
                    common::wake_after(platform.waker(), 400);
                }
                _ => platform.quit(),
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            let mut seen = self.seen.borrow_mut();
            match event {
                WindowEvent::Frame if self.hidden => seen.frames_hidden += 1,
                WindowEvent::Frame if self.wakes >= 2 => seen.frames_shown += 1,
                WindowEvent::Occluded(occluded) if self.wakes >= 1 => {
                    seen.occluded.push(occluded);
                }
                _ => {}
            }
        }
    }

    pub fn main() {
        common::deadline("hidden_window", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            wakes: 0,
            hidden: false,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        let [shown, hidden, again] = seen.steps[..] else {
            panic!("three steps: {seen:?}");
        };
        assert!(shown.visible && !shown.excluded, "{seen:?}");
        assert!(!hidden.visible && hidden.excluded, "{seen:?}");
        assert!(again.visible && !again.excluded, "{seen:?}");
        // Where it was.
        assert_eq!(again.frame, shown.frame, "{seen:?}");
        assert_eq!(seen.frames_hidden, 0, "{seen:?}");
        assert!(seen.frames_shown >= 1, "{seen:?}");
        assert_eq!(seen.occluded.first(), Some(&true), "{seen:?}");
        assert_eq!(seen.occluded.last(), Some(&false), "{seen:?}");
        println!("hidden_window: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

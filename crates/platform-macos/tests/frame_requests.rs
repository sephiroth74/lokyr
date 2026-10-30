//! The requests for a frame that a stopped display link takes before its
//! first frame come as that one frame.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        wakes: usize,
        /// The frames when the second wake came, a while after the requests.
        frames: Option<usize>,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        frames: usize,
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "frames".to_owned(),
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
            // A while after the window opened, when its link has stopped.
            common::wake_after(platform.waker(), 300);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let Some(window) = self.window else {
                return;
            };
            let mut seen = self.seen.borrow_mut();
            seen.wakes += 1;
            if seen.wakes == 1 {
                platform.request_frame(window);
                platform.request_frame(window);
                // Long enough for a refresh or two of any display.
                common::wake_after(platform.waker(), 200);
            } else {
                seen.frames = Some(self.frames);
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            if event == WindowEvent::Frame {
                self.frames += 1;
            }
        }
    }

    pub fn main() {
        common::deadline("frame_requests", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            frames: 0,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.frames, Some(1), "{seen:?}");
        println!("frame_requests: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

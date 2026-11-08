//! The system's request to quit, as AppKit's `terminate:` makes it when the
//! user logs out or chooses Quit in the Dock: AppKit waits for the app's
//! answer, as a logout does; the app refuses the first, and the application
//! goes on; it accepts the second, and AppKit ends the process, with a
//! status of 0, rather than `run` returning.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, msg_send, sel};
    use objc2_app_kit::{NSApplication, NSModalPanelRunLoopMode};
    use objc2_foundation::NSRunLoop;

    use crate::common;

    /// Asks the application to quit, as the system does, from the run
    /// loop, once `delay` seconds have passed.
    fn terminate_later(delay: f64) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let app = NSApplication::sharedApplication(mtm);
        let nothing: Option<&AnyObject> = None;
        // SAFETY: `performSelector:withObject:afterDelay:` takes a selector,
        // an object or nil, and a delay, and returns nothing; `terminate:`
        // takes its sender, which may be nil.
        unsafe {
            let () = msg_send![&*app, performSelector: sel!(terminate:), withObject: nothing, afterDelay: delay];
        }
    }

    struct Probe {
        requests: usize,
    }

    /// Whether AppKit waits for the answer to a request to quit, which it
    /// does in the loop of a modal panel.
    fn waits() -> bool {
        // SAFETY: the constant is AppKit's, which the application loaded.
        let modal = unsafe { NSModalPanelRunLoopMode };
        NSRunLoop::currentRunLoop()
            .currentMode()
            .is_some_and(|mode| &*mode == modal)
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform
                .create_window(&WindowAttributes {
                    title: "terminate".to_owned(),
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
            terminate_later(0.2);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            match event {
                // The answer comes later, as after a question to the user.
                AppEvent::QuitRequested if self.requests == 0 => {
                    self.requests += 1;
                    common::wake_after(platform.waker(), 200);
                }
                AppEvent::Wake if self.requests == 1 => {
                    assert!(waits(), "AppKit waits for the answer");
                    // Refused: the application goes on, and the system can
                    // ask again.
                    platform.refuse_quit();
                    self.requests += 1;
                    terminate_later(0.2);
                }
                AppEvent::QuitRequested => {
                    println!("terminate_later: ok");
                    platform.quit();
                }
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("terminate_later", 20);
        lokyr_platform_macos::run(Probe { requests: 0 }).expect("the application runs");
        eprintln!("terminate_later: `run` returned, where AppKit ends the process");
        std::process::exit(1);
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

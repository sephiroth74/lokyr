//! The application on the main thread's run loop: it starts, opens a window
//! of the size asked, takes a wake from another thread, and returns from
//! `run` once its handler quits. AppKit needs the process's main thread,
//! which this test has to itself.

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::process;
    use std::rc::Rc;
    use std::thread;
    use std::time::Duration;

    use bolt_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId, WindowSize,
    };

    /// How long the test waits before it fails, rather than hang.
    const DEADLINE: Duration = Duration::from_secs(30);

    const SIZE: Size = Size {
        width: 400.0,
        height: 300.0,
    };

    /// What the handler saw.
    #[derive(Debug, Default)]
    struct Seen {
        started: bool,
        size: Option<WindowSize>,
        title_kept: bool,
        wakes: usize,
        wake_on_main: bool,
        closed: bool,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "probe".to_owned(),
                    size: SIZE,
                })
                .expect("the window opens");
            platform.set_title(window, "probe, renamed");
            let mut seen = self.seen.borrow_mut();
            seen.started = true;
            seen.size = platform.window_size(window);
            seen.title_kept = platform.window_size(WindowId::new(u64::MAX)).is_none();
            self.window = Some(window);

            let waker = platform.waker();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(100));
                waker.wake();
            });
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let mut seen = self.seen.borrow_mut();
            seen.wakes += 1;
            seen.wake_on_main = thread::current().name() == Some("main");
            if let Some(window) = self.window.take() {
                platform.close_window(window);
                seen.closed = platform.window_size(window).is_none();
            }
            platform.quit();
        }

        fn window_event(
            &mut self,
            _platform: &mut dyn Platform,
            _window: WindowId,
            _event: WindowEvent,
        ) {
        }
    }

    pub fn main() {
        thread::spawn(|| {
            thread::sleep(DEADLINE);
            eprintln!("run_loop: no wake within {DEADLINE:?}");
            process::exit(1);
        });

        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
        };
        bolt_platform_macos::run(probe).expect("the application runs");

        let seen = seen.borrow();
        assert!(seen.started, "{seen:?}");
        let size = seen.size.expect("the window has a size");
        assert_eq!(size.points, SIZE);
        assert_eq!(f64::from(size.pixels.width), SIZE.width * size.scale);
        assert_eq!(f64::from(size.pixels.height), SIZE.height * size.scale);
        assert!(seen.title_kept, "{seen:?}");
        assert_eq!(seen.wakes, 1, "{seen:?}");
        assert!(seen.wake_on_main, "{seen:?}");
        assert!(seen.closed, "{seen:?}");
        println!("run_loop: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

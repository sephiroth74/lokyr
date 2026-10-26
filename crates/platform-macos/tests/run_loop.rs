//! The application on the main thread's run loop: it starts, opens a window
//! of the size asked, whose content a `CAMetalLayer` backs, takes a wake from
//! another thread, draws the frames it asks for at once after a while without
//! any and at the display's pace after, stops its display link once it asks
//! for none, and returns from `run` once its handler quits. AppKit needs the
//! process's main thread, which this test has to itself.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    use bolt_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId, WindowSize,
    };
    use objc2_app_kit::NSView;
    use objc2_quartz_core::CAMetalLayer;
    use raw_window_handle::RawWindowHandle;

    use crate::common;

    const SIZE: Size = Size {
        width: 400.0,
        height: 300.0,
    };

    /// The title that the test gives its window.
    const TITLE: &str = "probe, renamed";

    /// The frames that the test asks for in a row, after the first.
    const PACED: usize = 30;

    /// The whiles without a frame, in milliseconds, after each of which the
    /// test asks for one, once the frames in a row are over. They differ, so
    /// that the requests fall at other points of a refresh, as a while of a
    /// whole number of refreshes would keep them at the same point.
    const IDLE_WAITS: [u64; 5] = [100, 113, 127, 139, 151];
    const IDLE_CHECKS: usize = IDLE_WAITS.len();

    /// What a frame that comes at once takes at most: a quarter of a refresh
    /// at 120 Hz. A display link that still ran would give the frame at its
    /// next refresh, anywhere within one, and within this as rarely as a
    /// quarter of the time at 120 Hz, an eighth at 60 Hz.
    fn at_once() -> Duration {
        Duration::from_secs(1) / 120 / 4
    }

    /// What the handler saw.
    #[derive(Debug, Default)]
    struct Seen {
        started: bool,
        size: Option<WindowSize>,
        /// The window's title, as AppKit has it.
        title: Option<String>,
        /// Whether a window that was never opened has no size.
        unknown_window_has_no_size: bool,
        /// The scale of the content view's layer, if it is a `CAMetalLayer`.
        metal_layer_scale: Option<f64>,
        wakes: usize,
        wakes_on_main: bool,
        /// From the wake that asked for the first frame to the frame.
        first_frame: Option<Duration>,
        /// From the second frame to the last that the test asked for in a
        /// row.
        paced: Option<Duration>,
        /// From each request after a while without frames to its frame.
        after_idle: Vec<Duration>,
        closed: bool,
    }

    /// Where the test is.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Phase {
        /// The first frame, after a while without any.
        First,
        /// The frames in a row.
        Paced,
        /// The frames asked for, one at a time, a while after the last.
        Idle,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        /// When the first wake was sent, from its thread.
        woken_at: Arc<Mutex<Option<Instant>>>,
        window: Option<WindowId>,
        phase: Phase,
        frames: usize,
        paced_from: Option<Instant>,
        requested_at: Option<Instant>,
    }

    impl Probe {
        fn close(&mut self, platform: &mut dyn Platform) {
            let Some(window) = self.window.take() else {
                return;
            };
            platform.close_window(window);
            self.seen.borrow_mut().closed = platform.window_size(window).is_none();
            platform.quit();
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "probe".to_owned(),
                    size: SIZE,
                })
                .expect("the window opens");
            platform.set_title(window, TITLE);
            let mut seen = self.seen.borrow_mut();
            seen.started = true;
            seen.size = platform.window_size(window);
            seen.unknown_window_has_no_size =
                platform.window_size(WindowId::new(u64::MAX)).is_none();
            let handle = platform
                .window_handle(window)
                .expect("the window has a handle");
            if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                // SAFETY: the handle holds the window's view while the
                // platform is borrowed.
                let view: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
                seen.title = view.window().map(|window| window.title().to_string());
                seen.metal_layer_scale = view
                    .layer()
                    .and_then(|layer| layer.downcast::<CAMetalLayer>().ok())
                    .map(|layer| layer.contentsScale());
            }
            self.window = Some(window);

            // A wake after a while without frames, which tells when it was
            // sent.
            let waker = platform.waker();
            let woken_at = Arc::clone(&self.woken_at);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                *woken_at.lock().unwrap() = Some(Instant::now());
                waker.wake();
            });
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let Some(window) = self.window else {
                return;
            };
            {
                let mut seen = self.seen.borrow_mut();
                seen.wakes += 1;
                let on_main = thread::current().name() == Some("main");
                seen.wakes_on_main = on_main && (seen.wakes == 1 || seen.wakes_on_main);
            }
            if self.phase == Phase::Idle {
                self.requested_at = Some(Instant::now());
            }
            platform.request_frame(window);
        }

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            if event != WindowEvent::Frame {
                return;
            }
            match self.phase {
                Phase::First => {
                    let woken_at = *self.woken_at.lock().unwrap();
                    self.seen.borrow_mut().first_frame = woken_at.map(|at| at.elapsed());
                    self.paced_from = Some(Instant::now());
                    self.phase = Phase::Paced;
                    platform.request_frame(window);
                }
                Phase::Paced => {
                    self.frames += 1;
                    if self.frames < PACED {
                        platform.request_frame(window);
                        return;
                    }
                    self.seen.borrow_mut().paced = self.paced_from.map(|from| from.elapsed());
                    // No frame for a while: the link stops at its next
                    // refresh.
                    self.phase = Phase::Idle;
                    common::wake_after(platform.waker(), IDLE_WAITS[0]);
                }
                Phase::Idle => {
                    let waited = self.requested_at.take().map(|at| at.elapsed());
                    let mut seen = self.seen.borrow_mut();
                    seen.after_idle.extend(waited);
                    if let Some(&wait) = IDLE_WAITS.get(seen.after_idle.len()) {
                        common::wake_after(platform.waker(), wait);
                        return;
                    }
                    drop(seen);
                    self.close(platform);
                }
            }
        }
    }

    pub fn main() {
        common::deadline("run_loop", 30);

        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            woken_at: Arc::default(),
            window: None,
            phase: Phase::First,
            frames: 0,
            paced_from: None,
            requested_at: None,
        };
        bolt_platform_macos::run(probe).expect("the application runs");

        let seen = seen.borrow();
        eprintln!("run_loop: {seen:?}");
        assert!(seen.started, "{seen:?}");
        let size = seen.size.expect("the window has a size");
        assert_eq!(size.points, SIZE);
        assert_eq!(f64::from(size.pixels.width), SIZE.width * size.scale);
        assert_eq!(f64::from(size.pixels.height), SIZE.height * size.scale);
        assert_eq!(seen.title.as_deref(), Some(TITLE), "{seen:?}");
        assert!(seen.unknown_window_has_no_size, "{seen:?}");
        assert_eq!(seen.metal_layer_scale, Some(size.scale), "{seen:?}");
        assert_eq!(seen.wakes, 1 + IDLE_CHECKS, "{seen:?}");
        assert!(seen.wakes_on_main, "{seen:?}");
        assert!(seen.closed, "{seen:?}");

        // A wake after a while without frames draws at once, and the frames
        // that follow keep to the display's pace: 60 Hz or 120 Hz, and at
        // worst half of it.
        assert!(
            seen.first_frame.is_some_and(|wait| wait < at_once()),
            "{seen:?}"
        );
        let paced = seen.paced.expect("the frames were drawn");
        let refresh = Duration::from_secs(1) / 120;
        let paced_frames = u32::try_from(PACED).unwrap();
        assert!(paced >= refresh * paced_frames * 4 / 5, "{seen:?}");
        assert!(
            paced <= Duration::from_secs(1) / 30 * paced_frames,
            "{seen:?}"
        );

        // Once no frame is asked for, the display link stops: every frame
        // asked for a while later comes at once, which a link that still ran
        // would give only by chance, at each of five points of a refresh.
        assert_eq!(seen.after_idle.len(), IDLE_CHECKS, "{seen:?}");
        assert!(
            seen.after_idle.iter().all(|&wait| wait < at_once()),
            "{seen:?}"
        );
        println!("run_loop: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

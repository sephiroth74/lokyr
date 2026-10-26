//! The application on the main thread's run loop: it starts, opens a window
//! of the size asked, whose content a `CAMetalLayer` backs, takes a wake from
//! another thread, draws the frames it asks for at once after a while without
//! any and at the display's pace after, wakes nothing once it asks for none,
//! and returns from `run` once its handler quits. AppKit needs the process's
//! main thread, which this test has to itself.

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::mem::MaybeUninit;
    use std::process;
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

    /// How long the test waits before it fails, rather than hang.
    const DEADLINE: Duration = Duration::from_secs(30);

    const SIZE: Size = Size {
        width: 400.0,
        height: 300.0,
    };

    /// The frames that the test asks for in a row, after the first.
    const PACED: usize = 30;

    /// The samples of the idle application's wakeups, and how long each
    /// lasts.
    const SAMPLES: usize = 10;
    const SAMPLE: Duration = Duration::from_millis(200);

    /// The most wakeups of a quiet sample. A display link that runs wakes
    /// the process at every refresh, 12 times or more in a sample; the
    /// window server's events, such as the pointer's over the window, come
    /// in bursts, which leave most samples quiet.
    const QUIET: u64 = 2;

    /// The interrupt wakeups of the process since it started, as
    /// `bolt-bench idle` counts them.
    fn interrupt_wakeups() -> u64 {
        let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
        // SAFETY: `info` is writable and sized for what `RUSAGE_INFO_V4`
        // fills in, and zeroed, so that it is initialized whether the call
        // succeeds or not.
        let info = unsafe {
            libc::proc_pid_rusage(
                libc::getpid(),
                libc::RUSAGE_INFO_V4,
                info.as_mut_ptr().cast(),
            );
            info.assume_init()
        };
        info.ri_interrupt_wkups
    }

    /// What the handler saw.
    #[derive(Debug, Default)]
    struct Seen {
        started: bool,
        size: Option<WindowSize>,
        title_kept: bool,
        /// The scale of the content view's layer, if it is a `CAMetalLayer`.
        metal_layer_scale: Option<f64>,
        wakes: usize,
        wake_on_main: bool,
        /// From the wake that asked for the first frame to the frame.
        first_frame: Option<Duration>,
        /// From the second frame to the last that the test asked for.
        paced: Option<Duration>,
        /// The wakeups of each sample, while the application asked for no
        /// frame.
        idle: Vec<u64>,
        closed: bool,
    }

    /// What the threads of the test tell the handler.
    #[derive(Debug, Default)]
    struct Shared {
        /// When the first wake was sent.
        woken_at: Option<Instant>,
        /// The wakeups of each sample of the idle while.
        idle: Vec<u64>,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        shared: Arc<Mutex<Shared>>,
        window: Option<WindowId>,
        frames: usize,
        paced_from: Option<Instant>,
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
            let handle = platform
                .window_handle(window)
                .expect("the window has a handle");
            if let RawWindowHandle::AppKit(appkit) = handle.as_raw() {
                // SAFETY: the handle holds the window's view while the
                // platform is borrowed.
                let view: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
                seen.metal_layer_scale = view
                    .layer()
                    .and_then(|layer| layer.downcast::<CAMetalLayer>().ok())
                    .map(|layer| layer.contentsScale());
            }
            self.window = Some(window);

            // A wake after a while without frames.
            let waker = platform.waker();
            let shared = Arc::clone(&self.shared);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                shared.lock().unwrap().woken_at = Some(Instant::now());
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
            let mut seen = self.seen.borrow_mut();
            seen.wakes += 1;
            seen.wake_on_main = thread::current().name() == Some("main");
            if seen.wakes == 1 {
                platform.request_frame(window);
                return;
            }
            // The second wake comes once the idle while is over.
            seen.idle = std::mem::take(&mut self.shared.lock().unwrap().idle);
            platform.close_window(window);
            seen.closed = platform.window_size(window).is_none();
            self.window = None;
            platform.quit();
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
            self.frames += 1;
            let mut seen = self.seen.borrow_mut();
            if self.frames == 1 {
                let woken_at = self.shared.lock().unwrap().woken_at;
                seen.first_frame = woken_at.map(|at| at.elapsed());
                self.paced_from = Some(Instant::now());
            }
            if self.frames <= PACED {
                platform.request_frame(window);
                return;
            }
            seen.paced = self.paced_from.map(|from| from.elapsed());

            // No more frames: the link stops at its next refresh, and the
            // application should wake no more.
            let waker = platform.waker();
            let shared = Arc::clone(&self.shared);
            thread::spawn(move || {
                thread::sleep(SAMPLE);
                let mut last = interrupt_wakeups();
                let mut samples = Vec::with_capacity(SAMPLES);
                for _ in 0..SAMPLES {
                    thread::sleep(SAMPLE);
                    let now = interrupt_wakeups();
                    samples.push(now - last);
                    last = now;
                }
                shared.lock().unwrap().idle = samples;
                waker.wake();
            });
        }
    }

    pub fn main() {
        thread::spawn(|| {
            thread::sleep(DEADLINE);
            eprintln!("run_loop: not done within {DEADLINE:?}");
            process::exit(1);
        });

        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            shared: Arc::default(),
            window: None,
            frames: 0,
            paced_from: None,
        };
        bolt_platform_macos::run(probe).expect("the application runs");

        let seen = seen.borrow();
        eprintln!("run_loop: {seen:?}");
        assert!(seen.started, "{seen:?}");
        let size = seen.size.expect("the window has a size");
        assert_eq!(size.points, SIZE);
        assert_eq!(f64::from(size.pixels.width), SIZE.width * size.scale);
        assert_eq!(f64::from(size.pixels.height), SIZE.height * size.scale);
        assert!(seen.title_kept, "{seen:?}");
        assert_eq!(seen.metal_layer_scale, Some(size.scale), "{seen:?}");
        assert_eq!(seen.wakes, 2, "{seen:?}");
        assert!(seen.wake_on_main, "{seen:?}");
        assert!(seen.closed, "{seen:?}");

        // A wake after a while without frames draws within a refresh, at
        // 120 Hz, and frames that follow keep to the display's pace, 60 Hz
        // or 120 Hz, and at worst half of it.
        let refresh = Duration::from_secs(1) / 120;
        assert!(
            seen.first_frame.is_some_and(|wait| wait < refresh),
            "{seen:?}"
        );
        let paced = seen.paced.expect("the frames were drawn");
        let paced_frames = u32::try_from(PACED).unwrap();
        assert!(paced >= refresh * paced_frames * 4 / 5, "{seen:?}");
        assert!(
            paced <= Duration::from_secs(1) / 30 * paced_frames,
            "{seen:?}"
        );

        // Once no frame is asked for, the display link stops.
        let quiet = seen
            .idle
            .iter()
            .filter(|&&wakeups| wakeups <= QUIET)
            .count();
        assert!(quiet * 2 >= SAMPLES, "{seen:?}");
        println!("run_loop: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

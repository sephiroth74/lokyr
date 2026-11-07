//! The windows' frames on the screens' plane: a window moved by its
//! content's top left corner has its frame there; the window that shows
//! at a point is the front one of the app's there, and none where the app
//! has none; the drag's image, over them, which the pointer goes through,
//! is none of them, and has a layer while it shows.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, ScreenPoint, ScreenRect, Size, WindowAttributes, WindowEvent,
        WindowId,
    };

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        first: Option<WindowId>,
        second: Option<WindowId>,
        /// The windows' frames once moved, the first's and the second's.
        frames: Vec<Option<ScreenRect>>,
        /// The windows at the first's alone, at both's, and off the
        /// screens.
        at: Vec<Option<WindowId>>,
        /// The windows at both's under the drag's image, and whether the
        /// image had a layer as it showed and once it hid.
        under_image: Option<Option<WindowId>>,
        handles: Vec<bool>,
        /// When the second window opened, from which the probe waits for
        /// the window server to show it.
        opened: Option<Instant>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    // Near the main screen's top left corner, away from its middle, where
    // the system's windows of other apps, as an alert, show over a small
    // screen's, as a runner of the CI's has.
    const FIRST: ScreenPoint = ScreenPoint { x: 40.0, y: 80.0 };
    const SECOND: ScreenPoint = ScreenPoint { x: 110.0, y: 130.0 };
    const FIRST_ALONE: ScreenPoint = ScreenPoint { x: 60.0, y: 100.0 };
    const BOTH: ScreenPoint = ScreenPoint { x: 180.0, y: 200.0 };

    /// How long the window server may take to show the second window, as a
    /// runner of the CI's can be slow, and how often the probe looks.
    const SHOWN_WITHIN: Duration = Duration::from_secs(5);
    const LOOK_AGAIN: Duration = Duration::from_millis(100);

    fn open(platform: &mut dyn Platform) -> WindowId {
        platform
            .create_window(&WindowAttributes {
                title: "Screen places".to_owned(),
                size: Size {
                    width: 320.0,
                    height: 200.0,
                },
                remember: false,
                restore: false,
                restore_size: false,
            })
            .expect("a window")
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let first = open(platform);
            platform.set_window_origin(first, FIRST);
            self.0.borrow_mut().first = Some(first);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            let AppEvent::Timer(_) = event else {
                return;
            };
            let mut seen = self.0.borrow_mut();
            let (Some(first), Some(second)) = (seen.first, seen.second) else {
                return;
            };
            // Until the window server shows the second window over the
            // first, for a while.
            if platform.window_at(BOTH) != Some(second)
                && seen
                    .opened
                    .is_some_and(|opened| opened.elapsed() < SHOWN_WITHIN)
            {
                platform.start_timer(Instant::now() + LOOK_AGAIN);
                return;
            }
            seen.frames = vec![platform.window_frame(first), platform.window_frame(second)];
            let off = ScreenPoint {
                x: -1.0e6,
                y: -1.0e6,
            };
            seen.at = vec![
                platform.window_at(FIRST_ALONE),
                platform.window_at(BOTH),
                platform.window_at(off),
            ];
            platform.show_drag_image(ScreenRect {
                x: BOTH.x - 50.0,
                y: BOTH.y - 50.0,
                width: 100.0,
                height: 100.0,
            });
            seen.under_image = Some(platform.window_at(BOTH));
            seen.handles.push(platform.drag_image_handle().is_some());
            platform.hide_drag_image(None);
            seen.handles.push(platform.drag_image_handle().is_some());
            platform.quit();
        }

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            if event != WindowEvent::Focused(true) {
                return;
            }
            let mut seen = self.0.borrow_mut();
            if seen.first == Some(window) && seen.second.is_none() {
                let second = open(platform);
                platform.set_window_origin(second, SECOND);
                seen.second = Some(second);
                seen.opened = Some(Instant::now());
                // A moment for the window server, before the checks.
                platform.start_timer(Instant::now() + Duration::from_millis(300));
            }
        }
    }

    pub fn main() {
        common::deadline("screen_places", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let (Some(first), Some(second)) = (seen.first, seen.second) else {
            panic!("two windows: {seen:?}");
        };
        let [Some(first_frame), Some(second_frame)] = seen.frames[..] else {
            panic!("two frames: {seen:?}");
        };
        assert_eq!(first_frame.origin(), FIRST, "{seen:?}");
        assert_eq!(second_frame.origin(), SECOND, "{seen:?}");
        assert_eq!(first_frame.width, 320.0, "{seen:?}");
        assert!(first_frame.height > 200.0, "the titlebar's too: {seen:?}");
        assert_eq!(seen.at, [Some(first), Some(second), None], "{seen:?}");
        assert_eq!(seen.under_image, Some(Some(second)), "{seen:?}");
        assert_eq!(seen.handles, [true, false], "{seen:?}");
        println!("screen_places: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

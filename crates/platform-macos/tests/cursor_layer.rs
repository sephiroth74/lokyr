//! The cursor's layer of a window: a view over the content's, where the app
//! places it, whose opacity the window server animates as the blink says,
//! faded or switched, and which clicks go through.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use lokyr_platform_core::{
        AppEvent, Blink, CursorPlace, Easing, Handler, Platform, Point, Size, WindowAttributes,
        WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSView};
    use objc2_foundation::{NSNumber, NSPoint, NSRect, NSString};
    use objc2_quartz_core::{CAKeyframeAnimation, CAMediaTiming};

    use crate::common;

    /// An animation's values, key times, duration, and whether it switches.
    type Animation = (Vec<f64>, Vec<f64>, f64, bool);

    #[derive(Debug, Default)]
    struct Seen {
        /// The view's frame, and whether it is hidden, after each place.
        frames: Vec<(NSRect, bool)>,
        /// The animation's values, key times, duration, and whether it
        /// switches, after each place; `None` without one.
        animations: Vec<Option<Animation>>,
        /// Whether a click over the cursor goes to the content's view.
        through: bool,
        handle: bool,
        /// The content view's height, in points.
        height: f64,
    }

    struct Probe(Rc<RefCell<Seen>>);

    /// The content view of the probe's window, and the cursor's, over it.
    fn views() -> (Retained<NSView>, Retained<NSView>) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the window");
        let content = window.contentView().expect("the content view");
        let cursor = content.subviews().firstObject().expect("the cursor's view");
        (content, cursor)
    }

    fn numbers(array: Option<Retained<objc2_foundation::NSArray<NSNumber>>>) -> Vec<f64> {
        array.map_or_else(Vec::new, |array| {
            (0..array.count())
                .map(|index| array.objectAtIndex(index).doubleValue())
                .collect()
        })
    }

    /// The cursor's animation, as the layer runs it.
    fn animation(cursor: &NSView) -> Option<Animation> {
        let layer = cursor.layer()?;
        // SAFETY: the key that the backend gives its animation.
        let found = unsafe { layer.animationForKey(&NSString::from_str("blink")) }?;
        let keyframes = found.downcast::<CAKeyframeAnimation>().ok()?;
        let values = keyframes.values().map_or_else(Vec::new, |values| {
            (0..values.count())
                .filter_map(|index| values.objectAtIndex(index).downcast::<NSNumber>().ok())
                .map(|number| number.doubleValue())
                .collect()
        });
        let switched = keyframes.calculationMode().to_string() == "discrete";
        Some((
            values,
            numbers(keyframes.keyTimes()),
            keyframes.duration(),
            switched,
        ))
    }

    fn look(seen: &mut Seen) {
        let (_, cursor) = views();
        seen.frames.push((cursor.frame(), cursor.isHidden()));
        seen.animations.push(animation(&cursor));
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "The cursor's layer".to_owned(),
                    size: Size {
                        width: 400.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                })
                .expect("the window opens");
            let mut seen = self.0.borrow_mut();
            seen.handle = platform.cursor_handle(window).is_some();
            seen.height = views().0.bounds().size.height;
            let millis = Duration::from_millis;
            let breathing = Blink {
                start: Instant::now(),
                visible: millis(500),
                fade_out: millis(900),
                fade_out_curve: Easing::EaseInOut,
                hidden: millis(500),
                fade_in: millis(900),
                fade_in_curve: Easing::EaseInOut,
            };
            let place = |blink| CursorPlace {
                origin: Point { x: 20.0, y: 50.0 },
                size: Size {
                    width: 8.0,
                    height: 16.0,
                },
                blink,
            };
            // Faded, then switched, then steady, then hidden.
            platform.set_cursor(window, Some(&place(Some(breathing))));
            look(&mut seen);
            let switched = Blink {
                fade_out: Duration::ZERO,
                fade_in: Duration::ZERO,
                ..breathing
            };
            platform.set_cursor(window, Some(&place(Some(switched))));
            look(&mut seen);
            platform.set_cursor(window, Some(&place(None)));
            look(&mut seen);
            // A click over the cursor's cell.
            let (content, cursor) = views();
            let middle = NSPoint::new(cursor.frame().origin.x + 4.0, cursor.frame().origin.y + 8.0);
            // The window's frame view, the content's superview, takes the
            // window's coordinates.
            let hit = content.hitTest(content.convertPoint_toView(middle, None));
            seen.through = hit.is_some_and(|hit| hit == content);
            platform.set_cursor(window, None);
            look(&mut seen);
            common::wake_after(platform.waker(), 100);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event == AppEvent::Wake {
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("cursor_layer", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert!(seen.handle, "{seen:?}");
        // From the content's top left corner.
        let (frame, hidden) = seen.frames[0];
        assert_eq!(
            (
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height
            ),
            (20.0, seen.height - 50.0 - 16.0, 8.0, 16.0),
            "{seen:?}"
        );
        assert!(!hidden, "{seen:?}");
        // Held, faded out, held, faded in, over 2.8 s.
        let (values, times, duration, switched) = seen.animations[0].clone().expect("a fade");
        assert_eq!(values, [1.0, 1.0, 0.0, 0.0, 1.0], "{seen:?}");
        let expected = [0.0, 0.5, 1.4, 1.9, 2.8].map(|time| time / 2.8);
        assert!(
            times
                .iter()
                .zip(expected)
                .all(|(time, wanted)| (time - wanted).abs() < 1e-9),
            "{seen:?}"
        );
        assert!((duration - 2.8).abs() < 1e-9 && !switched, "{seen:?}");
        // Without fades, switched, over the holds.
        let (values, _, duration, switched) = seen.animations[1].clone().expect("a switch");
        assert_eq!(values, [1.0, 0.0, 1.0], "{seen:?}");
        assert!((duration - 1.0).abs() < 1e-9 && switched, "{seen:?}");
        // Steady, then hidden.
        assert_eq!(seen.animations[2], None, "{seen:?}");
        assert!(!seen.frames[2].1 && seen.frames[3].1, "{seen:?}");
        assert!(seen.through, "{seen:?}");
        println!("cursor_layer: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

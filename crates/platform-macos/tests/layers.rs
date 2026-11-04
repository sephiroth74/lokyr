//! The layers of a window, of the text that blinks and of the cursor: views
//! over the content's, the cursor's over the text's, where the app places
//! them, whose opacity the window server animates as the blink says, faded
//! or switched, and which clicks go through.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use lokyr_platform_core::{
        AppEvent, Blink, Easing, Handler, Layer, LayerPlace, Platform, Point, Size,
        WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSView, NSWorkspace};
    use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};
    use objc2_quartz_core::{CAKeyframeAnimation, CAMediaTiming};

    use crate::common;

    /// An animation's values, key times, duration, and whether it switches.
    type Animation = (Vec<f64>, Vec<f64>, f64, bool);

    #[derive(Debug, Default)]
    struct Seen {
        /// The cursor's view's frame, and whether it is hidden, after each
        /// place.
        frames: Vec<(NSRect, bool)>,
        /// The animation's values, key times, duration, and whether it
        /// switches, after each place; `None` without one.
        animations: Vec<Option<Animation>>,
        /// Whether a click over the cursor goes to the content's view.
        through: bool,
        handles: [bool; 2],
        /// The text's view's frame, whether it is hidden, and its
        /// animation, placed, then hidden; and the cursor's frame then.
        text: Vec<(NSRect, bool, Option<Animation>)>,
        cursor_then: Option<(NSRect, bool)>,
        /// The cursor's distance from the content's top, and its frame's
        /// size, before and after the window grows, and shrinks.
        resized: Vec<(f64, NSSize)>,
        /// The content view's height, in points.
        height: f64,
    }

    struct Probe(Rc<RefCell<Seen>>);

    /// The content view of the probe's window, the text's, over it, and
    /// the cursor's, over both.
    fn views() -> (Retained<NSView>, Retained<NSView>, Retained<NSView>) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the window");
        let content = window.contentView().expect("the content view");
        let subviews = content.subviews();
        // The sweeps' view first, under the layers'.
        assert_eq!(subviews.count(), 3, "the layers' views");
        let text = subviews.objectAtIndex(1);
        let cursor = subviews.objectAtIndex(2);
        (content, text, cursor)
    }

    fn numbers(array: Option<Retained<objc2_foundation::NSArray<NSNumber>>>) -> Vec<f64> {
        array.map_or_else(Vec::new, |array| {
            (0..array.count())
                .map(|index| array.objectAtIndex(index).doubleValue())
                .collect()
        })
    }

    /// The animation of a layer's view, as the layer runs it.
    fn animation(view: &NSView) -> Option<Animation> {
        let layer = view.layer()?;
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
        let (_, _, cursor) = views();
        seen.frames.push((cursor.frame(), cursor.isHidden()));
        seen.animations.push(animation(&cursor));
    }

    fn look_at_text(seen: &mut Seen) {
        let (_, text, _) = views();
        seen.text
            .push((text.frame(), text.isHidden(), animation(&text)));
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
                    restore_size: false,
                })
                .expect("the window opens");
            let mut seen = self.0.borrow_mut();
            seen.handles = [Layer::BlinkingText, Layer::Cursor]
                .map(|layer| platform.layer_handle(window, layer).is_some());
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
            let place = |blink| LayerPlace {
                origin: Point { x: 20.0, y: 50.0 },
                size: Size {
                    width: 8.0,
                    height: 16.0,
                },
                blink,
            };
            // Faded, then switched, then steady, then hidden.
            let cursor = Layer::Cursor;
            platform.place_layer(window, cursor, Some(&place(Some(breathing))));
            look(&mut seen);
            let switched = Blink {
                fade_out: Duration::ZERO,
                fade_in: Duration::ZERO,
                ..breathing
            };
            platform.place_layer(window, cursor, Some(&place(Some(switched))));
            look(&mut seen);
            platform.place_layer(window, cursor, Some(&place(None)));
            look(&mut seen);
            // The text's, rows of it, switched, under the cursor's, which it
            // leaves where it is; then hidden.
            let rows = LayerPlace {
                origin: Point { x: 4.0, y: 40.0 },
                size: Size {
                    width: 392.0,
                    height: 48.0,
                },
                blink: Some(switched),
            };
            platform.place_layer(window, Layer::BlinkingText, Some(&rows));
            look_at_text(&mut seen);
            let (_, _, shown) = views();
            seen.cursor_then = Some((shown.frame(), shown.isHidden()));
            platform.place_layer(window, Layer::BlinkingText, None);
            look_at_text(&mut seen);
            // A click over the cursor's cell.
            let (content, _, cursor) = views();
            let middle = NSPoint::new(cursor.frame().origin.x + 4.0, cursor.frame().origin.y + 8.0);
            // The window's frame view, the content's superview, takes the
            // window's coordinates.
            let hit = content.hitTest(content.convertPoint_toView(middle, None));
            seen.through = hit.is_some_and(|hit| hit == content);
            // The window's resizes keep the cursor where the app placed it,
            // from the content's top, until it places it again.
            let from_top = |content: &NSView, cursor: &NSView| {
                let frame = cursor.frame();
                let top = content.bounds().size.height - frame.origin.y - frame.size.height;
                (top, frame.size)
            };
            seen.resized.push(from_top(&content, &cursor));
            let ns_window = content.window().expect("the window");
            for height in [420.0, 200.0] {
                ns_window.setContentSize(NSSize::new(400.0, height));
                let (content, _, cursor) = views();
                seen.resized.push(from_top(&content, &cursor));
            }
            platform.place_layer(window, Layer::Cursor, None);
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
        common::deadline("layers", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.handles, [true; 2], "{seen:?}");
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
        // Held, faded out, held, faded in, over 2.8 s; where the system
        // reduces motion, as a CI's runner can, switched at each fade's
        // middle.
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        let (values, times, duration, switched) = seen.animations[0].clone().expect("a fade");
        let (wanted_values, wanted_times): (&[f64], &[f64]) = if reduced {
            (&[1.0, 0.0, 1.0], &[0.0, 0.95, 2.35, 2.8])
        } else {
            (&[1.0, 1.0, 0.0, 0.0, 1.0], &[0.0, 0.5, 1.4, 1.9, 2.8])
        };
        assert_eq!(values, wanted_values, "{reduced} {seen:?}");
        assert!(
            times.len() == wanted_times.len()
                && times
                    .iter()
                    .zip(wanted_times)
                    .all(|(time, wanted)| (time - wanted / 2.8).abs() < 1e-9),
            "{reduced} {seen:?}"
        );
        assert!(
            (duration - 2.8).abs() < 1e-9 && switched == reduced,
            "{reduced} {seen:?}"
        );
        // Without fades, switched, over the holds.
        let (values, _, duration, switched) = seen.animations[1].clone().expect("a switch");
        assert_eq!(values, [1.0, 0.0, 1.0], "{seen:?}");
        assert!((duration - 1.0).abs() < 1e-9 && switched, "{seen:?}");
        // Steady, then hidden.
        assert_eq!(seen.animations[2], None, "{seen:?}");
        assert!(!seen.frames[2].1 && seen.frames[3].1, "{seen:?}");
        assert!(seen.through, "{seen:?}");
        let placed = (50.0, NSSize::new(8.0, 16.0));
        assert_eq!(seen.resized, [placed; 3], "{seen:?}");
        // The text's rows, switched, then hidden; the cursor's as it was.
        let (frame, hidden, ref animation) = seen.text[0];
        assert_eq!(
            (
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height
            ),
            (4.0, seen.height - 40.0 - 48.0, 392.0, 48.0),
            "{seen:?}"
        );
        assert!(!hidden, "{seen:?}");
        let (values, _, _, switched) = animation.clone().expect("a switch");
        assert!(values == [1.0, 0.0, 1.0] && switched, "{seen:?}");
        assert!(seen.text[1].1, "{seen:?}");
        assert_eq!(seen.cursor_then, Some(seen.frames[2]), "{seen:?}");
        println!("layers: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

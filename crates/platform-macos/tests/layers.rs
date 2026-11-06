//! The layers of a window, of the text that blinks and of the cursor: views
//! over the content's, the cursor's over the text's, where the app places
//! them, whose opacity the window server animates as the blink says, faded
//! or switched, and which clicks go through; the sweeps of its
//! progresses, under them, which keep their distance from the top as the
//! window resizes; and the fills of its progresses, under the sweeps, which
//! keep their layers as their values change and slide to them.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use lokyr_platform_core::{
        AppEvent, Blink, Easing, Fill, FillShape, Handler, Layer, LayerPlace, Platform, Point,
        Size, Sweep, SweepShape, WindowAttributes, WindowEvent, WindowId,
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
        /// A sweep's distance from the content's top, and its frame's size,
        /// as the cursor's.
        swept: Vec<(f64, NSSize)>,
        /// The content view's height, in points.
        height: f64,
        /// A fill's layers as it showed, of a bar's width, and as its value
        /// changed: whether it kept its layer, the bar's width, and whether
        /// it slides; then how many fills show once another took its place.
        filled: Option<f64>,
        refilled: Option<(bool, f64, bool)>,
        replaced: usize,
        /// The dots of a sweep: their centers from its square's top left
        /// corner, how many of them light of an animation of their own, and
        /// whether their group pulses.
        dots: Option<Dots>,
    }

    /// The dots' centers, how many light of their own, and whether their
    /// group pulses.
    type Dots = (Vec<(f64, f64)>, usize, bool);

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
        // The fills' view first, the sweeps' over it, under the layers'.
        assert_eq!(subviews.count(), 4, "the layers' views");
        let text = subviews.objectAtIndex(2);
        let cursor = subviews.objectAtIndex(3);
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
            // A bar's sweep 40 points from the top, which no call places
            // again as the window resizes.
            let bar = (
                Point { x: 10.0, y: 40.0 },
                Size {
                    width: 100.0,
                    height: 4.0,
                },
            );
            platform.set_sweeps(
                window,
                &[Sweep {
                    origin: bar.0,
                    size: bar.1,
                    within: bar,
                    radius: 2.0,
                    shape: SweepShape::Bar { width: 25.0 },
                    color: [0.0, 0.5, 1.0, 1.0],
                    period: Duration::from_secs(1),
                    start: Instant::now(),
                }],
            );
            let sweep_from_top = |content: &NSView| {
                let sweeps = content.subviews().objectAtIndex(1);
                let layer = sweeps.layer().expect("the sweeps' layer");
                // SAFETY: the layer's sublayers, its sweeps'.
                let within = unsafe { layer.sublayers() }
                    .expect("a sweep's layer")
                    .objectAtIndex(0)
                    .frame();
                let top = content.bounds().size.height - within.origin.y - within.size.height;
                (top, within.size)
            };
            seen.swept.push(sweep_from_top(&content));
            let ns_window = content.window().expect("the window");
            for height in [420.0, 200.0] {
                ns_window.setContentSize(NSSize::new(400.0, height));
                let (content, _, cursor) = views();
                seen.resized.push(from_top(&content, &cursor));
                seen.swept.push(sweep_from_top(&content));
            }
            // Three dots in a square of 10 points, 60 from the top.
            let square = (
                Point { x: 20.0, y: 60.0 },
                Size {
                    width: 10.0,
                    height: 10.0,
                },
            );
            platform.set_sweeps(
                window,
                &[Sweep {
                    origin: square.0,
                    size: square.1,
                    within: square,
                    radius: 0.0,
                    shape: SweepShape::Dots {
                        diameter: 3.0,
                        centers: [(5.0, 2.0), (8.5, 8.0), (1.5, 8.0)],
                    },
                    color: [0.5, 0.5, 0.5, 1.0],
                    period: Duration::from_millis(1200),
                    start: Instant::now(),
                }],
            );
            {
                let (content, _, _) = views();
                let sweeps = content.subviews().objectAtIndex(1);
                let layer = sweeps.layer().expect("the sweeps' layer");
                let under = |layer: &objc2_quartz_core::CALayer| {
                    // SAFETY: the layer's sublayers, of the sweep's parts.
                    unsafe { layer.sublayers() }
                        .expect("a part")
                        .objectAtIndex(0)
                };
                // The part of the window, the square, the dots' group.
                let group = under(&under(&under(&layer)));
                // SAFETY: the group's sublayers, its dots.
                let dots = unsafe { group.sublayers() }.expect("the dots");
                let key = NSString::from_str("sweep");
                let mut centers = Vec::new();
                let mut lit = 0;
                for at in 0..dots.count() {
                    let dot = dots.objectAtIndex(at);
                    let position = dot.position();
                    centers.push((position.x, 10.0 - position.y));
                    // SAFETY: the key that the backend gives its sweeps.
                    lit += usize::from(unsafe { dot.animationForKey(&key) }.is_some());
                }
                // SAFETY: as of the dots.
                let pulses = unsafe { group.animationForKey(&key) }.is_some();
                seen.dots = Some((centers, lit, pulses));
            }
            platform.place_layer(window, Layer::Cursor, None);
            look(&mut seen);
            // A bar's fill, a quarter of 100 points; then a half, of the
            // same id, which keeps its layers and slides; then another's.
            let fill = Fill {
                id: 7,
                origin: bar.0,
                size: bar.1,
                within: bar,
                radius: 2.0,
                shape: FillShape::Bar,
                color: [0.0, 0.5, 1.0, 1.0],
                value: 0.25,
            };
            let fill_layers = |content: &NSView| {
                let fills = content.subviews().objectAtIndex(0);
                let layer = fills.layer().expect("the fills' layer");
                // SAFETY: the layer's sublayers, its fills'.
                unsafe { layer.sublayers() }
            };
            // The bar's layer, within its box's, within the part of the
            // window where it shows.
            let bar_of = |within: &objc2_quartz_core::CALayer| {
                // SAFETY: the layer's sublayers, its fill's box.
                let boxed = unsafe { within.sublayers() }
                    .expect("the box")
                    .objectAtIndex(0);
                // SAFETY: the box's sublayers, its bar.
                unsafe { boxed.sublayers() }
                    .expect("the bar")
                    .objectAtIndex(0)
            };
            let (content, _, _) = views();
            platform.set_fills(window, &[fill]);
            let first = fill_layers(&content)
                .expect("a fill's layer")
                .objectAtIndex(0);
            seen.filled = Some(bar_of(&first).bounds().size.width);
            platform.set_fills(window, &[Fill { value: 0.5, ..fill }]);
            let again = fill_layers(&content)
                .expect("a fill's layer")
                .objectAtIndex(0);
            let moved = bar_of(&again);
            // SAFETY: the key that the backend gives its slide.
            let slides = unsafe { moved.animationForKey(&NSString::from_str("slide")) }.is_some();
            seen.refilled = Some((again == first, moved.bounds().size.width, slides));
            platform.set_fills(window, &[Fill { id: 8, ..fill }]);
            seen.replaced = fill_layers(&content).map_or(0, |layers| layers.count());
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
        assert_eq!(seen.swept, [(40.0, NSSize::new(100.0, 4.0)); 3], "{seen:?}");
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
        // The fill as far as its value, its layers kept as it changes, which
        // slides there, but where the system reduces motion.
        assert_eq!(seen.filled, Some(25.0), "{seen:?}");
        assert_eq!(seen.refilled, Some((true, 50.0, !reduced)), "{seen:?}");
        assert_eq!(seen.replaced, 1, "{seen:?}");
        // The dots at their centers, each lit of its own animation, or all
        // pulsing together where the system reduces motion.
        let lit = if reduced { 0 } else { 3 };
        assert_eq!(
            seen.dots,
            Some((vec![(5.0, 2.0), (8.5, 8.0), (1.5, 8.0)], lit, reduced)),
            "{seen:?}"
        );
        println!("layers: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

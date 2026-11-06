//! The sweeps of a window: the parts of its indeterminate progresses that
//! the window server moves without the app's frames, on layers of a view
//! over the content's, under the layers of the text that blinks and of the
//! cursor: a segment across a bar, an arc round a ring, three dots lit in
//! turn, each within the part of the window where its progress shows. Where
//! the system reduces motion, the segment holds in the bar's middle, the
//! arc at the ring's top and the dots lit alike, their opacity pulsing.

use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::time::Instant;

use lokyr_platform_core::{Point, Size, Sweep, SweepShape};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSResponder, NSView, NSWorkspace};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_foundation::{NSArray, NSNumber, NSObject, NSPoint, NSSize, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CACurrentMediaTime, CAKeyframeAnimation, CALayer, CAMediaTiming,
    CAShapeLayer, CATransaction, kCALineCapButt,
};

use crate::layer::{opacities, timing};

/// The key of a sweep's animation on its layer.
const SWEEP: &str = "sweep";

/// The opacities between which a sweep pulses where the system reduces
/// motion.
const PULSE: (f64, f64) = (1.0, 0.35);

/// A dot's opacities as it lights and fades, a round of the dots: from its
/// least, lit within a tenth of the round, faded by seven tenths, and held
/// least to the round's end, as the next dots light.
const DOT_LIGHT: ([f64; 4], [f64; 4]) = ([0.25, 1.0, 0.25, 0.25], [0.0, 0.1, 0.7, 1.0]);

pub(crate) struct SweepsViewIvars {
    /// The sweeps that its layers show, of the view's height and of the
    /// system's reduced motion as they were placed.
    shown: RefCell<(Vec<Sweep>, f64, bool)>,
}

define_class!(
    // SAFETY: `NSView` has no requirement of its subclasses that this one
    // breaks, and the view does not implement `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = SweepsViewIvars]
    pub(crate) struct SweepsView;

    impl SweepsView {
        // SAFETY: AppKit's signature: clicks go through, to the content's
        // view.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }

        // SAFETY: AppKit's signature, which a resize of the content's view
        // calls through the view's autoresizing.
        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            // SAFETY: `NSView`'s own, of this signature.
            let () = unsafe { msg_send![super(self), setFrameSize: size] };
            self.place();
        }
    }
);

impl SweepsView {
    /// A view of no sweeps over the content view `over`, as large, over its
    /// other views so far.
    pub(crate) fn new(mtm: MainThreadMarker, over: &NSView) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SweepsViewIvars {
            shown: RefCell::new((Vec::new(), 0.0, false)),
        });
        // SAFETY: `NSView`'s `initWithFrame:` has this signature.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: over.bounds()] };
        view.setWantsLayer(true);
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        over.addSubview(&view);
        view
    }

    /// Places the sweeps that it shows again at their distance from the
    /// view's top, as a resize changed its height, which a layer's origin
    /// counts from its bottom: their animations go on as they were.
    fn place(&self) {
        let height = self.bounds().size.height;
        let mut shown = self.ivars().shown.borrow_mut();
        if shown.1 == height {
            return;
        }
        shown.1 = height;
        // SAFETY: the view's layer's sublayers, its sweeps', in their order.
        let Some(layers) = self.layer().and_then(|layer| unsafe { layer.sublayers() }) else {
            return;
        };
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for (at, sweep) in shown.0.iter().enumerate().take(layers.count()) {
            layers
                .objectAtIndex(at)
                .setFrame(within_frame(sweep, height));
        }
        CATransaction::commit();
    }

    /// Shows `sweeps`, in place of those that it showed, unless they are the
    /// same, of the same height and motion, which go on.
    pub(crate) fn set(&self, sweeps: &[Sweep]) {
        let height = self.bounds().size.height;
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        {
            let shown = self.ivars().shown.borrow();
            if shown.0 == sweeps && shown.1 == height && shown.2 == reduced {
                return;
            }
        }
        *self.ivars().shown.borrow_mut() = (sweeps.to_vec(), height, reduced);
        let Some(layer) = self.layer() else {
            return;
        };
        // No implicit animation of the layers that come and go.
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        // SAFETY: no sublayers, which the view's layer takes.
        unsafe { layer.setSublayers(None) };
        let now = layer.convertTime_fromLayer(CACurrentMediaTime(), None);
        for sweep in sweeps {
            layer.addSublayer(&sweep_layer(sweep, height, (reduced, now)));
        }
        CATransaction::commit();
    }
}

/// The frame of `origin` and `size`, from the top left corner of a frame
/// at `from`, as a layer's of a superlayer there, whose origin is its
/// bottom left corner.
pub(crate) fn up(origin: Point, size: Size, from: (f64, f64)) -> CGRect {
    CGRect::new(
        CGPoint::new(origin.x - from.0, from.1 - origin.y - size.height),
        CGSize::new(size.width, size.height),
    )
}

/// The frame of the layer of the part of the window where `sweep` shows, in
/// a view `height` points high.
fn within_frame(sweep: &Sweep, height: f64) -> CGRect {
    let (origin, size) = sweep.within;
    up(origin, size, (0.0, height))
}

/// The layer of `sweep`, over a view `height` points high, whose time is
/// `now`: the part of the window where its progress shows, which clips its
/// bar or its ring, which clips what moves.
fn sweep_layer(sweep: &Sweep, height: f64, (reduced, now): (bool, f64)) -> Retained<CALayer> {
    let (within_origin, within_size) = sweep.within;
    let within = CALayer::new();
    within.setFrame(within_frame(sweep, height));
    within.setMasksToBounds(true);
    let track = CALayer::new();
    track.setFrame(up(
        sweep.origin,
        sweep.size,
        (within_origin.x, within_origin.y + within_size.height),
    ));
    track.setMasksToBounds(true);
    track.setCornerRadius(sweep.radius);
    within.addSublayer(&track);
    let [red, green, blue, alpha] = sweep.color;
    let color = CGColor::new_srgb(red, green, blue, alpha);
    let (width, tall) = (sweep.size.width, sweep.size.height);
    let animation = match sweep.shape {
        SweepShape::Bar { width: segment } => {
            let moving = CALayer::new();
            moving.setBounds(CGRect::new(CGPoint::ZERO, CGSize::new(segment, tall)));
            moving.setCornerRadius(sweep.radius.min(tall / 2.0));
            moving.setBackgroundColor(Some(&color));
            let (from, to) = if reduced {
                (width / 2.0, width / 2.0)
            } else {
                (-segment / 2.0, width + segment / 2.0)
            };
            moving.setPosition(CGPoint::new(from, tall / 2.0));
            track.addSublayer(&moving);
            (moving, "position.x", (from, to))
        }
        SweepShape::Ring { span, thickness } => {
            let side = width.min(tall);
            let moving = CAShapeLayer::new();
            moving.setFrame(CGRect::new(CGPoint::ZERO, CGSize::new(side, side)));
            let path = CGMutablePath::new();
            let middle = side / 2.0;
            // From the top, clockwise, as the layer's y goes up.
            // SAFETY: a path of the call's own, and no transform.
            unsafe {
                CGMutablePath::add_arc(
                    Some(&path),
                    std::ptr::null(),
                    middle,
                    middle,
                    (middle - thickness / 2.0).max(0.0),
                    FRAC_PI_2,
                    FRAC_PI_2 - span * TAU,
                    true,
                );
            }
            moving.setPath(Some(&path));
            moving.setFillColor(None);
            moving.setStrokeColor(Some(&color));
            moving.setLineWidth(thickness);
            // SAFETY: Core Animation's constant.
            moving.setLineCap(unsafe { kCALineCapButt });
            track.addSublayer(&moving);
            // Clockwise, as the layer's y goes up: of a negative angle.
            (
                Retained::into_super(moving),
                "transform.rotation.z",
                (0.0, -TAU),
            )
        }
        SweepShape::Outline { span, thickness } => {
            let moving = CAShapeLayer::new();
            moving.setFrame(CGRect::new(CGPoint::ZERO, CGSize::new(width, tall)));
            let (path, length) = outline_path(width, tall, sweep.radius, thickness);
            moving.setPath(Some(&path));
            moving.setFillColor(None);
            moving.setStrokeColor(Some(&color));
            moving.setLineWidth(thickness);
            // SAFETY: Core Animation's constant.
            moving.setLineCap(unsafe { kCALineCapButt });
            // One dash, the segment, and a gap the rest of the length, whose
            // phase runs it round, as the path's end meets its start.
            let segment = (span.clamp(0.0, 1.0) * length).max(0.0);
            let dashes = NSArray::from_retained_slice(&[
                NSNumber::new_f64(segment),
                NSNumber::new_f64((length - segment).max(0.0)),
            ]);
            moving.setLineDashPattern(Some(&dashes));
            track.addSublayer(&moving);
            (
                Retained::into_super(moving),
                "lineDashPhase",
                (0.0, -length),
            )
        }
        SweepShape::Dots { diameter, centers } => {
            let group = CALayer::new();
            group.setFrame(CGRect::new(CGPoint::ZERO, CGSize::new(width, tall)));
            let dots = centers.map(|(x, y)| {
                let dot = CALayer::new();
                dot.setBounds(CGRect::new(CGPoint::ZERO, CGSize::new(diameter, diameter)));
                dot.setCornerRadius(diameter / 2.0);
                dot.setBackgroundColor(Some(&color));
                // From the square's top, as the layer's y goes up.
                dot.setPosition(CGPoint::new(x, tall - y));
                group.addSublayer(&dot);
                dot
            });
            track.addSublayer(&group);
            if !reduced {
                light(&dots, sweep, now);
                return within;
            }
            (group, "opacity", PULSE)
        }
    };
    let (moving, key, (from, to)) = animation;
    let pulse = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(if reduced {
        "opacity"
    } else {
        key
    })));
    let (from, to) = if reduced { PULSE } else { (from, to) };
    // SAFETY: numbers, as an animation of a position, an angle or an
    // opacity takes.
    unsafe {
        pulse.setFromValue(Some(&NSNumber::new_f64(from)));
        pulse.setToValue(Some(&NSNumber::new_f64(to)));
    }
    let period = sweep.period.as_secs_f64().max(0.05);
    pulse.setDuration(period);
    pulse.setRepeatCount(f32::INFINITY);
    pulse.setRemovedOnCompletion(false);
    if reduced {
        pulse.setAutoreverses(true);
    }
    pulse.setTimingFunction(Some(&timing(match sweep.shape {
        SweepShape::Bar { .. } | SweepShape::Dots { .. } => lokyr_platform_core::Easing::EaseInOut,
        SweepShape::Ring { .. } | SweepShape::Outline { .. } => lokyr_platform_core::Easing::Linear,
    })));
    // From the sweeps' start, as the app counts it, so that a sweep placed
    // again goes on where it was.
    let since = Instant::now().saturating_duration_since(sweep.start);
    pulse.setBeginTime(now - since.as_secs_f64());
    moving.addAnimation_forKey(&pulse, Some(&NSString::from_str(SWEEP)));
    within
}

/// Lights `dots` in turn, of `sweep`'s period, whose layers' time is `now`:
/// each fades as the next lights, a third of the round after it, from the
/// sweeps' start, so that dots placed again go on where they were.
fn light(dots: &[Retained<CALayer>; 3], sweep: &Sweep, now: f64) {
    let period = sweep.period.as_secs_f64().max(0.05);
    let since = Instant::now().saturating_duration_since(sweep.start);
    // A round earlier, which changes nothing of their phases, so that every
    // dot's light began before now, however late it comes.
    let first = now - since.as_secs_f64() - period;
    let (values, times) = DOT_LIGHT;
    for (at, dot) in dots.iter().enumerate() {
        let animation =
            CAKeyframeAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
        // SAFETY: numbers, as an animation of the opacity takes.
        unsafe { animation.setValues(Some(&opacities(&values))) };
        animation.setKeyTimes(Some(&NSArray::from_retained_slice(
            &times.map(NSNumber::new_f64),
        )));
        animation.setDuration(period);
        animation.setRepeatCount(f32::INFINITY);
        animation.setRemovedOnCompletion(false);
        animation.setBeginTime(first + period * at as f64 / 3.0);
        dot.addAnimation_forKey(&animation, Some(&NSString::from_str(SWEEP)));
    }
}

/// The path of an outline of a box `width` by `height` points, of corners
/// of `radius`, along the middle of a line `thickness` thick inside its
/// edges, clockwise from the middle of its left edge, and its length.
pub(crate) fn outline_path(
    width: f64,
    height: f64,
    radius: f64,
    thickness: f64,
) -> (CFRetained<CGMutablePath>, f64) {
    let inset = thickness / 2.0;
    let (w, h) = ((width - thickness).max(0.0), (height - thickness).max(0.0));
    let r = (radius - inset).clamp(0.0, w.min(h) / 2.0);
    let (left, bottom, right, top) = (inset, inset, inset + w, inset + h);
    let path = CGMutablePath::new();
    // Clockwise, as the layer's y goes up: up the left edge first.
    // SAFETY: a path of the call's own, and no transform.
    unsafe {
        let none = std::ptr::null();
        CGMutablePath::move_to_point(Some(&path), none, left, bottom + h / 2.0);
        CGMutablePath::add_line_to_point(Some(&path), none, left, top - r);
        CGMutablePath::add_arc(
            Some(&path),
            none,
            left + r,
            top - r,
            r,
            std::f64::consts::PI,
            FRAC_PI_2,
            true,
        );
        CGMutablePath::add_line_to_point(Some(&path), none, right - r, top);
        CGMutablePath::add_arc(
            Some(&path),
            none,
            right - r,
            top - r,
            r,
            FRAC_PI_2,
            0.0,
            true,
        );
        CGMutablePath::add_line_to_point(Some(&path), none, right, bottom + r);
        CGMutablePath::add_arc(
            Some(&path),
            none,
            right - r,
            bottom + r,
            r,
            0.0,
            -FRAC_PI_2,
            true,
        );
        CGMutablePath::add_line_to_point(Some(&path), none, left + r, bottom);
        CGMutablePath::add_arc(
            Some(&path),
            none,
            left + r,
            bottom + r,
            r,
            -FRAC_PI_2,
            -std::f64::consts::PI,
            true,
        );
        CGMutablePath::close_subpath(Some(&path));
    }
    let length = 2.0 * (w - 2.0 * r) + 2.0 * (h - 2.0 * r) + TAU * r;
    (path, length.max(0.0))
}

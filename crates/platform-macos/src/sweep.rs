//! The sweeps of a window: the parts of its indeterminate progresses that
//! the window server moves without the app's frames, on layers of a view
//! over the content's, under the layers of the text that blinks and of the
//! cursor: a segment across a bar, an arc round a ring, each within the
//! part of the window where its progress shows. Where the system reduces
//! motion, the segment holds in the bar's middle and the arc at the ring's
//! top, their opacity pulsing.

use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::time::Instant;

use lokyr_platform_core::{Point, Size, Sweep, SweepShape};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSResponder, NSView, NSWorkspace};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_foundation::{NSNumber, NSObject, NSPoint, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CACurrentMediaTime, CALayer, CAMediaTiming, CAShapeLayer, CATransaction,
    kCALineCapButt,
};

use crate::layer::timing;

/// The key of a sweep's animation on its layer.
const SWEEP: &str = "sweep";

/// The opacities between which a sweep pulses where the system reduces
/// motion.
const PULSE: (f64, f64) = (1.0, 0.35);

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

/// The layer of `sweep`, over a view `height` points high, whose time is
/// `now`: the part of the window where its progress shows, which clips its
/// bar or its ring, which clips what moves.
fn sweep_layer(sweep: &Sweep, height: f64, (reduced, now): (bool, f64)) -> Retained<CALayer> {
    // A layer's origin is its superlayer's bottom left corner.
    let up = |origin: Point, size: Size, from: (f64, f64)| {
        CGRect::new(
            CGPoint::new(origin.x - from.0, from.1 - origin.y - size.height),
            CGSize::new(size.width, size.height),
        )
    };
    let (within_origin, within_size) = sweep.within;
    let within = CALayer::new();
    within.setFrame(up(within_origin, within_size, (0.0, height)));
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
        SweepShape::Bar { .. } => lokyr_platform_core::Easing::EaseInOut,
        SweepShape::Ring { .. } => lokyr_platform_core::Easing::Linear,
    })));
    // From the sweeps' start, as the app counts it, so that a sweep placed
    // again goes on where it was.
    let since = Instant::now().saturating_duration_since(sweep.start);
    pulse.setBeginTime(now - since.as_secs_f64());
    moving.addAnimation_forKey(&pulse, Some(&NSString::from_str(SWEEP)));
    within
}

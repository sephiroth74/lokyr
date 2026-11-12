//! The fills of a window's progresses that are not indeterminate, on layers
//! of a view over the content's, under the sweeps' and the layers of the
//! text that blinks and of the cursor, which the window server draws as far
//! as their values: a bar's from its left, a ring's clockwise from its top,
//! an outline's from its left along its top and its bottom at once. A change
//! of a value slides from the value that shows, eased, with no frame of the
//! app's; at once where the system reduces motion.

use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::time::Duration;

use lokyr_platform_core::{Easing, Fill, FillShape};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSResponder, NSView, NSWorkspace};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_foundation::{NSNumber, NSObject, NSPoint, NSSize, NSString};
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CAShapeLayer, CATransaction, kCALineCapButt,
};

use crate::layer::timing;
use crate::sweep::{outline_path, up};

/// How long a value's slide takes.
const SLIDE: Duration = Duration::from_millis(200);

/// The key of a slide's animation on its layer.
const SLIDING: &str = "slide";

/// A fill that the view shows: the fill as the app gave it, the layer of
/// the part of the window where it shows, and the layer that its value
/// moves.
struct Shown {
    fill: Fill,
    within: Retained<CALayer>,
    moving: Moving,
}

/// What a fill's value moves: the width of a layer from its left, a bar's
/// or what shows of an outline; or the end of a ring's stroke.
enum Moving {
    Width(Retained<CALayer>),
    Stroke(Retained<CAShapeLayer>),
}

pub(crate) struct FillsViewIvars {
    /// The fills that its layers show, in their order, and the view's
    /// height as they were placed.
    shown: RefCell<(Vec<Shown>, f64)>,
}

define_class!(
    // SAFETY: `NSView` has no requirement of its subclasses that this one
    // breaks, and the view does not implement `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = FillsViewIvars]
    pub(crate) struct FillsView;

    impl FillsView {
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

impl FillsView {
    /// A view of no fills over the content view `over`, as large, over its
    /// other views so far.
    pub(crate) fn new(mtm: MainThreadMarker, over: &NSView) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FillsViewIvars {
            shown: RefCell::new((Vec::new(), 0.0)),
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

    /// Places the fills that it shows again at their distance from the
    /// view's top, as a resize changed its height, which a layer's origin
    /// counts from its bottom.
    fn place(&self) {
        let height = self.bounds().size.height;
        let mut shown = self.ivars().shown.borrow_mut();
        if shown.1 == height {
            return;
        }
        shown.1 = height;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        for fill in &shown.0 {
            fill.within.setFrame(within_frame(&fill.fill, height));
        }
        CATransaction::commit();
    }

    /// Shows `fills`, in place of those that it showed: one of an id that
    /// it shows, of the same box, keeps its layers, and slides from the
    /// value that shows to its new one.
    pub(crate) fn set(&self, fills: &[Fill]) {
        let Some(layer) = self.layer() else {
            return;
        };
        let height = self.bounds().size.height;
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        let mut shown = self.ivars().shown.borrow_mut();
        let mut before = std::mem::take(&mut shown.0);
        // No implicit animation of the layers that come, go and move.
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        let mut next = Vec::with_capacity(fills.len());
        for fill in fills {
            let kept = before
                .iter()
                .position(|was| was.fill.id == fill.id && same_box(&was.fill, fill));
            match kept {
                Some(at) => {
                    let was = before.remove(at);
                    if was.fill.value != fill.value {
                        slide(&was.moving, fill, !reduced);
                    }
                    next.push(Shown { fill: *fill, ..was });
                }
                None => {
                    let (within, moving) = fill_layers(fill, height);
                    layer.addSublayer(&within);
                    next.push(Shown {
                        fill: *fill,
                        within,
                        moving,
                    });
                }
            }
        }
        for gone in before {
            gone.within.removeFromSuperlayer();
        }
        CATransaction::commit();
        *shown = (next, height);
    }
}

/// Whether `one` and `other` fill the same box alike, but for their
/// values.
fn same_box(one: &Fill, other: &Fill) -> bool {
    Fill {
        value: other.value,
        ..*one
    } == *other
}

/// The frame of the layer of the part of the window where `fill` shows, in
/// a view `height` points high.
fn within_frame(fill: &Fill, height: f64) -> CGRect {
    let (origin, size) = fill.within;
    up(origin, size, (0.0, height))
}

/// The layers of `fill`, over a view `height` points high: the part of the
/// window where it shows, which clips it, and what its value moves, as far
/// as its value.
fn fill_layers(fill: &Fill, height: f64) -> (Retained<CALayer>, Moving) {
    let (within_origin, within_size) = fill.within;
    let within = CALayer::new();
    within.setFrame(within_frame(fill, height));
    within.setMasksToBounds(true);
    let boxed = CALayer::new();
    boxed.setFrame(up(
        fill.origin,
        fill.size,
        (within_origin.x, within_origin.y + within_size.height),
    ));
    within.addSublayer(&boxed);
    let [red, green, blue, alpha] = fill.color;
    let color = CGColor::new_srgb(red, green, blue, alpha);
    let (width, tall) = (fill.size.width, fill.size.height);
    let value = fill.value.clamp(0.0, 1.0);
    let moving = match fill.shape {
        FillShape::Bar => {
            let bar = from_left(width * value, tall);
            bar.setCornerRadius(fill.radius.min(tall / 2.0));
            bar.setBackgroundColor(Some(&color));
            boxed.addSublayer(&bar);
            Moving::Width(bar)
        }
        FillShape::Outline { thickness } => {
            // The whole line, within a part from the left as wide as the
            // value.
            let shows = from_left(width * value, tall);
            shows.setMasksToBounds(true);
            let line = CAShapeLayer::new();
            line.setFrame(CGRect::new(CGPoint::ZERO, CGSize::new(width, tall)));
            let (path, _) = outline_path(width, tall, fill.radius, thickness);
            line.setPath(Some(&path));
            line.setFillColor(None);
            line.setStrokeColor(Some(&color));
            line.setLineWidth(thickness);
            shows.addSublayer(&line);
            boxed.addSublayer(&shows);
            Moving::Width(shows)
        }
        FillShape::Ring { thickness } => {
            let side = width.min(tall);
            let ring = CAShapeLayer::new();
            ring.setFrame(CGRect::new(
                CGPoint::new((width - side) / 2.0, (tall - side) / 2.0),
                CGSize::new(side, side),
            ));
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
                    FRAC_PI_2 - TAU,
                    true,
                );
            }
            ring.setPath(Some(&path));
            ring.setFillColor(None);
            ring.setStrokeColor(Some(&color));
            ring.setLineWidth(thickness);
            // SAFETY: Core Animation's constant.
            ring.setLineCap(unsafe { kCALineCapButt });
            ring.setStrokeEnd(value);
            boxed.addSublayer(&ring);
            Moving::Stroke(ring)
        }
    };
    (within, moving)
}

/// A layer `width` points wide and `tall` high at its box's left, which a
/// change of its width grows or shrinks from its left.
fn from_left(width: f64, tall: f64) -> Retained<CALayer> {
    let layer = CALayer::new();
    layer.setAnchorPoint(CGPoint::new(0.0, 0.5));
    layer.setBounds(CGRect::new(CGPoint::ZERO, CGSize::new(width, tall)));
    layer.setPosition(CGPoint::new(0.0, tall / 2.0));
    layer
}

/// Moves `moving` to `fill`'s value: sliding, eased, from where it shows,
/// as `slides` says, or at once.
fn slide(moving: &Moving, fill: &Fill, slides: bool) {
    let value = fill.value.clamp(0.0, 1.0);
    let (layer, key, from, to): (&CALayer, &str, f64, f64) = match moving {
        Moving::Width(layer) => {
            // SAFETY: the layer's presentation, a copy as it shows now.
            let shown = unsafe { layer.presentationLayer() }.unwrap_or_else(|| layer.clone());
            let to = fill.size.width * value;
            let mut bounds = layer.bounds();
            let from = shown.bounds().size.width;
            bounds.size.width = to;
            layer.setBounds(bounds);
            (layer, "bounds.size.width", from, to)
        }
        Moving::Stroke(ring) => {
            // SAFETY: the layer's presentation, a copy as it shows now.
            let shown = unsafe { ring.presentationLayer() }
                .and_then(|shown| shown.downcast::<CAShapeLayer>().ok())
                .unwrap_or_else(|| ring.clone());
            let from = shown.strokeEnd();
            ring.setStrokeEnd(value);
            (ring, "strokeEnd", from, value)
        }
    };
    if !slides || from == to {
        layer.removeAnimationForKey(&NSString::from_str(SLIDING));
        return;
    }
    let animation = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str(key)));
    // SAFETY: numbers, as an animation of a width or of a stroke's end
    // takes.
    unsafe {
        animation.setFromValue(Some(&NSNumber::new_f64(from)));
        animation.setToValue(Some(&NSNumber::new_f64(to)));
    }
    animation.setDuration(SLIDE.as_secs_f64());
    animation.setTimingFunction(Some(&timing(Easing::EaseOut)));
    layer.addAnimation_forKey(&animation, Some(&NSString::from_str(SLIDING)));
}

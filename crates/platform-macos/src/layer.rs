//! A layer of a window: a view over the content's, of the cursor's cells or
//! of the text that blinks, which the renderer draws into, whose opacity the
//! window server animates as it blinks, so that the blinking and its fades
//! wake the app no more.

use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use lokyr_platform_core::{Blink, Easing, LayerPlace};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSResponder, NSView, NSWorkspace};
use objc2_foundation::{NSArray, NSNumber, NSObject, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{
    CACurrentMediaTime, CAKeyframeAnimation, CALayer, CAMediaTiming, CAMediaTimingFunction,
    CAMetalLayer, kCAAnimationDiscrete, kCAMediaTimingFunctionEaseIn,
    kCAMediaTimingFunctionEaseInEaseOut, kCAMediaTimingFunctionEaseOut,
    kCAMediaTimingFunctionLinear,
};

/// The key of the blinking's animation on the layer.
const BLINK: &str = "blink";

/// The shortest cycle that blinks: a shorter one, as of nothing but zeros,
/// leaves the layer on.
const SHORTEST: Duration = Duration::from_millis(50);

pub(crate) struct LayerViewIvars {
    /// The blink that its layer runs, as the app last gave it.
    blink: Cell<Option<Blink>>,
}

define_class!(
    // SAFETY: `NSView` has no requirement of its subclasses that this one
    // breaks, and the view does not implement `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = LayerViewIvars]
    pub(crate) struct LayerView;

    impl LayerView {
        // SAFETY: AppKit's signature: the layer that backs the view, clear
        // where the renderer draws nothing.
        #[unsafe(method_id(makeBackingLayer))]
        fn make_backing_layer(&self) -> Retained<CALayer> {
            let layer = CAMetalLayer::new();
            layer.setOpaque(false);
            Retained::into_super(layer)
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            // The renderer draws the layer; AppKit draws nothing into it.
            true
        }

        // SAFETY: AppKit's signature: clicks go through, to the content's
        // view.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            // SAFETY: `NSView`'s method, of this signature.
            let () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
            if let (Some(window), Some(layer)) = (self.window(), self.layer()) {
                layer.setContentsScale(window.backingScaleFactor());
            }
        }
    }
);

impl LayerView {
    /// A hidden view of a layer, for the content view `over`, over its
    /// other views.
    pub(crate) fn new(mtm: MainThreadMarker, over: &NSView) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(LayerViewIvars {
            blink: Cell::new(None),
        });
        let frame = NSRect::new(NSPoint::ZERO, NSSize::new(1.0, 1.0));
        // SAFETY: `NSView`'s `initWithFrame:` has this signature.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setWantsLayer(true);
        view.setHidden(true);
        // It keeps its distance from the content's top, where the app
        // places it from, as the window resizes: AppKit's origin is the
        // bottom left corner.
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        over.addSubview(&view);
        view
    }

    /// A view of a layer that fills its window, as its content view, and
    /// shows, which the window's whole content is.
    pub(crate) fn filling(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(LayerViewIvars {
            blink: Cell::new(None),
        });
        // SAFETY: `NSView`'s `initWithFrame:` has this signature.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setWantsLayer(true);
        view.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        view
    }

    /// The view, by its address, for a renderer: it lives as long as the
    /// window.
    pub(crate) fn address(&self) -> NonNull<c_void> {
        NonNull::from(self).cast()
    }

    /// Places the view at `place`, over the content view whose height is
    /// `height` points, blinking as it says; `None` hides it.
    pub(crate) fn place(&self, place: Option<&LayerPlace>, height: f64) {
        let Some(place) = place else {
            self.setHidden(true);
            self.blink(None);
            return;
        };
        // The content view's origin is its bottom left corner.
        let frame = NSRect::new(
            NSPoint::new(place.origin.x, height - place.origin.y - place.size.height),
            NSSize::new(place.size.width, place.size.height),
        );
        if self.frame() != frame {
            self.setFrame(frame);
        }
        self.setHidden(false);
        self.blink(place.blink);
    }

    /// Runs `blink` on the layer, in place of the one that runs, unless it
    /// is the same; `None` shows the layer, steady.
    fn blink(&self, blink: Option<Blink>) {
        if self.ivars().blink.replace(blink) == blink {
            return;
        }
        let Some(layer) = self.layer() else {
            return;
        };
        layer.removeAnimationForKey(&NSString::from_str(BLINK));
        let Some(blink) = blink else {
            return;
        };
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        if let Some(animation) = animation(&blink, reduced) {
            // From the blink's start, as the app counts it: the layer's time
            // goes back by what passed since.
            let since = Instant::now().saturating_duration_since(blink.start);
            let now = layer.convertTime_fromLayer(CACurrentMediaTime(), None);
            animation.setBeginTime(now - since.as_secs_f64());
            layer.addAnimation_forKey(&animation, Some(&NSString::from_str(BLINK)));
        }
    }
}

/// The animation of `blink`'s opacity, a cycle that repeats for ever: held
/// visible, faded out, held hidden and faded in, of the fades' curves; or,
/// without fades, or where the system reduces motion, switched, at each
/// fade's middle. `None` for a cycle too short to blink.
fn animation(blink: &Blink, reduced: bool) -> Option<Retained<CAKeyframeAnimation>> {
    let cycle = blink.visible + blink.fade_out + blink.hidden + blink.fade_in;
    if cycle < SHORTEST {
        return None;
    }
    let total = cycle.as_secs_f64();
    let at = |time: Duration| NSNumber::new_f64(time.as_secs_f64() / total);
    let animation = CAKeyframeAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
    let switched = reduced || (blink.fade_out.is_zero() && blink.fade_in.is_zero());
    if switched {
        // The value of each step holds until the next key time.
        let off = blink.visible + blink.fade_out / 2;
        let on = blink.visible + blink.fade_out + blink.hidden + blink.fade_in / 2;
        // SAFETY: numbers, as an animation of the opacity takes.
        unsafe { animation.setValues(Some(&opacities(&[1.0, 0.0, 1.0]))) };
        let times = [Duration::ZERO, off, on, cycle].map(at);
        animation.setKeyTimes(Some(&NSArray::from_retained_slice(&times)));
        // SAFETY: Core Animation's constant.
        animation.setCalculationMode(unsafe { kCAAnimationDiscrete });
    } else {
        // SAFETY: numbers, as an animation of the opacity takes.
        unsafe { animation.setValues(Some(&opacities(&[1.0, 1.0, 0.0, 0.0, 1.0]))) };
        let held = blink.visible;
        let out = held + blink.fade_out;
        let hidden = out + blink.hidden;
        let times = [Duration::ZERO, held, out, hidden, cycle].map(at);
        animation.setKeyTimes(Some(&NSArray::from_retained_slice(&times)));
        let curves = [
            Easing::Linear,
            blink.fade_out_curve,
            Easing::Linear,
            blink.fade_in_curve,
        ]
        .map(timing);
        animation.setTimingFunctions(Some(&NSArray::from_retained_slice(&curves)));
    }
    animation.setDuration(total);
    animation.setRepeatCount(f32::INFINITY);
    animation.setRemovedOnCompletion(false);
    Some(animation)
}

/// The keyframes' values of `opacities`, as objects.
fn opacities(opacities: &[f64]) -> Retained<NSArray> {
    let values: Vec<Retained<AnyObject>> = opacities
        .iter()
        .map(|&opacity| {
            Retained::into_super(Retained::into_super(Retained::into_super(
                NSNumber::new_f64(opacity),
            )))
        })
        .collect();
    NSArray::from_retained_slice(&values)
}

/// Core Animation's function of `easing`.
pub(crate) fn timing(easing: Easing) -> Retained<CAMediaTimingFunction> {
    // SAFETY: Core Animation's constants.
    let name = unsafe {
        match easing {
            Easing::Linear => kCAMediaTimingFunctionLinear,
            Easing::EaseIn => kCAMediaTimingFunctionEaseIn,
            Easing::EaseOut => kCAMediaTimingFunctionEaseOut,
            Easing::EaseInOut => kCAMediaTimingFunctionEaseInEaseOut,
        }
    };
    CAMediaTimingFunction::functionWithName(name)
}

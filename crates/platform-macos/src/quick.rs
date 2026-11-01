//! Quick windows: panels without a titlebar that slide in from an edge of a
//! screen over every app's windows, those in full screen too, on every
//! Space or on the one that they showed on, and out again, giving the focus
//! back to the app that had it.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use block2::RcBlock;
use lokyr_platform_core::{Easing, QuickPlace, QuickScreen, ScreenRect};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSApplication,
    NSApplicationActivationOptions, NSBackingStoreType, NSEvent, NSFloatingWindowLevel, NSPanel,
    NSPopUpMenuWindowLevel, NSResponder, NSRunningApplication, NSScreen, NSWindow,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize};

use crate::layer;

/// The size of a quick window's content before it first shows.
const FIRST_SIZE: NSSize = NSSize {
    width: 800.0,
    height: 400.0,
};

define_class!(
    // SAFETY: `NSPanel` has no requirement of its subclasses that this one
    // breaks, and the panel does not implement `Drop`.
    #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    struct QuickPanel;

    impl QuickPanel {
        // SAFETY: AppKit's signature: a panel without a titlebar takes the
        // keys all the same.
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key_window(&self) -> bool {
            true
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main_window(&self) -> bool {
            true
        }
    }
);

/// A quick window's panel, hidden, and its content's rectangle: one that
/// takes the keys without making the app active as it is clicked, which
/// stays as the app is not, and which the Window menu and the cycle of the
/// windows leave out.
pub(crate) fn panel(mtm: MainThreadMarker) -> (Retained<NSWindow>, NSRect) {
    let content = NSRect::new(NSPoint::ZERO, FIRST_SIZE);
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    // SAFETY: a new panel, of a valid style, which `setReleasedWhenClosed`
    // below keeps from being released twice.
    let panel: Retained<QuickPanel> = unsafe {
        objc2::msg_send![
            QuickPanel::alloc(mtm),
            initWithContentRect: content,
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false,
        ]
    };
    let panel: Retained<NSPanel> = Retained::into_super(panel);
    panel.setFloatingPanel(true);
    panel.setBecomesKeyOnlyIfNeeded(false);
    // SAFETY: `Retained` owns the panel, which closing must then not
    // release.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setHidesOnDeactivate(false);
    panel.setExcludedFromWindowsMenu(true);
    panel.setHasShadow(true);
    // Its own slide, rather than the system's fade as it orders in.
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    (Retained::into_super(panel), content)
}

/// What a quick window keeps of its showing.
#[derive(Default)]
pub(crate) struct Quick {
    /// Whether it shows, or slides in.
    shown: Cell<bool>,
    /// The number of its last slide, in or out, which the end of a slide
    /// that another followed leaves alone.
    turn: Cell<u64>,
    /// Where it showed last.
    place: Cell<Option<QuickPlace>>,
    /// The app that had the focus as it showed, which takes it back as it
    /// hides.
    previous: RefCell<Option<Retained<NSRunningApplication>>>,
}

impl Quick {
    /// Slides `window` in as `place` says, if it is hidden, and gives it
    /// the focus, which the app that has it gives back as it hides; or,
    /// shown, gives it the focus again. Whether it was hidden.
    pub(crate) fn show(self: &Rc<Self>, window: &Retained<NSWindow>, place: &QuickPlace) -> bool {
        let mtm = window.mtm();
        let app = NSApplication::sharedApplication(mtm);
        // Shown from Lokyr itself, it gives the focus back to Lokyr's
        // window that takes it next, not to an app before.
        *self.previous.borrow_mut() = if app.isActive() {
            None
        } else {
            let ours = NSRunningApplication::currentApplication();
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .filter(|front| !front.isEqual(Some(&ours)))
        };
        let hidden = !self.shown.replace(true);
        if hidden {
            let turn = self.next_turn();
            self.place.set(Some(*place));
            let (shown, from, fades) = match screen(place.screen, mtm) {
                Some(screen) => frames(place, &screen),
                None => (window.frame(), window.frame(), true),
            };
            let spaces = if place.every_space {
                NSWindowCollectionBehavior::CanJoinAllSpaces
            } else {
                NSWindowCollectionBehavior::MoveToActiveSpace
            };
            window.setCollectionBehavior(
                spaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            // Over the menu bar and the Dock as it slides, which it passes.
            window.setLevel(NSPopUpMenuWindowLevel);
            window.setAlphaValue(if fades { 0.0 } else { 1.0 });
            window.setFrame_display(from, false);
            window.makeKeyAndOrderFront(None);
            let this = Rc::clone(self);
            let ended = window.clone();
            slide(
                window,
                duration(place.animation),
                true,
                move |animator| {
                    animator.setFrame_display(shown, true);
                    if fades {
                        animator.setAlphaValue(1.0);
                    }
                },
                move || {
                    // Under the menus of the menu bar, and the input
                    // method's candidates, once it is in.
                    if this.turn.get() == turn {
                        ended.setLevel(NSFloatingWindowLevel);
                    }
                },
            );
        } else {
            window.makeKeyAndOrderFront(None);
        }
        // An application that is not active comes forward only when it
        // asks, and the cooperative `activate` waits for the active one to
        // yield, which the one in front does not as a global key shows the
        // window.
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
        hidden
    }

    /// Slides `window` out, if it shows, and gives the focus back to the
    /// app that had it, if the window still has it: not when the user
    /// went to another window or app meanwhile. Whether it showed.
    pub(crate) fn hide(self: &Rc<Self>, window: &Retained<NSWindow>) -> bool {
        if !self.shown.replace(false) {
            return false;
        }
        let turn = self.next_turn();
        self.give_focus_back(window);
        let place = self.place.get().unwrap_or_default();
        let (to, fades) = match window
            .screen()
            .or_else(|| screen(place.screen, window.mtm()))
        {
            Some(screen) => {
                let (_, to, fades) = frames(&place, &screen);
                (to, fades)
            }
            None => (window.frame(), true),
        };
        // On another Space, where it does not show, it goes at once.
        let time = if window.isOnActiveSpace() {
            duration(place.animation)
        } else {
            Duration::ZERO
        };
        window.setLevel(NSPopUpMenuWindowLevel);
        let this = Rc::clone(self);
        let ended = window.clone();
        slide(
            window,
            time,
            false,
            move |animator| {
                if fades {
                    animator.setAlphaValue(0.0);
                } else {
                    animator.setFrame_display(to, true);
                }
            },
            move || {
                if this.turn.get() == turn {
                    ended.orderOut(None);
                    ended.setAlphaValue(1.0);
                }
            },
        );
        true
    }

    /// Gives the focus back to the app that had it as `window` showed, if
    /// the window still has it, as it hides or closes.
    pub(crate) fn give_focus_back(&self, window: &NSWindow) {
        let previous = self.previous.take();
        let app = NSApplication::sharedApplication(window.mtm());
        if !window.isKeyWindow() || !app.isActive() {
            return;
        }
        if let Some(previous) = previous.filter(|previous| !previous.isTerminated()) {
            previous.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }

    /// Whether it shows, or slides in.
    pub(crate) fn shows(&self) -> bool {
        self.shown.get()
    }

    fn next_turn(&self) -> u64 {
        let turn = self.turn.get() + 1;
        self.turn.set(turn);
        turn
    }
}

/// How long a slide of `animation` takes: at once with the system's Reduce
/// motion.
fn duration(animation: Duration) -> Duration {
    if NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion() {
        Duration::ZERO
    } else {
        animation
    }
}

/// Animates what `change` does to `window`'s animator over `time`, slow at
/// its end as it slides in, and at its start as it slides out, then calls
/// `ended`; at once, and `ended` with it, for no time.
fn slide(
    window: &NSWindow,
    time: Duration,
    slides_in: bool,
    change: impl Fn(&NSWindow) + 'static,
    ended: impl Fn() + 'static,
) {
    if time.is_zero() {
        change(window);
        ended();
        return;
    }
    let animator = window.animator();
    let changes = RcBlock::new(move |context: NonNull<NSAnimationContext>| {
        // SAFETY: the context that AppKit gives the block, for its call.
        let context = unsafe { context.as_ref() };
        context.setDuration(time.as_secs_f64());
        let easing = if slides_in {
            Easing::EaseOut
        } else {
            Easing::EaseIn
        };
        context.setTimingFunction(Some(&layer::timing(easing)));
        change(&animator);
    });
    let ended = RcBlock::new(ended);
    NSAnimationContext::runAnimationGroup_completionHandler(&changes, Some(&ended));
}

/// The screen that `choice` names, or the main one; `None` without a
/// screen.
fn screen(choice: QuickScreen, mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm);
    let chosen = match choice {
        QuickScreen::Main => NSScreen::mainScreen(mtm),
        QuickScreen::Pointer => {
            let at = NSEvent::mouseLocation();
            (0..screens.count())
                .map(|index| screens.objectAtIndex(index))
                .find(|screen| contains(screen.frame(), at))
        }
        QuickScreen::MenuBar => screens.firstObject(),
    };
    chosen
        .or_else(|| NSScreen::mainScreen(mtm))
        .or_else(|| screens.firstObject())
}

fn contains(rect: NSRect, point: NSPoint) -> bool {
    point.x >= rect.origin.x
        && point.x < rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y < rect.origin.y + rect.size.height
}

/// The frames of a quick window of `place` on `screen`, where it shows and
/// where it comes from, as AppKit places windows, and whether it fades
/// rather than slides, as the other screens tell: one that fades comes from
/// where it shows.
fn frames(place: &QuickPlace, screen: &NSScreen) -> (NSRect, NSRect, bool) {
    let screens = NSScreen::screens(screen.mtm());
    let top = screens
        .firstObject()
        .map_or(screen.frame(), |first| first.frame());
    let top = top.origin.y + top.size.height;
    let others: Vec<ScreenRect> = (0..screens.count())
        .map(|index| screens.objectAtIndex(index))
        .filter(|other| !other.isEqual(Some(screen)))
        .map(|other| on_screens(other.frame(), top))
        .collect();
    let (shown, from, fades) = place.way(
        on_screens(screen.frame(), top),
        on_screens(screen.visibleFrame(), top),
        &others,
    );
    (in_appkit(shown, top), in_appkit(from, top), fades)
}

/// `rect`, of AppKit's coordinates, whose origin is the bottom left corner
/// of the menu bar's screen, upwards, on the screens' plane, downwards from
/// that screen's top, `top` in AppKit's.
fn on_screens(rect: NSRect, top: f64) -> ScreenRect {
    ScreenRect {
        x: rect.origin.x,
        y: top - rect.origin.y - rect.size.height,
        width: rect.size.width,
        height: rect.size.height,
    }
}

/// `rect`, of the screens' plane, in AppKit's coordinates.
fn in_appkit(rect: ScreenRect, top: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(rect.x, top - rect.y - rect.height),
        NSSize::new(rect.width, rect.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_goes_between_appkit_s_coordinates_and_the_screens_plane() {
        // A screen of 900 points high, whose menu bar takes 25: its area
        // for windows, in AppKit's coordinates, from the bottom, and on the
        // plane, from the top.
        let area = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 875.0));
        let plane = on_screens(area, 900.0);
        assert_eq!(
            plane,
            ScreenRect {
                x: 0.0,
                y: 25.0,
                width: 1440.0,
                height: 875.0
            }
        );
        assert_eq!(in_appkit(plane, 900.0), area);
        // A screen above it, past the plane's top.
        let above = NSRect::new(NSPoint::new(100.0, 900.0), NSSize::new(1920.0, 1080.0));
        assert_eq!(on_screens(above, 900.0).y, -1080.0);
        assert_eq!(in_appkit(on_screens(above, 900.0), 900.0), above);
    }
}

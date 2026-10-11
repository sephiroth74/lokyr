//! The AppKit side of the spike: one window, an application menu whose Quit
//! item answers to ⌘Q, and termination when the last window closes.

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSMenu, NSMenuItem, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, ns_string,
};

#[derive(Debug, Default)]
struct Ivars {
    window: OnceCell<Retained<NSWindow>>,
}

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, and `AppDelegate`
    // does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct AppDelegate;

    // SAFETY: `NSObjectProtocol` has no safety requirements.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: `NSApplicationDelegate` has no safety requirements.
    unsafe impl NSApplicationDelegate for AppDelegate {
        // SAFETY: the signature matches `applicationDidFinishLaunching:`.
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            app.setMainMenu(Some(&main_menu(mtm)));
            let window = main_window(mtm);
            window.makeKeyAndOrderFront(None);
            assert!(
                self.ivars().window.set(window).is_ok(),
                "the app finishes launching only once"
            );
            app.activate();
        }

        // SAFETY: the signature matches
        // `applicationShouldTerminateAfterLastWindowClosed:`.
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: `NSObject`'s `init` takes no arguments and returns the
        // initialized object.
        unsafe { msg_send![super(this), init] }
    }
}

/// The menu bar: an application menu with a single Quit item (⌘Q).
fn main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    // SAFETY: `terminate:` is an action that `NSApplication` responds to.
    let quit = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            ns_string!("Quit Spike"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        )
    };
    let app_menu = NSMenu::new(mtm);
    app_menu.addItem(&quit);

    let app_menu_item = NSMenuItem::new(mtm);
    app_menu_item.setSubmenu(Some(&app_menu));

    let menu_bar = NSMenu::new(mtm);
    menu_bar.addItem(&app_menu_item);
    menu_bar
}

/// A resizable window, centered on the screen.
fn main_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(640.0, 400.0));
    // SAFETY: a freshly allocated window initialized with a valid frame,
    // style and backing store.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the delegate keeps the window alive, so AppKit must not
    // release it when it closes.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(ns_string!("Saetta spike"));
    window.center();
    window
}

/// Runs the app until it quits.
pub fn run() {
    let mtm = MainThreadMarker::new().expect("the app runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}

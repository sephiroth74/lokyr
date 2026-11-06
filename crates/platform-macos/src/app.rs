//! The application, its delegate, and its run loop.

use std::panic;
use std::rc::{Rc, Weak};

use lokyr_platform_core::{AppEvent, Error, Handler};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
    NSApplicationTerminateReply, NSWindow,
};
use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol};

use crate::appearance::AppearanceObserver;
use crate::shared::{Pending, Shared};
use crate::wake::WakeSource;

/// Starts the application, as a regular one, with a Dock icon and a menu
/// bar, calls [`Handler::started`] once it has launched, and hands the
/// handler every event until it calls
/// [`Platform::quit`](lokyr_platform_core::Platform::quit). Returns once the
/// handler, then every window, are gone. A panic of the handler ends the
/// application, and goes on from here.
///
/// It must be called from the process's main thread, once.
pub fn run(handler: impl Handler + 'static) -> Result<(), Error> {
    run_with(move || handler)
}

/// Starts the application as [`run`] does, with the handler that `make`
/// gives once the application has launched, before [`Handler::started`]:
/// what the caller starts beside the launch, as on a thread of its own,
/// `make` can wait for. A panic of `make` ends the application, as the
/// handler's does.
pub fn run_with<H: Handler + 'static>(make: impl FnOnce() -> H + 'static) -> Result<(), Error> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| Error::Start("the application must run on the main thread".to_owned()))?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    // The windows are no tabs of the system's, nor does its Window menu
    // offer them.
    NSWindow::setAllowsAutomaticWindowTabbing(false, mtm);

    let wake = WakeSource::install()?;
    let make = Box::new(move || Box::new(make()) as Box<dyn Handler>);
    let shared = Shared::new(mtm, make, wake.waker());
    wake.set_target(&shared);
    let delegate = AppDelegate::new(mtm, Rc::downgrade(&shared));
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    let appearance = AppearanceObserver::start(mtm, Rc::downgrade(&shared));

    app.run();

    appearance.stop();
    app.setDelegate(None);
    drop(wake);
    if let Some(payload) = shared.finish() {
        panic::resume_unwind(payload);
    }
    Ok(())
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // delegate does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Weak<Shared>]
    struct AppDelegate;

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for AppDelegate {}

    // SAFETY: `NSApplicationDelegate` has no requirement; each method has
    // AppKit's signature.
    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn application_did_finish_launching(&self, _notification: &NSNotification) {
            // An application that starts from a terminal, rather than from
            // its bundle, comes forward only when it asks, and the
            // cooperative `activate` waits for the active application to
            // yield, which a terminal does not.
            #[allow(deprecated)]
            NSApplication::sharedApplication(self.mtm()).activateIgnoringOtherApps(true);
            self.post(Pending::Started);
        }

        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn application_should_handle_reopen(
            &self,
            sender: &NSApplication,
            visible: Bool,
        ) -> Bool {
            // AppKit brings a minimized window back; without one, the app
            // opens a window.
            let windows = sender.windows();
            let minimized =
                (0..windows.count()).any(|index| windows.objectAtIndex(index).isMiniaturized());
            if visible.as_bool() || minimized {
                return Bool::YES;
            }
            self.post(Pending::App(AppEvent::Reopen));
            Bool::NO
        }

        #[unsafe(method(applicationShouldTerminate:))]
        fn application_should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            // The app decides, as its user may answer a question first:
            // AppKit waits, and the system with it, as a logout does,
            // until the app quits or refuses.
            if let Some(shared) = self.ivars().upgrade() {
                shared.begin_terminating();
                shared.post(Pending::App(AppEvent::QuitRequested));
            }
            NSApplicationTerminateReply::TerminateLater
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker, shared: Weak<Shared>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(shared);
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    fn post(&self, pending: Pending) {
        if let Some(shared) = self.ivars().upgrade() {
            shared.post(pending);
        }
    }
}

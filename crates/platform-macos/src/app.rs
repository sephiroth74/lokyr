//! The application, its delegate, and its run loop.

use std::cell::RefCell;
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
use objc2_foundation::{NSArray, NSNotification, NSObject, NSObjectProtocol, NSString, NSURL};

use crate::appearance::AppearanceObserver;
use crate::services::ServicesProvider;
use crate::shared::{Pending, Shared};
use crate::wake::WakeSource;

thread_local! {
    /// The running application's shared state, on the main thread, which
    /// [`post_app_event`] reaches.
    static RUNNING: RefCell<Weak<Shared>> = const { RefCell::new(Weak::new()) };
}

/// Hands the running application the automation's request `request` of
/// the app `client`, as the system's scripts and shortcuts make them, from
/// the main thread: `reply` takes its answer, once, as the app gives it.
/// Whether an application ran to take it; `reply` goes unanswered without.
pub fn post_automation(
    client: &str,
    request: lokyr_platform_core::automation::Request,
    reply: impl FnOnce(lokyr_platform_core::automation::Answer) + 'static,
) -> bool {
    if MainThreadMarker::new().is_none() {
        return false;
    }
    let Some(shared) = RUNNING.with(|running| running.borrow().upgrade()) else {
        return false;
    };
    // The bridges run of the system's events, outside the app's calls.
    shared.post_automation(client, request, Box::new(reply))
}

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
    RUNNING.with(|running| *running.borrow_mut() = Rc::downgrade(&shared));
    let delegate = AppDelegate::new(mtm, Rc::downgrade(&shared));
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // Before the launch, so that a service that starts the app reaches it.
    let services = ServicesProvider::new(mtm, Rc::downgrade(&shared));
    // SAFETY: the provider answers the messages of the bundle's services,
    // of their signatures, and lives until it is taken away below.
    unsafe { app.setServicesProvider(Some(&services)) };
    let appearance = AppearanceObserver::start(mtm, Rc::downgrade(&shared));
    // AppleScript's classes, which the dictionary names, before a script
    // asks.
    crate::scripting::register();

    app.run();

    appearance.stop();
    RUNNING.with(|running| *running.borrow_mut() = Weak::new());
    // SAFETY: none takes its place.
    unsafe { app.setServicesProvider(None) };
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

    impl AppDelegate {
        /// AppleScript's windows of the application, of the dictionary's
        /// key that the delegate answers.
        #[unsafe(method_id(lokyrWindows))]
        fn lokyr_windows(&self) -> Retained<NSArray<crate::scripting::ScriptWindow>> {
            crate::scripting::windows(self.mtm())
        }
    }

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

        #[unsafe(method(applicationDidBecomeActive:))]
        fn application_did_become_active(&self, _notification: &NSNotification) {
            // Another app may have taken the default terminal while the app
            // was in the background.
            crate::default_terminal::look_again();
            // The user may have allowed the double taps meanwhile, or
            // changed the double-click interval: the app hears whether they
            // are watched now.
            if let Some(watching) = crate::double_taps::again() {
                self.post(Pending::App(AppEvent::DoubleTapsWatched(watching)));
            }
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

        #[unsafe(method(application:openURLs:))]
        fn application_open_urls(&self, _sender: &NSApplication, urls: &NSArray<NSURL>) {
            // The files that the Finder opens with the app, or that the user
            // drops on its icon, of the types that its bundle declares; and
            // the links of its schemes.
            let urls: Vec<_> = (0..urls.count()).map(|index| urls.objectAtIndex(index)).collect();
            let paths: Vec<std::path::PathBuf> = urls
                .iter()
                .filter(|url| url.isFileURL())
                .filter_map(|url| url.path().map(|path| std::path::PathBuf::from(path.to_string())))
                .collect();
            let links: Vec<String> = urls
                .iter()
                .filter(|url| !url.isFileURL())
                .filter_map(|url| url.absoluteString().map(|text| text.to_string()))
                .collect();
            if !paths.is_empty() {
                self.post(Pending::App(AppEvent::OpenFiles(paths)));
            }
            if !links.is_empty() {
                self.post(Pending::App(AppEvent::OpenUrls(links)));
            }
        }

        // AppleScript's windows, of the dictionary's key, which the
        // delegate answers for the application.
        #[unsafe(method(application:delegateHandlesKey:))]
        fn application_delegate_handles_key(&self, _sender: &NSApplication, key: &NSString) -> Bool {
            Bool::new(key.to_string() == crate::scripting::WINDOWS)
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

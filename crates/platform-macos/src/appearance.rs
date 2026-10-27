//! The system's appearance, and an observer of the application's, which
//! tells the app when it changes.

use std::cell::Cell;
use std::ffi::c_void;
use std::rc::Weak;

use lokyr_platform_core::{AppEvent, Appearance};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication};
use objc2_foundation::{
    NSArray, NSDictionary, NSKeyValueChangeKey, NSKeyValueObservingOptions, NSObject,
    NSObjectNSKeyValueObserverRegistration, NSObjectProtocol, NSString,
};

use crate::shared::{Pending, Shared};

/// The application's key path that the observer watches.
const KEY_PATH: &str = "effectiveAppearance";

/// The application's appearance: dark, or light.
pub(crate) fn current(mtm: MainThreadMarker) -> Appearance {
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: the names are AppKit's constants.
    let (aqua, dark) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
    let names = NSArray::from_slice(&[aqua, dark]);
    match app
        .effectiveAppearance()
        .bestMatchFromAppearancesWithNames(&names)
    {
        Some(name) if &*name == dark => Appearance::Dark,
        _ => Appearance::Light,
    }
}

pub(crate) struct ObserverIvars {
    shared: Weak<Shared>,
    /// The appearance that the app knows.
    known: Cell<Appearance>,
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // observer does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ObserverIvars]
    pub(crate) struct AppearanceObserver;

    impl AppearanceObserver {
        // SAFETY: key-value observing's signature.
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observe(
            &self,
            _key_path: Option<&NSString>,
            _object: Option<&AnyObject>,
            _change: Option<&NSDictionary<NSKeyValueChangeKey, AnyObject>>,
            _context: *mut c_void,
        ) {
            let appearance = current(self.mtm());
            let ivars = self.ivars();
            if ivars.known.replace(appearance) == appearance {
                return;
            }
            if let Some(shared) = ivars.shared.upgrade() {
                shared.post(Pending::App(AppEvent::AppearanceChanged(appearance)));
            }
        }
    }

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for AppearanceObserver {}
);

impl AppearanceObserver {
    /// An observer of the application's appearance, which it watches until
    /// [`AppearanceObserver::stop`].
    pub(crate) fn start(mtm: MainThreadMarker, shared: Weak<Shared>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ObserverIvars {
            shared,
            known: Cell::new(current(mtm)),
        });
        // SAFETY: `NSObject`'s `init` has this signature.
        let observer: Retained<Self> = unsafe { msg_send![super(this), init] };
        let app = NSApplication::sharedApplication(mtm);
        // SAFETY: an observer that answers key-value observing, of a key path
        // that the application makes observable, without a context; `stop`
        // removes it before it goes.
        unsafe {
            app.addObserver_forKeyPath_options_context(
                &observer,
                &NSString::from_str(KEY_PATH),
                NSKeyValueObservingOptions::New,
                std::ptr::null_mut(),
            );
        }
        observer
    }

    /// Stops watching the application.
    pub(crate) fn stop(&self) {
        let app = NSApplication::sharedApplication(self.mtm());
        // SAFETY: the observer that `start` added, of its key path.
        unsafe { app.removeObserver_forKeyPath(self, &NSString::from_str(KEY_PATH)) };
    }
}

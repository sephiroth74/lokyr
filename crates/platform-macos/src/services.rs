//! The services that the app gives the system's other apps, which the
//! bundle declares: a window, or a tab, at each folder that the Finder
//! selected, from its menu of a folder or the Services menu.

use std::rc::Weak;

use lokyr_platform_core::{AppEvent, Dropped};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

use crate::shared::{Pending, Shared};
use crate::window;

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // provider does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Weak<Shared>]
    pub(crate) struct ServicesProvider;

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for ServicesProvider {}

    impl ServicesProvider {
        // SAFETY: a service's signature, of the bundle's `NSMessage`
        // `openWindow`.
        #[unsafe(method(openWindow:userData:error:))]
        fn open_window(
            &self,
            pasteboard: &NSPasteboard,
            _user_data: Option<&NSString>,
            _error: *mut *mut NSString,
        ) {
            self.open(pasteboard, false);
        }

        // SAFETY: a service's signature, of the bundle's `NSMessage`
        // `openTab`.
        #[unsafe(method(openTab:userData:error:))]
        fn open_tab(
            &self,
            pasteboard: &NSPasteboard,
            _user_data: Option<&NSString>,
            _error: *mut *mut NSString,
        ) {
            self.open(pasteboard, true);
        }
    }
);

impl ServicesProvider {
    pub(crate) fn new(mtm: MainThreadMarker, shared: Weak<Shared>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(shared);
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    /// Asks the app for a window, or a tab, at each folder of `pasteboard`.
    fn open(&self, pasteboard: &NSPasteboard, tabs: bool) {
        let Some(Dropped::Files(folders)) = window::dropped(pasteboard) else {
            return;
        };
        if let Some(shared) = self.ivars().upgrade() {
            shared.post(Pending::App(AppEvent::OpenFolders { folders, tabs }));
        }
    }
}

//! The system's notifications, through the user notification center: each
//! request asks for the permission first, which the center asks the user
//! once and keeps, and a delegate shows them while the application is
//! active too, and tells the window of the one that the user clicks.

use std::sync::Mutex;

use block2::RcBlock;
use lokyr_platform_core::{Waker, WindowId};
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{AllocAnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::{NSBundle, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

/// What starts the identifier of each notification, before its window's
/// number and its own.
const PREFIX: &str = "lokyr-";

/// The windows and the app's numbers of the notifications that the user
/// clicked, which the main thread takes once the delegate wakes it, as the
/// center calls the delegate from a thread of its own.
static CLICKED: Mutex<Vec<(WindowId, u64)>> = Mutex::new(Vec::new());

/// Takes the notifications clicked since the last take.
pub(crate) fn take_clicked() -> Vec<(WindowId, u64)> {
    CLICKED
        .lock()
        .map(|mut clicked| std::mem::take(&mut *clicked))
        .unwrap_or_default()
}

/// The window that the notification of `identifier` is for, and the app's
/// number of it.
fn window_of(identifier: &str) -> Option<(WindowId, u64)> {
    let (window, id) = identifier.strip_prefix(PREFIX)?.split_once('-')?;
    Some((WindowId::new(window.parse().ok()?), id.parse().ok()?))
}

/// The notifications of the application.
pub(crate) struct Notifier {
    center: Retained<UNUserNotificationCenter>,
    /// The center's delegate, which the center does not keep.
    _delegate: Retained<NotificationDelegate>,
}

impl Notifier {
    /// The notifier, with a delegate that wakes the main thread with
    /// `waker` as the user clicks a notification; `None` outside a bundle
    /// with an identifier, as the center needs one.
    pub(crate) fn new(waker: Waker) -> Option<Self> {
        NSBundle::mainBundle().bundleIdentifier()?;
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = NotificationDelegate::new(waker);
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Some(Self {
            center,
            _delegate: delegate,
        })
    }

    /// Shows a notification of `title` and `body` for `window`, of the
    /// app's number `id`, once the user allows them: the center asks the
    /// first time, and answers at once after.
    pub(crate) fn notify(&mut self, window: WindowId, id: u64, title: &str, body: &str) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setBody(&NSString::from_str(body));
        let identifier = NSString::from_str(&format!("{PREFIX}{}-{id}", window.get()));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );
        let center = self.center.clone();
        let add = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            if granted.as_bool() {
                center.addNotificationRequest_withCompletionHandler(&request, None);
            }
        });
        self.center
            .requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert, &add);
    }
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // delegate does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[ivars = Waker]
    struct NotificationDelegate;

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for NotificationDelegate {}

    // SAFETY: `UNUserNotificationCenterDelegate` has no requirement; each
    // method has the framework's signature, and calls its completion
    // handler once.
    unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
        // The app shows a notification only for a window without the
        // focus, which shows while the application is active too.
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            if let Some(clicked_one) = window_of(&identifier)
                && let Ok(mut clicked) = CLICKED.lock()
            {
                clicked.push(clicked_one);
                self.ivars().wake();
            }
            completion.call(());
        }
    }
);

impl NotificationDelegate {
    fn new(waker: Waker) -> Retained<Self> {
        let this = Self::alloc().set_ivars(waker);
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notification_s_identifier_tells_its_window() {
        assert_eq!(window_of("lokyr-42-7"), Some((WindowId::new(42), 7)));
        assert_eq!(window_of("lokyr-42"), None);
        assert_eq!(window_of("other-42-7"), None);
        assert_eq!(window_of("lokyr-x-7"), None);
    }
}

//! The services that the app gives the Finder's folders: the provider that
//! the application has answers New Lokyr Window Here and New Lokyr Tab Here
//! of a pasteboard of folders, which come to the app as their paths, a
//! window's then a tab's; the app started of its own is a default launch,
//! and the system's answer of whether it is the default terminal comes
//! after the start.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    use lokyr_platform_core::{AppEvent, Handler, Platform, WindowEvent, WindowId};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::{NSApplication, NSPasteboard, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSString, NSURL};

    use crate::common;

    /// The folders that the Finder selected.
    const FOLDERS: [&str; 2] = ["/tmp", "/Applications/Utilities"];

    #[derive(Debug, Default)]
    struct Seen {
        default_launch: Option<bool>,
        /// The folders that came, and whether as tabs.
        opened: Vec<(Vec<PathBuf>, bool)>,
        /// Whether the default terminal's answer came, and what it told: a
        /// binary outside a bundle is never the default.
        known: Option<bool>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    /// A pasteboard of [`FOLDERS`]' URLs.
    fn pasteboard() -> Retained<NSPasteboard> {
        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        pasteboard.clearContents();
        let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = FOLDERS
            .iter()
            .map(|path| {
                let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                ProtocolObject::from_retained(url)
            })
            .collect();
        pasteboard.writeObjects(&NSArray::from_retained_slice(&urls));
        pasteboard
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            self.0.borrow_mut().default_launch = Some(platform.default_launch());
            let mtm = MainThreadMarker::new().expect("the main thread");
            let app = NSApplication::sharedApplication(mtm);
            // SAFETY: the application's provider of services, an object.
            let provider: Option<Retained<AnyObject>> =
                unsafe { msg_send![&app, servicesProvider] };
            let provider = provider.expect("a provider of services");
            let pasteboard = pasteboard();
            let user_data: Option<&NSString> = None;
            let mut error: *mut NSString = std::ptr::null_mut();
            // SAFETY: the services' messages, of their signature, as the
            // system sends them.
            unsafe {
                let () = msg_send![
                    &provider,
                    openWindow: &*pasteboard,
                    userData: user_data,
                    error: &mut error
                ];
                let () = msg_send![
                    &provider,
                    openTab: &*pasteboard,
                    userData: user_data,
                    error: &mut error
                ];
            }
            common::wake_after(platform.waker(), 200);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            match event {
                AppEvent::OpenFolders { folders, tabs } => {
                    self.0.borrow_mut().opened.push((folders, tabs));
                }
                AppEvent::DefaultTerminalKnown => {
                    self.0.borrow_mut().known = Some(platform.is_default_terminal());
                }
                AppEvent::Wake if self.0.borrow().known.is_some() => platform.quit(),
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("services", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.default_launch, Some(true), "{seen:?}");
        assert_eq!(seen.known, Some(false), "{seen:?}");
        let folders: Vec<PathBuf> = FOLDERS.iter().map(PathBuf::from).collect();
        assert_eq!(
            seen.opened,
            [(folders.clone(), false), (folders, true)],
            "{seen:?}"
        );
        println!("services: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

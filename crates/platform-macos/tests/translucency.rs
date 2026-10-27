//! A window made translucent, then opaque again: the window, its content
//! view and the view's layer turn non-opaque together, over a clear
//! background, and back.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use bolt_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSView};
    use raw_window_handle::RawWindowHandle;

    use crate::common;

    /// Whether the window, its view and the view's layer are opaque, and
    /// the opacity of the window's background.
    type Look = (bool, bool, bool, f64);

    fn look(platform: &dyn Platform, window: WindowId) -> Look {
        let handle = platform.window_handle(window).expect("a handle");
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            panic!("an AppKit view");
        };
        // SAFETY: the handle holds the window's view while the platform is
        // borrowed.
        let view: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
        let ns_window = view.window().expect("the view's window");
        let layer = view.layer().expect("the view's layer");
        let background: Retained<_> = ns_window.backgroundColor();
        (
            ns_window.isOpaque(),
            view.isOpaque(),
            layer.isOpaque(),
            background.alphaComponent(),
        )
    }

    struct Probe(Rc<RefCell<Vec<Look>>>);

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "translucency".to_owned(),
                    size: Size {
                        width: 200.0,
                        height: 100.0,
                    },
                })
                .expect("the window opens");
            let mtm = MainThreadMarker::new().expect("the main thread");
            NSApplication::sharedApplication(mtm).hide(None);
            let mut seen = self.0.borrow_mut();
            seen.push(look(platform, window));
            platform.set_translucent(window, Some(20.0));
            seen.push(look(platform, window));
            platform.set_translucent(window, Some(0.0));
            seen.push(look(platform, window));
            platform.set_translucent(window, None);
            seen.push(look(platform, window));
            platform.quit();
        }

        fn app_event(&mut self, _: &mut dyn Platform, _: AppEvent) {}

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("translucency", 20);
        let seen = Rc::new(RefCell::new(Vec::new()));
        bolt_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let opaque = seen[0];
        assert_eq!((opaque.0, opaque.1, opaque.2), (true, true, true));
        assert!(opaque.3 > 0.0, "{seen:?}");
        // Translucent, blurred or not, over a clear background.
        for translucent in &seen[1..3] {
            assert_eq!(*translucent, (false, false, false, 0.0), "{seen:?}");
        }
        assert_eq!(seen[3], opaque, "back as it opened");
        println!("translucency: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

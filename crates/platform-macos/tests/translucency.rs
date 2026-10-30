//! A window made translucent, then opaque again: the window, its content
//! view and the view's layer turn non-opaque together, over the backdrop's
//! color, and back.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Backdrop, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSView};
    use raw_window_handle::RawWindowHandle;

    use crate::common;

    /// Whether the window, its view and the view's layer are opaque, the
    /// opacity of the window's background, and whether its titlebar is
    /// transparent.
    type Look = (bool, bool, bool, f64, bool);

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
            ns_window.titlebarAppearsTransparent(),
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
                    remember: false,
                    restore: false,
                    restore_size: false,
                })
                .expect("the window opens");
            let mtm = MainThreadMarker::new().expect("the main thread");
            NSApplication::sharedApplication(mtm).hide(None);
            let mut seen = self.0.borrow_mut();
            seen.push(look(platform, window));
            let backdrop = Backdrop {
                color: [0.2, 0.4, 0.6, 0.8],
                blur: 20.0,
            };
            platform.set_translucent(window, Some(backdrop));
            seen.push(look(platform, window));
            platform.set_translucent(
                window,
                Some(Backdrop {
                    blur: 0.0,
                    ..backdrop
                }),
            );
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
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let opaque = seen[0];
        assert_eq!((opaque.0, opaque.1, opaque.2), (true, true, true));
        assert!(opaque.3 > 0.0, "{seen:?}");
        // The titlebar shows what the view draws under it, opaque or not.
        assert!(opaque.4, "{seen:?}");
        // Translucent, blurred or not, over the backdrop's color.
        for translucent in &seen[1..3] {
            let (window, view, layer, alpha, titlebar) = *translucent;
            assert_eq!((window, view, layer), (false, false, false), "{seen:?}");
            assert!((alpha - 0.8).abs() < 1e-6, "{seen:?}");
            assert!(titlebar, "the titlebar shows the backdrop: {seen:?}");
        }
        assert_eq!(seen[3], opaque, "back as it opened");
        println!("translucency: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

//! A quick window opens hidden; it shows at its edge of the screen, of its
//! size, over the other windows and on every Space, with the focus, and
//! hides; it slides in from another edge; and the global keys register,
//! but a key without a code.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    use lokyr_platform_core::{
        AppEvent, GlobalKey, Handler, KeyCode, Length, Platform, QuickEdge, QuickPlace,
        QuickScreen, WindowEvent, WindowId, WindowSize,
    };
    use objc2::rc::Retained;
    use objc2::{ClassType, MainThreadMarker};
    use objc2_app_kit::{
        NSApplication, NSFloatingWindowLevel, NSPanel, NSScreen, NSWindow,
        NSWindowCollectionBehavior,
    };
    use objc2_foundation::{NSObjectProtocol, NSRect};

    use crate::common;

    /// What the panel was, at a step.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Panel {
        visible: bool,
        frame: NSRect,
        level: isize,
        behavior: NSWindowCollectionBehavior,
    }

    #[derive(Debug, Default)]
    struct Seen {
        quick: Option<WindowId>,
        /// The area of the menu bar's screen for windows.
        area: Option<NSRect>,
        /// The panel as it opened, as it showed at the top, as it hid, and
        /// once it slid in from the bottom.
        panels: Vec<Panel>,
        /// Its sizes as it opened and as it showed.
        sizes: Vec<WindowSize>,
        focused: bool,
        /// What the system said of the global keys.
        refused: Vec<Vec<usize>>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    fn place(edge: QuickEdge, animation: Duration) -> QuickPlace {
        QuickPlace {
            edge,
            primary: Length::Percent(40.0),
            secondary: Some(Length::Points(600.0)),
            screen: QuickScreen::MenuBar,
            animation,
            every_space: true,
        }
    }

    /// The application's panel, the quick window's.
    fn panel() -> Panel {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        let panel: Retained<NSWindow> = (0..windows.count())
            .map(|index| windows.objectAtIndex(index))
            .find(|window| window.isKindOfClass(NSPanel::class()))
            .expect("a panel");
        Panel {
            visible: panel.isVisible(),
            frame: panel.frame(),
            level: panel.level(),
            behavior: panel.collectionBehavior(),
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let mtm = MainThreadMarker::new().expect("the main thread");
            let quick = platform
                .create_quick_window("A quick window")
                .expect("a window");
            let mut seen = self.0.borrow_mut();
            seen.quick = Some(quick);
            seen.area = NSScreen::screens(mtm)
                .firstObject()
                .map(|screen| screen.visibleFrame());
            seen.panels.push(panel());
            seen.sizes.extend(platform.window_size(quick));
            platform.show_quick_window(quick, Some(&place(QuickEdge::Top, Duration::ZERO)));
            seen.panels.push(panel());
            seen.sizes.extend(platform.window_size(quick));
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let mut seen = self.0.borrow_mut();
            seen.panels.push(panel());
            let key = |code| GlobalKey {
                code,
                command: true,
                control: true,
                alt: true,
                shift: true,
            };
            seen.refused
                .push(platform.set_global_keys(&[key(KeyCode::F(19))]));
            seen.refused
                .push(platform.set_global_keys(&[key(KeyCode::F(18)), key(KeyCode::F(24))]));
            seen.refused.push(platform.set_global_keys(&[]));
            platform.quit();
        }

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            let mut seen = self.0.borrow_mut();
            if event != WindowEvent::Focused(true) || seen.quick != Some(window) || seen.focused {
                return;
            }
            seen.focused = true;
            platform.show_quick_window(window, None);
            seen.panels.push(panel());
            let slide = place(QuickEdge::Bottom, Duration::from_millis(150));
            platform.show_quick_window(window, Some(&slide));
            common::wake_after(platform.waker(), 600);
        }
    }

    pub fn main() {
        common::deadline("quick_window", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let area = seen.area.expect("a screen");
        let [opened, top, hidden, bottom] = seen.panels[..] else {
            panic!("four steps: {seen:?}");
        };
        assert!(!opened.visible, "{seen:?}");
        let (width, height) = (
            600.0_f64.min(area.size.width),
            (area.size.height * 0.4).round(),
        );
        let left = (area.origin.x + (area.size.width - width) / 2.0).round();
        let area_top = area.origin.y + area.size.height;
        // At the top of the area under the menu bar, centred, over the
        // other windows, on every Space and over the apps in full screen.
        assert!(top.visible, "{seen:?}");
        assert_eq!(top.frame.size.width, width, "{seen:?}");
        assert_eq!(top.frame.size.height, height, "{seen:?}");
        assert_eq!(top.frame.origin.x, left, "{seen:?}");
        assert_eq!(top.frame.origin.y + height, area_top.round(), "{seen:?}");
        assert_eq!(top.level, NSFloatingWindowLevel, "{seen:?}");
        for behavior in [
            NSWindowCollectionBehavior::CanJoinAllSpaces,
            NSWindowCollectionBehavior::FullScreenAuxiliary,
        ] {
            assert!(top.behavior.contains(behavior), "{seen:?}");
        }
        let [before, shown] = seen.sizes[..] else {
            panic!("two sizes: {seen:?}");
        };
        assert_eq!((before.points.width, before.points.height), (800.0, 400.0));
        assert_eq!((shown.points.width, shown.points.height), (width, height));
        assert_eq!(shown.bar, 0.0, "{seen:?}");
        assert!(seen.focused, "{seen:?}");
        assert!(!hidden.visible, "{seen:?}");
        // From the bottom, above the Dock, once its slide ended.
        assert!(bottom.visible, "{seen:?}");
        assert_eq!(bottom.frame.origin.y, area.origin.y.round(), "{seen:?}");
        assert_eq!(bottom.level, NSFloatingWindowLevel, "{seen:?}");
        // A key without a code, as F24, which the Mac's keyboards lack, is
        // refused.
        assert_eq!(seen.refused, [vec![], vec![1], vec![]], "{seen:?}");
        println!("quick_window: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

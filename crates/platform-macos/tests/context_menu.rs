//! A context menu over a window: it opens once the handler's call has
//! returned, and runs its own loop, while timers fire and the window draws;
//! its dimmed items are disabled; its chosen item's command comes as the
//! window's event. A menu that is open closes with its window, and as the
//! app quits, so that `run` returns.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::ptr::NonNull;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use block2::RcBlock;
    use lokyr_platform_core::{
        AppEvent, ContextMenu, Handler, MenuItem, Platform, Point, Size, WindowAttributes,
        WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{
        NSApplication, NSMenu, NSMenuDidBeginTrackingNotification,
        NSMenuDidEndTrackingNotification, NSPopUpMenuWindowLevel,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint};

    use crate::common;

    thread_local! {
        /// The menu whose loop began last, until it ends.
        static TRACKED: RefCell<Option<Retained<NSMenu>>> = const { RefCell::new(None) };
    }

    /// How many times the first menu opens again, when a click or a key of
    /// the user's closed it before its time.
    const RETRIES: usize = 3;

    #[derive(Debug, Default)]
    struct Seen {
        /// The step that the probe is at.
        step: usize,
        /// Whether each step's timer fired within a menu's loop.
        in_menu: Vec<bool>,
        /// Whether the menu's items were enabled, as it showed them.
        enabled: Vec<bool>,
        /// The frames that came while the first menu was open.
        frames_in_menu: usize,
        /// Whether a menu's window had its top left corner at the point
        /// that the app asked for.
        at_point: bool,
        commands: Vec<(WindowId, String)>,
        /// How many times the first menu opened again.
        retries: usize,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
    }

    fn open(platform: &mut dyn Platform) -> WindowId {
        platform
            .create_window(&WindowAttributes {
                title: "A context menu".to_owned(),
                size: Size {
                    width: 320.0,
                    height: 200.0,
                },
            })
            .expect("a window")
    }

    fn menu() -> ContextMenu {
        let item = |command: &str| MenuItem::Command {
            title: command.to_owned(),
            command: command.to_owned(),
            shortcut: None,
        };
        ContextMenu {
            items: vec![item("test.first"), MenuItem::Separator, item("test.dimmed")],
            dimmed: vec!["test.dimmed".to_owned()],
        }
    }

    /// Whether a window of the menus has its top left corner near the point
    /// of the screen that is `point` from the top left corner of the probe's
    /// window's content.
    fn menu_at(point: Point) -> bool {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        let probe = windows.firstObject().expect("the probe's window");
        let view = probe.contentView().expect("the window's view");
        let content = probe.convertRectToScreen(view.convertRect_toView(view.bounds(), None));
        let wanted = NSPoint::new(
            content.origin.x + point.x,
            content.origin.y + content.size.height - point.y,
        );
        (0..windows.count()).any(|index| {
            let window = windows.objectAtIndex(index);
            let frame = window.frame();
            let top_left = NSPoint::new(frame.origin.x, frame.origin.y + frame.size.height);
            window.level() == NSPopUpMenuWindowLevel
                && (top_left.x - wanted.x).abs() <= 10.0
                && (top_left.y - wanted.y).abs() <= 10.0
        })
    }

    /// The menu that is open, if one's loop runs.
    fn tracked() -> Option<Retained<NSMenu>> {
        TRACKED.with(|tracked| tracked.borrow().clone())
    }

    impl Probe {
        fn step(&mut self, platform: &mut dyn Platform) {
            let mut seen = self.seen.borrow_mut();
            seen.step += 1;
            seen.in_menu.push(tracked().is_some());
            let window = self.window.expect("the probe's window");
            match seen.step {
                // The first menu is open at its point: the window draws
                // meanwhile.
                1 => {
                    seen.at_point = menu_at(Point { x: 20.0, y: 20.0 });
                    platform.request_frame(window);
                }
                // Something of the user's closed the first menu, or the third,
                // before its time: it opens again, for the step again.
                2 | 4 if tracked().is_none() && seen.retries < RETRIES => {
                    seen.retries += 1;
                    seen.step -= 1;
                    seen.in_menu.pop();
                    platform.show_context_menu(window, Point { x: 20.0, y: 20.0 }, &menu());
                }
                // Its first item is chosen, which closes it.
                2 => {
                    let menu = tracked().expect("the first menu");
                    for index in [0, 2] {
                        let item = menu.itemAtIndex(index).expect("an item");
                        seen.enabled.push(item.isEnabled());
                    }
                    TRACKED.with(|tracked| tracked.borrow_mut().take());
                    menu.performActionForItemAtIndex(0);
                    menu.cancelTracking();
                }
                // The second menu closes with its window.
                3 => {
                    TRACKED.with(|tracked| tracked.borrow_mut().take());
                    platform.close_window(window);
                    let window = open(platform);
                    self.window = Some(window);
                    platform.show_context_menu(window, Point { x: 20.0, y: 20.0 }, &menu());
                }
                // The third is open as the app quits.
                _ => platform.quit(),
            }
            if seen.step < 4 {
                platform.start_timer(Instant::now() + Duration::from_millis(400));
            }
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = open(platform);
            self.window = Some(window);
            platform.show_context_menu(window, Point { x: 20.0, y: 20.0 }, &menu());
            platform.start_timer(Instant::now() + Duration::from_millis(400));
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if let AppEvent::Timer(_) = event {
                self.step(platform);
            }
        }

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            let mut seen = self.seen.borrow_mut();
            match event {
                WindowEvent::Frame if seen.step == 1 && tracked().is_some() => {
                    seen.frames_in_menu += 1;
                }
                WindowEvent::ContextCommand(command) => {
                    seen.commands.push((window, command));
                    // The second menu, once the first has closed.
                    platform.show_context_menu(window, Point { x: 40.0, y: 40.0 }, &menu());
                }
                _ => {}
            }
        }
    }

    pub fn main() {
        common::deadline("context_menu", 30);
        let block = RcBlock::new(|notification: NonNull<NSNotification>| {
            // SAFETY: the notification lives through the observer's call.
            let notification = unsafe { notification.as_ref() };
            let menu = notification
                .object()
                .and_then(|object| object.downcast::<NSMenu>().ok());
            TRACKED.with(|tracked| *tracked.borrow_mut() = menu);
        });
        // SAFETY: the observer runs on the main thread, which posts the
        // notification, and lives as long as the process.
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSMenuDidBeginTrackingNotification),
                None,
                None,
                &block,
            )
        };
        let ended = RcBlock::new(|notification: NonNull<NSNotification>| {
            // SAFETY: the notification lives through the observer's call.
            let notification = unsafe { notification.as_ref() };
            let menu = notification
                .object()
                .and_then(|object| object.downcast::<NSMenu>().ok());
            TRACKED.with(|tracked| {
                let mut tracked = tracked.borrow_mut();
                if tracked.as_ref() == menu.as_ref() {
                    *tracked = None;
                }
            });
        });
        // SAFETY: as the observer of the beginnings.
        let end_observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSMenuDidEndTrackingNotification),
                None,
                None,
                &ended,
            )
        };
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        drop((observer, end_observer));
        let seen = seen.borrow();
        assert_eq!(seen.in_menu, [true, true, true, true], "{seen:?}");
        assert!(seen.frames_in_menu >= 1, "{seen:?}");
        assert!(seen.at_point, "{seen:?}");
        assert_eq!(seen.enabled, [true, false], "{seen:?}");
        let commands: Vec<&str> = seen
            .commands
            .iter()
            .map(|(_, command)| command.as_str())
            .collect();
        assert_eq!(commands, ["test.first"], "{seen:?}");
        println!("context_menu: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

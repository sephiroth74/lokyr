//! An alert withdrawn as a window's sheet: the sheet goes, and no answer
//! comes of it, as what it asked about is gone; another sheet opens on
//! the window after it.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        Alert, AlertId, AlertStyle, AppEvent, Handler, Platform, Size, WindowAttributes,
        WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// Whether a sheet was attached: as the first alert opened, once it
        /// was withdrawn, and as the second opened.
        sheets: Vec<bool>,
        /// The answers that came.
        answers: usize,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        alert: Option<AlertId>,
        step: usize,
    }

    /// Whether a window of the app has a sheet attached.
    fn sheet_attached() -> bool {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        (0..windows.count()).any(|index| windows.objectAtIndex(index).attachedSheet().is_some())
    }

    fn question(message: &str) -> Alert {
        Alert {
            style: AlertStyle::Warning,
            message: message.to_owned(),
            detail: String::new(),
            buttons: vec!["Close".to_owned(), "Cancel".to_owned()],
            suppression: None,
            input: None,
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "A withdrawn alert".to_owned(),
                    size: Size {
                        width: 400.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("the window opens");
            self.window = Some(window);
            self.alert = Some(platform.show_alert(Some(window), &question("Close this tab?")));
            common::wake_after(platform.waker(), 500);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            match event {
                AppEvent::Wake => {
                    self.step += 1;
                    self.seen.borrow_mut().sheets.push(sheet_attached());
                    match self.step {
                        1 => {
                            platform.dismiss_alert(self.alert.expect("the alert"));
                            // Withdrawn again, or of an alert that is not
                            // open, nothing.
                            platform.dismiss_alert(self.alert.expect("the alert"));
                            common::wake_after(platform.waker(), 500);
                        }
                        2 => {
                            let window = self.window.expect("the window");
                            platform.show_alert(Some(window), &question("Close this pane?"));
                            common::wake_after(platform.waker(), 500);
                        }
                        _ => platform.quit(),
                    }
                }
                AppEvent::AlertClosed { .. } => self.seen.borrow_mut().answers += 1,
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("alert_dismiss", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            alert: None,
            step: 0,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.sheets, [true, false, true], "{seen:?}");
        assert_eq!(seen.answers, 0, "{seen:?}");
        println!("alert_dismiss: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

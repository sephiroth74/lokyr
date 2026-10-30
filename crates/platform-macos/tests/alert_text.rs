//! An alert with a field of text, as a window's sheet: the field holds the
//! text that the alert opened with, and the answer gives the text that it
//! holds as a button closes it.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        Alert, AlertStyle, AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent,
        WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSButton, NSTextField, NSView};
    use objc2_foundation::NSString;

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// The field's text as the alert opened.
        opened_with: Option<String>,
        /// What the answer gave.
        answers: Vec<(usize, Option<String>)>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    /// The views of `view`'s tree that are `T`s, from the top.
    fn find<T: objc2::DowncastTarget>(view: &NSView) -> Vec<Retained<T>> {
        let mut found = Vec::new();
        let subviews = view.subviews();
        for index in 0..subviews.count() {
            let subview = subviews.objectAtIndex(index);
            if let Ok(matching) = subview.clone().downcast::<T>() {
                found.push(matching);
            }
            found.extend(find::<T>(&subview));
        }
        found
    }

    /// The editable field of the sheet that is open.
    fn field() -> Retained<NSTextField> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        let sheet = (0..windows.count())
            .find_map(|index| windows.objectAtIndex(index).attachedSheet())
            .expect("the alert's sheet");
        let content = sheet.contentView().expect("the sheet's view");
        find::<NSTextField>(&content)
            .into_iter()
            .find(|field| field.isEditable())
            .expect("the alert's field")
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "An alert's field".to_owned(),
                    size: Size {
                        width: 400.0,
                        height: 300.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                })
                .expect("the window opens");
            platform.show_alert(
                Some(window),
                &Alert {
                    style: AlertStyle::Informational,
                    message: "The window's title".to_owned(),
                    detail: String::new(),
                    buttons: vec!["Set".to_owned(), "Cancel".to_owned()],
                    suppression: None,
                    input: Some("old".to_owned()),
                },
            );
            // As it opens, before a key that the user types can reach it.
            self.0.borrow_mut().opened_with = Some(field().stringValue().to_string());
            common::wake_after(platform.waker(), 500);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            match event {
                // The sheet is open: its field takes a text, and its first
                // button closes it.
                AppEvent::Wake => {
                    let field = field();
                    let content = field
                        .window()
                        .and_then(|sheet| sheet.contentView())
                        .expect("the sheet's view");
                    field.setStringValue(&NSString::from_str("new title"));
                    let button = find::<NSButton>(&content)
                        .into_iter()
                        .find(|button| button.title().to_string() == "Set")
                        .expect("the alert's first button");
                    // SAFETY: a click of the alert's button, with no sender.
                    unsafe { button.performClick(None) };
                }
                AppEvent::AlertClosed { button, text, .. } => {
                    self.0.borrow_mut().answers.push((button, text));
                    platform.quit();
                }
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("alert_text", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.opened_with.as_deref(), Some("old"), "{seen:?}");
        assert_eq!(
            seen.answers,
            [(0, Some("new title".to_owned()))],
            "{seen:?}"
        );
        println!("alert_text: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

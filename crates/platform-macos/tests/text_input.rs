//! The window's view is the input method's client: the text that it
//! composes comes as a preedit, the text that it types as a commit, and its
//! candidates go where the app said the typed text goes. A key with an
//! Option that acts as Meta skips the input method.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Ime, KeyCode, Platform, Point, Size, WindowAttributes, WindowEvent,
        WindowId,
    };
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSView};
    use objc2_foundation::{NSNotFound, NSPoint, NSRange, NSRect, NSString};

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        events: Vec<WindowEvent>,
        rect: Option<NSRect>,
        frame: Option<NSRect>,
        marked: Vec<bool>,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        sent: bool,
    }

    fn view() -> Retained<NSView> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window");
        window.contentView().expect("the window's view")
    }

    fn none() -> NSRange {
        NSRange::new(NSNotFound as usize, 0)
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "text input".to_owned(),
                    size: Size {
                        width: 300.0,
                        height: 200.0,
                    },
                })
                .expect("the window opens");
            // The app steps back at once, so that the keys of someone typing
            // at the Mac go to their own app; the probe's go to the window
            // itself.
            let mtm = MainThreadMarker::new().expect("the main thread");
            NSApplication::sharedApplication(mtm).hide(None);
            self.window = Some(window);
            common::wake_after(platform.waker(), 200);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            if self.sent {
                platform.quit();
                return;
            }
            self.sent = true;
            let window = self.window.expect("the window");
            platform.set_text_area(
                window,
                Point { x: 10.0, y: 20.0 },
                Size {
                    width: 8.0,
                    height: 16.0,
                },
            );
            platform.set_option_as_meta(true, false);
            let view = view();
            // SAFETY: the client's method, which takes nothing.
            let has_marked = || -> bool { unsafe { msg_send![&*view, hasMarkedText] } };
            let mut seen = self.seen.borrow_mut();
            // SAFETY: the client's methods, with an NSString and ranges.
            unsafe {
                let _: () = msg_send![&*view, setMarkedText: &*NSString::from_str("にほ"),
                    selectedRange: NSRange::new(2, 0), replacementRange: none()];
                seen.marked.push(has_marked());
                let _: () = msg_send![&*view, insertText: &*NSString::from_str("日本"),
                    replacementRange: none()];
                seen.marked.push(has_marked());
                let _: () = msg_send![&*view, setMarkedText: &*NSString::from_str("´"),
                    selectedRange: NSRange::new(1, 0), replacementRange: none()];
                let _: () = msg_send![&*view, unmarkText];
                seen.marked.push(has_marked());
                let mut actual = none();
                let rect: NSRect = msg_send![&*view, firstRectForCharacterRange: NSRange::new(0, 0),
                    actualRange: &mut actual];
                seen.rect = Some(rect);
            }
            seen.frame = view.window().map(|window| {
                window.convertRectToScreen(view.convertRect_toView(view.bounds(), None))
            });
            // Left Option and `e`, a dead key on the US layout, as Meta.
            let left_option = NSEventModifierFlags::Option.0 | 0x20;
            let key = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                NSEventType::KeyDown,
                NSPoint::ZERO,
                NSEventModifierFlags(left_option),
                0.0,
                view.window().expect("the window").windowNumber(),
                None,
                &NSString::from_str(""),
                &NSString::from_str("e"),
                false,
                0x0E,
            )
            .expect("a key event");
            view.window().expect("the window").sendEvent(&key);
            common::wake_after(platform.waker(), 200);
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            // The keys of someone typing at the Mac, which reach the window
            // while it has the focus, are not the probe's.
            let probe_s = match &event {
                WindowEvent::Ime(_) => true,
                WindowEvent::Key(key) => key.code == KeyCode::KeyE,
                _ => false,
            };
            if probe_s {
                self.seen.borrow_mut().events.push(event);
            }
        }
    }

    pub fn main() {
        common::deadline("text_input", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            sent: false,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        let imes: Vec<&Ime> = seen
            .events
            .iter()
            .filter_map(|event| match event {
                WindowEvent::Ime(ime) => Some(ime),
                _ => None,
            })
            .collect();
        assert_eq!(
            imes,
            [
                &Ime::Preedit {
                    text: "にほ".to_owned(),
                    selection: 6..6
                },
                &Ime::Commit("日本".to_owned()),
                &Ime::Preedit {
                    text: "´".to_owned(),
                    selection: 2..2
                },
                &Ime::Commit("´".to_owned()),
            ],
            "{seen:?}"
        );
        assert_eq!(seen.marked, [true, false, false], "{seen:?}");
        // The candidates go 10 points right of the content's left edge and
        // 20 down from its top, in a cell of 8 by 16.
        let (rect, frame) = (seen.rect.expect("a rect"), seen.frame.expect("the frame"));
        assert_eq!((rect.size.width, rect.size.height), (8.0, 16.0), "{seen:?}");
        assert_eq!(rect.origin.x, frame.origin.x + 10.0, "{seen:?}");
        let top = frame.origin.y + frame.size.height;
        assert_eq!(rect.origin.y + rect.size.height, top - 20.0, "{seen:?}");
        // The key with Option as Meta came as a key, uncomposed.
        let keys: Vec<KeyCode> = seen
            .events
            .iter()
            .filter_map(|event| match event {
                WindowEvent::Key(key) => Some(key.code),
                _ => None,
            })
            .collect();
        assert_eq!(keys, [KeyCode::KeyE], "{seen:?}");
        println!("text_input: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

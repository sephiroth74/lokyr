//! The keys that reach a window's view come to the handler as key events:
//! a key down, its repeat and its key up, with what the key types, and a
//! modifier pressed and released, the two Options apart.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, KeyCode, KeyEvent, KeyState, Platform, Size, WindowAttributes,
        WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSString};

    use crate::common;

    /// The bits of a key of each side, as IOKit numbers them.
    const LEFT_SHIFT: usize = 0x0002;
    const RIGHT_OPTION: usize = 0x0040;

    struct Probe {
        keys: Rc<RefCell<Vec<KeyEvent>>>,
        sent: bool,
    }

    /// Sends a key event, as AppKit would, to the app's window.
    fn send(kind: NSEventType, flags: usize, typed: (&str, &str), repeat: bool, code: u16) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let app = NSApplication::sharedApplication(mtm);
        let window = app.windows().firstObject().expect("the probe's window");
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::ZERO,
            NSEventModifierFlags(flags),
            0.0,
            window.windowNumber(),
            None,
            &NSString::from_str(typed.0),
            &NSString::from_str(typed.1),
            repeat,
            code,
        )
        .expect("a key event");
        window.sendEvent(&event);
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform
                .create_window(&WindowAttributes {
                    title: "keys".to_owned(),
                    size: Size {
                        width: 300.0,
                        height: 200.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                })
                .expect("the window opens");
            // The app steps back at once, so that the keys of someone typing
            // at the Mac go to their own app; the probe's go to the window
            // itself.
            let mtm = MainThreadMarker::new().expect("the main thread");
            NSApplication::sharedApplication(mtm).hide(None);
            common::wake_after(platform.waker(), 200);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            // Once the keys have come: nothing comes after the quit.
            if self.sent {
                platform.quit();
                return;
            }
            self.sent = true;
            let shift = NSEventModifierFlags::Shift.0 | LEFT_SHIFT;
            let option = NSEventModifierFlags::Option.0 | RIGHT_OPTION;
            // `a` on the key of `a` on the US layout, which types it on the
            // common Latin layouts too.
            send(NSEventType::KeyDown, 0, ("a", "a"), false, 0x00);
            send(NSEventType::KeyDown, 0, ("a", "a"), true, 0x00);
            send(NSEventType::KeyUp, 0, ("a", "a"), false, 0x00);
            send(NSEventType::KeyDown, shift, ("A", "A"), false, 0x00);
            send(NSEventType::FlagsChanged, option, ("", ""), false, 0x3D);
            send(NSEventType::FlagsChanged, 0, ("", ""), false, 0x3D);
            common::wake_after(platform.waker(), 200);
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            // The keys of someone typing at the Mac, which reach the window
            // while it has the focus, are not the probe's.
            if let WindowEvent::Key(key) = event
                && matches!(key.code, KeyCode::KeyA | KeyCode::AltRight)
            {
                self.keys.borrow_mut().push(key);
            }
        }
    }

    pub fn main() {
        common::deadline("keys", 20);
        let keys = Rc::new(RefCell::new(Vec::new()));
        let probe = Probe {
            keys: Rc::clone(&keys),
            sent: false,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let keys = keys.borrow();
        let seen: Vec<(KeyCode, KeyState, &str, &str, &str)> = keys
            .iter()
            .map(|key| {
                (
                    key.code,
                    key.state,
                    key.text.as_str(),
                    key.unmodified.as_str(),
                    key.plain.as_str(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                (KeyCode::KeyA, KeyState::Pressed, "a", "a", "a"),
                (KeyCode::KeyA, KeyState::Repeated, "a", "a", "a"),
                (KeyCode::KeyA, KeyState::Released, "a", "a", "a"),
                // With Shift held, what the key types without it.
                (KeyCode::KeyA, KeyState::Pressed, "A", "A", "a"),
                (KeyCode::AltRight, KeyState::Pressed, "", "", ""),
                (KeyCode::AltRight, KeyState::Released, "", "", ""),
            ],
            "{keys:?}"
        );
        assert!(keys[3].mods.shift && !keys[3].mods.alt());
        assert!(keys[4].mods.right_alt && !keys[4].mods.left_alt);
        assert!(!keys[5].mods.alt());
        println!("keys: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

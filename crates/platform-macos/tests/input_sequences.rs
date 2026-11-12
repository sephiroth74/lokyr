//! The view hands the events of a key to the app in the order that the
//! input method makes them: a commit before the composition that the same
//! key starts, the key itself after a commit when the input method turns it
//! into a command, once for a key that AppKit's key bindings give two
//! commands, and nothing for what the input method keeps. Past the first
//! keys, a script takes the place of AppKit's `interpretKeyEvents:`, and
//! calls the view as input methods do, so that the test needs no input
//! method and no focus.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::ffi::CStr;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Ime, KeyCode, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::runtime::{AnyObject, Imp, Sel};
    use objc2::{MainThreadMarker, msg_send, sel};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSNotFound, NSPoint, NSRange, NSString};

    use crate::common;

    /// A call that an input method makes on the view.
    #[derive(Clone, Copy, Debug)]
    enum Call {
        Insert(&'static str, NSRange),
        Mark(&'static str),
        Unmark,
        Command(&'static CStr),
    }

    thread_local! {
        /// The calls of the next key.
        static SCRIPT: RefCell<Vec<Call>> = const { RefCell::new(Vec::new()) };
    }

    fn none() -> NSRange {
        NSRange::new(NSNotFound as usize, 0)
    }

    /// The view's `interpretKeyEvents:`, which makes the script's calls.
    unsafe extern "C-unwind" fn scripted(this: &AnyObject, _: Sel, _: &AnyObject) {
        let calls = SCRIPT.with(|script| std::mem::take(&mut *script.borrow_mut()));
        for call in calls {
            // SAFETY: the input client's methods, with their arguments.
            unsafe {
                match call {
                    Call::Insert(text, range) => {
                        let _: () = msg_send![this, insertText: &*NSString::from_str(text),
                            replacementRange: range];
                    }
                    Call::Mark(text) => {
                        let at = text.encode_utf16().count();
                        let _: () = msg_send![this, setMarkedText: &*NSString::from_str(text),
                            selectedRange: NSRange::new(at, 0), replacementRange: none()];
                    }
                    Call::Unmark => {
                        let _: () = msg_send![this, unmarkText];
                    }
                    Call::Command(name) => {
                        let _: () = msg_send![this, doCommandBySelector: Sel::register(name)];
                    }
                }
            }
        }
    }

    /// Gives the window's view the script in place of AppKit's
    /// `interpretKeyEvents:`.
    fn script_the_view() {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window");
        let view = window.contentView().expect("the window's view");
        let class = view.class();
        let function: unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject) = scripted;
        // SAFETY: a method of the view's own class, which has none of this
        // name, of the signature of `interpretKeyEvents:`, which the types
        // string says: nothing back, the receiver, the selector, an object.
        let added = unsafe {
            objc2::ffi::class_addMethod(
                std::ptr::from_ref(class).cast_mut(),
                sel!(interpretKeyEvents:),
                std::mem::transmute::<unsafe extern "C-unwind" fn(&AnyObject, Sel, &AnyObject), Imp>(
                    function,
                ),
                c"v@:@".as_ptr(),
            )
        };
        assert!(added.as_bool(), "the view takes the script");
    }

    /// Sends a key down to the window, which the script handles.
    fn press(code: u16, text: &str, repeat: bool, calls: &[Call]) {
        press_with(code, (text, text), NSEventModifierFlags(0), repeat, calls);
    }

    /// Sends a key down to the window with `flags`, which types `text`, and
    /// the second text without them, and which the script handles.
    fn press_with(
        code: u16,
        (text, unmodified): (&str, &str),
        flags: NSEventModifierFlags,
        repeat: bool,
        calls: &[Call],
    ) {
        SCRIPT.with(|script| *script.borrow_mut() = calls.to_vec());
        let mtm = MainThreadMarker::new().expect("the main thread");
        let window = NSApplication::sharedApplication(mtm)
            .windows()
            .firstObject()
            .expect("the probe's window");
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::ZERO,
            flags,
            0.0,
            window.windowNumber(),
            None,
            &NSString::from_str(text),
            &NSString::from_str(unmodified),
            repeat,
            code,
        )
        .expect("a key event");
        window.sendEvent(&event);
    }

    struct Probe {
        seen: Rc<RefCell<Vec<String>>>,
        sent: bool,
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform
                .create_window(&WindowAttributes {
                    title: "input sequences".to_owned(),
                    size: Size {
                        width: 300.0,
                        height: 200.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("the window opens");
            // The keys of someone typing at the Mac go to their own app.
            let mtm = MainThreadMarker::new().expect("the main thread");
            NSApplication::sharedApplication(mtm).hide(None);
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
            // Through AppKit's own key bindings, before the script takes
            // their place: Control-O, a newline and a move back, and
            // Option-Up, a move back and one to the paragraph's start.
            press_with(
                0x1F,
                ("\u{f}", "o"),
                NSEventModifierFlags::Control,
                false,
                &[],
            );
            let arrow = NSEventModifierFlags::NumericPad.0 | NSEventModifierFlags::Function.0;
            press_with(
                0x7E,
                ("\u{f700}", "\u{f700}"),
                NSEventModifierFlags(NSEventModifierFlags::Option.0 | arrow),
                false,
                &[],
            );
            script_the_view();
            // Korean: a syllable composed, then typed as the next starts.
            press(0x0F, "r", false, &[Call::Mark("한")]);
            press(
                0x0F,
                "r",
                false,
                &[Call::Insert("한", none()), Call::Mark("ㄱ")],
            );
            // Return after a syllable: the syllable, then Return.
            press(
                0x24,
                "\r",
                false,
                &[Call::Insert("ㄱ", none()), Call::Command(c"insertNewline:")],
            );
            // A dead key, then Tab: the dead key's character, then Tab.
            press(0x0E, "", false, &[Call::Mark("´")]);
            press(
                0x30,
                "\t",
                false,
                &[Call::Unmark, Call::Command(c"insertTab:")],
            );
            // An arrow, which types nothing, then plain typing.
            press(0x7B, "\u{f702}", false, &[Call::Command(c"moveLeft:")]);
            press(0x0E, "e", false, &[Call::Insert("e", none())]);
            // A replacement of the `e`, then a repeat that the input method
            // keeps.
            press(0x13, "2", false, &[Call::Insert("é", NSRange::new(0, 1))]);
            press(0x0E, "e", true, &[]);
            common::wake_after(platform.waker(), 200);
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            let seen = match event {
                WindowEvent::Ime(Ime::Commit(text)) => format!("commit {text}"),
                WindowEvent::Ime(Ime::Preedit { text, .. }) => format!("preedit {text}"),
                WindowEvent::Key(key) => format!("key {:?}", key.code),
                _ => return,
            };
            self.seen.borrow_mut().push(seen);
        }
    }

    pub fn main() {
        common::deadline("input_sequences", 20);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            sent: false,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let backspace = format!("key {:?}", KeyCode::Backspace);
        assert_eq!(
            *seen.borrow(),
            [
                "key KeyO",
                "key ArrowUp",
                "preedit 한",
                "commit 한",
                "preedit ㄱ",
                "commit ㄱ",
                "key Enter",
                "preedit ´",
                "commit ´",
                "key Tab",
                "key ArrowLeft",
                "key KeyE",
                &backspace,
                "commit é",
            ]
        );
        println!("input_sequences: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

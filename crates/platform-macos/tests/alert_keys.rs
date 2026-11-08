//! The field of an alert's sheet takes the keys of its editing, ⌘A, ⌘V,
//! ⌘C, ⌘X, ⌘Z and ⇧⌘Z, which the main menu's items of the same keys would
//! otherwise take as their commands, of a layout whose base map is not
//! Latin, as a Russian one. The clipboard is the system's, whose items the
//! probe puts back as it ends.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        Alert, AlertStyle, AppEvent, Handler, MainMenu, Menu, MenuItem, Platform, Shortcut,
        ShortcutKey, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{
        NSApplication, NSButton, NSEvent, NSEventModifierFlags, NSEventType, NSPasteboard,
        NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting, NSTextField, NSView,
    };
    use objc2_foundation::{NSArray, NSData, NSPoint, NSString};

    use crate::common;

    /// What a step does, and what the field can then hold, and the
    /// clipboard: the undo takes the cut back, or the paste with it, as the
    /// system groups the edits of the wakes that come close together.
    const STEPS: [(&str, &[&str], &str); 6] = [
        ("⌘A ⌘V", &["pasted"], "pasted"),
        ("⌘A ⌘C", &["pasted"], "pasted"),
        ("⌘A ⌘X", &[""], "pasted"),
        ("⌘Z", &["pasted", "old"], "pasted"),
        ("⇧⌘Z", &[""], "pasted"),
        ("close", &[""], "pasted"),
    ];

    #[derive(Debug, Default)]
    struct Seen {
        /// The field and the clipboard after each step.
        states: Vec<(String, String)>,
        /// The main menu's commands that ran.
        commands: Vec<String>,
        /// What the answer gave.
        answers: Vec<(usize, Option<String>)>,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        step: usize,
    }

    /// The main menu, of the Edit menu's keys, as the app binds them.
    fn main_menu() -> MainMenu {
        let item = |name: &str, character: char, shift: bool| MenuItem::Command {
            title: name.to_owned(),
            command: format!("test.{name}"),
            shortcut: Some(Shortcut {
                key: ShortcutKey::Char(character),
                command: true,
                shift,
                alt: false,
                control: false,
            }),
        };
        MainMenu {
            menus: vec![
                Menu {
                    title: "Probe".to_owned(),
                    items: Vec::new(),
                    role: None,
                },
                Menu {
                    title: "Edit".to_owned(),
                    items: vec![
                        item("undo", 'z', false),
                        item("redo", 'z', true),
                        item("cut", 'x', false),
                        item("copy", 'c', false),
                        item("paste", 'v', false),
                        item("select-all", 'a', false),
                    ],
                    role: None,
                },
            ],
        }
    }

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

    /// The view of the sheet that is open.
    fn sheet() -> Retained<NSView> {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let windows = NSApplication::sharedApplication(mtm).windows();
        let sheet = (0..windows.count())
            .find_map(|index| windows.objectAtIndex(index).attachedSheet())
            .expect("the alert's sheet");
        sheet.contentView().expect("the sheet's view")
    }

    /// The editable field of the sheet that is open.
    fn field() -> Retained<NSTextField> {
        find::<NSTextField>(&sheet())
            .into_iter()
            .find(|field| field.isEditable())
            .expect("the alert's field")
    }

    /// The character of the key of the Latin `character` in a Russian
    /// layout's base map, which a Command key's characters without its
    /// modifiers take, where its characters take the layout's Command map,
    /// the Latin one.
    fn russian(character: &str) -> &'static str {
        match character {
            "a" => "ф",
            "c" => "с",
            "v" => "м",
            "x" => "ч",
            "z" => "я",
            _ => panic!("no key of {character}"),
        }
    }

    /// Sends the key with Command, and Shift when `shift`, of `character`,
    /// as the system would, to the app, of a Russian layout.
    fn command_key(character: &str, shift: bool) {
        let mtm = MainThreadMarker::new().expect("the main thread");
        let app = NSApplication::sharedApplication(mtm);
        let window = app.keyWindow().expect("the sheet, which is key");
        let mut flags = NSEventModifierFlags::Command;
        if shift {
            flags |= NSEventModifierFlags::Shift;
        }
        let (typed, base) = if shift {
            (character.to_uppercase(), russian(character).to_uppercase())
        } else {
            (character.to_owned(), russian(character).to_owned())
        };
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            NSPoint::ZERO,
            flags,
            0.0,
            window.windowNumber(),
            None,
            &NSString::from_str(&typed),
            &NSString::from_str(&base),
            false,
            0,
        )
        .expect("a key event");
        app.sendEvent(&event);
    }

    fn pasteboard() -> Retained<NSPasteboard> {
        NSPasteboard::generalPasteboard()
    }

    /// The clipboard's text.
    fn clipboard() -> String {
        // SAFETY: a type's constant of AppKit's.
        let string = unsafe { NSPasteboardTypeString };
        pasteboard()
            .stringForType(string)
            .map(|text| text.to_string())
            .unwrap_or_default()
    }

    fn set_clipboard(text: &str) {
        let pasteboard = pasteboard();
        pasteboard.clearContents();
        // SAFETY: a type's constant of AppKit's.
        let string = unsafe { NSPasteboardTypeString };
        pasteboard.setString_forType(&NSString::from_str(text), string);
    }

    /// The clipboard's items, each by its types' data.
    type Items = Vec<Vec<(Retained<NSString>, Retained<NSData>)>>;

    fn saved() -> Items {
        let Some(items) = pasteboard().pasteboardItems() else {
            return Vec::new();
        };
        (0..items.count())
            .map(|index| {
                let item = items.objectAtIndex(index);
                let types = item.types();
                (0..types.count())
                    .filter_map(|index| {
                        let kind = types.objectAtIndex(index);
                        let data = item.dataForType(&kind)?;
                        Some((kind, data))
                    })
                    .collect()
            })
            .collect()
    }

    /// Puts the clipboard's items back as it goes, a panic's unwinding too.
    struct Kept(Items);

    impl Drop for Kept {
        fn drop(&mut self) {
            restore(&self.0);
        }
    }

    fn restore(items: &Items) {
        let pasteboard = pasteboard();
        pasteboard.clearContents();
        let written: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items
            .iter()
            .map(|types| {
                let item = NSPasteboardItem::new();
                for (kind, data) in types {
                    item.setData_forType(data, kind);
                }
                ProtocolObject::from_retained(item)
            })
            .collect();
        if !written.is_empty() {
            pasteboard.writeObjects(&NSArray::from_retained_slice(&written));
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform.set_main_menu(&main_menu());
            let window = platform
                .create_window(&WindowAttributes {
                    title: "An alert's keys".to_owned(),
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
            common::wake_after(platform.waker(), 500);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            match event {
                // A step a wake, so that each edit is a group of its own
                // for the undo.
                AppEvent::Wake => {
                    let Some(&(keys, ..)) = STEPS.get(self.step) else {
                        return;
                    };
                    match keys {
                        "⌘A ⌘V" => {
                            set_clipboard("pasted");
                            command_key("a", false);
                            command_key("v", false);
                        }
                        "⌘A ⌘C" => {
                            set_clipboard("other");
                            command_key("a", false);
                            command_key("c", false);
                        }
                        "⌘A ⌘X" => {
                            command_key("a", false);
                            command_key("x", false);
                        }
                        "⌘Z" => command_key("z", false),
                        "⇧⌘Z" => command_key("z", true),
                        _ => {
                            let button = find::<NSButton>(&sheet())
                                .into_iter()
                                .find(|button| button.title().to_string() == "Set")
                                .expect("the alert's first button");
                            // SAFETY: a click of the alert's button, with no
                            // sender.
                            unsafe { button.performClick(None) };
                        }
                    }
                    if keys != "close" {
                        let text = field().stringValue().to_string();
                        self.seen.borrow_mut().states.push((text, clipboard()));
                        self.step += 1;
                        common::wake_after(platform.waker(), 100);
                    }
                }
                AppEvent::Command(command) => self.seen.borrow_mut().commands.push(command),
                AppEvent::AlertClosed { button, text, .. } => {
                    let mut seen = self.seen.borrow_mut();
                    seen.states
                        .push((text.clone().unwrap_or_default(), clipboard()));
                    seen.answers.push((button, text));
                    platform.quit();
                }
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("alert_keys", 20);
        let kept = Kept(saved());
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            step: 0,
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        drop(kept);
        let seen = seen.borrow();
        assert_eq!(seen.states.len(), STEPS.len(), "{seen:?}");
        for ((field, clipboard), &(keys, fields, wanted)) in seen.states.iter().zip(&STEPS) {
            assert!(fields.contains(&field.as_str()), "{keys}: {seen:?}");
            assert_eq!(clipboard, wanted, "{keys}: {seen:?}");
        }
        assert_eq!(seen.commands, [] as [String; 0], "{seen:?}");
        assert_eq!(seen.answers, [(0, Some(String::new()))], "{seen:?}");
        println!("alert_keys: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

//! A drop on a window's content: the view takes files and text, and a drop
//! of two files, one of a space in its name, then of text, comes to the app
//! as their paths, then as the text; a drop of neither is refused.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Dropped, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSApplication, NSDragOperation, NSDraggingDestination, NSDraggingInfo, NSPasteboard,
        NSPasteboardTypeFileURL, NSPasteboardTypeString, NSPasteboardWriting, NSView,
    };
    use objc2_foundation::{NSArray, NSObject, NSString, NSURL};

    use crate::common;

    pub(crate) struct InfoIvars {
        pasteboard: Retained<NSPasteboard>,
    }

    define_class!(
        // SAFETY: `NSObject` has no requirement of its subclasses that this
        // one breaks.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[ivars = InfoIvars]
        struct Info;

        // A drag's info of the one method that the view calls, of
        // AppKit's signature, rather than the protocol's every method.
        impl Info {
            #[unsafe(method_id(draggingPasteboard))]
            fn dragging_pasteboard(&self) -> Retained<NSPasteboard> {
                self.ivars().pasteboard.clone()
            }
        }
    );

    /// A drag of what `fill` writes into a pasteboard of its own.
    fn info(mtm: MainThreadMarker, fill: impl FnOnce(&NSPasteboard)) -> Retained<Info> {
        let pasteboard = NSPasteboard::pasteboardWithUniqueName();
        pasteboard.clearContents();
        fill(&pasteboard);
        let this = Info::alloc(mtm).set_ivars(InfoIvars { pasteboard });
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    #[derive(Debug, Default)]
    struct Seen {
        registered: Vec<String>,
        operations: Vec<NSDragOperation>,
        performed: Vec<bool>,
        dropped: Vec<Dropped>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    const FILES: [&str; 2] = ["/tmp/a file.txt", "/usr/bin"];

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            platform
                .create_window(&WindowAttributes {
                    title: "A drop".to_owned(),
                    size: Size {
                        width: 300.0,
                        height: 200.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("the window");
            let mtm = MainThreadMarker::new().expect("the main thread");
            let window = NSApplication::sharedApplication(mtm)
                .windows()
                .firstObject()
                .expect("the window");
            let view: Retained<NSView> = window.contentView().expect("the content view");
            let mut seen = self.0.borrow_mut();
            let registered = view.registeredDraggedTypes();
            seen.registered = (0..registered.count())
                .map(|index| registered.objectAtIndex(index).to_string())
                .collect();
            let files = info(mtm, |pasteboard| {
                let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = FILES
                    .iter()
                    .map(|path| {
                        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                        ProtocolObject::from_retained(url)
                    })
                    .collect();
                pasteboard.writeObjects(&NSArray::from_retained_slice(&urls));
            });
            let text = info(mtm, |pasteboard| {
                // SAFETY: AppKit's constant of the text's type.
                let kind = unsafe { NSPasteboardTypeString };
                pasteboard.setString_forType(&NSString::from_str("echo hi"), kind);
            });
            let nothing = info(mtm, |_| {});
            // SAFETY: the view is the window's content view, which takes
            // drags, as the backend makes it.
            let destination: &ProtocolObject<dyn NSDraggingDestination> =
                unsafe { &*std::ptr::from_ref::<NSView>(&view).cast() };
            for drag in [&files, &text, &nothing] {
                // SAFETY: an object that answers the one method of the
                // protocol that the view calls.
                let drag: &ProtocolObject<dyn NSDraggingInfo> =
                    unsafe { &*std::ptr::from_ref::<Info>(drag).cast() };
                seen.operations.push(destination.draggingEntered(drag));
                seen.performed.push(destination.performDragOperation(drag));
            }
            common::wake_after(platform.waker(), 200);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event == AppEvent::Wake {
                platform.quit();
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, event: WindowEvent) {
            if let WindowEvent::Dropped(dropped) = event {
                self.0.borrow_mut().dropped.push(dropped);
            }
        }
    }

    pub fn main() {
        common::deadline("drop", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        // SAFETY: AppKit's constants of the pasteboard's types.
        let kinds = unsafe { [NSPasteboardTypeFileURL, NSPasteboardTypeString] };
        for kind in kinds {
            assert!(seen.registered.contains(&kind.to_string()), "{seen:?}");
        }
        assert_eq!(
            seen.operations,
            [
                NSDragOperation::Copy,
                NSDragOperation::Copy,
                NSDragOperation::None
            ],
            "{seen:?}"
        );
        assert_eq!(seen.performed, [true, true, false], "{seen:?}");
        let paths = FILES.iter().map(PathBuf::from).collect();
        assert_eq!(
            seen.dropped,
            [Dropped::Files(paths), Dropped::Text("echo hi".to_owned())],
            "{seen:?}"
        );
        println!("drop: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

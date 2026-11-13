//! The system's title and the room for the content's small buttons at the
//! titlebar's left: the title stands centred in the window, of no room and
//! of one, past the room; a click over the room goes through to the
//! content; the room goes with a titlebar of tabs and comes back with the
//! title's, the title centred again.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, Titlebar, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::rc::Retained;
    use objc2::{MainThreadMarker, Message};
    use objc2_app_kit::{NSApplication, NSTextField, NSView, NSWindow, NSWindowButton};
    use objc2_foundation::NSPoint;

    use crate::common;

    const TITLE: &str = "Library — zsh — 116×33";

    /// The room that the probe gives, two buttons' of 28 points.
    const ROOM: f64 = 56.0;

    #[derive(Debug, Default)]
    struct Seen {
        /// The title's left edge and middle, the window's middle, and the
        /// window's buttons' right edge, of no room, of the room, and of
        /// the title's titlebar again.
        titles: Vec<(f64, f64, f64, f64)>,
        /// The class of the view that a click over the room reaches.
        clicked: Option<String>,
        /// Whether the room's view was there with the tabs' titlebar.
        room_with_tabs: Option<bool>,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        step: usize,
    }

    /// The view under `view` of the class `name`, if any.
    fn find_class(view: &NSView, name: &str) -> bool {
        if view
            .class()
            .name()
            .to_str()
            .is_ok_and(|class| class == name)
        {
            return true;
        }
        let subviews = view.subviews();
        (0..subviews.count()).any(|index| find_class(&subviews.objectAtIndex(index), name))
    }

    /// The text field under `view` of the title.
    fn title_field(view: &NSView) -> Option<Retained<NSView>> {
        if view
            .downcast_ref::<NSTextField>()
            .is_some_and(|field| field.stringValue().to_string() == TITLE)
        {
            return Some(view.retain());
        }
        let subviews = view.subviews();
        (0..subviews.count()).find_map(|index| title_field(&subviews.objectAtIndex(index)))
    }

    /// The window's frame view, the content's view's parent, which holds the
    /// titlebar's views too.
    fn frame_view(window: &NSWindow) -> Retained<NSView> {
        let content = window.contentView().expect("the content view");
        // SAFETY: the content view of a window that is open has its parent,
        // the window's frame view, which lives as long as the window.
        unsafe { content.superview() }.expect("the frame view")
    }

    /// The title's left edge and middle, the window's middle and its
    /// buttons' right edge, in the window's coordinates.
    fn measure(window: &NSWindow) -> (f64, f64, f64, f64) {
        let frame = frame_view(window);
        frame.layoutSubtreeIfNeeded();
        let field = title_field(&frame).expect("the title's field");
        let rect = field.convertRect_toView(field.bounds(), None);
        let zoom = window
            .standardWindowButton(NSWindowButton::ZoomButton)
            .expect("the zoom button");
        let zoom = zoom.convertRect_toView(zoom.bounds(), None);
        (
            rect.origin.x,
            rect.origin.x + rect.size.width / 2.0,
            window.frame().size.width / 2.0,
            zoom.origin.x + zoom.size.width,
        )
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: TITLE.to_owned(),
                    size: Size {
                        width: 900.0,
                        height: 400.0,
                    },
                    remember: false,
                    restore: false,
                    restore_size: false,
                    frame: None,
                })
                .expect("the window opens");
            self.window = Some(window);
            common::wake_after(platform.waker(), 300);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            let mtm = MainThreadMarker::new().expect("the main thread");
            let window = NSApplication::sharedApplication(mtm)
                .windows()
                .firstObject()
                .expect("the window");
            let id = self.window.expect("the probe's window");
            let mut seen = self.seen.borrow_mut();
            self.step += 1;
            match self.step {
                1 => {
                    seen.titles.push(measure(&window));
                    platform.set_title_buttons_room(id, ROOM);
                }
                2 => {
                    let measured = measure(&window);
                    seen.titles.push(measured);
                    // A click over the room, past the window's buttons.
                    let frame = frame_view(&window);
                    let height = frame.frame().size.height;
                    let at = NSPoint::new(measured.3 + 24.0, height - 14.0);
                    seen.clicked = frame
                        .hitTest(at)
                        .map(|view| view.class().name().to_string_lossy().into_owned());
                    platform.set_titlebar(id, Titlebar::Tabs);
                }
                3 => {
                    let frame = frame_view(&window);
                    seen.room_with_tabs = Some(find_class(&frame, "TitleRoomView"));
                    platform.set_titlebar(id, Titlebar::Titled);
                }
                _ => {
                    seen.titles.push(measure(&window));
                    platform.quit();
                    return;
                }
            }
            common::wake_after(platform.waker(), 300);
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("title_room", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe {
            seen: Rc::clone(&seen),
            window: None,
            step: 0,
        })
        .expect("the application runs");
        let seen = seen.borrow();
        assert_eq!(seen.titles.len(), 3, "{seen:?}");
        for &(_, middle, window_middle, _) in &seen.titles {
            assert!((middle - window_middle).abs() <= 2.0, "centred: {seen:?}");
        }
        let (left, _, _, buttons) = seen.titles[1];
        assert!(left >= buttons + ROOM, "past the room: {seen:?}");
        assert!(
            seen.clicked
                .as_deref()
                .is_some_and(|class| class != "TitleRoomView"),
            "{seen:?}"
        );
        assert_eq!(seen.room_with_tabs, Some(false), "{seen:?}");
        println!("title_room: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

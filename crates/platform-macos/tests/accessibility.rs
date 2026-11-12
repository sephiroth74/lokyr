//! A window's accessibility, as a client, as VoiceOver, asks for it: its
//! content view's children ask the app for its tree, which comes on the
//! run loop's next turn; the tree that the app gives shows as AppKit's
//! elements, a terminal as a text area of its text, which a client finds
//! at its place; and a client's focus of one of its elements comes to the
//! app as an action, the asks and the actions without a wake.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use accesskit::{Action, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};
    use lokyr_platform_core::{
        AppEvent, Handler, Platform, Size, WindowAttributes, WindowEvent, WindowId,
    };
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadMarker, msg_send};
    use objc2_app_kit::{NSApplication, NSView};
    use objc2_foundation::{NSArray, NSPoint, NSString};

    use crate::common;

    #[derive(Debug, Default)]
    struct Seen {
        /// Whether the platform told of an ask before a client asked.
        asked_before: bool,
        /// The asks for the tree, and the actions, that came.
        asks: usize,
        actions: Vec<(Action, NodeId)>,
        /// The roles and the values of the elements that the client found,
        /// after the tree came.
        elements: Vec<(String, Option<String>)>,
        /// The role of the element at the view's middle, as a client finds
        /// it.
        middle: Option<String>,
        /// The wakes that came before the time that the probe asked for
        /// them, as a client's ask would bring.
        early: usize,
    }

    struct Probe {
        seen: Rc<RefCell<Seen>>,
        window: Option<WindowId>,
        step: usize,
        /// When the probe asked for the next wake.
        asked: Instant,
    }

    /// The time between a wake asked and its coming, at least.
    const WAKE: Duration = Duration::from_millis(300);

    impl Probe {
        /// Asks for the next step's wake.
        fn wake_after(&mut self, platform: &dyn Platform) {
            self.asked = Instant::now();
            common::wake_after(platform.waker(), WAKE.as_millis() as u64);
        }
    }

    const TERMINAL: NodeId = NodeId(2);

    /// A window of a terminal of one line.
    fn tree() -> TreeUpdate {
        let mut window = Node::new(Role::Window);
        window.set_label("The accessibility");
        window.set_children(vec![TERMINAL]);
        let mut terminal = Node::new(Role::Terminal);
        terminal.set_label("Terminal");
        terminal.set_bounds(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 10_000.0,
            y1: 10_000.0,
        });
        terminal.set_children(vec![NodeId(3)]);
        terminal.add_action(Action::Focus);
        let mut run = Node::new(Role::TextRun);
        run.set_value("hello\n");
        run.set_character_lengths(vec![1; 6]);
        TreeUpdate {
            nodes: vec![(NodeId(1), window), (TERMINAL, terminal), (NodeId(3), run)],
            tree: Some(TreeInfo::new(NodeId(1))),
            tree_id: TreeId::ROOT,
            focus: TERMINAL,
        }
    }

    /// The content view's accessibility children, as a client asks.
    fn children(content: &NSView) -> Vec<Retained<AnyObject>> {
        // SAFETY: `NSAccessibility`'s method, of this signature.
        let array: Option<Retained<NSArray<AnyObject>>> =
            unsafe { msg_send![content, accessibilityChildren] };
        array.map_or_else(Vec::new, |array| array.to_vec())
    }

    /// The role of an element.
    fn role(element: &AnyObject) -> String {
        // SAFETY: `NSAccessibility`'s method, of this signature.
        let role: Option<Retained<NSString>> = unsafe { msg_send![element, accessibilityRole] };
        role.map(|role| role.to_string()).unwrap_or_default()
    }

    /// An element and its descendants, depth first.
    fn walk(element: &AnyObject, found: &mut Vec<Retained<AnyObject>>) {
        // SAFETY: `NSAccessibility`'s methods, of these signatures.
        unsafe {
            let this: Option<Retained<AnyObject>> = msg_send![element, self];
            found.extend(this);
            let children: Option<Retained<NSArray<AnyObject>>> =
                msg_send![element, accessibilityChildren];
            for child in children.map_or_else(Vec::new, |children| children.to_vec()) {
                walk(&child, found);
            }
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let window = platform
                .create_window(&WindowAttributes {
                    title: "The accessibility".to_owned(),
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
            self.seen.borrow_mut().asked_before = platform.accessibility_asked(window);
            self.wake_after(platform);
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            if event != AppEvent::Wake {
                return;
            }
            if self.asked.elapsed() < WAKE {
                self.seen.borrow_mut().early += 1;
            }
            let mtm = MainThreadMarker::new().expect("the main thread");
            let ns_window = NSApplication::sharedApplication(mtm)
                .windows()
                .firstObject()
                .expect("the window");
            let content: Retained<NSView> = ns_window.contentView().expect("the content view");
            self.step += 1;
            match self.step {
                // A client asks, of a placeholder; the app hears on the
                // next turn.
                1 => {
                    let _ = children(&content);
                    self.wake_after(platform);
                }
                // The tree came: its elements, and a client's focus of the
                // terminal's.
                2 => {
                    let mut found = Vec::new();
                    for child in children(&content) {
                        walk(&child, &mut found);
                    }
                    let mut seen = self.seen.borrow_mut();
                    for element in &found {
                        // SAFETY: `NSAccessibility`'s methods, of these
                        // signatures.
                        let value: Option<Retained<AnyObject>> =
                            unsafe { msg_send![&**element, accessibilityValue] };
                        let value = value
                            .and_then(|value| value.downcast::<NSString>().ok())
                            .map(|value| value.to_string());
                        let role = role(element);
                        if role == "AXTextArea" {
                            // SAFETY: as above.
                            let () =
                                unsafe { msg_send![&**element, setAccessibilityFocused: true] };
                        }
                        seen.elements.push((role, value));
                    }
                    // The view's middle, on the screen.
                    let size = content.frame().size;
                    let middle = content.convertPoint_toView(
                        NSPoint::new(size.width / 2.0, size.height / 2.0),
                        None,
                    );
                    let middle = ns_window.convertPointToScreen(middle);
                    // SAFETY: `NSAccessibility`'s method, of this signature.
                    let element: Option<Retained<AnyObject>> =
                        unsafe { msg_send![&*content, accessibilityHitTest: middle] };
                    seen.middle = element.as_deref().map(role);
                    drop(seen);
                    self.wake_after(platform);
                }
                _ => platform.quit(),
            }
        }

        fn window_event(
            &mut self,
            platform: &mut dyn Platform,
            window: WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::AccessibilityAsked => {
                    self.seen.borrow_mut().asks += 1;
                    assert!(platform.accessibility_asked(window));
                    platform.update_accessibility(window, tree());
                }
                WindowEvent::AccessibilityAction(request) => {
                    self.seen
                        .borrow_mut()
                        .actions
                        .push((request.action, request.target_node));
                }
                _ => {}
            }
        }
    }

    pub fn main() {
        common::deadline("accessibility", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        let probe = Probe {
            seen: Rc::clone(&seen),
            window: None,
            step: 0,
            asked: Instant::now(),
        };
        lokyr_platform_macos::run(probe).expect("the application runs");
        let seen = seen.borrow();
        // Nothing before a client asked, then one ask.
        assert!(!seen.asked_before, "{seen:?}");
        assert_eq!(seen.asks, 1, "{seen:?}");
        // The ask, and the action, come without a wake.
        assert_eq!(seen.early, 0, "{seen:?}");
        // The terminal a text area, of its text.
        assert!(
            seen.elements
                .iter()
                .any(|(role, value)| role == "AXTextArea" && value.as_deref() == Some("hello\n")),
            "{seen:?}"
        );
        // The element at a point, the terminal's.
        assert_eq!(seen.middle.as_deref(), Some("AXTextArea"), "{seen:?}");
        // The client's focus, an action of the app's.
        assert_eq!(seen.actions, [(Action::Focus, TERMINAL)], "{seen:?}");
        println!("accessibility: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

//! Windows, their content view, and the delegate that tells what happens
//! to them.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

use lokyr_platform_core::{
    Backdrop, Ime, Layer, LayerPlace, MouseButton, PixelSize, Point, PointerShape, ScrollDelta,
    ScrollPhase, Size, WindowAttributes, WindowEvent, WindowId, WindowSize,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSCursor, NSEvent, NSEventPhase, NSEventType, NSResponder,
    NSScreen, NSTextInputClient, NSTitlebarSeparatorStyle, NSView, NSWindow, NSWindowDelegate,
    NSWindowOcclusionState, NSWindowStyleMask, NSWindowTabbingMode,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSNotFound, NSNotification, NSObject,
    NSObjectProtocol, NSPoint, NSRange, NSRangePointer, NSRect, NSSize, NSString, NSUInteger,
    NSUserDefaults,
};

use objc2_quartz_core::{CALayer, CAMetalLayer};

use crate::frame::Pacer;
use crate::keys;
use crate::layer::LayerView;
use crate::shared::{Pending, Shared};
use crate::text_input::{self, Handled, Step, TextInput};

/// A window that the app opened.
pub(crate) struct Window {
    ns_window: Retained<NSWindow>,
    view: Retained<ContentView>,
    /// The view of the text that blinks, over the content's, and of the
    /// cursor's cells, over everything.
    blinking_text: Retained<LayerView>,
    cursor: Retained<LayerView>,
    pacer: Pacer,
    /// Whether the window is hidden or covered, as its delegate tells, and
    /// whether it asked for a frame meanwhile.
    occluded: Rc<Cell<bool>>,
    frame_held: Cell<bool>,
    /// Whether it opened at the frame that a window kept.
    restored: bool,
    // The window holds its delegate weakly.
    delegate: Retained<WindowDelegate>,
}

impl Window {
    /// A window of the content size and the title of `attributes`, centred,
    /// and not yet shown.
    pub(crate) fn new(
        mtm: MainThreadMarker,
        shared: Weak<Shared>,
        id: WindowId,
        attributes: &WindowAttributes,
        option_as_meta: Rc<Cell<(bool, bool)>>,
    ) -> Self {
        let below = NSRect::new(
            NSPoint::ZERO,
            NSSize::new(attributes.size.width, attributes.size.height),
        );
        let titled = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // The content runs under the titlebar, so that the terminal's
        // background is the bar's too: it is the bar's height taller, for
        // the size asked to be the one below the bar.
        let bar = NSWindow::frameRectForContentRect_styleMask(below, titled, mtm)
            .size
            .height
            - below.size.height;
        let content = NSRect::new(
            NSPoint::ZERO,
            NSSize::new(below.size.width, below.size.height + bar.max(0.0)),
        );
        let style = titled | NSWindowStyleMask::FullSizeContentView;
        // SAFETY: a new window, of a valid style, which `setReleasedWhenClosed`
        // below keeps from being released twice.
        let ns_window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                content,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: `Retained` owns the window, which closing must then not
        // release.
        unsafe { ns_window.setReleasedWhenClosed(false) };
        ns_window.setTitle(&NSString::from_str(&attributes.title));
        // The bar shows what the view draws under it, without the system's
        // material nor its line; the system's tabs are not the app's.
        ns_window.setTitlebarAppearsTransparent(true);
        ns_window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
        ns_window.setTabbingMode(NSWindowTabbingMode::Disallowed);
        let view = ContentView::new(mtm, content, shared.clone(), id, option_as_meta);
        ns_window.setContentView(Some(&view));
        let blinking_text = LayerView::new(mtm, &view);
        let cursor = LayerView::new(mtm, &view);
        // The keys go to the view.
        ns_window.makeFirstResponder(Some(&view));
        let pacer = Pacer::new(mtm, &view, shared.clone(), id);
        let occluded = Rc::new(Cell::new(false));
        let delegate = WindowDelegate::new(
            mtm,
            shared,
            id,
            (ns_window.backingScaleFactor(), Rc::clone(&occluded)),
        );
        ns_window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        // The frame that a window kept, on a screen that is still there,
        // rather than the size asked for.
        let restored = attributes.restore && restore_frame(&ns_window);
        Self {
            ns_window,
            view,
            blinking_text,
            cursor,
            pacer,
            occluded,
            frame_held: Cell::new(false),
            restored,
            delegate,
        }
    }

    /// The window's content view, by its address, for a renderer: it lives
    /// as long as the window.
    pub(crate) fn view_address(&self) -> NonNull<c_void> {
        NonNull::from(&*self.view).cast()
    }

    /// The view of `layer`.
    fn layer(&self, layer: Layer) -> &LayerView {
        match layer {
            Layer::BlinkingText => &self.blinking_text,
            Layer::Cursor => &self.cursor,
        }
    }

    /// The view of `layer`, by its address, for a renderer: it lives as long
    /// as the window.
    pub(crate) fn layer_address(&self, layer: Layer) -> NonNull<c_void> {
        self.layer(layer).address()
    }

    /// Places the view of `layer`, blinking as `place` says, or hides it.
    pub(crate) fn place_layer(&self, layer: Layer, place: Option<&LayerPlace>) {
        self.layer(layer)
            .place(place, self.view.bounds().size.height);
    }

    /// Places the window down and to the right of a window of the frame
    /// `after`, as the system cascades windows, within the screen; or in the
    /// screen's centre.
    pub(crate) fn place(&self, after: Option<NSRect>) {
        match after {
            Some(frame) => {
                let top_left = NSPoint::new(frame.origin.x, frame.origin.y + frame.size.height);
                // The first call puts the window at the point, and gives the
                // next one's.
                let next = self.ns_window.cascadeTopLeftFromPoint(top_left);
                self.ns_window.cascadeTopLeftFromPoint(next);
            }
            None => self.ns_window.center(),
        }
    }

    /// Whether the window keeps its frame as the user moves or resizes it
    /// from now: the backend turns it on once it placed the window, so
    /// that its own placing keeps nothing.
    pub(crate) fn set_remember(&self, remember: bool) {
        self.delegate.ivars().remember.set(remember);
    }

    /// The window's content view.
    pub(crate) fn view(&self) -> Retained<NSView> {
        Retained::into_super(self.view.clone())
    }

    pub(crate) fn ns_window(&self) -> &NSWindow {
        &self.ns_window
    }

    /// Brings the window in front of the others, with the focus.
    pub(crate) fn show(&self) {
        self.ns_window.makeKeyAndOrderFront(None);
    }

    /// Asks for a frame. Returns whether it is due at once, rather than at
    /// the display's next refresh.
    pub(crate) fn request_frame(&self) -> bool {
        // A window that does not show gets its frame as it shows again.
        if self.occluded.get() {
            self.frame_held.set(true);
            return false;
        }
        self.pacer.request()
    }

    /// The window shows again: the frame that it asked for meanwhile, if
    /// one; whether it comes at once, as [`Window::request_frame`] tells.
    pub(crate) fn request_held_frame(&self) -> bool {
        self.frame_held.replace(false) && self.request_frame()
    }

    /// Whether it opened at the frame that a window kept, rather than
    /// where the backend places it.
    pub(crate) fn is_restored(&self) -> bool {
        self.restored
    }

    /// Tells the pacer that the frame came.
    pub(crate) fn frame_delivered(&self) {
        self.pacer.delivered();
    }

    pub(crate) fn close(&self) {
        self.pacer.stop();
        // What the window does as it closes, such as losing the focus, has
        // no window left to tell it to.
        self.ns_window.setDelegate(None);
        if let Some(sheet) = self.ns_window.attachedSheet() {
            self.ns_window.endSheet(&sheet);
        }
        self.ns_window.close();
    }

    pub(crate) fn set_title(&self, title: &str) {
        self.ns_window.setTitle(&NSString::from_str(title));
    }

    pub(crate) fn size(&self) -> WindowSize {
        size_of(&self.ns_window)
    }

    pub(crate) fn is_focused(&self) -> bool {
        self.ns_window.isKeyWindow()
    }

    /// Where the typed text goes, for the input method's candidates.
    pub(crate) fn set_text_area(&self, origin: Point, size: Size) {
        self.view.ivars().input.borrow_mut().area = (origin, size);
    }

    /// Whether the pointer's moves without a button come to the view.
    pub(crate) fn track_mouse_moves(&self, track: bool) {
        self.ns_window.setAcceptsMouseMovedEvents(track);
    }

    /// Gives the pointer over the content `shape`, through the view's
    /// cursor rectangle, which AppKit asks for again.
    pub(crate) fn set_pointer_shape(&self, shape: PointerShape) {
        if self.view.ivars().pointer.replace(shape) != shape {
            self.ns_window.invalidateCursorRectsForView(&self.view);
        }
    }

    /// Makes the window translucent over `backdrop`, its background, which
    /// fills its whole shape, the titlebar and the rounded corners too,
    /// under the content that the renderer draws clear; or, with `None`,
    /// keeps it opaque. The window, its view and the view's layer turn
    /// non-opaque together, as the window server composes the window with
    /// what is behind it only then.
    pub(crate) fn set_translucent(&self, backdrop: Option<Backdrop>) {
        let opaque = backdrop.is_none();
        self.view.ivars().opaque.set(opaque);
        if let Some(layer) = self.view.layer() {
            layer.setOpaque(opaque);
        }
        self.ns_window.setOpaque(opaque);
        let background = match backdrop {
            Some(Backdrop {
                color: [red, green, blue, alpha],
                ..
            }) => NSColor::colorWithSRGBRed_green_blue_alpha(red, green, blue, alpha),
            None => NSColor::windowBackgroundColor(),
        };
        self.ns_window.setBackgroundColor(Some(&background));
        let radius = backdrop.map_or(0, |backdrop| backdrop.blur.round() as isize);
        // SAFETY: the window server's calls take its connection, a window's
        // number and a radius; a window of the process, open, is its own.
        // Each argument goes as a word, which the call reads whole or in its
        // lower half, whichever its width.
        unsafe {
            CGSSetWindowBackgroundBlurRadius(
                CGSMainConnectionID() as isize,
                self.ns_window.windowNumber(),
                radius,
            );
        }
        // The shadow follows the window's new outline.
        self.ns_window.invalidateShadow();
    }
}

// The window server's calls of the blur behind a window, private ones of
// Core Graphics, which the terminals of macOS that blur make too: there is
// no public one.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// The process's connection to the window server.
    fn CGSMainConnectionID() -> i32;
    /// Blurs what is behind the non-opaque parts of `window`, by a radius
    /// of `radius` points; 0 for none.
    fn CGSSetWindowBackgroundBlurRadius(connection: isize, window: isize, radius: isize) -> i32;
}

/// Where a trackpad's gesture, or its momentum, is in `phase`; `None` for
/// a wheel's step, which has no phase.
fn scroll_phase(phase: NSEventPhase) -> Option<ScrollPhase> {
    let has = |flag: NSEventPhase| phase.0 & flag.0 != 0;
    if has(NSEventPhase::Began) || has(NSEventPhase::MayBegin) {
        Some(ScrollPhase::Started)
    } else if has(NSEventPhase::Changed) || has(NSEventPhase::Stationary) {
        Some(ScrollPhase::Moved)
    } else if has(NSEventPhase::Ended) || has(NSEventPhase::Cancelled) {
        Some(ScrollPhase::Ended)
    } else {
        None
    }
}

/// The size of a window's content, in points and in the pixels of its
/// backing store.
fn size_of(window: &NSWindow) -> WindowSize {
    let scale = window.backingScaleFactor();
    let Some(view) = window.contentView() else {
        let frame = window.contentRectForFrameRect(window.frame());
        return WindowSize::new(points(frame.size), scale);
    };
    let bounds = view.bounds();
    let backing = view.convertRectToBacking(bounds);
    // The content runs under the titlebar, over the layout's rectangle.
    let bar = (bounds.size.height - window.contentLayoutRect().size.height).max(0.0);
    WindowSize {
        points: points(bounds.size),
        pixels: PixelSize {
            // A cast from a float saturates, and takes NaN to 0.
            width: backing.size.width.round() as u32,
            height: backing.size.height.round() as u32,
        },
        scale,
        bar,
    }
}

fn points(size: NSSize) -> Size {
    Size {
        width: size.width,
        height: size.height,
    }
}

pub(crate) struct ContentViewIvars {
    shared: Weak<Shared>,
    id: WindowId,
    /// The view's side of the input method.
    input: RefCell<TextInput>,
    /// Which Option keys act as Meta, as the app said for every window.
    option_as_meta: Rc<Cell<(bool, bool)>>,
    /// Whether the view is opaque, as the window is unless the app made it
    /// translucent.
    opaque: Cell<bool>,
    /// How the pointer looks over the view.
    pointer: Cell<PointerShape>,
    /// Whether the buttons held went down over the titlebar, whose drags
    /// and releases are the window's, not the content's.
    over_bar: Cell<bool>,
}

define_class!(
    // SAFETY: `NSView` has no requirement of its subclasses that this one
    // breaks, and the view does not implement `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ContentViewIvars]
    pub(crate) struct ContentView;

    impl ContentView {
        // SAFETY: AppKit's signature.
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        // SAFETY: AppKit's signature. AppKit shows the cursor of the
        // rectangle that the pointer is in, in the key window.
        #[unsafe(method(resetCursorRects))]
        fn reset_cursor_rects(&self) {
            let cursor = match self.ivars().pointer.get() {
                PointerShape::Arrow => NSCursor::arrowCursor(),
                PointerShape::Text => NSCursor::IBeamCursor(),
            };
            // Below the titlebar, whose pointer is the arrow.
            self.addCursorRect_cursor(self.below_bar(), &cursor);
        }

        // SAFETY: AppKit's signature. The keys that the menus' shortcuts
        // take never come here.
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            self.key_down_through_input_method(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            self.post_key(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            self.post_key(event);
        }

        // SAFETY: AppKit's signature, for each of the mouse's buttons. A
        // press over the titlebar is the window's, which it drags, or which
        // a double click zooms: the content learns of nothing, until every
        // button is up.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        // SAFETY: AppKit's signature. The view takes the right button,
        // which would otherwise open its contextual menu.
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        // SAFETY: AppKit's signature, for a move with no button held, which
        // comes while the window accepts them; one over the titlebar is
        // the window's.
        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            if !self.is_over_bar(event) {
                self.post_moved(event);
            }
        }

        // SAFETY: AppKit's signature, for each button held.
        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.drag(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.drag(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.drag(event);
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.post_scroll(event);
        }

        // SAFETY: AppKit's signature: the layer that backs the view.
        #[unsafe(method_id(makeBackingLayer))]
        fn make_backing_layer(&self) -> Retained<CALayer> {
            let layer = CAMetalLayer::new();
            layer.setOpaque(true);
            Retained::into_super(layer)
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(wantsUpdateLayer))]
        fn wants_update_layer(&self) -> bool {
            // The renderer draws the layer; AppKit draws nothing into it.
            true
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            // SAFETY: `NSView`'s method, of this signature.
            let () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
            // AppKit leaves the scale of the layer that the view made to the
            // view: it takes the window's, as the view joins a window and as
            // the window's display changes.
            if let (Some(window), Some(layer)) = (self.window(), self.layer()) {
                layer.setContentsScale(window.backingScaleFactor());
            }
        }

        // SAFETY: AppKit's signature.
        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            self.ivars().opaque.get()
        }
    }

    // SAFETY: `NSTextInputClient` has no requirement; each method has
    // AppKit's signature, and those that take a pointer check it.
    unsafe impl NSTextInputClient for ContentView {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, replacement: NSRange) {
            let text = text_input::text_of(string);
            let mut input = self.ivars().input.borrow_mut();
            let deleted = input.insert(&text, replacement);
            self.handle(input, Step::Inserted { text, deleted });
        }

        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: Sel) {
            // The key does not type text: the view sends it as a key once
            // the input method has returned. AppKit's own handling of the
            // selector would beep.
            let mut input = self.ivars().input.borrow_mut();
            let composing = input.marked.is_some();
            if let Some(handled) = &mut input.handling {
                handled.steps.push(Step::Command { composing });
            }
        }

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, string: &AnyObject, selected: NSRange, _replacement: NSRange) {
            let text = text_input::text_of(string);
            let selection = text_input::bytes(&text, selected);
            let mut input = self.ivars().input.borrow_mut();
            input.marked = (!text.is_empty()).then(|| (text.clone(), selected));
            self.handle(input, Step::Marked { text, selection });
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            // The text composed so far is typed, as AppKit asks.
            let mut input = self.ivars().input.borrow_mut();
            if let Some((text, _)) = input.marked.clone() {
                input.insert(&text, text_input::not_found());
                self.handle(input, Step::Inserted { text, deleted: 0 });
            }
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            let input = self.ivars().input.borrow();
            let typed = text_input::utf16_len(&input.typed);
            match &input.marked {
                Some((_, selected)) => NSRange::new(typed + selected.location, selected.length),
                None => NSRange::new(typed, 0),
            }
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            let input = self.ivars().input.borrow();
            match &input.marked {
                Some((text, _)) => NSRange::new(
                    text_input::utf16_len(&input.typed),
                    text_input::utf16_len(text),
                ),
                None => text_input::not_found(),
            }
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            self.ivars().input.borrow().marked.is_some()
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(
            &self,
            range: NSRange,
            actual: NSRangePointer,
        ) -> Option<Retained<NSAttributedString>> {
            self.substring(range, actual)
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes_for_marked_text(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect_for_character_range(&self, range: NSRange, actual: NSRangePointer) -> NSRect {
            if !actual.is_null() {
                // SAFETY: AppKit's pointer, which it checked, to a range.
                unsafe { *actual = range };
            }
            self.text_rect_on_screen()
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index_for_point(&self, _point: NSPoint) -> NSUInteger {
            NSNotFound as NSUInteger
        }
    }
);

impl ContentView {
    /// A view of the size of `frame`, backed by a `CAMetalLayer`, which
    /// tells `shared` the keys of the window `id`.
    fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        shared: Weak<Shared>,
        id: WindowId,
        option_as_meta: Rc<Cell<(bool, bool)>>,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ContentViewIvars {
            shared,
            id,
            input: RefCell::new(TextInput::default()),
            option_as_meta,
            opaque: Cell::new(true),
            pointer: Cell::new(PointerShape::Arrow),
            over_bar: Cell::new(false),
        });
        // SAFETY: `NSView`'s `initWithFrame:` has this signature.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        view.setWantsLayer(true);
        view
    }

    fn post(&self, event: WindowEvent) {
        let ivars = self.ivars();
        if let Some(shared) = ivars.shared.upgrade() {
            shared.post(Pending::Window(ivars.id, event));
        }
    }

    fn post_key(&self, event: &NSEvent) {
        if let Some(key) = keys::key_event(event) {
            self.post(WindowEvent::Key(key));
        }
    }

    /// Where `event` happened, from the top left corner of the view, in
    /// points.
    fn position(&self, event: &NSEvent) -> Point {
        let local = self.convertPoint_fromView(event.locationInWindow(), None);
        Point {
            x: local.x,
            // The view's own coordinates go up from its bottom edge.
            y: self.bounds().size.height - local.y,
        }
    }

    fn post_button(&self, event: &NSEvent, pressed: bool) {
        // The left and the right buttons by the event's type, the others by
        // their numbers, as the event's type names them all alike.
        let button = match event.r#type() {
            NSEventType::LeftMouseDown | NSEventType::LeftMouseUp => MouseButton::Left,
            NSEventType::RightMouseDown | NSEventType::RightMouseUp => MouseButton::Right,
            _ => match event.buttonNumber() {
                2 => MouseButton::Middle,
                3 => MouseButton::Back,
                4 => MouseButton::Forward,
                other => MouseButton::Other(u16::try_from(other).unwrap_or(u16::MAX)),
            },
        };
        self.post(WindowEvent::MouseButton {
            button,
            pressed,
            position: self.position(event),
            mods: keys::modifiers(event.modifierFlags()),
            clicks: u32::try_from(event.clickCount()).unwrap_or(0),
        });
    }

    /// The view's part below the titlebar, which the content runs under.
    fn below_bar(&self) -> NSRect {
        let bounds = self.bounds();
        let Some(window) = self.window() else {
            return bounds;
        };
        let layout = self.convertRect_fromView(window.contentLayoutRect(), None);
        NSRect::new(
            bounds.origin,
            NSSize::new(
                bounds.size.width,
                (layout.origin.y + layout.size.height - bounds.origin.y)
                    .clamp(0.0, bounds.size.height),
            ),
        )
    }

    /// Whether `event` is over the titlebar: above the part below it, in
    /// the view, whose origin is its bottom left corner.
    fn is_over_bar(&self, event: &NSEvent) -> bool {
        let local = self.convertPoint_fromView(event.locationInWindow(), None);
        let below = self.below_bar();
        local.y >= below.origin.y + below.size.height
    }

    /// A button went down: over the titlebar, the window's, which the
    /// content learns nothing of, with the drags and the releases that
    /// follow; otherwise the content's. As the titlebar's drag can keep its
    /// release, each press tells anew.
    fn press(&self, event: &NSEvent) {
        let over = self.is_over_bar(event);
        self.ivars().over_bar.set(over);
        if !over {
            self.post_button(event, true);
            return;
        }
        // The window's frame does neither for the content that runs under
        // its bar: the left button drags the window, as the system does,
        // and a double click does what the system's settings say.
        if event.r#type() == NSEventType::LeftMouseDown
            && let Some(window) = self.window()
        {
            if event.clickCount() == 2 {
                match bar_double_click() {
                    BarDoubleClick::Zoom => window.performZoom(None),
                    BarDoubleClick::Minimize => window.performMiniaturize(None),
                    BarDoubleClick::Nothing => {}
                }
            } else {
                window.performWindowDragWithEvent(event);
            }
        }
    }

    /// A button went up: the content's, unless the buttons went down over
    /// the titlebar, which they no longer are once every one is up.
    fn release(&self, event: &NSEvent) {
        let ivars = self.ivars();
        if ivars.over_bar.get() {
            if NSEvent::pressedMouseButtons() == 0 {
                ivars.over_bar.set(false);
            }
            return;
        }
        self.post_button(event, false);
    }

    /// The pointer moved with buttons held: the content's, unless they went
    /// down over the titlebar.
    fn drag(&self, event: &NSEvent) {
        if !self.ivars().over_bar.get() {
            self.post_moved(event);
        }
    }

    fn post_moved(&self, event: &NSEvent) {
        self.post(WindowEvent::MouseMoved {
            position: self.position(event),
            mods: keys::modifiers(event.modifierFlags()),
        });
    }

    fn post_scroll(&self, event: &NSEvent) {
        // AppKit's deltas go toward the content's top and left edge when
        // positive, natural scrolling applied.
        let (x, y) = (event.scrollingDeltaX(), event.scrollingDeltaY());
        let delta = if event.hasPreciseScrollingDeltas() {
            ScrollDelta::Points { x, y }
        } else {
            ScrollDelta::Lines { x, y }
        };
        let momentum = event.momentumPhase();
        let moving = momentum.0 != NSEventPhase::None.0;
        let phase = scroll_phase(if moving { momentum } else { event.phase() });
        self.post(WindowEvent::Scroll {
            delta,
            phase,
            momentum: moving,
            position: self.position(event),
            mods: keys::modifiers(event.modifierFlags()),
        });
    }

    /// The part `range` of the document that the input method reads, which
    /// it also writes into `actual`: the text typed last, which press and
    /// hold reads; the screen's is the program's, which the input method
    /// does not read.
    fn substring(
        &self,
        range: NSRange,
        actual: NSRangePointer,
    ) -> Option<Retained<NSAttributedString>> {
        let document = self.ivars().input.borrow().document();
        let bytes = text_input::bytes(&document, range);
        if bytes.is_empty() {
            return None;
        }
        let text = &document[bytes.clone()];
        if !actual.is_null() {
            let location = text_input::utf16_len(&document[..bytes.start]);
            // SAFETY: AppKit's pointer, which it checked, to a range.
            unsafe { *actual = NSRange::new(location, text_input::utf16_len(text)) };
        }
        Some(NSAttributedString::from_nsstring(&NSString::from_str(text)))
    }

    /// Keeps `step` for the key that the view handles, or, when the input
    /// method acts on its own, as a click in its candidates does, sends it
    /// at once. `input` is the view's, borrowed.
    fn handle(&self, mut input: std::cell::RefMut<'_, TextInput>, step: Step) {
        if let Some(handled) = &mut input.handling {
            handled.steps.push(step);
            return;
        }
        drop(input);
        match step {
            Step::Inserted { text, deleted } => {
                for _ in 0..deleted {
                    self.post(WindowEvent::Key(text_input::backspace()));
                }
                self.post(WindowEvent::Ime(Ime::Commit(text)));
            }
            Step::Marked { text, selection } => {
                self.post(WindowEvent::Ime(Ime::Preedit { text, selection }));
            }
            Step::Command { .. } => {}
        }
    }

    /// Hands a key down to the input method, which composes or types with
    /// it, and sends it as a key when the input method typed what the key
    /// types, as plain typing does, or nothing, as for the keys that do not
    /// type, so that the encoder sees every key that a program may report.
    /// A key with Command, or with an Option that acts as Meta, goes as it
    /// is, unless the input method composes.
    fn key_down_through_input_method(&self, event: &NSEvent) {
        let Some(key) = keys::key_event(event) else {
            return;
        };
        let ivars = self.ivars();
        let composing = ivars.input.borrow().marked.is_some();
        let (left, right) = ivars.option_as_meta.get();
        let meta = (key.mods.left_alt && left) || (key.mods.right_alt && right);
        if !composing && (key.mods.super_key || meta) {
            self.post(WindowEvent::Key(key));
            return;
        }
        ivars.input.borrow_mut().handling = Some(Handled::default());
        self.interpretKeyEvents(&NSArray::from_slice(&[event]));
        let handled = ivars.input.borrow_mut().handling.take().unwrap_or_default();
        let still = ivars.input.borrow().marked.is_some();
        // A key that types nothing starts the text typed anew.
        let typed_nothing = handled.steps.is_empty()
            || handled
                .steps
                .iter()
                .any(|step| matches!(step, Step::Command { composing: false }));
        if typed_nothing && !composing && !still {
            ivars.input.borrow_mut().typed.clear();
        }
        for event in text_input::events(key, composing, handled.steps, still) {
            self.post(event);
        }
    }

    /// Where the typed text goes, on the screen, as AppKit's coordinates
    /// have it: from the bottom left corner of the main display, upwards.
    fn text_rect_on_screen(&self) -> NSRect {
        let (origin, size) = self.ivars().input.borrow().area;
        let height = self.bounds().size.height;
        // The view's own coordinates go up from its bottom edge.
        let in_view = NSRect::new(
            NSPoint::new(origin.x, height - origin.y - size.height),
            NSSize::new(size.width, size.height),
        );
        let in_window = self.convertRect_toView(in_view, None);
        match self.window() {
            Some(window) => window.convertRectToScreen(in_window),
            None => in_window,
        }
    }
}

pub(crate) struct WindowDelegateIvars {
    shared: Weak<Shared>,
    id: WindowId,
    /// The window's scale, which a change of its backing's other
    /// properties, as its color space, leaves.
    scale: Cell<f64>,
    /// Whether the window keeps its frame as it moves and resizes, which
    /// the backend turns on once it placed the window.
    remember: Cell<bool>,
    /// Whether the window is hidden or covered, which the backend reads.
    occluded: Rc<Cell<bool>>,
}

/// The name under which the windows keep their frame, in the app's
/// preferences.
const FRAME_NAME: &str = "Lokyr Window";

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // delegate does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WindowDelegateIvars]
    pub(crate) struct WindowDelegate;

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for WindowDelegate {}

    // SAFETY: `NSWindowDelegate` has no requirement; each method has
    // AppKit's signature.
    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            // The window stays open until the app closes it.
            self.post(WindowEvent::CloseRequested);
            false
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                self.keep_frame(&window);
                self.post(WindowEvent::Resized(size_of(&window)));
            }
        }

        #[unsafe(method(windowDidMove:))]
        fn window_did_move(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                self.keep_frame(&window);
            }
        }

        // A change of the scale, and not of the backing's other properties,
        // as its color space, which the same notification tells.
        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                let scale = window.backingScaleFactor();
                if self.ivars().scale.replace(scale) != scale {
                    self.post(WindowEvent::ScaleChanged(size_of(&window)));
                }
            }
        }

        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            self.post(WindowEvent::Focused(true));
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.post(WindowEvent::Focused(false));
        }

        #[unsafe(method(windowDidChangeOcclusionState:))]
        fn window_did_change_occlusion_state(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                let visible = window
                    .occlusionState()
                    .contains(NSWindowOcclusionState::Visible);
                self.ivars().occluded.set(!visible);
                self.post(WindowEvent::Occluded(!visible));
            }
        }
    }
);

impl WindowDelegate {
    fn new(
        mtm: MainThreadMarker,
        shared: Weak<Shared>,
        id: WindowId,
        (scale, occluded): (f64, Rc<Cell<bool>>),
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WindowDelegateIvars {
            shared,
            id,
            scale: Cell::new(scale),
            remember: Cell::new(false),
            occluded,
        });
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    /// Keeps `window`'s frame in the app's preferences, for the window that
    /// restores it, if the window keeps it, but in full screen, whose frame
    /// is the screen's.
    fn keep_frame(&self, window: &NSWindow) {
        if self.ivars().remember.get()
            && !window.styleMask().contains(NSWindowStyleMask::FullScreen)
        {
            window.saveFrameUsingName(&NSString::from_str(FRAME_NAME));
        }
    }

    fn post(&self, event: WindowEvent) {
        let ivars = self.ivars();
        if let Some(shared) = ivars.shared.upgrade() {
            shared.post(Pending::Window(ivars.id, event));
        }
    }
}

/// Gives `window` the frame that a window kept last, if one did and it is on
/// a screen that is still there: whether it did.
fn restore_frame(window: &NSWindow) -> bool {
    let before = window.frame();
    if !window.setFrameUsingName(&NSString::from_str(FRAME_NAME)) {
        return false;
    }
    let frame = window.frame();
    let mtm = window.mtm();
    let screens = NSScreen::screens(mtm);
    let shown = (0..screens.count()).any(|index| {
        let visible = screens.objectAtIndex(index).visibleFrame();
        frame.origin.x < visible.origin.x + visible.size.width
            && visible.origin.x < frame.origin.x + frame.size.width
            && frame.origin.y < visible.origin.y + visible.size.height
            && visible.origin.y < frame.origin.y + frame.size.height
    });
    if !shown {
        window.setFrame_display(before, false);
    }
    shown
}

/// What a double click on a window's titlebar does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BarDoubleClick {
    Zoom,
    Minimize,
    Nothing,
}

/// What a double click on a window's titlebar does, as the system's
/// settings say, in Desktop & Dock: zoom by default, and for Fill, which
/// tiles a window where the public calls only zoom it; minimize; or
/// nothing.
fn bar_double_click() -> BarDoubleClick {
    let defaults = NSUserDefaults::standardUserDefaults();
    let action = defaults
        .stringForKey(&NSString::from_str("AppleActionOnDoubleClick"))
        .map(|action| action.to_string());
    let minimize = defaults.boolForKey(&NSString::from_str("AppleMiniaturizeOnDoubleClick"));
    double_click_of(action.as_deref(), minimize)
}

/// The double click of the settings' `action`, or, without one, of the
/// older setting of minimizing.
fn double_click_of(action: Option<&str>, minimize: bool) -> BarDoubleClick {
    match action {
        Some("Minimize") => BarDoubleClick::Minimize,
        Some("None") => BarDoubleClick::Nothing,
        Some(_) => BarDoubleClick::Zoom,
        None if minimize => BarDoubleClick::Minimize,
        None => BarDoubleClick::Zoom,
    }
}

/// The window that a window's notification is about.
fn window_of(notification: &NSNotification) -> Option<Retained<NSWindow>> {
    notification.object()?.downcast::<NSWindow>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_double_click_on_the_bar_does_as_the_settings_say() {
        for (action, minimize, wanted) in [
            (None, false, BarDoubleClick::Zoom),
            (Some("Maximize"), false, BarDoubleClick::Zoom),
            (Some("Fill"), false, BarDoubleClick::Zoom),
            (Some("Minimize"), false, BarDoubleClick::Minimize),
            (Some("None"), true, BarDoubleClick::Nothing),
            // The older setting, without the newer one.
            (None, true, BarDoubleClick::Minimize),
        ] {
            assert_eq!(double_click_of(action, minimize), wanted, "{action:?}");
        }
    }

    #[test]
    fn a_gesture_s_phases_become_the_scroll_s() {
        let phase = |flags: usize| scroll_phase(NSEventPhase(flags));
        assert_eq!(phase(NSEventPhase::MayBegin.0), Some(ScrollPhase::Started));
        assert_eq!(phase(NSEventPhase::Began.0), Some(ScrollPhase::Started));
        assert_eq!(phase(NSEventPhase::Changed.0), Some(ScrollPhase::Moved));
        assert_eq!(phase(NSEventPhase::Stationary.0), Some(ScrollPhase::Moved));
        assert_eq!(phase(NSEventPhase::Ended.0), Some(ScrollPhase::Ended));
        assert_eq!(phase(NSEventPhase::Cancelled.0), Some(ScrollPhase::Ended));
        // A wheel's step has none.
        assert_eq!(phase(NSEventPhase::None.0), None);
    }
}

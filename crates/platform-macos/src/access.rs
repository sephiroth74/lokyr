//! A window's accessibility: AccessKit's macOS adapter of the content's
//! view, which the view's accessibility methods ask for the nodes of the
//! window's tree. The adapter builds on its own versions of `objc2`, older
//! than the app's, and takes the view as a raw pointer, its answers given
//! back as raw pointers.
//!
//! Nothing happens until an accessibility client, as VoiceOver, first asks
//! for the view's children, its focus or the element at a point: the
//! adapter then answers of a placeholder, and the app is told, on the run
//! loop's next turn, to give its tree, which the adapter takes as its
//! first whole one; an action that a client asks for goes to the app the
//! same way. The answers come within AppKit's calls, which hold the
//! adapter, so that the app's own calls into it wait for their end.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Weak;

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use lokyr_platform_core::{WindowEvent, WindowId};

use crate::shared::Shared;

/// The accessibility of a window's content view.
pub(crate) struct Access {
    adapter: RefCell<accesskit_macos::Adapter>,
    /// Whether a client asked for the tree, which the app was told of.
    asked: Cell<bool>,
    shared: Weak<Shared>,
    id: WindowId,
}

/// What a client's first ask does: tells the app, on the run loop's next
/// turn, to give its tree.
struct Asking<'a>(&'a Access);

impl ActivationHandler for Asking<'_> {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let access = self.0;
        if !access.asked.replace(true)
            && let Some(shared) = access.shared.upgrade()
        {
            shared.post_later(access.id, WindowEvent::AccessibilityAsked);
        }
        None
    }
}

/// What a client's action does: tells the app, on the run loop's next turn.
struct Actions {
    shared: Weak<Shared>,
    id: WindowId,
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        if let Some(shared) = self.shared.upgrade() {
            shared.post_later(self.id, WindowEvent::AccessibilityAction(request));
        }
    }
}

impl Access {
    /// The accessibility of the content view `view` of the window `id`,
    /// whose events go to `shared`: nothing until a client asks.
    ///
    /// # Safety
    ///
    /// `view` is an `NSView`, alive for the call: the adapter keeps a weak
    /// reference to it.
    pub(crate) unsafe fn new(view: *mut c_void, shared: Weak<Shared>, id: WindowId) -> Self {
        let actions = Actions {
            shared: shared.clone(),
            id,
        };
        // SAFETY: as this function's caller vouches.
        let adapter = unsafe { accesskit_macos::Adapter::new(view, false, actions) };
        Self {
            adapter: RefCell::new(adapter),
            asked: Cell::new(false),
            shared,
            id,
        }
    }

    /// Whether a client asked for the tree.
    pub(crate) fn asked(&self) -> bool {
        self.asked.get()
    }

    /// The view's accessibility children, an autoreleased `NSArray`, of the
    /// tree's root: `None` while the adapter answers another call.
    pub(crate) fn children(&self) -> Option<*mut c_void> {
        let mut adapter = self.adapter.try_borrow_mut().ok()?;
        Some(adapter.view_children(&mut Asking(self)).cast())
    }

    /// The element of the tree that has the keyboard's focus, autoreleased,
    /// if it may: `None` while the adapter answers another call.
    pub(crate) fn focus(&self) -> Option<*mut c_void> {
        let mut adapter = self.adapter.try_borrow_mut().ok()?;
        Some(adapter.focus(&mut Asking(self)).cast())
    }

    /// The element of the tree at `point`, in the window's coordinates, as
    /// AppKit's hit test gives it, autoreleased: `None` while the adapter
    /// answers another call.
    pub(crate) fn hit_test(&self, (x, y): (f64, f64)) -> Option<*mut c_void> {
        let mut adapter = self.adapter.try_borrow_mut().ok()?;
        let point = accesskit_macos::NSPoint::new(x, y);
        Some(adapter.hit_test(point, &mut Asking(self)).cast())
    }

    /// The window took the keyboard's focus, or lost it.
    pub(crate) fn focused(&self, focused: bool) {
        let events = self
            .adapter
            .try_borrow_mut()
            .ok()
            .and_then(|mut adapter| adapter.update_view_focus_state(focused));
        // The borrow ends before the client hears, as it may ask again.
        if let Some(events) = events {
            events.raise();
        }
    }

    /// Takes the tree's `update`, once a client asked for it: whole the
    /// first time, its nodes changed after. Before, the adapter drops it.
    pub(crate) fn update(&self, update: TreeUpdate) {
        let events = self
            .adapter
            .try_borrow_mut()
            .ok()
            .and_then(|mut adapter| adapter.update_if_active(|| update));
        if let Some(events) = events {
            events.raise();
        }
    }
}

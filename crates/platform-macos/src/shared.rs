//! What AppKit's callbacks share with the app's handler, and the queue
//! through which events reach the handler one at a time.
//!
//! AppKit calls its delegates from within its own methods, such as a
//! window's `makeKeyAndOrderFront:`, which tells the delegate that the
//! window took the focus before it returns. When the app calls such a
//! method from its handler, the callback must not call the handler again:
//! every callback queues what it has to tell, and the one dispatch in
//! progress, or the one that the callback starts, hands it on once no call
//! is in progress.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::rc::{Rc, Weak};

use bolt_platform_core::{AlertId, AppEvent, Handler, Waker, WindowEvent, WindowId};
use objc2::MainThreadMarker;
use objc2_app_kit::NSModalResponse;

use crate::alert;
use crate::backend::Backend;

/// Something for the handler, which waits its turn.
pub(crate) enum Pending {
    /// The application finished launching.
    Started,
    /// An event of the application.
    App(AppEvent),
    /// An event of a window.
    Window(WindowId, WindowEvent),
    /// An alert without a window, whose loop runs once no call is in
    /// progress.
    Modal(AlertId),
    /// An alert closed, with AppKit's answer.
    AlertResponse(AlertId, NSModalResponse),
}

/// The state of a running application, on the main thread.
pub(crate) struct Shared {
    queue: RefCell<VecDeque<Pending>>,
    /// Whether a dispatch is in progress, which hands on what the queue
    /// takes meanwhile.
    dispatching: Cell<bool>,
    handler: RefCell<Option<Box<dyn Handler>>>,
    backend: RefCell<Backend>,
    /// The panic of a call of the handler, which ends the application and
    /// goes on once `run` returns.
    panic: RefCell<Option<Box<dyn Any + Send>>>,
}

impl Shared {
    pub(crate) fn new(mtm: MainThreadMarker, handler: Box<dyn Handler>, waker: Waker) -> Rc<Self> {
        Rc::new_cyclic(|shared: &Weak<Self>| Self {
            queue: RefCell::new(VecDeque::new()),
            dispatching: Cell::new(false),
            handler: RefCell::new(Some(handler)),
            backend: RefCell::new(Backend::new(mtm, shared.clone(), waker)),
            panic: RefCell::new(None),
        })
    }

    /// Queues `pending`, and hands it on now unless a dispatch is in
    /// progress, which will.
    pub(crate) fn post(&self, pending: Pending) {
        self.enqueue(pending);
        self.dispatch();
    }

    /// Queues `pending` for the dispatch in progress.
    pub(crate) fn enqueue(&self, pending: Pending) {
        self.queue.borrow_mut().push_back(pending);
    }

    /// Ends the application, from the main thread once `run` returned: the
    /// handler goes first, with what it holds, then the windows. Returns the
    /// panic of the handler, if one ended the application.
    pub(crate) fn finish(&self) -> Option<Box<dyn Any + Send>> {
        drop(self.handler.borrow_mut().take());
        self.queue.borrow_mut().clear();
        self.backend.borrow_mut().close_all();
        self.panic.borrow_mut().take()
    }

    fn dispatch(&self) {
        if self.dispatching.replace(true) {
            return;
        }
        loop {
            let Some(pending) = self.queue.borrow_mut().pop_front() else {
                break;
            };
            if self.backend.borrow().is_stopped() {
                self.queue.borrow_mut().clear();
                break;
            }
            if let Pending::Modal(alert) = pending {
                self.run_modal(alert);
            } else {
                self.call(pending);
            }
            let mut backend = self.backend.borrow_mut();
            if backend.wants_to_stop() {
                backend.stop();
            }
        }
        self.dispatching.set(false);
    }

    /// Hands `pending` to the handler, whose panic ends the application.
    fn call(&self, pending: Pending) {
        let mut handler = self.handler.borrow_mut();
        let Some(handler) = handler.as_deref_mut() else {
            return;
        };
        let mut backend = self.backend.borrow_mut();
        let backend = &mut *backend;
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| match pending {
            Pending::Started => handler.started(backend),
            Pending::App(event) => handler.app_event(backend, event),
            Pending::Window(window, event) => {
                // A window that closed meanwhile has nothing more to tell.
                if backend.has_window(window) {
                    handler.window_event(backend, window, event);
                }
            }
            Pending::AlertResponse(alert, response) => {
                // An alert that closed with its window is not answered.
                if let Some(buttons) = backend.take_alert(alert) {
                    let button = alert::button(response, buttons);
                    handler.app_event(backend, AppEvent::AlertClosed { alert, button });
                }
            }
            Pending::Modal(_) => {}
        }));
        if let Err(payload) = outcome {
            self.panic.borrow_mut().get_or_insert(payload);
            backend.quit_now();
        }
    }

    /// Runs the loop of an alert without a window. No call is in progress,
    /// so the events that come during it are handed on as they come.
    fn run_modal(&self, alert: AlertId) {
        let Some(modal) = self.backend.borrow().alert(alert) else {
            return;
        };
        self.dispatching.set(false);
        let response = modal.runModal();
        self.dispatching.set(true);
        self.enqueue(Pending::AlertResponse(alert, response));
    }
}

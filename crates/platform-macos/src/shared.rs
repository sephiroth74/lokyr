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

use lokyr_platform_core::{AlertId, AppEvent, Handler, TimerId, Waker, WindowEvent, WindowId};
use objc2::MainThreadMarker;
use objc2_app_kit::NSModalResponse;

use crate::alert;
use crate::backend::Backend;
use crate::wake;

/// What makes the application's handler, as it starts.
pub(crate) type MakeHandler = Box<dyn FnOnce() -> Box<dyn Handler>>;

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
    /// The context menu that the app asked for, whose loop runs once no
    /// call is in progress, as an alert's.
    ContextMenu,
    /// An alert closed, with AppKit's answer.
    AlertResponse(AlertId, NSModalResponse),
    /// A timer fired.
    Timer(TimerId),
}

/// The state of a running application, on the main thread.
pub(crate) struct Shared {
    queue: RefCell<VecDeque<Pending>>,
    /// Whether a dispatch is in progress, which hands on what the queue
    /// takes meanwhile.
    dispatching: Cell<bool>,
    handler: RefCell<Option<Box<dyn Handler>>>,
    /// What makes the handler, once the application has launched, before
    /// it starts.
    make: RefCell<Option<MakeHandler>>,
    backend: RefCell<Backend>,
    /// The panic of a call of the handler, which ends the application and
    /// goes on once `run` returns.
    panic: RefCell<Option<Box<dyn Any + Send>>>,
    /// The loops of alerts without a window in progress, within each other.
    modals: Cell<usize>,
    /// Whether the innermost of those loops was asked to end, for the
    /// application to stop once it has.
    modal_aborted: Cell<bool>,
    /// Whether a context menu's loop runs, which opens the next menu once
    /// it returns.
    menu_open: Cell<bool>,
    /// Whether the system asked the application to quit and waits for its
    /// answer, as AppKit's `NSTerminateLater` has it.
    terminating: Cell<bool>,
    /// The events of windows that wait for the run loop's next turn, as an
    /// accessibility client's asks, which come within AppKit's calls that
    /// the app's answers would come back into.
    later: RefCell<Vec<(WindowId, WindowEvent)>>,
    /// The application's events that came before it launched, as the files
    /// that the system opens with it as it starts, which the handler takes
    /// once it has started.
    early: RefCell<Vec<AppEvent>>,
}

impl Shared {
    pub(crate) fn new(mtm: MainThreadMarker, make: MakeHandler, waker: Waker) -> Rc<Self> {
        Rc::new_cyclic(|shared: &Weak<Self>| Self {
            queue: RefCell::new(VecDeque::new()),
            dispatching: Cell::new(false),
            handler: RefCell::new(None),
            make: RefCell::new(Some(make)),
            backend: RefCell::new(Backend::new(mtm, shared.clone(), waker)),
            panic: RefCell::new(None),
            modals: Cell::new(0),
            terminating: Cell::new(false),
            modal_aborted: Cell::new(false),
            menu_open: Cell::new(false),
            later: RefCell::new(Vec::new()),
            early: RefCell::new(Vec::new()),
        })
    }

    /// Hands the window `id` `event` on the run loop's next turn, as the
    /// call in progress, of AppKit's, may hold what the app's answer would
    /// take.
    pub(crate) fn post_later(&self, id: WindowId, event: WindowEvent) {
        let first = {
            let mut later = self.later.borrow_mut();
            later.push((id, event));
            later.len() == 1
        };
        if first {
            wake::post_later();
        }
    }

    /// The events of windows that waited for this turn, in their order.
    pub(crate) fn take_later(&self) -> Vec<(WindowId, WindowEvent)> {
        std::mem::take(&mut *self.later.borrow_mut())
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
        // What AppKit tells while the windows close, such as the answers of
        // the sheets that it ends, has no handler left: it is queued, never
        // dispatched, and dropped.
        self.dispatching.set(true);
        drop(self.handler.borrow_mut().take());
        let windows = self.backend.borrow_mut().take_windows();
        for window in windows {
            window.close();
        }
        self.queue.borrow_mut().clear();
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
            // Nothing comes once the app has quit.
            if self.backend.borrow().has_quit() {
                self.queue.borrow_mut().clear();
                break;
            }
            match pending {
                Pending::Modal(alert) => self.run_modal(alert),
                Pending::ContextMenu => self.run_context_menu(),
                pending => self.call(pending),
            }
            self.stop_if_quit();
        }
        self.dispatching.set(false);
    }

    /// Notes that the system asked the application to quit, and waits for
    /// its answer.
    pub(crate) fn begin_terminating(&self) {
        self.terminating.set(true);
    }

    /// Takes the note of the system's request to quit: whether one waits
    /// for its answer.
    pub(crate) fn end_terminating(&self) -> bool {
        self.terminating.replace(false)
    }

    /// Stops the application once the app has quit: after the system's
    /// request, by answering it, which ends the process. Within an alert's
    /// loop, where `stop` would end the alert's loop rather than the
    /// application's, it ends the alert's loop first, and the application
    /// stops once that loop has returned.
    fn stop_if_quit(&self) {
        let mut backend = self.backend.borrow_mut();
        if !backend.has_quit() || backend.is_stopped() {
            return;
        }
        if self.end_terminating() {
            // AppKit takes the answer and ends the process: the system,
            // as a logout, goes on.
            backend.answer_termination(true);
            return;
        }
        if self.modals.get() == 0 {
            backend.stop();
        } else if !self.modal_aborted.replace(true) {
            backend.abort_modal();
        }
    }

    /// Hands `pending` to the handler, whose panic ends the application:
    /// the one made as the application starts.
    fn call(&self, pending: Pending) {
        let pending = match pending {
            Pending::App(event) if self.make.borrow().is_some() => {
                self.early.borrow_mut().push(event);
                return;
            }
            pending => pending,
        };
        if matches!(pending, Pending::Started)
            && let Some(make) = self.make.borrow_mut().take()
        {
            // What came before, right after the start.
            let mut queue = self.queue.borrow_mut();
            for event in self.early.take().into_iter().rev() {
                queue.push_front(Pending::App(event));
            }
            drop(queue);
            match panic::catch_unwind(AssertUnwindSafe(make)) {
                Ok(handler) => *self.handler.borrow_mut() = Some(handler),
                Err(payload) => {
                    self.panic.borrow_mut().get_or_insert(payload);
                    self.backend.borrow_mut().quit_now();
                    return;
                }
            }
        }
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
                    if event == WindowEvent::Frame {
                        backend.frame_delivered(window);
                    }
                    if event == WindowEvent::Occluded(false) {
                        backend.request_held_frame(window);
                    }
                    handler.window_event(backend, window, event);
                }
            }
            Pending::AlertResponse(alert, response) => {
                // An alert that closed with its window is not answered.
                if let Some((buttons, suppressed, text)) = backend.take_alert(alert) {
                    let button = alert::button(response, buttons);
                    let event = AppEvent::AlertClosed {
                        alert,
                        button,
                        suppressed,
                        text,
                    };
                    handler.app_event(backend, event);
                }
            }
            Pending::Timer(timer) => {
                // A timer cancelled after it fired, before its turn, is not
                // told.
                if backend.take_timer(timer) {
                    handler.app_event(backend, AppEvent::Timer(timer));
                }
            }
            Pending::Modal(_) | Pending::ContextMenu => {}
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
        self.modals.set(self.modals.get() + 1);
        self.dispatching.set(false);
        let response = modal.runModal();
        self.dispatching.set(true);
        self.modals.set(self.modals.get() - 1);
        // An outer loop, if one runs, is the next to end.
        self.modal_aborted.set(false);
        self.enqueue(Pending::AlertResponse(alert, response));
    }

    /// Runs the loop of the context menu that waits for its turn, at its
    /// place over its window's view, and of the one that the app asked for
    /// meanwhile, once it returns: within a menu's loop, the menu that the
    /// app asked for closes it, and opens once its loop has returned. No
    /// call is in progress, so the events that come during it, as its
    /// chosen item's, are handed on as they come.
    fn run_context_menu(&self) {
        if self.menu_open.replace(true) {
            return;
        }
        loop {
            let Some((menu, view, location, window)) =
                self.backend.borrow_mut().open_context_menu()
            else {
                break;
            };
            self.dispatching.set(false);
            menu.popUpMenuPositioningItem_atLocation_inView(None, location, Some(&view));
            self.dispatching.set(true);
            self.backend.borrow_mut().context_menu_closed();
            // Its closing, after its choice, which came as it was made.
            self.enqueue(Pending::Window(window, WindowEvent::ContextMenuClosed));
        }
        self.menu_open.set(false);
    }
}

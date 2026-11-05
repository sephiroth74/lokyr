//! Plugins on threads of their own: each such plugin's calls, its renders
//! too, run on a thread of the host's for it alone, and its requests cross
//! to the main thread as messages, as a Luau VM's will. The main thread
//! answers each request as it turns, and takes each call's end as it comes,
//! never waiting for either: the plugin's thread waits for the answers. A
//! call begins once the one before it ended, so that the host keeps each
//! call's writes, which it applies as the call returns.

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::sync::mpsc;

use lokyr_plugin_sdk::host;
use lokyr_plugin_sdk::message::{Call, Event, Response};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::Node;

use super::{Outbox, Write, message};
use crate::commands::{Args, Code, Error};
use crate::workspace::WindowId;

/// What wakes the main thread, as a message of a plugin's thread waits for
/// it.
pub(super) type Wake = Arc<dyn Fn() + Send + Sync>;

/// A call of a plugin's, as it crosses to its thread.
pub(super) enum Job {
    /// Its activation.
    Activate,
    /// A command of its, with its arguments, which commands ran as deep
    /// as the number tells.
    Command(String, Args, usize),
    /// A batch of events.
    Events(Vec<Event>),
    /// A render of one of its views.
    Render(ViewId),
    /// An event of a node of one of its views.
    Ui(UiEvent),
}

/// Which call is under way, as its end tells what to do with its result.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Called {
    /// Its activation, whose failure disables it.
    Activate,
    /// A command, whose failure the user is told of.
    Command,
    /// A batch of events.
    Events,
    /// A render of the view, whose tree the host keeps.
    Render(ViewId),
    /// An event of a node of one of its views.
    Ui,
}

/// What the main thread keeps of the call under way: which call, its
/// writes, and what it asks of the app and emits, which come as it
/// returns; how deep commands ran the command that it is, and the window
/// that had the focus as it was asked, which its reads of the focus read,
/// as a call of the main thread would have.
pub(super) struct Busy {
    pub(super) called: Called,
    pub(super) writes: Vec<Write>,
    pub(super) out: Outbox,
    pub(super) depth: usize,
    pub(super) focused: Option<WindowId>,
}

/// How a call ended on the plugin's thread.
pub(super) enum Done {
    /// It returned, with its result.
    Returned(Result<(), Error>),
    /// A render returned its tree.
    Rendered(Option<Box<Node>>),
    /// It panicked: `: ` and the panic's message, if it has one.
    Panicked(String),
}

/// What the main thread sends a plugin's thread.
enum ToWorker {
    /// A call to make.
    Call(Job),
    /// The answer to the request that the call under way waits for.
    Answer(Response),
}

/// What a plugin's thread sends the main thread.
pub(super) enum FromWorker {
    /// A request of the call under way, which waits for its answer.
    Request(Call),
    /// The call under way ended.
    Done(Done),
}

/// A plugin's thread, as the main thread reaches it: the call under way,
/// and those that wait for it to end, each with the window that had the
/// focus as it was asked; and the events posted while it activates, before
/// its subscriptions are known.
pub(super) struct Worker {
    to: mpsc::Sender<ToWorker>,
    from: mpsc::Receiver<FromWorker>,
    busy: Option<Busy>,
    waiting: VecDeque<(Job, Option<WindowId>)>,
    held: Vec<(Option<String>, Event)>,
}

impl Worker {
    /// Starts the thread of `plugin`, of the name `name`, whose messages
    /// `wake` tells the main thread of; the error of a thread that did not
    /// start, which drops the plugin.
    pub(super) fn start(
        name: &str,
        plugin: Box<dyn Plugin + Send>,
        wake: Wake,
    ) -> std::io::Result<Self> {
        let (to, calls) = mpsc::channel();
        let (sender, from) = mpsc::channel();
        let caller = name.to_owned();
        std::thread::Builder::new()
            .name(format!("lokyr-plugin {name}"))
            .spawn(move || run(&caller, plugin, &calls, &sender, &*wake))?;
        Ok(Self {
            to,
            from,
            busy: None,
            waiting: VecDeque::new(),
            held: Vec::new(),
        })
    }

    /// Asks for `job`, as `focused` had the focus: now, if no call is
    /// under way, or after those that wait. Events join the batch that
    /// waits last, if it is the last call that waits, which then reads the
    /// focus of the latest; a render of a view whose render is under way
    /// or waits is not asked again, as that one renders it: a render that
    /// the plugin asks for within its render comes as the render returns,
    /// once it is no longer under way.
    pub(super) fn ask(&mut self, job: Job, focused: Option<WindowId>) {
        let Some(busy) = &self.busy else {
            self.begin(job, focused);
            return;
        };
        match job {
            Job::Events(events) => match self.waiting.back_mut() {
                Some((Job::Events(batch), was)) => {
                    batch.extend(events);
                    *was = focused;
                }
                _ => self.waiting.push_back((Job::Events(events), focused)),
            },
            Job::Render(view) => {
                let asked = busy.called == Called::Render(view.clone())
                    || self
                        .waiting
                        .iter()
                        .any(|(job, _)| matches!(job, Job::Render(waiting) if *waiting == view));
                if !asked {
                    self.waiting.push_back((Job::Render(view), focused));
                }
            }
            job => self.waiting.push_back((job, focused)),
        }
    }

    fn begin(&mut self, job: Job, focused: Option<WindowId>) {
        let (called, depth) = match &job {
            Job::Activate => (Called::Activate, 0),
            Job::Command(_, _, depth) => (Called::Command, *depth),
            Job::Events(_) => (Called::Events, 0),
            Job::Render(view) => (Called::Render(view.clone()), 0),
            Job::Ui(_) => (Called::Ui, 0),
        };
        // A thread that ended takes nothing, which its next message tells.
        let _ = self.to.send(ToWorker::Call(job));
        self.busy = Some(Busy {
            called,
            writes: Vec::new(),
            out: Outbox::default(),
            depth,
            focused,
        });
    }

    /// Whether its activation is under way, whose subscriptions its
    /// requests are still making.
    pub(super) fn activating(&self) -> bool {
        self.busy
            .as_ref()
            .is_some_and(|busy| busy.called == Called::Activate)
    }

    /// Holds `events`, posted while it activates, each with the plugin it
    /// is for, if it is for one, until its activation ends.
    pub(super) fn hold(&mut self, events: impl IntoIterator<Item = (Option<String>, Event)>) {
        self.held.extend(events);
    }

    /// Asks for the events held while it activated, as its activation
    /// ended, as `focused` has the focus: those for it and those of the
    /// names of `subscribed`, before the calls that wait.
    pub(super) fn activated(&mut self, subscribed: &[String], focused: Option<WindowId>) {
        let batch: Vec<Event> = std::mem::take(&mut self.held)
            .into_iter()
            .filter(|(to, event)| {
                to.is_some() || subscribed.iter().any(|name| name == event.name())
            })
            .map(|(_, event)| event)
            .collect();
        if !batch.is_empty() {
            self.waiting.push_front((Job::Events(batch), focused));
        }
    }

    /// Whether a call is under way.
    pub(super) fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    /// The call under way, whose requests the host answers.
    pub(super) fn busy_mut(&mut self) -> Option<&mut Busy> {
        self.busy.as_mut()
    }

    /// The next message of the plugin's thread, if one came.
    pub(super) fn take(&self) -> Result<FromWorker, mpsc::TryRecvError> {
        self.from.try_recv()
    }

    /// Answers the request that the call under way waits for.
    pub(super) fn answer(&self, response: Response) {
        // A thread that ended takes nothing, which its next message tells.
        let _ = self.to.send(ToWorker::Answer(response));
    }

    /// Ends the call under way, as it ended: what the host kept of it.
    pub(super) fn end(&mut self) -> Option<Busy> {
        self.busy.take()
    }

    /// Begins the next call that waits, if one does and none is under way.
    pub(super) fn next(&mut self) {
        if self.busy.is_none()
            && let Some((job, focused)) = self.waiting.pop_front()
        {
            self.begin(job, focused);
        }
    }
}

/// The loop of the thread of the plugin `name`: each call that comes, within
/// `catch_unwind`, and its end told, until the main thread drops its end of
/// the channel, or the plugin panics, which ends the thread.
fn run(
    name: &str,
    mut plugin: Box<dyn Plugin + Send>,
    calls: &mpsc::Receiver<ToWorker>,
    to_main: &mpsc::Sender<FromWorker>,
    wake: &(dyn Fn() + Send + Sync),
) {
    while let Ok(received) = calls.recv() {
        // Answers come within calls alone.
        let ToWorker::Call(job) = received else {
            continue;
        };
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut proxy = Proxy {
                calls,
                to_main,
                wake,
            };
            let mut cx = Cx::new(&mut proxy, name);
            match job {
                Job::Activate => Done::Returned(plugin.activate(&mut cx)),
                Job::Command(command, args, _) => {
                    Done::Returned(plugin.on_command(&mut cx, &command, args))
                }
                Job::Events(events) => Done::Returned(plugin.on_event(&mut cx, &events)),
                Job::Render(view) => Done::Rendered(plugin.render(&mut cx, &view).map(Box::new)),
                Job::Ui(event) => Done::Returned(plugin.on_ui_event(&mut cx, &event)),
            }
        }));
        let (done, panicked) = match outcome {
            Ok(done) => (done, false),
            Err(payload) => (Done::Panicked(message(payload.as_ref())), true),
        };
        if to_main.send(FromWorker::Done(done)).is_err() {
            return;
        }
        wake();
        if panicked {
            return;
        }
    }
}

/// The host of a call on a plugin's thread: each request crosses to the
/// main thread, which the thread waits for the answer of.
struct Proxy<'p> {
    calls: &'p mpsc::Receiver<ToWorker>,
    to_main: &'p mpsc::Sender<FromWorker>,
    wake: &'p (dyn Fn() + Send + Sync),
}

impl host::Host for Proxy<'_> {
    fn handle(&mut self, call: Call) -> Response {
        let gone = || Response::Failed(Error::new(Code::Unavailable, "the app's host stopped"));
        if self.to_main.send(FromWorker::Request(call)).is_err() {
            return gone();
        }
        (self.wake)();
        match self.calls.recv() {
            Ok(ToWorker::Answer(response)) => response,
            // The host begins a call once the one under way ended.
            Ok(ToWorker::Call(_)) => {
                Response::Failed(Error::new(Code::Fault, "a call came within a call"))
            }
            Err(_) => gone(),
        }
    }
}

#[cfg(test)]
mod tests;

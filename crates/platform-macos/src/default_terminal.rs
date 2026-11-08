//! The app as the system's default terminal: the application that Launch
//! Services opens shell scripts, `.command` and `.tool` files and
//! executables in, and `ssh` and `x-man-page` URLs, which the bundle
//! declares. Whether it is, which takes Launch Services some milliseconds
//! in a new process, is asked on a thread of its own after the start and
//! as the app becomes active, as another app may take the default in the
//! meantime, and kept. The system asks the user to confirm each change,
//! and answers on a thread of its own: the changes go one after another,
//! each once the main thread took the answer to the one before.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use block2::RcBlock;
use lokyr_platform_core::{AppEvent, Waker};
use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSError, NSString, NSURL};
use objc2_uniform_type_identifiers::UTType;

use crate::shared::{Pending, Shared};

/// The types of the files that the default terminal opens: Terminal's
/// shell scripts, `.command` and `.tool`; shell scripts, `.sh`; and the
/// executables, which it opens in the Shell role.
const TYPES: [&str; 3] = [
    "com.apple.terminal.shell-script",
    "public.shell-script",
    "public.unix-executable",
];

/// The schemes of the URLs that the default terminal opens.
const SCHEMES: [&str; 2] = ["ssh", "x-man-page"];

/// A change of the system's default handlers.
#[derive(Clone, Copy, Debug)]
enum Change {
    Type(&'static str),
    Scheme(&'static str),
}

/// The answers to the changes, which the system gives on a thread of its
/// own: what failed, or `None`.
static ANSWERS: Mutex<Vec<Option<String>>> = Mutex::new(Vec::new());

/// Whether the app is the default terminal, as the system told it last:
/// [`UNKNOWN`] before, [`NOT_DEFAULT`] or [`DEFAULT`].
static KNOWN: AtomicU8 = AtomicU8::new(UNKNOWN);
const UNKNOWN: u8 = 0;
const NOT_DEFAULT: u8 = 1;
const DEFAULT: u8 = 2;

/// Whether a look found an answer other than the one before, which the
/// main thread takes at the wake that the look gives.
static TOLD: AtomicBool = AtomicBool::new(false);

/// Whether a look is under way, which a look that comes meanwhile leaves
/// alone.
static LOOKING: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The changes under way, on the main thread.
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
    /// What wakes the main thread as a look finds a new answer, from the
    /// start's look on.
    static WAKER: RefCell<Option<Waker>> = const { RefCell::new(None) };
}

/// The changes under way: those to come, what failed, the bundle's URL, and
/// what wakes the main thread as an answer comes.
struct Run {
    remaining: VecDeque<Change>,
    failed: Vec<String>,
    bundle: Retained<NSURL>,
    waker: Waker,
}

/// The app's bundle, its URL and its identifier; `None` outside one, as
/// `cargo run` starts the binary alone.
fn bundle() -> Option<(Retained<NSURL>, String)> {
    let main = NSBundle::mainBundle();
    let identifier = main.bundleIdentifier()?;
    Some((main.bundleURL(), identifier.to_string()))
}

/// Whether the application at `url` is the bundle of `identifier`.
fn is_app(url: Option<Retained<NSURL>>, identifier: &str) -> bool {
    url.and_then(|url| NSBundle::bundleWithURL(&url))
        .and_then(|bundle| bundle.bundleIdentifier())
        .is_some_and(|found| found.to_string() == identifier)
}

/// Whether the app is the default terminal, as the system told it last;
/// `false` before it told.
pub(crate) fn known() -> bool {
    KNOWN.load(Ordering::Acquire) == DEFAULT
}

/// Keeps whether the app is the default terminal, as the system tells it
/// now.
fn look_now() {
    let found = if is_default() { DEFAULT } else { NOT_DEFAULT };
    KNOWN.store(found, Ordering::Release);
}

/// Asks the system whether the app is the default terminal, on a thread of
/// its own, out of the start's way, of `waker`, which wakes the main thread
/// as the answer is another than the app shows, and the looks after.
pub(crate) fn look(waker: Waker) {
    WAKER.with(|kept| *kept.borrow_mut() = Some(waker));
    look_again();
}

/// Asks again, as [`look`] does, as the app becomes active: nothing before
/// the start's look, nor while one is under way; outside a bundle, where
/// the app is never the default, as the menu's item shows from the start,
/// of no thread and no wake.
pub(crate) fn look_again() {
    let Some(waker) = WAKER.with(|kept| kept.borrow().clone()) else {
        return;
    };
    if bundle().is_none() {
        KNOWN.store(NOT_DEFAULT, Ordering::Release);
        return;
    }
    if LOOKING.swap(true, Ordering::AcqRel) {
        return;
    }
    let started = std::thread::Builder::new()
        .name("default-terminal".to_owned())
        .spawn(move || {
            let found = if is_default() { DEFAULT } else { NOT_DEFAULT };
            LOOKING.store(false, Ordering::Release);
            settle(found, &waker);
        });
    if let Err(error) = started {
        LOOKING.store(false, Ordering::Release);
        eprintln!("lokyr: whether Lokyr is the default terminal is not asked: {error}");
    }
}

/// Keeps `found`, and wakes the main thread of `waker` once, where it is
/// another answer than the one before: the app shows the item unticked
/// before the first, as of an app that is not the default.
fn settle(found: u8, waker: &Waker) {
    let before = KNOWN.swap(found, Ordering::AcqRel);
    if (before == DEFAULT) != (found == DEFAULT) {
        TOLD.store(true, Ordering::Release);
        waker.wake();
    }
}

/// Posts the app's event of the look's answer, on the main thread, once it
/// came.
pub(crate) fn take_known(shared: &Shared) {
    if TOLD.swap(false, Ordering::AcqRel) {
        shared.post(Pending::App(AppEvent::DefaultTerminalKnown));
    }
}

/// Whether the app is the system's default application for each of
/// [`TYPES`] and [`SCHEMES`], as Launch Services finds it now.
fn is_default() -> bool {
    let Some((_, identifier)) = bundle() else {
        return false;
    };
    let workspace = NSWorkspace::sharedWorkspace();
    let types = TYPES.iter().all(|name| {
        UTType::typeWithIdentifier(&NSString::from_str(name)).is_some_and(|kind| {
            is_app(
                workspace.URLForApplicationToOpenContentType(&kind),
                &identifier,
            )
        })
    });
    types
        && SCHEMES.iter().all(|scheme| {
            NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://localhost")))
                .is_some_and(|url| is_app(workspace.URLForApplicationToOpenURL(&url), &identifier))
        })
}

/// Starts making the app the system's default terminal, unless it is under
/// way: whether it is, as outside a bundle the app cannot be one.
pub(crate) fn make_default(waker: Waker) -> bool {
    if RUN.with(|run| run.borrow().is_some()) {
        return true;
    }
    let Some((bundle, _)) = bundle() else {
        return false;
    };
    let remaining = TYPES
        .iter()
        .map(|&name| Change::Type(name))
        .chain(SCHEMES.iter().map(|&scheme| Change::Scheme(scheme)))
        .collect();
    RUN.with(|run| {
        *run.borrow_mut() = Some(Run {
            remaining,
            failed: Vec::new(),
            bundle,
            waker,
        });
    });
    next();
    true
}

/// Asks the system for the next change, if one is left: whether one was.
/// The changes' borrow ends before AppKit's call, which may run the main
/// thread's loop, and the answers' take with it.
fn next() -> bool {
    let taken = RUN.with(|run| {
        let mut run = run.borrow_mut();
        let run = run.as_mut()?;
        let change = run.remaining.pop_front()?;
        Some((change, run.bundle.clone(), run.waker.clone()))
    });
    let Some((change, bundle, waker)) = taken else {
        return false;
    };
    {
        let named = match change {
            Change::Type(name) => format!("the type {name}"),
            Change::Scheme(scheme) => format!("the {scheme} URLs"),
        };
        let answered = RcBlock::new(move |error: *mut NSError| {
            // SAFETY: the system gives an error that lives through the
            // call, or none.
            let failed = unsafe { error.as_ref() }
                .map(|error| format!("{named}: {}", error.localizedDescription()));
            if let Ok(mut answers) = ANSWERS.lock() {
                answers.push(failed);
            }
            waker.wake();
        });
        let workspace = NSWorkspace::sharedWorkspace();
        match change {
            Change::Type(name) => match UTType::typeWithIdentifier(&NSString::from_str(name)) {
                Some(kind) => workspace
                    .setDefaultApplicationAtURL_toOpenContentType_completionHandler(
                        &bundle,
                        &kind,
                        Some(&answered),
                    ),
                None => answered.call((std::ptr::null_mut(),)),
            },
            Change::Scheme(scheme) => workspace
                .setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
                    &bundle,
                    &NSString::from_str(scheme),
                    Some(&answered),
                ),
        }
    }
    true
}

/// Takes the answers that came, on the main thread, as a waker woke it:
/// the next change, or, once the last answered, the app's event of what
/// failed.
pub(crate) fn take_answers(shared: &Shared) {
    let answers = ANSWERS
        .lock()
        .map(|mut answers| std::mem::take(&mut *answers))
        .unwrap_or_default();
    if answers.is_empty() {
        return;
    }
    let done = RUN.with(|run| {
        let mut run = run.borrow_mut();
        let under_way = run.as_mut()?;
        under_way.failed.extend(answers.into_iter().flatten());
        if under_way.remaining.is_empty() {
            return run.take().map(|run| run.failed);
        }
        None
    });
    match done {
        Some(failed) => {
            look_now();
            shared.post(Pending::App(AppEvent::DefaultTerminalMade { failed }));
        }
        None => {
            next();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn a_look_wakes_the_main_thread_once_of_each_new_answer() {
        let wakes = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&wakes);
        let waker = Waker::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
        });
        let told = || TOLD.swap(false, Ordering::AcqRel);
        // Not the default, as the item shows before any answer: no wake.
        settle(NOT_DEFAULT, &waker);
        assert_eq!(wakes.load(Ordering::Relaxed), 0);
        assert!(!told());
        // The default: once, and not again of the same answer.
        settle(DEFAULT, &waker);
        settle(DEFAULT, &waker);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert!(told());
        assert!(known());
        // Another app took it, as a look at an activation finds.
        settle(NOT_DEFAULT, &waker);
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert!(told());
        assert!(!known());
    }
}

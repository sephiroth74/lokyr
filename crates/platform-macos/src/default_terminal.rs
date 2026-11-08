//! The app as the system's default terminal: the application that Launch
//! Services opens shell scripts, `.command` and `.tool` files and
//! executables in, and `ssh` and `x-man-page` URLs, which the bundle
//! declares. The system asks the user to confirm each change, and answers
//! on a thread of its own: the changes go one after another, each once the
//! main thread took the answer to the one before.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;

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

thread_local! {
    /// The changes under way, on the main thread.
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
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

/// Whether the app is the system's default application for each of
/// [`TYPES`] and [`SCHEMES`].
pub(crate) fn is_default() -> bool {
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
fn next() -> bool {
    RUN.with(|run| {
        let mut run = run.borrow_mut();
        let Some(run) = run.as_mut() else {
            return false;
        };
        let Some(change) = run.remaining.pop_front() else {
            return false;
        };
        let waker = run.waker.clone();
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
                        &run.bundle,
                        &kind,
                        Some(&answered),
                    ),
                None => answered.call((std::ptr::null_mut(),)),
            },
            Change::Scheme(scheme) => workspace
                .setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
                    &run.bundle,
                    &NSString::from_str(scheme),
                    Some(&answered),
                ),
        }
        true
    })
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
        Some(failed) => shared.post(Pending::App(AppEvent::DefaultTerminalMade { failed })),
        None => {
            next();
        }
    }
}

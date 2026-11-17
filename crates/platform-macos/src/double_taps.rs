//! The double taps of the modifiers, from any app: a tap of the session's
//! events that listens to the modifiers' changes alone, on a thread of its
//! own, which wakes the main thread only as a double tap of a modifier
//! that the app watches ends. macOS gives such a tap to an app that the
//! user allowed in System Settings › Privacy & Security › Input
//! Monitoring; the tap sees no key that is typed, and the time since the
//! last key or click, which tells a tap from a shortcut, the system tells.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, mpsc};
use std::time::{Duration, Instant};

use lokyr_platform_core::{AppEvent, DoubleTap, DoubleTaps, Held, Modifier, Waker};
use objc2_app_kit::NSEvent;

use crate::shared::{Pending, Shared};

type CfTypeRef = *const c_void;
type CfMachPortRef = *mut c_void;
type CfRunLoopRef = *mut c_void;
type CfRunLoopSourceRef = *mut c_void;
type CgEventRef = *mut c_void;
type TapCallback = extern "C" fn(*mut c_void, u32, CgEventRef, *mut c_void) -> CgEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    /// Whether the user allowed the app to listen to the input's events.
    fn CGPreflightListenEventAccess() -> bool;
    /// Asks the user to allow it, of the system's question, once.
    fn CGRequestListenEventAccess() -> bool;
    /// A tap of the events of `mask` at `place`, which `callback` takes.
    fn CGEventTapCreate(
        place: u32,
        at: u32,
        options: u32,
        mask: u64,
        callback: TapCallback,
        info: *mut c_void,
    ) -> CfMachPortRef;
    fn CGEventTapEnable(tap: CfMachPortRef, enable: bool);
    fn CGEventGetFlags(event: CgEventRef) -> u64;
    /// The seconds since the last event of `kind` of the source `state`.
    fn CGEventSourceSecondsSinceLastEventType(state: u32, kind: u32) -> f64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: CfTypeRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: CfTypeRef,
        port: CfMachPortRef,
        order: isize,
    ) -> CfRunLoopSourceRef;
    fn CFMachPortInvalidate(port: CfMachPortRef);
    fn CFRunLoopGetCurrent() -> CfRunLoopRef;
    fn CFRunLoopAddSource(run_loop: CfRunLoopRef, source: CfRunLoopSourceRef, mode: CfTypeRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(run_loop: CfRunLoopRef);
    fn CFRetain(object: CfTypeRef) -> CfTypeRef;
    fn CFRelease(object: CfTypeRef);
}

/// The session's events, as the user's login sees them.
const SESSION: u32 = 1;
/// The tap first among the others.
const HEAD: u32 = 0;
/// A tap that listens, which never holds nor changes an event.
const LISTEN_ONLY: u32 = 1;
/// The events of the modifiers' changes.
const FLAGS_CHANGED: u32 = 12;
/// What a tap is told as the system turns it off, as it was slow, or as
/// the user's input did.
const DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
/// The modifiers' flags of an event, of either side.
const SHIFT: u64 = 0x0002_0000;
const CONTROL: u64 = 0x0004_0000;
const ALTERNATE: u64 = 0x0008_0000;
const COMMAND: u64 = 0x0010_0000;
/// The state of the keyboard and the mouse, of every app.
const HID_SYSTEM: u32 = 1;
/// A key's press, and the mouse's buttons' presses.
const INPUTS: [u32; 4] = [10, 1, 3, 25];

/// What the tap's thread and the main thread share.
struct Watch {
    /// The modifiers whose double taps the app watches.
    watched: Vec<Modifier>,
    /// What wakes the main thread as one ends.
    waker: Option<Waker>,
    /// The finder of the double taps.
    finder: DoubleTap,
    /// The tap and its thread's run loop, retained, while it runs.
    tap: usize,
    run_loop: usize,
    /// The double taps that ended since the main thread took them.
    tapped: Vec<Modifier>,
}

static WATCH: Mutex<Watch> = Mutex::new(Watch {
    watched: Vec::new(),
    waker: None,
    finder: DoubleTap::new(Duration::from_millis(500)),
    tap: 0,
    run_loop: 0,
    tapped: Vec::new(),
});

/// Whether the app asked the user for the leave once, as the system asks
/// once.
static ASKED: AtomicBool = AtomicBool::new(false);

fn watch() -> MutexGuard<'static, Watch> {
    WATCH.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The system's double-click interval.
fn interval() -> Duration {
    let seconds = NSEvent::doubleClickInterval();
    if seconds.is_finite() && seconds > 0.0 {
        Duration::from_secs_f64(seconds.min(5.0))
    } else {
        Duration::from_millis(500)
    }
}

/// Watches the double taps of `modifiers`, in place of those before, of
/// `waker`, which wakes the main thread as one ends; none stops the tap.
/// Where the user has not allowed the app to listen to the input, asks,
/// of the system's question, once, and watches as the app becomes active
/// once it is allowed.
pub(crate) fn set(modifiers: &[Modifier], waker: Waker) -> DoubleTaps {
    {
        let mut watch = watch();
        watch.watched = modifiers.to_vec();
        watch.waker = Some(waker);
        watch.finder.set_interval(interval());
    }
    if modifiers.is_empty() {
        stop();
        return DoubleTaps::Off;
    }
    start()
}

/// Watches again as the app becomes active, where it watches some and
/// the tap does not run, as the user may have allowed the app meanwhile,
/// of the double-click interval of now.
pub(crate) fn again() {
    let watching = {
        let mut watch = watch();
        watch.finder.set_interval(interval());
        !watch.watched.is_empty() && watch.tap == 0
    };
    if watching {
        start();
    }
}

/// Starts the tap on a thread of its own, unless it runs: watching, or
/// not allowed.
fn start() -> DoubleTaps {
    if watch().tap != 0 {
        return DoubleTaps::Watching;
    }
    // SAFETY: CoreGraphics' calls of no argument, of any thread.
    if !unsafe { CGPreflightListenEventAccess() } {
        if !ASKED.swap(true, Ordering::AcqRel) {
            // SAFETY: as above.
            unsafe { CGRequestListenEventAccess() };
        }
        return DoubleTaps::NotAllowed;
    }
    let (started, taken) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("lokyr-double-taps".to_owned())
        .spawn(move || run(&started));
    if let Err(error) = spawned {
        eprintln!("lokyr: the double taps are not watched: {error}");
        return DoubleTaps::NotAllowed;
    }
    match taken.recv_timeout(Duration::from_secs(2)) {
        Ok(true) => DoubleTaps::Watching,
        _ => {
            eprintln!("lokyr: the system gave no tap of the modifiers' changes");
            DoubleTaps::NotAllowed
        }
    }
}

/// Stops the tap, if it runs: its thread ends, and takes the tap away.
fn stop() {
    let run_loop = {
        let mut watch = watch();
        watch.tap = 0;
        std::mem::take(&mut watch.run_loop)
    };
    if run_loop != 0 {
        // SAFETY: the run loop, retained by its thread, which the stop
        // ends, of any thread; released here, as the thread keeps no
        // reference of its own past its run.
        unsafe {
            CFRunLoopStop(run_loop as CfRunLoopRef);
            CFRelease(run_loop as CfTypeRef);
        }
    }
}

/// The tap's thread: makes the tap, tells `started` whether it could, and
/// runs its run loop until [`stop`] ends it, then takes the tap away.
fn run(started: &mpsc::Sender<bool>) {
    // SAFETY: CoreGraphics' and CoreFoundation's calls on this thread, of
    // the tap and the source that they make, released at the end; the
    // callback is a function of no captured state.
    unsafe {
        let tap = CGEventTapCreate(
            SESSION,
            HEAD,
            LISTEN_ONLY,
            1 << FLAGS_CHANGED,
            changed,
            ptr::null_mut(),
        );
        if tap.is_null() {
            let _ = started.send(false);
            return;
        }
        let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
        if source.is_null() {
            CFMachPortInvalidate(tap);
            CFRelease(tap.cast_const());
            let _ = started.send(false);
            return;
        }
        let run_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);
        CGEventTapEnable(tap, true);
        {
            let mut watch = watch();
            watch.tap = tap as usize;
            watch.run_loop = CFRetain(run_loop.cast_const()) as usize;
        }
        let _ = started.send(true);
        CFRunLoopRun();
        CFMachPortInvalidate(tap);
        CFRelease(source.cast_const());
        CFRelease(tap.cast_const());
    }
}

/// How long since the last key or click of any app, as the system tells
/// it.
fn idle() -> Duration {
    INPUTS
        .iter()
        .map(|&kind| {
            // SAFETY: a query of the system's input of no pointer.
            let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM, kind) };
            if seconds.is_finite() && seconds >= 0.0 {
                Duration::from_secs_f64(seconds.min(1e9))
            } else {
                Duration::MAX
            }
        })
        .min()
        .unwrap_or(Duration::MAX)
}

/// The tap's callback, on its thread: each change of the modifiers goes to
/// the finder, and a double tap of a watched modifier wakes the main
/// thread; a tap that the system turned off is turned on again. The event
/// goes on as it came.
extern "C" fn changed(
    _proxy: *mut c_void,
    kind: u32,
    event: CgEventRef,
    _info: *mut c_void,
) -> CgEventRef {
    if kind == DISABLED_BY_TIMEOUT || kind == DISABLED_BY_USER_INPUT {
        let tap = watch().tap;
        if tap != 0 {
            // SAFETY: the tap, which lives while its thread runs, as this
            // callback does.
            unsafe { CGEventTapEnable(tap as CfMachPortRef, true) };
        }
        return event;
    }
    if kind != FLAGS_CHANGED || event.is_null() {
        return event;
    }
    // SAFETY: the event that the tap gives, for this call.
    let flags = unsafe { CGEventGetFlags(event) };
    let held = Held {
        control: flags & CONTROL != 0,
        alt: flags & ALTERNATE != 0,
        command: flags & COMMAND != 0,
        shift: flags & SHIFT != 0,
    };
    let idle = idle();
    let now = Instant::now();
    let mut watch = watch();
    if let Some(modifier) = watch.finder.change(held, now, idle)
        && watch.watched.contains(&modifier)
    {
        watch.tapped.push(modifier);
        if let Some(waker) = &watch.waker {
            waker.wake();
        }
    }
    event
}

/// Posts the app's events of the double taps that ended, on the main
/// thread, as its waker woke it.
pub(crate) fn take_tapped(shared: &Shared) {
    let tapped = std::mem::take(&mut watch().tapped);
    for modifier in tapped {
        shared.post(Pending::App(AppEvent::DoubleTap(modifier)));
    }
}

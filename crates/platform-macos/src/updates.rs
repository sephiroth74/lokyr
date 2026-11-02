//! The app's updates, through Sparkle's updater, which the bundle holds as
//! `Contents/Frameworks/Sparkle.framework`
//! (<https://sparkle-project.org/documentation/>) and the app does not
//! link: a thread of GCD loads it, as its first load after an install has
//! macOS check its code's signature for a few hundred milliseconds, and
//! the main thread then makes its standard controller, whose own windows
//! show what a check finds, with what is new, to install it or skip it.
//! The bundle's `Info.plist` gives it its feed and its key; outside a
//! bundle, as `cargo run` runs the app, there is no updater.

use std::cell::RefCell;
use std::ffi::{CStr, CString, c_void};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{MainThreadMarker, msg_send};
use objc2_foundation::{NSBundle, NSString};

/// The framework, in the bundle's private frameworks.
const FRAMEWORK: &str = "Sparkle.framework";

/// The class of Sparkle's controller of the standard windows.
const CONTROLLER: &CStr = c"SPUStandardUpdaterController";

/// A queue of GCD, as `dispatch_queue_t` points to it.
#[repr(C)]
struct DispatchQueue {
    _opaque: [u8; 0],
}

// SAFETY: the declarations are GCD's, of libSystem, which every process
// links: `dispatch_get_global_queue`, `dispatch_async_f` and the main
// queue, which `dispatch_get_main_queue()` gives as `&_dispatch_main_q`
// (<dispatch/queue.h>).
unsafe extern "C" {
    static _dispatch_main_q: DispatchQueue;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut DispatchQueue;
    fn dispatch_async_f(
        queue: *mut DispatchQueue,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

/// GCD's class of service of work that the user does not wait for,
/// `QOS_CLASS_UTILITY` of <sys/qos.h>.
const UTILITY: isize = 0x11;

thread_local! {
    /// The updates that the load ends, on the main thread.
    static LOADING: RefCell<Weak<RefCell<State>>> = const { RefCell::new(Weak::new()) };
}

/// Where the updater is.
#[derive(Debug, Default)]
enum State {
    /// Not started, or no updater in the bundle.
    #[default]
    Idle,
    /// Loading out of the main thread, with its checks now and then, on or
    /// off, and whether the app asked for a check meanwhile.
    Loading { checks: bool, asked: bool },
    /// Sparkle's controller, running.
    Running(Retained<AnyObject>),
    /// The framework did not load, as the log told.
    Failed,
}

/// The work of the load, out of the main thread and back on it: the
/// framework's binary, and why it did not load, if it did not.
struct Load {
    binary: CString,
    failed: Option<String>,
}

/// The app's updates, on the main thread.
#[derive(Debug, Default)]
pub(crate) struct Updates {
    state: Rc<RefCell<State>>,
}

impl Updates {
    /// Starts loading the updater of the bundle, if it has one, with its
    /// checks now and then, on or off: whether it has one, or loads or runs
    /// already, as a second call changes nothing.
    pub(crate) fn start(&self, checks: bool) -> bool {
        match &*self.state.borrow() {
            State::Idle => {}
            State::Loading { .. } | State::Running(_) => return true,
            State::Failed => return false,
        }
        let Some(binary) = framework().map(|framework| framework.join("Sparkle")) else {
            return false;
        };
        let Ok(binary) = CString::new(binary.as_os_str().as_encoded_bytes()) else {
            return false;
        };
        *self.state.borrow_mut() = State::Loading {
            checks,
            asked: false,
        };
        LOADING.with(|loading| *loading.borrow_mut() = Rc::downgrade(&self.state));
        let load = Box::new(Load {
            binary,
            failed: None,
        });
        // SAFETY: the global queue of a class of service exists for the
        // process's life; `load` takes the box back once, as its context.
        unsafe {
            dispatch_async_f(
                dispatch_get_global_queue(UTILITY, 0),
                Box::into_raw(load).cast(),
                load_framework,
            );
        }
        true
    }

    /// Turns the checks now and then on or off, once started.
    pub(crate) fn set_checks(&self, checks: bool) {
        match &mut *self.state.borrow_mut() {
            State::Loading { checks: wanted, .. } => *wanted = checks,
            State::Running(controller) => set_automatic_checks(controller, checks),
            State::Idle | State::Failed => {}
        }
    }

    /// Checks for a newer version now, in Sparkle's windows; once the
    /// updater has loaded, if it is loading.
    pub(crate) fn check(&self) {
        match &mut *self.state.borrow_mut() {
            State::Loading { asked, .. } => *asked = true,
            State::Running(controller) => check_now(controller),
            State::Idle | State::Failed => {}
        }
    }
}

/// The bundle's Sparkle's framework, if the bundle has it.
fn framework() -> Option<PathBuf> {
    let frameworks = NSBundle::mainBundle().privateFrameworksPath()?;
    let framework = Path::new(&frameworks.to_string()).join(FRAMEWORK);
    framework.is_dir().then_some(framework)
}

/// Loads the framework's binary, out of the main thread, then hands on to
/// the main thread.
extern "C" fn load_framework(context: *mut c_void) {
    // SAFETY: the context is the box that `Updates::start` gave, once.
    let mut load = unsafe { Box::from_raw(context.cast::<Load>()) };
    // SAFETY: the path is a C string; `dlopen` and `dlerror` are safe from
    // any thread, and the error's text is read before another call.
    unsafe {
        if libc::dlopen(load.binary.as_ptr(), libc::RTLD_NOW).is_null() {
            let error = libc::dlerror();
            load.failed = Some(if error.is_null() {
                "an unknown error".to_owned()
            } else {
                CStr::from_ptr(error).to_string_lossy().into_owned()
            });
        }
    }
    // SAFETY: the main queue exists for the process's life; `framework_loaded`
    // takes the box back once.
    unsafe {
        dispatch_async_f(
            (&raw const _dispatch_main_q).cast_mut(),
            Box::into_raw(load).cast(),
            framework_loaded,
        );
    }
}

/// Starts Sparkle's controller once its framework has loaded, on the main
/// thread, or tells why it did not.
extern "C" fn framework_loaded(context: *mut c_void) {
    // SAFETY: the context is the box that `load_framework` gave, once.
    let load = unsafe { Box::from_raw(context.cast::<Load>()) };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(state) = LOADING.with(|loading| loading.borrow().upgrade()) else {
        return;
    };
    let State::Loading { checks, asked } = *state.borrow() else {
        return;
    };
    let controller = match load.failed {
        Some(error) => Err(error),
        None => controller(mtm),
    };
    match controller {
        Ok(controller) => {
            set_automatic_checks(&controller, checks);
            // SAFETY: `startUpdater` of `SPUStandardUpdaterController`, on
            // the main thread, after its settings; a failure shows Sparkle's
            // own alert.
            let _: () = unsafe { msg_send![&controller, startUpdater] };
            if asked {
                check_now(&controller);
            }
            *state.borrow_mut() = State::Running(controller);
        }
        Err(error) => {
            eprintln!("lokyr: the updater did not load, and the app cannot update itself: {error}");
            *state.borrow_mut() = State::Failed;
        }
    }
}

/// Sparkle's standard controller, its updater not started, once the
/// framework has loaded.
fn controller(_mtm: MainThreadMarker) -> Result<Retained<AnyObject>, String> {
    // The bundle of the binary that `dlopen` loaded, which Sparkle looks
    // for among those that Foundation knows.
    if let Some(framework) = framework() {
        let bundle = NSBundle::bundleWithPath(&NSString::from_str(&framework.to_string_lossy()));
        // SAFETY: the framework's code, which `dlopen` loaded already and
        // which runs nothing as it loads, but its classes' `+load`.
        if !bundle.is_some_and(|bundle| unsafe { bundle.load() }) {
            return Err(format!("{} does not load as a bundle", framework.display()));
        }
    }
    let class = AnyClass::get(CONTROLLER)
        .ok_or_else(|| format!("the framework has no {}", CONTROLLER.to_string_lossy()))?;
    // SAFETY: `+alloc` of an `NSObject`'s class.
    let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
    let none: Option<&AnyObject> = None;
    // SAFETY: `-initWithStartingUpdater:updaterDelegate:userDriverDelegate:`
    // of `SPUStandardUpdaterController`, on the main thread, without
    // delegates, which are optional.
    let controller: Option<Retained<AnyObject>> = unsafe {
        msg_send![
            allocated,
            initWithStartingUpdater: Bool::NO,
            updaterDelegate: none,
            userDriverDelegate: none
        ]
    };
    controller.ok_or_else(|| "Sparkle's controller did not start".to_owned())
}

/// Turns the updater's checks now and then, a day apart as the bundle's
/// `Info.plist` sets, on or off.
fn set_automatic_checks(controller: &AnyObject, checks: bool) {
    // SAFETY: `updater` of `SPUStandardUpdaterController`, and the property
    // `automaticallyChecksForUpdates` of `SPUUpdater`, on the main thread.
    unsafe {
        let updater: Retained<AnyObject> = msg_send![controller, updater];
        let _: () = msg_send![&updater, setAutomaticallyChecksForUpdates: Bool::new(checks)];
    }
}

/// Checks for a newer version now, in Sparkle's windows.
fn check_now(controller: &AnyObject) {
    let sender: Option<&AnyObject> = None;
    // SAFETY: the action `checkForUpdates:` of
    // `SPUStandardUpdaterController`, whose sender may be nil, on the main
    // thread.
    let _: () = unsafe { msg_send![controller, checkForUpdates: sender] };
}

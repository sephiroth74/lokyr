//! A VM's watchdog: a thread of its own, asleep while its plugin's code
//! does not run, which makes code that runs past its deadline fail, at its
//! next loop or call, native code too, through Luau's interrupt, which
//! costs nothing until it is set.
//!
//! The interrupt is a field of the state's callbacks, which the watchdog
//! writes from its thread while the code reads it, as Luau's own command
//! line writes it from a signal's handler at a Control-C: a store of one
//! aligned pointer, which a 64-bit CPU makes whole. The clock stops while
//! the code waits for the host, whose time is not the plugin's.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mlua::ffi;

/// What the watchdog watches, under its lock.
#[derive(Default)]
struct Watch {
    /// The deadline of the call under way, if one is.
    deadline: Option<Instant>,
    /// Whether the call's deadline passed, and the interrupt was set.
    fired: bool,
    /// Whether the watchdog ends.
    quit: bool,
}

/// What the watchdog and the VM share.
#[derive(Default)]
struct Shared {
    watch: Mutex<Watch>,
    wake: Condvar,
    /// How many times the watchdog's thread woke, which a VM at rest does
    /// not make it.
    wakes: AtomicUsize,
}

/// A VM's watchdog.
pub(crate) struct Watchdog {
    shared: Arc<Shared>,
    state: usize,
    thread: Option<JoinHandle<()>>,
}

impl Watchdog {
    /// The watchdog of the Lua state at `state`, as an address.
    ///
    /// # Safety
    ///
    /// The state outlives the watchdog, whose drop ends its thread.
    #[allow(unsafe_code)]
    pub(crate) unsafe fn new(state: usize, name: &str) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let watching = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name(format!("lokyr-watchdog {name}"))
            .spawn(move || watch(&watching, state))?;
        Ok(Self {
            shared,
            state,
            thread: Some(thread),
        })
    }

    /// Watches a call until `deadline`, past which its code fails: the
    /// interrupt of a deadline that passed as an earlier call ended goes
    /// first.
    pub(crate) fn arm(&self, deadline: Instant) {
        let mut watch = self.lock();
        // SAFETY: the state is alive, as the watchdog is, and no code of
        // its runs between the calls.
        #[allow(unsafe_code)]
        unsafe {
            set_interrupt(self.state, None);
        }
        watch.deadline = Some(deadline);
        watch.fired = false;
        self.shared.wake.notify_one();
    }

    /// Stops the clock of the call under way, as its code waits for the
    /// host, whose time is not the plugin's: what was left of its deadline,
    /// if it had one.
    pub(crate) fn pause(&self) -> Option<Duration> {
        let mut watch = self.lock();
        let left = watch
            .deadline
            .take()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()));
        self.shared.wake.notify_one();
        left
    }

    /// Starts the clock of the call under way again, of what was `left` of
    /// its deadline, unless it passed.
    pub(crate) fn resume(&self, left: Option<Duration>) {
        let mut watch = self.lock();
        if let Some(left) = left
            && !watch.fired
        {
            watch.deadline = Some(Instant::now() + left);
            self.shared.wake.notify_one();
        }
    }

    /// Ends the call's watch: whether its deadline passed, which made its
    /// code fail, if it ran then.
    pub(crate) fn disarm(&self) -> bool {
        let mut watch = self.lock();
        watch.deadline = None;
        self.shared.wake.notify_one();
        watch.fired
    }

    /// How many times its thread woke.
    #[cfg(test)]
    pub(crate) fn wakes(&self) -> usize {
        self.shared.wakes.load(Ordering::Relaxed)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Watch> {
        self.shared
            .watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.lock().quit = true;
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The watchdog's thread: asleep until a call is watched, then until its
/// deadline, which sets the interrupt of the state at `state`, once.
fn watch(shared: &Shared, state: usize) {
    let mut watch = shared.watch.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        shared.wakes.fetch_add(1, Ordering::Relaxed);
        if watch.quit {
            return;
        }
        watch = match watch.deadline {
            Some(deadline) if !watch.fired => {
                let now = Instant::now();
                if now >= deadline {
                    // SAFETY: the state outlives the watchdog's thread, and
                    // the lock keeps the call from ending in between.
                    #[allow(unsafe_code)]
                    unsafe {
                        set_interrupt(state, Some(stop));
                    }
                    watch.fired = true;
                    continue;
                }
                shared
                    .wake
                    .wait_timeout(watch, deadline - now)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            _ => shared
                .wake
                .wait(watch)
                .unwrap_or_else(PoisonError::into_inner),
        };
    }
}

/// Sets the interrupt of the state at `state`.
///
/// # Safety
///
/// The state is alive.
#[allow(unsafe_code)]
unsafe fn set_interrupt(
    state: usize,
    interrupt: Option<unsafe extern "C-unwind" fn(*mut ffi::lua_State, std::ffi::c_int)>,
) {
    // SAFETY: the caller vouches for the state, whose callbacks live as
    // long; the store is of one aligned pointer.
    unsafe {
        let callbacks = ffi::lua_callbacks(state as *mut ffi::lua_State);
        std::ptr::addr_of_mut!((*callbacks).interrupt).write_volatile(interrupt);
    }
}

/// The interrupt of a call past its deadline: the code fails, at each of
/// its loops and calls until the call ends, so that a `pcall` that catches
/// the error ends at its own next loop or call; but in the collector, which
/// calls it of a step, and where an error may not be raised.
#[allow(unsafe_code)]
unsafe extern "C-unwind" fn stop(state: *mut ffi::lua_State, gc: std::ffi::c_int) {
    if gc >= 0 {
        return;
    }
    // SAFETY: Luau calls the interrupt with a valid state, at a point where
    // it may raise an error.
    unsafe {
        ffi::lua_rawcheckstack(state, 1);
        ffi::luaL_error(state, c"the call ran past its deadline".as_ptr());
    }
}

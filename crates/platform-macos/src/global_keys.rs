//! The keys that the app takes from every app: Carbon's hot keys, which
//! the system gives the app wherever the focus is, without the
//! Accessibility permission that a tap of the system's events takes.

use std::ffi::c_void;
use std::ptr;
use std::rc::Weak;

use lokyr_platform_core::{AppEvent, GlobalKey};

use crate::keys;
use crate::shared::{Pending, Shared};

type OsStatus = i32;
type EventRef = *mut c_void;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventHandler = extern "C" fn(*mut c_void, EventRef, *mut c_void) -> OsStatus;

/// A class and a kind of Carbon's events.
#[repr(C)]
struct EventTypeSpec {
    class: u32,
    kind: u32,
}

/// A hot key's own name: the app's signature, and the key's number.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// The target of the application's events.
    fn GetApplicationEventTarget() -> EventTargetRef;
    /// Calls `handler` with the events of `types` that reach `target`.
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandler,
        count: usize,
        types: *const EventTypeSpec,
        data: *mut c_void,
        installed: *mut EventHandlerRef,
    ) -> OsStatus;
    fn RemoveEventHandler(handler: EventHandlerRef) -> OsStatus;
    /// Takes the key of the virtual code `code` with `modifiers` from every
    /// app, for `target`, whose events tell it by `id`.
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: EventTargetRef,
        options: u32,
        registered: *mut EventHotKeyRef,
    ) -> OsStatus;
    fn UnregisterEventHotKey(key: EventHotKeyRef) -> OsStatus;
    /// Reads the parameter `name` of `event`, of the type `kind`, into
    /// `data`, of `size` bytes.
    fn GetEventParameter(
        event: EventRef,
        name: u32,
        kind: u32,
        actual_kind: *mut u32,
        size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OsStatus;
}

/// Carbon's codes of four characters.
const fn code(name: [u8; 4]) -> u32 {
    u32::from_be_bytes(name)
}

/// The keyboard's events, and among them a hot key's press.
const KEYBOARD: u32 = code(*b"keyb");
const HOT_KEY_PRESSED: u32 = 5;
/// The event's parameter that names its hot key, and its type.
const DIRECT_OBJECT: u32 = code(*b"----");
const HOT_KEY_ID: u32 = code(*b"hkid");
/// The app's signature on its hot keys.
const SIGNATURE: u32 = code(*b"LKYR");
/// The modifiers, as Carbon's `cmdKey`, `shiftKey`, `optionKey` and
/// `controlKey` give them.
const COMMAND: u32 = 1 << 8;
const SHIFT: u32 = 1 << 9;
const OPTION: u32 = 1 << 11;
const CONTROL: u32 = 1 << 12;
/// Carbon's answer for an event that its handler leaves to the next one.
const NOT_HANDLED: OsStatus = -9874;

/// The keys that the app takes, while it takes some.
pub(crate) struct GlobalKeys {
    /// What the handler posts the presses to, which it reaches through the
    /// box's address.
    shared: Box<Weak<Shared>>,
    handler: EventHandlerRef,
    registered: Vec<EventHotKeyRef>,
}

impl GlobalKeys {
    pub(crate) fn new(shared: Weak<Shared>) -> Self {
        Self {
            shared: Box::new(shared),
            handler: ptr::null_mut(),
            registered: Vec::new(),
        }
    }

    /// Takes `keys` in place of the keys before; gives the indices of those
    /// that the system refused, which the log tells.
    pub(crate) fn set(&mut self, keys: &[GlobalKey]) -> Vec<usize> {
        self.release();
        if keys.is_empty() {
            return Vec::new();
        }
        // SAFETY: Carbon's calls, on the main thread: the handler reads the
        // box, which lives until the handler is removed, as `Drop` does
        // first.
        unsafe {
            if self.handler.is_null() {
                let pressed = EventTypeSpec {
                    class: KEYBOARD,
                    kind: HOT_KEY_PRESSED,
                };
                let data = ptr::from_ref::<Weak<Shared>>(&*self.shared)
                    .cast_mut()
                    .cast();
                let status = InstallEventHandler(
                    GetApplicationEventTarget(),
                    pressed_key,
                    1,
                    &raw const pressed,
                    data,
                    &raw mut self.handler,
                );
                if status != 0 {
                    eprintln!("lokyr: the global keys cannot be taken: {status}");
                    self.handler = ptr::null_mut();
                    return (0..keys.len()).collect();
                }
            }
        }
        let mut refused = Vec::new();
        for (index, key) in keys.iter().enumerate() {
            let Some(code) = keys::virtual_code(key.code) else {
                refused.push(index);
                continue;
            };
            let modifiers = [
                (key.command, COMMAND),
                (key.shift, SHIFT),
                (key.alt, OPTION),
                (key.control, CONTROL),
            ]
            .into_iter()
            .filter(|(held, _)| *held)
            .fold(0, |all, (_, modifier)| all | modifier);
            let id = EventHotKeyId {
                signature: SIGNATURE,
                id: u32::try_from(index).unwrap_or(u32::MAX),
            };
            let mut registered = ptr::null_mut();
            // SAFETY: Carbon's call, on the main thread, of a key that the
            // `Drop` gives back.
            let status = unsafe {
                RegisterEventHotKey(
                    u32::from(code),
                    modifiers,
                    id,
                    GetApplicationEventTarget(),
                    0,
                    &raw mut registered,
                )
            };
            if status == 0 {
                self.registered.push(registered);
            } else {
                eprintln!("lokyr: the global key {key:?} cannot be taken: {status}");
                refused.push(index);
            }
        }
        refused
    }

    /// Gives the keys back.
    fn release(&mut self) {
        for key in self.registered.drain(..) {
            // SAFETY: a key that `set` registered, once.
            unsafe { UnregisterEventHotKey(key) };
        }
    }
}

impl Drop for GlobalKeys {
    fn drop(&mut self) {
        self.release();
        if !self.handler.is_null() {
            // SAFETY: the handler that `set` installed, once.
            unsafe { RemoveEventHandler(self.handler) };
        }
    }
}

/// Carbon's handler of the presses of the hot keys, which posts each of the
/// app's to the app.
extern "C" fn pressed_key(_call: *mut c_void, event: EventRef, data: *mut c_void) -> OsStatus {
    let mut id = EventHotKeyId::default();
    // SAFETY: the event of a hot key, whose parameter is an id of this size.
    let status = unsafe {
        GetEventParameter(
            event,
            DIRECT_OBJECT,
            HOT_KEY_ID,
            ptr::null_mut(),
            size_of::<EventHotKeyId>(),
            ptr::null_mut(),
            (&raw mut id).cast(),
        )
    };
    if status != 0 || id.signature != SIGNATURE {
        return NOT_HANDLED;
    }
    // SAFETY: the box's address, which lives as long as the handler.
    let shared = unsafe { &*data.cast::<Weak<Shared>>() };
    if let Some(shared) = shared.upgrade() {
        let index = usize::try_from(id.id).unwrap_or(usize::MAX);
        shared.post(Pending::App(AppEvent::GlobalKey(index)));
    }
    0
}

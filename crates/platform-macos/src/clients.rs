//! Who sends the app a script's request, as the system tells it rather
//! than as the sender names itself: an app, by its code signature's team
//! and identifier; a command-line program, as `osascript`, by the app that
//! it runs in and its own path; a program in one of the app's own
//! terminals, by its process, whose answer holds while it runs alone.

use std::ffi::{CStr, c_char, c_void};

use lokyr_platform_core::automation::Client;
use objc2_app_kit::NSRunningApplication;

/// The most processes that a sender's parents are looked at of, past which
/// the walk stops.
const MOST_PARENTS: usize = 64;

/// The client of the process `pid`, which sent a script's request.
pub(crate) fn of_pid(pid: i32) -> Client {
    let own = i32::try_from(std::process::id()).unwrap_or(0);
    let chain = ancestors(pid);
    let program = program_name(pid).unwrap_or_else(|| "a program".to_owned());
    // A program of one of the app's own terminals, whose shell the app
    // started: macOS asks nothing of it, and one program is not the next.
    if pid != own && chain.contains(&own) {
        return Client {
            id: format!("terminal:{pid}:{}", start_time(pid)),
            name: format!("{program}, in a Lokyr terminal"),
            terminal: true,
            waits: false,
        };
    }
    // The app responsible: the sender, or the first of its parents that is
    // an app, as the terminal app that ran `osascript`.
    let app = chain.iter().find_map(|&process| app_of(process));
    match app {
        Some((app_pid, name)) => {
            let signed = signature(app_pid)
                .or_else(|| program_path(app_pid).map(|path| format!("path:{path}")))
                .unwrap_or_else(|| format!("process:{app_pid}:{}", start_time(app_pid)));
            if app_pid == pid {
                Client {
                    id: format!("app:{signed}"),
                    name,
                    terminal: false,
                    waits: false,
                }
            } else {
                let path = program_path(pid).unwrap_or_else(|| program.clone());
                Client {
                    id: format!("app:{signed}:{path}"),
                    name: format!("{program} in {name}"),
                    terminal: false,
                    waits: false,
                }
            }
        }
        // No app: a program of no app's, as of `cron` or `ssh`.
        None => Client {
            id: format!(
                "program:{}",
                program_path(pid).unwrap_or_else(|| format!("{pid}:{}", start_time(pid)))
            ),
            name: program,
            terminal: false,
            waits: false,
        },
    }
}

/// The client of the App Intents, which the system runs in the app's
/// process for the user's shortcuts, Siri and Spotlight: one of its own,
/// which waits for the user's answer, as an intent awaits its result.
pub(crate) fn shortcuts() -> Client {
    Client {
        id: "shortcuts".to_owned(),
        name: "Shortcuts".to_owned(),
        terminal: false,
        waits: true,
    }
}

/// `pid` and its parents, from it up, [`MOST_PARENTS`] at most.
fn ancestors(pid: i32) -> Vec<i32> {
    let mut chain = vec![pid];
    let mut current = pid;
    while chain.len() < MOST_PARENTS {
        let Some(parent) = bsd_info(current).map(|info| info.pbi_ppid) else {
            break;
        };
        let Ok(parent) = i32::try_from(parent) else {
            break;
        };
        if parent <= 1 || chain.contains(&parent) {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}

/// The process `pid`'s BSD information, if it runs.
fn bsd_info(pid: i32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: an all-zero `proc_bsdinfo` is valid, of plain integers.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: the buffer is a `proc_bsdinfo` of `size` bytes, which the
    // call fills for `PROC_PIDTBSDINFO`.
    let read =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut info).cast(), size) };
    (read == size).then_some(info)
}

/// When the process `pid` started, in microseconds of the epoch, which
/// tells it from a later process of its id; 0 of one that is gone.
fn start_time(pid: i32) -> u64 {
    bsd_info(pid).map_or(0, |info| {
        info.pbi_start_tvsec
            .saturating_mul(1_000_000)
            .saturating_add(info.pbi_start_tvusec)
    })
}

/// The name of the process `pid`'s executable, as `osascript`.
fn program_name(pid: i32) -> Option<String> {
    let path = program_path(pid)?;
    path.rsplit('/').next().map(str::to_owned)
}

/// The path of the process `pid`'s executable.
fn program_path(pid: i32) -> Option<String> {
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let size = u32::try_from(path.len()).ok()?;
    // SAFETY: the buffer holds `size` bytes, which the call fills with a
    // path ended by a zero.
    let length = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), size) };
    let length = usize::try_from(length).ok().filter(|&length| length > 0)?;
    Some(String::from_utf8_lossy(&path[..length]).into_owned())
}

/// The process `pid`, if it is an app of a bundle, and its name.
fn app_of(pid: i32) -> Option<(i32, String)> {
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
    app.bundleIdentifier()?;
    let name = app
        .localizedName()
        .map(|name| name.to_string())
        .or_else(|| program_name(pid))?;
    Some((pid, name))
}

/// The code signature of the process `pid`, if it is valid: its team and
/// its signing identifier, as `TEAMID:com.example.app`; `apple` for Apple's
/// own code, of no team; none of code unsigned or of a signature that the
/// system refuses.
fn signature(pid: i32) -> Option<String> {
    // SAFETY: each call is of CoreFoundation's and Security's, of the
    // objects that the calls before made, each released once.
    unsafe {
        let number = CFNumberCreate(
            std::ptr::null(),
            K_CF_NUMBER_SINT32_TYPE,
            (&raw const pid).cast(),
        );
        if number.is_null() {
            return None;
        }
        let keys = [kSecGuestAttributePid];
        let values = [number];
        let attributes = CFDictionaryCreate(
            std::ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            1,
            (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
            (&raw const kCFTypeDictionaryValueCallBacks).cast(),
        );
        CFRelease(number);
        if attributes.is_null() {
            return None;
        }
        let mut code: *const c_void = std::ptr::null();
        let status = SecCodeCopyGuestWithAttributes(std::ptr::null(), attributes, 0, &raw mut code);
        CFRelease(attributes);
        if status != 0 || code.is_null() {
            return None;
        }
        let signed = signature_of(code);
        CFRelease(code);
        signed
    }
}

/// The signature of `code`, a `SecCode`, as [`signature`] gives it.
///
/// # Safety
///
/// `code` is a `SecCode` that lives through the call.
unsafe fn signature_of(code: *const c_void) -> Option<String> {
    // SAFETY: as the function's contract says, and of the objects that the
    // calls make, each released once.
    unsafe {
        if SecCodeCheckValidity(code, 0, std::ptr::null()) != 0 {
            return None;
        }
        let mut information: *const c_void = std::ptr::null();
        let status =
            SecCodeCopySigningInformation(code, K_SEC_CS_SIGNING_INFORMATION, &raw mut information);
        if status != 0 || information.is_null() {
            return None;
        }
        let identifier = string_of(CFDictionaryGetValue(information, kSecCodeInfoIdentifier));
        let team = string_of(CFDictionaryGetValue(
            information,
            kSecCodeInfoTeamIdentifier,
        ));
        CFRelease(information);
        let identifier = identifier?;
        match team {
            Some(team) => Some(format!("{team}:{identifier}")),
            None if is_apple(code) => Some(format!("apple:{identifier}")),
            None => None,
        }
    }
}

/// Whether `code`, a `SecCode`, is Apple's own, as its anchor tells.
///
/// # Safety
///
/// `code` is a `SecCode` that lives through the call.
unsafe fn is_apple(code: *const c_void) -> bool {
    // SAFETY: as the function's contract says, of the requirement that the
    // call makes, released once.
    unsafe {
        let text = CFStringCreateWithCString(
            std::ptr::null(),
            c"anchor apple".as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        );
        if text.is_null() {
            return false;
        }
        let mut requirement: *const c_void = std::ptr::null();
        let status = SecRequirementCreateWithString(text, 0, &raw mut requirement);
        CFRelease(text);
        if status != 0 || requirement.is_null() {
            return false;
        }
        let apple = SecCodeCheckValidity(code, 0, requirement) == 0;
        CFRelease(requirement);
        apple
    }
}

/// The text of `value`, a `CFString`, or none of a null or another type.
///
/// # Safety
///
/// `value` is a CoreFoundation object that lives through the call, or
/// null.
unsafe fn string_of(value: *const c_void) -> Option<String> {
    if value.is_null() {
        return None;
    }
    // SAFETY: as the function's contract says; the buffer holds its
    // length's bytes, which the call fills with a text ended by a zero.
    unsafe {
        if CFGetTypeID(value) != CFStringGetTypeID() {
            return None;
        }
        let mut buffer = [0 as c_char; 512];
        let length = isize::try_from(buffer.len()).ok()?;
        if CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            length,
            K_CF_STRING_ENCODING_UTF8,
        ) == 0
        {
            return None;
        }
        Some(
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// CoreFoundation's `kCFNumberSInt32Type`.
const K_CF_NUMBER_SINT32_TYPE: isize = 3;
/// CoreFoundation's `kCFStringEncodingUTF8`.
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
/// Security's `kSecCSSigningInformation`.
const K_SEC_CS_SIGNING_INFORMATION: u32 = 1 << 1;

/// A CoreFoundation structure of no field that Rust reads, as callbacks'.
#[repr(C)]
struct Opaque {
    _private: [u8; 0],
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeDictionaryKeyCallBacks: Opaque;
    static kCFTypeDictionaryValueCallBacks: Opaque;
    fn CFNumberCreate(allocator: *const c_void, kind: isize, value: *const c_void)
    -> *const c_void;
    fn CFDictionaryCreate(
        allocator: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        text: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFStringGetCString(
        text: *const c_void,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> u8;
    fn CFGetTypeID(object: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFRelease(object: *const c_void);
}

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    static kSecGuestAttributePid: *const c_void;
    static kSecCodeInfoIdentifier: *const c_void;
    static kSecCodeInfoTeamIdentifier: *const c_void;
    fn SecCodeCopyGuestWithAttributes(
        host: *const c_void,
        attributes: *const c_void,
        flags: u32,
        guest: *mut *const c_void,
    ) -> i32;
    fn SecCodeCheckValidity(code: *const c_void, flags: u32, requirement: *const c_void) -> i32;
    fn SecCodeCopySigningInformation(
        code: *const c_void,
        flags: u32,
        information: *mut *const c_void,
    ) -> i32;
    fn SecRequirementCreateWithString(
        text: *const c_void,
        flags: u32,
        requirement: *mut *const c_void,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_of_the_app_is_a_program_of_its_terminals_for_its_process_alone() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let pid = i32::try_from(child.id()).unwrap();
        let client = of_pid(pid);
        let _ = child.kill();
        let _ = child.wait();
        assert!(client.terminal, "{client:?}");
        assert!(
            client.id.starts_with(&format!("terminal:{pid}:")),
            "{client:?}"
        );
        assert_eq!(client.name, "sleep, in a Lokyr terminal");
        assert!(!client.waits);
    }

    #[test]
    fn apple_s_own_code_is_told_by_its_signature() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let pid = i32::try_from(child.id()).unwrap();
        let signed = signature(pid);
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(signed.as_deref(), Some("apple:com.apple.sleep"));
    }

    #[test]
    fn the_shortcuts_are_one_client_that_waits() {
        let client = shortcuts();
        assert_eq!(client.id, "shortcuts");
        assert!(client.waits && !client.terminal);
    }
}

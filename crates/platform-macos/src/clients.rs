//! Who sends the app a script's request, as the system tells it rather
//! than as the sender names itself: an app, by its code signature's team
//! and identifier; a command-line program, as `osascript`, by the app that
//! it runs in and its own path; a program in one of the app's own
//! terminals, by its pane's first process and the programs from it down to
//! the sender, whose answer holds in that pane while its shell runs, its
//! pane found of its terminal's session where it left its parents; a
//! program of no app above it, as in a tmux or an ssh session, by its
//! path, whose answer holds while the app runs alone. Each named by its
//! executable's whole path, so that a program that takes another's name
//! shows as itself.

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
    let path = program_path(pid);
    let program = path.clone().unwrap_or_else(|| "a program".to_owned());
    let in_lokyr = format!("{program}, in a Lokyr terminal");
    // A program of one of the app's own terminals, whose shell the app
    // started: macOS asks nothing of it.
    if pid != own
        && let Some(index) = chain.iter().position(|&process| process == own)
    {
        return Client {
            id: in_terminal(&chain[..index]),
            name: in_lokyr,
            terminal: true,
            waits: false,
            fleeting: false,
        };
    }
    let path_or_process = || {
        path.clone()
            .unwrap_or_else(|| format!("{pid}:{}", start_time(pid)))
    };
    // One that left its parents, as by a double fork, or past the parents
    // looked at: of its pane still, its terminal's session, whose leader is
    // the pane's first process, the app's child.
    if pid != own
        && let Some(leader) = session_of(pid)
        && leader != own
        && parent_of(leader) == Some(own)
    {
        return Client {
            id: format!(
                "terminal:{leader}:{}>detached>{}",
                start_time(leader),
                path_or_process()
            ),
            name: in_lokyr,
            terminal: true,
            waits: false,
            fleeting: false,
        };
    }
    // One that the app is responsible for, of no pane that tells it, as in
    // a tmux that one of its terminals started: its answer held while the
    // app runs alone.
    if pid != own && responsible_of(pid) == Some(own) {
        return Client {
            id: format!("terminal:lokyr>{}", path_or_process()),
            name: in_lokyr,
            terminal: false,
            waits: false,
            fleeting: true,
        };
    }
    // Past the parents looked at, the app that runs it unknown.
    let cut = chain.len() >= MOST_PARENTS;
    // The app responsible: the sender, or the first of its parents that is
    // an app, as the terminal app that ran `osascript`.
    let app = chain.iter().find_map(|&process| app_of(process));
    match app {
        // Past the parents looked at, of no app among them: as of no app.
        None if cut => Client {
            id: format!("program:{}", path_or_process()),
            name: program,
            terminal: false,
            waits: false,
            fleeting: true,
        },
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
                    fleeting: false,
                }
            } else {
                Client {
                    id: format!("app:{signed}:{program}"),
                    name: format!("{program} in {name}"),
                    terminal: false,
                    waits: false,
                    fleeting: false,
                }
            }
        }
        // No app: a program of no app's, as of `cron`, `ssh` or a tmux,
        // whose path any of them shares: its answer held while the app
        // runs alone.
        None => Client {
            id: format!("program:{}", path_or_process()),
            name: program,
            terminal: false,
            waits: false,
            fleeting: true,
        },
    }
}

/// The leader of the process `pid`'s session, of its terminal: a pane's
/// first process, for the programs of its shell that kept its session.
fn session_of(pid: i32) -> Option<i32> {
    // SAFETY: `getsid` reads the session of a process, of no memory.
    let leader = unsafe { libc::getsid(pid) };
    (leader > 1).then_some(leader)
}

/// The process responsible for `pid`, as macOS's privacy prompts take it:
/// the app that started it, of its children too, unless one disclaimed it.
/// Of the system's library, looked up as it runs, as no header declares
/// it; none where it is not there.
fn responsible_of(pid: i32) -> Option<i32> {
    type Responsible = unsafe extern "C" fn(i32) -> i32;
    static FUNCTION: std::sync::OnceLock<Option<Responsible>> = std::sync::OnceLock::new();
    let function = FUNCTION.get_or_init(|| {
        // SAFETY: the name is a C string, which `dlsym` looks up in the
        // libraries loaded, of a function of this signature where it is
        // there.
        let symbol = unsafe {
            libc::dlsym(
                libc::RTLD_DEFAULT,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            )
        };
        // SAFETY: the symbol, where found, is the function of this
        // signature.
        (!symbol.is_null())
            .then(|| unsafe { std::mem::transmute::<*mut c_void, Responsible>(symbol) })
    });
    // SAFETY: the function reads a process's responsible one, of no memory.
    let responsible = unsafe { (*function)?(pid) };
    (responsible > 0).then_some(responsible)
}

/// The id of a program in one of the app's own terminals, of `chain`, the
/// sender then its parents up to the app's child that the pane started:
/// that child, of its pid and start time, which tell the pane while its
/// shell runs, then the programs' paths from it down to the sender, of no
/// pid, so that the same program run again from the same place, as
/// `osascript` at the prompt, is the same client, and another pane, or a
/// program that another program runs, is not.
fn in_terminal(chain: &[i32]) -> String {
    let Some((&first, below)) = chain.split_last() else {
        return "terminal".to_owned();
    };
    let mut id = format!("terminal:{first}:{}", start_time(first));
    for &process in below.iter().rev() {
        id.push('>');
        match program_path(process) {
            Some(path) => id.push_str(&path),
            None => id.push_str(&format!("{process}:{}", start_time(process))),
        }
    }
    id
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
        fleeting: false,
    }
}

/// `pid` and its parents, from it up, [`MOST_PARENTS`] at most.
fn ancestors(pid: i32) -> Vec<i32> {
    let mut chain = vec![pid];
    let mut current = pid;
    while chain.len() < MOST_PARENTS {
        let Some(parent) = parent_of(current) else {
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

/// The parent of the process `pid`, if it runs: of its short BSD
/// information, which the system gives of another user's process too, as
/// the root-owned `login` that a terminal's shell runs under, whose whole
/// BSD information it refuses.
fn parent_of(pid: i32) -> Option<i32> {
    // SAFETY: an all-zero `proc_bsdshortinfo` is valid, of plain integers
    // and characters.
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdshortinfo>()).ok()?;
    // SAFETY: the buffer is a `proc_bsdshortinfo` of `size` bytes, which
    // the call fills for `PROC_PIDT_SHORTBSDINFO`.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDT_SHORTBSDINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    (read == size)
        .then_some(info.pbsi_ppid)
        .and_then(|parent| i32::try_from(parent).ok())
}

/// The bytes of a `struct kinfo_proc` that the system gives of a process,
/// 648 of a 64-bit process, the room larger.
const KINFO_PROC_ROOM: usize = 1024;

/// When the process `pid` started, in microseconds of the epoch, which
/// tells it from a later process of its id; 0 of one that is gone. Read of
/// the kernel's `kinfo_proc`, whose first field is the start's `timeval`
/// (`p_starttime`), which the system gives of another user's process too.
fn start_time(pid: i32) -> u64 {
    let mut name = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];
    let mut info = [0u8; KINFO_PROC_ROOM];
    let mut size = info.len();
    // SAFETY: `name` holds the four integers of the query, and `info` the
    // `size` bytes that the call fills, telling how many it wrote.
    let read = unsafe {
        libc::sysctl(
            name.as_mut_ptr(),
            4,
            info.as_mut_ptr().cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    // A `timeval`: its seconds, an `i64`, then its microseconds, an `i32`.
    if read != 0 || size < 12 {
        return 0;
    }
    let seconds = i64::from_ne_bytes(info[..8].try_into().unwrap_or_default());
    let micros = i32::from_ne_bytes(info[8..12].try_into().unwrap_or_default());
    match (u64::try_from(seconds), u64::try_from(micros)) {
        (Ok(seconds), Ok(micros)) => seconds.saturating_mul(1_000_000).saturating_add(micros),
        _ => 0,
    }
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
    fn a_child_of_the_app_is_a_program_of_its_terminals() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let pid = i32::try_from(child.id()).unwrap();
        let client = of_pid(pid);
        let _ = child.kill();
        let _ = child.wait();
        assert!(client.terminal, "{client:?}");
        // The app's own child is the pane's first process.
        assert!(
            client.id.starts_with(&format!("terminal:{pid}:")),
            "{client:?}"
        );
        assert_eq!(client.name, "/bin/sleep, in a Lokyr terminal");
        assert!(!client.waits && !client.fleeting);
    }

    #[test]
    fn a_program_that_left_its_pane_s_shell_is_of_its_pane_still() {
        use std::os::unix::process::CommandExt;
        // The pane's first process, the app's child of a session of its
        // own, as a terminal's; its program forks twice, launchd its
        // parent then.
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", "( /bin/sleep 7 & ); /bin/sleep 7"]);
        // SAFETY: `setsid` is safe to call after the fork, of no memory.
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut shell = command.spawn().unwrap();
        let leader = i32::try_from(shell.id()).unwrap();
        let mut detached = None;
        for _ in 0..200 {
            detached = processes()
                .into_iter()
                .find(|&pid| session_of(pid) == Some(leader) && parent_of(pid) == Some(1));
            if detached.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let detached = detached.expect("the program of no parent but launchd");
        let client = of_pid(detached);
        // SAFETY: a signal to the processes of the session, which the test
        // made.
        unsafe {
            libc::kill(-leader, libc::SIGKILL);
        }
        let _ = shell.wait();
        assert!(client.terminal && !client.fleeting, "{client:?}");
        assert!(
            client.id.starts_with(&format!("terminal:{leader}:"))
                && client.id.ends_with(">detached>/bin/sleep"),
            "{client:?}"
        );
    }

    #[test]
    fn a_program_of_no_app_above_it_is_answered_while_the_app_runs_alone() {
        // A program of a session of its own and no parent but launchd, as
        // a tmux server's: its path, its answer of no state.
        let mut perl = std::process::Command::new("/usr/bin/perl")
            .args([
                "-e",
                "use POSIX; my $pid = fork; if ($pid) { print \"$pid\\n\"; exit 0 } POSIX::setsid(); open STDOUT, '>', '/dev/null'; exec '/bin/sleep', '7';",
            ])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut told = String::new();
        std::io::Read::read_to_string(perl.stdout.as_mut().unwrap(), &mut told).unwrap();
        let _ = perl.wait();
        let pid: i32 = told.trim().parse().unwrap();
        for _ in 0..200 {
            if parent_of(pid) == Some(1) && program_path(pid).as_deref() == Some("/bin/sleep") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let client = of_pid(pid);
        // SAFETY: a signal to the process that the test started.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        assert_eq!(client.id, "program:/bin/sleep", "{client:?}");
        assert_eq!(client.name, "/bin/sleep");
        assert!(client.fleeting && !client.terminal, "{client:?}");
    }

    /// Every process's pid.
    fn processes() -> Vec<i32> {
        let mut pids = vec![0i32; 8192];
        let size = i32::try_from(pids.len() * std::mem::size_of::<i32>()).unwrap();
        // SAFETY: the buffer holds `size` bytes of pids, which the call
        // fills with the processes that it lists.
        let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        let count = usize::try_from(listed).unwrap_or(0).min(pids.len());
        pids[..count]
            .iter()
            .copied()
            .filter(|&pid| pid > 0)
            .collect()
    }

    #[test]
    fn a_program_run_again_from_its_pane_s_shell_is_the_same_client() {
        // The pane's shell, the app's child, which runs a program twice.
        let mut shell = std::process::Command::new("/bin/sh")
            .args(["-c", "/bin/sleep 5 & /bin/sleep 5 & wait"])
            .spawn()
            .unwrap();
        let shell_pid = i32::try_from(shell.id()).unwrap();
        let mut sleeps = Vec::new();
        for _ in 0..200 {
            sleeps = children_of(shell_pid);
            if sleeps.len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let clients: Vec<Client> = sleeps.iter().map(|&pid| of_pid(pid)).collect();
        let shell_client = of_pid(shell_pid);
        let _ = shell.kill();
        let _ = shell.wait();
        assert_eq!(clients.len(), 2, "{sleeps:?}");
        assert_eq!(clients[0].id, clients[1].id);
        assert!(
            clients[0].id.starts_with(&format!("terminal:{shell_pid}:")),
            "{clients:?}"
        );
        assert!(clients[0].id.ends_with(">/bin/sleep"), "{clients:?}");
        // The shell itself is another client of its pane.
        assert_ne!(shell_client.id, clients[0].id);
    }

    /// The processes whose parent is `parent`.
    fn children_of(parent: i32) -> Vec<i32> {
        let mut pids = vec![0i32; 4096];
        let size = i32::try_from(pids.len() * std::mem::size_of::<i32>()).unwrap();
        // SAFETY: the buffer holds `size` bytes of pids, which the call
        // fills with the processes that it lists.
        let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        let count = usize::try_from(listed).unwrap_or(0).min(pids.len());
        pids[..count]
            .iter()
            .copied()
            .filter(|&pid| pid > 0 && parent_of(pid) == Some(parent))
            .collect()
    }

    #[test]
    fn another_user_s_process_tells_its_parent_and_its_start() {
        // launchd, root's, as the `login` that a terminal's shell runs
        // under, whose whole BSD information the system refuses.
        assert_eq!(parent_of(1), Some(0));
        assert!(start_time(1) > 0);
        let own = i32::try_from(std::process::id()).unwrap();
        assert!(start_time(own) > start_time(1));
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

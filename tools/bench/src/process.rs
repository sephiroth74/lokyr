//! What the kernel tells about a process: its name, its memory and how often
//! it wakes up. A process of the same user needs no special privileges.

use std::io;
use std::mem::MaybeUninit;

use serde::Serialize;

/// Memory statistics of a process, in bytes.
#[derive(Clone, Copy, Debug)]
pub struct Memory {
    /// `phys_footprint`: the figure Activity Monitor shows as "Memory".
    pub footprint: u64,
    /// The highest footprint over the life of the process.
    pub lifetime_peak: u64,
}

/// How many times a process has woken up since it started.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Wakeups {
    /// Wakeups that brought a CPU package out of idle: what `top` shows as
    /// IDLEW.
    pub package_idle: u64,
    /// Wakeups caused by interrupts, such as timers.
    pub interrupt: u64,
}

/// Reads the memory statistics of `pid`.
pub fn memory(pid: libc::pid_t) -> io::Result<Memory> {
    let info = rusage(pid)?;
    Ok(Memory {
        footprint: info.ri_phys_footprint,
        lifetime_peak: info.ri_lifetime_max_phys_footprint,
    })
}

/// Reads the wakeup counters of `pid`.
pub fn wakeups(pid: libc::pid_t) -> io::Result<Wakeups> {
    let info = rusage(pid)?;
    Ok(Wakeups {
        package_idle: info.ri_pkg_idle_wkups,
        interrupt: info.ri_interrupt_wkups,
    })
}

/// The name of the process `pid`.
pub fn name(pid: libc::pid_t) -> io::Result<String> {
    let mut buffer = [0_u8; 256];
    let size = u32::try_from(buffer.len()).expect("the buffer is small");
    // SAFETY: `buffer` is writable for `size` bytes, and `proc_name` writes
    // at most that many.
    #[allow(unsafe_code)]
    let length = unsafe { libc::proc_name(pid, buffer.as_mut_ptr().cast(), size) };
    let length = usize::try_from(length).unwrap_or(0);
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(String::from_utf8_lossy(&buffer[..length]).into_owned())
}

/// The kernel's resource usage record for `pid`.
fn rusage(pid: libc::pid_t) -> io::Result<libc::rusage_info_v4> {
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    // SAFETY: `info` is writable and sized for `rusage_info_v4`, the struct
    // that `RUSAGE_INFO_V4` asks the kernel to fill in. `zeroed` already made
    // every field a valid integer, so the struct is initialized whether the
    // call succeeds or not.
    #[allow(unsafe_code)]
    let (status, info) = unsafe {
        let status = libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, info.as_mut_ptr().cast());
        (status, info.assume_init())
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn own_pid() -> libc::pid_t {
        libc::pid_t::try_from(std::process::id()).expect("PIDs fit in a pid_t")
    }

    #[test]
    fn reads_its_own_memory() {
        let memory = memory(own_pid()).expect("a process can read its own statistics");
        assert!(memory.footprint > 0);
        assert!(memory.lifetime_peak >= memory.footprint);
    }

    #[test]
    fn wakeup_counters_only_grow() {
        let before = wakeups(own_pid()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let after = wakeups(own_pid()).unwrap();
        assert!(after.package_idle >= before.package_idle);
        assert!(after.interrupt >= before.interrupt);
    }

    #[test]
    fn reads_its_own_name() {
        // Test binaries are named after the crate, with a hash appended.
        let name = name(own_pid()).unwrap();
        assert!(name.starts_with("saetta_bench"), "{name}");
    }

    #[test]
    fn reports_a_missing_process() {
        // PIDs never reach `pid_t::MAX`: the kernel caps them far lower.
        let error = memory(libc::pid_t::MAX).expect_err("no such process");
        assert_eq!(error.raw_os_error(), Some(libc::ESRCH));
        assert!(name(libc::pid_t::MAX).is_err());
    }
}

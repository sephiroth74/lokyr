//! Resource usage of a process, as the kernel accounts it.

use std::io;
use std::mem::MaybeUninit;

/// Memory statistics of a process, in bytes.
#[derive(Clone, Copy, Debug)]
pub struct Memory {
    /// `phys_footprint`: the figure Activity Monitor shows as "Memory".
    pub footprint: u64,
    /// The highest footprint over the life of the process.
    pub lifetime_peak: u64,
}

/// Reads the memory statistics of `pid`. A process of the same user needs no
/// special privileges.
pub fn memory(pid: libc::pid_t) -> io::Result<Memory> {
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
    Ok(Memory {
        footprint: info.ri_phys_footprint,
        lifetime_peak: info.ri_lifetime_max_phys_footprint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_its_own_process() {
        let pid = libc::pid_t::try_from(std::process::id()).expect("PIDs fit in a pid_t");
        let memory = memory(pid).expect("a process can read its own statistics");
        assert!(memory.footprint > 0);
        assert!(memory.lifetime_peak >= memory.footprint);
    }

    #[test]
    fn reports_a_missing_process() {
        // PIDs never reach `pid_t::MAX`: the kernel caps them far lower.
        let error = memory(libc::pid_t::MAX).expect_err("no such process");
        assert_eq!(error.raw_os_error(), Some(libc::ESRCH));
    }
}

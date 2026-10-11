//! The memory footprint of this process, the figure Activity Monitor shows.

use std::io;
use std::mem::MaybeUninit;

/// The current `phys_footprint` of this process, in bytes.
pub fn current() -> io::Result<u64> {
    let mut info = MaybeUninit::<libc::rusage_info_v4>::zeroed();
    let pid = i32::try_from(std::process::id()).expect("PIDs fit in a pid_t");
    // SAFETY: `info` points to writable memory sized for `rusage_info_v4`,
    // which is the struct `RUSAGE_INFO_V4` asks the kernel to fill in.
    let status =
        unsafe { libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, info.as_mut_ptr().cast()) };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call succeeded, so the kernel initialized the struct (and
    // `zeroed` already made every field a valid integer).
    Ok(unsafe { info.assume_init() }.ri_phys_footprint)
}

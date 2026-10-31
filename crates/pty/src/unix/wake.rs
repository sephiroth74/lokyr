//! What one thread gives another that waits in `poll`, to end its wait: a
//! descriptor that becomes ready once given, and stays so until taken. No
//! child inherits it.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

/// A wake, which [`Wake::give`] makes ready and [`Wake::take`] makes not
/// ready again.
#[derive(Debug)]
pub(super) struct Wake {
    /// On macOS, a kqueue with a user event that its retrieval clears, as
    /// for the PTY's drop: a kqueue is never inherited.
    #[cfg(target_os = "macos")]
    queue: OwnedFd,
    /// Elsewhere, a pipe that does not block, opened closed on exec.
    #[cfg(not(target_os = "macos"))]
    read: OwnedFd,
    #[cfg(not(target_os = "macos"))]
    write: OwnedFd,
}

#[cfg(target_os = "macos")]
impl Wake {
    pub(super) fn new() -> io::Result<Self> {
        // SAFETY: `kqueue` takes no arguments.
        let queue = unsafe { libc::kqueue() };
        if queue == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `kqueue` opened it, and nothing else owns it.
        let queue = unsafe { OwnedFd::from_raw_fd(queue) };
        super::closing::user_event(&queue, libc::EV_ADD | libc::EV_CLEAR, 0)?;
        Ok(Self { queue })
    }

    /// The descriptor to wait on, for reading.
    pub(super) fn fd(&self) -> &OwnedFd {
        &self.queue
    }

    /// Makes the descriptor ready. A kqueue that has its event does not
    /// fail to trigger it.
    pub(super) fn give(&self) {
        let _ = super::closing::user_event(&self.queue, 0, libc::NOTE_TRIGGER);
    }

    /// Makes the descriptor not ready, as its event, retrieved, clears.
    pub(super) fn take(&self) {
        let mut event = libc::kevent {
            ident: 0,
            filter: 0,
            flags: 0,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let now = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `kevent` reads no change, writes one event at most into
        // `event`, and does not wait.
        unsafe {
            libc::kevent(
                self.queue.as_raw_fd(),
                std::ptr::null(),
                0,
                &raw mut event,
                1,
                &raw const now,
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl Wake {
    pub(super) fn new() -> io::Result<Self> {
        let mut ends = [-1; 2];
        // SAFETY: `pipe2` writes the two descriptors into `ends`.
        if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `pipe2` opened both, and nothing else owns them.
        let (read, write) =
            unsafe { (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1])) };
        Ok(Self { read, write })
    }

    /// The descriptor to wait on, for reading.
    pub(super) fn fd(&self) -> &OwnedFd {
        &self.read
    }

    /// Makes the descriptor ready: a pipe that is full is ready already.
    pub(super) fn give(&self) {
        // SAFETY: `write` reads the one byte.
        unsafe { libc::write(self.write.as_raw_fd(), [1u8].as_ptr().cast(), 1) };
    }

    /// Makes the descriptor not ready, as the pipe empties.
    pub(super) fn take(&self) {
        let mut bytes = [0u8; 64];
        // SAFETY: `read` writes at most `bytes.len()` bytes into `bytes`.
        while unsafe {
            libc::read(
                self.read.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        } > 0
        {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `fd` has something to read, without waiting.
    fn ready(fd: &OwnedFd) -> bool {
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `poll` reads and writes the one entry.
        let ready = unsafe { libc::poll(&raw mut poll, 1, 0) };
        assert_ne!(ready, -1, "{}", io::Error::last_os_error());
        poll.revents != 0
    }

    #[test]
    fn a_wake_stays_ready_from_its_giving_to_its_taking() {
        let wake = Wake::new().unwrap();
        assert!(!ready(wake.fd()));
        wake.give();
        assert!(ready(wake.fd()));
        assert!(ready(wake.fd()), "and stays ready");
        wake.give();
        wake.take();
        assert!(!ready(wake.fd()), "taken once, for both");
        wake.give();
        assert!(ready(wake.fd()), "and ready again");
    }
}

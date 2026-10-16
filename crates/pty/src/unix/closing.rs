//! What tells a PTY's readers and writers that it has dropped: a
//! descriptor that they wait on with the master, which becomes ready, for
//! good, once the PTY drops the other side. No child inherits it, so that
//! no other process can keep the waits going.

use std::io;
#[cfg(target_os = "macos")]
use std::os::fd::AsRawFd;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;

/// The side that the PTY holds: dropping it makes the other ready.
#[derive(Debug)]
pub(super) struct Closing {
    #[cfg(target_os = "macos")]
    queue: Arc<OwnedFd>,
    #[cfg(not(target_os = "macos"))]
    _write: OwnedFd,
}

/// The descriptor that readers and writers wait on, and its other side.
///
/// On macOS, a kqueue with a user event, which the drop triggers: a kqueue
/// is never inherited, by a fork or by a spawn, where a pipe, which macOS
/// cannot open closed on exec, could be inherited by a fork elsewhere in
/// the app before it is marked.
#[cfg(target_os = "macos")]
pub(super) fn closing() -> io::Result<(Arc<OwnedFd>, Closing)> {
    // SAFETY: `kqueue` takes no arguments.
    let queue = unsafe { libc::kqueue() };
    if queue == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `kqueue` opened it, and nothing else owns it.
    let queue = Arc::new(unsafe { OwnedFd::from_raw_fd(queue) });
    user_event(&queue, libc::EV_ADD, 0)?;
    Ok((Arc::clone(&queue), Closing { queue }))
}

/// Adds the kqueue's user event, or changes it, with `flags` and `fflags`.
#[cfg(target_os = "macos")]
fn user_event(queue: &OwnedFd, flags: u16, fflags: u32) -> io::Result<()> {
    let event = libc::kevent {
        ident: 0,
        filter: libc::EVFILT_USER,
        flags,
        fflags,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    // SAFETY: `kevent` reads the one change in `event`, and writes no event
    // back.
    let done = unsafe {
        libc::kevent(
            queue.as_raw_fd(),
            &raw const event,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };
    if done == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
impl Drop for Closing {
    fn drop(&mut self) {
        // The event stays triggered, so that every wait, now and later,
        // finds the kqueue ready. A kqueue that has its event does not fail
        // to trigger it.
        let _ = user_event(&self.queue, 0, libc::NOTE_TRIGGER);
    }
}

/// The descriptor that readers and writers wait on, and its other side: a
/// pipe, opened closed on exec, whose end to read reads the end once the
/// end to write drops.
#[cfg(not(target_os = "macos"))]
pub(super) fn closing() -> io::Result<(Arc<OwnedFd>, Closing)> {
    let mut ends = [-1; 2];
    // SAFETY: `pipe2` writes the two descriptors into `ends`.
    if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `pipe2` opened both, and nothing else owns them.
    let (read, write) = unsafe { (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1])) };
    Ok((Arc::new(read), Closing { _write: write }))
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsRawFd;

    use super::*;

    /// Whether `fd` has something to read, or the end, without waiting.
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
    fn the_drop_makes_the_waits_ready_for_good() {
        let (closed, closing) = closing().unwrap();
        assert!(!ready(&closed));
        drop(closing);
        assert!(ready(&closed));
        assert!(ready(&closed), "and stays ready");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_fork_does_not_inherit_it() {
        let (closed, _closing) = closing().unwrap();
        let fd = closed.as_raw_fd();
        // SAFETY: the child calls only `fcntl` and `_exit`, which are
        // async-signal-safe.
        let pid = unsafe { libc::fork() };
        assert_ne!(pid, -1, "{}", io::Error::last_os_error());
        if pid == 0 {
            // SAFETY: as above.
            unsafe {
                let open = libc::fcntl(fd, libc::F_GETFD) != -1;
                libc::_exit(i32::from(open));
            }
        }
        let mut status = 0;
        // SAFETY: `waitpid` writes the status of the child just forked.
        assert_eq!(unsafe { libc::waitpid(pid, &raw mut status, 0) }, pid);
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 0, "the child had the descriptor");
    }
}

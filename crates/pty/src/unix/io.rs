//! Reading and writing a PTY's master, which never blocks: each read or
//! write waits, with `poll`, for the master and for the end of a pipe that
//! the PTY closes when it drops, so that its readers and writers stop
//! then, wherever they are.

use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Instant;

use crate::ReadOutcome;

/// What the program writes to the terminal.
#[derive(Debug)]
pub struct PtyReader {
    master: OwnedFd,
    closed: Arc<OwnedFd>,
}

/// What is typed into the terminal, for the program.
#[derive(Debug)]
pub struct PtyWriter {
    master: OwnedFd,
    closed: Arc<OwnedFd>,
}

impl PtyReader {
    /// A reader of `master`, a copy of the master, which stops once
    /// `closed`, the end of the PTY's pipe, reads the end.
    pub(super) fn new(master: OwnedFd, closed: Arc<OwnedFd>) -> Self {
        Self { master, closed }
    }

    /// Reads what the program wrote into `buf`, waiting for it until
    /// `deadline` at most, or for as long as it takes without one. The end
    /// comes once the program and every process that it left have closed
    /// the terminal, or once the PTY has dropped.
    pub fn read_until(
        &mut self,
        buf: &mut [u8],
        deadline: Option<Instant>,
    ) -> io::Result<ReadOutcome> {
        if buf.is_empty() {
            return Ok(ReadOutcome::Read(0));
        }
        loop {
            match wait(&self.master, &self.closed, libc::POLLIN, deadline)? {
                Ready::Closed => return Ok(ReadOutcome::End),
                Ready::TimedOut => return Ok(ReadOutcome::TimedOut),
                Ready::Master => {}
            }
            // SAFETY: `read` writes at most `buf.len()` bytes into `buf`.
            let n =
                unsafe { libc::read(self.master.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            match n {
                0 => return Ok(ReadOutcome::End),
                // Positive, and at most `buf.len()`.
                1.. => return Ok(ReadOutcome::Read(n as usize)),
                _ => {
                    let error = io::Error::last_os_error();
                    match error.raw_os_error() {
                        // Once no process has the terminal open, Linux's
                        // master fails with EIO rather than reading the end.
                        Some(libc::EIO) => return Ok(ReadOutcome::End),
                        Some(libc::EAGAIN | libc::EINTR) => {}
                        _ => return Err(error),
                    }
                }
            }
        }
    }
}

impl Read for PtyReader {
    /// Reads as [`PtyReader::read_until`] does without a deadline; the end
    /// reads as 0.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.read_until(buf, None)? {
            ReadOutcome::Read(n) => Ok(n),
            ReadOutcome::End | ReadOutcome::TimedOut => Ok(0),
        }
    }
}

impl PtyWriter {
    /// A writer to `master`, as [`PtyReader::new`] has it.
    pub(super) fn new(master: OwnedFd, closed: Arc<OwnedFd>) -> Self {
        Self { master, closed }
    }
}

impl Write for PtyWriter {
    /// Writes some of `buf`, once the program has room for it: a program
    /// that does not read its input can keep this waiting, until the PTY
    /// drops, when it fails with `BrokenPipe`.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            match wait(&self.master, &self.closed, libc::POLLOUT, None)? {
                Ready::Closed => return Err(ErrorKind::BrokenPipe.into()),
                Ready::TimedOut | Ready::Master => {}
            }
            // SAFETY: `write` reads at most `buf.len()` bytes from `buf`.
            let n = unsafe { libc::write(self.master.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
            if n >= 0 {
                // Not negative, and at most `buf.len()`.
                return Ok(n as usize);
            }
            let error = io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(libc::EAGAIN | libc::EINTR)) {
                return Err(error);
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// What a wait found ready.
enum Ready {
    /// The master, for the events asked for, or with an error or a hang-up
    /// that the read or the write will tell.
    Master,
    /// The PTY's pipe: the PTY has dropped.
    Closed,
    /// Neither, by the deadline.
    TimedOut,
}

/// Waits for `master` to have `events`, for `closed` to read the end, or
/// for `deadline`.
fn wait(
    master: &OwnedFd,
    closed: &OwnedFd,
    events: libc::c_short,
    deadline: Option<Instant>,
) -> io::Result<Ready> {
    loop {
        let timeout = match deadline {
            None => -1,
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Ok(Ready::TimedOut);
                }
                // Rounded up, so that the wait does not end before the
                // deadline and spin.
                let millis = left.as_nanos().div_ceil(1_000_000);
                libc::c_int::try_from(millis).unwrap_or(libc::c_int::MAX)
            }
        };
        let mut fds = [
            libc::pollfd {
                fd: master.as_raw_fd(),
                events,
                revents: 0,
            },
            libc::pollfd {
                fd: closed.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: `poll` reads and writes the two entries of `fds`.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        if ready == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if fds[1].revents != 0 {
            return Ok(Ready::Closed);
        }
        if fds[0].revents != 0 {
            return Ok(Ready::Master);
        }
        // Timed out: the deadline, checked again, ends the wait.
    }
}

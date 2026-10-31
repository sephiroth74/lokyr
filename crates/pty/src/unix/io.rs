//! Reading and writing a PTY's master, which never blocks: each read or
//! write waits, with `poll`, for the master and for a descriptor that
//! becomes ready when the PTY drops, so that its readers and writers stop
//! then, wherever they are.

use std::io::{self, ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::Arc;
use std::time::Instant;

use crate::{ReadOutcome, Readiness};

/// What the program writes to the terminal.
#[derive(Debug)]
pub struct PtyReader {
    pub(super) master: OwnedFd,
    pub(super) closed: Arc<OwnedFd>,
}

/// What is typed into the terminal, for the program.
#[derive(Debug)]
pub struct PtyWriter {
    master: OwnedFd,
    closed: Arc<OwnedFd>,
}

impl PtyReader {
    /// A reader of `master`, a copy of the master, which stops once
    /// `closed` becomes ready, as the PTY drops.
    pub(super) fn new(master: OwnedFd, closed: Arc<OwnedFd>) -> Self {
        Self { master, closed }
    }

    /// Reads what the program wrote into `buf`, waiting for it until
    /// `deadline` at most, or for as long as it takes without one; a
    /// deadline that has passed reads what is there, without waiting, so
    /// that a program that writes all the time never makes a read time out:
    /// a caller that reads until a deadline looks at the time after each
    /// read. The end comes once the program and every process that it left
    /// have closed the terminal, or once the PTY has dropped.
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
                // A program that has ended may have left output to read.
                Ready::Master | Ready::HungUp => {}
            }
            match self.read_now(buf)? {
                ReadOutcome::TimedOut => {}
                outcome => return Ok(outcome),
            }
        }
    }

    /// Reads what the program wrote into `buf`, without waiting for it:
    /// `TimedOut` while there is nothing to read.
    pub(super) fn read_now(&mut self, buf: &mut [u8]) -> io::Result<ReadOutcome> {
        if buf.is_empty() {
            return Ok(ReadOutcome::Read(0));
        }
        loop {
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
                        Some(libc::EAGAIN) => return Ok(ReadOutcome::TimedOut),
                        Some(libc::EINTR) => {}
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

impl PtyReader {
    /// Waits until the program has written something, or has room for what
    /// `writer`, of the same PTY, would type, until `deadline` at most, or
    /// for as long as it takes without one: for a caller that reads and
    /// writes on one thread, and so waits for both at once. A deadline that
    /// has passed looks once, without waiting.
    pub fn wait_either(
        &self,
        writer: &PtyWriter,
        deadline: Option<Instant>,
    ) -> io::Result<Readiness> {
        let mut fds = [
            pollfd(&self.master, libc::POLLIN),
            pollfd(&writer.master, libc::POLLOUT),
            pollfd(&self.closed, libc::POLLIN),
        ];
        poll(&mut fds, deadline)?;
        let closed = fds[2].revents != 0;
        Ok(Readiness {
            output: closed || fds[0].revents != 0,
            room: closed || fds[1].revents != 0,
        })
    }
}

impl PtyWriter {
    /// Writes some of `buf`, once the program has room for it, waiting for
    /// that until `deadline` at most, or for as long as it takes without
    /// one: how many bytes, or `None` if the deadline came first. A deadline
    /// that has passed writes what fits, without waiting, as a read takes
    /// what is there. A program
    /// that does not read its input keeps the room from coming; once the PTY
    /// drops, this fails with `BrokenPipe`.
    pub fn write_until(
        &mut self,
        buf: &[u8],
        deadline: Option<Instant>,
    ) -> io::Result<Option<usize>> {
        if buf.is_empty() {
            return Ok(Some(0));
        }
        loop {
            match wait(&self.master, &self.closed, libc::POLLOUT, deadline)? {
                Ready::Closed => return Err(ErrorKind::BrokenPipe.into()),
                Ready::TimedOut => return Ok(None),
                // No process has the terminal open, so no one would read
                // what is written. Linux's master would take it into its
                // buffer all the same; macOS's fails with EIO, as this does.
                Ready::HungUp => return Err(io::Error::from_raw_os_error(libc::EIO)),
                Ready::Master => {}
            }
            // SAFETY: `write` reads at most `buf.len()` bytes from `buf`.
            let n = unsafe { libc::write(self.master.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
            if n >= 0 {
                // Not negative, and at most `buf.len()`.
                return Ok(Some(n as usize));
            }
            let error = io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(libc::EAGAIN | libc::EINTR)) {
                return Err(error);
            }
        }
    }
}

impl Write for PtyWriter {
    /// Writes as [`PtyWriter::write_until`] does without a deadline.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(self.write_until(buf, None)?.unwrap_or(0))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// What a wait found ready.
pub(super) enum Ready {
    /// The master, for the events asked for, or with an error that the
    /// read or the write will tell.
    Master,
    /// The master, hung up: no process has the terminal open any more.
    HungUp,
    /// What the PTY's drop makes ready: the PTY has dropped.
    Closed,
    /// Neither, by the deadline.
    TimedOut,
}

/// Waits for `master` to have `events`, for `closed` to be ready, or for
/// `deadline`.
pub(super) fn wait(
    master: &OwnedFd,
    closed: &OwnedFd,
    events: libc::c_short,
    deadline: Option<Instant>,
) -> io::Result<Ready> {
    let mut fds = [pollfd(master, events), pollfd(closed, libc::POLLIN)];
    poll(&mut fds, deadline)?;
    Ok(if fds[1].revents != 0 {
        Ready::Closed
    } else if fds[0].revents & libc::POLLHUP != 0 {
        Ready::HungUp
    } else if fds[0].revents != 0 {
        Ready::Master
    } else {
        Ready::TimedOut
    })
}

/// An entry for `poll`, of `fd` for `events`.
pub(super) fn pollfd(fd: &OwnedFd, events: libc::c_short) -> libc::pollfd {
    libc::pollfd {
        fd: fd.as_raw_fd(),
        events,
        revents: 0,
    }
}

/// Waits for one of `fds` to be ready, or for `deadline`, after which none
/// is: a deadline that has passed still looks once, without waiting.
pub(super) fn poll(fds: &mut [libc::pollfd], deadline: Option<Instant>) -> io::Result<()> {
    loop {
        let (timeout, last) = match deadline {
            None => (-1, false),
            Some(deadline) => {
                let left = deadline.saturating_duration_since(Instant::now());
                // Rounded up, so that the wait does not end before the
                // deadline and spin.
                let millis = left.as_nanos().div_ceil(1_000_000);
                let timeout = libc::c_int::try_from(millis).unwrap_or(libc::c_int::MAX);
                (timeout, left.is_zero())
            }
        };
        for fd in fds.iter_mut() {
            fd.revents = 0;
        }
        // A few entries.
        let count = fds.len() as libc::nfds_t;
        // SAFETY: `poll` reads and writes the `count` entries of `fds`.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), count, timeout) };
        if ready == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if ready > 0 || last {
            return Ok(());
        }
        // Timed out: the deadline, checked again, ends the wait.
    }
}

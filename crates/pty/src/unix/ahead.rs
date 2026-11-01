//! Reading ahead of the parse: a thread that only reads the program's
//! output, into a few buffers, which the parser takes in turn, so that the
//! program does not wait for the parse.
//!
//! A terminal on macOS queues about a KiB of output: a program that writes
//! more waits, each KiB, for a read to take it. A thread that read and
//! parsed in turn would keep the program waiting through each parse, and
//! wait itself through each refill of the queue, a KiB for a wakeup of
//! each. This thread reads as long as output comes, and bridges the
//! refills of a stream that fills the queue, which take the program a few
//! microseconds, with reads that do not wait, then with short waits while
//! the parser has something to parse, so that a batch holds many refills;
//! output that comes a little at a time, as a shell's echo, goes to the
//! parser at once.

use std::collections::VecDeque;
use std::io;
use std::ops::Deref;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::io::{PtyReader, Ready, poll, pollfd, wait};
use super::wake::Wake;
use crate::ReadOutcome;

/// How many buffers the reader and the parser pass between them: the
/// reader reads that many batches ahead at most, past which the program
/// waits, as the terminal's queue fills.
const BUFFERS: usize = 4;

/// What a read takes at most on macOS, whose terminals queue about a KiB:
/// a batch that holds as much comes of a program that fills the queue, and
/// is worth a short wait for more; less is output that comes a little at a
/// time, which goes at once.
const FULL: usize = 1024;

/// How many reads that find the queue empty a batch of a full stream tries
/// again at once, before it waits: the program fills the queue again
/// within a few microseconds, where a wait and its wakeup take several
/// more.
const SPINS: usize = 16;

/// How long a batch waits for the program's next refill, once those reads
/// found nothing: output that stops this long has ended its burst.
const REFILL: Duration = Duration::from_millis(1);

/// How long a batch waits for refills, in all, before it goes to the
/// parser regardless, give or take a wait for a refill: well under a
/// frame.
const BUDGET: Duration = Duration::from_millis(3);

/// The program's output, read ahead of its parse on a thread of its own,
/// as [`PtyReader::read_ahead`] starts it. Dropping it stops the reads at
/// the next output, or as the PTY drops.
#[derive(Debug)]
pub struct ReadAhead {
    shared: Arc<Shared>,
}

/// What [`ReadAhead::next`] gave.
#[derive(Debug)]
pub enum Next<'a> {
    /// Bytes of output.
    Output(Batch<'a>),
    /// Nothing, by the deadline.
    TimedOut,
    /// The end, as [`ReadOutcome::End`] has it, or a read that failed: the
    /// batches before it have all been taken.
    End,
}

/// A batch of output, whose buffer goes back to the reader as it drops.
pub struct Batch<'a> {
    shared: &'a Shared,
    buffer: Vec<u8>,
    len: usize,
}

/// What the reader and the parser share.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// Signalled when a batch comes, and when the output ends.
    ready: Condvar,
    /// Signalled when a buffer comes back, and when the parser goes.
    room: Condvar,
    /// Given when the parser has done with its last batch while the reader
    /// waits for a refill, so that the reader hands over what it has at
    /// once.
    hurry: Wake,
}

#[derive(Debug, Default)]
struct State {
    /// The batches read, in order, each with its length.
    full: VecDeque<(Vec<u8>, usize)>,
    /// The buffers that the reader can read into.
    empty: Vec<Vec<u8>>,
    /// Whether the parser has a batch.
    parsing: bool,
    /// Whether the reader waits for a refill, which `hurry` cuts short.
    bridging: bool,
    /// Whether the reads have ended.
    ended: bool,
    /// Whether the parser has gone.
    gone: bool,
}

impl PtyReader {
    /// Reads the program's output on a thread of its own, into a few
    /// buffers of `size` bytes, ahead of the parse, which takes each in
    /// turn through [`ReadAhead::next`]: the reads stop once the output
    /// ends or the PTY drops, or, after the [`ReadAhead`] drops, at the
    /// next output.
    pub fn read_ahead(self, size: usize) -> io::Result<ReadAhead> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                empty: (0..BUFFERS).map(|_| vec![0; size.max(1)]).collect(),
                ..State::default()
            }),
            ready: Condvar::new(),
            room: Condvar::new(),
            hurry: Wake::new()?,
        });
        {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("pty-read".into())
                .spawn(move || read(self, &shared))?;
        }
        Ok(ReadAhead { shared })
    }
}

impl ReadAhead {
    /// The next batch of output, waiting for it until `deadline` at most,
    /// or for as long as it takes without one: a deadline that has passed
    /// takes a batch that is there, without waiting, so that output that
    /// never stops never times this out.
    pub fn next(&mut self, deadline: Option<Instant>) -> Next<'_> {
        let shared = &*self.shared;
        let mut state = lock(&shared.state);
        loop {
            if let Some((buffer, len)) = state.full.pop_front() {
                state.parsing = true;
                return Next::Output(Batch {
                    shared,
                    buffer,
                    len,
                });
            }
            if state.ended {
                return Next::End;
            }
            state = match deadline {
                None => recover(shared.ready.wait(state)),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Next::TimedOut;
                    }
                    recover(shared.ready.wait_timeout(state, left)).0
                }
            };
        }
    }
}

impl Drop for ReadAhead {
    fn drop(&mut self) {
        lock(&self.shared.state).gone = true;
        self.shared.room.notify_all();
        self.shared.hurry.give();
    }
}

impl std::fmt::Debug for Batch<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Batch").field(&&**self).finish()
    }
}

impl Deref for Batch<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
}

impl Drop for Batch<'_> {
    fn drop(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        let hurry = {
            let mut state = lock(&self.shared.state);
            state.empty.push(buffer);
            state.parsing = false;
            state.full.is_empty() && state.bridging
        };
        self.shared.room.notify_one();
        if hurry {
            self.shared.hurry.give();
        }
    }
}

/// The reader's thread: reads batch after batch, until the output ends,
/// the PTY drops, or the parser goes.
fn read(mut reader: PtyReader, shared: &Shared) {
    let _ended = Ended(shared);
    loop {
        let Some(mut buffer) = claim(shared) else {
            return;
        };
        let (len, end) = fill(&mut reader, &mut buffer, shared);
        let full = len == buffer.len();
        {
            let mut state = lock(&shared.state);
            if len > 0 {
                state.full.push_back((buffer, len));
            } else {
                state.empty.push(buffer);
            }
        }
        if len > 0 {
            shared.ready.notify_one();
        }
        if end {
            return;
        }
        // The output goes on, with no wait for it.
        if full {
            continue;
        }
        match wait(&reader.master, &reader.closed, libc::POLLIN, None) {
            Ok(Ready::Closed) | Err(_) => return,
            // Output, or the end, which the next read tells.
            Ok(_) => {}
        }
    }
}

/// An empty buffer, once one has come back; none once the parser has gone.
fn claim(shared: &Shared) -> Option<Vec<u8>> {
    let mut state = lock(&shared.state);
    loop {
        if state.gone {
            return None;
        }
        if let Some(buffer) = state.empty.pop() {
            return Some(buffer);
        }
        state = recover(shared.room.wait(state));
    }
}

/// Reads into `buffer` as long as output comes, bridging the refills of a
/// stream that fills the terminal's queue: how many bytes it read, and
/// whether the output has ended or the PTY has dropped.
fn fill(reader: &mut PtyReader, buffer: &mut [u8], shared: &Shared) -> (usize, bool) {
    let mut len = 0;
    let mut spins = 0;
    let mut bridged: Option<Instant> = None;
    while len < buffer.len() {
        match reader.read_now(&mut buffer[len..]) {
            Ok(ReadOutcome::Read(n)) => {
                len += n;
                spins = 0;
            }
            Ok(ReadOutcome::End) | Err(_) => return (len, true),
            Ok(ReadOutcome::TimedOut) => {
                if len < FULL {
                    break;
                }
                if spins < SPINS {
                    spins += 1;
                    continue;
                }
                let now = Instant::now();
                if now.duration_since(*bridged.get_or_insert(now)) >= BUDGET {
                    break;
                }
                // A wait costs nothing while the parser has something to
                // parse; once it has nothing, the wait would hold the batch
                // from it, and a program that waits for the answer to a
                // query in the batch would wait in vain.
                {
                    let mut state = lock(&shared.state);
                    if state.full.is_empty() && !state.parsing {
                        break;
                    }
                    state.bridging = true;
                }
                let refill = refill(reader, shared, now + REFILL);
                lock(&shared.state).bridging = false;
                match refill {
                    Refill::Output => {}
                    Refill::Closed => return (len, true),
                    Refill::Quiet => break,
                }
            }
        }
    }
    (len, false)
}

/// What a wait for a refill found.
enum Refill {
    /// More output.
    Output,
    /// Nothing by the deadline, the end, or a parser that has nothing left.
    Quiet,
    /// The PTY has dropped.
    Closed,
}

/// Waits for more output until `deadline`, or for the parser to hurry the
/// reader, or for the PTY's drop.
fn refill(reader: &PtyReader, shared: &Shared, deadline: Instant) -> Refill {
    let mut fds = [
        pollfd(&reader.master, libc::POLLIN),
        pollfd(&reader.closed, libc::POLLIN),
        pollfd(shared.hurry.fd(), libc::POLLIN),
    ];
    if poll(&mut fds, Some(deadline)).is_err() {
        return Refill::Quiet;
    }
    if fds[1].revents != 0 {
        Refill::Closed
    } else if fds[2].revents != 0 {
        shared.hurry.take();
        Refill::Quiet
    } else if fds[0].revents & libc::POLLIN != 0 {
        Refill::Output
    } else {
        // Timed out, or hung up with nothing left to read.
        Refill::Quiet
    }
}

/// Tells the parser that the reads have ended, as the reader's thread
/// ends, by a panic too.
struct Ended<'a>(&'a Shared);

impl Drop for Ended<'_> {
    fn drop(&mut self) {
        lock(&self.0.state).ended = true;
        self.0.ready.notify_all();
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    recover(state.lock())
}

/// A lock that a thread which panicked left: the state stays as it was.
fn recover<G>(result: Result<G, PoisonError<G>>) -> G {
    result.unwrap_or_else(PoisonError::into_inner)
}

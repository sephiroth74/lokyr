//! The bytes on their way to the program, and the deadline of the
//! synchronized update under way, which the writer thread keeps.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

/// What the writer thread does next.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Work {
    /// Writes these bytes to the program.
    Write(Vec<u8>),
    /// Ends the synchronized update with this number, if it is still under
    /// way: its deadline has passed.
    Deadline(u64),
    /// Stops: the session has ended.
    Stop,
}

#[derive(Debug, Default)]
struct State {
    chunks: VecDeque<Vec<u8>>,
    deadline: Option<(u64, Instant)>,
    closed: bool,
}

/// A queue of the writes to the program, in order, with the deadline of a
/// synchronized update.
#[derive(Debug, Default)]
pub(crate) struct Queue {
    state: Mutex<State>,
    ready: Condvar,
}

impl Queue {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues `bytes`, unless they are empty or the queue is closed.
    pub(crate) fn push(&self, bytes: Vec<u8>) {
        let mut state = self.state();
        if bytes.is_empty() || state.closed {
            return;
        }
        state.chunks.push_back(bytes);
        self.ready.notify_one();
    }

    /// Gives the synchronized update with number `update` the deadline
    /// `at`, in place of any other.
    pub(crate) fn set_deadline(&self, update: u64, at: Instant) {
        self.state().deadline = Some((update, at));
        self.ready.notify_one();
    }

    /// Drops the writes still queued, takes no more, and stops the writer.
    pub(crate) fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        state.chunks.clear();
        self.ready.notify_one();
    }

    /// Waits for the next work: a write, which comes first, a deadline
    /// that has passed, or the end.
    pub(crate) fn next(&self) -> Work {
        let mut state = self.state();
        loop {
            if state.closed {
                return Work::Stop;
            }
            if let Some(chunk) = state.chunks.pop_front() {
                return Work::Write(chunk);
            }
            state = match state.deadline {
                Some((update, at)) => {
                    let now = Instant::now();
                    if now >= at {
                        state.deadline = None;
                        return Work::Deadline(update);
                    }
                    let (state, _) = self
                        .ready
                        .wait_timeout(state, at - now)
                        .unwrap_or_else(PoisonError::into_inner);
                    state
                }
                None => self
                    .ready
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn writes_come_in_order_and_before_a_deadline() {
        let queue = Queue::default();
        queue.push(b"a".to_vec());
        queue.push(Vec::new());
        queue.set_deadline(1, Instant::now());
        queue.push(b"b".to_vec());
        assert_eq!(queue.next(), Work::Write(b"a".to_vec()));
        assert_eq!(queue.next(), Work::Write(b"b".to_vec()));
        assert_eq!(queue.next(), Work::Deadline(1));
        queue.close();
        queue.push(b"c".to_vec());
        assert_eq!(queue.next(), Work::Stop);
    }

    #[test]
    fn a_deadline_waits_for_its_time_and_a_later_one_replaces_it() {
        let queue = Queue::default();
        let start = Instant::now();
        queue.set_deadline(1, start + Duration::from_secs(60));
        queue.set_deadline(2, start + Duration::from_millis(50));
        assert_eq!(queue.next(), Work::Deadline(2));
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn closing_drops_what_is_queued() {
        let queue = Queue::default();
        queue.push(b"a".to_vec());
        queue.close();
        assert_eq!(queue.next(), Work::Stop);
    }
}

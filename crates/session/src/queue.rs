//! The bytes on their way to the program, which the writer thread writes.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// The bytes of the terminal's replies that can wait to be written: past
/// them, the next replies are dropped, so that a program that sends
/// queries and never reads its input cannot grow the queue without end.
pub(crate) const REPLY_BYTES: usize = 1 << 20;

#[derive(Debug, Default)]
struct State {
    /// The writes, each with whether it is a reply.
    entries: VecDeque<(Vec<u8>, bool)>,
    /// The bytes of the replies among them, and of the reply that the
    /// writer writes, until it asks for the next entry.
    replies: usize,
    /// The bytes of the reply that the writer writes.
    in_flight: usize,
    closed: bool,
}

/// A queue of the writes to the program, in order.
#[derive(Debug, Default)]
pub(crate) struct Queue {
    state: Mutex<State>,
    ready: Condvar,
}

impl Queue {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues `bytes`, keys or a paste, which go to the program after what
    /// is queued, and before what comes after, unless they are empty or the
    /// queue is closed.
    pub(crate) fn push(&self, bytes: Vec<u8>) {
        self.add(bytes, false);
    }

    /// Queues `bytes`, the terminal's replies, as [`Queue::push`] does,
    /// unless the replies that wait already take [`REPLY_BYTES`].
    pub(crate) fn push_replies(&self, bytes: Vec<u8>) {
        self.add(bytes, true);
    }

    fn add(&self, bytes: Vec<u8>, reply: bool) {
        let mut state = self.state();
        if bytes.is_empty() || state.closed || reply && state.replies >= REPLY_BYTES {
            return;
        }
        if reply {
            state.replies += bytes.len();
        }
        state.entries.push_back((bytes, reply));
        self.ready.notify_one();
    }

    /// The bytes of the replies that wait.
    #[cfg(test)]
    pub(crate) fn replies(&self) -> usize {
        self.state().replies
    }

    /// The bytes of every write that waits.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.state()
            .entries
            .iter()
            .map(|(bytes, _)| bytes.len())
            .sum()
    }

    /// Drops the writes still queued, takes no more, and stops the writer.
    pub(crate) fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        state.entries.clear();
        state.replies = 0;
        state.in_flight = 0;
        self.ready.notify_one();
    }

    /// Whether the queue is closed.
    pub(crate) fn is_closed(&self) -> bool {
        self.state().closed
    }

    /// Waits for the next entry, or for the queue to close; the entry
    /// before, which the writer has written, no longer counts.
    pub(crate) fn next(&self) -> Option<Vec<u8>> {
        let mut state = self.state();
        state.replies -= std::mem::take(&mut state.in_flight);
        loop {
            if state.closed {
                return None;
            }
            if let Some((bytes, reply)) = state.entries.pop_front() {
                if reply {
                    state.in_flight = bytes.len();
                }
                return Some(bytes);
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_come_in_order_until_the_queue_closes() {
        let queue = Queue::default();
        queue.push(b"a".to_vec());
        queue.push(Vec::new());
        queue.push(b"b".to_vec());
        assert_eq!(queue.next(), Some(b"a".to_vec()));
        assert_eq!(queue.next(), Some(b"b".to_vec()));
        assert!(!queue.is_closed());
        queue.push(b"c".to_vec());
        queue.close();
        assert!(queue.is_closed());
        queue.push(b"d".to_vec());
        assert_eq!(queue.next(), None, "closing drops what is queued");
    }

    #[test]
    fn replies_wait_up_to_a_limit_and_keys_past_it() {
        let queue = Queue::default();
        let reply = vec![b'r'; REPLY_BYTES / 2];
        queue.push_replies(reply.clone());
        queue.push_replies(reply.clone());
        assert_eq!(queue.replies(), REPLY_BYTES);
        // Past the limit, a reply is dropped, but not a key.
        queue.push_replies(b"dropped".to_vec());
        queue.push(b"key".to_vec());
        assert_eq!(queue.replies(), REPLY_BYTES);
        // A reply that the writer writes still counts, until it asks for
        // the next entry, having written it.
        assert_eq!(queue.next(), Some(reply.clone()));
        assert_eq!(queue.replies(), REPLY_BYTES);
        queue.push_replies(b"dropped".to_vec());
        assert_eq!(queue.next(), Some(reply));
        assert_eq!(queue.replies(), REPLY_BYTES / 2);
        queue.push_replies(b"kept".to_vec());
        assert_eq!(queue.next(), Some(b"key".to_vec()));
        assert_eq!(queue.next(), Some(b"kept".to_vec()));
        queue.push(b"last".to_vec());
        assert_eq!(queue.next(), Some(b"last".to_vec()));
        assert_eq!(queue.replies(), 0);
    }

    #[test]
    fn a_waiting_writer_wakes_for_an_entry() {
        let queue = std::sync::Arc::new(Queue::default());
        let writer = {
            let queue = std::sync::Arc::clone(&queue);
            std::thread::spawn(move || queue.next())
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        queue.push(b"a".to_vec());
        assert_eq!(writer.join().unwrap(), Some(b"a".to_vec()));
    }
}

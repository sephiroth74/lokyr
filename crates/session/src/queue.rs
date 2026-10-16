//! The bytes on their way to the program, which the writer thread writes.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
struct State {
    entries: VecDeque<Vec<u8>>,
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

    /// Queues `bytes`, which go to the program after what is queued, and
    /// before what comes after, unless they are empty or the queue is
    /// closed.
    pub(crate) fn push(&self, bytes: Vec<u8>) {
        let mut state = self.state();
        if bytes.is_empty() || state.closed {
            return;
        }
        state.entries.push_back(bytes);
        self.ready.notify_one();
    }

    /// Drops the writes still queued, takes no more, and stops the writer.
    pub(crate) fn close(&self) {
        let mut state = self.state();
        state.closed = true;
        state.entries.clear();
        self.ready.notify_one();
    }

    /// Whether the queue is closed.
    pub(crate) fn is_closed(&self) -> bool {
        self.state().closed
    }

    /// Waits for the next entry, or for the queue to close.
    pub(crate) fn next(&self) -> Option<Vec<u8>> {
        let mut state = self.state();
        loop {
            if state.closed {
                return None;
            }
            if let Some(entry) = state.entries.pop_front() {
                return Some(entry);
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

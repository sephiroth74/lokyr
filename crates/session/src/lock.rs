//! The grid lock, which the I/O thread hands over to whoever waits for it
//! between two slices of output.

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// A mutex that a thread which takes it again and again, as the I/O thread
/// does once for each slice of output, lets the other threads take in
/// between. `std`'s mutex makes no such promise: a thread that unlocks and
/// locks again at once can keep the others waiting for as long as it runs.
///
/// The turn-taker waits, before each turn, for the threads that came before
/// it asked, while those that come later wait for its turn to start. Each
/// side has the lock in turn, whichever locks more often.
#[derive(Debug, Default)]
pub(crate) struct FairMutex<T> {
    data: Mutex<T>,
    turns: Mutex<Turns>,
    /// Signalled when a thread has taken the lock in [`FairMutex::lock`],
    /// and when a turn has started.
    changed: Condvar,
}

#[derive(Debug, Default)]
struct Turns {
    /// How many times threads came to [`FairMutex::lock`].
    arrived: u64,
    /// How many of them have had the lock.
    served: u64,
    /// Whether the turn-taker waits for its turn, ahead of those who come
    /// now.
    turn: bool,
}

impl<T> FairMutex<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            data: Mutex::new(value),
            turns: Mutex::new(Turns::default()),
            changed: Condvar::new(),
        }
    }

    /// Locks, ahead of the next turn of [`FairMutex::lock_in_turn`]'s
    /// thread. A thread that panicked with the lock held leaves the value
    /// as it was.
    pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
        self.lock_after(|| ())
    }

    /// [`FairMutex::lock`], which calls `arrived` once this thread has its
    /// place: from then on, at most the turn under way goes before it.
    pub(crate) fn lock_after(&self, arrived: impl FnOnce()) -> MutexGuard<'_, T> {
        {
            let mut turns = recover(self.turns.lock());
            while turns.turn {
                turns = recover(self.changed.wait(turns));
            }
            turns.arrived += 1;
        }
        arrived();
        let guard = recover(self.data.lock());
        recover(self.turns.lock()).served += 1;
        self.changed.notify_all();
        guard
    }

    /// Locks for the thread that takes the lock again and again, once the
    /// threads that came to [`FairMutex::lock`] before it asked have had
    /// it.
    pub(crate) fn lock_in_turn(&self) -> MutexGuard<'_, T> {
        {
            let mut turns = recover(self.turns.lock());
            turns.turn = true;
            let before = turns.arrived;
            while turns.served < before {
                turns = recover(self.changed.wait(turns));
            }
        }
        let guard = recover(self.data.lock());
        recover(self.turns.lock()).turn = false;
        self.changed.notify_all();
        guard
    }
}

fn recover<G>(result: Result<G, PoisonError<G>>) -> G {
    result.unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::*;

    /// Waits until `done` holds of `lock`'s turns.
    fn until<T>(lock: &FairMutex<T>, done: impl Fn(&Turns) -> bool) {
        loop {
            let turns = recover(lock.turns.lock());
            if done(&turns) {
                return;
            }
            drop(turns);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_thread_that_waits_has_the_lock_before_the_next_turn() {
        let lock = Arc::new(FairMutex::new(()));
        let turns = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (lock, turns, stop) = (Arc::clone(&lock), Arc::clone(&turns), Arc::clone(&stop));
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let _guard = lock.lock_in_turn();
                    // A turn of some work, with the lock held.
                    let start = std::time::Instant::now();
                    while start.elapsed() < Duration::from_micros(200) {
                        std::hint::spin_loop();
                    }
                    turns.fetch_add(1, Ordering::Relaxed);
                }
            })
        };
        // The worker takes its turns from the start.
        while turns.load(Ordering::Relaxed) == 0 {
            thread::yield_now();
        }
        let mut most = 0;
        for _ in 0..500 {
            let mut before = 0;
            let guard = lock.lock_after(|| before = turns.load(Ordering::Relaxed));
            let after = turns.load(Ordering::Relaxed);
            drop(guard);
            most = most.max(after - before);
        }
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap();
        assert!(most <= 1, "waited {most} turns");
    }

    /// Who had the lock, in order.
    type Order = Arc<Mutex<Vec<&'static str>>>;

    fn take(
        lock: &Arc<FairMutex<()>>,
        order: &Order,
        name: &'static str,
        turn: bool,
    ) -> thread::JoinHandle<()> {
        let (lock, order) = (Arc::clone(lock), Arc::clone(order));
        thread::spawn(move || {
            let _guard = if turn {
                lock.lock_in_turn()
            } else {
                lock.lock()
            };
            order.lock().unwrap().push(name);
        })
    }

    #[test]
    fn a_turn_waits_for_those_who_came_before_it_asked() {
        let lock = Arc::new(FairMutex::new(()));
        let order = Order::default();
        let held = lock.lock();
        let early = take(&lock, &order, "early", false);
        until(&lock, |turns| turns.arrived == 2);
        let turn = take(&lock, &order, "turn", true);
        until(&lock, |turns| turns.turn);
        drop(held);
        early.join().unwrap();
        turn.join().unwrap();
        assert_eq!(*order.lock().unwrap(), ["early", "turn"]);
    }

    #[test]
    fn those_who_come_after_a_turn_asked_wait_for_it() {
        let lock = Arc::new(FairMutex::new(()));
        let order = Order::default();
        let held = lock.lock();
        let turn = take(&lock, &order, "turn", true);
        until(&lock, |turns| turns.turn);
        let late = take(&lock, &order, "late", false);
        // The late one waits at the gate, not for the value.
        thread::sleep(Duration::from_millis(20));
        assert_eq!(recover(lock.turns.lock()).arrived, 1);
        drop(held);
        turn.join().unwrap();
        late.join().unwrap();
        assert_eq!(*order.lock().unwrap(), ["turn", "late"]);
    }
}

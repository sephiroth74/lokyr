//! Timers fire once, in the order of their times, no earlier than theirs;
//! one cancelled never fires, and one whose time is past fires at once.

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
mod probe {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use lokyr_platform_core::{AppEvent, Handler, Platform, TimerId, WindowEvent, WindowId};

    use crate::common;

    /// How late a timer may fire, on a busy machine.
    const LATE: Duration = Duration::from_millis(250);

    #[derive(Debug, Default)]
    struct Seen {
        /// Each timer started, by its name, with its time.
        started: Vec<(TimerId, &'static str, Instant)>,
        /// Each timer fired, by its name, with how late it fired.
        fired: Vec<(&'static str, Duration)>,
        early: Vec<&'static str>,
    }

    struct Probe(Rc<RefCell<Seen>>);

    impl Probe {
        fn start(&self, platform: &mut dyn Platform, name: &'static str, at: Instant) -> TimerId {
            let timer = platform.start_timer(at);
            self.0.borrow_mut().started.push((timer, name, at));
            timer
        }
    }

    impl Handler for Probe {
        fn started(&mut self, platform: &mut dyn Platform) {
            let now = Instant::now();
            let after = |millis| now + Duration::from_millis(millis);
            self.start(platform, "last", after(200));
            self.start(platform, "first", after(50));
            let cancelled = self.start(platform, "cancelled", after(100));
            platform.cancel_timer(cancelled);
            self.start(platform, "past", now - Duration::from_millis(10));
        }

        fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
            let now = Instant::now();
            match event {
                AppEvent::Timer(timer) => {
                    let name = {
                        let mut seen = self.0.borrow_mut();
                        let &(_, name, at) = seen
                            .started
                            .iter()
                            .find(|(started, ..)| *started == timer)
                            .expect("a timer that the probe started");
                        if now < at {
                            seen.early.push(name);
                        }
                        seen.fired.push((name, now.saturating_duration_since(at)));
                        name
                    };
                    match name {
                        // A timer started as one fires.
                        "first" => {
                            self.start(platform, "chained", now + Duration::from_millis(30));
                        }
                        // Long enough for the cancelled one to have fired.
                        "last" => common::wake_after(platform.waker(), 200),
                        _ => {}
                    }
                }
                AppEvent::Wake => platform.quit(),
                _ => {}
            }
        }

        fn window_event(&mut self, _: &mut dyn Platform, _: WindowId, _: WindowEvent) {}
    }

    pub fn main() {
        common::deadline("timers", 20);
        let seen = Rc::new(RefCell::new(Seen::default()));
        lokyr_platform_macos::run(Probe(Rc::clone(&seen))).expect("the application runs");
        let seen = seen.borrow();
        let order: Vec<&str> = seen.fired.iter().map(|&(name, _)| name).collect();
        assert_eq!(order, ["past", "first", "chained", "last"], "{seen:?}");
        assert!(seen.early.is_empty(), "{seen:?}");
        assert!(seen.fired.iter().all(|&(_, late)| late < LATE), "{seen:?}");
        println!("timers: ok");
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    probe::main();
}

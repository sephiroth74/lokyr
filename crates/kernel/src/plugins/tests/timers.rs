//! The plugins' timers, of the host's time: once and repeating, one event
//! however many times a timer fired since its plugin last heard of it,
//! cancelled, ended with their plugin, refused in a look, left as they were
//! by a call that fails, and bounded.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use lokyr_plugin_sdk::message::{DELAY_RANGE, TIMERS_BOUND};

use super::*;
use crate::config::Config;

/// A call that a test's plugin makes as its command runs.
type Ask = Box<dyn FnOnce(&mut Cx<'_>) -> Result<(), Error>>;

/// The timers' ids of each batch that a plugin heard.
type Heard = Rc<RefCell<Vec<Vec<u64>>>>;

/// A plugin `timed`, whose command `timed.go` makes the call that the test
/// gave last, and which records the timers' ids of each batch that it hears.
struct Timed {
    next: Rc<RefCell<Option<Ask>>>,
    heard: Heard,
}

impl Plugin for Timed {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("timed.go"))
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        match self.next.borrow_mut().take() {
            Some(ask) => ask(cx),
            None => Ok(()),
        }
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let ids = events
            .iter()
            .filter_map(|event| match event {
                Event::Timer { id } => Some(*id),
                _ => None,
            })
            .collect();
        self.heard.borrow_mut().push(ids);
        Ok(())
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        // A look starts no timer.
        cx.start_timer(Duration::from_millis(10), false).is_ok()
    }
}

/// The plugins of `timed`, activated, and what makes its calls and what it
/// heard.
struct Fixture {
    plugins: Plugins,
    next: Rc<RefCell<Option<Ask>>>,
    heard: Heard,
}

impl Fixture {
    fn new(app: &mut App) -> Self {
        let (next, heard) = (Rc::default(), Rc::default());
        let mut plugins = Plugins::new();
        plugins
            .add(
                "timed",
                Box::new(Timed {
                    next: Rc::clone(&next),
                    heard: Rc::clone(&heard),
                }),
            )
            .unwrap();
        assert!(plugins.activate(&mut app.context()).is_empty());
        Self {
            plugins,
            next,
            heard,
        }
    }

    /// Runs `timed.go`, of the call `ask`: its result.
    fn run(
        &mut self,
        app: &mut App,
        ask: impl FnOnce(&mut Cx<'_>) -> Result<(), Error> + 'static,
    ) -> Result<(), Error> {
        *self.next.borrow_mut() = Some(Box::new(ask));
        self.plugins
            .run("timed.go", Args::new(), &mut app.context())
    }

    /// Runs `timed.go` of a call that starts timers of `delays`, each of
    /// its delay in milliseconds and whether it repeats: their ids, in
    /// their order.
    fn start(&mut self, app: &mut App, delays: &[(u64, bool)]) -> Vec<u64> {
        let ids = Rc::new(RefCell::new(Vec::new()));
        let (kept, delays) = (Rc::clone(&ids), delays.to_vec());
        self.run(app, move |cx| {
            for (delay, repeats) in delays {
                kept.borrow_mut()
                    .push(cx.start_timer(Duration::from_millis(delay), repeats)?);
            }
            Ok(())
        })
        .unwrap();
        ids.take()
    }

    /// Moves the host's time on by `by` milliseconds, fires the timers whose
    /// time came and delivers their events: the batches that the plugin
    /// heard.
    fn wait(&mut self, app: &mut App, by: u64) -> Vec<Vec<u64>> {
        let now = app.wait(Duration::from_millis(by));
        self.plugins.timers_due(now);
        assert!(self.plugins.deliver(&mut app.context()).is_empty());
        self.heard.take()
    }
}

#[test]
fn a_timer_fires_once_and_one_that_repeats_at_each_delay_of_the_host_s_time() {
    let mut app = App::default();
    let mut timed = Fixture::new(&mut app);
    let start = app.now();
    assert_eq!(timed.plugins.next_timer(), None);
    let [once, again] = timed.start(&mut app, &[(100, false), (40, true)])[..] else {
        panic!("two timers");
    };
    assert_eq!(
        timed.plugins.next_timer(),
        Some(start + Duration::from_millis(40))
    );
    // Before their time, nothing, and nothing waits for a delivery.
    let now = app.wait(Duration::from_millis(39));
    timed.plugins.timers_due(now);
    assert!(!timed.plugins.has_events());
    // The one that repeats, which waits again, its event to its plugin
    // alone, which subscribed to none.
    assert_eq!(timed.wait(&mut app, 1), [vec![again]]);
    assert_eq!(
        timed.plugins.next_timer(),
        Some(start + Duration::from_millis(80))
    );
    // At 140 ms, the one of 100 ms, and the one that repeats once, which
    // fired at 80 and at 120 ms, as the app was busy: it waits from now.
    assert_eq!(timed.wait(&mut app, 100), [vec![once, again]]);
    assert_eq!(
        timed.plugins.next_timer(),
        Some(start + Duration::from_millis(180))
    );
    // The one of 100 ms went with its event.
    assert_eq!(timed.wait(&mut app, 40), [vec![again]]);
    assert!(timed.wait(&mut app, 39).is_empty());
}

#[test]
fn a_cancelled_timer_tells_nothing_more_nor_the_event_that_it_owed() {
    let mut app = App::default();
    let mut timed = Fixture::new(&mut app);
    let [again, other] = timed.start(&mut app, &[(10, true), (10, true)])[..] else {
        panic!("two timers");
    };
    // It fired, and its event waits for the delivery as the plugin cancels
    // it: the event goes with it.
    let now = app.wait(Duration::from_millis(10));
    timed.plugins.timers_due(now);
    assert!(timed.plugins.has_events());
    timed
        .run(&mut app, move |cx| cx.cancel_timer(again))
        .unwrap();
    assert!(timed.plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(timed.heard.take(), [vec![other]]);
    // A timer that is not there, as one cancelled, stays so.
    timed
        .run(&mut app, move |cx| {
            cx.cancel_timer(again)?;
            cx.cancel_timer(other)?;
            cx.cancel_timer(999_999)
        })
        .unwrap();
    assert_eq!(timed.plugins.next_timer(), None);
    assert!(timed.wait(&mut app, 1000).is_empty());
}

#[test]
fn a_plugin_has_its_timers_within_their_bounds() {
    let mut app = App::default();
    let mut timed = Fixture::new(&mut app);
    // A delay of none waits the least.
    let start = app.now();
    let [soon] = timed.start(&mut app, &[(0, false)])[..] else {
        panic!("a timer");
    };
    assert_eq!(
        timed.plugins.next_timer(),
        Some(start + Duration::from_millis(host::LEAST_DELAY))
    );
    assert_eq!(timed.wait(&mut app, host::LEAST_DELAY), [vec![soon]]);
    // A day at most.
    let codes = Rc::new(RefCell::new(Vec::new()));
    let ids = Rc::new(RefCell::new(Vec::new()));
    let (seen, taken) = (Rc::clone(&codes), Rc::clone(&ids));
    timed
        .run(&mut app, move |cx| {
            let past = cx.start_timer(Duration::from_millis(host::MOST_DELAY + 1), false);
            seen.borrow_mut().push(past.unwrap_err());
            for _ in 0..host::MOST_TIMERS {
                taken
                    .borrow_mut()
                    .push(cx.start_timer(Duration::from_millis(host::MOST_DELAY), false)?);
            }
            // As many as a plugin has, and then one more, refused.
            seen.borrow_mut()
                .push(cx.start_timer(Duration::from_millis(10), true).unwrap_err());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        *codes.borrow(),
        [
            Error::new(Code::Failed, DELAY_RANGE),
            Error::new(Code::Unavailable, TIMERS_BOUND),
        ]
    );
    // One cancelled, another starts.
    let first = Rc::new(RefCell::new(None));
    let (kept, oldest) = (Rc::clone(&first), ids.borrow()[0]);
    timed
        .run(&mut app, move |cx| {
            let id = cx.start_timer(Duration::from_millis(10), false);
            assert_eq!(id.unwrap_err().code, Code::Unavailable);
            cx.cancel_timer(oldest)?;
            *kept.borrow_mut() = Some(cx.start_timer(Duration::from_millis(10), false)?);
            Ok(())
        })
        .unwrap();
    let started = first.borrow().expect("a timer");
    assert_eq!(timed.wait(&mut app, 10), [vec![started]]);
}

#[test]
fn a_look_starts_no_timer_and_a_failed_call_leaves_the_timers_as_they_were() {
    let mut app = App::default();
    let mut timed = Fixture::new(&mut app);
    assert_eq!(
        timed
            .plugins
            .applies("timed.go", &Args::new(), &mut app.context()),
        Ok(false)
    );
    assert_eq!(timed.plugins.next_timer(), None);
    let [kept] = timed.start(&mut app, &[(20, true)])[..] else {
        panic!("a timer");
    };
    // A call that cancels it, starts another and fails: the one cancelled
    // comes back, and the other's id names nothing.
    let failed = timed.run(&mut app, move |cx| {
        cx.cancel_timer(kept)?;
        cx.start_timer(Duration::from_millis(10), true)?;
        Err(Error::new(Code::Failed, "after its timers"))
    });
    assert_eq!(failed.unwrap_err().code, Code::Failed);
    assert_eq!(timed.wait(&mut app, 10), Vec::<Vec<u64>>::new());
    assert_eq!(timed.wait(&mut app, 10), [vec![kept]]);
}

#[test]
fn a_plugin_s_timers_end_as_it_stops_faults_or_fails_to_start() {
    // Its section turns it off.
    let mut app = App::default();
    let mut timed = Fixture::new(&mut app);
    timed.start(&mut app, &[(10, true)]);
    let off = Config::parse(
        "[plugins.timed]\nenabled = false\n",
        Path::new("config.toml"),
    )
    .unwrap();
    assert!(timed.plugins.configure(&off.plugins).is_empty());
    assert!(timed.plugins.activate(&mut app.context()).is_empty());
    assert_eq!(timed.plugins.status("timed"), Some(Status::Off));
    assert_eq!(timed.plugins.next_timer(), None);
    // Gone, so that the plugin turned on again, made anew, hears of none.
    assert!(timed.plugins.entries[0].state.timers.is_empty());
    assert!(timed.wait(&mut app, 100).is_empty());
    // A call panics.
    let mut timed = Fixture::new(&mut app);
    timed.start(&mut app, &[(10, true)]);
    let fault = timed.run(&mut app, |_| panic!("on purpose"));
    assert_eq!(fault.unwrap_err().code, Code::Fault);
    assert_eq!(timed.plugins.status("timed"), Some(Status::Disabled));
    assert_eq!(timed.plugins.next_timer(), None);
    assert!(timed.plugins.entries[0].state.timers.is_empty());
    assert!(timed.wait(&mut app, 100).is_empty());
    // Its activation, which started one, fails.
    let mut plugins = Plugins::new();
    plugins
        .add(
            "starter",
            scripted(
                |cx| {
                    cx.start_timer(Duration::from_millis(10), true)?;
                    Err(Error::new(Code::Failed, "after its timer"))
                },
                |_, _, _| Ok(()),
            ),
        )
        .unwrap();
    assert_eq!(plugins.activate(&mut app.context()).len(), 1);
    assert_eq!(plugins.status("starter"), Some(Status::Disabled));
    assert_eq!(plugins.next_timer(), None);
    let now = app.wait(Duration::from_millis(100));
    plugins.timers_due(now);
    assert!(!plugins.has_events());
}

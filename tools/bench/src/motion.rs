//! A window's motion, the same each run: a drag of its titlebar, or a
//! resize from its bottom-right corner, along a sine of a period of two
//! seconds, or a move to a place, as onto another display, which the
//! mouse's synthetic events give, as a hand would; or a trackpad's swipes
//! over the window, up and down, each of its momentum after, as precise
//! scroll events of pixels; so that what the window server and the app do
//! meanwhile can be measured, as Instruments records it, one terminal
//! against another.
//!
//! Posting the mouse's events takes the Accessibility permission of the
//! app that runs the probe, as the terminal or the editor that starts it;
//! without it, the probe asks for it and stops. The pointer moves on the
//! screen: nothing else should take the mouse meanwhile.

use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventTapLocation, CGEventType, CGMouseButton,
    CGPreflightPostEventAccess, CGRequestPostEventAccess, CGScrollEventUnit,
};
use serde::Serialize;

use crate::windows::{self, Frame};

/// What the probe moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Kind {
    /// The window, by its titlebar.
    Drag,
    /// The window's size, by its bottom-right corner.
    Resize,
    /// The window, by its titlebar, to a place, in a straight line, as
    /// onto another display before a motion there.
    Move,
    /// The window's content, by a trackpad's swipes over its middle: up
    /// then down each period, each swipe a quarter of it, then its
    /// momentum another quarter, as macOS sends them, so that the view
    /// comes back where it was.
    Scroll,
}

/// What a run did.
#[derive(Debug, Serialize)]
struct Report {
    kind: Kind,
    pid: libc::pid_t,
    /// The seconds since the epoch as the motion began and ended, which
    /// place it in a recording.
    began: f64,
    ended: f64,
    /// The mouse's events posted, its press and its release among them.
    events: usize,
    /// The window before the motion and after it.
    before: Frame,
    after: Frame,
    /// The farthest that the window went from where it was, in points: its
    /// origin's for a drag, its size's for a resize, as the window server
    /// told it every tenth event.
    farthest: f64,
}

/// The period of the sine, as a hand moves back and forth.
const PERIOD: f64 = 2.0;

/// How long before a scroll's event its thread stops sleeping and spins,
/// past a sleep's usual lateness.
const SPIN: Duration = Duration::from_micros(1500);

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let (mut pid, mut kind, mut seconds, mut rate, mut amplitude, mut to) =
        (None, None, 10.0, 120.0, 200.0, None);
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .with_context(|| format!("`{arg}` takes a value"))
        };
        match arg.as_str() {
            "--pid" => pid = Some(value()?.parse::<libc::pid_t>()?),
            "--kind" => {
                kind = Some(match value()?.as_str() {
                    "drag" => Kind::Drag,
                    "resize" => Kind::Resize,
                    "move" => Kind::Move,
                    "scroll" => Kind::Scroll,
                    other => {
                        bail!("`--kind` is `drag`, `resize`, `move` or `scroll`, not `{other}`")
                    }
                });
            }
            "--seconds" => seconds = value()?.parse()?,
            "--rate" => rate = value()?.parse()?,
            "--amplitude" => amplitude = value()?.parse()?,
            "--to" => {
                let text = value()?;
                let (x, y) = text
                    .split_once(',')
                    .with_context(|| format!("`--to` is `x,y`, not `{text}`"))?;
                to = Some((x.trim().parse::<f64>()?, y.trim().parse::<f64>()?));
            }
            other => bail!("unknown option `{other}`"),
        }
    }
    let pid = pid.context("`--pid` names the process whose window moves")?;
    let kind = kind.context("`--kind` is `drag`, `resize`, `move` or `scroll`")?;
    if kind == Kind::Move && to.is_none() {
        bail!("`--kind move` takes `--to x,y`, the window's top left corner to come");
    }
    if !CGPreflightPostEventAccess() {
        CGRequestPostEventAccess();
        bail!(
            "posting the mouse's events takes the Accessibility permission of the app that runs lokyr-bench: grant it in System Settings, Privacy & Security, Accessibility, and run again"
        );
    }
    let before = *windows::frames(pid)
        .first()
        .with_context(|| format!("the process {pid} has no window on screen"))?;
    // Within the titlebar, past its buttons; or just inside the corner; or
    // the window's middle.
    let start = match kind {
        Kind::Drag | Kind::Move => CGPoint::new(before.x + before.width / 2.0, before.y + 12.0),
        Kind::Resize => CGPoint::new(
            before.x + before.width - 2.0,
            before.y + before.height - 2.0,
        ),
        Kind::Scroll => CGPoint::new(
            before.x + before.width / 2.0,
            before.y + before.height / 2.0,
        ),
    };
    let post = |event_type: CGEventType, at: CGPoint| -> Result<()> {
        let event = CGEvent::new_mouse_event(None, event_type, at, CGMouseButton::Left)
            .context("an event of the mouse")?;
        CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&event));
        Ok(())
    };
    let now = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
    };
    if kind == Kind::Scroll {
        post(CGEventType::MouseMoved, start)?;
        std::thread::sleep(Duration::from_millis(100));
        // Away from the screen first, so that the swipes down meet no end.
        swipe(start, 2000.0, 12, Duration::from_millis(8))?;
        std::thread::sleep(Duration::from_secs(1));
        let began = now();
        let events = scroll(start, seconds, rate, amplitude)?;
        let ended = now();
        let report = Report {
            kind,
            pid,
            began,
            ended,
            events,
            before,
            after: before,
            farthest: amplitude,
        };
        println!("{}", serde_json::to_string(&report)?);
        return Ok(ExitCode::SUCCESS);
    }
    let began = now();
    post(CGEventType::MouseMoved, start)?;
    std::thread::sleep(Duration::from_millis(100));
    post(CGEventType::LeftMouseDown, start)?;
    std::thread::sleep(Duration::from_millis(100));
    let (steps, tick) = (
        (seconds * rate) as usize,
        Duration::from_secs_f64(1.0 / rate),
    );
    let origin = Instant::now();
    let mut at = start;
    let mut farthest: f64 = 0.0;
    for step in 1..=steps {
        let time = step as f64 / rate;
        let offset = amplitude * (std::f64::consts::TAU * time / PERIOD).sin();
        at = match (kind, to) {
            (Kind::Move, Some((x, y))) => {
                let done = (step as f64 / steps as f64).min(1.0);
                CGPoint::new(
                    start.x + (x - before.x) * done,
                    start.y + (y - before.y) * done,
                )
            }
            (Kind::Drag | Kind::Move, _) => CGPoint::new(start.x + offset, start.y + offset / 4.0),
            (Kind::Resize | Kind::Scroll, _) => {
                CGPoint::new(start.x + offset / 2.0, start.y + offset / 4.0)
            }
        };
        post(CGEventType::LeftMouseDragged, at)?;
        if step % 10 == 0
            && let Some(now) = windows::frames(pid).first()
        {
            let (dx, dy) = match kind {
                Kind::Drag | Kind::Move => (now.x - before.x, now.y - before.y),
                Kind::Resize | Kind::Scroll => {
                    (now.width - before.width, now.height - before.height)
                }
            };
            farthest = farthest.max(dx.hypot(dy));
        }
        let due = origin + tick * u32::try_from(step).unwrap_or(u32::MAX);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    post(CGEventType::LeftMouseUp, at)?;
    let ended = now();
    std::thread::sleep(Duration::from_millis(300));
    let after = windows::frames(pid).first().copied().unwrap_or(before);
    let report = Report {
        kind,
        pid,
        began,
        ended,
        events: steps + 3,
        before,
        after,
        farthest,
    };
    println!("{}", serde_json::to_string(&report)?);
    Ok(ExitCode::SUCCESS)
}

/// A scroll event of `pixels` up, a trackpad's of its precise deltas, at
/// `at`, of the scroll's phase `phase` and the momentum's `momentum`, as
/// macOS numbers them: began 1, changed 2 and ended 4 of the fingers', and
/// begin 1, continue 2 and end 3 of the momentum's, 0 of none.
fn post_scroll(at: CGPoint, pixels: i32, phase: i64, momentum: i64) -> Result<()> {
    let event = CGEvent::new_scroll_wheel_event2(None, CGScrollEventUnit::Pixel, 1, pixels, 0, 0)
        .context("an event of the scroll")?;
    let event = Some(&*event);
    CGEvent::set_location(event, at);
    CGEvent::set_integer_value_field(event, CGEventField::ScrollWheelEventIsContinuous, 1);
    CGEvent::set_integer_value_field(event, CGEventField::ScrollWheelEventScrollPhase, phase);
    CGEvent::set_integer_value_field(event, CGEventField::ScrollWheelEventMomentumPhase, momentum);
    CGEvent::post(CGEventTapLocation::HIDEventTap, event);
    Ok(())
}

/// A swipe of `pixels` up, down where negative, at `at`, in `steps`
/// events `tick` apart, the fingers' alone, of no momentum.
fn swipe(at: CGPoint, pixels: f64, steps: usize, tick: Duration) -> Result<()> {
    let each = (pixels / steps as f64).round() as i32;
    for step in 0..steps {
        post_scroll(at, each, if step == 0 { 1 } else { 2 }, 0)?;
        std::thread::sleep(tick);
    }
    post_scroll(at, 0, 4, 0)
}

/// Swipes at `at` for `seconds`, `rate` events a second: each period, a
/// swipe up of `amplitude` points over a quarter of it, its momentum over
/// the next, slowing to a pixel an event, then the same down; the events
/// posted.
fn scroll(at: CGPoint, seconds: f64, rate: f64, amplitude: f64) -> Result<usize> {
    let quarter = PERIOD / 4.0;
    let per = (quarter * rate).round().max(2.0) as usize;
    let tick = Duration::from_secs_f64(1.0 / rate);
    // The fingers' points of each event, and the momentum's first, which
    // then slows as a square to none: its sum a third of the swipe's.
    let fingers = amplitude / per as f64;
    // The events on time, as a trackpad's come: the thread of the
    // interactive class, which the system's timers do not coalesce, asleep
    // until a moment before each event, then spinning to it.
    // SAFETY: a call on the calling thread of a class of the system's.
    #[allow(unsafe_code)]
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
    let origin = Instant::now();
    let mut posted = 0usize;
    let wait = |posted: usize| {
        let due = origin + tick * u32::try_from(posted).unwrap_or(u32::MAX);
        if let Some(wait) = due
            .checked_duration_since(Instant::now())
            .and_then(|wait| wait.checked_sub(SPIN))
        {
            std::thread::sleep(wait);
        }
        while Instant::now() < due {
            std::hint::spin_loop();
        }
    };
    let periods = (seconds / PERIOD).round().max(1.0) as usize;
    for _ in 0..periods {
        for sign in [1.0, -1.0] {
            for step in 0..per {
                let phase = if step == 0 { 1 } else { 2 };
                post_scroll(at, (sign * fingers).round() as i32, phase, 0)?;
                posted += 1;
                wait(posted);
            }
            post_scroll(at, 0, 4, 0)?;
            for step in 0..per {
                let left = 1.0 - step as f64 / per as f64;
                // Down to a pixel an event, never none, so that the view
                // moves throughout.
                let pixels = (sign * (fingers * left * left).round().max(1.0)) as i32;
                let momentum = match step {
                    0 => 1,
                    _ if step + 1 == per => 3,
                    _ => 2,
                };
                post_scroll(at, pixels, 0, momentum)?;
                posted += 1;
                wait(posted);
            }
        }
    }
    Ok(posted)
}

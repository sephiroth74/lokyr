//! A window's motion, the same each run: a drag of its titlebar, or a
//! resize from its bottom-right corner, along a sine of a period of two
//! seconds, which the mouse's synthetic events give, as a hand would, so
//! that what the window server and the app do meanwhile can be measured,
//! as Instruments records it, one terminal against another.
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
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, CGPreflightPostEventAccess,
    CGRequestPostEventAccess,
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

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let (mut pid, mut kind, mut seconds, mut rate, mut amplitude) =
        (None, None, 10.0, 120.0, 200.0);
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
                    other => bail!("`--kind` is `drag` or `resize`, not `{other}`"),
                });
            }
            "--seconds" => seconds = value()?.parse()?,
            "--rate" => rate = value()?.parse()?,
            "--amplitude" => amplitude = value()?.parse()?,
            other => bail!("unknown option `{other}`"),
        }
    }
    let pid = pid.context("`--pid` names the process whose window moves")?;
    let kind = kind.context("`--kind` is `drag` or `resize`")?;
    if !CGPreflightPostEventAccess() {
        CGRequestPostEventAccess();
        bail!(
            "posting the mouse's events takes the Accessibility permission of the app that runs lokyr-bench: grant it in System Settings, Privacy & Security, Accessibility, and run again"
        );
    }
    let before = *windows::frames(pid)
        .first()
        .with_context(|| format!("the process {pid} has no window on screen"))?;
    // Within the titlebar, past its buttons; or just inside the corner.
    let start = match kind {
        Kind::Drag => CGPoint::new(before.x + before.width / 2.0, before.y + 12.0),
        Kind::Resize => CGPoint::new(
            before.x + before.width - 2.0,
            before.y + before.height - 2.0,
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
        at = match kind {
            Kind::Drag => CGPoint::new(start.x + offset, start.y + offset / 4.0),
            Kind::Resize => CGPoint::new(start.x + offset / 2.0, start.y + offset / 4.0),
        };
        post(CGEventType::LeftMouseDragged, at)?;
        if step % 10 == 0
            && let Some(now) = windows::frames(pid).first()
        {
            let (dx, dy) = match kind {
                Kind::Drag => (now.x - before.x, now.y - before.y),
                Kind::Resize => (now.width - before.width, now.height - before.height),
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

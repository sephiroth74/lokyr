//! The cursor: the `[cursor]` section of the configuration, the shape and
//! the blinking that it takes from the configuration and from the programs,
//! and the clock of its blinking.
//!
//! ```toml
//! [cursor]
//! shape = "block"   # block | bar | underline | hollow
//! blink = true
//! thickness = 2     # a bar's width and an underline's height, in points,
//!                   # 0.5 to 20; the face's underline thickness when absent
//! length = 0.6      # a bar's height and an underline's width, a share of
//!                   # the cell, 0.1 to 1, centered in it
//! blink-interval = 500         # on, then off, in ms, without smooth-blink
//! blink-timeout = 15           # seconds after a key or output; 0 never stops
//! smooth-blink = "breathing"   # off | breathing | linear | fast | slow | subtle | custom
//! fade-in = 900                # with "custom": the fades, in ms, their curves,
//! fade-in-curve = "ease-in-out" # and how long it holds visible and hidden
//! visible = 500
//! fade-out = 900
//! fade-out-curve = "ease-in-out"
//! hidden = 500
//! movement = "instant"         # instant | snappy
//! ```

use std::time::{Duration, Instant};

use lokyr_render_core::CursorShape;
use lokyr_text_core::CellMetrics;
use lokyr_vt_core::terminal::{self, SnapshotCursor};
use serde::{Deserialize, Serialize};

/// How long the blinking cursor stays on, then off, by default.
pub const BLINK_INTERVAL: Duration = Duration::from_millis(500);

/// How long the cursor blinks after the last key or output by default; then
/// it stays on, so that an idle window takes no wakeups.
pub const BLINK_TIMEOUT: Duration = Duration::from_secs(15);

/// The `[cursor]` section: the cursor that programs start with, and that
/// `CSI 0 SP q` and a reset give back.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct CursorConfig {
    /// Its shape.
    pub shape: Shape,
    /// Whether it blinks.
    pub blink: bool,
    /// A bar's width and an underline's height; `None` for the face's
    /// underline thickness.
    pub thickness: Option<Thickness>,
    /// A bar's height and an underline's width, of the cell's.
    pub length: Share,
    /// How long it stays on, then off, without smooth blinking.
    pub blink_interval: Interval,
    /// How long it blinks after the last key or output: `None` for ever.
    pub blink_timeout: Timeout,
    /// Whether it fades out and in, and how: by a preset, or by the keys
    /// that follow.
    pub smooth_blink: SmoothBlink,
    /// With [`SmoothBlink::Custom`]: the fade in.
    pub fade_in: Span,
    /// With [`SmoothBlink::Custom`]: the fade in's curve.
    pub fade_in_curve: Curve,
    /// With [`SmoothBlink::Custom`]: how long it holds visible.
    pub visible: Span,
    /// With [`SmoothBlink::Custom`]: the fade out.
    pub fade_out: Span,
    /// With [`SmoothBlink::Custom`]: the fade out's curve.
    pub fade_out_curve: Curve,
    /// With [`SmoothBlink::Custom`]: how long it holds hidden.
    pub hidden: Span,
    /// How it moves to a new cell.
    pub movement: Movement,
}

impl Default for CursorConfig {
    fn default() -> Self {
        let breathing = SmoothBlink::BREATHING;
        Self {
            shape: Shape::Block,
            blink: true,
            thickness: None,
            length: Share(1.0),
            blink_interval: Interval(500),
            blink_timeout: Timeout(15),
            smooth_blink: SmoothBlink::Off,
            fade_in: Span(breathing.fade_in),
            fade_in_curve: breathing.fade_in_curve,
            visible: Span(breathing.visible),
            fade_out: Span(breathing.fade_out),
            fade_out_curve: breathing.fade_out_curve,
            hidden: Span(breathing.hidden),
            movement: Movement::Instant,
        }
    }
}

impl CursorConfig {
    /// The cycle of the blinking: on and off for `blink-interval` each, or
    /// smooth-blink's fades and holds, its preset's or its keys'.
    pub fn pattern(&self) -> Pattern {
        let millis = |millis: u32| Duration::from_millis(u64::from(millis));
        let steps = match self.smooth_blink {
            SmoothBlink::Off => {
                let interval = self.blink_interval.0;
                return Pattern {
                    visible: millis(interval),
                    fade_out: Duration::ZERO,
                    fade_out_curve: Curve::Linear,
                    hidden: millis(interval),
                    fade_in: Duration::ZERO,
                    fade_in_curve: Curve::Linear,
                };
            }
            SmoothBlink::Custom => Steps {
                fade_in: self.fade_in.0,
                fade_in_curve: self.fade_in_curve,
                visible: self.visible.0,
                fade_out: self.fade_out.0,
                fade_out_curve: self.fade_out_curve,
                hidden: self.hidden.0,
            },
            preset => preset.steps(),
        };
        Pattern {
            visible: millis(steps.visible),
            fade_out: millis(steps.fade_out),
            fade_out_curve: steps.fade_out_curve,
            hidden: millis(steps.hidden),
            fade_in: millis(steps.fade_in),
            fade_in_curve: steps.fade_in_curve,
        }
    }

    /// How long the cursor blinks after a key or output, `None` for ever.
    pub fn timeout(&self) -> Option<Duration> {
        (self.blink_timeout.0 > 0).then(|| Duration::from_secs(u64::from(self.blink_timeout.0)))
    }

    /// The pixels of a bar's width and an underline's height, in a cell of
    /// `cell` on a display of `scale`: one at least.
    pub fn thickness(&self, cell: &CellMetrics, scale: f32) -> u32 {
        self.thickness
            .map_or(cell.underline_thickness, |thickness| {
                // A cast from a float saturates.
                (thickness.points() * scale).round().max(1.0) as u32
            })
    }
}

/// A thickness of the cursor, in points.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Thickness(f32);

impl From<Thickness> for f64 {
    fn from(thickness: Thickness) -> Self {
        lokyr_text_core::as_written(thickness.0)
    }
}

impl Thickness {
    /// The thinnest.
    pub const MIN: f32 = 0.5;
    /// The thickest.
    pub const MAX: f32 = 20.0;

    /// The thickness in points.
    pub fn points(self) -> f32 {
        self.0
    }
}

impl TryFrom<f64> for Thickness {
    type Error = String;

    fn try_from(points: f64) -> Result<Self, Self::Error> {
        if (f64::from(Self::MIN)..=f64::from(Self::MAX)).contains(&points) {
            // Within the range, the value fits an `f32`.
            Ok(Self(points as f32))
        } else {
            Err(format!(
                "the cursor's thickness, in points, is from {} to {}, not {points}",
                Self::MIN,
                Self::MAX
            ))
        }
    }
}

/// The share of the cell that a bar's height and an underline's width
/// take.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Share(f32);

impl From<Share> for f64 {
    fn from(share: Share) -> Self {
        lokyr_text_core::as_written(share.0)
    }
}

impl Share {
    /// The shortest, as the config writes it: of `f64`, as `0.1` of `f32`
    /// is above the config's `0.1`.
    pub const MIN: f64 = 0.1;
    /// The whole cell.
    pub const MAX: f64 = 1.0;

    /// The share, from 0.1 to 1.
    pub fn value(self) -> f32 {
        self.0
    }
}

impl TryFrom<f64> for Share {
    type Error = String;

    fn try_from(share: f64) -> Result<Self, Self::Error> {
        if (Self::MIN..=Self::MAX).contains(&share) {
            // Within the range, the value fits an `f32`.
            Ok(Self(share as f32))
        } else {
            Err(format!(
                "the cursor's length, a share of the cell, is from {} to {}, not {share}",
                Self::MIN,
                Self::MAX
            ))
        }
    }
}

/// How long the cursor stays on, then off, without smooth blinking, in ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Interval(u32);

impl From<Interval> for u32 {
    fn from(interval: Interval) -> Self {
        interval.0
    }
}

impl TryFrom<u32> for Interval {
    type Error = String;

    fn try_from(millis: u32) -> Result<Self, Self::Error> {
        if (100..=5000).contains(&millis) {
            Ok(Self(millis))
        } else {
            Err(format!(
                "the blink's interval, in ms, is from 100 to 5000, not {millis}"
            ))
        }
    }
}

/// How long the cursor blinks after the last key or output, in seconds: 0
/// for ever.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Timeout(u32);

impl From<Timeout> for u32 {
    fn from(timeout: Timeout) -> Self {
        timeout.0
    }
}

impl TryFrom<u32> for Timeout {
    type Error = String;

    fn try_from(seconds: u32) -> Result<Self, Self::Error> {
        if seconds <= 3600 {
            Ok(Self(seconds))
        } else {
            Err(format!(
                "the blink's timeout, in seconds, is from 0 to 3600, not {seconds}"
            ))
        }
    }
}

/// A fade's duration, or how long the cursor holds, in ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct Span(u32);

impl From<Span> for u32 {
    fn from(span: Span) -> Self {
        span.0
    }
}

impl TryFrom<u32> for Span {
    type Error = String;

    fn try_from(millis: u32) -> Result<Self, Self::Error> {
        if millis <= 10_000 {
            Ok(Self(millis))
        } else {
            Err(format!(
                "a fade, or a hold, in ms, is from 0 to 10000, not {millis}"
            ))
        }
    }
}

/// Whether the cursor fades out and in as it blinks, rather than switching,
/// and how: by one of iTerm2's presets, or by the keys of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SmoothBlink {
    /// It switches, on and off.
    #[default]
    Off,
    /// A slow, symmetric breath.
    Breathing,
    /// Fades of a constant rate.
    Linear,
    /// Quick fades.
    Fast,
    /// Slow, its fade out longer than its fade in.
    Slow,
    /// Mostly on, with a brief dip.
    Subtle,
    /// The fades and the holds of the keys of `[cursor]`.
    Custom,
}

/// A smooth blink's steps, in ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Steps {
    fade_in: u32,
    fade_in_curve: Curve,
    visible: u32,
    fade_out: u32,
    fade_out_curve: Curve,
    hidden: u32,
}

impl SmoothBlink {
    /// Breathing's steps, which the custom keys take by default.
    const BREATHING: Steps = Steps {
        fade_in: 900,
        fade_in_curve: Curve::EaseInOut,
        visible: 500,
        fade_out: 900,
        fade_out_curve: Curve::EaseInOut,
        hidden: 500,
    };

    /// The steps of a preset, as iTerm2 has them.
    fn steps(self) -> Steps {
        let eased = |fade_in, visible, fade_out, hidden| Steps {
            fade_in,
            fade_in_curve: Curve::EaseInOut,
            visible,
            fade_out,
            fade_out_curve: Curve::EaseInOut,
            hidden,
        };
        match self {
            Self::Linear => Steps {
                fade_in_curve: Curve::Linear,
                fade_out_curve: Curve::Linear,
                ..eased(400, 700, 400, 500)
            },
            Self::Fast => eased(200, 700, 200, 200),
            Self::Slow => eased(500, 1000, 750, 200),
            Self::Subtle => eased(150, 900, 150, 200),
            Self::Off | Self::Breathing | Self::Custom => Self::BREATHING,
        }
    }
}

/// How the cursor moves to a new cell: at once, or eased there, the
/// window server sliding its layer, the text at its next frame still.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Movement {
    /// At once.
    #[default]
    Instant,
    /// Eased there quickly, slowing as it comes.
    Snappy,
}

impl Movement {
    /// How long its slide takes, and its curve; `None` of none.
    pub fn slide(self) -> Option<(Duration, Curve)> {
        match self {
            Self::Instant => None,
            Self::Snappy => Some((Duration::from_millis(80), Curve::EaseOut)),
        }
    }
}

/// How a fade goes, from its start to its end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Curve {
    /// At a constant rate.
    Linear,
    /// Slow at its start.
    EaseIn,
    /// Slow at its end.
    EaseOut,
    /// Slow at its start and at its end.
    #[default]
    EaseInOut,
}

/// A blink's cycle: the cursor holds visible, fades out, holds hidden, and
/// fades in; without fades, it switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// How long it holds visible.
    pub visible: Duration,
    /// Its fade out.
    pub fade_out: Duration,
    /// The fade out's curve.
    pub fade_out_curve: Curve,
    /// How long it holds hidden.
    pub hidden: Duration,
    /// Its fade in.
    pub fade_in: Duration,
    /// The fade in's curve.
    pub fade_in_curve: Curve,
}

impl Pattern {
    /// The cycle's length.
    pub fn cycle(&self) -> Duration {
        self.visible + self.fade_out + self.hidden + self.fade_in
    }
}

/// The shape of the cursor, as the configuration names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    /// A block over the cell.
    #[default]
    Block,
    /// A vertical bar at the cell's left edge.
    Bar,
    /// A line under the cell.
    Underline,
    /// A block of its edges alone.
    Hollow,
}

impl From<Shape> for CursorShape {
    fn from(shape: Shape) -> Self {
        match shape {
            Shape::Block => Self::Block,
            Shape::Bar => Self::Bar,
            Shape::Underline => Self::Underline,
            Shape::Hollow => Self::Hollow,
        }
    }
}

/// The shape that the cursor takes: the one that the program asked for with
/// DECSCUSR, or the configuration's; a hollow block in a window without the
/// focus.
pub fn shape(cursor: &SnapshotCursor, config: &CursorConfig, focused: bool) -> CursorShape {
    if !focused {
        return CursorShape::Hollow;
    }
    match cursor.style.map(|style| style.shape) {
        Some(terminal::CursorShape::Block) => CursorShape::Block,
        Some(terminal::CursorShape::Bar) => CursorShape::Bar,
        Some(terminal::CursorShape::Underline) => CursorShape::Underline,
        None => config.shape.into(),
    }
}

/// Whether the cursor blinks, in the window that has the focus alone, and
/// while it shows: as the program asked with DECSCUSR, or as the
/// configuration says. Mode 12, which `cvvis` sets in xterm's terminfo,
/// makes it blink too; its reset, which `cnorm` sends as each full-screen
/// program ends, leaves the configuration's blinking as it is. A cursor that
/// the program hides (DECTCEM) does not blink, and takes no wakeup.
pub fn blinks(cursor: &SnapshotCursor, config: &CursorConfig, focused: bool) -> bool {
    focused
        && cursor.visible
        && match cursor.style {
            Some(style) => style.blinking,
            None => config.blink || cursor.blink,
        }
}

/// The clock of the blinking cursor and text: on, then off, for the
/// config's interval each, from the last key or output and until its
/// timeout; then on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blink {
    since: Instant,
}

impl Blink {
    /// A clock that starts blinking at `now`, on.
    pub fn new(now: Instant) -> Self {
        Self { since: now }
    }

    /// Starts blinking again at `now`, on, as a key or output does.
    pub fn restart(&mut self, now: Instant) {
        self.since = now;
    }

    /// When it started blinking, last.
    pub fn since(&self) -> Instant {
        self.since
    }

    /// When it stops blinking, after `config`'s timeout; `None` for never.
    pub fn stops(&self, config: &CursorConfig) -> Option<Instant> {
        config.timeout().map(|timeout| self.since + timeout)
    }

    /// Whether it blinks still at `now`, before `config`'s timeout.
    pub fn blinks(&self, now: Instant, config: &CursorConfig) -> bool {
        self.stops(config).is_none_or(|stops| now < stops)
    }

    /// Whether what blinks shows at `now`, on and off for `config`'s
    /// interval, and when that changes next: `None` once it stays on.
    pub fn at(&self, now: Instant, config: &CursorConfig) -> (bool, Option<Instant>) {
        if !self.blinks(now, config) {
            return (true, None);
        }
        let interval = Duration::from_millis(u64::from(config.blink_interval.0));
        let elapsed = now.saturating_duration_since(self.since);
        let phase = elapsed.as_nanos() / interval.as_nanos();
        // An hour's phases at most, which fit a `u32`.
        let next = self.since + interval * u32::try_from(phase + 1).unwrap_or(u32::MAX);
        let next = self.stops(config).map_or(next, |stops| next.min(stops));
        (phase.is_multiple_of(2), Some(next))
    }
}

#[cfg(test)]
mod tests {
    use lokyr_vt_core::terminal::CursorStyle;

    use super::*;

    fn program(style: Option<(terminal::CursorShape, bool)>, blink: bool) -> SnapshotCursor {
        SnapshotCursor {
            style: style.map(|(shape, blinking)| CursorStyle { shape, blinking }),
            blink,
            visible: true,
            ..SnapshotCursor::default()
        }
    }

    fn config(shape: Shape, blink: bool) -> CursorConfig {
        CursorConfig {
            shape,
            blink,
            ..CursorConfig::default()
        }
    }

    #[test]
    fn the_thickness_is_the_face_s_or_the_configuration_s_points() {
        let cell = CellMetrics {
            width: 16,
            height: 32,
            baseline: 25,
            underline_top: 27,
            underline_thickness: 2,
            strikeout_top: 15,
            strikeout_thickness: 2,
        };
        let mut config = CursorConfig::default();
        assert_eq!(config.thickness(&cell, 2.0), 2);
        config.thickness = Some(Thickness::try_from(1.5).unwrap());
        assert_eq!(config.thickness(&cell, 1.0), 2);
        assert_eq!(config.thickness(&cell, 2.0), 3);
        config.thickness = Some(Thickness::try_from(0.5).unwrap());
        assert_eq!(config.thickness(&cell, 1.0), 1);
        assert!(Thickness::try_from(0.4).is_err());
        assert!(Thickness::try_from(20.5).is_err());
        assert!(Thickness::try_from(f64::NAN).is_err());
    }

    #[test]
    fn the_program_s_shape_takes_the_configuration_s_place() {
        let bar = config(Shape::Bar, true);
        assert_eq!(shape(&program(None, false), &bar, true), CursorShape::Bar);
        let underline = program(Some((terminal::CursorShape::Underline, false)), false);
        assert_eq!(shape(&underline, &bar, true), CursorShape::Underline);
        let hollow = config(Shape::Hollow, false);
        assert_eq!(
            shape(&program(None, false), &hollow, true),
            CursorShape::Hollow
        );
        // Without the focus, a hollow block.
        assert_eq!(shape(&underline, &bar, false), CursorShape::Hollow);
    }

    #[test]
    fn the_program_s_blinking_takes_the_configuration_s_place() {
        let steady = program(Some((terminal::CursorShape::Block, false)), true);
        assert!(!blinks(&steady, &config(Shape::Block, true), true));
        let blinking = program(Some((terminal::CursorShape::Bar, true)), false);
        assert!(blinks(&blinking, &config(Shape::Block, false), true));
        // Mode 12 turns blinking on, and its reset leaves the
        // configuration's.
        assert!(blinks(
            &program(None, true),
            &config(Shape::Block, false),
            true
        ));
        assert!(blinks(
            &program(None, false),
            &config(Shape::Block, true),
            true
        ));
        assert!(!blinks(
            &program(None, false),
            &config(Shape::Block, false),
            true
        ));
        // Only the window that has the focus blinks, and a hidden cursor
        // does not.
        assert!(!blinks(&blinking, &config(Shape::Block, true), false));
        let hidden = SnapshotCursor {
            visible: false,
            ..blinking
        };
        assert!(!blinks(&hidden, &config(Shape::Block, true), true));
    }

    #[test]
    fn blinking_goes_on_and_off_then_stops_on() {
        let start = Instant::now();
        let blink = Blink::new(start);
        let config = CursorConfig::default();
        let at = |millis: u64| blink.at(start + Duration::from_millis(millis), &config);
        let after = |millis: u64| Some(start + Duration::from_millis(millis));
        assert_eq!(at(0), (true, after(500)));
        assert_eq!(at(499), (true, after(500)));
        assert_eq!(at(500), (false, after(1000)));
        assert_eq!(at(1250), (true, after(1500)));
        // The last phase, off, ends with the timeout.
        assert_eq!(at(14_600), (false, after(15_000)));
        assert_eq!(at(15_000), (true, None));
        assert_eq!(at(60_000), (true, None));
    }

    #[test]
    fn a_restart_blinks_again_from_on() {
        let start = Instant::now();
        let mut blink = Blink::new(start);
        let config = CursorConfig::default();
        let later = start + Duration::from_secs(20);
        assert_eq!(blink.at(later, &config), (true, None));
        blink.restart(later);
        assert_eq!(
            blink.at(later + Duration::from_millis(700), &config),
            (false, Some(later + Duration::from_millis(1000)))
        );
    }

    #[test]
    fn the_config_s_interval_and_timeout_time_the_clock() {
        let start = Instant::now();
        let blink = Blink::new(start);
        let config = CursorConfig {
            blink_interval: Interval::try_from(200).unwrap(),
            blink_timeout: Timeout::try_from(1).unwrap(),
            ..CursorConfig::default()
        };
        let after = |millis: u64| start + Duration::from_millis(millis);
        assert_eq!(blink.at(after(250), &config), (false, Some(after(400))));
        assert_eq!(blink.at(after(900), &config), (true, Some(after(1000))));
        assert_eq!(blink.at(after(1000), &config), (true, None));
        assert_eq!(blink.stops(&config), Some(after(1000)));
        // A timeout of 0 never stops.
        let never = CursorConfig {
            blink_timeout: Timeout::try_from(0).unwrap(),
            ..config
        };
        assert_eq!(blink.stops(&never), None);
        assert!(blink.blinks(after(3_600_000), &never));
        assert!(blink.at(after(3_600_100), &never).0);
        // Out of their ranges, none.
        assert!(Interval::try_from(99).is_err() && Interval::try_from(5001).is_err());
        assert!(Timeout::try_from(3601).is_err());
        assert!(Span::try_from(10_001).is_err());
    }

    #[test]
    fn each_smooth_blink_has_iterm2_s_steps_or_the_keys() {
        let millis = |millis| Duration::from_millis(millis);
        let pattern = |smooth_blink| {
            CursorConfig {
                smooth_blink,
                ..CursorConfig::default()
            }
            .pattern()
        };
        let eased = Curve::EaseInOut;
        for (preset, steps) in [
            (SmoothBlink::Breathing, (900, 500, 900, 500, eased)),
            (SmoothBlink::Linear, (400, 700, 400, 500, Curve::Linear)),
            (SmoothBlink::Fast, (200, 700, 200, 200, eased)),
            (SmoothBlink::Slow, (500, 1000, 750, 200, eased)),
            (SmoothBlink::Subtle, (150, 900, 150, 200, eased)),
        ] {
            let (fade_in, visible, fade_out, hidden, curve) = steps;
            assert_eq!(
                pattern(preset),
                Pattern {
                    visible: millis(visible),
                    fade_out: millis(fade_out),
                    fade_out_curve: curve,
                    hidden: millis(hidden),
                    fade_in: millis(fade_in),
                    fade_in_curve: curve,
                },
                "{preset:?}"
            );
        }
        // Off switches, on and off for the interval each.
        let off = pattern(SmoothBlink::Off);
        assert_eq!((off.visible, off.hidden), (millis(500), millis(500)));
        assert_eq!(
            (off.fade_in, off.fade_out),
            (Duration::ZERO, Duration::ZERO)
        );
        assert_eq!(off.cycle(), millis(1000));
        // Custom takes the keys, Breathing's by default.
        assert_eq!(
            pattern(SmoothBlink::Custom),
            pattern(SmoothBlink::Breathing)
        );
        let custom = CursorConfig {
            smooth_blink: SmoothBlink::Custom,
            fade_in: Span::try_from(100).unwrap(),
            fade_in_curve: Curve::EaseIn,
            visible: Span::try_from(200).unwrap(),
            fade_out: Span::try_from(300).unwrap(),
            fade_out_curve: Curve::EaseOut,
            hidden: Span::try_from(400).unwrap(),
            ..CursorConfig::default()
        }
        .pattern();
        assert_eq!(
            (custom.fade_in, custom.fade_in_curve, custom.fade_out_curve),
            (millis(100), Curve::EaseIn, Curve::EaseOut)
        );
        assert_eq!(custom.cycle(), millis(1000));
    }
}

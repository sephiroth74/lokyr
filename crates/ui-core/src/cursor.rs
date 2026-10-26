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
//! ```

use std::time::{Duration, Instant};

use bolt_render_core::CursorShape;
use bolt_text_core::CellMetrics;
use bolt_vt_core::terminal::{self, SnapshotCursor};
use serde::Deserialize;

/// How long the blinking cursor stays on, then off.
pub const BLINK_INTERVAL: Duration = Duration::from_millis(500);

/// How long the cursor blinks after the last key or output; then it stays
/// on, so that an idle window takes no wakeups.
pub const BLINK_TIMEOUT: Duration = Duration::from_secs(15);

/// The `[cursor]` section: the cursor that programs start with, and that
/// `CSI 0 SP q` and a reset give back.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct CursorConfig {
    /// Its shape.
    pub shape: Shape,
    /// Whether it blinks.
    pub blink: bool,
    /// A bar's width and an underline's height; `None` for the face's
    /// underline thickness.
    pub thickness: Option<Thickness>,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: Shape::Block,
            blink: true,
            thickness: None,
        }
    }
}

impl CursorConfig {
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
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(try_from = "f64")]
pub struct Thickness(f32);

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

/// The shape of the cursor, as the configuration names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
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

/// Whether the cursor blinks, in the window that has the focus alone: as
/// the program asked with DECSCUSR, or as the configuration says. Mode 12,
/// which `cvvis` sets in xterm's terminfo, makes it blink too; its reset,
/// which `cnorm` sends as each full-screen program ends, leaves the
/// configuration's blinking as it is.
pub fn blinks(cursor: &SnapshotCursor, config: &CursorConfig, focused: bool) -> bool {
    focused
        && match cursor.style {
            Some(style) => style.blinking,
            None => config.blink || cursor.blink,
        }
}

/// The clock of the blinking cursor: on, then off, for [`BLINK_INTERVAL`]
/// each, from the last key or output and for [`BLINK_TIMEOUT`]; then on.
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

    /// Whether the cursor shows at `now`, and when that changes next:
    /// `None` once it stays on.
    pub fn at(&self, now: Instant) -> (bool, Option<Instant>) {
        let elapsed = now.saturating_duration_since(self.since);
        if elapsed >= BLINK_TIMEOUT {
            return (true, None);
        }
        let phase = elapsed.as_nanos() / BLINK_INTERVAL.as_nanos();
        // Fewer phases than the timeout's, which fits a `u32`.
        let next = self.since + BLINK_INTERVAL * (phase as u32 + 1);
        (
            phase.is_multiple_of(2),
            Some(next.min(self.since + BLINK_TIMEOUT)),
        )
    }
}

#[cfg(test)]
mod tests {
    use bolt_vt_core::terminal::CursorStyle;

    use super::*;

    fn program(style: Option<(terminal::CursorShape, bool)>, blink: bool) -> SnapshotCursor {
        SnapshotCursor {
            style: style.map(|(shape, blinking)| CursorStyle { shape, blinking }),
            blink,
            ..SnapshotCursor::default()
        }
    }

    fn config(shape: Shape, blink: bool) -> CursorConfig {
        CursorConfig {
            shape,
            blink,
            thickness: None,
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
        // Only the window that has the focus blinks.
        assert!(!blinks(&blinking, &config(Shape::Block, true), false));
    }

    #[test]
    fn blinking_goes_on_and_off_then_stops_on() {
        let start = Instant::now();
        let blink = Blink::new(start);
        let at = |millis: u64| blink.at(start + Duration::from_millis(millis));
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
        let later = start + Duration::from_secs(20);
        assert_eq!(blink.at(later), (true, None));
        blink.restart(later);
        assert_eq!(
            blink.at(later + Duration::from_millis(700)),
            (false, Some(later + Duration::from_millis(1000)))
        );
    }
}

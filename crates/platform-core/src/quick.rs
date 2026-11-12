//! Quick windows, which slide in from an edge of a screen over every app's
//! windows, and out again; and the keys that the app takes from every app,
//! which show them.

use std::time::Duration;

use crate::{KeyCode, ScreenRect};

/// A key that the app takes from every app, as the system gives it: by its
/// place on the keyboard, with the modifiers held, of either side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlobalKey {
    /// The key.
    pub code: KeyCode,
    /// Command, or Super.
    pub command: bool,
    /// Control.
    pub control: bool,
    /// Alt, or Option.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
}

/// Where a quick window comes from: an edge of its screen, which it slides
/// in from, or the centre, where it fades in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuickEdge {
    /// The top edge.
    #[default]
    Top,
    /// The bottom edge.
    Bottom,
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The centre.
    Center,
}

/// A length of a quick window, of its screen's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Length {
    /// A percentage of the screen's, from 0 to 100.
    Percent(f64),
    /// Points.
    Points(f64),
}

impl Length {
    /// Its points, of `whole` points, within them.
    pub fn of(self, whole: f64) -> f64 {
        match self {
            Self::Percent(percent) => whole * percent.clamp(0.0, 100.0) / 100.0,
            Self::Points(points) => points.clamp(0.0, whole.max(0.0)),
        }
    }
}

/// The screen that a quick window shows on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuickScreen {
    /// The system's main one, of the window that has the keyboard's focus,
    /// whichever app's.
    #[default]
    Main,
    /// The one that the pointer is on.
    Pointer,
    /// The one of the menu bar.
    MenuBar,
}

/// Where a quick window shows, and how it slides in and out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuickPlace {
    /// The edge that it comes from, or the centre.
    pub edge: QuickEdge,
    /// How far it reaches into its screen from its edge: its height at the
    /// top or the bottom, its width at the left, the right or the centre.
    pub primary: Length,
    /// Its length along its edge: its width at the top or the bottom, its
    /// height at the left, the right or the centre; `None` for the screen's
    /// whole, or, at the centre, `primary`'s share of the screen's height.
    pub secondary: Option<Length>,
    /// Its screen.
    pub screen: QuickScreen,
    /// How long it slides, or fades; zero for at once.
    pub animation: Duration,
    /// Whether it shows on every Space, the one in front, rather than on
    /// the one that it showed on.
    pub every_space: bool,
}

impl Default for QuickPlace {
    fn default() -> Self {
        Self {
            edge: QuickEdge::Top,
            primary: Length::Percent(50.0),
            secondary: None,
            screen: QuickScreen::Main,
            animation: Duration::from_millis(200),
            every_space: true,
        }
    }
}

impl QuickPlace {
    /// Whether the quick window fades in and out, rather than slides: at
    /// the centre, and where the frame that it slides from, `from`, lies
    /// over one of the `others` screens, which the slide would cross.
    pub fn fades(&self, from: ScreenRect, others: &[ScreenRect]) -> bool {
        self.edge == QuickEdge::Center || others.iter().any(|other| other.overlaps(&from))
    }

    /// The quick window's way in and out on a screen of the frame `screen`,
    /// whose area for windows is `area`, beside the `others` screens: the
    /// frame where it shows, the one that it comes from and goes to, and
    /// whether it fades rather than slides. A window that fades comes and
    /// goes where it shows, so that it never crosses another screen.
    pub fn way(
        &self,
        screen: ScreenRect,
        area: ScreenRect,
        others: &[ScreenRect],
    ) -> (ScreenRect, ScreenRect, bool) {
        let (shown, from) = self.frames(screen, area);
        let fades = self.fades(from, others);
        (shown, if fades { shown } else { from }, fades)
    }

    /// The quick window's frames on a screen of the frame `screen`, whose
    /// area for windows, below the menu bar and beside the Dock, is `area`:
    /// the one where it shows, within the area, centred along its edge; and
    /// the one that it slides from, past the screen's edge, or, at the
    /// centre, the same, where it fades. In whole points.
    pub fn frames(&self, screen: ScreenRect, area: ScreenRect) -> (ScreenRect, ScreenRect) {
        let length = |whole: f64, default: f64| {
            self.secondary
                .map_or(default, |secondary| secondary.of(whole))
                .round()
        };
        let centred = |start: f64, whole: f64, part: f64| (start + (whole - part) / 2.0).round();
        match self.edge {
            QuickEdge::Top | QuickEdge::Bottom => {
                let height = self.primary.of(area.height).round();
                let width = length(area.width, area.width);
                let x = centred(area.x, area.width, width);
                let (shown, hidden) = if self.edge == QuickEdge::Top {
                    (area.y, screen.y - height)
                } else {
                    (area.y + area.height - height, screen.y + screen.height)
                };
                let at = |y| ScreenRect {
                    x,
                    y,
                    width,
                    height,
                };
                (at(shown.round()), at(hidden.round()))
            }
            QuickEdge::Left | QuickEdge::Right => {
                let width = self.primary.of(area.width).round();
                let height = length(area.height, area.height);
                let y = centred(area.y, area.height, height);
                let (shown, hidden) = if self.edge == QuickEdge::Left {
                    (area.x, screen.x - width)
                } else {
                    (area.x + area.width - width, screen.x + screen.width)
                };
                let at = |x| ScreenRect {
                    x,
                    y,
                    width,
                    height,
                };
                (at(shown.round()), at(hidden.round()))
            }
            QuickEdge::Center => {
                let width = self.primary.of(area.width).round();
                let height = length(area.height, self.primary.of(area.height));
                let frame = ScreenRect {
                    x: centred(area.x, area.width, width),
                    y: centred(area.y, area.height, height),
                    width,
                    height,
                };
                (frame, frame)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A screen of 1440 by 900 points, right of the menu bar's, whose menu
    /// bar takes 25 points and whose Dock 60 at the bottom.
    const SCREEN: ScreenRect = ScreenRect {
        x: 1440.0,
        y: 0.0,
        width: 1440.0,
        height: 900.0,
    };
    const AREA: ScreenRect = ScreenRect {
        x: 1440.0,
        y: 25.0,
        width: 1440.0,
        height: 815.0,
    };

    fn rect(x: f64, y: f64, width: f64, height: f64) -> ScreenRect {
        ScreenRect {
            x,
            y,
            width,
            height,
        }
    }

    fn place(edge: QuickEdge, primary: Length, secondary: Option<Length>) -> QuickPlace {
        QuickPlace {
            edge,
            primary,
            secondary,
            ..QuickPlace::default()
        }
    }

    #[test]
    fn a_quick_window_shows_at_its_edge_and_slides_from_past_the_screen() {
        // Half the area's height under the menu bar, the screen's width;
        // from above the screen.
        let top = place(QuickEdge::Top, Length::Percent(50.0), None);
        assert_eq!(
            top.frames(SCREEN, AREA),
            (
                rect(1440.0, 25.0, 1440.0, 408.0),
                rect(1440.0, -408.0, 1440.0, 408.0)
            )
        );
        // Above the Dock, of 300 points and 80% of the width, centred;
        // from below the screen.
        let bottom = place(
            QuickEdge::Bottom,
            Length::Points(300.0),
            Some(Length::Percent(80.0)),
        );
        assert_eq!(
            bottom.frames(SCREEN, AREA),
            (
                rect(1584.0, 540.0, 1152.0, 300.0),
                rect(1584.0, 900.0, 1152.0, 300.0)
            )
        );
        // At the sides, the area's height, a quarter of its width.
        let left = place(QuickEdge::Left, Length::Percent(25.0), None);
        assert_eq!(
            left.frames(SCREEN, AREA),
            (
                rect(1440.0, 25.0, 360.0, 815.0),
                rect(1080.0, 25.0, 360.0, 815.0)
            )
        );
        let right = place(
            QuickEdge::Right,
            Length::Points(400.0),
            Some(Length::Points(415.0)),
        );
        assert_eq!(
            right.frames(SCREEN, AREA),
            (
                rect(2480.0, 225.0, 400.0, 415.0),
                rect(2880.0, 225.0, 400.0, 415.0)
            )
        );
    }

    #[test]
    fn at_the_centre_it_fades_of_its_share_both_ways_or_its_own_height() {
        let centre = place(QuickEdge::Center, Length::Percent(50.0), None);
        let shown = rect(1800.0, 229.0, 720.0, 408.0);
        assert_eq!(centre.frames(SCREEN, AREA), (shown, shown));
        let sized = place(
            QuickEdge::Center,
            Length::Points(800.0),
            Some(Length::Points(2000.0)),
        );
        // No taller than the area.
        let shown = rect(1760.0, 25.0, 800.0, 815.0);
        assert_eq!(sized.frames(SCREEN, AREA), (shown, shown));
    }

    #[test]
    fn past_an_edge_that_another_screen_touches_it_fades() {
        // A screen left of this one, the menu bar's, and a laptop's below.
        let left = rect(0.0, 0.0, 1440.0, 900.0);
        let below = rect(1440.0, 900.0, 1512.0, 982.0);
        let others = [left, below];
        let fades = |edge| {
            let place = place(edge, Length::Percent(50.0), None);
            let (_, from) = place.frames(SCREEN, AREA);
            place.fades(from, &others)
        };
        assert!(fades(QuickEdge::Left));
        assert!(fades(QuickEdge::Bottom));
        assert!(!fades(QuickEdge::Top));
        assert!(!fades(QuickEdge::Right));
        assert!(fades(QuickEdge::Center));
        // A screen that touches the corner alone, past no edge's frame.
        let corner = rect(2880.0, 900.0, 800.0, 600.0);
        let place = place(QuickEdge::Right, Length::Percent(50.0), None);
        let (_, from) = place.frames(SCREEN, AREA);
        assert!(!place.fades(from, &[corner]));
    }

    #[test]
    fn a_window_that_fades_comes_and_goes_where_it_shows() {
        // A laptop's screen below, past the bottom edge.
        let below = rect(1440.0, 900.0, 1512.0, 982.0);
        let bottom = place(QuickEdge::Bottom, Length::Percent(50.0), None);
        let (shown, from) = bottom.frames(SCREEN, AREA);
        assert_ne!(shown, from, "it would slide over the screen below");
        assert_eq!(bottom.way(SCREEN, AREA, &[below]), (shown, shown, true));
        // Past an edge that no screen touches, it slides from past it.
        let top = place(QuickEdge::Top, Length::Percent(50.0), None);
        let (shown, from) = top.frames(SCREEN, AREA);
        assert_eq!(top.way(SCREEN, AREA, &[below]), (shown, from, false));
    }

    #[test]
    fn a_length_takes_its_share_or_its_points_within_the_whole() {
        assert_eq!(Length::Percent(25.0).of(800.0), 200.0);
        assert_eq!(Length::Percent(150.0).of(800.0), 800.0);
        assert_eq!(Length::Points(300.0).of(200.0), 200.0);
        assert_eq!(Length::Points(300.0).of(800.0), 300.0);
    }
}

//! The progress bars, rings and dots: their natural sizes, the room where a
//! ring or the dots go in their box, and the sweeps of the indeterminate
//! ones, which the host hands the platform, whose window server moves them
//! without the app's frames: a segment across a bar, an arc round a ring,
//! the dots lit in turn, each within the clips of the nodes around it.

use std::time::{Duration, Instant};

use lokyr_plugin_sdk::view::{Kind, Progress, ProgressShape, ProgressState};
use lokyr_render_core::{Color, Rect};

use crate::animation::Animations;
use crate::layout::{Frame, Layout};
use crate::paint::{inside_border, intersection, is_empty, rect};
use crate::theme::Tokens;
use crate::tree::{NodeId, Tree};

/// A bar's natural size, in points, which its layout's replaces.
pub const BAR: (f32, f32) = (160.0, 4.0);

/// A ring's natural side, in points.
pub const RING: f32 = 16.0;

/// The dots' natural side, in points.
pub const DOTS: f32 = 10.0;

/// The part of the dots' side that a dot's diameter takes.
const DOT: f32 = 0.3;

/// How long an indeterminate bar's sweep takes, a ring's turn, and an
/// outline's run along it, of a progress that gives none.
pub const PERIODS: (Duration, Duration, Duration) = (
    Duration::from_millis(1500),
    Duration::from_secs(1),
    Duration::from_secs(2),
);

/// How long the dots' round takes, each lit once, of a progress that gives
/// none.
pub const DOTS_PERIOD: Duration = Duration::from_millis(1200);

/// The part of an indeterminate progress that the window server moves: its
/// box, in pixels from the tree's corner, the part of it within its clips,
/// its corners' radius, its color, its shape and how long a sweep takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepArea {
    /// Its progress's node.
    pub node: NodeId,
    /// The bar's box, or the ring's square.
    pub rect: Rect,
    /// The part of it that shows, within the clips around it.
    pub visible: Rect,
    /// The bar's corners' radius, in pixels.
    pub radius: f32,
    /// The segment's color.
    pub color: Color,
    /// What moves.
    pub shape: SweepShape,
    /// How long a sweep across, or a turn, takes.
    pub period: Duration,
}

/// The fill of a progress that is not indeterminate, as far as its value,
/// which the window server draws, so that a change of the value slides with
/// no frame of the host's: its node, by which the platform keeps it from one
/// placing to the next, its box, in pixels from the tree's corner, the part
/// of it within its clips, its corners' radius, its color, its shape and
/// its value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillArea {
    /// Its progress's node.
    pub node: NodeId,
    /// The bar's box, the ring's square or the outline's box.
    pub rect: Rect,
    /// The part of it that shows, within the clips around it.
    pub visible: Rect,
    /// The bar's corners' radius, or the outline's, in pixels.
    pub radius: f32,
    /// Its color.
    pub color: Color,
    /// What it fills.
    pub shape: FillShape,
    /// How far, from 0 to 1.
    pub value: f32,
}

/// What a fill fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillShape {
    /// A bar, from its left.
    Bar,
    /// A ring, clockwise from its top, of its line's thickness in pixels.
    Ring {
        /// The line's thickness.
        thickness: f32,
    },
    /// An outline, from its left along its top and its bottom, of its
    /// line's thickness in pixels.
    Outline {
        /// The line's thickness.
        thickness: f32,
    },
}

/// What a sweep moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SweepShape {
    /// A segment of a bar, of its width in pixels, across it.
    Bar {
        /// The segment's width.
        width: f32,
    },
    /// An arc of a ring, of its span of the turn and its line's thickness
    /// in pixels, round it.
    Ring {
        /// The part of the turn.
        span: f32,
        /// The line's thickness.
        thickness: f32,
    },
    /// A segment of an outline, of its span of the outline's length and its
    /// line's thickness in pixels, along it, clockwise.
    Outline {
        /// The part of the length.
        span: f32,
        /// The line's thickness.
        thickness: f32,
    },
    /// Three dots, lit in turn, clockwise from the top, of their diameter
    /// and their centers in pixels from the square's corner, as [`dots`]
    /// places them.
    Dots {
        /// A dot's diameter.
        diameter: f32,
        /// The dots' centers, the top one first, then clockwise.
        centers: [(f32, f32); 3],
    },
}

/// The natural size of a progress of `shape`, in points.
pub fn natural(shape: ProgressShape) -> (f32, f32) {
    match shape {
        ProgressShape::Bar => BAR,
        ProgressShape::Ring => (RING, RING),
        ProgressShape::Dots => (DOTS, DOTS),
        // Its parent's box, which its place gives it.
        ProgressShape::Outline => (0.0, 0.0),
    }
}

/// The square of a ring in `frame`: its side the frame's shorter one, in
/// its middle.
pub fn ring_square(frame: Frame) -> Frame {
    let side = frame.width.min(frame.height);
    Frame {
        x: frame.x + (frame.width - side) / 2.0,
        y: frame.y + (frame.height - side) / 2.0,
        width: side,
        height: side,
    }
}

/// The dots in a square of `side`: their centers from its corner, the top
/// one first, then clockwise, the bottom right and the bottom left, in a
/// triangle of equal sides as wide as the square less a dot, in its
/// middle; and a dot's diameter.
pub fn dots(side: f32) -> ([(f32, f32); 3], f32) {
    let diameter = side * DOT;
    let wide = side - diameter;
    let high = wide * 3.0_f32.sqrt() / 2.0;
    let (top, bottom) = ((side - high) / 2.0, (side + high) / 2.0);
    let half = diameter / 2.0;
    (
        [(side / 2.0, top), (side - half, bottom), (half, bottom)],
        diameter,
    )
}

/// The radius of the corners of a bar of `progress`, `height` pixels high,
/// in pixels at `scale`: its own, or a pill's.
pub fn radius(progress: &Progress, height: f32, scale: f32) -> f32 {
    progress
        .radius
        .map_or(height / 2.0, |radius| radius * scale)
}

/// The radius of the corners of an outline of `progress`, `id` of `tree`,
/// in pixels at `scale`: its own, or its parent's.
pub fn outline_radius(tree: &Tree, id: NodeId, progress: &Progress, scale: f32) -> f32 {
    progress.radius.map_or_else(
        || {
            tree.parent(id)
                .and_then(|parent| tree.style(parent))
                .map_or(0.0, |style| style.radius * scale)
        },
        |radius| radius * scale,
    )
}

/// The sweeps of the indeterminate progresses of `tree`, laid out in
/// `layout`, of `tokens`' colors, in the tree's order, as `paint` draws
/// their nodes: of the opacity of their nodes and of those around them,
/// and where the transitions of `motion` at its time slide and fade them;
/// those that their clips hide, or of no opacity, are left out.
pub fn sweeps(
    tree: &Tree,
    layout: &Layout,
    tokens: &Tokens,
    motion: Option<(&Animations, Instant)>,
) -> Vec<SweepArea> {
    parts(tree, layout, tokens, motion).sweeps
}

/// The fills of the progresses of `tree` that are not indeterminate, laid
/// out in `layout`, of `tokens`' colors, in the tree's order, as `paint`
/// draws their nodes, as [`sweeps`] gives the sweeps: those that their
/// clips hide, or of no opacity, are left out.
pub fn fills(
    tree: &Tree,
    layout: &Layout,
    tokens: &Tokens,
    motion: Option<(&Animations, Instant)>,
) -> Vec<FillArea> {
    parts(tree, layout, tokens, motion).fills
}

/// The parts of the progresses of `tree` that the window server draws.
#[derive(Default)]
struct Parts {
    sweeps: Vec<SweepArea>,
    fills: Vec<FillArea>,
}

fn parts(
    tree: &Tree,
    layout: &Layout,
    tokens: &Tokens,
    motion: Option<(&Animations, Instant)>,
) -> Parts {
    let mut parts = Parts::default();
    if let Some(root) = tree.root() {
        let shown = Shown {
            clip: None,
            offset: (0, 0),
            opacity: 1.0,
        };
        walk((tree, layout, tokens, motion), root, shown, &mut parts);
    }
    parts
}

/// What the nodes around a node do to its sweep: their clips, of no clip
/// for `None`; how far their transitions moved them, in pixels; and the
/// opacity of them all.
#[derive(Clone, Copy)]
struct Shown {
    clip: Option<Rect>,
    offset: (i32, i32),
    opacity: f32,
}

fn walk(
    (tree, layout, tokens, motion): (&Tree, &Layout, &Tokens, Option<(&Animations, Instant)>),
    id: NodeId,
    around: Shown,
    parts: &mut Parts,
) {
    let (Some(kind), Some(style), Some(frame)) =
        (tree.kind(id), tree.style(id), layout.frame(tree, id))
    else {
        return;
    };
    // Its own opacity, and its transition's, as `paint` takes them.
    let own = style.opacity.unwrap_or(1.0);
    let (offset, opacity) = match (motion, tree.key(id)) {
        (Some((animations, now)), Some(key)) => (
            animations.offset(key, now),
            animations.opacity(key, own, now),
        ),
        _ => ((0.0, 0.0), own),
    };
    let around = Shown {
        offset: (
            around.offset.0.saturating_add(offset.0.round() as i32),
            around.offset.1.saturating_add(offset.1.round() as i32),
        ),
        opacity: around.opacity * opacity,
        ..around
    };
    if around.opacity <= 0.0 {
        return;
    }
    let frame = Frame {
        x: frame.x + around.offset.0 as f32,
        y: frame.y + around.offset.1 as f32,
        ..frame
    };
    let clip = around.clip;
    let scale = layout.scale();
    if let Kind::Progress(progress) = kind
        && progress.state != ProgressState::Indeterminate
    {
        let state = match progress.state {
            ProgressState::Error => "progress.error",
            ProgressState::Paused => "progress.paused",
            ProgressState::Normal | ProgressState::Indeterminate => "progress.normal",
        };
        let color = style
            .color
            .as_ref()
            .and_then(|token| tokens.get(&token.0))
            .or_else(|| tokens.get(state));
        let filled = match progress.shape {
            ProgressShape::Bar => {
                Some((frame, FillShape::Bar, radius(progress, frame.height, scale)))
            }
            ProgressShape::Ring => Some((
                ring_square(frame),
                FillShape::Ring {
                    thickness: progress.thickness * scale,
                },
                0.0,
            )),
            ProgressShape::Outline => Some((
                frame,
                FillShape::Outline {
                    thickness: progress.thickness * scale,
                },
                outline_radius(tree, id, progress, scale),
            )),
            // The dots of a value are the host's to draw.
            ProgressShape::Dots => None,
        };
        if let Some((boxed, shape, radius)) = filled
            && let Some(color) = color
            && !is_empty(clip.map_or(rect(boxed), |clip| intersection(clip, rect(boxed))))
        {
            let pixels = rect(boxed);
            let visible = clip.map_or(pixels, |clip| intersection(clip, pixels));
            parts.fills.push(FillArea {
                node: id,
                rect: pixels,
                visible,
                radius,
                color: Color {
                    alpha: color.alpha * around.opacity,
                    ..color
                },
                shape,
                value: progress.value.clamp(0.0, 1.0),
            });
        }
    }
    if let Kind::Progress(progress) = kind
        && progress.state == ProgressState::Indeterminate
    {
        let color = style
            .color
            .as_ref()
            .and_then(|token| tokens.get(&token.0))
            .or_else(|| tokens.get("progress.normal"));
        let (boxed, shape) = match progress.shape {
            ProgressShape::Bar => (
                frame,
                SweepShape::Bar {
                    width: (frame.width * progress.segment).round(),
                },
            ),
            ProgressShape::Ring => (
                ring_square(frame),
                SweepShape::Ring {
                    span: progress.segment,
                    thickness: progress.thickness * scale,
                },
            ),
            ProgressShape::Outline => (
                frame,
                SweepShape::Outline {
                    span: progress.segment,
                    thickness: progress.thickness * scale,
                },
            ),
            ProgressShape::Dots => {
                let square = ring_square(frame);
                let (centers, diameter) = dots(square.width);
                (square, SweepShape::Dots { diameter, centers })
            }
        };
        let pixels = rect(boxed);
        let visible = clip.map_or(pixels, |clip| intersection(clip, pixels));
        if let Some(color) = color
            && !is_empty(visible)
        {
            let period = progress.period.map_or(
                match progress.shape {
                    ProgressShape::Bar => PERIODS.0,
                    ProgressShape::Ring => PERIODS.1,
                    ProgressShape::Outline => PERIODS.2,
                    ProgressShape::Dots => DOTS_PERIOD,
                },
                Duration::from_secs_f32,
            );
            let radius = match progress.shape {
                ProgressShape::Outline => outline_radius(tree, id, progress, scale),
                ProgressShape::Dots => 0.0,
                _ => radius(progress, frame.height, scale),
            };
            parts.sweeps.push(SweepArea {
                node: id,
                rect: pixels,
                visible,
                radius,
                color: Color {
                    alpha: color.alpha * around.opacity,
                    ..color
                },
                shape,
                period,
            });
        }
    }
    let inner = if style.clip || matches!(kind, Kind::Scroll) {
        let inside = inside_border(rect(frame), style, scale);
        Some(clip.map_or(inside, |clip| intersection(clip, inside)))
    } else {
        clip
    };
    for &child in tree.children(id) {
        walk(
            (tree, layout, tokens, motion),
            child,
            Shown {
                clip: inner,
                ..around
            },
            parts,
        );
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{self, Direction, Length, Node, Style, Token};

    use super::*;
    use crate::layout::Measure;
    use crate::theme::Theme;

    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (6.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    fn of(progress: Progress) -> Node {
        Node::new(Kind::Progress(progress))
    }

    /// `children` in a column, laid out at `scale` in 200 points.
    fn laid(children: Vec<Node>, scale: f32) -> (Tree, Layout) {
        let mut tree = Tree::new();
        let root = Node::container(children).layout(view::Layout {
            direction: Direction::Column,
            gap: 10.0,
            ..view::Layout::default()
        });
        let changes = tree.update(&root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0 * scale), None, &mut Fixed);
        (tree, layout)
    }

    fn token(name: &str) -> Color {
        Theme::builtin().dark.tokens.get(name).unwrap()
    }

    #[test]
    fn a_bar_and_a_ring_take_their_natural_sizes_and_a_ring_its_square() {
        let ring = Progress {
            shape: ProgressShape::Ring,
            ..Progress::default()
        };
        let (tree, layout) = laid(
            vec![
                of(Progress::default()).layout(view::Layout {
                    width: Length::Auto,
                    ..view::Layout::default()
                }),
                of(ring),
            ],
            2.0,
        );
        let root = tree.root().unwrap();
        let frame = |index: usize| layout.frame(&tree, tree.children(root)[index]).unwrap();
        // In a column, across its width; 4 points high, a ring 16.
        assert_eq!(frame(0).height, 8.0);
        assert_eq!((frame(1).width, frame(1).height), (400.0, 32.0));
        assert_eq!(
            ring_square(frame(1)),
            Frame {
                x: 184.0,
                y: frame(1).y,
                width: 32.0,
                height: 32.0
            }
        );
        assert_eq!(natural(ProgressShape::Bar), BAR);
        // Of a frame taller than wide, its width's square, down its middle.
        let tall = Frame {
            x: 10.0,
            y: 0.0,
            width: 20.0,
            height: 40.0,
        };
        assert_eq!(
            ring_square(tall),
            Frame {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0
            }
        );
    }

    #[test]
    fn the_indeterminate_ones_sweep_within_their_clips() {
        let bar = of(Progress {
            state: ProgressState::Indeterminate,
            segment: 0.25,
            ..Progress::default()
        })
        .style(Style {
            color: Some(Token::from("accent")),
            ..Style::default()
        });
        let ring = of(Progress {
            state: ProgressState::Indeterminate,
            shape: ProgressShape::Ring,
            thickness: 3.0,
            segment: 0.4,
            period: Some(0.5),
            radius: Some(2.0),
            ..Progress::default()
        })
        .layout(view::Layout {
            height: Length::Points(20.0),
            ..view::Layout::default()
        });
        let normal = of(Progress::default());
        // A scroll area of 30 points, of a bar 25 points down, which shows
        // its top half.
        let hidden = Node::scroll(vec![
            Node::container(vec![]).layout(view::Layout {
                height: Length::Points(25.0),
                ..view::Layout::default()
            }),
            of(Progress {
                state: ProgressState::Indeterminate,
                ..Progress::default()
            })
            .layout(view::Layout {
                height: Length::Points(10.0),
                ..view::Layout::default()
            }),
        ])
        .key("area")
        .layout(view::Layout {
            direction: Direction::Column,
            height: Length::Points(30.0),
            ..view::Layout::default()
        });
        let (tree, layout) = laid(vec![bar, ring, normal, hidden], 2.0);
        let tokens = &Theme::builtin().dark.tokens;
        let sweeps = sweeps(&tree, &layout, tokens, None);
        assert_eq!(sweeps.len(), 3);
        // The bar: of the style's color, a quarter of its 400 pixels, a
        // pill's corners, the bar's period.
        let bar = sweeps[0];
        assert_eq!(
            bar.rect,
            Rect {
                x: 0,
                y: 0,
                width: 400,
                height: 8
            }
        );
        assert_eq!(bar.visible, bar.rect);
        assert_eq!(bar.color, token("accent"));
        assert_eq!(bar.shape, SweepShape::Bar { width: 100.0 });
        assert_eq!(bar.radius, 4.0);
        assert_eq!(bar.period, PERIODS.0);
        // The ring: its square, of the state's color, its thickness and its
        // period.
        let ring = sweeps[1];
        assert_eq!(
            ring.rect,
            Rect {
                x: 180,
                y: 28,
                width: 40,
                height: 40
            }
        );
        assert_eq!(ring.color, token("progress.normal"));
        assert_eq!(
            ring.shape,
            SweepShape::Ring {
                span: 0.4,
                thickness: 6.0
            }
        );
        assert_eq!(ring.radius, 4.0);
        assert_eq!(ring.period, Duration::from_millis(500));
        // The one in the scroll area: of its 20 pixels, the 10 that show.
        let clipped = sweeps[2];
        assert_eq!(clipped.rect.height, 20);
        assert_eq!(clipped.visible.height, 10);
        assert_eq!(clipped.visible.y, clipped.rect.y);
    }

    #[test]
    fn a_sweep_takes_the_opacity_and_the_slide_of_its_nodes() {
        use std::time::Duration;

        use lokyr_plugin_sdk::view::{Easing, Transition};

        use crate::animation::Animations;

        let spinning = || {
            of(Progress {
                state: ProgressState::Indeterminate,
                ..Progress::default()
            })
            .layout(view::Layout {
                height: Length::Points(10.0),
                ..view::Layout::default()
            })
        };
        let within = |opacity: f32, child: Node| {
            Node::container(vec![child]).style(Style {
                opacity: Some(opacity),
                ..Style::default()
            })
        };
        // Of a node of no opacity, none; of one of half inside one of
        // half, a quarter of its color's.
        let (tree, layout) = laid(
            vec![
                within(0.0, spinning()),
                within(0.5, within(0.5, spinning())),
            ],
            1.0,
        );
        let tokens = &Theme::builtin().dark.tokens;
        let shown = sweeps(&tree, &layout, tokens, None);
        assert_eq!(shown.len(), 1);
        let normal = token("progress.normal");
        assert_eq!(
            shown[0].color,
            Color {
                alpha: normal.alpha * 0.25,
                ..normal
            }
        );
        // A box of a transition that a layout moved down by 20 points:
        // halfway, its sweep 10 points above its place, and of half its
        // fade from none.
        let moving = |above: f32, opacity: f32| {
            Node::container(vec![
                Node::container(vec![]).layout(view::Layout {
                    height: Length::Points(above),
                    ..view::Layout::default()
                }),
                Node::container(vec![spinning()]).key("box").style(Style {
                    opacity: Some(opacity),
                    transition: Some(Transition {
                        duration: 0.2,
                        easing: Easing::Linear,
                    }),
                    ..Style::default()
                }),
            ])
            .layout(view::Layout {
                direction: Direction::Column,
                ..view::Layout::default()
            })
        };
        let mut tree = Tree::new();
        let mut layout = Layout::new(1.0);
        let mut animations = Animations::new();
        let start = Instant::now();
        for (above, opacity) in [(0.0, 0.0), (20.0, 1.0)] {
            let changes = tree.update(&moving(above, opacity)).unwrap();
            layout.update(&tree, &changes);
            layout.compute(&tree, Some(200.0), None, &mut Fixed);
            animations.laid_out(&tree, &layout, start, false);
        }
        let half = start + Duration::from_millis(100);
        let shown = sweeps(&tree, &layout, tokens, Some((&animations, half)));
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].rect.y, 10);
        assert!(
            (shown[0].color.alpha - normal.alpha * 0.5).abs() < 1e-3,
            "{shown:?}"
        );
    }

    /// A box as a tab's, 100 by 24 points, of corners of 6, padded, of a
    /// text and an outline of `progress` over it.
    fn outlined(progress: Progress) -> Node {
        Node::container(vec![
            Node::text("make"),
            of(Progress {
                shape: ProgressShape::Outline,
                ..progress
            }),
        ])
        .layout(view::Layout {
            width: Length::Points(100.0),
            height: Length::Points(24.0),
            padding: view::Edges {
                top: 4.0,
                right: 8.0,
                bottom: 4.0,
                left: 8.0,
            },
            ..view::Layout::default()
        })
        .style(Style {
            radius: 6.0,
            ..Style::default()
        })
    }

    #[test]
    fn an_outline_takes_its_parent_s_whole_box_and_none_of_its_children_s_room() {
        let (tree, layout) = laid(vec![outlined(Progress::default())], 2.0);
        let root = tree.root().unwrap();
        let tab = tree.children(root)[0];
        let (text, outline) = (tree.children(tab)[0], tree.children(tab)[1]);
        let frame = |id| layout.frame(&tree, id).unwrap();
        assert_eq!(frame(outline), frame(tab));
        // The text at the padding's corner, as without the outline.
        assert_eq!((frame(text).x, frame(text).y), (16.0, 8.0));
        assert_eq!(natural(ProgressShape::Outline), (0.0, 0.0));
    }

    #[test]
    fn an_indeterminate_outline_sweeps_along_its_parent_s_corners() {
        let (tree, layout) = laid(
            vec![outlined(Progress {
                state: ProgressState::Indeterminate,
                thickness: 1.5,
                segment: 0.2,
                ..Progress::default()
            })],
            2.0,
        );
        let tokens = Theme::builtin().dark.tokens;
        let swept = sweeps(&tree, &layout, &tokens, None);
        let [sweep] = swept[..] else {
            panic!("one sweep: {swept:?}");
        };
        assert_eq!(
            sweep.rect,
            Rect {
                x: 0,
                y: 0,
                width: 200,
                height: 48
            }
        );
        assert_eq!(sweep.radius, 12.0);
        assert_eq!(
            sweep.shape,
            SweepShape::Outline {
                span: 0.2,
                thickness: 3.0
            }
        );
        assert_eq!(sweep.period, PERIODS.2);
        assert_eq!(sweep.color, token("progress.normal"));
        // Of a radius of its own.
        let (tree, layout) = laid(
            vec![outlined(Progress {
                state: ProgressState::Indeterminate,
                radius: Some(2.0),
                ..Progress::default()
            })],
            2.0,
        );
        assert_eq!(sweeps(&tree, &layout, &tokens, None)[0].radius, 4.0);
        assert!(fills(&tree, &layout, &tokens, None).is_empty());
    }

    #[test]
    fn the_dots_lie_in_a_triangle_in_their_square_and_light_in_turn() {
        // Of a side of 20: dots of 6, a triangle of sides of 14 in its
        // middle, the top one first, then clockwise.
        let (centers, diameter) = dots(20.0);
        assert!((diameter - 6.0).abs() < 1e-5);
        let high = 14.0 * 3.0_f32.sqrt() / 2.0;
        let wanted = [
            (10.0, (20.0 - high) / 2.0),
            (17.0, (20.0 + high) / 2.0),
            (3.0, (20.0 + high) / 2.0),
        ];
        for (center, wanted) in centers.iter().zip(wanted) {
            assert!(
                (center.0 - wanted.0).abs() < 1e-4 && (center.1 - wanted.1).abs() < 1e-4,
                "{centers:?}"
            );
        }
        // Every dot within the square.
        assert!(centers.iter().all(|&(x, y)| {
            x - diameter / 2.0 >= 0.0
                && x + diameter / 2.0 <= 20.0
                && y - diameter / 2.0 >= 0.0
                && y + diameter / 2.0 <= 20.0
        }));
        assert_eq!(natural(ProgressShape::Dots), (DOTS, DOTS));
        // An indeterminate one's sweep, of its square in its box's middle,
        // of the dots' period; one of a value has no fill of the window
        // server's.
        let spinning = of(Progress {
            state: ProgressState::Indeterminate,
            shape: ProgressShape::Dots,
            ..Progress::default()
        });
        let valued = of(Progress {
            value: 0.5,
            shape: ProgressShape::Dots,
            ..Progress::default()
        });
        let (tree, layout) = laid(vec![spinning, valued], 2.0);
        let tokens = Theme::builtin().dark.tokens;
        let swept = sweeps(&tree, &layout, &tokens, None);
        let [sweep] = swept[..] else {
            panic!("one sweep: {swept:?}");
        };
        // A column of 400 pixels: the square of the box's 20 in its middle.
        assert_eq!(
            sweep.rect,
            Rect {
                x: 190,
                y: 0,
                width: 20,
                height: 20
            }
        );
        assert_eq!(sweep.shape, SweepShape::Dots { diameter, centers });
        assert_eq!(sweep.period, DOTS_PERIOD);
        assert_eq!(sweep.radius, 0.0);
        assert_eq!(sweep.color, token("progress.normal"));
        assert!(fills(&tree, &layout, &tokens, None).is_empty());
    }

    #[test]
    fn the_fills_of_bars_rings_and_outlines_go_as_far_as_their_values() {
        let bar = of(Progress {
            value: 0.25,
            ..Progress::default()
        });
        let ring = of(Progress {
            state: ProgressState::Error,
            value: 0.5,
            shape: ProgressShape::Ring,
            thickness: 3.0,
            ..Progress::default()
        })
        .layout(view::Layout {
            height: Length::Points(20.0),
            ..view::Layout::default()
        });
        let outline = outlined(Progress {
            state: ProgressState::Paused,
            value: 1.0,
            ..Progress::default()
        });
        let spinning = of(Progress {
            state: ProgressState::Indeterminate,
            ..Progress::default()
        });
        let (tree, layout) = laid(vec![bar, ring, outline, spinning], 2.0);
        let tokens = Theme::builtin().dark.tokens;
        let fills = super::fills(&tree, &layout, &tokens, None);
        let root = tree.root().unwrap();
        let children = tree.children(root);
        assert_eq!(fills.len(), 3, "{fills:?}");
        assert_eq!(fills[0].node, children[0]);
        assert_eq!(fills[0].shape, FillShape::Bar);
        assert_eq!(fills[0].value, 0.25);
        assert_eq!(fills[0].radius, 4.0);
        assert_eq!(fills[0].color, token("progress.normal"));
        assert_eq!(fills[1].shape, FillShape::Ring { thickness: 6.0 });
        assert_eq!((fills[1].rect.width, fills[1].rect.height), (40, 40));
        assert_eq!(fills[1].value, 0.5);
        assert_eq!(fills[1].color, token("progress.error"));
        // The outline's of its parent's box and corners.
        assert_eq!(fills[2].node, tree.children(children[2])[1]);
        assert_eq!(fills[2].shape, FillShape::Outline { thickness: 4.0 });
        assert_eq!((fills[2].rect.width, fills[2].rect.height), (200, 48));
        assert_eq!(fills[2].radius, 12.0);
        assert_eq!(fills[2].value, 1.0);
        assert_eq!(fills[2].color, token("progress.paused"));
    }
}

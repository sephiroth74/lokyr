//! The progress bars and rings: their natural sizes, the room where a ring
//! goes in its box, and the sweeps of the indeterminate ones, which the host
//! hands the platform, whose window server moves them without the app's
//! frames: a segment across a bar, an arc round a ring, each within the
//! clips of the nodes around it.

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

/// How long an indeterminate bar's sweep takes, and a ring's turn, of a
/// progress that gives none.
pub const PERIODS: (Duration, Duration) = (Duration::from_millis(1500), Duration::from_secs(1));

/// The part of an indeterminate progress that the window server moves: its
/// box, in pixels from the tree's corner, the part of it within its clips,
/// its corners' radius, its color, its shape and how long a sweep takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepArea {
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
}

/// The natural size of a progress of `shape`, in points.
pub fn natural(shape: ProgressShape) -> (f32, f32) {
    match shape {
        ProgressShape::Bar => BAR,
        ProgressShape::Ring => (RING, RING),
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

/// The radius of the corners of a bar of `progress`, `height` pixels high,
/// in pixels at `scale`: its own, or a pill's.
pub fn radius(progress: &Progress, height: f32, scale: f32) -> f32 {
    progress
        .radius
        .map_or(height / 2.0, |radius| radius * scale)
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
    let mut sweeps = Vec::new();
    if let Some(root) = tree.root() {
        let shown = Shown {
            clip: None,
            offset: (0, 0),
            opacity: 1.0,
        };
        walk((tree, layout, tokens, motion), root, shown, &mut sweeps);
    }
    sweeps
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
    sweeps: &mut Vec<SweepArea>,
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
                },
                Duration::from_secs_f32,
            );
            sweeps.push(SweepArea {
                rect: pixels,
                visible,
                radius: radius(progress, frame.height, scale),
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
            sweeps,
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
}

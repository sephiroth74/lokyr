//! The view tree's layout, of `taffy`'s flexbox
//! (<https://www.w3.org/TR/css-flexbox-1/>): a container's direction, gap,
//! padding and alignments, and a node's sizes and grow, in the display's
//! pixels at its scale, so that the edges fall on whole pixels; the texts
//! and the icons sized by a [`Measure`], in points, as the interface's
//! font draws them at their size of points, which the scale multiplies.
//!
//! The layout keeps `taffy`'s tree beside the view tree, which the view
//! tree's changes update: a change dirties its node and its ancestors,
//! which the next layout lays out again, `taffy` keeping the others'
//! layouts; and each text and icon keeps its measure until it changes.

use std::collections::HashMap;

use lokyr_plugin_sdk::view::{self, Align, Direction, Justify, Kind, Length};
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, FlexDirection, JustifyContent, LengthPercentage, Rect,
    Size, Style, TaffyTree,
};

use crate::tree::{Changes, NodeId, Tree};

/// What sizes the leaves of the view tree that take their content's size.
pub trait Measure {
    /// The size of `text`, on one line, in points, its width and height.
    fn text(&mut self, text: &str) -> (f32, f32);

    /// The size of the icon `name`, in points.
    fn icon(&mut self, name: &str) -> (f32, f32);
}

/// A node's place and size, in pixels, from the root's top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame {
    /// From the root's left edge.
    pub x: f32,
    /// From the root's top edge.
    pub y: f32,
    /// Its width.
    pub width: f32,
    /// Its height.
    pub height: f32,
}

/// A leaf that takes its content's size: its content, and its size in
/// points once measured, which a layout keeps until the content changes.
#[derive(Clone, Debug, PartialEq)]
struct Leaf {
    content: Content,
    measured: Option<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
enum Content {
    Text(String),
    Icon(String),
}

/// The layout of a view tree.
pub struct Layout {
    taffy: TaffyTree<Leaf>,
    nodes: HashMap<NodeId, taffy::NodeId>,
    scale: f32,
}

impl Layout {
    /// An empty layout, at `scale` pixels a point.
    pub fn new(scale: f32) -> Self {
        Self {
            taffy: TaffyTree::new(),
            nodes: HashMap::new(),
            scale,
        }
    }

    /// Takes `changes`, of `tree`'s update: the nodes added and taken, the
    /// properties changed and the children rearranged, each node changed
    /// dirty for the next layout, with its ancestors.
    pub fn update(&mut self, tree: &Tree, changes: &Changes) {
        for id in &changes.removed {
            if let Some(node) = self.nodes.remove(id) {
                let _ = self.taffy.remove(node);
            }
        }
        for &id in &changes.added {
            let (Some(kind), Some(layout)) = (tree.kind(id), tree.layout(id)) else {
                continue;
            };
            let style = style(layout, self.scale);
            let made = match leaf(kind) {
                Some(leaf) => self.taffy.new_leaf_with_context(style, leaf),
                None => self.taffy.new_leaf(style),
            };
            if let Ok(node) = made {
                self.nodes.insert(id, node);
            }
        }
        for &id in &changes.changed {
            let (Some(&node), Some(kind), Some(layout)) =
                (self.nodes.get(&id), tree.kind(id), tree.layout(id))
            else {
                continue;
            };
            let _ = self.taffy.set_style(node, style(layout, self.scale));
            let leaf = leaf(kind);
            let kept = self.taffy.get_node_context(node).map(|kept| &kept.content);
            if kept != leaf.as_ref().map(|leaf| &leaf.content) {
                let _ = self.taffy.set_node_context(node, leaf);
            }
        }
        for &id in changes.added.iter().chain(&changes.rearranged) {
            self.set_children(tree, id);
        }
    }

    /// Measures every text and icon again at the next layout, as when the
    /// font that a [`Measure`] measures of changed.
    pub fn measure_again(&mut self) {
        for &node in self.nodes.values() {
            if let Some(leaf) = self.taffy.get_node_context_mut(node) {
                leaf.measured = None;
                let _ = self.taffy.mark_dirty(node);
            }
        }
    }

    /// Lays out `tree` again at `scale` pixels a point, every node's style
    /// of the new scale.
    pub fn set_scale(&mut self, tree: &Tree, scale: f32) {
        self.scale = scale;
        for (&id, &node) in &self.nodes {
            if let Some(layout) = tree.layout(id) {
                let _ = self.taffy.set_style(node, style(layout, scale));
            }
        }
    }

    /// Lays out `tree` in `width` and `height` pixels, or of its content's
    /// size where either is `None`, its leaves sized by `measure`: only the
    /// nodes dirty since the last layout, and their ancestors, and only the
    /// texts and icons new or changed measured.
    pub fn compute(
        &mut self,
        tree: &Tree,
        width: Option<f32>,
        height: Option<f32>,
        measure: &mut impl Measure,
    ) {
        let Some((id, layout)) = tree.root().and_then(|id| Some((id, tree.layout(id)?))) else {
            return;
        };
        let Some(&root) = self.nodes.get(&id) else {
            return;
        };
        // The root takes the room that it is given, where it is given.
        let mut style = style(layout, self.scale);
        if let Some(width) = width {
            style.size.width = Dimension::length(width);
        }
        if let Some(height) = height {
            style.size.height = Dimension::length(height);
        }
        if self.taffy.style(root).is_ok_and(|kept| *kept != style) {
            let _ = self.taffy.set_style(root, style);
        }
        let available = |length: Option<f32>| {
            length.map_or(AvailableSpace::MaxContent, AvailableSpace::Definite)
        };
        let scale = self.scale;
        let _ = self.taffy.compute_layout_with_measure(
            root,
            Size {
                width: available(width),
                height: available(height),
            },
            |inputs, _, context, style| {
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, _| {
                        let (width, height) = context.map_or((0.0, 0.0), |leaf| {
                            *leaf.measured.get_or_insert_with(|| match &leaf.content {
                                Content::Text(text) => measure.text(text),
                                Content::Icon(name) => measure.icon(name),
                            })
                        });
                        Size {
                            width: known.width.unwrap_or(width * scale),
                            height: known.height.unwrap_or(height * scale),
                        }
                    },
                )
            },
        );
    }

    /// The frame of `id`, of the last layout, from the root's top left
    /// corner, in pixels.
    pub fn frame(&self, tree: &Tree, id: NodeId) -> Option<Frame> {
        let laid = self.taffy.layout(*self.nodes.get(&id)?).ok()?;
        let mut frame = Frame {
            x: laid.location.x,
            y: laid.location.y,
            width: laid.size.width,
            height: laid.size.height,
        };
        let mut parent = tree.parent(id);
        while let Some(above) = parent {
            let laid = self.taffy.layout(*self.nodes.get(&above)?).ok()?;
            frame.x += laid.location.x;
            frame.y += laid.location.y;
            parent = tree.parent(above);
        }
        Some(frame)
    }

    fn set_children(&mut self, tree: &Tree, id: NodeId) {
        if !tree.kind(id).is_some_and(Kind::has_children) {
            return;
        }
        let Some(&node) = self.nodes.get(&id) else {
            return;
        };
        let children: Vec<taffy::NodeId> = tree
            .children(id)
            .iter()
            .filter_map(|child| self.nodes.get(child).copied())
            .collect();
        let _ = self.taffy.set_children(node, &children);
    }
}

/// What a node of `kind` holds to be measured, if its content sizes it.
fn leaf(kind: &Kind) -> Option<Leaf> {
    let content = match kind {
        Kind::Text(text) => Content::Text(text.clone()),
        Kind::Icon(name) => Content::Icon(name.clone()),
        Kind::Container | Kind::Spacer => return None,
    };
    Some(Leaf {
        content,
        measured: None,
    })
}

/// `taffy`'s style of `layout`, in pixels at `scale`.
fn style(layout: &view::Layout, scale: f32) -> Style {
    let pixels = |points: f32| LengthPercentage::length(points * scale);
    let length = |length: Length| match length {
        Length::Auto => Dimension::auto(),
        Length::Points(points) => Dimension::length(points * scale),
        Length::Fraction(fraction) => Dimension::percent(fraction),
    };
    Style {
        flex_direction: match layout.direction {
            Direction::Row => FlexDirection::Row,
            Direction::Column => FlexDirection::Column,
        },
        gap: Size {
            width: pixels(layout.gap),
            height: pixels(layout.gap),
        },
        padding: Rect {
            left: pixels(layout.padding.left),
            right: pixels(layout.padding.right),
            top: pixels(layout.padding.top),
            bottom: pixels(layout.padding.bottom),
        },
        align_items: match layout.align {
            Align::Start => AlignItems::FLEX_START,
            Align::Center => AlignItems::CENTER,
            Align::End => AlignItems::FLEX_END,
            Align::Stretch => AlignItems::STRETCH,
        },
        justify_content: match layout.justify {
            Justify::Start => JustifyContent::FLEX_START,
            Justify::Center => JustifyContent::CENTER,
            Justify::End => JustifyContent::FLEX_END,
            Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
        },
        size: Size {
            width: length(layout.width),
            height: length(layout.height),
        },
        flex_grow: layout.grow,
        ..Style::default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lokyr_plugin_sdk::view::{Edges, Node};

    use super::*;

    /// A font of the tests': a text 6 points a character wide and 14 high,
    /// an icon of 16, which counts what it measures.
    #[derive(Default)]
    struct Fixed {
        measured: BTreeMap<String, usize>,
    }

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            *self.measured.entry(text.to_owned()).or_default() += 1;
            (6.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, name: &str) -> (f32, f32) {
            *self.measured.entry(name.to_owned()).or_default() += 1;
            (16.0, 16.0)
        }
    }

    /// A tree laid out at `scale`, of `width` pixels.
    fn laid(root: &Node, scale: f32, width: f32) -> (Tree, Layout, Fixed) {
        let mut tree = Tree::new();
        let changes = tree.update(root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        let mut font = Fixed::default();
        layout.compute(&tree, Some(width), None, &mut font);
        (tree, layout, font)
    }

    /// A bar: an icon, a title, room, a status, 4 points apart, of 2 of
    /// padding, the items in the middle.
    fn bar(title: &str) -> Node {
        Node::container(vec![
            Node::icon("terminal"),
            Node::text(title).key("title"),
            Node::spacer(),
            Node::text("ready").key("status"),
        ])
        .layout(view::Layout {
            gap: 4.0,
            padding: Edges::all(2.0),
            align: Align::Center,
            ..view::Layout::default()
        })
    }

    fn frames(tree: &Tree, layout: &Layout) -> Vec<Frame> {
        let root = tree.root().unwrap();
        tree.children(root)
            .iter()
            .map(|&child| layout.frame(tree, child).unwrap())
            .collect()
    }

    #[test]
    fn a_row_lays_out_its_icon_and_texts_with_room_between() {
        let (tree, layout, _) = laid(&bar("~"), 1.0, 200.0);
        let root = layout.frame(&tree, tree.root().unwrap()).unwrap();
        // 200 wide, as tall as the icon and the padding.
        assert_eq!((root.width, root.height), (200.0, 20.0));
        let [icon, title, spacer, status] = frames(&tree, &layout).try_into().unwrap();
        assert_eq!((icon.x, icon.y, icon.width), (2.0, 2.0, 16.0));
        // After the icon and the gap, a character wide, in the middle.
        assert_eq!(
            (title.x, title.y, title.width, title.height),
            (22.0, 3.0, 6.0, 14.0)
        );
        // The status at the right, past the room.
        assert_eq!(status.x + status.width, 198.0);
        assert_eq!(spacer.x, title.x + title.width + 4.0);
        assert_eq!(spacer.x + spacer.width + 4.0, status.x);
    }

    #[test]
    fn a_column_stacks_its_children_and_twice_the_scale_doubles_it() {
        let column = Node::container(vec![
            Node::text("one"),
            Node::text("three"),
            Node::icon("x"),
        ])
        .layout(view::Layout {
            direction: Direction::Column,
            gap: 2.0,
            align: Align::Start,
            ..view::Layout::default()
        });
        let (tree, layout, _) = laid(&column, 1.0, 100.0);
        let ones = frames(&tree, &layout);
        assert_eq!(
            ones.iter()
                .map(|frame| (frame.y, frame.width))
                .collect::<Vec<_>>(),
            [(0.0, 18.0), (16.0, 30.0), (32.0, 16.0)]
        );
        // At 2 pixels a point, every place and size twice.
        let (tree, layout, _) = laid(&column, 2.0, 200.0);
        let twos = frames(&tree, &layout);
        for (one, two) in ones.iter().zip(&twos) {
            assert_eq!(
                (two.x, two.y, two.width, two.height),
                (one.x * 2.0, one.y * 2.0, one.width * 2.0, one.height * 2.0)
            );
        }
        // A bar too, at both scales.
        let (tree, layout, _) = laid(&bar("~/work"), 2.0, 400.0);
        let [icon, title, ..]: [Frame; 4] = frames(&tree, &layout).try_into().unwrap();
        assert_eq!(
            (icon.x, icon.width, title.x, title.width),
            (4.0, 32.0, 44.0, 72.0)
        );
    }

    #[test]
    fn a_change_measures_its_node_alone_again() {
        let (mut tree, mut layout, mut font) = laid(&bar("~"), 1.0, 200.0);
        let before = frames(&tree, &layout);
        font.measured.clear();
        // The title longer: it alone is measured again.
        let changes = tree.update(&bar("~/Projects")).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert_eq!(font.measured.keys().collect::<Vec<_>>(), ["~/Projects"]);
        let after = frames(&tree, &layout);
        // It grew by 9 characters, the room shrank by as much; the icon and
        // the status kept their places.
        assert_eq!(after[1].width, before[1].width + 54.0);
        assert_eq!(after[2].width, before[2].width - 54.0);
        assert_eq!((after[0], after[3]), (before[0], before[3]));
        // Nothing changed: nothing measured.
        font.measured.clear();
        let changes = tree.update(&bar("~/Projects")).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert!(font.measured.is_empty());
        // The status's padding alone: laid out again, of its measure kept.
        let mut padded = bar("~/Projects");
        padded.children[3].layout.padding = Edges::all(1.0);
        let changes = tree.update(&padded).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert!(font.measured.is_empty());
        let status = layout.frame(&tree, tree.children(tree.root().unwrap())[3]);
        assert_eq!(
            status.map(|status| status.width),
            Some(after[3].width + 2.0)
        );
    }

    #[test]
    fn a_new_scale_lays_out_again_of_the_measures_kept() {
        let (tree, mut layout, mut font) = laid(&bar("~/work"), 1.0, 200.0);
        let ones = frames(&tree, &layout);
        font.measured.clear();
        layout.set_scale(&tree, 2.0);
        layout.compute(&tree, Some(400.0), None, &mut font);
        assert!(font.measured.is_empty());
        for (one, two) in ones.iter().zip(frames(&tree, &layout)) {
            assert_eq!((two.x, two.width), (one.x * 2.0, one.width * 2.0));
        }
        // Another font: every text and icon measured again.
        layout.measure_again();
        layout.compute(&tree, Some(400.0), None, &mut font);
        assert_eq!(
            font.measured.keys().collect::<Vec<_>>(),
            ["ready", "terminal", "~/work"]
        );
    }

    #[test]
    fn nodes_added_and_taken_are_laid_out_and_forgotten() {
        let (mut tree, mut layout, mut font) = laid(&bar("~"), 1.0, 200.0);
        let root = tree.root().unwrap();
        let mut more = bar("~");
        more.children.insert(1, Node::text("·").key("dot"));
        let changes = tree.update(&more).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        let dot = tree.children(root)[1];
        let title = tree.children(root)[2];
        assert_eq!(layout.frame(&tree, dot).unwrap().x, 22.0);
        assert_eq!(layout.frame(&tree, title).unwrap().x, 32.0);
        // Taken away, it has no frame, and the title is back in its place.
        let changes = tree.update(&bar("~")).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert_eq!(layout.frame(&tree, dot), None);
        assert_eq!(layout.frame(&tree, title).unwrap().x, 22.0);
    }

    #[test]
    fn a_nested_node_s_frame_is_from_the_root() {
        let column = Node::container(vec![Node::text("top"), bar("~")]).layout(view::Layout {
            direction: Direction::Column,
            padding: Edges::all(10.0),
            gap: 6.0,
            ..view::Layout::default()
        });
        let (tree, layout, _) = laid(&column, 2.0, 400.0);
        let root = tree.root().unwrap();
        let inner = tree.children(root)[1];
        let title = tree.children(inner)[1];
        // Below the padding, the first line and the gap; past the bar's
        // padding, its icon and its gap.
        let frame = layout.frame(&tree, title).unwrap();
        assert_eq!((frame.x, frame.y), (20.0 + 44.0, 20.0 + 28.0 + 12.0 + 6.0));
    }

    #[test]
    fn sizes_of_points_and_of_fractions_and_grow() {
        let row = Node::container(vec![
            Node::container(Vec::new()).layout(view::Layout {
                width: Length::Points(50.0),
                ..view::Layout::default()
            }),
            Node::container(Vec::new()).layout(view::Layout {
                width: Length::Fraction(0.25),
                ..view::Layout::default()
            }),
            Node::container(Vec::new()).layout(view::Layout {
                grow: 1.0,
                ..view::Layout::default()
            }),
        ]);
        let (tree, layout, _) = laid(&row, 2.0, 400.0);
        let widths: Vec<f32> = frames(&tree, &layout)
            .iter()
            .map(|frame| frame.width)
            .collect();
        // 50 points at 2, a quarter of 400, and the rest.
        assert_eq!(widths, [100.0, 100.0, 200.0]);
    }
}

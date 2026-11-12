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
//! It keeps how far each scroll area is scrolled, which moves its
//! children's frames without a layout; and the control of each view of
//! tabs, a leaf of `taffy`'s before its page, of its tabs' segments.

use std::collections::HashMap;

use lokyr_plugin_sdk::view::{
    self, Align, Direction, Distribution, Justify, Kind, Length, ProgressShape,
};
use taffy::Overflow;
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, FlexDirection, JustifyContent, LengthPercentage,
    LengthPercentageAuto, Position, Rect, Size, Style, TaffyTree,
};

use crate::tree::{Changes, NodeId, Tree, Typeset};
use crate::{field, progress, tabs, widget};

/// What sizes the leaves of the view tree that take their content's size.
pub trait Measure {
    /// Takes `typeset` for the texts that it measures next, of a node whose
    /// style or whose parent's names a size or a weight; one that knows a
    /// single face does nothing.
    fn typeset(&mut self, _typeset: Typeset) {}

    /// The size of `text`, on one line, in points, its width and height.
    fn text(&mut self, text: &str) -> (f32, f32);

    /// The lines of `text` broken between its words within `room` points,
    /// as many as it needs, each of the most words that fit, and whether a
    /// word of it is wider than a line of its own, which its line cuts: as
    /// [`crate::font::Font::interface_lines`] breaks it, of the widths that
    /// [`Measure::text`] measures. A text of no word is one line.
    fn lines(&mut self, text: &str, room: f32) -> (usize, bool) {
        let words: Vec<&str> = text.split_whitespace().collect();
        let (mut lines, mut wide) = (0, false);
        let mut at = 0;
        while at < words.len() {
            let mut line = words[at].to_owned();
            wide |= self.text(&line).0 > room;
            at += 1;
            while at < words.len() {
                let longer = format!("{line} {}", words[at]);
                if self.text(&longer).0 > room {
                    break;
                }
                line = longer;
                at += 1;
            }
            lines += 1;
        }
        (lines.max(1), wide)
    }

    /// The size of the icon `name`, in points.
    fn icon(&mut self, name: &str) -> (f32, f32);

    /// The natural size of a text field, in points: of its width, and of a
    /// line of text within its padding.
    fn field(&mut self) -> (f32, f32) {
        let (_, line) = self.text("");
        (field::WIDTH, line + 2.0 * field::PADDING.0)
    }

    /// The size of a button or a toggle of `label` and `icon`, either
    /// empty, in points: its padding around them, side by side.
    fn button(&mut self, label: &str, icon: &str) -> (f32, f32) {
        let text = if label.is_empty() {
            (0.0, 0.0)
        } else {
            self.text(label)
        };
        let glyph = if icon.is_empty() {
            (0.0, 0.0)
        } else {
            self.icon(icon)
        };
        widget::size(label, icon, (text, glyph))
    }

    /// The size of a separator, in points, a point thick: its parent
    /// stretches it across.
    fn separator(&mut self) -> (f32, f32) {
        (1.0, 1.0)
    }
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
    /// How its text is set, of its style or its parents'.
    typeset: Typeset,
    measured: Option<(f32, f32)>,
    /// The widths of its parts, in points, once measured: a control's
    /// segments'.
    parts: Vec<f32>,
    /// A text's lines at the room of points whose bits it holds, once
    /// counted, and whether a word of it is wider than a line, as
    /// [`Measure::lines`] counts them.
    wrapped: Option<(u32, (usize, bool))>,
}

#[derive(Clone, Debug, PartialEq)]
enum Content {
    /// A text, and the most lines that it takes, as its layout's `lines`.
    Text(String, Option<u16>),
    Icon(String),
    Field,
    Button {
        label: String,
        icon: String,
    },
    Separator,
    Progress(ProgressShape),
    /// A view of tabs' control, of each tab's label and icon.
    Segments(Vec<(String, String)>),
}

/// The room of a window's titlebar that its buttons take, in pixels, which
/// a view's lengths of the titlebar and of the buttons give.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Room {
    /// The titlebar's height; 0 without one.
    pub titlebar: f32,
    /// The width that the buttons take at the titlebar's left; 0 without
    /// them.
    pub buttons: f32,
}

/// The layout of a view tree.
pub struct Layout {
    taffy: TaffyTree<Leaf>,
    nodes: HashMap<NodeId, taffy::NodeId>,
    scale: f32,
    room: Room,
    /// How far each scroll area is scrolled along its direction, down a
    /// column or across a row, in pixels.
    scrolled: HashMap<NodeId, f32>,
    /// The control of each view of tabs.
    controls: HashMap<NodeId, taffy::NodeId>,
}

impl Layout {
    /// An empty layout, at `scale` pixels a point.
    pub fn new(scale: f32) -> Self {
        Self {
            taffy: TaffyTree::new(),
            nodes: HashMap::new(),
            scale,
            room: Room::default(),
            scrolled: HashMap::new(),
            controls: HashMap::new(),
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
            if let Some(control) = self.controls.remove(id) {
                let _ = self.taffy.remove(control);
            }
            self.scrolled.remove(id);
        }
        let mut controls = Vec::new();
        for &id in &changes.added {
            let (Some(kind), Some(layout)) = (tree.kind(id), tree.layout(id)) else {
                continue;
            };
            let style = style_in(tree, id, style(kind, layout, self.scale, self.room));
            let made = match leaf(kind, layout, tree.typeset(id)) {
                Some(leaf) => self.taffy.new_leaf_with_context(style, leaf),
                None => self.taffy.new_leaf(style),
            };
            if let Ok(node) = made {
                self.nodes.insert(id, node);
            }
            self.keep_control(tree, id);
        }
        for &id in &changes.changed {
            let (Some(&node), Some(kind), Some(layout)) =
                (self.nodes.get(&id), tree.kind(id), tree.layout(id))
            else {
                continue;
            };
            let _ = self.taffy.set_style(
                node,
                style_in(tree, id, style(kind, layout, self.scale, self.room)),
            );
            let leaf = leaf(kind, layout, tree.typeset(id));
            let kept = self
                .taffy
                .get_node_context(node)
                .map(|kept| (&kept.content, kept.typeset));
            if kept != leaf.as_ref().map(|leaf| (&leaf.content, leaf.typeset)) {
                let _ = self.taffy.set_node_context(node, leaf);
            }
            if self.keep_control(tree, id) {
                controls.push(id);
            }
        }
        // The children of a scroll area that changed take its direction.
        for &id in &changes.changed {
            if !matches!(tree.kind(id), Some(Kind::Scroll)) {
                continue;
            }
            for &child in tree.children(id) {
                let (Some(&node), Some(kind), Some(layout)) =
                    (self.nodes.get(&child), tree.kind(child), tree.layout(child))
                else {
                    continue;
                };
                let _ = self.taffy.set_style(
                    node,
                    style_in(tree, child, style(kind, layout, self.scale, self.room)),
                );
            }
        }
        for &id in changes
            .added
            .iter()
            .chain(&changes.rearranged)
            .chain(&controls)
        {
            self.set_children(tree, id);
        }
        // A node whose style changed may change its texts' size, and its
        // children's that name none.
        for &id in &changes.restyled {
            self.size_texts(tree, id);
        }
    }

    /// Takes the size of the texts of `id` of `tree` and of its subtree's,
    /// measuring again those whose size changed.
    fn size_texts(&mut self, tree: &Tree, id: NodeId) {
        let mut left = vec![id];
        while let Some(at) = left.pop() {
            left.extend_from_slice(tree.children(at));
            let Some(&node) = self.nodes.get(&at) else {
                continue;
            };
            let typeset = tree.typeset(at);
            if let Some(leaf) = self.taffy.get_node_context_mut(node)
                && leaf.typeset != typeset
            {
                leaf.typeset = typeset;
                leaf.measured = None;
                leaf.wrapped = None;
                let _ = self.taffy.mark_dirty(node);
            }
        }
    }

    /// Keeps the control of `id` of `tree`, a view of tabs, of its tabs'
    /// labels and icons, measured anew as they change; or takes it away, of
    /// another node. Whether it came or went, which changes the node's
    /// children in `taffy`'s tree.
    fn keep_control(&mut self, tree: &Tree, id: NodeId) -> bool {
        let Some(Kind::Tabs(view)) = tree.kind(id) else {
            return self
                .controls
                .remove(&id)
                .is_some_and(|control| self.taffy.remove(control).is_ok());
        };
        let content = Content::Segments(
            view.tabs
                .iter()
                .map(|tab| (tab.label.clone(), tab.icon.clone()))
                .collect(),
        );
        let style = control_style(self.scale, view.distribution);
        if let Some(&control) = self.controls.get(&id) {
            if self.taffy.style(control).is_ok_and(|kept| *kept != style) {
                let _ = self.taffy.set_style(control, style);
            }
            let kept = self
                .taffy
                .get_node_context(control)
                .map(|kept| &kept.content);
            if kept != Some(&content) {
                let _ = self.taffy.set_node_context(
                    control,
                    Some(Leaf {
                        content,
                        typeset: Typeset::default(),
                        measured: None,
                        parts: Vec::new(),
                        wrapped: None,
                    }),
                );
            }
            return false;
        }
        // A segmented control's labels keep the interface's size, as
        // macOS's do.
        let leaf = Leaf {
            content,
            typeset: Typeset::default(),
            measured: None,
            parts: Vec::new(),
            wrapped: None,
        };
        match self.taffy.new_leaf_with_context(style, leaf) {
            Ok(control) => {
                self.controls.insert(id, control);
                true
            }
            Err(_) => false,
        }
    }

    /// The frame of the control of the view of tabs `id` of `tree`, of the
    /// last layout, as [`Layout::frame`] gives frames.
    pub fn control(&self, tree: &Tree, id: NodeId) -> Option<Frame> {
        let view = self.frame(tree, id)?;
        let laid = self.taffy.layout(*self.controls.get(&id)?).ok()?;
        Some(Frame {
            x: view.x + laid.location.x,
            y: view.y + laid.location.y,
            width: laid.size.width,
            height: laid.size.height,
        })
    }

    /// The frames of the segments of the control of the view of tabs `id`
    /// of `tree`, side by side, of the last layout, as [`Layout::frame`]
    /// gives frames, of its distribution: each of its content's width, or
    /// of the control's, the view's, shared; none before its first layout.
    pub fn segments(&self, tree: &Tree, id: NodeId) -> Vec<Frame> {
        let (Some(Kind::Tabs(view)), Some(control), Some(&node)) = (
            tree.kind(id),
            self.control(tree, id),
            self.controls.get(&id),
        ) else {
            return Vec::new();
        };
        let Some(parts) = self
            .taffy
            .get_node_context(node)
            .map(|leaf| leaf.parts.as_slice())
        else {
            return Vec::new();
        };
        let natural: Vec<f32> = parts.iter().map(|points| points * self.scale).collect();
        let widths = tabs::widths(&natural, control.width, view.distribution);
        let mut x = control.x;
        widths
            .into_iter()
            .map(|width| {
                let segment = Frame {
                    x,
                    width,
                    ..control
                };
                x += width;
                segment
            })
            .collect()
    }

    /// Measures every text and icon again at the next layout, as when the
    /// font that a [`Measure`] measures of changed.
    pub fn measure_again(&mut self) {
        for &node in self.nodes.values().chain(self.controls.values()) {
            if let Some(leaf) = self.taffy.get_node_context_mut(node) {
                leaf.measured = None;
                leaf.wrapped = None;
                let _ = self.taffy.mark_dirty(node);
            }
        }
    }

    /// The pixels a point of the layout.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The window's titlebar's room, of `tree`'s lengths of the titlebar and
    /// of the buttons: every node's style anew as it changes.
    pub fn set_room(&mut self, tree: &Tree, room: Room) {
        if room == self.room {
            return;
        }
        self.room = room;
        for (&id, &node) in &self.nodes {
            if let (Some(kind), Some(layout)) = (tree.kind(id), tree.layout(id)) {
                let _ = self.taffy.set_style(
                    node,
                    style_in(tree, id, style(kind, layout, self.scale, room)),
                );
            }
        }
    }

    /// Lays out `tree` again at `scale` pixels a point, every node's style
    /// of the new scale, as its parent has it, and each scroll area as far
    /// scrolled in points as before.
    pub fn set_scale(&mut self, tree: &Tree, scale: f32) {
        if self.scale > 0.0 {
            let ratio = scale / self.scale;
            for scrolled in self.scrolled.values_mut() {
                *scrolled *= ratio;
            }
        }
        self.scale = scale;
        for (&id, &node) in &self.nodes {
            if let (Some(kind), Some(layout)) = (tree.kind(id), tree.layout(id)) {
                let _ = self.taffy.set_style(
                    node,
                    style_in(tree, id, style(kind, layout, scale, self.room)),
                );
            }
        }
        for (&id, &control) in &self.controls {
            if let Some(Kind::Tabs(view)) = tree.kind(id) {
                let _ = self
                    .taffy
                    .set_style(control, control_style(scale, view.distribution));
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
        let Some((id, kind, layout)) = tree
            .root()
            .and_then(|id| Some((id, tree.kind(id)?, tree.layout(id)?)))
        else {
            return;
        };
        let Some(&root) = self.nodes.get(&id) else {
            return;
        };
        // The root takes the room that it is given, where it is given.
        let mut style = style(kind, layout, self.scale, self.room);
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
                    |known, available| {
                        // The room across that a text of lines breaks in.
                        let room = known.width.or(match available.width {
                            AvailableSpace::Definite(room) => Some(room),
                            _ => None,
                        });
                        let (width, height) = context.map_or((0.0, 0.0), |leaf| {
                            if leaf.measured.is_none() {
                                measure.typeset(leaf.typeset);
                                let (size, parts) = measured(&leaf.content, measure);
                                measure.typeset(Typeset::default());
                                leaf.measured = Some(size);
                                leaf.parts = parts;
                            }
                            let (width, height) = leaf.measured.unwrap_or_default();
                            // A text of lines too long for its room breaks
                            // into it; another keeps its line, which its
                            // paint cuts.
                            match (room, wrapping(&leaf.content)) {
                                (Some(room), Some(most)) if width * scale > room => {
                                    let (lines, _) = wrapped(leaf, measure, room / scale);
                                    let shown = if most == 0 {
                                        lines
                                    } else {
                                        lines.min(usize::from(most))
                                    };
                                    (room / scale, height * shown as f32)
                                }
                                _ => (width, height),
                            }
                        });
                        Size {
                            width: known.width.unwrap_or(width * scale),
                            height: known.height.unwrap_or(height * scale),
                        }
                    },
                )
            },
        );
        self.clamp_scrolls(tree);
    }

    /// Keeps each scroll area of `tree` within its content, which a layout
    /// can shrink.
    fn clamp_scrolls(&mut self, tree: &Tree) {
        let ids: Vec<NodeId> = self.scrolled.keys().copied().collect();
        for id in ids {
            let room = self.scroll_room(tree, id);
            if let Some(scrolled) = self.scrolled.get_mut(&id) {
                *scrolled = scrolled.min(room);
            }
        }
    }

    /// The frame of `id`, of the last layout, from the root's top left
    /// corner, in pixels, moved up as far as the scroll areas around it
    /// are scrolled.
    pub fn frame(&self, tree: &Tree, id: NodeId) -> Option<Frame> {
        let mut frame = self.local(id)?;
        let mut parent = tree.parent(id);
        while let Some(above) = parent {
            let laid = self.local(above)?;
            let scrolled = self.scrolled(above);
            frame.x += laid.x;
            frame.y += laid.y;
            if across(tree, above) {
                frame.x -= scrolled;
            } else {
                frame.y -= scrolled;
            }
            parent = tree.parent(above);
        }
        Some(frame)
    }

    /// The frame of `id`, of the last layout, from the root's top left
    /// corner, in pixels, as the layout placed it, of no scroll: what a
    /// scroll does not move.
    pub fn placed(&self, tree: &Tree, id: NodeId) -> Option<Frame> {
        let mut frame = self.local(id)?;
        let mut parent = tree.parent(id);
        while let Some(above) = parent {
            let laid = self.local(above)?;
            frame.x += laid.x;
            frame.y += laid.y;
            parent = tree.parent(above);
        }
        Some(frame)
    }

    /// How far the scroll area `id` is scrolled along its direction, down
    /// a column or across a row, in pixels: 0 for another node.
    pub fn scrolled(&self, id: NodeId) -> f32 {
        self.scrolled.get(&id).copied().unwrap_or(0.0)
    }

    /// Scrolls the scroll area `id` of `tree` along its direction by
    /// `pixels`, down or rightwards, back where they are negative, within
    /// its content: whether it moved.
    pub fn scroll_by(&mut self, tree: &Tree, id: NodeId, pixels: f32) -> bool {
        let before = self.scrolled(id);
        let after = (before + pixels).clamp(0.0, self.scroll_room(tree, id));
        if after == before || !pixels.is_finite() {
            return false;
        }
        self.scrolled.insert(id, after);
        true
    }

    /// How far the scroll area `id` of `tree` can scroll along its
    /// direction, in pixels: its content's height, its children's and its
    /// padding's, past its own, or its content's width across a row.
    pub fn scroll_room(&self, tree: &Tree, id: NodeId) -> f32 {
        let (Some(frame), Some(layout)) = (self.local(id), tree.layout(id)) else {
            return 0.0;
        };
        let children = tree
            .children(id)
            .iter()
            .filter_map(|&child| self.local(child));
        if across(tree, id) {
            let right = children
                .map(|child| child.x + child.width)
                .fold(0.0, f32::max);
            return (right + layout.padding.right * self.scale - frame.width).max(0.0);
        }
        let bottom = children
            .map(|child| child.y + child.height)
            .fold(0.0, f32::max);
        (bottom + layout.padding.bottom * self.scale - frame.height).max(0.0)
    }

    /// Scrolls the nearest scroll area around `id` of `tree`, of the last
    /// layout, as little as shows the whole of `id`'s box, or its start
    /// where the area is smaller: whether it moved.
    pub fn reveal(&mut self, tree: &Tree, id: NodeId) -> bool {
        let area = std::iter::successors(tree.parent(id), |&above| tree.parent(above))
            .find(|&above| matches!(tree.kind(above), Some(Kind::Scroll)));
        let (Some(area), Some(node), Some(room)) = (
            area,
            self.placed(tree, id),
            area.and_then(|area| self.placed(tree, area)),
        ) else {
            return false;
        };
        let (start, size, view) = if across(tree, area) {
            (node.x - room.x, node.width, room.width)
        } else {
            (node.y - room.y, node.height, room.height)
        };
        let scrolled = self.scrolled(area);
        let wanted = if start < scrolled || size > view {
            start
        } else if start + size > scrolled + view {
            start + size - view
        } else {
            return false;
        };
        self.scroll_by(tree, area, wanted - scrolled)
    }

    /// Whether each text of lines of the last layout, by its node, is cut,
    /// of more lines than its most or of a word wider than its frame, its
    /// lines counted by `measure` within its frame's width, and a pixel
    /// more, as the layout's rounding of edges takes up to one from it, as
    /// its paint breaks it.
    pub fn cuts(&mut self, measure: &mut impl Measure) -> Vec<(NodeId, bool)> {
        let scale = self.scale;
        let mut cuts = Vec::new();
        for (&id, &node) in &self.nodes {
            let Some(width) = self.taffy.layout(node).ok().map(|laid| laid.size.width) else {
                continue;
            };
            let Some(leaf) = self.taffy.get_node_context_mut(node) else {
                continue;
            };
            let Content::Text(_, Some(most)) = leaf.content else {
                continue;
            };
            let (lines, wide) = wrapped(leaf, measure, (width + 1.0) / scale);
            cuts.push((id, wide || (most > 0 && lines > usize::from(most))));
        }
        cuts.sort_unstable_by_key(|&(id, _)| id);
        cuts
    }

    /// The size of `id`'s text or icon, in points, as the last layout
    /// measured it: its natural size, whatever room its frame takes.
    pub(crate) fn measured(&self, id: NodeId) -> Option<(f32, f32)> {
        self.taffy.get_node_context(*self.nodes.get(&id)?)?.measured
    }

    /// The frame of `id`, of the last layout, from its parent's top left
    /// corner, in pixels.
    pub(crate) fn local(&self, id: NodeId) -> Option<Frame> {
        let laid = self.taffy.layout(*self.nodes.get(&id)?).ok()?;
        Some(Frame {
            x: laid.location.x,
            y: laid.location.y,
            width: laid.size.width,
            height: laid.size.height,
        })
    }

    fn set_children(&mut self, tree: &Tree, id: NodeId) {
        if !tree.kind(id).is_some_and(Kind::has_children) {
            return;
        }
        let Some(mut node) = self.nodes.get(&id).copied() else {
            return;
        };
        // A node that loses its last child is laid out as a leaf, which
        // keeps the out-of-flow boxes that it held, gone now, among those
        // that its layout's rounding visits: it takes a node of its own
        // anew, of none.
        let childless = tree.children(id).is_empty() && !self.controls.contains_key(&id);
        if childless
            && self
                .taffy
                .hoisted_children(node)
                .is_ok_and(|held| !held.is_empty())
        {
            node = self.renew(id, node);
        }
        // A view of tabs' control first, over its page.
        let children: Vec<taffy::NodeId> = self
            .controls
            .get(&id)
            .copied()
            .into_iter()
            .chain(
                tree.children(id)
                    .iter()
                    .filter_map(|child| self.nodes.get(child).copied()),
            )
            .collect();
        let _ = self.taffy.set_children(node, &children);
    }
}

impl Layout {
    /// Gives `id` a node of `taffy`'s anew in place of `node`, of its style
    /// and its content, at its place among its parent's children: the new
    /// node.
    fn renew(&mut self, id: NodeId, node: taffy::NodeId) -> taffy::NodeId {
        let style = self.taffy.style(node).cloned().unwrap_or_default();
        let made = match self.taffy.get_node_context(node).cloned() {
            Some(leaf) => self.taffy.new_leaf_with_context(style, leaf),
            None => self.taffy.new_leaf(style),
        };
        let Ok(fresh) = made else {
            return node;
        };
        if let Some(parent) = self.taffy.parent(node) {
            let at = self
                .taffy
                .children(parent)
                .ok()
                .and_then(|children| children.iter().position(|&child| child == node));
            if let Some(at) = at {
                let _ = self.taffy.replace_child_at_index(parent, at, fresh);
            }
        }
        let _ = self.taffy.remove(node);
        self.nodes.insert(id, fresh);
        fresh
    }
}

/// What a node of `kind` holds to be measured, if its content sizes it.
fn leaf(kind: &Kind, layout: &view::Layout, typeset: Typeset) -> Option<Leaf> {
    let content = match kind {
        Kind::Text(text) => Content::Text(text.clone(), layout.lines),
        Kind::Icon(name) => Content::Icon(name.clone()),
        Kind::Field(_) => Content::Field,
        Kind::Button(button) => Content::Button {
            label: button.label.clone(),
            icon: button.icon.clone(),
        },
        Kind::Toggle(toggle) => Content::Button {
            label: toggle.label.clone(),
            icon: toggle.icon.clone(),
        },
        Kind::Separator => Content::Separator,
        Kind::Progress(progress) => Content::Progress(progress.shape),
        Kind::Container
        | Kind::Spacer
        | Kind::List(_)
        | Kind::Scroll
        | Kind::Tabs(_)
        | Kind::Image(_) => {
            return None;
        }
    };
    Some(Leaf {
        content,
        typeset,
        measured: None,
        parts: Vec::new(),
        wrapped: None,
    })
}

/// The lines of `leaf`'s text, as [`Measure::lines`] counts them within
/// `room` points, once for each room, and whether a word of it is wider
/// than a line; one line of no text.
fn wrapped(leaf: &mut Leaf, measure: &mut impl Measure, room: f32) -> (usize, bool) {
    let Content::Text(text, _) = &leaf.content else {
        return (1, false);
    };
    match leaf.wrapped {
        Some((bits, counted)) if bits == room.to_bits() => counted,
        _ => {
            measure.typeset(leaf.typeset);
            let counted = measure.lines(text, room);
            measure.typeset(Typeset::default());
            leaf.wrapped = Some((room.to_bits(), counted));
            counted
        }
    }
}

/// The most lines of `content`, a text that breaks into lines, 0 for as
/// many as it needs; `None` of a text of one line, as by default, and of
/// another content.
fn wrapping(content: &Content) -> Option<u16> {
    match *content {
        Content::Text(_, Some(most)) if most != 1 => Some(most),
        _ => None,
    }
}

/// The size of `content`, in points, as `measure` measures it, and the
/// widths of its parts: a control's, its segments' side by side, each as a
/// button's of its tab's label and icon.
fn measured(content: &Content, measure: &mut impl Measure) -> ((f32, f32), Vec<f32>) {
    let size = match content {
        Content::Text(text, _) => measure.text(text),
        Content::Icon(name) => measure.icon(name),
        Content::Field => measure.field(),
        Content::Button { label, icon } => measure.button(label, icon),
        Content::Separator => measure.separator(),
        Content::Progress(shape) => progress::natural(*shape),
        Content::Segments(tabs) => {
            let sizes: Vec<(f32, f32)> = tabs
                .iter()
                .map(|(label, icon)| measure.button(label, icon))
                .collect();
            let width = sizes.iter().map(|size| size.0).sum();
            let height = sizes
                .iter()
                .fold(0.0_f32, |height, size| height.max(size.1));
            return (
                (width, height),
                sizes.into_iter().map(|size| size.0).collect(),
            );
        }
    };
    (size, Vec::new())
}

/// The style of a view of tabs' control, at `scale`, of `distribution`: in
/// the middle of its view's top, of its content's width, or of the view's
/// where its tabs fill it; the page below it.
fn control_style(scale: f32, distribution: Distribution) -> Style {
    Style {
        align_self: Some(match distribution {
            Distribution::Fit => AlignItems::CENTER,
            _ => AlignItems::STRETCH,
        }),
        flex_shrink: 0.0,
        margin: Rect {
            left: LengthPercentageAuto::length(0.0),
            right: LengthPercentageAuto::length(0.0),
            top: LengthPercentageAuto::length(0.0),
            bottom: LengthPercentageAuto::length(tabs::INSET * scale),
        },
        ..Style::default()
    }
}

/// `style`, of `id` of `tree`, as its parent has it: a child of a scroll
/// area keeps its height down a column, and its width across a row, which
/// the scroll shows.
fn style_in(tree: &Tree, id: NodeId, mut style: Style) -> Style {
    let scrolls = tree
        .parent(id)
        .is_some_and(|parent| matches!(tree.kind(parent), Some(Kind::Scroll)));
    if scrolls {
        style.flex_shrink = 0.0;
    }
    style
}

/// Whether `id` of `tree` is a scroll area of a row, which scrolls
/// across.
pub(crate) fn across(tree: &Tree, id: NodeId) -> bool {
    matches!(tree.kind(id), Some(Kind::Scroll))
        && tree
            .layout(id)
            .is_some_and(|layout| layout.direction == Direction::Row)
}

/// `taffy`'s style of `layout`, of a node of `kind`, in pixels at `scale`,
/// the lengths of the titlebar and of its buttons of `room`: a text shrinks
/// below its content's width, as a title too long for its room, which its
/// box clips; a separator stretches across its parent.
fn style(kind: &Kind, layout: &view::Layout, scale: f32, room: Room) -> Style {
    let pixels = |points: f32| LengthPercentage::length(points * scale);
    let length = |length: Length| match length {
        Length::Auto => Dimension::auto(),
        Length::Points(points) => Dimension::length(points * scale),
        Length::Fraction(fraction) => Dimension::percent(fraction),
        Length::Titlebar => Dimension::length(room.titlebar),
        Length::Buttons => Dimension::length(room.buttons),
    };
    // A view of tabs: a column of its control and its page, inside its
    // box's room.
    let (tabbed, inset, top) = match kind {
        Kind::Tabs(_) => (true, tabs::INSET, tabs::TOP),
        _ => (false, 0.0, 0.0),
    };
    let outline = matches!(
        kind,
        Kind::Progress(progress) if progress.shape == view::ProgressShape::Outline
    );
    Style {
        flex_direction: match layout.direction {
            _ if tabbed => FlexDirection::Column,
            Direction::Row => FlexDirection::Row,
            Direction::Column => FlexDirection::Column,
        },
        gap: Size {
            width: pixels(layout.gap),
            height: pixels(layout.gap),
        },
        padding: Rect {
            left: pixels(layout.padding.left + inset),
            right: pixels(layout.padding.right + inset),
            top: pixels(layout.padding.top + top),
            bottom: pixels(layout.padding.bottom + inset),
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
        min_size: Size {
            width: match (kind, layout.min_width) {
                (_, Length::Points(points)) => LengthPercentageAuto::length(points * scale),
                (_, Length::Fraction(fraction)) => LengthPercentageAuto::percent(fraction),
                (_, Length::Titlebar) => LengthPercentageAuto::length(room.titlebar),
                (_, Length::Buttons) => LengthPercentageAuto::length(room.buttons),
                (Kind::Text(_), Length::Auto) => LengthPercentageAuto::length(0.0),
                (_, Length::Auto) => LengthPercentageAuto::auto(),
            },
            height: LengthPercentageAuto::auto(),
        },
        align_self: matches!(kind, Kind::Separator).then_some(AlignItems::STRETCH),
        // A scroll area keeps to the room that its layout gives it along
        // its direction, its children to their own heights down a column,
        // and to their own widths across a row.
        overflow: {
            let scrolls = matches!(kind, Kind::Scroll);
            let row = layout.direction == Direction::Row;
            let of = |along: bool| {
                if scrolls && along {
                    Overflow::Scroll
                } else {
                    Overflow::Visible
                }
            };
            taffy::Point {
                x: of(row),
                y: of(!row),
            }
        },
        scrollbar_width: 0.0,
        flex_grow: layout.grow,
        // An outline takes its parent's whole box, none of its children's
        // room.
        position: if outline {
            Position::Absolute
        } else {
            Position::Relative
        },
        inset: if outline {
            Rect {
                left: LengthPercentageAuto::length(0.0),
                right: LengthPercentageAuto::length(0.0),
                top: LengthPercentageAuto::length(0.0),
                bottom: LengthPercentageAuto::length(0.0),
            }
        } else {
            Rect::auto()
        },
        ..Style::default()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lokyr_plugin_sdk::view::{Edges, Node};

    use super::*;
    use lokyr_plugin_sdk::view::TextSize;

    /// A font of the tests': a text 6 points a character wide and 14 high,
    /// 4 and 10 of the small size, an icon of 16, which counts what it
    /// measures.
    #[derive(Default)]
    struct Fixed {
        measured: BTreeMap<String, usize>,
        small: bool,
    }

    impl Measure for Fixed {
        fn typeset(&mut self, typeset: Typeset) {
            self.small = typeset.size == TextSize::Small;
        }

        fn text(&mut self, text: &str) -> (f32, f32) {
            *self.measured.entry(text.to_owned()).or_default() += 1;
            let (width, height) = if self.small { (4.0, 10.0) } else { (6.0, 14.0) };
            (width * text.chars().count() as f32, height)
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
    fn a_text_of_lines_breaks_between_its_words_at_its_width_up_to_its_most_and_tells_its_cut() {
        let text = |key: &str, words: &str, lines: Option<u16>| {
            Node::text(words).key(key).layout(view::Layout {
                lines,
                ..view::Layout::default()
            })
        };
        let words = "aaa bbb ccc ddd eee";
        let root = Node::container(vec![
            text("two", words, Some(2)),
            text("all", words, Some(0)),
            text("one", words, None),
            text("single", words, Some(1)),
            text("short", "aaa", Some(2)),
            text("wide", "aaaaaaaaaaaa bb", Some(0)),
        ])
        .layout(view::Layout {
            direction: Direction::Column,
            ..view::Layout::default()
        });
        // 50 points at 2 pixels a point: three lines of 8 characters, "aaa
        // bbb", "ccc ddd" and "eee", of 14 points each.
        let (tree, mut layout, mut font) = laid(&root, 2.0, 100.0);
        let node = |key: &str| tree.by_key(&view::Key::from(key)).unwrap();
        let heights: Vec<f32> = ["two", "all", "one", "single", "short", "wide"]
            .into_iter()
            .map(|key| layout.frame(&tree, node(key)).unwrap().height)
            .collect();
        assert_eq!(heights, [56.0, 84.0, 28.0, 28.0, 28.0, 56.0]);
        assert_eq!(layout.frame(&tree, node("two")).unwrap().width, 100.0);
        // Cut of more lines than its most, or of a word wider than a line;
        // a text of one line, as by default, of none.
        let cuts: BTreeMap<String, bool> = layout
            .cuts(&mut font)
            .into_iter()
            .map(|(id, cut)| (tree.key(id).unwrap().0.clone(), cut))
            .collect();
        let expected = [
            ("two", true),
            ("all", false),
            ("single", true),
            ("short", false),
            ("wide", true),
        ];
        assert_eq!(
            cuts,
            expected
                .into_iter()
                .map(|(key, cut)| (key.to_owned(), cut))
                .collect()
        );
        // Its lines counted once at a room.
        font.measured.clear();
        layout.cuts(&mut font);
        assert!(font.measured.is_empty(), "{:?}", font.measured);
    }

    #[test]
    fn a_small_text_measures_in_the_small_size_of_its_style_or_its_parent_s_as_it_changes() {
        let bar = |size: Option<TextSize>| {
            Node::container(vec![
                Node::text("count").key("count"),
                Node::field("Find").key("field").style(view::Style {
                    text_size: Some(TextSize::Regular),
                    ..view::Style::default()
                }),
            ])
            .style(view::Style {
                text_size: size,
                ..view::Style::default()
            })
            .layout(view::Layout {
                direction: Direction::Column,
                align: Align::Start,
                ..view::Layout::default()
            })
        };
        let (mut tree, mut layout, mut font) = laid(&bar(Some(TextSize::Small)), 1.0, 200.0);
        let width = |tree: &Tree, layout: &Layout, key: &str| {
            let node = tree.by_key(&view::Key::from(key)).unwrap();
            layout.frame(tree, node).unwrap().width
        };
        // The parent's small size, of 4 points a character; the field's own
        // regular one, of its own.
        assert_eq!(width(&tree, &layout, "count"), 20.0);
        assert_eq!(
            tree.text_size(tree.by_key(&view::Key::from("count")).unwrap()),
            TextSize::Small
        );
        assert_eq!(
            tree.text_size(tree.by_key(&view::Key::from("field")).unwrap()),
            TextSize::Regular
        );
        // The parent's style changed: the child measured again, of 6.
        let changes = tree.update(&bar(None)).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert_eq!(width(&tree, &layout, "count"), 30.0);
        assert_eq!(font.measured["count"], 2);
        // A measure leaves the font of the regular size.
        assert!(!font.small);
    }

    #[test]
    fn the_lengths_of_the_titlebar_and_its_buttons_are_the_window_s() {
        // A band of the titlebar's height, its text past the buttons.
        let band = Node::container(vec![
            Node::container(Vec::new()).layout(view::Layout {
                width: Length::Buttons,
                ..view::Layout::default()
            }),
            Node::text("~").key("keys"),
        ])
        .layout(view::Layout {
            height: Length::Titlebar,
            ..view::Layout::default()
        });
        let (tree, mut layout, mut font) = laid(&band, 2.0, 200.0);
        let root = tree.root().unwrap();
        let keys = tree.children(root)[1];
        // Of none, as in full screen, none.
        assert_eq!(layout.frame(&tree, root).unwrap().height, 0.0);
        assert_eq!(layout.frame(&tree, keys).unwrap().x, 0.0);
        // Of the window's room, in pixels, whatever the scale.
        let room = Room {
            titlebar: 80.0,
            buttons: 150.0,
        };
        layout.set_room(&tree, room);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert_eq!(layout.frame(&tree, root).unwrap().height, 80.0);
        assert_eq!(layout.frame(&tree, keys).unwrap().x, 150.0);
        // Kept through a change of the scale, which the window gives anew.
        layout.set_scale(&tree, 1.0);
        layout.compute(&tree, Some(100.0), None, &mut font);
        assert_eq!(layout.frame(&tree, root).unwrap().height, 80.0);
        layout.set_room(&tree, Room::default());
        layout.compute(&tree, Some(100.0), None, &mut font);
        assert_eq!(layout.frame(&tree, root).unwrap().height, 0.0);
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
    fn a_row_s_scroll_area_scrolls_across_its_tabs_of_their_least_width() {
        // Eight tabs that share the row's 100 pixels, each of 30 at least:
        // 240 across, 140 to scroll.
        let tabs: Vec<Node> = (0..8)
            .map(|at| {
                Node::container(Vec::new())
                    .key(format!("tab-{at}").as_str())
                    .layout(view::Layout {
                        width: Length::Points(0.0),
                        min_width: Length::Points(30.0),
                        height: Length::Points(10.0),
                        grow: 1.0,
                        ..view::Layout::default()
                    })
            })
            .collect();
        let area = Node::scroll(tabs).key("strip").layout(view::Layout {
            width: Length::Points(100.0),
            ..view::Layout::default()
        });
        let (tree, mut layout, _) = laid(&area, 1.0, 100.0);
        let strip = tree.root().unwrap();
        let tab = |at: usize| tree.by_key(&format!("tab-{at}").as_str().into()).unwrap();
        assert_eq!(layout.frame(&tree, tab(1)).unwrap().width, 30.0);
        assert_eq!(layout.scroll_room(&tree, strip), 140.0);
        // A scroll moves them across, not down.
        assert!(layout.scroll_by(&tree, strip, 50.0));
        let second = layout.frame(&tree, tab(1)).unwrap();
        assert_eq!((second.x, second.y), (-20.0, 0.0));
        // The last shown, as far as it ends at the area's edge; the first,
        // back to the start; one shown already, no scroll.
        assert!(layout.reveal(&tree, tab(7)));
        assert_eq!(layout.scrolled(strip), 140.0);
        assert!(!layout.reveal(&tree, tab(6)));
        assert!(layout.reveal(&tree, tab(0)));
        assert_eq!(layout.scrolled(strip), 0.0);
        // Of room for them all, they share it, of no scroll.
        let mut wide = area.clone();
        wide.layout.width = Length::Points(320.0);
        let (tree, layout, _) = laid(&wide, 1.0, 320.0);
        let strip = tree.root().unwrap();
        assert_eq!(layout.scroll_room(&tree, strip), 0.0);
        let first = tree.by_key(&"tab-0".into()).unwrap();
        assert_eq!(layout.frame(&tree, first).unwrap().width, 40.0);
    }

    #[test]
    fn buttons_take_their_content_and_separators_stretch_across() {
        // A row, its items in the middle: a button of a label, one of an
        // icon, one of both, a toggle, and a separator; under it, a
        // separator across the column.
        let row = Node::container(vec![
            Node::button("Replace").key("replace"),
            Node::icon_button("add").key("add"),
            Node::new(Kind::Button(view::Button {
                label: "Gear".to_owned(),
                icon: "settings".to_owned(),
                disabled: false,
            }))
            .key("settings"),
            Node::separator(),
            Node::toggle("regex", true).key("regex"),
        ])
        .layout(view::Layout {
            gap: 4.0,
            align: Align::Center,
            ..view::Layout::default()
        });
        let column = Node::container(vec![row, Node::separator()]).layout(view::Layout {
            direction: Direction::Column,
            ..view::Layout::default()
        });
        for scale in [1.0, 2.0] {
            let (tree, layout, _) = laid(&column, scale, 300.0 * scale);
            let root = tree.root().unwrap();
            let row = tree.children(root)[0];
            let frame = |id| {
                let frame = layout.frame(&tree, id).unwrap();
                [frame.x, frame.y, frame.width, frame.height].map(|value| value / scale)
            };
            let [replace, add, settings, upright, regex] = tree.children(row).to_vec()[..] else {
                panic!();
            };
            // A label of 7 characters, 42 points, and 8 at each side; 3
            // above and below its line of 14, in the middle of the row,
            // which an icon's 16 makes 22 high.
            assert_eq!(frame(replace), [0.0, 1.0, 58.0, 20.0]);
            assert_eq!(frame(add), [62.0, 0.0, 22.0, 22.0]);
            // The icon, 4 points, then the label, 24.
            assert_eq!(frame(settings), [88.0, 0.0, 60.0, 22.0]);
            // Upright, a point wide, as tall as the row.
            assert_eq!(frame(upright), [152.0, 0.0, 1.0, 22.0]);
            assert_eq!(frame(regex), [157.0, 0.0, 22.0, 22.0]);
            // Flat, a point high, as wide as the column.
            let flat = tree.children(root)[1];
            assert_eq!(frame(flat), [0.0, 22.0, 300.0, 1.0]);
        }
    }

    #[test]
    fn a_scroll_area_keeps_its_children_s_heights_and_moves_them_as_it_scrolls() {
        let boxes = |count: usize| -> Vec<Node> {
            (0..count)
                .map(|index| {
                    Node::container(vec![])
                        .key(format!("box-{index}").as_str())
                        .layout(view::Layout {
                            height: Length::Points(10.0),
                            ..view::Layout::default()
                        })
                })
                .collect()
        };
        let area = |direction: Direction, count: usize| {
            Node::container(vec![Node::scroll(boxes(count)).key("area").layout(
                view::Layout {
                    direction,
                    padding: Edges::all(5.0),
                    height: Length::Points(50.0),
                    ..view::Layout::default()
                },
            )])
        };
        let mut tree = Tree::new();
        let changes = tree.update(&area(Direction::Column, 8)).unwrap();
        let mut layout = Layout::new(2.0);
        layout.update(&tree, &changes);
        let mut font = Fixed::default();
        layout.compute(&tree, Some(200.0), None, &mut font);
        let node = tree.children(tree.root().unwrap())[0];
        let child = |tree: &Tree, layout: &Layout, index: usize| {
            layout.frame(tree, tree.children(node)[index]).unwrap()
        };
        // 8 boxes of 10 points and a padding of 5 about them, in 50: 40 to
        // scroll, 80 pixels.
        assert_eq!(child(&tree, &layout, 7).height, 20.0);
        assert_eq!(layout.scroll_room(&tree, node), 80.0);
        assert!(layout.scroll_by(&tree, node, 30.0));
        assert_eq!(child(&tree, &layout, 0).y, 10.0 - 30.0);
        assert!(!layout.scroll_by(&tree, node, f32::NAN));
        // Of 4 boxes, the content fits again, and the scroll goes.
        let changes = tree.update(&area(Direction::Column, 4)).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert_eq!(layout.scrolled(node), 0.0);
        // The boxes of a row that scrolls, kept as it turns into a column,
        // take the column's heights.
        let mut tree = Tree::new();
        let changes = tree.update(&area(Direction::Row, 8)).unwrap();
        let mut layout = Layout::new(2.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        let changes = tree.update(&area(Direction::Column, 8)).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        let node = tree.children(tree.root().unwrap())[0];
        let first = layout.frame(&tree, tree.children(node)[0]).unwrap();
        assert_eq!(first.height, 20.0);
        // Moved to a display of a scale of 1, as far scrolled in points,
        // its boxes of their heights.
        assert!(layout.scroll_by(&tree, node, 30.0));
        layout.set_scale(&tree, 1.0);
        layout.compute(&tree, Some(100.0), None, &mut font);
        assert_eq!(layout.scrolled(node), 15.0);
        assert_eq!(layout.scroll_room(&tree, node), 40.0);
        let first = layout.frame(&tree, tree.children(node)[0]).unwrap();
        assert_eq!(first.height, 10.0);
        // And back.
        layout.set_scale(&tree, 2.0);
        layout.compute(&tree, Some(200.0), None, &mut font);
        assert_eq!(layout.scrolled(node), 30.0);
        assert_eq!(layout.scroll_room(&tree, node), 80.0);
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

    /// A tiny random generator of the tests', of a seed.
    struct Random(u64);

    impl Random {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    /// A random node of `depth`, of boxes of up to three children, texts,
    /// icons and progresses of every shape, the outline's out of the flow;
    /// a third of them of one of `keys`, which each takes once.
    fn random_node(random: &mut Random, depth: u32, keys: &mut Vec<&'static str>) -> Node {
        use lokyr_plugin_sdk::view::{ProgressShape, ProgressState};
        let mut node = match random.below(5) {
            0 | 1 if depth < 7 => Node::container(
                (0..random.below(4))
                    .map(|_| random_node(random, depth + 1, keys))
                    .collect(),
            ),
            2 => Node::text(["x", "yy", "zzz"][random.below(3) as usize]),
            3 => Node::progress(
                [
                    ProgressShape::Bar,
                    ProgressShape::Ring,
                    ProgressShape::Outline,
                    ProgressShape::Dots,
                ][random.below(4) as usize],
                ProgressState::Normal,
                0.5,
            ),
            _ => Node::icon(["a", "b"][random.below(2) as usize]),
        };
        if random.below(3) == 0 && !keys.is_empty() {
            let at = random.below(keys.len() as u64) as usize;
            node = node.key(keys.remove(at));
        }
        node
    }

    /// Checks that the layout's nodes of `id`'s subtree have the children
    /// of `tree`'s, each of them as its parent.
    fn in_step(tree: &Tree, layout: &Layout, id: NodeId) {
        let node = layout.nodes[&id];
        if tree.kind(id).is_some_and(Kind::has_children) {
            let wanted: Vec<taffy::NodeId> = layout
                .controls
                .get(&id)
                .copied()
                .into_iter()
                .chain(tree.children(id).iter().map(|child| layout.nodes[child]))
                .collect();
            assert_eq!(layout.taffy.children(node).unwrap(), wanted, "{id:?}");
        }
        for &child in tree.children(id) {
            assert_eq!(layout.taffy.parent(layout.nodes[&child]), Some(node));
            in_step(tree, layout, child);
        }
    }

    #[test]
    fn a_box_that_loses_its_last_child_out_of_the_flow_lays_out_again() {
        use lokyr_plugin_sdk::view::{ProgressShape, ProgressState};
        let outline = Node::progress(ProgressShape::Outline, ProgressState::Normal, 0.5);
        let (mut tree, mut layout, mut font) = laid(&Node::container(vec![outline]), 2.0, 200.0);
        let changes = tree.update(&Node::container(Vec::new())).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0), None, &mut font);
        let root = tree.root().unwrap();
        assert!(layout.frame(&tree, root).is_some());
        in_step(&tree, &layout, root);
    }

    #[test]
    fn random_renders_keep_the_layout_s_tree_in_step() {
        for seed in 1..800_u64 {
            let mut random = Random(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let mut tree = Tree::new();
            let mut layout = Layout::new(2.0);
            let mut font = Fixed::default();
            for _ in 0..25 {
                let mut keys = vec!["k1", "k2", "k3", "k4", "k5", "k6"];
                let root = Node::container(
                    (0..random.below(4))
                        .map(|_| random_node(&mut random, 1, &mut keys))
                        .collect(),
                );
                let changes = tree.update(&root).unwrap();
                layout.update(&tree, &changes);
                in_step(&tree, &layout, tree.root().unwrap());
                layout.compute(&tree, Some(400.0), None, &mut font);
            }
        }
    }
}

//! The lists of many rows, of flat items and a row's template, and the
//! scroll areas. A list lays out and draws the rows that show alone, each a
//! small tree of the template that takes its item's texts, kept while it
//! shows, so that a list of 100 000 rows costs a frame what its visible
//! rows cost. Its plugin gives a window of its items, and the host asks for
//! those about the rows that show, as they scroll past the window, by
//! `list-range`.
//!
//! The host keeps each list's scroll and selection by its node's key: its
//! scroll as the first row that shows, its anchor, and the points of it
//! above the list's top, so that rows of their own heights, measured as
//! they show and estimated before, keep the rows that show in place as
//! their estimates turn into their heights. The keys move the selection, a
//! click selects a row, Return and a double click activate it, and a wheel
//! or a trackpad scrolls the rows, the platform giving the trackpad's
//! momentum.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use lokyr_plugin_sdk::view::{self, Key as NodeKey, Kind, Node, UiEventKind};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, Keypad};

use crate::layout::{Layout, Measure};
use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// The rows past those that show, above them and below them, whose items
/// the host asks for with theirs.
pub const AHEAD: usize = 50;

/// The height of a row that no measure gave yet, in points, before the
/// template's.
const ESTIMATE: f32 = 20.0;

/// A row's least height, in points, of its list's or of its measure: so
/// many rows fill a list at most, as rows of no height would never fill
/// it, and each layout and scroll would walk every row.
pub const SHORTEST_ROW: f32 = 1.0;

/// The lists of a tree, by their keys.
#[derive(Debug, Default)]
pub struct Lists {
    states: HashMap<NodeKey, ListState>,
}

/// A list's state, which the host keeps.
#[derive(Debug, Default)]
pub struct ListState {
    /// The first row that shows, at least in part.
    anchor: usize,
    /// The points of the anchor's row above the list's top.
    offset: f32,
    selected: Option<usize>,
    /// The rows' heights, in points, as they were measured, of a list of
    /// rows of their own heights.
    heights: HashMap<usize, f32>,
    /// The height of a row not measured yet, in points: the template's.
    estimate: Option<f32>,
    /// The rows that show, built and laid out, by their indices.
    rows: HashMap<usize, Row>,
    /// The template that the rows were built of.
    template: Option<Node>,
    /// The items asked of the plugin last, which its render gives.
    asked: Option<Range<usize>>,
    /// The items that the plugin's last render gave, and its count, whose
    /// change asks again for those about the rows that show.
    given: Option<(Range<usize>, usize)>,
    /// The list's height and width, in points, of its last layout.
    view: (f32, f32),
}

/// A row that shows: its item's texts, and the tree of the template that
/// shows them, laid out.
pub struct Row {
    texts: Vec<String>,
    tree: Tree,
    layout: Layout,
}

impl std::fmt::Debug for Row {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Row")
            .field("texts", &self.texts)
            .finish_non_exhaustive()
    }
}

impl Row {
    /// Its tree.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Its layout, of the list's width.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }
}

impl ListState {
    /// The selected item, if one is.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The first row that shows, and the points of it above the list's top.
    pub fn anchor(&self) -> (usize, f32) {
        (self.anchor, self.offset)
    }

    /// The height of the row `index` of `list`, in points: its own, its
    /// measure, or an estimate.
    fn height(&self, list: &view::List, index: usize) -> f32 {
        list.row_height
            .unwrap_or_else(|| {
                self.heights
                    .get(&index)
                    .copied()
                    .unwrap_or_else(|| self.estimate.unwrap_or(ESTIMATE))
            })
            .max(SHORTEST_ROW)
    }

    /// The height of the row `index` of `list`, in points: its own, its
    /// measure, or an estimate, as the rows that show are placed.
    pub fn height_of(&self, list: &view::List, index: usize) -> f32 {
        self.height(list, index)
    }

    /// The rows that show, of `list`: each index and its top, in points
    /// from the list's top.
    pub fn visible(&self, list: &view::List) -> Vec<(usize, f32)> {
        let mut rows = Vec::new();
        let (mut index, mut top) = (self.anchor, -self.offset);
        while index < list.count && top < self.view.0 {
            rows.push((index, top));
            top += self.height(list, index);
            index += 1;
        }
        rows
    }

    /// The row `index`, if it shows and its item came.
    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(&index)
    }

    /// How far the rows are scrolled, and how far they can, in points, as
    /// a scroll bar shows them: of the rows' heights, estimated those not
    /// measured.
    pub fn extent(&self, list: &view::List) -> (f32, f32) {
        let typical = list.row_height.unwrap_or_else(|| {
            if self.heights.is_empty() {
                self.estimate.unwrap_or(ESTIMATE)
            } else {
                self.heights.values().sum::<f32>() / self.heights.len() as f32
            }
        });
        let total = list.count as f32 * typical;
        (
            self.anchor as f32 * typical + self.offset,
            (total - self.view.0).max(0.0),
        )
    }

    /// Scrolls the rows of `list` by `points`, down where they are
    /// positive, within the first row's top and the last row's bottom.
    fn scroll(&mut self, list: &view::List, points: f32) {
        if !points.is_finite() {
            return;
        }
        self.offset += points;
        while self.offset < 0.0 && self.anchor > 0 {
            self.anchor -= 1;
            self.offset += self.height(list, self.anchor);
        }
        if self.offset < 0.0 {
            self.offset = 0.0;
        }
        while self.anchor + 1 < list.count && self.offset >= self.height(list, self.anchor) {
            self.offset -= self.height(list, self.anchor);
            self.anchor += 1;
        }
        self.keep_within(list);
    }

    /// Keeps the rows of `list` from scrolling past the last row's bottom,
    /// or past the first's top where they all fit.
    fn keep_within(&mut self, list: &view::List) {
        if list.count == 0 {
            (self.anchor, self.offset) = (0, 0.0);
            return;
        }
        self.anchor = self.anchor.min(list.count - 1);
        let last = self.lowest(list, list.count - 1);
        if (self.anchor, self.offset) > last {
            (self.anchor, self.offset) = last;
        }
    }

    /// The scroll that puts the bottom of the row `index` of `list` at the
    /// list's bottom: the rows above it that fill the list, or the first
    /// row's top where they do not.
    fn lowest(&self, list: &view::List, index: usize) -> (usize, f32) {
        let mut filled = self.height(list, index);
        let mut anchor = index;
        while filled < self.view.0 && anchor > 0 {
            anchor -= 1;
            filled += self.height(list, anchor);
        }
        (anchor, (filled - self.view.0).max(0.0))
    }

    /// Scrolls the rows of `list` the least that shows all of the row
    /// `index`, or its top where it is taller than the list.
    fn reveal(&mut self, list: &view::List, index: usize) {
        // Of a list not laid out yet, at its top.
        if index < self.anchor || (index == self.anchor && self.offset > 0.0) || self.view.0 <= 0.0
        {
            (self.anchor, self.offset) = (index, 0.0);
            return;
        }
        let bottom = self.lowest(list, index);
        if (self.anchor, self.offset) < bottom {
            (self.anchor, self.offset) = bottom;
        }
    }

    /// Selects the item `index` of `list`, and shows its row: its event,
    /// if the selection moved.
    fn select(&mut self, list: &view::List, index: usize) -> Option<UiEventKind> {
        if list.count == 0 {
            return None;
        }
        let index = index.min(list.count - 1);
        self.reveal(list, index);
        (self.selected != Some(index)).then(|| {
            self.selected = Some(index);
            UiEventKind::Select { item: index }
        })
    }
}

impl Lists {
    /// No list.
    pub fn new() -> Self {
        Self::default()
    }

    /// The state of the list of `key`, if it has one yet.
    pub fn state(&self, key: &NodeKey) -> Option<&ListState> {
        self.states.get(key)
    }

    /// Forgets the lists whose keys `tree` no longer has as lists.
    pub fn updated(&mut self, tree: &Tree) {
        self.states.retain(|key, _| {
            tree.by_key(key)
                .is_some_and(|node| matches!(tree.kind(node), Some(Kind::List(_))))
        });
    }

    /// Lays out the rows of each list of `tree` that show, as `layout`
    /// lays the list out, their texts measured by `measure`: builds the
    /// rows that came into view of their items, measures those of their
    /// own heights, forgets those that left it, and keeps the scroll within
    /// the rows. The `list-range` events of the lists whose rows that show
    /// passed the items that their plugins gave, of the items about them.
    pub fn lay_out(
        &mut self,
        tree: &Tree,
        layout: &Layout,
        measure: &mut impl Measure,
    ) -> Vec<NodeEvent> {
        let mut events = Vec::new();
        let scale = layout.scale();
        for node in lists(tree) {
            let (Some(Kind::List(list)), Some(key), Some(frame)) =
                (tree.kind(node), tree.key(node), layout.frame(tree, node))
            else {
                continue;
            };
            let state = self.states.entry(key.clone()).or_default();
            let view = (frame.height / scale, frame.width / scale);
            // A new template, or a new width, builds and measures anew.
            if state.template.as_ref() != Some(&*list.row) || state.view.1 != view.1 {
                state.rows.clear();
                state.heights.clear();
                state.estimate = None;
                state.template = Some((*list.row).clone());
            }
            state.view = view;
            if state.estimate.is_none() {
                let empty = build(&list.row, &[], frame.width, (scale, &mut *measure));
                state.estimate = Some(height(&empty));
            }
            if let Some(selected) = state.selected
                && selected >= list.count
            {
                state.selected = list.count.checked_sub(1);
            }
            state.keep_within(list);
            // A render of other items, or of another count, as its plugin's
            // data changed, asks again for those about the rows that show.
            let given = (list.first..list.first + list.items.len(), list.count);
            if state.given.as_ref() != Some(&given) {
                state.asked = None;
                state.given = Some(given);
            }
            // The rows that show, built of their items' texts, measured;
            // the anchor's row keeps its place as the rows above and below
            // turn from estimates into heights; a row whose item the render
            // did not give, blank.
            let mut shown = HashSet::new();
            let (mut index, mut top) = (state.anchor, -state.offset);
            while index < list.count && top < state.view.0 {
                shown.insert(index);
                if let Some(item) = index
                    .checked_sub(list.first)
                    .and_then(|at| list.items.get(at))
                {
                    let fresh = state
                        .rows
                        .get(&index)
                        .is_none_or(|row| row.texts != item.texts);
                    if fresh {
                        let row =
                            build(&list.row, &item.texts, frame.width, (scale, &mut *measure));
                        if list.row_height.is_none() {
                            state.heights.insert(index, height(&row));
                        }
                        state.rows.insert(index, row);
                    }
                } else {
                    state.rows.remove(&index);
                }
                top += state.height(list, index);
                index += 1;
            }
            state.rows.retain(|index, _| shown.contains(index));
            state.keep_within(list);
            // The items about the rows that show, if the plugin's do not
            // hold them and none were asked for that do.
            let showing = state.anchor..index.max(state.anchor + 1).min(list.count);
            let given = list.first..list.first + list.items.len();
            let covers =
                |range: &Range<usize>| range.start <= showing.start && showing.end <= range.end;
            if !showing.is_empty() && !covers(&given) && !state.asked.as_ref().is_some_and(covers) {
                let first = showing.start.saturating_sub(AHEAD);
                let end = (showing.end + AHEAD).min(list.count);
                state.asked = Some(first..end);
                events.push(NodeEvent {
                    node,
                    key: key.clone(),
                    kind: UiEventKind::ListRange {
                        first,
                        count: end - first,
                    },
                });
            }
        }
        events
    }

    /// Scrolls the list `node` of `tree` by `points`, down where they are
    /// positive: whether it is a list.
    pub fn scroll(&mut self, tree: &Tree, node: NodeId, points: f32) -> bool {
        let (Some(Kind::List(list)), Some(key)) = (tree.kind(node), tree.key(node)) else {
            return false;
        };
        self.states
            .entry(key.clone())
            .or_default()
            .scroll(list, points);
        true
    }

    /// Selects the item `item` of the list `node` of `tree`, and shows its
    /// row, as its plugin asks: no event.
    pub fn select(&mut self, tree: &Tree, node: NodeId, item: usize) {
        if let (Some(Kind::List(list)), Some(key)) = (tree.kind(node), tree.key(node)) {
            self.states
                .entry(key.clone())
                .or_default()
                .select(list, item);
        }
    }

    /// What `key` does to the list `node` of `tree`, which has the
    /// keyboard's focus: the arrows up and down move the selection by a
    /// row, Page Up and Page Down by the rows that show, Home and End to
    /// the first and the last, each showing its row, and Return activates
    /// the item selected; `None` for another key, or another node, which
    /// the app's bindings may take. A release takes nothing.
    pub fn key(&mut self, tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
        let (Some(Kind::List(list)), Some(node_key)) = (tree.kind(node), tree.key(node)) else {
            return None;
        };
        let mods = key.mods;
        if mods.shift
            || mods.control
            || mods.left_alt
            || mods.right_alt
            || mods.super_key
            || mods.hyper
            || mods.meta
        {
            return None;
        }
        let state = self.states.entry(node_key.clone()).or_default();
        let last = list.count.checked_sub(1);
        let page = state.visible(list).len().saturating_sub(1).max(1);
        let selected = state.selected;
        let to = match key.key {
            Key::Up => selected.map_or(last, |at| Some(at.saturating_sub(1))),
            Key::Down => selected.map_or(Some(0), |at| Some(at + 1)),
            Key::PageUp => selected.map_or(Some(0), |at| Some(at.saturating_sub(page))),
            Key::PageDown => selected.map_or(Some(page), |at| Some(at + page)),
            Key::Home => Some(0),
            Key::End => last,
            Key::Enter | Key::Keypad(Keypad::Enter) => {
                if key.action != KeyAction::Press {
                    return Some(Vec::new());
                }
                return Some(
                    selected
                        .map(|item| NodeEvent {
                            node,
                            key: node_key.clone(),
                            kind: UiEventKind::Activate { item },
                        })
                        .into_iter()
                        .collect(),
                );
            }
            _ => return None,
        };
        if key.action == KeyAction::Release {
            return Some(Vec::new());
        }
        let kind = to.and_then(|to| state.select(list, to));
        Some(
            kind.map(|kind| NodeEvent {
                node,
                key: node_key.clone(),
                kind,
            })
            .into_iter()
            .collect(),
        )
    }

    /// The pointer's primary button went down at `at`, in pixels from the
    /// tree's corner, over the list `node` of `tree`, of `clicks` in a row:
    /// the row under it selected, and a double click's activated.
    pub fn press(
        &mut self,
        (tree, layout): (&Tree, &Layout),
        node: NodeId,
        (at, clicks): ((f32, f32), u32),
    ) -> Vec<NodeEvent> {
        let (Some(Kind::List(list)), Some(key), Some(frame)) =
            (tree.kind(node), tree.key(node), layout.frame(tree, node))
        else {
            return Vec::new();
        };
        let state = self.states.entry(key.clone()).or_default();
        let y = (at.1 - frame.y) / layout.scale();
        // Below the last row, nothing.
        let Some((index, _)) = state
            .visible(list)
            .into_iter()
            .take_while(|&(_, top)| top <= y)
            .last()
            .filter(|&(index, top)| y < top + state.height(list, index))
        else {
            return Vec::new();
        };
        let mut kinds: Vec<UiEventKind> = state.select(list, index).into_iter().collect();
        if clicks == 2 {
            kinds.push(UiEventKind::Activate { item: index });
        }
        kinds
            .into_iter()
            .map(|kind| NodeEvent {
                node,
                key: key.clone(),
                kind,
            })
            .collect()
    }
}

/// The innermost list or scroll area of `path`, nodes of `tree` from the
/// root, which a wheel or a trackpad over them scrolls.
pub fn scrollable(tree: &Tree, path: &[NodeId]) -> Option<NodeId> {
    path.iter()
        .rev()
        .copied()
        .find(|&id| matches!(tree.kind(id), Some(Kind::List(_) | Kind::Scroll)))
}

/// Scrolls the list or the scroll area `node` of `tree`, as a wheel or a
/// trackpad does, `y` points towards the content above, as the system's
/// natural scrolling has it: whether it is one.
pub fn scroll(
    (tree, layout, lists): (&Tree, &mut Layout, &mut Lists),
    node: NodeId,
    y: f32,
) -> bool {
    match tree.kind(node) {
        Some(Kind::List(_)) => lists.scroll(tree, node, -y),
        Some(Kind::Scroll) => {
            let scale = layout.scale();
            layout.scroll_by(tree, node, -y * scale);
            true
        }
        _ => false,
    }
}

/// The lists of `tree`, in its order.
fn lists(tree: &Tree) -> Vec<NodeId> {
    fn walk(tree: &Tree, id: NodeId, found: &mut Vec<NodeId>) {
        if matches!(tree.kind(id), Some(Kind::List(_))) {
            found.push(id);
        }
        for &child in tree.children(id) {
            walk(tree, child, found);
        }
    }
    let mut found = Vec::new();
    if let Some(root) = tree.root() {
        walk(tree, root, &mut found);
    }
    found
}

/// A row of `template`, of `texts`, laid out in `width` pixels at
/// `scale`, its texts measured by `measure`.
fn build(
    template: &Node,
    texts: &[String],
    width: f32,
    (scale, measure): (f32, &mut impl Measure),
) -> Row {
    let filled = fill(template, texts);
    let mut tree = Tree::new();
    let mut layout = Layout::new(scale);
    if let Ok(changes) = tree.update(&filled) {
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(width), None, measure);
    }
    Row {
        texts: texts.to_vec(),
        tree,
        layout,
    }
}

/// The height of `row`, in points.
fn height(row: &Row) -> f32 {
    row.tree
        .root()
        .and_then(|root| row.layout.frame(&row.tree, root))
        .map_or(0.0, |frame| frame.height / row.layout.scale())
}

/// `template` of `texts`: its texts and its icons, in the tree's order,
/// each of the next text, those past `texts` as they are.
fn fill(template: &Node, texts: &[String]) -> Node {
    fn walk(node: &mut Node, texts: &mut std::slice::Iter<'_, String>) {
        match &mut node.kind {
            Kind::Text(text) | Kind::Icon(text) => {
                if let Some(given) = texts.next() {
                    text.clone_from(given);
                }
            }
            _ => {}
        }
        for child in &mut node.children {
            walk(child, texts);
        }
    }
    let mut node = template.clone();
    walk(&mut node, &mut texts.iter());
    node
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{Direction, Length, ListItem};
    use lokyr_vt_core::input::Mods;

    use super::*;

    /// A font of the tests': a character 6 points wide, a text 14 points
    /// high, or 30 of one that starts with `tall`; an icon of 16.
    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            let height = if text.starts_with("tall") { 30.0 } else { 14.0 };
            (6.0 * text.chars().count() as f32, height)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    /// The items from `first` to `end`, of their numbers' texts, `tall`
    /// for the even ones where `tall`.
    fn items(range: Range<usize>, tall: bool) -> Vec<ListItem> {
        range
            .map(|index| ListItem {
                texts: vec![if tall && index % 2 == 0 {
                    format!("tall {index}")
                } else {
                    format!("row {index}")
                }],
            })
            .collect()
    }

    /// A list `rows` of `count` items, of the items from `first` to `end`,
    /// 100 points high, of rows of `height`, or of their own.
    fn list(count: usize, given: Range<usize>, height: Option<f32>) -> Node {
        let mut node = Node::list(
            Node::container(vec![Node::text("")]),
            count,
            (given.start, items(given, height.is_none())),
        )
        .key("rows")
        .layout(view::Layout {
            height: Length::Points(100.0),
            ..view::Layout::default()
        });
        if let Kind::List(list) = &mut node.kind {
            list.row_height = height;
        }
        Node::container(vec![node]).layout(view::Layout {
            direction: Direction::Column,
            ..view::Layout::default()
        })
    }

    /// The tree of `root`, laid out at `scale` in 200 points.
    fn laid(root: &Node, scale: f32) -> (Tree, Layout, NodeId) {
        let mut tree = Tree::new();
        let changes = tree.update(root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(200.0 * scale), None, &mut Fixed);
        let node = tree.children(tree.root().unwrap())[0];
        (tree, layout, node)
    }

    fn told(events: Vec<NodeEvent>) -> Vec<UiEventKind> {
        events.into_iter().map(|event| event.kind).collect()
    }

    fn state(lists: &Lists) -> &ListState {
        lists.state(&NodeKey::from("rows")).unwrap()
    }

    fn of(tree: &Tree, node: NodeId) -> &view::List {
        let Some(Kind::List(list)) = tree.kind(node) else {
            panic!("no list");
        };
        list
    }

    #[test]
    fn a_list_lays_out_its_rows_that_show_alone_and_asks_for_their_items() {
        // 100 000 rows of 20 points, in 100: 5 show, of the items given.
        let (tree, layout, node) = laid(&list(100_000, 0..60, Some(20.0)), 2.0);
        let mut lists = Lists::new();
        assert_eq!(told(lists.lay_out(&tree, &layout, &mut Fixed)), []);
        let rows = of(&tree, node);
        let shown = state(&lists).visible(rows);
        assert_eq!(
            shown,
            [(0, 0.0), (1, 20.0), (2, 40.0), (3, 60.0), (4, 80.0)]
        );
        assert_eq!(state(&lists).rows.len(), 5);
        let row = state(&lists).row(3).unwrap();
        assert_eq!(
            row.tree()
                .kind(row.tree().children(row.tree().root().unwrap())[0]),
            Some(&Kind::Text("row 3".to_owned()))
        );
        // Half a row down: 6 show, the first in part.
        lists.scroll(&tree, node, 10.0);
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert_eq!(state(&lists).visible(rows).len(), 6);
        assert_eq!(state(&lists).visible(rows)[0], (0, -10.0));
        // Within the items given, nothing asked; past them, the items
        // about the rows that show, once.
        lists.scroll(&tree, node, 1090.0);
        assert_eq!(state(&lists).anchor(), (55, 0.0));
        assert_eq!(told(lists.lay_out(&tree, &layout, &mut Fixed)), []);
        lists.scroll(&tree, node, 100.0);
        assert_eq!(
            told(lists.lay_out(&tree, &layout, &mut Fixed)),
            [UiEventKind::ListRange {
                first: 10,
                count: 105
            }]
        );
        assert_eq!(told(lists.lay_out(&tree, &layout, &mut Fixed)), []);
        // The rows without their items yet are not built.
        assert!(state(&lists).row(62).is_none());
        // The render that gives them builds them.
        let mut tree = tree;
        tree.update(&list(100_000, 10..115, Some(20.0))).unwrap();
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert!(state(&lists).row(62).is_some());
        assert_eq!(state(&lists).rows.len(), 5);
        // At the end, the last rows' items.
        lists.scroll(&tree, node, 1e9);
        assert_eq!(state(&lists).anchor(), (99_995, 0.0));
        assert_eq!(
            told(lists.lay_out(&tree, &layout, &mut Fixed)),
            [UiEventKind::ListRange {
                first: 99_945,
                count: 55
            }]
        );
        // And back past the top, the first row's.
        lists.scroll(&tree, node, -1e9);
        assert_eq!(state(&lists).anchor(), (0, 0.0));
        assert_eq!(
            state(&lists).extent(of(&tree, node)),
            (0.0, 2_000_000.0 - 100.0)
        );
    }

    fn pressed(key: Key, mods: Mods, action: KeyAction) -> KeyEvent<'static> {
        KeyEvent {
            key,
            action,
            mods,
            text: "",
            unmodified: "",
            base_layout: None,
        }
    }

    #[test]
    fn the_keys_move_the_selection_and_show_its_row() {
        let (tree, layout, node) = laid(&list(100_000, 0..60, Some(20.0)), 1.0);
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut Fixed);
        let press = |lists: &mut Lists, key| {
            told(
                lists
                    .key(&tree, node, &pressed(key, Mods::NONE, KeyAction::Press))
                    .unwrap(),
            )
        };
        let select = |item| vec![UiEventKind::Select { item }];
        // Up without a selection selects the last; Home, the first, to the
        // top of its row, which a scroll of half of it hid.
        assert_eq!(press(&mut lists, Key::Up), select(99_999));
        lists.scroll(&tree, node, -1e9);
        lists.scroll(&tree, node, 10.0);
        assert_eq!(press(&mut lists, Key::Home), select(0));
        assert_eq!(state(&lists).anchor(), (0, 0.0));
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert_eq!(press(&mut lists, Key::Down), select(0));
        assert_eq!(press(&mut lists, Key::Down), select(1));
        assert_eq!(press(&mut lists, Key::Up), select(0));
        assert_eq!(press(&mut lists, Key::Up), []);
        // A page, of the rows that show but one.
        assert_eq!(press(&mut lists, Key::PageDown), select(4));
        assert_eq!(press(&mut lists, Key::PageDown), select(8));
        assert_eq!(press(&mut lists, Key::PageUp), select(4));
        assert_eq!(press(&mut lists, Key::End), select(99_999));
        assert_eq!(
            press(&mut lists, Key::Enter),
            [UiEventKind::Activate { item: 99_999 }]
        );
        assert_eq!(press(&mut lists, Key::Home), select(0));
        assert_eq!(
            press(&mut lists, Key::Keypad(Keypad::Enter)),
            [UiEventKind::Activate { item: 0 }]
        );
        // The row selected shows: at the bottom from below, at the top
        // from above.
        for _ in 0..7 {
            press(&mut lists, Key::Down);
        }
        assert_eq!(state(&lists).selected(), Some(7));
        assert_eq!(state(&lists).anchor(), (3, 0.0));
        press(&mut lists, Key::End);
        assert_eq!(state(&lists).anchor(), (99_995, 0.0));
        press(&mut lists, Key::Home);
        assert_eq!(state(&lists).anchor(), (0, 0.0));
        // A repeat moves again; a release, a modifier, or another key
        // takes nothing.
        let mut lists_key = |key, mods, action| {
            lists
                .key(&tree, node, &pressed(key, mods, action))
                .map(told)
        };
        assert_eq!(
            lists_key(Key::Down, Mods::NONE, KeyAction::Repeat),
            Some(select(1))
        );
        assert_eq!(
            lists_key(Key::Down, Mods::NONE, KeyAction::Release),
            Some(vec![])
        );
        assert_eq!(
            lists_key(Key::Enter, Mods::NONE, KeyAction::Release),
            Some(vec![])
        );
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        assert_eq!(lists_key(Key::Down, shift, KeyAction::Press), None);
        assert_eq!(
            lists_key(Key::Char('a'), Mods::NONE, KeyAction::Press),
            None
        );
        let root = tree.root().unwrap();
        assert_eq!(
            lists.key(
                &tree,
                root,
                &pressed(Key::Down, Mods::NONE, KeyAction::Press)
            ),
            None
        );
    }

    #[test]
    fn a_click_selects_its_row_and_a_double_click_activates_it() {
        let (tree, layout, node) = laid(&list(100, 0..100, Some(20.0)), 2.0);
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut Fixed);
        lists.scroll(&tree, node, 10.0);
        let frame = layout.frame(&tree, node).unwrap();
        // At a scale of 2, 50 pixels down: 25 points, of the rows from 10
        // points above the top, the second's.
        let at = (frame.x + 20.0, frame.y + 50.0);
        let mut click = |at, clicks| told(lists.press((&tree, &layout), node, (at, clicks)));
        assert_eq!(click(at, 1), [UiEventKind::Select { item: 1 }]);
        assert_eq!(click(at, 2), [UiEventKind::Activate { item: 1 }]);
        // At the third row's top, 30 points down, the third.
        let top = (frame.x + 20.0, frame.y + 60.0);
        assert_eq!(click(top, 1), [UiEventKind::Select { item: 2 }]);
        // Of two rows, the second's bottom 40 points down; past it, nothing
        // selected or activated.
        let (tree, layout, node) = laid(&list(2, 0..2, Some(20.0)), 1.0);
        let frame = layout.frame(&tree, node).unwrap();
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut Fixed);
        let mut click =
            |y, clicks| told(lists.press((&tree, &layout), node, ((frame.x, frame.y + y), clicks)));
        assert_eq!(click(40.0, 1), []);
        assert_eq!(click(39.0, 1), [UiEventKind::Select { item: 1 }]);
        assert_eq!(click(40.0, 2), []);
        assert_eq!(click(90.0, 2), []);
        assert_eq!(state(&lists).visible(of(&tree, node)).len(), 2);
    }

    #[test]
    fn rows_of_no_height_fill_the_list_as_rows_of_a_point() {
        // A template of nothing, of a trillion rows: those of a point that
        // fill 100 points, and the items about them, whatever the count.
        let template = Node::list(Node::container(vec![]), 1_000_000_000_000, (0, Vec::new()))
            .key("rows")
            .layout(view::Layout {
                height: Length::Points(100.0),
                ..view::Layout::default()
            });
        let root = Node::container(vec![template]).layout(view::Layout {
            direction: Direction::Column,
            ..view::Layout::default()
        });
        let (tree, layout, node) = laid(&root, 1.0);
        let mut lists = Lists::new();
        assert_eq!(
            told(lists.lay_out(&tree, &layout, &mut Fixed)),
            [UiEventKind::ListRange {
                first: 0,
                count: 150
            }]
        );
        let rows = of(&tree, node);
        assert_eq!(state(&lists).visible(rows).len(), 100);
        // A scroll of a thousand points, a row a point.
        lists.scroll(&tree, node, 1000.0);
        assert_eq!(state(&lists).anchor(), (1000, 0.0));
        // A row's height of under a point is refused.
        let mut small = root.clone();
        if let Kind::List(list) = &mut small.children[0].kind {
            list.row_height = Some(0.5);
        }
        assert!(Tree::new().update(&small).is_err());
        if let Kind::List(list) = &mut small.children[0].kind {
            list.row_height = Some(1.0);
        }
        assert!(Tree::new().update(&small).is_ok());
    }

    #[test]
    fn items_that_change_under_a_list_scrolled_blank_their_rows_and_ask_again() {
        // Scrolled to the row 500 of 1 000, the items about it given.
        let (mut tree, layout, node) = laid(&list(1000, 0..60, Some(20.0)), 1.0);
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut Fixed);
        lists.scroll(&tree, node, 10_000.0);
        assert_eq!(
            told(lists.lay_out(&tree, &layout, &mut Fixed)),
            [UiEventKind::ListRange {
                first: 450,
                count: 105
            }]
        );
        tree.update(&list(1000, 450..555, Some(20.0))).unwrap();
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert!(state(&lists).row(500).is_some());
        // Its plugin's data changed, a render of the items from the start:
        // the rows blank, and the items about them asked again, once.
        tree.update(&list(1000, 0..60, Some(20.0))).unwrap();
        assert_eq!(
            told(lists.lay_out(&tree, &layout, &mut Fixed)),
            [UiEventKind::ListRange {
                first: 450,
                count: 105
            }]
        );
        assert!(state(&lists).row(500).is_none());
        assert_eq!(told(lists.lay_out(&tree, &layout, &mut Fixed)), []);
        // A plugin that renders the same items again is asked no more.
        tree.update(&list(1000, 0..60, Some(20.0))).unwrap();
        assert_eq!(told(lists.lay_out(&tree, &layout, &mut Fixed)), []);
    }

    #[test]
    fn rows_of_their_own_heights_are_measured_as_they_show() {
        // The even rows 30 points high, the odd ones 14, estimated at 14.
        let (tree, layout, node) = laid(&list(1000, 0..1000, None), 1.0);
        let mut lists = Lists::new();
        lists.lay_out(&tree, &layout, &mut Fixed);
        let list = of(&tree, node);
        let tops: Vec<f32> = state(&lists)
            .visible(list)
            .iter()
            .map(|&(_, top)| top)
            .collect();
        assert_eq!(tops, [0.0, 30.0, 44.0, 74.0, 88.0]);
        // A row not shown yet takes the template's height, of an empty
        // text, 14.
        assert_eq!(state(&lists).height_of(list, 500), 14.0);
        // 40 points down: past the first row, 10 into the second.
        lists.scroll(&tree, node, 40.0);
        assert_eq!(state(&lists).anchor(), (1, 10.0));
        lists.scroll(&tree, node, -12.0);
        assert_eq!(state(&lists).anchor(), (0, 28.0));
        // At the end, the last rows measured, the last's bottom at the
        // list's.
        lists.scroll(&tree, node, 1e9);
        lists.lay_out(&tree, &layout, &mut Fixed);
        lists.scroll(&tree, node, 1e9);
        let visible = state(&lists).visible(list);
        let &(last, top) = visible.last().unwrap();
        assert_eq!(last, 999);
        assert_eq!(top + state(&lists).height_of(list, last), 100.0);
    }

    #[test]
    fn a_row_takes_its_item_s_texts_by_their_places() {
        let template = Node::container(vec![
            Node::icon("file"),
            Node::container(vec![Node::text("name"), Node::text("size")]),
        ]);
        let filled = fill(&template, &["folder".to_owned(), "Projects".to_owned()]);
        assert_eq!(filled.children[0].kind, Kind::Icon("folder".to_owned()));
        let inner = &filled.children[1].children;
        assert_eq!(inner[0].kind, Kind::Text("Projects".to_owned()));
        assert_eq!(inner[1].kind, Kind::Text("size".to_owned()));
    }

    #[test]
    fn a_list_gone_is_forgotten_and_its_selection_kept_within_its_items() {
        let (mut tree, layout, node) = laid(&list(100, 0..100, Some(20.0)), 1.0);
        let mut lists = Lists::new();
        // Before a layout, at the top; after it, at the bottom.
        lists.select(&tree, node, 80);
        assert_eq!(state(&lists).anchor(), (80, 0.0));
        lists.lay_out(&tree, &layout, &mut Fixed);
        lists.select(&tree, node, 0);
        lists.select(&tree, node, 80);
        assert_eq!(state(&lists).selected(), Some(80));
        assert_eq!(state(&lists).anchor(), (76, 0.0));
        tree.update(&list(10, 0..10, Some(20.0))).unwrap();
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert_eq!(state(&lists).selected(), Some(9));
        // A row whose item's texts change is built anew.
        let mut renamed = list(10, 0..10, Some(20.0));
        if let Kind::List(list) = &mut renamed.children[0].kind {
            list.items[7].texts = vec!["renamed".to_owned()];
        }
        tree.update(&renamed).unwrap();
        lists.lay_out(&tree, &layout, &mut Fixed);
        let row = state(&lists).row(7).unwrap();
        let text = row.tree().children(row.tree().root().unwrap())[0];
        assert_eq!(
            row.tree().kind(text),
            Some(&Kind::Text("renamed".to_owned()))
        );
        assert_eq!(state(&lists).anchor(), (5, 0.0));
        tree.update(&list(0, 0..0, Some(20.0))).unwrap();
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert_eq!(state(&lists).selected(), None);
        assert_eq!(state(&lists).visible(of(&tree, node)), []);
        tree.update(&Node::container(vec![])).unwrap();
        lists.updated(&tree);
        assert!(lists.state(&NodeKey::from("rows")).is_none());
    }

    #[test]
    fn a_wheel_scrolls_the_innermost_list_or_scroll_area() {
        // A scroll area of 50 points, of a column of 20 boxes of 10 points,
        // and a list in it.
        let boxes: Vec<Node> = (0..20)
            .map(|index| {
                Node::container(vec![])
                    .key(format!("box-{index}").as_str())
                    .layout(view::Layout {
                        height: Length::Points(10.0),
                        ..view::Layout::default()
                    })
            })
            .collect();
        let area = Node::scroll(boxes).key("area").layout(view::Layout {
            direction: Direction::Column,
            height: Length::Points(50.0),
            ..view::Layout::default()
        });
        let (tree, mut layout, node) = laid(&Node::container(vec![area]), 2.0);
        let mut lists = Lists::new();
        let child = |tree: &Tree, index: usize| tree.children(node)[index];
        let path = [tree.root().unwrap(), node, child(&tree, 3)];
        assert_eq!(scrollable(&tree, &path), Some(node));
        assert_eq!(scrollable(&tree, &path[..1]), None);
        // 30 points up the content, by a wheel's -30: the fourth box at
        // the top.
        assert!(scroll((&tree, &mut layout, &mut lists), node, -30.0));
        assert_eq!(layout.scrolled(node), 60.0);
        assert_eq!(layout.frame(&tree, child(&tree, 3)).unwrap().y, 0.0);
        // Past the content's end, its end: 150 points of 50.
        scroll((&tree, &mut layout, &mut lists), node, -1000.0);
        assert_eq!(layout.scrolled(node), 300.0);
        assert!(!layout.scroll_by(&tree, node, 10.0));
        scroll((&tree, &mut layout, &mut lists), node, 1000.0);
        assert_eq!(layout.scrolled(node), 0.0);
        // A node of neither scrolls nothing.
        assert!(!scroll(
            (&tree, &mut layout, &mut lists),
            child(&tree, 0),
            -10.0
        ));
        // A wheel's -30 over a list scrolls its rows down 30 points.
        let (tree, mut layout, rows) = laid(&list(100, 0..100, Some(20.0)), 1.0);
        lists.lay_out(&tree, &layout, &mut Fixed);
        assert!(scroll((&tree, &mut layout, &mut lists), rows, -30.0));
        assert_eq!(state(&lists).anchor(), (1, 10.0));
    }
}

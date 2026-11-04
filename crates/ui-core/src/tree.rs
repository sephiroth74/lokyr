//! The view tree that the host keeps of a render: a flat arena of nodes,
//! whose ids stay from one render to the next as long as their nodes do,
//! and which a new tree updates in place, diffed against the last: among a
//! node's children, a child takes the place of the one of its key, or,
//! without one, of the next of its kind in their order, so that only what
//! changed is laid out and drawn again. A node's key is unique in its whole
//! tree, as the events tell a node by its key alone, which [`Tree::by_key`]
//! finds. A tree of a key twice, of children under a node that has none,
//! of a node that holds state without its key, too deep, or of numbers
//! that are none, changes nothing, the last staying.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::{Discriminant, discriminant};

use lokyr_plugin_sdk::view::{Key, Kind, Layout, Length, Node, Role, Style};

/// A node of the arena: its slot, and the slot's generation, which a node
/// taken away ends, so that its id finds no other node in its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    index: u32,
    generation: u32,
}

impl NodeId {
    /// Its slot in the arena.
    pub(crate) fn index(self) -> u32 {
        self.index
    }

    /// Its slot's generation.
    pub(crate) fn generation(self) -> u32 {
        self.generation
    }
}

/// What an update changed, by the nodes' ids, each in the new tree's order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The nodes that the new tree adds, a parent before its children.
    pub added: Vec<NodeId>,
    /// The nodes that it takes away, a parent before its children.
    pub removed: Vec<NodeId>,
    /// The nodes kept whose own properties changed, their kind's or their
    /// layout's.
    pub changed: Vec<NodeId>,
    /// The nodes kept whose children changed: added, taken away, or moved
    /// among them.
    pub rearranged: Vec<NodeId>,
    /// The nodes kept whose style alone changed, whether they take the
    /// keyboard's focus, their tooltip, or what accessibility calls them or
    /// takes them for, which draw again without being laid out again.
    pub restyled: Vec<NodeId>,
}

impl Changes {
    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && self.rearranged.is_empty()
            && self.restyled.is_empty()
    }
}

/// Why a tree is refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    /// Two nodes of the same key, anywhere in the tree.
    SameKey(Key),
    /// A node that holds state, or takes the keyboard's focus, without its
    /// key.
    NoKey,
    /// Children under a node that has none.
    Children,
    /// Nodes nested past [`DEEPEST`] levels.
    TooDeep,
    /// A number of a node's layout or style that is not one, infinite, or
    /// out of its range, as a negative padding, or a list's items past its
    /// count: the property's name.
    Number(&'static str),
    /// A list's row template of a node other than a container, a text, an
    /// icon, a spacer or a separator, or of a key, a focus or a tooltip,
    /// which a row of no node of the tree cannot have.
    Template,
}

/// The levels of nodes that a tree nests at most, the root's the first:
/// past them, its diff, its layout and its paint, which recurse, would take
/// more of a thread's stack than an interface needs, whatever a plugin
/// sends.
pub const DEEPEST: usize = 64;

/// A node as the arena keeps it.
#[derive(Debug)]
struct Entry {
    key: Option<Key>,
    kind: Kind,
    layout: Layout,
    style: Style,
    focusable: bool,
    tooltip: Option<String>,
    label: Option<String>,
    role: Option<Role>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

/// A slot of the arena, of a node or free.
#[derive(Debug)]
struct Slot {
    generation: u32,
    entry: Option<Entry>,
}

/// A view tree, as a flat arena of nodes.
#[derive(Debug, Default)]
pub struct Tree {
    slots: Vec<Slot>,
    free: Vec<u32>,
    root: Option<NodeId>,
    /// The node of each key.
    keys: HashMap<Key, NodeId>,
    /// The updates that it took.
    revision: u64,
}

impl Tree {
    /// An empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes `root` in place of the tree before, diffed against it: the
    /// nodes kept keep their ids. A tree that is not valid changes nothing.
    pub fn update(&mut self, root: &Node) -> Result<Changes, Invalid> {
        valid(root)?;
        self.revision = self.revision.wrapping_add(1);
        let mut changes = Changes::default();
        let id = match self.root {
            Some(old) => self.reconcile(old, root, None, &mut changes),
            None => self.insert(root, None, &mut changes),
        };
        self.root = Some(id);
        Ok(changes)
    }

    /// How many updates it took, which tells a layout of a new render from
    /// another of the same, as a resize's.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The root, if the tree has one.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// How many nodes the tree has.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// Whether the tree has no node.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The kind of `id`, if the tree has it.
    pub fn kind(&self, id: NodeId) -> Option<&Kind> {
        self.entry(id).map(|entry| &entry.kind)
    }

    /// The key of `id`, if the tree has it and it has one.
    pub fn key(&self, id: NodeId) -> Option<&Key> {
        self.entry(id)?.key.as_ref()
    }

    /// The layout of `id`, if the tree has it.
    pub fn layout(&self, id: NodeId) -> Option<&Layout> {
        self.entry(id).map(|entry| &entry.layout)
    }

    /// The style of `id`, if the tree has it.
    pub fn style(&self, id: NodeId) -> Option<&Style> {
        self.entry(id).map(|entry| &entry.style)
    }

    /// Whether `id` takes the keyboard's focus: `false` if the tree lacks
    /// it.
    pub fn focusable(&self, id: NodeId) -> bool {
        self.entry(id)
            .is_some_and(|entry| entry.focusable || entry.kind.takes_focus())
    }

    /// The tooltip of `id`, if it has one.
    pub fn tooltip(&self, id: NodeId) -> Option<&str> {
        self.entry(id).and_then(|entry| entry.tooltip.as_deref())
    }

    /// What accessibility calls `id`, if its node says.
    pub fn label(&self, id: NodeId) -> Option<&str> {
        self.entry(id).and_then(|entry| entry.label.as_deref())
    }

    /// What accessibility takes `id` for, if its node says.
    pub fn role(&self, id: NodeId) -> Option<Role> {
        self.entry(id).and_then(|entry| entry.role)
    }

    /// The node in the slot `index`, if one is.
    pub(crate) fn at(&self, index: u32) -> Option<NodeId> {
        let slot = self.slots.get(index as usize)?;
        slot.entry.as_ref().map(|_| NodeId {
            index,
            generation: slot.generation,
        })
    }

    /// The node of `key`, if the tree has one.
    pub fn by_key(&self, key: &Key) -> Option<NodeId> {
        self.keys.get(key).copied()
    }

    /// The parent of `id`, if the tree has it and it is not the root.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.entry(id)?.parent
    }

    /// The children of `id`, in their order; none if the tree lacks it.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.entry(id).map_or(&[], |entry| &entry.children)
    }

    fn entry(&self, id: NodeId) -> Option<&Entry> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.entry.as_ref()
    }

    fn entry_mut(&mut self, id: NodeId) -> Option<&mut Entry> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.entry.as_mut()
    }

    /// Diffs `old`'s subtree against `new`: the node stays, its properties
    /// and its children updated, where `new` is of its key and its kind;
    /// else it goes, and `new` takes its place. The id of the node in its
    /// place.
    fn reconcile(
        &mut self,
        old: NodeId,
        new: &Node,
        parent: Option<NodeId>,
        changes: &mut Changes,
    ) -> NodeId {
        let Some(entry) = self.entry_mut(old) else {
            return self.insert(new, parent, changes);
        };
        if entry.key != new.key || discriminant(&entry.kind) != discriminant(&new.kind) {
            self.remove(old, changes);
            return self.insert(new, parent, changes);
        }
        if entry.kind != new.kind || entry.layout != new.layout {
            entry.kind = new.kind.clone();
            entry.layout = new.layout;
            changes.changed.push(old);
        }
        if entry.style != new.style
            || entry.focusable != new.focusable
            || entry.tooltip != new.tooltip
            || entry.label != new.label
            || entry.role != new.role
        {
            entry.style = new.style.clone();
            entry.focusable = new.focusable;
            entry.tooltip.clone_from(&new.tooltip);
            entry.label.clone_from(&new.label);
            entry.role = new.role;
            changes.restyled.push(old);
        }
        self.reconcile_children(old, &new.children, changes);
        old
    }

    /// Diffs `parent`'s children against `new`: a child of a key takes the
    /// place of the old one of its key, one without the place of the next
    /// old one of its kind without a key; the old ones left go.
    fn reconcile_children(&mut self, parent: NodeId, new: &[Node], changes: &mut Changes) {
        let old: Vec<NodeId> = self.children(parent).to_vec();
        let mut keyed: HashMap<Key, NodeId> = HashMap::new();
        let mut unkeyed: HashMap<Discriminant<Kind>, VecDeque<NodeId>> = HashMap::new();
        for &child in &old {
            let Some(entry) = self.entry(child) else {
                continue;
            };
            match &entry.key {
                Some(key) => {
                    keyed.insert(key.clone(), child);
                }
                None => unkeyed
                    .entry(discriminant(&entry.kind))
                    .or_default()
                    .push_back(child),
            }
        }
        let mut next = Vec::with_capacity(new.len());
        for child in new {
            let matched = match &child.key {
                Some(key) => keyed.remove(key),
                None => unkeyed
                    .get_mut(&discriminant(&child.kind))
                    .and_then(VecDeque::pop_front),
            };
            next.push(match matched {
                Some(old) => self.reconcile(old, child, Some(parent), changes),
                None => self.insert(child, Some(parent), changes),
            });
        }
        let kept: HashSet<NodeId> = next.iter().copied().collect();
        for &child in &old {
            if !kept.contains(&child) && self.entry(child).is_some() {
                self.remove(child, changes);
            }
        }
        if next != old {
            changes.rearranged.push(parent);
        }
        if let Some(entry) = self.entry_mut(parent) {
            entry.children = next;
        }
    }

    /// Adds `node`'s subtree under `parent`, the id of its root.
    fn insert(&mut self, node: &Node, parent: Option<NodeId>, changes: &mut Changes) -> NodeId {
        let entry = Entry {
            key: node.key.clone(),
            kind: node.kind.clone(),
            layout: node.layout,
            style: node.style.clone(),
            focusable: node.focusable,
            tooltip: node.tooltip.clone(),
            label: node.label.clone(),
            role: node.role,
            parent,
            children: Vec::new(),
        };
        let id = match self.free.pop() {
            Some(index) => {
                let slot = &mut self.slots[index as usize];
                slot.entry = Some(entry);
                NodeId {
                    index,
                    generation: slot.generation,
                }
            }
            None => {
                // An arena past `u32::MAX` nodes would hold no render.
                let index = u32::try_from(self.slots.len()).expect("fewer than 2³² nodes");
                self.slots.push(Slot {
                    generation: 0,
                    entry: Some(entry),
                });
                NodeId {
                    index,
                    generation: 0,
                }
            }
        };
        changes.added.push(id);
        if let Some(key) = &node.key {
            self.keys.insert(key.clone(), id);
        }
        let children: Vec<NodeId> = node
            .children
            .iter()
            .map(|child| self.insert(child, Some(id), changes))
            .collect();
        if let Some(entry) = self.entry_mut(id) {
            entry.children = children;
        }
        id
    }

    /// Takes `id`'s subtree away, its slots free for other nodes.
    fn remove(&mut self, id: NodeId, changes: &mut Changes) {
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return;
        };
        if slot.generation != id.generation {
            return;
        }
        let Some(entry) = slot.entry.take() else {
            return;
        };
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(id.index);
        changes.removed.push(id);
        // Its key may be another node's already, of the same render.
        if let Some(key) = &entry.key
            && self.keys.get(key) == Some(&id)
        {
            self.keys.remove(key);
        }
        for child in entry.children {
            self.remove(child, changes);
        }
    }
}

/// Whether `node`'s tree is one that the arena takes: its keys unique in
/// it, its nodes that hold state or take the focus of a key, children only
/// under the nodes that have them, [`DEEPEST`] levels at most, and its
/// numbers finite and in their ranges.
fn valid(node: &Node) -> Result<(), Invalid> {
    valid_at(node, 1, &mut HashSet::new())
}

/// [`valid`] of `node`, at the level `depth` of its tree, of the keys
/// `seen` before it.
fn valid_at<'a>(node: &'a Node, depth: usize, seen: &mut HashSet<&'a Key>) -> Result<(), Invalid> {
    if depth > DEEPEST {
        return Err(Invalid::TooDeep);
    }
    numbers(node)?;
    if let Kind::List(list) = &node.kind {
        template(&list.row, depth + 1)?;
        let given = list.first.checked_add(list.items.len());
        if given.is_none_or(|given| given > list.count) {
            return Err(Invalid::Number("count"));
        }
        if list
            .row_height
            .is_some_and(|height| !(height.is_finite() && height >= crate::list::SHORTEST_ROW))
        {
            return Err(Invalid::Number("row-height"));
        }
    }
    if let Kind::Tabs(tabs) = &node.kind
        && !tabs.tabs.is_empty()
        && tabs.chosen >= tabs.tabs.len()
    {
        return Err(Invalid::Number("chosen"));
    }
    if let Kind::Progress(progress) = &node.kind {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        let checks = [
            ("value", (0.0..=1.0).contains(&progress.value)),
            (
                "radius",
                progress
                    .radius
                    .is_none_or(|radius| radius.is_finite() && radius >= 0.0),
            ),
            ("thickness", positive(progress.thickness)),
            (
                "segment",
                positive(progress.segment) && progress.segment <= 1.0,
            ),
            (
                "period",
                progress
                    .period
                    .is_none_or(|period| positive(period) && period <= 60.0),
            ),
        ];
        if let Some((name, _)) = checks.into_iter().find(|(_, holds)| !holds) {
            return Err(Invalid::Number(name));
        }
    }
    if (node.kind.holds_state() || node.focusable || node.tooltip.is_some()) && node.key.is_none() {
        return Err(Invalid::NoKey);
    }
    if !node.kind.has_children() && !node.children.is_empty() {
        return Err(Invalid::Children);
    }
    if let Some(key) = &node.key
        && !seen.insert(key)
    {
        return Err(Invalid::SameKey(key.clone()));
    }
    for child in &node.children {
        valid_at(child, depth + 1, seen)?;
    }
    Ok(())
}

/// Whether `row`, a list's row template at the level `depth`, is one that a
/// row takes: of containers, texts, icons, spacers and separators, of no
/// key, focus or tooltip, [`DEEPEST`] levels at most, its numbers finite
/// and in their ranges.
fn template(row: &Node, depth: usize) -> Result<(), Invalid> {
    if depth > DEEPEST {
        return Err(Invalid::TooDeep);
    }
    numbers(row)?;
    let plain = matches!(
        row.kind,
        Kind::Container | Kind::Text(_) | Kind::Icon(_) | Kind::Spacer | Kind::Separator
    );
    if !plain || row.key.is_some() || row.focusable || row.tooltip.is_some() {
        return Err(Invalid::Template);
    }
    if !row.kind.has_children() && !row.children.is_empty() {
        return Err(Invalid::Children);
    }
    row.children
        .iter()
        .try_for_each(|child| template(child, depth + 1))
}

/// Whether `node`'s numbers are finite and in their ranges: its lengths,
/// paddings, gap, grow, radius, border and blur not negative, its fractions
/// from 0 to 1, its shadow's offsets of any sign.
fn numbers(node: &Node) -> Result<(), Invalid> {
    let layout = &node.layout;
    let style = &node.style;
    let length = |length: Length| match length {
        Length::Auto => true,
        Length::Points(points) => points.is_finite() && points >= 0.0,
        Length::Fraction(fraction) => (0.0..=1.0).contains(&fraction),
    };
    let size = |value: f32| value.is_finite() && value >= 0.0;
    let padding = &layout.padding;
    let checks = [
        ("gap", size(layout.gap)),
        (
            "padding",
            [padding.top, padding.right, padding.bottom, padding.left]
                .into_iter()
                .all(size),
        ),
        ("width", length(layout.width)),
        ("height", length(layout.height)),
        ("grow", size(layout.grow)),
        ("radius", size(style.radius)),
        (
            "opacity",
            style
                .opacity
                .is_none_or(|opacity| (0.0..=1.0).contains(&opacity)),
        ),
        (
            "transition",
            style
                .transition
                .is_none_or(|transition| (0.0..=10.0).contains(&transition.duration)),
        ),
        (
            "border",
            style
                .border
                .as_ref()
                .is_none_or(|border| size(border.width)),
        ),
        (
            "shadow",
            style.shadow.as_ref().is_none_or(|shadow| {
                shadow.x.is_finite() && shadow.y.is_finite() && size(shadow.blur)
            }),
        ),
    ];
    match checks.into_iter().find(|(_, holds)| !holds) {
        Some((name, _)) => Err(Invalid::Number(name)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::Direction;

    use super::*;

    /// A bar of an icon, a title, room and a status, the title and the
    /// status keyed.
    fn bar(title: &str, status: &str) -> Node {
        Node::container(vec![
            Node::icon("terminal"),
            Node::text(title).key("title"),
            Node::spacer(),
            Node::text(status).key("status"),
        ])
    }

    #[test]
    fn a_first_tree_adds_every_node_and_the_same_again_changes_nothing() {
        let mut tree = Tree::new();
        let changes = tree.update(&bar("~", "ready")).unwrap();
        assert_eq!(changes.added.len(), 5);
        assert_eq!(changes.added[0], tree.root().unwrap());
        assert!(changes.removed.is_empty() && changes.changed.is_empty());
        assert_eq!(tree.len(), 5);
        // The same tree: nothing.
        assert!(tree.update(&bar("~", "ready")).unwrap().is_empty());
    }

    #[test]
    fn a_text_that_changes_is_the_one_change() {
        let mut tree = Tree::new();
        tree.update(&bar("~", "ready")).unwrap();
        let root = tree.root().unwrap();
        let status = tree.children(root)[3];
        let changes = tree.update(&bar("~", "busy")).unwrap();
        assert_eq!(
            changes,
            Changes {
                changed: vec![status],
                ..Changes::default()
            }
        );
        assert_eq!(tree.kind(status), Some(&Kind::Text("busy".to_owned())));
        // A layout's change too, its id kept.
        let mut wider = bar("~", "busy");
        wider.layout.direction = Direction::Column;
        let changes = tree.update(&wider).unwrap();
        assert_eq!(changes.changed, [root]);
        assert_eq!(tree.root(), Some(root));
        // A style's change alone: restyled, not changed.
        let mut styled = wider.clone();
        styled.style.background = Some("panel.bg".into());
        let changes = tree.update(&styled).unwrap();
        assert!(changes.changed.is_empty());
        assert_eq!(changes.restyled, [root]);
        assert!(!changes.is_empty());
        assert_eq!(
            tree.style(root).and_then(|style| style.background.clone()),
            Some("panel.bg".into())
        );
    }

    #[test]
    fn a_child_added_keeps_its_siblings_and_rearranges_its_parent() {
        let mut tree = Tree::new();
        tree.update(&bar("~", "ready")).unwrap();
        let root = tree.root().unwrap();
        let before = tree.children(root).to_vec();
        let mut more = bar("~", "ready");
        more.children.insert(1, Node::text("·").key("dot"));
        let changes = tree.update(&more).unwrap();
        let dot = tree.children(root)[1];
        assert_eq!(
            changes,
            Changes {
                added: vec![dot],
                rearranged: vec![root],
                ..Changes::default()
            }
        );
        assert_eq!(tree.key(dot), Some(&Key::from("dot")));
        assert_eq!(tree.parent(dot), Some(root));
        // The others kept their ids.
        let after: Vec<NodeId> = tree.children(root).to_vec();
        assert_eq!([after[0], after[2], after[3], after[4]], before[..]);
    }

    #[test]
    fn keyed_children_that_move_keep_their_ids() {
        let mut tree = Tree::new();
        tree.update(&bar("~", "ready")).unwrap();
        let root = tree.root().unwrap();
        let [icon, title, spacer, status] = tree.children(root).try_into().unwrap();
        let swapped = Node::container(vec![
            Node::icon("terminal"),
            Node::text("ready").key("status"),
            Node::spacer(),
            Node::text("~").key("title"),
        ]);
        let changes = tree.update(&swapped).unwrap();
        // Moved, and their texts as they were: no node changed but their
        // parent's order.
        assert_eq!(
            changes,
            Changes {
                rearranged: vec![root],
                ..Changes::default()
            }
        );
        assert_eq!(tree.children(root), [icon, status, spacer, title]);
    }

    #[test]
    fn nodes_without_keys_keep_their_place_by_their_kind() {
        let mut tree = Tree::new();
        let root_of = |children| Node::container(children);
        tree.update(&root_of(vec![Node::text("a"), Node::icon("x")]))
            .unwrap();
        let root = tree.root().unwrap();
        let [text, icon] = tree.children(root).try_into().unwrap();
        // The icon first now, of another name: each of its kind's place.
        let changes = tree
            .update(&root_of(vec![Node::icon("y"), Node::text("a")]))
            .unwrap();
        assert_eq!(
            changes,
            Changes {
                changed: vec![icon],
                rearranged: vec![root],
                ..Changes::default()
            }
        );
        assert_eq!(tree.children(root), [icon, text]);
    }

    #[test]
    fn a_child_taken_goes_with_its_subtree_and_its_ids_find_nothing() {
        let mut tree = Tree::new();
        let nested = Node::container(vec![
            Node::container(vec![Node::text("in"), Node::icon("x")]).key("group"),
            Node::text("out").key("out"),
        ]);
        tree.update(&nested).unwrap();
        let root = tree.root().unwrap();
        let group = tree.children(root)[0];
        let inner = tree.children(group).to_vec();
        let changes = tree
            .update(&Node::container(vec![Node::text("out").key("out")]))
            .unwrap();
        assert_eq!(
            changes,
            Changes {
                removed: vec![group, inner[0], inner[1]],
                rearranged: vec![root],
                ..Changes::default()
            }
        );
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.kind(group), None);
        // A slot taken again finds the new node, not the old id's.
        tree.update(&nested).unwrap();
        assert_eq!(tree.kind(group), None);
        assert_eq!(tree.len(), 5);
    }

    #[test]
    fn a_keyed_node_of_another_kind_is_another_node() {
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![Node::text("x").key("a")]))
            .unwrap();
        let root = tree.root().unwrap();
        let old = tree.children(root)[0];
        let changes = tree
            .update(&Node::container(vec![Node::icon("x").key("a")]))
            .unwrap();
        let new = tree.children(root)[0];
        assert_ne!(old, new);
        assert_eq!(changes.removed, [old]);
        assert_eq!(changes.added, [new]);
        assert_eq!(changes.rearranged, [root]);
    }

    #[test]
    fn a_tree_that_is_not_valid_changes_nothing() {
        let mut tree = Tree::new();
        tree.update(&bar("~", "ready")).unwrap();
        let root = tree.root().unwrap();
        let twice = Node::container(vec![Node::text("a").key("k"), Node::icon("b").key("k")]);
        assert_eq!(tree.update(&twice), Err(Invalid::SameKey(Key::from("k"))));
        let mut under_text = Node::text("a");
        under_text.children.push(Node::text("b"));
        assert_eq!(
            tree.update(&Node::container(vec![under_text])),
            Err(Invalid::Children)
        );
        // A node that takes the focus needs its key, which keeps the focus.
        let unkeyed = Node::container(vec![Node::container(vec![]).focusable()]);
        assert_eq!(tree.update(&unkeyed), Err(Invalid::NoKey));
        // The last valid tree stays.
        assert_eq!(tree.root(), Some(root));
        assert_eq!(tree.len(), 5);
        assert!(tree.update(&bar("~", "ready")).unwrap().is_empty());
        // Whether a node takes the focus is restyled alone.
        let mut focusing = bar("~", "ready");
        focusing.children[0] = focusing.children[0].clone().key("icon").focusable();
        let mut keyed = bar("~", "ready");
        keyed.children[0] = keyed.children[0].clone().key("icon");
        tree.update(&keyed).unwrap();
        let icon = tree.children(root)[0];
        assert!(!tree.focusable(icon));
        let changes = tree.update(&focusing).unwrap();
        assert_eq!(
            changes,
            Changes {
                restyled: vec![icon],
                ..Changes::default()
            }
        );
        assert!(tree.focusable(icon));
        // A tooltip needs its key; one that changes restyles its node.
        let unkeyed = Node::container(vec![Node::icon("add").tooltip("New Tab")]);
        assert_eq!(tree.update(&unkeyed), Err(Invalid::NoKey));
        let tip =
            |tooltip: &str| Node::container(vec![Node::icon("add").key("add").tooltip(tooltip)]);
        tree.update(&tip("New Tab")).unwrap();
        let add = tree.children(tree.root().unwrap())[0];
        assert_eq!(tree.tooltip(add), Some("New Tab"));
        assert_eq!(
            tree.update(&tip("New Window")).unwrap(),
            Changes {
                restyled: vec![add],
                ..Changes::default()
            }
        );
        assert_eq!(tree.tooltip(add), Some("New Window"));
        // So does what accessibility calls it, or takes it for.
        let labelled = |label: &str, role: Role| {
            Node::container(vec![
                Node::icon("add")
                    .key("add")
                    .tooltip("New Window")
                    .label(label)
                    .role(role),
            ])
        };
        tree.update(&labelled("New", Role::Image)).unwrap();
        assert_eq!(
            (tree.label(add), tree.role(add)),
            (Some("New"), Some(Role::Image))
        );
        for (label, role) in [("Add", Role::Image), ("Add", Role::Group)] {
            assert_eq!(
                tree.update(&labelled(label, role)).unwrap(),
                Changes {
                    restyled: vec![add],
                    ..Changes::default()
                }
            );
            assert_eq!((tree.label(add), tree.role(add)), (Some(label), Some(role)));
        }
        // A button and a toggle need their keys, and take the focus unless
        // they are disabled.
        for node in [Node::button("Replace"), Node::toggle("regex", false)] {
            assert_eq!(
                tree.update(&Node::container(vec![node])),
                Err(Invalid::NoKey)
            );
        }
        let disabled = Node::new(Kind::Button(lokyr_plugin_sdk::view::Button {
            label: "Replace All".to_owned(),
            disabled: true,
            ..Default::default()
        }))
        .key("all");
        tree.update(&Node::container(vec![
            Node::button("Replace").key("replace"),
            disabled,
            Node::separator(),
        ]))
        .unwrap();
        let children = tree.children(tree.root().unwrap()).to_vec();
        assert_eq!(
            children
                .iter()
                .map(|&id| tree.focusable(id))
                .collect::<Vec<_>>(),
            [true, false, false]
        );
        // A list needs its key, its items within its count, a height of its
        // rows above 0, and a template of plain nodes.
        let list = |row: Node, count: usize, height: Option<f32>| {
            let mut node = Node::list(row, count, (1, vec![Default::default(); 2])).key("rows");
            if let Kind::List(list) = &mut node.kind {
                list.row_height = height;
            }
            Node::container(vec![node])
        };
        let plain = || Node::container(vec![Node::icon("file"), Node::text("name")]);
        assert!(tree.update(&list(plain(), 3, Some(20.0))).is_ok());
        assert_eq!(
            tree.update(&list(plain(), 2, None)),
            Err(Invalid::Number("count"))
        );
        for height in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                tree.update(&list(plain(), 3, Some(height))),
                Err(Invalid::Number("row-height"))
            );
        }
        for row in [
            Node::container(vec![Node::button("Go").key("go")]),
            Node::container(vec![Node::text("a").key("a")]),
            Node::container(vec![Node::text("a").tooltip("tip")]),
            Node::container(vec![]).focusable(),
        ] {
            assert_eq!(tree.update(&list(row, 3, None)), Err(Invalid::Template));
        }
        let mut under_text = Node::text("a");
        under_text.children.push(Node::text("b"));
        assert_eq!(
            tree.update(&list(Node::container(vec![under_text]), 3, None)),
            Err(Invalid::Children)
        );
        let deep = (0..DEEPEST).fold(Node::text("leaf"), |inner, _| Node::container(vec![inner]));
        assert_eq!(tree.update(&list(deep, 3, None)), Err(Invalid::TooDeep));
        let unkeyed = Node::container(vec![Node::list(plain(), 0, (0, vec![]))]);
        assert_eq!(tree.update(&unkeyed), Err(Invalid::NoKey));
        assert_eq!(
            tree.update(&Node::container(vec![Node::scroll(vec![])])),
            Err(Invalid::NoKey)
        );
        // An opacity from 0 to 1, a transition of 10 seconds at most.
        let styled = |style: lokyr_plugin_sdk::view::Style| {
            Node::container(vec![Node::container(vec![]).style(style)])
        };
        for (style, name) in [
            (
                lokyr_plugin_sdk::view::Style {
                    opacity: Some(1.5),
                    ..Default::default()
                },
                "opacity",
            ),
            (
                lokyr_plugin_sdk::view::Style {
                    opacity: Some(f32::NAN),
                    ..Default::default()
                },
                "opacity",
            ),
            (
                lokyr_plugin_sdk::view::Style {
                    transition: Some(lokyr_plugin_sdk::view::Transition {
                        duration: 11.0,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                "transition",
            ),
            (
                lokyr_plugin_sdk::view::Style {
                    transition: Some(lokyr_plugin_sdk::view::Transition {
                        duration: -1.0,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                "transition",
            ),
        ] {
            assert_eq!(tree.update(&styled(style)), Err(Invalid::Number(name)));
        }
        assert!(
            tree.update(&styled(lokyr_plugin_sdk::view::Style {
                opacity: Some(0.0),
                transition: Some(Default::default()),
                ..Default::default()
            }))
            .is_ok()
        );
        // A progress's numbers within their ranges.
        use lokyr_plugin_sdk::view::Progress;
        let progress = |change: fn(&mut Progress)| {
            let mut progress = Progress::default();
            change(&mut progress);
            Node::container(vec![Node::new(Kind::Progress(progress))])
        };
        assert!(tree.update(&progress(|_| {})).is_ok());
        type Change = fn(&mut Progress);
        let cases: [(&str, Change); 8] = [
            ("value", |p| p.value = 1.5),
            ("value", |p| p.value = f32::NAN),
            ("radius", |p| p.radius = Some(-1.0)),
            ("thickness", |p| p.thickness = 0.0),
            ("segment", |p| p.segment = 0.0),
            ("segment", |p| p.segment = 1.5),
            ("period", |p| p.period = Some(0.0)),
            ("period", |p| p.period = Some(61.0)),
        ];
        for (name, change) in cases {
            assert_eq!(tree.update(&progress(change)), Err(Invalid::Number(name)));
        }
        // A field holds its state by its key, and takes the focus of its
        // own.
        let field = Node::container(vec![Node::field("Find")]);
        assert_eq!(tree.update(&field), Err(Invalid::NoKey));
        tree.update(&Node::container(vec![Node::field("Find").key("find")]))
            .unwrap();
        assert!(tree.focusable(tree.children(tree.root().unwrap())[0]));
    }

    #[test]
    fn a_tree_too_deep_or_of_numbers_that_are_none_is_refused() {
        use lokyr_plugin_sdk::view::{Border, Edges, Shadow};
        let mut tree = Tree::new();
        tree.update(&bar("~", "ready")).unwrap();
        let root = tree.root();
        // A chain of `DEEPEST` levels, then one more.
        let chain = |levels: usize| {
            (1..levels).fold(Node::text("leaf"), |inner, _| Node::container(vec![inner]))
        };
        let mut deep = Tree::new();
        assert!(deep.update(&chain(DEEPEST)).is_ok());
        assert_eq!(tree.update(&chain(DEEPEST + 1)), Err(Invalid::TooDeep));
        assert_eq!(tree.update(&chain(5_000)), Err(Invalid::TooDeep));
        // Numbers that are none, infinite or out of their ranges.
        let laid = |layout: Layout| Node::container(vec![Node::spacer().layout(layout)]);
        let styled = |style: Style| Node::container(vec![Node::spacer().style(style)]);
        let refused = [
            (
                laid(Layout {
                    gap: f32::NAN,
                    ..Layout::default()
                }),
                "gap",
            ),
            (
                laid(Layout {
                    padding: Edges {
                        left: -1.0,
                        ..Edges::default()
                    },
                    ..Layout::default()
                }),
                "padding",
            ),
            (
                laid(Layout {
                    width: Length::Points(f32::INFINITY),
                    ..Layout::default()
                }),
                "width",
            ),
            (
                laid(Layout {
                    height: Length::Fraction(1.5),
                    ..Layout::default()
                }),
                "height",
            ),
            (
                laid(Layout {
                    grow: f32::NAN,
                    ..Layout::default()
                }),
                "grow",
            ),
            (
                styled(Style {
                    radius: -2.0,
                    ..Style::default()
                }),
                "radius",
            ),
            (
                styled(Style {
                    border: Some(Border {
                        color: "separator".into(),
                        width: f32::INFINITY,
                    }),
                    ..Style::default()
                }),
                "border",
            ),
            (
                styled(Style {
                    shadow: Some(Shadow {
                        blur: f32::NAN,
                        ..Shadow::default()
                    }),
                    ..Style::default()
                }),
                "shadow",
            ),
        ];
        for (node, name) in refused {
            assert_eq!(tree.update(&node), Err(Invalid::Number(name)), "{name}");
        }
        // The last valid tree stays.
        assert_eq!(tree.root(), root);
        // A shadow moved up and left, and a huge but finite border, read.
        let fine = styled(Style {
            border: Some(Border {
                color: "separator".into(),
                width: 1e30,
            }),
            shadow: Some(Shadow {
                x: -4.0,
                y: -2.0,
                ..Shadow::default()
            }),
            ..Style::default()
        });
        assert!(tree.update(&fine).is_ok());
    }

    #[test]
    fn a_key_is_unique_in_the_whole_tree_and_found_by_it() {
        let tab = |key: &str| Node::container(vec![Node::icon("close").key("close")]).key(key);
        let mut tree = Tree::new();
        // Two tabs' children `close`: two nodes of one key, in two branches.
        assert_eq!(
            tree.update(&Node::container(vec![tab("a"), tab("b")])),
            Err(Invalid::SameKey(Key::from("close")))
        );
        let tab = |key: &str| {
            Node::container(vec![
                Node::icon("close").key(format!("{key}.close").as_str()),
            ])
            .key(key)
        };
        tree.update(&Node::container(vec![tab("a"), tab("b")]))
            .unwrap();
        let close = tree.by_key(&Key::from("a.close")).unwrap();
        assert_eq!(tree.key(close), Some(&Key::from("a.close")));
        // Moved to the other tab: another node, which its key finds.
        let moved = Node::container(vec![
            Node::container(vec![]).key("a"),
            Node::container(vec![
                Node::icon("close").key("b.close"),
                Node::icon("close").key("a.close"),
            ])
            .key("b"),
        ]);
        tree.update(&moved).unwrap();
        let again = tree.by_key(&Key::from("a.close")).unwrap();
        assert_ne!(again, close);
        assert_eq!(tree.parent(again), tree.by_key(&Key::from("b")));
        // Taken away: found no more.
        tree.update(&Node::container(vec![])).unwrap();
        assert_eq!(tree.by_key(&Key::from("a.close")), None);
        assert_eq!(tree.by_key(&Key::from("a")), None);
    }
}

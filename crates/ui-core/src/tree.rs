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

use lokyr_plugin_sdk::view::{Key, Kind, Layout, Length, Node, Style};

/// A node of the arena: its slot, and the slot's generation, which a node
/// taken away ends, so that its id finds no other node in its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId {
    index: u32,
    generation: u32,
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
    /// The nodes kept whose style alone changed, or whether they take the
    /// keyboard's focus, which draw again without being laid out again.
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
    /// out of its range, as a negative padding: the property's name.
    Number(&'static str),
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
        let mut changes = Changes::default();
        let id = match self.root {
            Some(old) => self.reconcile(old, root, None, &mut changes),
            None => self.insert(root, None, &mut changes),
        };
        self.root = Some(id);
        Ok(changes)
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
        if entry.style != new.style || entry.focusable != new.focusable {
            entry.style = new.style.clone();
            entry.focusable = new.focusable;
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
    if (node.kind.holds_state() || node.focusable) && node.key.is_none() {
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

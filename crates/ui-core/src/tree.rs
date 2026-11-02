//! The view tree that the host keeps of a render: a flat arena of nodes,
//! whose ids stay from one render to the next as long as their nodes do,
//! and which a new tree updates in place, diffed against the last: among a
//! node's children, a child takes the place of the one of its key, or,
//! without one, of the next of its kind in their order, so that only what
//! changed is laid out and drawn again. A tree of keys twice among
//! siblings, of children under a node that has none, or of a node that
//! holds state without its key, changes nothing, the last staying.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::{Discriminant, discriminant};

use lokyr_plugin_sdk::view::{Key, Kind, Layout, Node, Style};

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
    /// The nodes kept whose style alone changed, which draw again without
    /// being laid out again.
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
    /// Two siblings of the same key.
    SameKey(Key),
    /// A node that holds state, without its key.
    NoKey,
    /// Children under a node that has none.
    Children,
}

/// A node as the arena keeps it.
#[derive(Debug)]
struct Entry {
    key: Option<Key>,
    kind: Kind,
    layout: Layout,
    style: Style,
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
        if entry.style != new.style {
            entry.style = new.style.clone();
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
        for child in entry.children {
            self.remove(child, changes);
        }
    }
}

/// Whether `node`'s tree is one that the arena takes: its siblings of
/// different keys, its nodes that hold state of a key, and children only
/// under the nodes that have them.
fn valid(node: &Node) -> Result<(), Invalid> {
    if node.kind.holds_state() && node.key.is_none() {
        return Err(Invalid::NoKey);
    }
    if !node.kind.has_children() && !node.children.is_empty() {
        return Err(Invalid::Children);
    }
    let mut keys = HashSet::new();
    for child in &node.children {
        if let Some(key) = &child.key
            && !keys.insert(key)
        {
            return Err(Invalid::SameKey(key.clone()));
        }
        valid(child)?;
    }
    Ok(())
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
        // The last valid tree stays.
        assert_eq!(tree.root(), Some(root));
        assert_eq!(tree.len(), 5);
        assert!(tree.update(&bar("~", "ready")).unwrap().is_empty());
    }
}

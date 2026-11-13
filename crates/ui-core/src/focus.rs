//! The keyboard's focus in a window: one element has it, the window's pane
//! or a node of a panel's view tree that takes it. Of the keys that the
//! keymap leaves, Tab and Shift-Tab move it among the panel's nodes that
//! take it, in the tree's order, from the last round to the first, and
//! Escape gives it back to where its panel took it from, a node of a panel
//! below or the pane; every other key goes to the element
//! that has it alone, and a key's release where its press went; after a
//! click on a node that takes no focus, Tab goes on from there. A panel
//! that closes with the focus gives it back there too, as a menu of the
//! find bar gives it back to the bar. A click on
//! a control, a button or a toggle, leaves the focus where it was, as
//! macOS's controls do, and Tab reaches the controls only as the system's
//! keyboard navigation reaches every control. Panels chained side by side,
//! as a sidebar and the page beside it, Tab goes through as one, from the
//! last node of one to the first of the next. Its moves
//! come as events of a focus and a blur to the nodes. The keymap's
//! contexts tell where it is:
//! `pane.focused` while the pane has it, and, of each panel shown, of its
//! plugin's namespace, `namespace.open`, and `namespace.focused` while the
//! panel has it.

use lokyr_vt_core::input::{Key, KeyAction, KeyEvent};

use lokyr_plugin_sdk::view::{Key as NodeKey, Kind, UiEventKind};

use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// What has the keyboard's focus, of a window whose panels `P` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target<P> {
    /// The window's pane.
    Pane,
    /// A node of a panel.
    Node {
        /// The panel.
        panel: P,
        /// The node.
        node: NodeId,
    },
}

/// Where a key goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Routed<P> {
    /// To the pane, which has the focus.
    Pane,
    /// To the node that has the focus.
    Node {
        /// Its panel.
        panel: P,
        /// The node.
        node: NodeId,
    },
    /// To the focus, which it moved, or nowhere, as the release of a key
    /// that moved it: no element learns of it.
    Focus,
}

/// The keyboard's focus of a window whose panels `P` names: on its pane
/// until a panel's node takes it.
#[derive(Debug)]
pub struct Focus<P> {
    focused: Option<(P, NodeId)>,
    /// The place of the node with the focus among its panel's nodes that
    /// take it, which a node taken away leaves to the one after it.
    place: usize,
    /// The keys down, and where their presses went, where their releases
    /// go.
    held: Vec<(Key, Routed<P>)>,
    /// The node of a panel last clicked that takes no focus, from which Tab
    /// goes on, until the focus moves.
    anchor: Option<(P, NodeId)>,
    /// The key of the node with the focus, by which its plugin knows it,
    /// and which the focus follows to the node that a render makes of it.
    key: Option<NodeKey>,
    /// The panel and the key of the node whose focus the events told last.
    told: Option<(P, NodeKey)>,
    /// Whether Tab reaches every control, as the system's keyboard
    /// navigation has it.
    every_control: bool,
    /// Where each panel took the focus from first since the pane last had
    /// it, a node of another panel by its key, or the pane, where it goes
    /// back as the panel closes or Escape takes it: kept as the panel takes
    /// it again from another, so that two panels never give it back to
    /// each other for good.
    came_from: Vec<(P, Option<(P, NodeKey)>)>,
    /// The panels that Tab goes through as one, in their order.
    chain: Vec<P>,
}

impl<P> Default for Focus<P> {
    fn default() -> Self {
        Self {
            focused: None,
            place: 0,
            held: Vec::new(),
            anchor: None,
            key: None,
            told: None,
            every_control: false,
            came_from: Vec::new(),
            chain: Vec::new(),
        }
    }
}

impl<P: Copy + Eq> Focus<P> {
    /// The focus on the pane.
    pub fn new() -> Self {
        Self::default()
    }

    /// What has the focus.
    pub fn target(&self) -> Target<P> {
        match self.focused {
            Some((panel, node)) => Target::Node { panel, node },
            None => Target::Pane,
        }
    }

    /// Gives the focus to `node` of `panel`, whose tree is `tree`, if it
    /// takes it: whether it does. From another panel, or the pane, the
    /// panel keeps where it took it from, unless it took it from elsewhere
    /// first since the pane had it.
    pub fn focus(&mut self, panel: P, tree: &Tree, node: NodeId) -> bool {
        let from = self.focused.zip(self.key.clone());
        if !self.take(panel, tree, node) {
            return false;
        }
        let elsewhere = from
            .as_ref()
            .is_none_or(|((focused, _), _)| *focused != panel);
        if elsewhere && !self.came_from.iter().any(|(of, _)| *of == panel) {
            self.came_from
                .push((panel, from.map(|((focused, _), key)| (focused, key))));
        }
        true
    }

    /// Gives the focus to `node` of `panel`, whose tree is `tree`, if it
    /// takes it: whether it does.
    fn take(&mut self, panel: P, tree: &Tree, node: NodeId) -> bool {
        let Some(place) = order(tree).iter().position(|&id| id == node) else {
            return false;
        };
        self.focused = Some((panel, node));
        self.key = tree.key(node).cloned();
        self.place = place;
        self.anchor = None;
        true
    }

    /// Whether Tab reaches every control, the buttons and the toggles, as
    /// the system's keyboard navigation reaches them, Keyboard navigation
    /// in macOS's settings, or the fields and the other nodes that take
    /// the focus alone, as by default.
    pub fn set_every_control(&mut self, every: bool) {
        self.every_control = every;
    }

    /// Chains `panels`, in their order, which Tab and Shift-Tab then go
    /// through as one: past the last node of one that Tab reaches, to the
    /// first of the next that has one, round from the last panel to the
    /// first. None but one are chained without.
    pub fn set_chain(&mut self, panels: &[P]) {
        if self.chain != panels {
            self.chain = panels.to_vec();
        }
    }

    /// Gives the focus to the first field of `panel` that takes it, or to
    /// its first node that takes it, as a panel that opens takes the
    /// keyboard, its text first, as macOS's dialogs: whether one does.
    pub fn first(&mut self, panel: P, tree: &Tree) -> bool {
        let order = order(tree);
        let field = order
            .iter()
            .copied()
            .find(|&id| matches!(tree.kind(id), Some(Kind::Field(_))) && self.tabs_to(tree, id));
        match field.or_else(|| order.into_iter().find(|&id| self.tabs_to(tree, id))) {
            Some(node) => self.focus(panel, tree, node),
            None => false,
        }
    }

    /// Gives the focus to the innermost node of `path` that takes it, a
    /// hit's of the pointer's press over `panel`, whose tree is `tree`:
    /// whether one does; the focus stays where it was otherwise, and Tab
    /// goes on from the topmost node of `path`, until the focus moves. A
    /// control, disabled or not, on the way leaves the focus where it was,
    /// and Tab where it went on from.
    pub fn pressed(&mut self, panel: P, tree: &Tree, path: &[NodeId]) -> bool {
        for &id in path.iter().rev() {
            if tree.kind(id).is_some_and(Kind::is_control) {
                return false;
            }
            if tree.focusable(id) {
                return self.focus(panel, tree, id);
            }
        }
        self.anchor = path.last().map(|&node| (panel, node));
        false
    }

    /// Whether Tab reaches `id` of `tree`: a node that takes the focus, a
    /// control only as the system's keyboard navigation reaches them.
    fn tabs_to(&self, tree: &Tree, id: NodeId) -> bool {
        tabs_to(tree, id, self.every_control)
    }

    /// Gives the focus back to the pane, from which the panels then take it
    /// anew.
    pub fn pane(&mut self) {
        self.focused = None;
        self.key = None;
        self.anchor = None;
        self.came_from.clear();
    }

    /// `panel` closed, the other panels' trees of `tree`: if it had the
    /// focus, the focus goes back to where it took it from, a node of
    /// another panel, by its key, or the pane, where that node is gone; the
    /// panels that took it from `panel` would give it back there too. The
    /// repeats and the releases of the keys whose presses went to it go
    /// nowhere, as Return held as it chose a menu's item.
    pub fn closed<'t>(&mut self, panel: P, tree: impl Fn(P) -> Option<&'t Tree>) {
        let from = self
            .came_from
            .iter()
            .position(|(of, _)| *of == panel)
            .and_then(|at| self.came_from.remove(at).1);
        for (_, came) in &mut self.came_from {
            if came.as_ref().is_some_and(|(of, _)| *of == panel) {
                came.clone_from(&from);
            }
        }
        for (_, routed) in &mut self.held {
            if matches!(routed, Routed::Node { panel: went, .. } if *went == panel) {
                *routed = Routed::Focus;
            }
        }
        if self.focused.is_some_and(|(focused, _)| focused == panel) {
            self.back(from, &tree);
        }
        if self.anchor.is_some_and(|(anchored, _)| anchored == panel) {
            self.anchor = None;
        }
    }

    /// Gives the focus back to `from`, a node of a panel by its key, which
    /// `tree` gives the panels' trees of, or to the pane where that node is
    /// gone or takes it no more, or of none.
    fn back<'t>(&mut self, from: Option<(P, NodeKey)>, tree: &impl Fn(P) -> Option<&'t Tree>) {
        let back = from.and_then(|(of, key)| {
            let tree = tree(of)?;
            Some((of, tree, tree.by_key(&key)?))
        });
        if !back.is_some_and(|(of, tree, node)| self.take(of, tree, node)) {
            self.pane();
        }
    }

    /// What the focus's moves since the last call did to the nodes, each
    /// with its panel, whose tree `tree` gives, by their keys, as their
    /// plugins know them: a blur for the key that had it, if its tree still
    /// has it, then a focus for the node that has it.
    pub fn events<'t>(&mut self, tree: impl Fn(P) -> Option<&'t Tree>) -> Vec<(P, NodeEvent)> {
        let now = self.focused.zip(self.key.clone());
        let told_now = now.as_ref().map(|((panel, _), key)| (*panel, key.clone()));
        if self.told == told_now {
            return Vec::new();
        }
        let mut events = Vec::new();
        if let Some((panel, key)) = self.told.take()
            && let Some(node) = tree(panel).and_then(|tree| tree.by_key(&key))
        {
            events.push((
                panel,
                NodeEvent {
                    node,
                    key,
                    kind: UiEventKind::Blur,
                },
            ));
        }
        if let Some(((panel, node), key)) = now {
            events.push((
                panel,
                NodeEvent {
                    node,
                    key,
                    kind: UiEventKind::Focus,
                },
            ));
        }
        self.told = told_now;
        events
    }

    /// The tree of `panel` changed, to `tree`: the node with the focus, if
    /// the tree took it away or it no longer takes the focus, leaves it to
    /// the node in its place among those that take it, or to the last, or,
    /// with none, to the pane.
    pub fn updated(&mut self, panel: P, tree: &Tree) {
        let Some((focused, node)) = self.focused else {
            return;
        };
        if focused != panel {
            return;
        }
        if self
            .anchor
            .is_some_and(|(anchored, node)| anchored == panel && tree.kind(node).is_none())
        {
            self.anchor = None;
        }
        let order = order(tree);
        if let Some(place) = order.iter().position(|&id| id == node) {
            self.place = place;
            return;
        }
        // The node of its key, wherever the render put it.
        let kept = self.key.as_ref().and_then(|key| tree.by_key(key));
        if let Some(place) = kept.and_then(|kept| order.iter().position(|&id| id == kept)) {
            self.focused = Some((panel, order[place]));
            self.place = place;
            return;
        }
        match order.get(self.place).or(order.last()) {
            Some(&next) => {
                self.focused = Some((panel, next));
                self.key = tree.key(next).cloned();
                self.place = self.place.min(order.len() - 1);
            }
            None => self.pane(),
        }
    }

    /// Where `key`, which the keymap left, goes: the pane, or the node with
    /// the focus, or the focus, which Tab, Shift-Tab and Escape over a node
    /// move, Escape back to where its panel took it from; a repeat and a
    /// release where its press went, as the release of a key that moved the
    /// focus goes nowhere, even to a node gone since, and Tab's repeat moves
    /// the focus again. `tree` gives the panels' trees.
    pub fn key<'t>(
        &mut self,
        tree: impl Fn(P) -> Option<&'t Tree>,
        key: &KeyEvent<'_>,
    ) -> Routed<P> {
        if key.action == KeyAction::Release {
            if let Some(at) = self.held.iter().position(|(held, _)| *held == key.key) {
                return self.held.swap_remove(at).1;
            }
            return match self.focused {
                Some((panel, node)) => Routed::Node { panel, node },
                None => Routed::Pane,
            };
        }
        // A repeat goes where its press went: Escape's, which gave the focus
        // to the pane, nowhere, not to its program; Tab's moves the focus
        // again while a node has it.
        if key.action == KeyAction::Repeat
            && let Some(&(_, went)) = self.held.iter().find(|(held, _)| *held == key.key)
        {
            let moves_again = key.key == Key::Tab && self.focused.is_some();
            if went != Routed::Focus || !moves_again {
                return went;
            }
        }
        let routed = self.route(tree, key);
        if key.action == KeyAction::Press {
            self.held.retain(|(held, _)| *held != key.key);
            self.held.push((key.key, routed));
        }
        routed
    }

    /// Where the press or the repeat `key` goes, the focus moved by it.
    fn route<'t>(
        &mut self,
        trees: impl Fn(P) -> Option<&'t Tree>,
        key: &KeyEvent<'_>,
    ) -> Routed<P> {
        let Some((panel, node)) = self.focused else {
            return Routed::Pane;
        };
        let Some(tree) = trees(panel) else {
            self.pane();
            return Routed::Pane;
        };
        let mods = key.mods;
        let plain = !(mods.control
            || mods.left_alt
            || mods.right_alt
            || mods.super_key
            || mods.hyper
            || mods.meta);
        match key.key {
            Key::Tab if plain => {
                if !self.step_across(&trees, (panel, node, tree), mods.shift) {
                    self.step(panel, node, tree, mods.shift);
                }
                Routed::Focus
            }
            Key::Escape if plain && !mods.shift => {
                let from = self
                    .came_from
                    .iter()
                    .find(|(of, _)| *of == panel)
                    .and_then(|(_, from)| from.clone());
                self.back(from, &trees);
                Routed::Focus
            }
            _ => Routed::Node { panel, node },
        }
    }

    /// Moves the focus from `node` of `panel` to the next node of `tree`
    /// that takes it, or the one before it if `back`, round from an end to
    /// the other; to the pane if none does.
    fn step(&mut self, panel: P, node: NodeId, tree: &Tree, back: bool) {
        let order = order(tree);
        let every = self.every_control;
        let tabs_to = |id: NodeId| tabs_to(tree, id, every);
        if !order.iter().any(|&id| tabs_to(id)) {
            self.pane();
            return;
        }
        // From the node last clicked, the next or the one before it of
        // those that Tab reaches, in the tree's order.
        if let Some((anchored, anchor)) = self.anchor.take()
            && anchored == panel
            && let Some(next) = beside(tree, anchor, back, &tabs_to)
        {
            self.focus(panel, tree, next);
            return;
        }
        // From the node with the focus, which a control that Tab does not
        // reach can be, as accessibility gave it.
        let count = order.len();
        let place = order
            .iter()
            .position(|&id| id == node)
            .unwrap_or(self.place.min(count - 1));
        let next = (1..=count)
            .map(|step| {
                if back {
                    (place + count - step) % count
                } else {
                    (place + step) % count
                }
            })
            .find(|&at| tabs_to(order[at]))
            .unwrap_or(place);
        self.focused = Some((panel, order[next]));
        self.key = tree.key(order[next]).cloned();
        self.place = next;
    }

    /// Moves the focus from `node` of `panel`, whose tree is `tree`, past
    /// the last node of the panel that Tab reaches, or before the first if
    /// `back`, to the first node of the next chained panel that has one,
    /// or the last of the one before: whether it did, as `panel` is
    /// chained and its end reached. From the node last clicked, if one of
    /// `panel` was since the focus moved.
    fn step_across<'t>(
        &mut self,
        trees: &impl Fn(P) -> Option<&'t Tree>,
        (panel, node, tree): (P, NodeId, &Tree),
        back: bool,
    ) -> bool {
        let count = self.chain.len();
        let Some(at) = self.chain.iter().position(|&chained| chained == panel) else {
            return false;
        };
        let every = self.every_control;
        let from = match self.anchor {
            Some((anchored, anchor)) if anchored == panel => anchor,
            _ => node,
        };
        if count < 2 || !at_end(tree, from, back, &|id| tabs_to(tree, id, every)) {
            return false;
        }
        for step in 1..count {
            let other = self.chain[if back {
                (at + count - step) % count
            } else {
                (at + step) % count
            }];
            let Some(other_tree) = trees(other) else {
                continue;
            };
            let mut reached = order(other_tree)
                .into_iter()
                .filter(|&id| tabs_to(other_tree, id, every));
            let next = if back {
                reached.next_back()
            } else {
                reached.next()
            };
            if let Some(next) = next {
                return self.focus(other, other_tree, next);
            }
        }
        false
    }

    /// The keymap's contexts of the focus, among `panels` shown, each of its
    /// plugin's namespace: `pane.focused` while the pane has it, then, of
    /// each namespace, `namespace.focused` while one of its panels has it
    /// and `namespace.open`.
    pub fn contexts(&self, panels: &[(P, &str)]) -> Vec<String> {
        let mut names = Vec::new();
        if self.focused.is_none() {
            names.push("pane.focused".to_owned());
        }
        for &(panel, namespace) in panels {
            if self.focused.is_some_and(|(focused, _)| focused == panel) {
                names.push(format!("{namespace}.focused"));
            }
            let open = format!("{namespace}.open");
            if !names.contains(&open) {
                names.push(open);
            }
        }
        names
    }
}

/// Whether Tab reaches `id` of `tree`: a node that takes the focus; a
/// control only when it reaches `every` control.
fn tabs_to(tree: &Tree, id: NodeId, every: bool) -> bool {
    tree.focusable(id) && (every || !tree.kind(id).is_some_and(Kind::is_control))
}

/// The node of `tree` that Tab reaches, as `tabs_to` tells, next after
/// `anchor`, or the one before it if `back`, in the tree's order, round
/// from an end to the other; `None` if none does, or the tree lacks
/// `anchor`.
fn beside(
    tree: &Tree,
    anchor: NodeId,
    back: bool,
    tabs_to: &dyn Fn(NodeId) -> bool,
) -> Option<NodeId> {
    fn walk(tree: &Tree, id: NodeId, all: &mut Vec<NodeId>) {
        all.push(id);
        for &child in tree.children(id) {
            walk(tree, child, all);
        }
    }
    let mut all = Vec::new();
    walk(tree, tree.root()?, &mut all);
    let at = all.iter().position(|&id| id == anchor)?;
    let takes = |id: &&NodeId| tabs_to(**id);
    if back {
        all[..at]
            .iter()
            .rev()
            .chain(all[at..].iter().rev())
            .find(takes)
            .copied()
    } else {
        all[at + 1..].iter().chain(&all[..=at]).find(takes).copied()
    }
}

/// Whether no node of `tree` that Tab reaches, as `tabs_to` tells, comes
/// after `from`, or before it if `back`, in the tree's order, a node before
/// its children; of a tree that lacks `from`, whether none is there.
fn at_end(tree: &Tree, from: NodeId, back: bool, tabs_to: &dyn Fn(NodeId) -> bool) -> bool {
    fn walk(tree: &Tree, id: NodeId, all: &mut Vec<NodeId>) {
        all.push(id);
        for &child in tree.children(id) {
            walk(tree, child, all);
        }
    }
    let mut all = Vec::new();
    if let Some(root) = tree.root() {
        walk(tree, root, &mut all);
    }
    let rest = match all.iter().position(|&id| id == from) {
        Some(at) if back => &all[..at],
        Some(at) => &all[at + 1..],
        None => &all[..],
    };
    !rest.iter().any(|&id| tabs_to(id))
}

/// The nodes of `tree` that take the focus, in the tree's order, a node
/// before its children.
fn order(tree: &Tree) -> Vec<NodeId> {
    fn walk(tree: &Tree, id: NodeId, order: &mut Vec<NodeId>) {
        if tree.focusable(id) {
            order.push(id);
        }
        for &child in tree.children(id) {
            walk(tree, child, order);
        }
    }
    let mut order = Vec::new();
    if let Some(root) = tree.root() {
        walk(tree, root, &mut order);
    }
    order
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::Node;
    use lokyr_vt_core::input::Mods;

    use super::*;

    const SEARCH: u8 = 1;
    const OTHER: u8 = 2;

    /// A find bar: its field, its toggle, a label that takes no focus, and
    /// a group of its two buttons.
    fn find_bar() -> Node {
        Node::container(vec![
            Node::container(vec![Node::text("find")])
                .key("field")
                .focusable(),
            Node::icon("case-sensitive").key("case").focusable(),
            Node::text("3 of 12").key("count"),
            Node::container(vec![
                Node::icon("arrow-up").key("previous").focusable(),
                Node::icon("arrow-down").key("next").focusable(),
            ]),
        ])
    }

    fn tree_of(root: &Node) -> Tree {
        let mut tree = Tree::new();
        tree.update(root).unwrap();
        tree
    }

    fn press(key: Key, mods: Mods) -> KeyEvent<'static> {
        KeyEvent {
            key,
            action: KeyAction::Press,
            mods,
            text: "",
            unmodified: "",
            base_layout: None,
        }
    }

    fn release(key: Key) -> KeyEvent<'static> {
        KeyEvent {
            action: KeyAction::Release,
            ..press(key, Mods::default())
        }
    }

    const SHIFT: Mods = Mods {
        shift: true,
        control: false,
        left_alt: false,
        right_alt: false,
        super_key: false,
        hyper: false,
        meta: false,
        caps_lock: false,
        num_lock: false,
    };

    /// The key of the node with the focus, `pane` for the pane.
    fn focused(focus: &Focus<u8>, tree: &Tree) -> String {
        match focus.target() {
            Target::Pane => "pane".to_owned(),
            Target::Node { node, .. } => tree.key(node).unwrap().0.clone(),
        }
    }

    #[test]
    fn tab_moves_the_focus_through_a_panel_and_escape_back_to_the_pane() {
        let tree = tree_of(&find_bar());
        let mut focus = Focus::new();
        let panels = [(SEARCH, "search")];
        assert_eq!(focus.contexts(&panels), ["pane.focused", "search.open"]);
        assert!(focus.first(SEARCH, &tree));
        assert_eq!(focused(&focus, &tree), "field");
        assert_eq!(focus.contexts(&panels), ["search.focused", "search.open"]);
        let mut tab = |mods| {
            let routed = focus.key(|_| Some(&tree), &press(Key::Tab, mods));
            assert_eq!(routed, Routed::Focus);
            focused(&focus, &tree)
        };
        // The label skipped, the buttons in their group's order, then round
        // to the first.
        assert_eq!(tab(Mods::default()), "case");
        assert_eq!(tab(Mods::default()), "previous");
        assert_eq!(tab(Mods::default()), "next");
        assert_eq!(tab(Mods::default()), "field");
        assert_eq!(tab(SHIFT), "next");
        assert_eq!(tab(SHIFT), "previous");
        let routed = focus.key(|_| Some(&tree), &press(Key::Escape, Mods::default()));
        assert_eq!(routed, Routed::Focus);
        assert_eq!(focus.target(), Target::Pane);
        assert_eq!(focus.contexts(&panels), ["pane.focused", "search.open"]);
        // Two panels of a namespace open once.
        assert_eq!(
            focus.contexts(&[(SEARCH, "search"), (OTHER, "search")]),
            ["pane.focused", "search.open"]
        );
    }

    #[test]
    fn a_panel_that_opens_takes_the_keyboard_at_its_field_before_its_controls() {
        // A search field's glass before its text, Tab reaching every
        // control: the text first.
        let tree = tree_of(&Node::container(vec![
            Node::icon_button("search").key("glass"),
            Node::field("Find").key("field"),
            Node::toggle("regex", false).key("regex"),
        ]));
        let mut focus = Focus::new();
        focus.set_every_control(true);
        assert!(focus.first(SEARCH, &tree));
        assert_eq!(focused(&focus, &tree), "field");
        // Of no field, its first control.
        let tree = tree_of(&Node::container(vec![
            Node::icon_button("search").key("glass"),
            Node::toggle("regex", false).key("regex"),
        ]));
        let mut focus = Focus::new();
        focus.set_every_control(true);
        assert!(focus.first(SEARCH, &tree));
        assert_eq!(focused(&focus, &tree), "glass");
    }

    #[test]
    fn a_key_goes_to_the_focused_element_alone() {
        let tree = tree_of(&find_bar());
        let field = tree.children(tree.root().unwrap())[0];
        let mut focus = Focus::new();
        let key = |focus: &mut Focus<u8>, event| focus.key(|_| Some(&tree), &event);
        // The pane takes every key, Tab and Escape too.
        for k in [Key::Char('a'), Key::Tab, Key::Escape] {
            assert_eq!(key(&mut focus, press(k, Mods::default())), Routed::Pane);
            assert_eq!(key(&mut focus, release(k)), Routed::Pane);
        }
        focus.first(SEARCH, &tree);
        let at_field = Routed::Node {
            panel: SEARCH,
            node: field,
        };
        assert_eq!(
            key(&mut focus, press(Key::Char('a'), Mods::default())),
            at_field
        );
        // Control-Tab, and Shift-Escape, which no binding took, go to the
        // node.
        let control = Mods {
            control: true,
            ..Mods::default()
        };
        assert_eq!(key(&mut focus, press(Key::Tab, control)), at_field);
        assert_eq!(key(&mut focus, release(Key::Tab)), at_field);
        assert_eq!(key(&mut focus, press(Key::Escape, SHIFT)), at_field);
        // Escape gives the focus to the pane, but its release goes nowhere,
        // and the release of the key pressed over the field to the field.
        assert_eq!(
            key(&mut focus, press(Key::Escape, Mods::default())),
            Routed::Focus
        );
        assert_eq!(key(&mut focus, release(Key::Escape)), Routed::Focus);
        assert_eq!(key(&mut focus, release(Key::Char('a'))), at_field);
        assert_eq!(
            key(&mut focus, press(Key::Char('b'), Mods::default())),
            Routed::Pane
        );
        // A press over the pane, released over the field, is the pane's.
        focus.first(SEARCH, &tree);
        assert_eq!(key(&mut focus, release(Key::Char('b'))), Routed::Pane);
        // A repeat of Tab moves the focus again.
        let repeat = KeyEvent {
            action: KeyAction::Repeat,
            ..press(Key::Tab, Mods::default())
        };
        assert_eq!(key(&mut focus, repeat), Routed::Focus);
        assert_eq!(focused(&focus, &tree), "case");
        assert_eq!(key(&mut focus, repeat), Routed::Focus);
        assert_eq!(focused(&focus, &tree), "previous");
        // A panel whose tree is gone gives the key, and the focus, to the
        // pane.
        let routed = focus.key(|_| None, &press(Key::Char('c'), Mods::default()));
        assert_eq!(routed, Routed::Pane);
        assert_eq!(focus.target(), Target::Pane);
    }

    #[test]
    fn a_repeat_goes_where_its_press_went() {
        let tree = tree_of(&find_bar());
        let field = tree.children(tree.root().unwrap())[0];
        let mut focus = Focus::new();
        focus.first(SEARCH, &tree);
        let repeat = |key| KeyEvent {
            action: KeyAction::Repeat,
            ..press(key, Mods::default())
        };
        let mut key = |event| focus.key(|_| Some(&tree), &event);
        // Escape held over the field: its press gives the focus to the
        // pane, its repeats go nowhere, not to the pane's program.
        assert_eq!(key(press(Key::Escape, Mods::default())), Routed::Focus);
        assert_eq!(key(repeat(Key::Escape)), Routed::Focus);
        assert_eq!(key(repeat(Key::Escape)), Routed::Focus);
        assert_eq!(key(release(Key::Escape)), Routed::Focus);
        // Pressed again over the pane, its repeats are the pane's.
        assert_eq!(key(press(Key::Escape, Mods::default())), Routed::Pane);
        assert_eq!(key(repeat(Key::Escape)), Routed::Pane);
        assert_eq!(key(release(Key::Escape)), Routed::Pane);
        // A letter held over the field repeats there, though the focus
        // moved to the pane meanwhile; Tab held moves the focus again.
        focus.first(SEARCH, &tree);
        let at_field = Routed::Node {
            panel: SEARCH,
            node: field,
        };
        let mut key = |event| focus.key(|_| Some(&tree), &event);
        assert_eq!(key(press(Key::Char('a'), Mods::default())), at_field);
        assert_eq!(key(press(Key::Tab, Mods::default())), Routed::Focus);
        assert_eq!(key(repeat(Key::Tab)), Routed::Focus);
        assert_eq!(key(repeat(Key::Char('a'))), at_field);
        assert_eq!(focused(&focus, &tree), "previous");
    }

    #[test]
    fn the_focus_follows_its_key_to_the_node_that_a_render_makes() {
        use UiEventKind::{Blur, Focus as Focused};
        let mut tree = tree_of(&find_bar());
        let mut focus = Focus::new();
        let told = |focus: &mut Focus<u8>, tree: &Tree| {
            focus
                .events(|_| Some(tree))
                .into_iter()
                .map(|(_, event)| (event.key.0, event.kind))
                .collect::<Vec<_>>()
        };
        focus.first(SEARCH, &tree);
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        assert_eq!(focused(&focus, &tree), "case");
        told(&mut focus, &tree);
        // The toggle moved to the end of the buttons' group: another node
        // of its key, of another place in the order, which keeps the focus,
        // and no event.
        let mut bar = find_bar();
        let case = bar.children.remove(1);
        bar.children[2].children.push(case);
        tree.update(&bar).unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(focused(&focus, &tree), "case");
        assert_eq!(told(&mut focus, &tree), []);
        // Tab goes on from its new place, the last, round to the first.
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        assert_eq!(focused(&focus, &tree), "field");
        assert_eq!(
            told(&mut focus, &tree),
            [("case".to_owned(), Blur), ("field".to_owned(), Focused)]
        );
    }

    #[test]
    fn a_node_taken_away_leaves_the_focus_to_the_one_in_its_place() {
        let mut tree = tree_of(&find_bar());
        let mut focus = Focus::new();
        focus.first(SEARCH, &tree);
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        assert_eq!(focused(&focus, &tree), "case");
        // The toggle goes: the button after it takes its place.
        let mut bar = find_bar();
        bar.children.remove(1);
        tree.update(&bar).unwrap();
        // Another panel's change changes nothing.
        focus.updated(OTHER, &tree);
        focus.updated(SEARCH, &tree);
        assert_eq!(focused(&focus, &tree), "previous");
        // A node kept keeps the focus, at its new place.
        tree.update(&find_bar()).unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(focused(&focus, &tree), "previous");
        assert_eq!(
            focus.key(|_| Some(&tree), &press(Key::Tab, SHIFT)),
            Routed::Focus
        );
        assert_eq!(focused(&focus, &tree), "case");
        // A node added before it moves its place, which a removal then
        // gives the one after it.
        let mut bar = find_bar();
        bar.children
            .insert(0, Node::icon("history").key("history").focusable());
        tree.update(&bar).unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(focused(&focus, &tree), "case");
        bar.children.remove(2);
        tree.update(&bar).unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(focused(&focus, &tree), "previous");
        tree.update(&find_bar()).unwrap();
        focus.updated(SEARCH, &tree);
        focus.key(|_| Some(&tree), &press(Key::Tab, SHIFT));
        assert_eq!(focused(&focus, &tree), "case");
        // The buttons no longer take the focus, past the last: the last.
        tree.update(&find_bar()).unwrap();
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        assert_eq!(focused(&focus, &tree), "next");
        let mut bar = find_bar();
        bar.children[3].children = vec![
            Node::icon("arrow-up").key("previous"),
            Node::icon("arrow-down").key("next"),
        ];
        tree.update(&bar).unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(focused(&focus, &tree), "case");
        // None that takes it: the pane.
        tree.update(&Node::container(vec![Node::text("3 of 12")]))
            .unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(focus.target(), Target::Pane);
        // A panel that closes gives the focus back to the pane, another's
        // closing leaving it.
        let tree = tree_of(&find_bar());
        focus.first(SEARCH, &tree);
        focus.closed(OTHER, |_| None);
        assert_eq!(focused(&focus, &tree), "field");
        focus.closed(SEARCH, |_| None);
        assert_eq!(focus.target(), Target::Pane);
    }

    #[test]
    fn a_panel_that_closes_gives_the_focus_back_where_it_took_it_from() {
        const MENU: u8 = 3;
        let bar = tree_of(&find_bar());
        let menu = tree_of(&Node::container(vec![
            Node::text("Regex").key("regex").focusable(),
            Node::text("Whole Words").key("words").focusable(),
        ]));
        let trees = |of: u8| match of {
            SEARCH => Some(&bar),
            MENU => Some(&menu),
            _ => None,
        };
        // A menu opened from the find bar's toggle, its focus moved within
        // it, closed: the toggle has it again.
        let mut focus = Focus::new();
        focus.first(SEARCH, &bar);
        focus.key(|_| Some(&bar), &press(Key::Tab, Mods::NONE));
        assert_eq!(focused(&focus, &bar), "case");
        focus.first(MENU, &menu);
        focus.key(|_| Some(&menu), &press(Key::Tab, Mods::NONE));
        assert_eq!(focused(&focus, &menu), "words");
        // Return, held as it chose an item, which closes the menu: its
        // repeats and its release go nowhere.
        assert_eq!(
            focus.key(|_| Some(&menu), &press(Key::Enter, Mods::NONE)),
            Routed::Node {
                panel: MENU,
                node: menu.by_key(&"words".into()).unwrap()
            }
        );
        focus.closed(MENU, trees);
        assert_eq!(
            focus.target(),
            Target::Node {
                panel: SEARCH,
                node: bar.by_key(&"case".into()).unwrap()
            }
        );
        let mut repeat = press(Key::Enter, Mods::NONE);
        repeat.action = KeyAction::Repeat;
        assert_eq!(focus.key(trees, &repeat), Routed::Focus);
        assert_eq!(focus.key(trees, &release(Key::Enter)), Routed::Focus);
        // The bar's own closing gives it back to the pane, where it took it.
        focus.closed(SEARCH, trees);
        assert_eq!(focus.target(), Target::Pane);
        // A menu of the bar that closes after the bar gives it to where the
        // bar took it from; one whose node is gone, to the pane.
        focus.first(SEARCH, &bar);
        focus.first(MENU, &menu);
        focus.closed(SEARCH, |_| None);
        focus.closed(MENU, trees);
        assert_eq!(focus.target(), Target::Pane);
        focus.first(SEARCH, &bar);
        focus.first(MENU, &menu);
        let gone = tree_of(&Node::container(vec![]));
        focus.closed(MENU, |of| (of == SEARCH).then_some(&gone));
        assert_eq!(focus.target(), Target::Pane);
        // A press on the menu's own node keeps where it took the focus from.
        focus.first(SEARCH, &bar);
        focus.first(MENU, &menu);
        let words = menu.by_key(&"words".into()).unwrap();
        assert!(focus.pressed(MENU, &menu, &[menu.root().unwrap(), words]));
        focus.closed(MENU, trees);
        assert_eq!(focused(&focus, &bar), "field");
        // Escape in a menu that stays open gives it back where the menu
        // took it from, the bar's field; Escape there, to the pane.
        focus.first(SEARCH, &bar);
        focus.first(MENU, &menu);
        assert_eq!(
            focus.key(trees, &press(Key::Escape, Mods::NONE)),
            Routed::Focus
        );
        assert_eq!(focused(&focus, &bar), "field");
        focus.key(trees, &press(Key::Escape, Mods::NONE));
        assert_eq!(focus.target(), Target::Pane);
    }

    #[test]
    fn escape_from_two_panels_that_took_the_focus_from_each_other_reaches_the_pane() {
        // Two bars that stay, as two slots' views: a press on one, on the
        // other, and on the first again.
        let (first, second) = (tree_of(&find_bar()), tree_of(&find_bar()));
        let trees = |of: u8| match of {
            SEARCH => Some(&first),
            OTHER => Some(&second),
            _ => None,
        };
        let field = |tree: &Tree| vec![tree.root().unwrap(), tree.by_key(&"field".into()).unwrap()];
        let mut focus = Focus::new();
        focus.pressed(SEARCH, &first, &field(&first));
        focus.pressed(OTHER, &second, &field(&second));
        focus.pressed(SEARCH, &first, &field(&first));
        // Escape takes it back to where the first took it from first.
        focus.key(trees, &press(Key::Escape, Mods::NONE));
        assert_eq!(focus.target(), Target::Pane);
        // Escape from the second goes to the first, then to the pane.
        focus.pressed(SEARCH, &first, &field(&first));
        focus.pressed(OTHER, &second, &field(&second));
        focus.key(trees, &press(Key::Escape, Mods::NONE));
        assert!(matches!(focus.target(), Target::Node { panel: SEARCH, .. }));
        focus.key(trees, &press(Key::Escape, Mods::NONE));
        assert_eq!(focus.target(), Target::Pane);
        // From the pane, a panel takes it anew: the second, pressed after
        // the pane had it, goes back to the pane.
        focus.pressed(SEARCH, &first, &field(&first));
        focus.pressed(OTHER, &second, &field(&second));
        focus.pane();
        focus.pressed(OTHER, &second, &field(&second));
        focus.key(trees, &press(Key::Escape, Mods::NONE));
        assert_eq!(focus.target(), Target::Pane);
    }

    #[test]
    fn the_focus_s_moves_come_as_events_and_tab_goes_on_from_the_last_click() {
        use UiEventKind::{Blur, Focus as Focused};
        let mut tree = tree_of(&find_bar());
        let mut focus = Focus::new();
        let told = |focus: &mut Focus<u8>, tree: &Tree| {
            focus
                .events(|_| Some(tree))
                .into_iter()
                .map(|(panel, event)| (panel, event.key.0, event.kind))
                .collect::<Vec<_>>()
        };
        let event = |key: &str, kind| (SEARCH, key.to_owned(), kind);
        assert_eq!(told(&mut focus, &tree), []);
        focus.first(SEARCH, &tree);
        assert_eq!(told(&mut focus, &tree), [event("field", Focused)]);
        assert_eq!(told(&mut focus, &tree), []);
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        assert_eq!(
            told(&mut focus, &tree),
            [event("field", Blur), event("case", Focused)]
        );
        focus.pane();
        assert_eq!(told(&mut focus, &tree), [event("case", Blur)]);
        // A node taken away is not told of its blur.
        focus.first(SEARCH, &tree);
        told(&mut focus, &tree);
        let mut bar = find_bar();
        bar.children.remove(0);
        tree.update(&bar).unwrap();
        focus.updated(SEARCH, &tree);
        assert_eq!(told(&mut focus, &tree), [event("case", Focused)]);
        // A click on the label, which takes no focus, leaves the focus on
        // the field; Tab goes on from the label, to the button after it.
        let tree = tree_of(&find_bar());
        focus.first(SEARCH, &tree);
        let root = tree.root().unwrap();
        let count = tree.children(root)[2];
        assert!(!focus.pressed(SEARCH, &tree, &[root, count]));
        assert_eq!(focused(&focus, &tree), "field");
        let tab = |focus: &mut Focus<u8>, mods| {
            focus.key(|_| Some(&tree), &press(Key::Tab, mods));
            focused(focus, &tree)
        };
        assert_eq!(tab(&mut focus, Mods::default()), "previous");
        // Then as the focus has it.
        assert_eq!(tab(&mut focus, Mods::default()), "next");
        // Shift-Tab, from the label, to the toggle before it.
        assert!(!focus.pressed(SEARCH, &tree, &[root, count]));
        assert_eq!(tab(&mut focus, SHIFT), "case");
        // A click in another panel moves nothing here; the focus's own
        // move forgets the click.
        assert!(!focus.pressed(OTHER, &tree, &[root, count]));
        assert_eq!(tab(&mut focus, Mods::default()), "previous");
        assert!(!focus.pressed(SEARCH, &tree, &[root, count]));
        focus.first(SEARCH, &tree);
        assert_eq!(tab(&mut focus, Mods::default()), "case");
    }

    #[test]
    fn a_press_focuses_the_innermost_node_that_takes_it() {
        let tree = tree_of(&find_bar());
        let root = tree.root().unwrap();
        let field = tree.children(root)[0];
        let text = tree.children(field)[0];
        let count = tree.children(root)[2];
        let mut focus = Focus::new();
        // The field's text takes no focus; its field does.
        assert!(focus.pressed(SEARCH, &tree, &[root, field, text]));
        assert_eq!(focused(&focus, &tree), "field");
        // A press over the label leaves the focus where it was.
        assert!(!focus.pressed(SEARCH, &tree, &[root, count]));
        assert_eq!(focused(&focus, &tree), "field");
        // A node that takes no focus is refused it.
        assert!(!focus.focus(SEARCH, &tree, count));
        assert_eq!(focused(&focus, &tree), "field");
        // Of a node that takes it within another, the inner one.
        let nested = tree_of(&Node::container(vec![
            Node::container(vec![Node::icon("close").key("close").focusable()])
                .key("group")
                .focusable(),
        ]));
        let group = nested.children(nested.root().unwrap())[0];
        let close = nested.children(group)[0];
        let path = [nested.root().unwrap(), group, close];
        assert!(focus.pressed(OTHER, &nested, &path));
        assert_eq!(focused(&focus, &nested), "close");
        // A panel without a node that takes it takes no focus.
        let label = tree_of(&Node::container(vec![Node::text("label")]));
        assert!(!focus.first(OTHER, &label));
        assert_eq!(focused(&focus, &nested), "close");
    }

    /// A find bar of controls: its field, two toggles, a disabled button in
    /// a group that takes the focus, and a button.
    fn controls_bar() -> Node {
        use lokyr_plugin_sdk::view::{self, Kind};
        let disabled = Node::new(Kind::Button(view::Button {
            label: "All".to_owned(),
            disabled: true,
            ..view::Button::default()
        }))
        .key("all");
        Node::container(vec![
            Node::field("Find").key("field"),
            Node::toggle("case-sensitive", false).key("case"),
            Node::toggle("regex", true).key("regex"),
            Node::container(vec![disabled]).key("group").focusable(),
            Node::button("Done").key("done"),
        ])
    }

    #[test]
    fn a_click_on_a_control_leaves_the_focus_where_it_was() {
        let tree = tree_of(&controls_bar());
        let root = tree.root().unwrap();
        let [field, case, _, group, done] = tree.children(root).to_vec()[..] else {
            panic!();
        };
        let all = tree.children(group)[0];
        let mut focus = Focus::new();
        assert!(focus.pressed(SEARCH, &tree, &[root, field]));
        // A toggle and a button clicked: the field keeps it, and gets no
        // event.
        focus.events(|_| Some(&tree));
        assert!(!focus.pressed(SEARCH, &tree, &[root, case]));
        assert!(!focus.pressed(SEARCH, &tree, &[root, done]));
        assert_eq!(focused(&focus, &tree), "field");
        assert!(focus.events(|_| Some(&tree)).is_empty());
        // A disabled button keeps its group from taking it.
        assert!(!focus.pressed(SEARCH, &tree, &[root, group, all]));
        assert_eq!(focused(&focus, &tree), "field");
        // Nor does Tab go on from a control clicked: from the field still.
        focus.key(|_| Some(&tree), &press(Key::Tab, Mods::default()));
        assert_eq!(focused(&focus, &tree), "group");
        // Accessibility's focus reaches a control.
        assert!(focus.focus(SEARCH, &tree, case));
        assert_eq!(focused(&focus, &tree), "case");
    }

    #[test]
    fn tab_reaches_the_controls_as_the_system_s_keyboard_navigation_has_it() {
        let tree = tree_of(&controls_bar());
        let tab = press(Key::Tab, Mods::default());
        let back = press(Key::Tab, SHIFT);
        let round = |focus: &mut Focus<u8>, key: &KeyEvent<'_>| {
            let mut keys = Vec::new();
            for _ in 0..4 {
                focus.key(|_| Some(&tree), key);
                keys.push(focused(focus, &tree));
            }
            keys
        };
        // Without it, the field and the group alone, either way.
        let mut focus = Focus::new();
        assert!(focus.first(SEARCH, &tree));
        assert_eq!(focused(&focus, &tree), "field");
        assert_eq!(
            round(&mut focus, &tab),
            ["group", "field", "group", "field"]
        );
        assert_eq!(
            round(&mut focus, &back),
            ["group", "field", "group", "field"]
        );
        // With it, every control that is not disabled.
        focus.set_every_control(true);
        assert_eq!(round(&mut focus, &tab), ["case", "regex", "group", "done"]);
        assert_eq!(
            round(&mut focus, &back),
            ["group", "regex", "case", "field"]
        );
        // From a control that accessibility focused, without it: the next
        // that Tab reaches.
        focus.set_every_control(false);
        let root = tree.root().unwrap();
        focus.focus(SEARCH, &tree, tree.children(root)[2]);
        focus.key(|_| Some(&tree), &tab);
        assert_eq!(focused(&focus, &tree), "group");
        focus.focus(SEARCH, &tree, tree.children(root)[2]);
        focus.key(|_| Some(&tree), &back);
        assert_eq!(focused(&focus, &tree), "field");
        // A panel of controls alone: the first, without it, is none, and
        // Tab from one gives the focus back to the pane.
        let controls = tree_of(&Node::container(vec![
            Node::button("Done").key("done"),
            Node::toggle("regex", false).key("regex"),
        ]));
        let mut focus = Focus::new();
        assert!(!focus.first(SEARCH, &controls));
        focus.focus(
            SEARCH,
            &controls,
            controls.children(controls.root().unwrap())[0],
        );
        focus.key(|_| Some(&controls), &tab);
        assert_eq!(focus.target(), Target::Pane);
        // From a label clicked, Tab goes on to the next that it reaches,
        // past a control.
        let mut focus = Focus::new();
        let labelled = tree_of(&Node::container(vec![
            Node::field("Find").key("find"),
            Node::text("label").key("label"),
            Node::button("Done").key("done"),
            Node::field("Replace").key("replace"),
        ]));
        let root = labelled.root().unwrap();
        let label = labelled.children(root)[1];
        assert!(focus.first(SEARCH, &labelled));
        focus.pressed(SEARCH, &labelled, &[root, label]);
        focus.key(|_| Some(&labelled), &tab);
        assert_eq!(focused(&focus, &labelled), "replace");
    }

    #[test]
    fn tab_goes_through_chained_panels_as_one_and_escape_back_to_the_first() {
        let sidebar = tree_of(&Node::container(vec![Node::field("pages").key("pages")]));
        let page = tree_of(&Node::container(vec![
            Node::text("General"),
            Node::field("Name").key("name"),
            Node::button("Apply").key("apply"),
            Node::field("Shell").key("shell"),
        ]));
        let empty = tree_of(&Node::container(vec![Node::text("nothing")]));
        let trees = |panel: u8| match panel {
            SEARCH => Some(&sidebar),
            OTHER => Some(&page),
            _ => Some(&empty),
        };
        let tab = press(Key::Tab, Mods::default());
        let back = press(Key::Tab, SHIFT);
        let mut focus = Focus::new();
        focus.set_chain(&[SEARCH, OTHER]);
        assert!(focus.first(SEARCH, &sidebar));
        // Past the sidebar's last node, the page's first; past the page's
        // last, the sidebar's again, the button not reached.
        let at = |focus: &Focus<u8>| match focus.target() {
            Target::Node { panel, node } => {
                (panel, trees(panel).unwrap().key(node).unwrap().0.clone())
            }
            Target::Pane => (0, "pane".to_owned()),
        };
        focus.key(trees, &tab);
        assert_eq!(at(&focus), (OTHER, "name".to_owned()));
        focus.key(trees, &tab);
        assert_eq!(at(&focus), (OTHER, "shell".to_owned()));
        focus.key(trees, &tab);
        assert_eq!(at(&focus), (SEARCH, "pages".to_owned()));
        // Shift-Tab, the other way: before the sidebar's first, the page's
        // last.
        focus.key(trees, &back);
        assert_eq!(at(&focus), (OTHER, "shell".to_owned()));
        // Escape in the page, back to the sidebar's node that it came from.
        focus.key(trees, &press(Key::Escape, Mods::default()));
        assert_eq!(at(&focus), (SEARCH, "pages".to_owned()));
        // A chained panel of no node that Tab reaches is passed over.
        focus.set_chain(&[SEARCH, 3]);
        focus.key(trees, &tab);
        assert_eq!(at(&focus), (SEARCH, "pages".to_owned()));
        // Unchained, Tab goes round the panel alone.
        focus.set_chain(&[]);
        focus.focus(OTHER, &page, page.by_key(&"shell".into()).unwrap());
        focus.key(trees, &tab);
        assert_eq!(at(&focus), (OTHER, "name".to_owned()));
    }
}

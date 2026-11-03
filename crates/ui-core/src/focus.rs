//! The keyboard's focus in a window: one element has it, the window's pane
//! or a node of a panel's view tree that takes it. Of the keys that the
//! keymap leaves, Tab and Shift-Tab move it among the panel's nodes that
//! take it, in the tree's order, from the last round to the first, and
//! Escape gives it back to the pane; every other key goes to the element
//! that has it alone, and a key's release where its press went. The
//! keymap's contexts tell where it is:
//! `pane.focused` while the pane has it, and, of each panel shown, of its
//! plugin's namespace, `namespace.open`, and `namespace.focused` while the
//! panel has it.

use lokyr_vt_core::input::{Key, KeyAction, KeyEvent};

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
}

impl<P> Default for Focus<P> {
    fn default() -> Self {
        Self {
            focused: None,
            place: 0,
            held: Vec::new(),
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
    /// takes it: whether it does.
    pub fn focus(&mut self, panel: P, tree: &Tree, node: NodeId) -> bool {
        let Some(place) = order(tree).iter().position(|&id| id == node) else {
            return false;
        };
        self.focused = Some((panel, node));
        self.place = place;
        true
    }

    /// Gives the focus to the first node of `panel` that takes it, as a
    /// panel that opens takes the keyboard: whether one does.
    pub fn first(&mut self, panel: P, tree: &Tree) -> bool {
        match order(tree).first() {
            Some(&node) => self.focus(panel, tree, node),
            None => false,
        }
    }

    /// Gives the focus to the innermost node of `path` that takes it, a
    /// hit's of the pointer's press over `panel`, whose tree is `tree`:
    /// whether one does; the focus stays where it was otherwise.
    pub fn pressed(&mut self, panel: P, tree: &Tree, path: &[NodeId]) -> bool {
        path.iter()
            .rev()
            .find(|&&id| tree.focusable(id))
            .is_some_and(|&node| self.focus(panel, tree, node))
    }

    /// Gives the focus back to the pane.
    pub fn pane(&mut self) {
        self.focused = None;
    }

    /// `panel` closed: the focus goes back to the pane if it had it.
    pub fn closed(&mut self, panel: P) {
        if self.focused.is_some_and(|(focused, _)| focused == panel) {
            self.pane();
        }
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
        let order = order(tree);
        if let Some(place) = order.iter().position(|&id| id == node) {
            self.place = place;
            return;
        }
        match order.get(self.place).or(order.last()) {
            Some(&next) => {
                self.focused = Some((panel, next));
                self.place = self.place.min(order.len() - 1);
            }
            None => self.pane(),
        }
    }

    /// Where `key`, which the keymap left, goes: the pane, or the node with
    /// the focus, or the focus, which Tab, Shift-Tab and Escape over a node
    /// move; a release where its press went, as the release of a key that
    /// moved the focus goes nowhere, even to a node gone since. `tree`
    /// gives a panel's tree.
    pub fn key<'t>(
        &mut self,
        tree: impl FnOnce(P) -> Option<&'t Tree>,
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
        tree: impl FnOnce(P) -> Option<&'t Tree>,
        key: &KeyEvent<'_>,
    ) -> Routed<P> {
        let Some((panel, node)) = self.focused else {
            return Routed::Pane;
        };
        let Some(tree) = tree(panel) else {
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
                self.step(panel, node, tree, mods.shift);
                Routed::Focus
            }
            Key::Escape if plain && !mods.shift => {
                self.pane();
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
        if order.is_empty() {
            self.pane();
            return;
        }
        let place = order
            .iter()
            .position(|&id| id == node)
            .unwrap_or(self.place.min(order.len() - 1));
        let next = if back {
            (place + order.len() - 1) % order.len()
        } else {
            (place + 1) % order.len()
        };
        self.focused = Some((panel, order[next]));
        self.place = next;
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
        focus.closed(OTHER);
        assert_eq!(focused(&focus, &tree), "field");
        focus.closed(SEARCH);
        assert_eq!(focus.target(), Target::Pane);
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
}

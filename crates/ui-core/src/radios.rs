//! The groups of radio buttons, as macOS's: one choice among their
//! options, down a column or across a row, each a circle of the accent's
//! fill and a white dot while it is chosen, its label after it. A click on
//! an option, or an arrow while the group has the keyboard's focus,
//! chooses one, past those that are disabled, which its plugin learns of
//! by `select`: the option chosen is its plugin's, in its render.

use lokyr_plugin_sdk::view::{Kind, Radios, UiEventKind};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent};

use crate::layout::Layout;
use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// The `select` of the option of the group `node` of `tree` that a click
/// at `at`, in pixels from the tree's corner, is over, as `layout` lays its
/// options out: of an option that is neither chosen nor disabled, of a
/// group that is not disabled.
pub fn clicked(tree: &Tree, layout: &Layout, node: NodeId, at: (f32, f32)) -> Option<NodeEvent> {
    let Some(Kind::Radios(radios)) = tree.kind(node) else {
        return None;
    };
    let (x, y) = at;
    let item = layout.options(tree, node).iter().position(|option| {
        x >= option.x
            && x < option.x + option.width
            && y >= option.y
            && y < option.y + option.height
    })?;
    chosen(tree, node, radios, item)
}

/// What `key` does to the group `node` of `tree`, which has the keyboard's
/// focus: Up and Left, pressed or repeated, choose the option before the
/// one chosen, Down and Right the one after it, past those that are
/// disabled, and stop at the ends; of none chosen, the first or the last;
/// `None` for another key, or another node, which the app's bindings may
/// take. A release takes nothing.
pub fn key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
    let Some(Kind::Radios(radios)) = tree.kind(node) else {
        return None;
    };
    let mods = key.mods;
    let plain = !(mods.shift
        || mods.control
        || mods.left_alt
        || mods.right_alt
        || mods.super_key
        || mods.hyper
        || mods.meta);
    let forward = match key.key {
        Key::Up | Key::Left if plain => false,
        Key::Down | Key::Right if plain => true,
        _ => return None,
    };
    if key.action == KeyAction::Release {
        return Some(Vec::new());
    }
    Some(
        step(radios, forward)
            .and_then(|item| chosen(tree, node, radios, item))
            .into_iter()
            .collect(),
    )
}

/// The first option of `radios` past the one chosen, after it if `forward`
/// and before it otherwise, that is not disabled; of none chosen, the
/// first such option from the start, or from the end.
fn step(radios: &Radios, forward: bool) -> Option<usize> {
    let count = radios.options.len();
    let enabled = |at: &usize| !radios.options[*at].disabled;
    match (radios.chosen.filter(|&chosen| chosen < count), forward) {
        (Some(from), true) => (from + 1..count).find(enabled),
        (Some(from), false) => (0..from).rev().find(enabled),
        (None, true) => (0..count).find(enabled),
        (None, false) => (0..count).rev().find(enabled),
    }
}

/// The `select` of the option `item` of the group `node` of `tree`, of
/// `radios`: none of an option chosen already, disabled or that it lacks,
/// or of a disabled group.
pub(crate) fn chosen(tree: &Tree, node: NodeId, radios: &Radios, item: usize) -> Option<NodeEvent> {
    let option = radios.options.get(item)?;
    if radios.disabled || option.disabled || radios.chosen == Some(item) {
        return None;
    }
    Some(NodeEvent {
        node,
        key: tree.key(node)?.clone(),
        kind: UiEventKind::Select { item },
    })
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{Node, RadioOption};
    use lokyr_vt_core::input::Mods;

    use super::*;
    use crate::layout::Measure;

    /// A font of the tests': a character 6 points wide, a text 14 high, an
    /// icon 16.
    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (6.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    /// A group of `Daily`, `Weekly` and a disabled `Monthly`, of `chosen`,
    /// across a row if `across`, laid out at a scale of 1.
    fn laid(chosen: Option<usize>, across: bool) -> (Tree, Layout, NodeId) {
        let mut group = Node::radios(
            vec![
                RadioOption::new("Daily"),
                RadioOption::new("Weekly"),
                RadioOption {
                    label: "Monthly".to_owned(),
                    disabled: true,
                },
            ],
            chosen,
        )
        .key("every");
        if let Kind::Radios(radios) = &mut group.kind {
            radios.across = across;
        }
        let mut tree = Tree::new();
        let changes = tree.update(&Node::container(vec![group])).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(300.0), None, &mut Fixed);
        let node = tree.by_key(&"every".into()).unwrap();
        (tree, layout, node)
    }

    #[test]
    fn a_group_s_options_go_down_its_column_or_across_its_row() {
        // Each its circle of 16, 3 of room and its label, 6 a character;
        // 6 between two down a column, 16 across a row.
        let (tree, layout, node) = laid(Some(0), false);
        let options: Vec<(f32, f32, f32)> = layout
            .options(&tree, node)
            .iter()
            .map(|option| (option.y, option.width, option.height))
            .collect();
        assert_eq!(
            options,
            [(0.0, 61.0, 16.0), (22.0, 61.0, 16.0), (44.0, 61.0, 16.0)]
        );
        let (tree, layout, node) = laid(Some(0), true);
        let options: Vec<(f32, f32)> = layout
            .options(&tree, node)
            .iter()
            .map(|option| (option.x, option.width))
            .collect();
        assert_eq!(options, [(0.0, 49.0), (65.0, 55.0), (136.0, 61.0)]);
    }

    #[test]
    fn a_click_or_an_arrow_chooses_an_option_but_the_chosen_and_the_disabled() {
        let (tree, layout, node) = laid(Some(0), false);
        let click = |at| clicked(&tree, &layout, node, at).map(|event| event.kind);
        assert_eq!(click((10.0, 30.0)), Some(UiEventKind::Select { item: 1 }));
        assert_eq!(click((10.0, 8.0)), None);
        assert_eq!(click((10.0, 50.0)), None);
        // Between two options, nothing.
        assert_eq!(click((10.0, 18.0)), None);
        let press = |tree: &Tree, node, key| {
            let event = KeyEvent {
                key,
                action: KeyAction::Press,
                mods: Mods::NONE,
                text: "",
                unmodified: "",
                base_layout: None,
            };
            super::key(tree, node, &event).map(|events| {
                events
                    .into_iter()
                    .map(|event| event.kind)
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(
            press(&tree, node, Key::Down),
            Some(vec![UiEventKind::Select { item: 1 }])
        );
        assert_eq!(
            press(&tree, node, Key::Right),
            Some(vec![UiEventKind::Select { item: 1 }])
        );
        // At the first, Up goes nowhere; past Weekly, the disabled one is
        // passed over and Down stops there.
        assert_eq!(press(&tree, node, Key::Up), Some(Vec::new()));
        let (weekly, _, node_weekly) = laid(Some(1), false);
        assert_eq!(press(&weekly, node_weekly, Key::Down), Some(Vec::new()));
        // Of none chosen, the first; another key, none of the group's.
        let (none, _, node_none) = laid(None, false);
        assert_eq!(
            press(&none, node_none, Key::Down),
            Some(vec![UiEventKind::Select { item: 0 }])
        );
        assert_eq!(
            press(&none, node_none, Key::Up),
            Some(vec![UiEventKind::Select { item: 1 }])
        );
        assert_eq!(press(&tree, node, Key::Char(' ')), None);
    }
}

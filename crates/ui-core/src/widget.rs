//! The buttons and the toggles: their room around their content, an icon
//! before a label, and what Space and Return do to the one that has the
//! keyboard's focus, as macOS's controls take them: a button's click, and a
//! toggle's switch. A toggle's state is its plugin's, in its render: the
//! switch tells the state asked, which the next render gives.

use lokyr_plugin_sdk::view::{Kind, UiEventKind};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, Keypad};

use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// The room inside a button's edges, in points, of a label: above and
/// below it, and before and after its content.
pub const PADDING: (f32, f32) = (3.0, 8.0);

/// The room inside the edges of a button of an icon alone, in points, on
/// every side.
pub const ICON_PADDING: f32 = 3.0;

/// The room between a button's icon and its label, in points.
pub const GAP: f32 = 4.0;

/// The radius of a button's corners, in points, of its own box.
pub const RADIUS: f32 = 5.0;

/// The size of a button of `label` and `icon`, either empty, in points,
/// of their sizes: its padding around them, side by side.
pub fn size(label: &str, icon: &str, (text, glyph): ((f32, f32), (f32, f32))) -> (f32, f32) {
    let (text, glyph) = (
        if label.is_empty() { (0.0, 0.0) } else { text },
        if icon.is_empty() { (0.0, 0.0) } else { glyph },
    );
    let gap = if label.is_empty() || icon.is_empty() {
        0.0
    } else {
        GAP
    };
    let (down, across) = padding(label);
    (
        glyph.0 + gap + text.0 + 2.0 * across,
        text.1.max(glyph.1) + 2.0 * down,
    )
}

/// The room inside the edges of a button of `label`, in points: above and
/// below its content, and before and after it.
pub fn padding(label: &str) -> (f32, f32) {
    if label.is_empty() {
        (ICON_PADDING, ICON_PADDING)
    } else {
        PADDING
    }
}

/// What `key` does to the node `node` of `tree`, which has the keyboard's
/// focus: Space or Return, pressed, clicks a button and switches a toggle;
/// `None` for another key, or another node, which the app's bindings may
/// take. A release or a repeat of them takes nothing.
pub fn key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
    let kind = tree.kind(node)?;
    if !matches!(kind, Kind::Button(_) | Kind::Toggle(_)) {
        return None;
    }
    let mods = key.mods;
    let plain = !(mods.shift
        || mods.control
        || mods.left_alt
        || mods.right_alt
        || mods.super_key
        || mods.hyper
        || mods.meta);
    let presses = matches!(
        key.key,
        Key::Char(' ') | Key::Enter | Key::Keypad(Keypad::Enter)
    );
    if !plain || !presses {
        return None;
    }
    if key.action != KeyAction::Press {
        return Some(Vec::new());
    }
    Some(pressed(tree, node).into_iter().collect())
}

/// The event of a press of the widget `node` of `tree`, as a click or a
/// key presses it: a button's click, a toggle's switch to the other
/// state; none of a widget disabled, or of another node.
pub fn pressed(tree: &Tree, node: NodeId) -> Option<NodeEvent> {
    let kind = match tree.kind(node)? {
        Kind::Button(button) if !button.disabled => UiEventKind::Click,
        Kind::Toggle(toggle) if !toggle.disabled => UiEventKind::Toggle { on: !toggle.on },
        _ => return None,
    };
    Some(NodeEvent {
        node,
        key: tree.key(node)?.clone(),
        kind,
    })
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{self, Node};
    use lokyr_vt_core::input::Mods;

    use super::*;

    #[test]
    fn a_button_takes_its_padding_around_its_icon_and_its_label() {
        let (text, glyph) = ((60.0, 16.0), (16.0, 16.0));
        assert_eq!(size("Replace", "", (text, glyph)), (76.0, 22.0));
        assert_eq!(size("", "add", (text, glyph)), (22.0, 22.0));
        assert_eq!(size("Settings", "settings", (text, glyph)), (96.0, 22.0));
        assert_eq!(size("Tall", "", ((20.0, 30.0), glyph)), (36.0, 36.0));
    }

    /// A button, a toggle that is on, a disabled one, and a text, of keys.
    fn widgets() -> (Tree, [NodeId; 4]) {
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::button("Replace").key("replace"),
            Node::toggle("regex", true).key("regex"),
            Node::new(Kind::Toggle(view::Toggle {
                icon: "pin".to_owned(),
                disabled: true,
                ..view::Toggle::default()
            }))
            .key("pin"),
            Node::text("3 of 12").key("count"),
        ]))
        .unwrap();
        let children = tree.children(tree.root().unwrap());
        let nodes = [children[0], children[1], children[2], children[3]];
        (tree, nodes)
    }

    fn pressed_key(key: Key, action: KeyAction, mods: Mods) -> KeyEvent<'static> {
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
    fn space_and_return_press_a_button_and_switch_a_toggle() {
        let (tree, [button, toggle, disabled, text]) = widgets();
        let press = |node, key, action, mods| {
            super::key(&tree, node, &pressed_key(key, action, mods)).map(|events| {
                events
                    .into_iter()
                    .map(|event| (event.key.0, event.kind))
                    .collect::<Vec<_>>()
            })
        };
        let plain = Mods::NONE;
        let click = vec![("replace".to_owned(), UiEventKind::Click)];
        assert_eq!(
            press(button, Key::Char(' '), KeyAction::Press, plain),
            Some(click.clone())
        );
        assert_eq!(
            press(button, Key::Enter, KeyAction::Press, plain),
            Some(click.clone())
        );
        assert_eq!(
            press(button, Key::Keypad(Keypad::Enter), KeyAction::Press, plain),
            Some(click)
        );
        assert_eq!(
            press(toggle, Key::Char(' '), KeyAction::Press, plain),
            Some(vec![(
                "regex".to_owned(),
                UiEventKind::Toggle { on: false }
            )])
        );
        // Their repeats and releases take nothing, and tell nothing.
        assert_eq!(
            press(button, Key::Char(' '), KeyAction::Repeat, plain),
            Some(vec![])
        );
        assert_eq!(
            press(toggle, Key::Enter, KeyAction::Release, plain),
            Some(vec![])
        );
        // A widget disabled tells nothing.
        assert_eq!(
            press(disabled, Key::Char(' '), KeyAction::Press, plain),
            Some(vec![])
        );
        // Another key, a modifier, or a node of no widget: the bindings'.
        assert_eq!(press(button, Key::Char('a'), KeyAction::Press, plain), None);
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        let command = Mods {
            super_key: true,
            ..Mods::NONE
        };
        assert_eq!(press(button, Key::Char(' '), KeyAction::Press, shift), None);
        assert_eq!(press(button, Key::Enter, KeyAction::Press, command), None);
        assert_eq!(press(text, Key::Char(' '), KeyAction::Press, plain), None);
    }
}

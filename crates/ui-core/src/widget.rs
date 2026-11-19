//! The buttons, the toggles and the switches: their room around their
//! content, an icon before a label, a switch's track and knob, and what
//! Space and Return do to the one that has the keyboard's focus, as macOS's
//! controls take them: a button's click, and a toggle's or a switch's
//! switch. A toggle's and a switch's state is its plugin's, in its render:
//! the switch tells the state asked, which the next render gives.

use lokyr_plugin_sdk::view::{ControlSize, Kind, UiEventKind};
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

/// The track of a switch of `size`, in points, its width and its height:
/// macOS's small switch at the regular size, beside a button, and its mini
/// one at the small.
pub fn track(size: ControlSize) -> (f32, f32) {
    match size {
        ControlSize::Regular => (44.0, 20.0),
        ControlSize::Small => (36.0, 16.0),
    }
}

/// The room between a switch's knob and its track's edges, in points.
pub fn knob_inset(size: ControlSize) -> f32 {
    match size {
        ControlSize::Regular => 2.0,
        ControlSize::Small => 1.5,
    }
}

/// How much wider than tall a switch's knob is, a capsule as macOS's.
pub const KNOB_ASPECT: f32 = 1.6;

/// The room between a switch's track and its label, in points.
pub const SWITCH_GAP: f32 = 6.0;

/// The size of a switch of `size` and `label`, which may be empty, in
/// points, of its label's size, `text`: its track, and its label after it.
pub fn switch_size(label: &str, size: ControlSize, text: (f32, f32)) -> (f32, f32) {
    let (width, height) = track(size);
    if label.is_empty() {
        return (width, height);
    }
    (width + SWITCH_GAP + text.0, height.max(text.1))
}

/// The knob of a switch of `size` within its track, `(x, y, width,
/// height)` in pixels at `scale`, as far as `amount` across, from 0, off,
/// at its left, to 1, on, at its right: its own box, in pixels.
pub fn knob(
    (x, y, width, height): (f32, f32, f32, f32),
    size: ControlSize,
    scale: f32,
    amount: f32,
) -> (f32, f32, f32, f32) {
    let inset = knob_inset(size) * scale;
    let tall = (height - 2.0 * inset).max(0.0);
    let wide = (tall * KNOB_ASPECT).min((width - 2.0 * inset).max(0.0));
    let room = (width - 2.0 * inset - wide).max(0.0);
    (
        x + inset + room * amount.clamp(0.0, 1.0),
        y + inset,
        wide,
        tall,
    )
}

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
/// focus: Space or Return, pressed, clicks a button and switches a toggle
/// or a switch; `None` for another key, or another node, which the app's
/// bindings may take. A release or a repeat of them takes nothing.
pub fn key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
    if !tree.kind(node)?.presses() {
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
/// key presses it: a button's click, a toggle's or a switch's switch to
/// the other state; none of a widget disabled, or of another node.
pub fn pressed(tree: &Tree, node: NodeId) -> Option<NodeEvent> {
    let kind = match tree.kind(node)? {
        Kind::Button(button) if !button.disabled => UiEventKind::Click,
        Kind::Toggle(toggle) if !toggle.disabled => UiEventKind::Toggle { on: !toggle.on },
        Kind::Switch(switch) if !switch.disabled => UiEventKind::Toggle { on: !switch.on },
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

    #[test]
    fn a_switch_takes_its_track_and_its_label_after_it_and_its_knob_slides_across() {
        let text = (42.0, 16.0);
        // macOS's small switch and its mini one; a label after a gap, of
        // the taller of the two.
        assert_eq!(switch_size("", ControlSize::Regular, text), (44.0, 20.0));
        assert_eq!(switch_size("", ControlSize::Small, text), (36.0, 16.0));
        assert_eq!(
            switch_size("Notify", ControlSize::Regular, text),
            (92.0, 20.0)
        );
        assert_eq!(
            switch_size("Notify", ControlSize::Small, (42.0, 18.0)),
            (84.0, 18.0)
        );
        // The knob inset of its track, from its left, off, to its right,
        // on, at a scale of 2.
        let track = (10.0, 20.0, 88.0, 40.0);
        assert_eq!(
            knob(track, ControlSize::Regular, 2.0, 0.0),
            (14.0, 24.0, 51.2, 32.0)
        );
        let (x, ..) = knob(track, ControlSize::Regular, 2.0, 1.0);
        assert!((x - (10.0 + 88.0 - 4.0 - 51.2)).abs() < 1e-4, "{x}");
        let (half, ..) = knob(track, ControlSize::Regular, 2.0, 0.5);
        assert!((half - (14.0 + 28.8 / 2.0)).abs() < 1e-4, "{half}");
        // Past either end, at that end.
        assert_eq!(knob(track, ControlSize::Regular, 2.0, -1.0).0, 14.0);
    }

    #[test]
    fn space_return_and_a_press_switch_a_switch_unless_it_is_disabled() {
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::switch("Notify", false).key("notify"),
            Node::new(Kind::Switch(view::Switch {
                on: true,
                disabled: true,
                ..view::Switch::default()
            }))
            .key("held"),
        ]))
        .unwrap();
        let children = tree.children(tree.root().unwrap());
        let (switch, held) = (children[0], children[1]);
        let on = vec![("notify".to_owned(), UiEventKind::Toggle { on: true })];
        for key in [Key::Char(' '), Key::Enter] {
            let events = super::key(
                &tree,
                switch,
                &pressed_key(key, KeyAction::Press, Mods::NONE),
            )
            .map(|events| {
                events
                    .into_iter()
                    .map(|event| (event.key.0, event.kind))
                    .collect::<Vec<_>>()
            });
            assert_eq!(events, Some(on.clone()));
        }
        let clicked = pressed(&tree, switch).map(|event| (event.key.0, event.kind));
        assert_eq!(clicked, on.into_iter().next());
        assert!(pressed(&tree, held).is_none());
        assert!(!tree.kind(held).unwrap().takes_focus());
        assert!(tree.kind(switch).unwrap().is_control());
    }
}

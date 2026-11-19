//! The buttons, the toggles and the switches: their room around their
//! content, an icon before a label, a switch's track and knob, and what
//! Space and Return do to the one that has the keyboard's focus, as macOS's
//! controls take them: a button's click, and a toggle's or a switch's
//! switch. A toggle's and a switch's state is its plugin's, in its render:
//! the switch tells the state asked, which the next render gives.

use lokyr_plugin_sdk::view::{ButtonVariant, ControlSize, Kind, UiEventKind};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, Keypad};

use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// The height of a control of `size`, in points, as macOS 27's: a button,
/// a toggle, a field or a segmented control; taller only where its text
/// needs more, as of a large interface's font.
pub fn height(size: ControlSize) -> f32 {
    match size {
        ControlSize::Large => 28.0,
        ControlSize::Regular => 24.0,
        ControlSize::Small => 20.0,
        ControlSize::Mini => 16.0,
    }
}

/// The room inside a button's edges, in points, before and after its
/// content of a label, of `size`.
pub fn across(size: ControlSize) -> f32 {
    match size {
        ControlSize::Large => 12.0,
        ControlSize::Regular => 10.0,
        ControlSize::Small => 8.0,
        ControlSize::Mini => 6.0,
    }
}

/// The least room above and below a control's text, in points, which a
/// text taller than its control's height takes.
pub const LEAST_DOWN: f32 = 2.0;

/// The room between a button's icon and its label, in points.
pub const GAP: f32 = 4.0;

/// The radius of a button's and a field's corners, in points, of their own
/// boxes, as macOS 27's.
pub const RADIUS: f32 = 6.0;

/// The radius of a group box's corners, in points, as macOS 27's forms.
pub const GROUP_RADIUS: f32 = 12.0;

/// The track of a switch of `size`, in points, its width and its height,
/// as macOS 27's switches of the four sizes.
pub fn track(size: ControlSize) -> (f32, f32) {
    match size {
        ControlSize::Large => (64.0, 28.0),
        ControlSize::Regular => (54.0, 24.0),
        ControlSize::Small => (44.0, 20.0),
        ControlSize::Mini => (36.0, 16.0),
    }
}

/// The room between a switch's knob and its track's edges, in points.
pub fn knob_inset(size: ControlSize) -> f32 {
    match size {
        ControlSize::Mini => 1.5,
        _ => 2.0,
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

/// The side of a checkbox's box and of a radio button's circle of `size`,
/// in points, as macOS 27's.
pub fn check_side(size: ControlSize) -> f32 {
    match size {
        ControlSize::Large => 18.0,
        ControlSize::Regular => 16.0,
        ControlSize::Small => 14.0,
        ControlSize::Mini => 12.0,
    }
}

/// The radius of a checkbox's corners, a share of its side, as macOS 27
/// rounds them: 5.5 points of 16.
pub const CHECK_ROUNDING: f32 = 0.34375;

/// The share of a radio button's side that its dot takes while it is
/// chosen.
pub const RADIO_DOT: f32 = 0.375;

/// The share of a checkbox's side that its check's line is thick, as macOS
/// 27's.
pub const CHECK_THICKNESS: f32 = 0.11;

/// The share of a checkbox's side that its dash is long while it is mixed,
/// 6.5 points of 16, as macOS 27's.
pub const DASH_LENGTH: f32 = 0.40625;

/// The share of a checkbox's side that its dash is thick, 2 points of 16.
pub const DASH_THICKNESS: f32 = 0.125;

/// The room between a checkbox's or a radio button's box and its label, in
/// points, as macOS 27's.
pub const CHECK_GAP: f32 = 3.0;

/// The room between a group's radio buttons down a column, in points.
pub const RADIO_DOWN: f32 = 6.0;

/// The room between a group's radio buttons across a row, in points.
pub const RADIO_ACROSS: f32 = 16.0;

/// The size of a checkbox, or of a radio button, of `label`, which may be
/// empty, and of `size`, in points, of its label's size, `text`: its box,
/// and its label after it, of the taller of the two.
pub fn check_size(label: &str, size: ControlSize, text: (f32, f32)) -> (f32, f32) {
    let side = check_side(size);
    if label.is_empty() {
        return (side, side);
    }
    (side + CHECK_GAP + text.0, side.max(text.1))
}

/// The room inside a pop-up button's left edge, before its text, of
/// `size`, in points, as macOS 27's.
pub fn pop_up_inset(size: ControlSize) -> f32 {
    match size {
        ControlSize::Large => 14.0,
        ControlSize::Regular => 12.0,
        ControlSize::Small => 10.0,
        ControlSize::Mini => 7.0,
    }
}

/// The room between a pop-up button's text and the square of its
/// chevrons, in points, as macOS 27's.
pub const POP_UP_GAP: f32 = 8.0;

/// The size of the text of a pop-up button's chevrons of `size`, in
/// points, as macOS 27's draws them of SF Symbols, about as tall.
pub fn chevrons(size: ControlSize) -> f32 {
    match size {
        ControlSize::Large => 13.0,
        ControlSize::Regular | ControlSize::Small => 11.0,
        ControlSize::Mini => 10.0,
    }
}

/// The size of a pop-up button of `size`, in points, of the size of its
/// widest text, `text`, an item's or its placeholder's, so that a choice
/// does not change its width: its room before its text, its text, the
/// room after it, and a square of its height at its right, its chevrons in
/// its middle; of its control's height, or of its text's and the least
/// room above and below where that is taller.
pub fn pop_up_size(size: ControlSize, text: (f32, f32)) -> (f32, f32) {
    let tall = height(size).max(text.1 + 2.0 * LEAST_DOWN);
    (pop_up_inset(size) + text.0 + POP_UP_GAP + tall, tall)
}

/// The size of a button of `label` and `icon`, either empty, and of
/// `size`, in points, of their sizes: its control's height, or its
/// content's and the least room above and below where that is taller, and
/// its room before and after them, side by side; a square of an icon alone.
pub fn size(
    label: &str,
    icon: &str,
    (text, glyph): ((f32, f32), (f32, f32)),
    size: ControlSize,
) -> (f32, f32) {
    let (text, glyph) = (
        if label.is_empty() { (0.0, 0.0) } else { text },
        if icon.is_empty() { (0.0, 0.0) } else { glyph },
    );
    let tall = height(size).max(text.1.max(glyph.1) + 2.0 * LEAST_DOWN);
    if label.is_empty() {
        return (tall, tall);
    }
    let gap = if icon.is_empty() { 0.0 } else { GAP };
    (glyph.0 + gap + text.0 + 2.0 * across(size), tall)
}

/// The room inside the edges of a button of `label` and of `size`, in
/// points, before and after its content: none past its icon's square of an
/// icon alone, which sits in its middle.
pub fn padding(label: &str, size: ControlSize) -> f32 {
    if label.is_empty() { 0.0 } else { across(size) }
}

/// What `key` does to the node `node` of `tree`, which has the keyboard's
/// focus: Space or Return, pressed, clicks a button and switches a toggle
/// or a switch; `None` for another key, or another node, which the app's
/// bindings may take. A release or a repeat of them takes nothing.
pub fn key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
    if !tree.kind(node)?.presses() {
        return None;
    }
    let presses = matches!(
        key.key,
        Key::Char(' ') | Key::Enter | Key::Keypad(Keypad::Enter)
    );
    if !plain(key) || !presses {
        return None;
    }
    if key.action != KeyAction::Press {
        return Some(Vec::new());
    }
    Some(pressed(tree, node).into_iter().collect())
}

/// Whether `key` opens the menu of the pop-up button `node` of `tree`,
/// which has the keyboard's focus, as macOS's open theirs: Space, Return,
/// ↑ or ↓ pressed, of no modifier; `Some(false)` of a release or a repeat
/// of them, which takes nothing; `None` of another key, of another node, or
/// of a pop-up button disabled.
pub fn pops_up(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<bool> {
    let Some(Kind::PopUp(pop_up)) = tree.kind(node) else {
        return None;
    };
    let opens = matches!(
        key.key,
        Key::Char(' ') | Key::Enter | Key::Keypad(Keypad::Enter) | Key::Up | Key::Down
    );
    if pop_up.disabled || !opens || !plain(key) {
        return None;
    }
    Some(key.action == KeyAction::Press)
}

/// What Return does wherever the focus of `tree` is, at `node`, as macOS's
/// default button takes it: it presses the tree's first default button
/// that is not disabled, but from a list, which takes Return itself, as a
/// field does before; `None` of another key, or of a tree of no default
/// button. A release or a repeat of it takes nothing.
pub fn default_key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
    let returns = matches!(key.key, Key::Enter | Key::Keypad(Keypad::Enter)) && plain(key);
    if !returns || matches!(tree.kind(node), Some(Kind::List(_) | Kind::Field(_))) {
        return None;
    }
    let default = first(tree, tree.root()?, &|kind| {
        matches!(
            kind,
            Kind::Button(button)
                if button.variant == ButtonVariant::Default && !button.disabled
        )
    })?;
    if key.action != KeyAction::Press {
        return Some(Vec::new());
    }
    Some(pressed(tree, default).into_iter().collect())
}

/// The first node of `id`'s subtree in `tree`, in its order, of a kind that
/// `wanted` takes.
fn first(tree: &Tree, id: NodeId, wanted: &dyn Fn(&Kind) -> bool) -> Option<NodeId> {
    if tree.kind(id).is_some_and(wanted) {
        return Some(id);
    }
    tree.children(id)
        .iter()
        .find_map(|&child| first(tree, child, wanted))
}

/// Whether `key` is of no modifier.
fn plain(key: &KeyEvent<'_>) -> bool {
    let mods = key.mods;
    !(mods.shift
        || mods.control
        || mods.left_alt
        || mods.right_alt
        || mods.super_key
        || mods.hyper
        || mods.meta)
}

/// The event of a press of the widget `node` of `tree`, as a click or a
/// key presses it: a button's click, a toggle's or a switch's switch to
/// the other state; none of a widget disabled, or of another node.
pub fn pressed(tree: &Tree, node: NodeId) -> Option<NodeEvent> {
    let kind = match tree.kind(node)? {
        Kind::Button(button) if !button.disabled => UiEventKind::Click,
        Kind::Toggle(toggle) if !toggle.disabled => UiEventKind::Toggle { on: !toggle.on },
        Kind::Switch(switch) if !switch.disabled => UiEventKind::Toggle { on: !switch.on },
        Kind::Checkbox(checkbox) if !checkbox.disabled => UiEventKind::Toggle {
            on: checkbox.mixed || !checkbox.on,
        },
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
    fn a_button_takes_its_control_s_height_and_its_padding_around_its_icon_and_its_label() {
        let (text, glyph) = ((60.0, 16.0), (16.0, 16.0));
        let regular = ControlSize::Regular;
        assert_eq!(size("Replace", "", (text, glyph), regular), (80.0, 24.0));
        // A square of an icon alone, its icon in its middle.
        assert_eq!(size("", "add", (text, glyph), regular), (24.0, 24.0));
        assert_eq!(padding("", regular), 0.0);
        assert_eq!(
            size("Settings", "settings", (text, glyph), regular),
            (100.0, 24.0)
        );
        // Taller where its text needs more room than its control's height.
        assert_eq!(
            size("Tall", "", ((20.0, 30.0), glyph), regular),
            (40.0, 34.0)
        );
        // The four sizes, as macOS 27's, of their own room across.
        let small = ((60.0, 11.0), (12.0, 12.0));
        assert_eq!(
            size("Replace", "", (text, glyph), ControlSize::Large),
            (84.0, 28.0)
        );
        assert_eq!(size("Replace", "", small, ControlSize::Small), (76.0, 20.0));
        assert_eq!(size("Replace", "", small, ControlSize::Mini), (72.0, 16.0));
        assert_eq!(size("", "add", small, ControlSize::Mini), (16.0, 16.0));
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
        // macOS 27's switches of the four sizes; a label after a gap, of
        // the taller of the two.
        assert_eq!(switch_size("", ControlSize::Large, text), (64.0, 28.0));
        assert_eq!(switch_size("", ControlSize::Regular, text), (54.0, 24.0));
        assert_eq!(switch_size("", ControlSize::Small, text), (44.0, 20.0));
        assert_eq!(switch_size("", ControlSize::Mini, text), (36.0, 16.0));
        assert_eq!(
            switch_size("Notify", ControlSize::Regular, text),
            (102.0, 24.0)
        );
        assert_eq!(
            switch_size("Notify", ControlSize::Mini, (42.0, 18.0)),
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

    #[test]
    fn return_presses_the_view_s_default_button_wherever_its_focus_is_but_in_a_list() {
        let button = |label: &str, variant: ButtonVariant, disabled: bool| {
            Node::new(Kind::Button(view::Button {
                label: label.to_owned(),
                variant,
                disabled,
                ..view::Button::default()
            }))
        };
        let tree_of = |default_disabled: bool| {
            let mut tree = Tree::new();
            tree.update(&Node::container(vec![
                Node::toggle("regex", false).key("regex"),
                button("Cancel", ButtonVariant::Plain, false).key("cancel"),
                button("Remove", ButtonVariant::Destructive, false).key("remove"),
                button("Back Up", ButtonVariant::Default, default_disabled).key("back-up"),
                Node::list(Node::text(""), 3, (0, Vec::new())).key("rows"),
            ]))
            .unwrap();
            tree
        };
        let tree = tree_of(false);
        let node = |key: &str| tree.by_key(&key.into()).unwrap();
        let typed = |tree: &Tree, at: NodeId, key: Key| {
            let event = pressed_key(key, KeyAction::Press, Mods::NONE);
            default_key(tree, at, &event)
                .or_else(|| super::key(tree, at, &event))
                .map(|events| {
                    events
                        .into_iter()
                        .map(|event| (event.key.0, event.kind))
                        .collect::<Vec<_>>()
                })
        };
        let back_up = Some(vec![("back-up".to_owned(), UiEventKind::Click)]);
        // From a toggle or another button, Return presses the default one;
        // Space presses the one of the focus.
        assert_eq!(typed(&tree, node("regex"), Key::Enter), back_up);
        assert_eq!(typed(&tree, node("cancel"), Key::Enter), back_up);
        assert_eq!(
            typed(&tree, node("regex"), Key::Char(' ')),
            Some(vec![("regex".to_owned(), UiEventKind::Toggle { on: true })])
        );
        // A list takes Return itself.
        assert_eq!(
            default_key(
                &tree,
                node("rows"),
                &pressed_key(Key::Enter, KeyAction::Press, Mods::NONE)
            ),
            None
        );
        // A disabled default button is passed over, Return pressing the
        // button of the focus, as of no default.
        let held = tree_of(true);
        let cancel = held.by_key(&"cancel".into()).unwrap();
        assert_eq!(
            typed(&held, cancel, Key::Enter),
            Some(vec![("cancel".to_owned(), UiEventKind::Click)])
        );
        // A modifier, a release: nothing, and nothing told.
        let shifted = pressed_key(
            Key::Enter,
            KeyAction::Press,
            Mods {
                shift: true,
                ..Mods::NONE
            },
        );
        assert_eq!(default_key(&tree, node("regex"), &shifted), None);
        let released = pressed_key(Key::Enter, KeyAction::Release, Mods::NONE);
        assert!(
            default_key(&tree, node("regex"), &released)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn space_and_a_press_switch_a_checkbox_unless_it_is_disabled() {
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::checkbox("Compress", false).key("compress"),
            Node::new(Kind::Checkbox(view::Checkbox {
                on: true,
                disabled: true,
                ..view::Checkbox::default()
            }))
            .key("held"),
            Node::new(Kind::Checkbox(view::Checkbox {
                mixed: true,
                ..view::Checkbox::default()
            }))
            .key("some"),
        ]))
        .unwrap();
        let children = tree.children(tree.root().unwrap());
        let (checkbox, held) = (children[0], children[1]);
        // A mixed one turns on, as macOS's does.
        let some = pressed(&tree, children[2]).map(|event| event.kind);
        assert_eq!(some, Some(UiEventKind::Toggle { on: true }));
        let on = vec![("compress".to_owned(), UiEventKind::Toggle { on: true })];
        let spaced = super::key(
            &tree,
            checkbox,
            &pressed_key(Key::Char(' '), KeyAction::Press, Mods::NONE),
        )
        .map(|events| {
            events
                .into_iter()
                .map(|event| (event.key.0, event.kind))
                .collect::<Vec<_>>()
        });
        assert_eq!(spaced, Some(on));
        assert!(pressed(&tree, held).is_none());
        assert_eq!(check_size("", ControlSize::Large, (0.0, 0.0)), (18.0, 18.0));
        assert_eq!(
            check_size("Notify", ControlSize::Mini, (36.0, 11.0)),
            (51.0, 12.0)
        );
    }

    #[test]
    fn a_pop_up_button_takes_its_widest_text_and_opens_of_space_return_and_the_arrows() {
        // Its room before its text, its text, 8, and its chevrons' square.
        assert_eq!(
            pop_up_size(ControlSize::Regular, (60.0, 16.0)),
            (104.0, 24.0)
        );
        assert_eq!(pop_up_size(ControlSize::Large, (60.0, 16.0)), (110.0, 28.0));
        assert_eq!(pop_up_size(ControlSize::Small, (60.0, 13.0)), (98.0, 20.0));
        assert_eq!(pop_up_size(ControlSize::Mini, (60.0, 11.0)), (91.0, 16.0));
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::pop_up(vec!["SF Mono".to_owned()], Some(0)).key("family"),
            Node::new(Kind::PopUp(view::PopUp {
                disabled: true,
                ..view::PopUp::default()
            }))
            .key("style"),
        ]))
        .unwrap();
        let children = tree.children(tree.root().unwrap());
        let (family, style) = (children[0], children[1]);
        let opens = |node, key, action, mods| pops_up(&tree, node, &pressed_key(key, action, mods));
        for key in [Key::Char(' '), Key::Enter, Key::Up, Key::Down] {
            assert_eq!(opens(family, key, KeyAction::Press, Mods::NONE), Some(true));
            assert_eq!(
                opens(family, key, KeyAction::Repeat, Mods::NONE),
                Some(false)
            );
            assert_eq!(opens(style, key, KeyAction::Press, Mods::NONE), None);
        }
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        assert_eq!(opens(family, Key::Down, KeyAction::Press, shift), None);
        assert_eq!(opens(family, Key::Left, KeyAction::Press, Mods::NONE), None);
        // A press of no click, as its menu takes it.
        assert!(pressed(&tree, family).is_none());
    }
}

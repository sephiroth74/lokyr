//! What accessibility asks of a node of a view tree, as VoiceOver's
//! actions: a press, which clicks it, as a button's, or switches a toggle,
//! and does nothing to a widget disabled; a focus, which gives
//! it the keyboard's focus; and a value set, which its plugin learns of.
//! Each goes to the node, or to its nearest ancestor that takes it: of a
//! key for a press or a value, that takes the focus for a focus.

use lokyr_plugin_sdk::view::{Kind, UiEventKind};

use crate::focus::Focus;
use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};
use crate::widget;

/// What accessibility asks of a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A press, as of a button.
    Press,
    /// The keyboard's focus.
    Focus,
    /// Its value set, as of a field.
    SetValue(String),
}

/// Does `action` to `node` of `tree`, of `panel`: the events of a press or
/// a value, to its node; a focus moves `focus`, whose own events tell it.
pub fn perform<P: Copy + Eq>(
    action: Action,
    (panel, tree, node): (P, &Tree, NodeId),
    focus: &mut Focus<P>,
) -> Vec<NodeEvent> {
    let nearest = |takes: &dyn Fn(NodeId) -> bool| {
        std::iter::successors(Some(node), |&id| tree.parent(id)).find(|&id| takes(id))
    };
    let kind = match action {
        Action::Focus => {
            if let Some(focusable) = nearest(&|id| tree.focusable(id)) {
                focus.focus(panel, tree, focusable);
            }
            return Vec::new();
        }
        Action::Press => UiEventKind::Click,
        Action::SetValue(value) => UiEventKind::SetValue { value },
    };
    let keyed = nearest(&|id| tree.key(id).is_some());
    if kind == UiEventKind::Click
        && let Some(id) = keyed
        && matches!(tree.kind(id), Some(Kind::Button(_) | Kind::Toggle(_)))
    {
        return widget::pressed(tree, id).into_iter().collect();
    }
    keyed
        .and_then(|id| {
            Some(NodeEvent {
                node: id,
                key: tree.key(id)?.clone(),
                kind,
            })
        })
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::Node;

    use super::*;

    #[test]
    fn accessibility_s_press_switches_a_toggle_and_presses_no_widget_disabled() {
        use lokyr_plugin_sdk::view::{self, Kind};
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::toggle("regex", true).key("regex"),
            Node::new(Kind::Button(view::Button {
                label: "Replace All".to_owned(),
                disabled: true,
                ..view::Button::default()
            }))
            .key("all"),
        ]))
        .unwrap();
        let root = tree.root().unwrap();
        let (toggle, disabled) = (tree.children(root)[0], tree.children(root)[1]);
        let mut focus = Focus::new();
        let events = perform(Action::Press, (0_u8, &tree, toggle), &mut focus);
        assert!(matches!(
            &events[..],
            [NodeEvent {
                kind: UiEventKind::Toggle { on: false },
                ..
            }]
        ));
        assert!(perform(Action::Press, (0, &tree, disabled), &mut focus).is_empty());
        assert!(perform(Action::Focus, (0, &tree, disabled), &mut focus).is_empty());
        assert_eq!(focus.target(), crate::focus::Target::Pane);
    }

    #[test]
    fn accessibility_presses_focuses_and_sets_values() {
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::container(vec![Node::text("find")])
                .key("field")
                .focusable(),
            Node::text("3 of 12"),
        ]))
        .unwrap();
        let root = tree.root().unwrap();
        let field = tree.children(root)[0];
        let text = tree.children(field)[0];
        let label = tree.children(root)[1];
        let mut focus = Focus::new();
        let told = |events: Vec<NodeEvent>| {
            events
                .into_iter()
                .map(|event| (event.key.0, event.kind))
                .collect::<Vec<_>>()
        };
        // A press of the field's text, which has no key: the field's click.
        let events = perform(Action::Press, (0_u8, &tree, text), &mut focus);
        assert_eq!(told(events), [("field".to_owned(), UiEventKind::Click)]);
        let events = perform(
            Action::SetValue("main".to_owned()),
            (0, &tree, field),
            &mut focus,
        );
        assert_eq!(
            told(events),
            [(
                "field".to_owned(),
                UiEventKind::SetValue {
                    value: "main".to_owned()
                }
            )]
        );
        // A focus of the text: the field's, as its events tell.
        assert!(perform(Action::Focus, (0, &tree, text), &mut focus).is_empty());
        let events: Vec<_> = focus
            .events(|_| Some(&tree))
            .into_iter()
            .map(|(_, event)| (event.key.0, event.kind))
            .collect();
        assert_eq!(events, [("field".to_owned(), UiEventKind::Focus)]);
        // Of a node with no key above it, nothing; a label takes no focus.
        assert!(perform(Action::Press, (0, &tree, label), &mut focus).is_empty());
        perform(Action::Focus, (0, &tree, label), &mut focus);
        assert!(focus.events(|_| Some(&tree)).is_empty());
    }
}

//! The views of tabs, as a settings panel's: a box, and inside it at its top
//! a control of their tabs, each a segment of a label, an icon before it or
//! both, sized as a button's, side by side, over their page, the tab
//! chosen's, as macOS 27's tab views draw them. A click on a tab, or Left
//! and Right while the view has the keyboard's focus, chooses one, past
//! those that are disabled, which its plugin learns of by `select`: the tab
//! chosen is its plugin's, in its render, which gives its page too.

use lokyr_plugin_sdk::view::{Distribution, Kind, Tabs, UiEventKind};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent};

use crate::layout::Layout;
use crate::pointer::NodeEvent;
use crate::tree::{NodeId, Tree};

/// The room inside a view's box about its page, in points: on its sides,
/// below it, and between the control and it.
pub const INSET: f32 = 12.0;

/// The room inside a view's box above its control, in points.
pub const TOP: f32 = 8.0;

/// The radius of the corners of a view's box, in points.
pub const RADIUS: f32 = 6.0;

/// The widths of a control's segments, in pixels, of their contents'
/// `natural` widths, in a control `room` wide, as `distribution` shares it:
/// their own, of `fit`; of an equal share of the room left, of `fill`;
/// equal, of `fill-equally`; as their own share theirs, of
/// `fill-proportionally`. Without room for their own, those that fill
/// share it as their own share theirs.
pub fn widths(natural: &[f32], room: f32, distribution: Distribution) -> Vec<f32> {
    let total: f32 = natural.iter().sum();
    let count = natural.len().max(1) as f32;
    let proportion = |width: f32| {
        if total > 0.0 {
            width * room / total
        } else {
            room / count
        }
    };
    natural
        .iter()
        .map(|&width| match distribution {
            Distribution::Fit => width,
            _ if room < total => proportion(width),
            Distribution::Fill => width + (room - total) / count,
            Distribution::FillEqually => room / count,
            Distribution::FillProportionally => proportion(width),
        })
        .collect()
}

/// The `select` of the tab of the view `node` of `tree` that a click at
/// `at`, in pixels from the tree's corner, is over, as `layout` lays its
/// control out: of a tab that is neither chosen nor disabled.
pub fn clicked(tree: &Tree, layout: &Layout, node: NodeId, at: (f32, f32)) -> Option<NodeEvent> {
    let Some(Kind::Tabs(tabs)) = tree.kind(node) else {
        return None;
    };
    let (x, y) = at;
    let item = layout.segments(tree, node).iter().position(|segment| {
        x >= segment.x
            && x < segment.x + segment.width
            && y >= segment.y
            && y < segment.y + segment.height
    })?;
    chosen(tree, node, tabs, item)
}

/// What `key` does to the view of tabs `node` of `tree`, which has the
/// keyboard's focus: Left and Right, pressed or repeated, choose the tab
/// before and after the one chosen, past those that are disabled, and
/// stop at the ends; `None` for another key, or another node, which the
/// app's bindings may take. A release takes nothing.
pub fn key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<NodeEvent>> {
    let Some(Kind::Tabs(tabs)) = tree.kind(node) else {
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
        Key::Left if plain => false,
        Key::Right if plain => true,
        _ => return None,
    };
    if key.action == KeyAction::Release {
        return Some(Vec::new());
    }
    let next = step(tabs, forward);
    Some(
        next.and_then(|item| chosen(tree, node, tabs, item))
            .into_iter()
            .collect(),
    )
}

/// The first tab of `tabs` past the one chosen, after it if `forward` and
/// before it otherwise, that is not disabled.
fn step(tabs: &Tabs, forward: bool) -> Option<usize> {
    let from = tabs.chosen.min(tabs.tabs.len().saturating_sub(1));
    let enabled = |at: &usize| !tabs.tabs[*at].disabled;
    if forward {
        (from + 1..tabs.tabs.len()).find(enabled)
    } else {
        (0..from).rev().find(enabled)
    }
}

/// The `select` of the tab `item` of the view `node` of `tree`, of
/// `tabs`: none of a tab chosen already, disabled or that it lacks.
fn chosen(tree: &Tree, node: NodeId, tabs: &Tabs, item: usize) -> Option<NodeEvent> {
    let tab = tabs.tabs.get(item)?;
    if tab.disabled || item == tabs.chosen {
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
    use lokyr_plugin_sdk::view::{self, Direction, Distribution, Length, Node, Tab};
    use lokyr_vt_core::input::Mods;

    use super::*;
    use crate::layout::{Frame, Measure};
    use crate::pointer::{Button, Pointer};

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

    /// A settings view of four tabs, the third disabled, of `chosen`, 300
    /// points wide.
    fn settings(chosen: usize) -> Node {
        let tabs = vec![
            Tab::new("General"),
            Tab {
                icon: "settings".to_owned(),
                ..Tab::new("Look")
            },
            Tab {
                disabled: true,
                ..Tab::new("Advanced")
            },
            Tab::new("Keys"),
        ];
        Node::container(vec![
            Node::tabs(tabs, chosen, Node::text("page").key("page")).key("settings"),
        ])
        .layout(view::Layout {
            direction: Direction::Column,
            width: Length::Points(300.0),
            ..view::Layout::default()
        })
    }

    fn laid(chosen: usize, scale: f32) -> (Tree, Layout, NodeId) {
        let mut tree = Tree::new();
        let changes = tree.update(&settings(chosen)).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        let node = tree.by_key(&"settings".into()).unwrap();
        (tree, layout, node)
    }

    #[test]
    fn the_control_s_segments_go_side_by_side_in_the_middle_over_the_page() {
        // Of a button's sizes: `General` 42 and 16 of padding, `Look` of
        // its icon too, 60 and 22 high, `Advanced` 64, `Keys` 40; in the
        // middle of the 276 points inside the box's room, 8 below its top.
        let (tree, layout, node) = laid(0, 1.0);
        let frame = |x, width| Frame {
            x,
            y: 8.0,
            width,
            height: 22.0,
        };
        assert_eq!(layout.control(&tree, node), Some(frame(39.0, 222.0)));
        assert_eq!(
            layout.segments(&tree, node),
            [
                frame(39.0, 58.0),
                frame(97.0, 60.0),
                frame(157.0, 64.0),
                frame(221.0, 40.0)
            ]
        );
        // The page below it, of the box's room; the view about it.
        let page = layout
            .frame(&tree, tree.by_key(&"page".into()).unwrap())
            .unwrap();
        assert_eq!((page.x, page.y), (12.0, 42.0));
        assert_eq!(
            layout.frame(&tree, node).unwrap().height,
            42.0 + 14.0 + 12.0
        );
        // At a scale of 2, in pixels.
        let (tree, layout, node) = laid(0, 2.0);
        assert_eq!(layout.segments(&tree, node)[1].x, 194.0);
        // A label that changes measures anew.
        let mut tree = tree;
        let mut layout = layout;
        let mut longer = settings(0);
        if let Kind::Tabs(view) = &mut longer.children[0].kind {
            view.tabs[0].label = "Generally".to_owned();
        }
        let changes = tree.update(&longer).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        assert_eq!(layout.segments(&tree, node)[0].width, 2.0 * (54.0 + 16.0));
        // Of another scale, its page below it by the room of that scale.
        layout.set_scale(&tree, 1.0);
        layout.compute(&tree, None, None, &mut Fixed);
        let page = layout
            .frame(&tree, tree.by_key(&"page".into()).unwrap())
            .unwrap();
        assert_eq!(page.y, 42.0);
        // Of another font, measured anew.
        struct Wide;
        impl Measure for Wide {
            fn text(&mut self, text: &str) -> (f32, f32) {
                (10.0 * text.chars().count() as f32, 14.0)
            }
            fn icon(&mut self, _: &str) -> (f32, f32) {
                (16.0, 16.0)
            }
        }
        layout.measure_again();
        layout.compute(&tree, None, None, &mut Wide);
        assert_eq!(layout.segments(&tree, node)[0].width, 90.0 + 16.0);
    }

    #[test]
    fn the_distributions_share_the_control_s_width() {
        let natural = [58.0, 60.0, 64.0, 40.0];
        let close = |widths: Vec<f32>, wanted: [f32; 4]| {
            widths
                .iter()
                .zip(wanted)
                .all(|(width, wanted)| (width - wanted).abs() < 0.01)
        };
        // Of 276 points, 54 more than their own 222.
        assert_eq!(widths(&natural, 276.0, Distribution::Fit), natural);
        assert_eq!(
            widths(&natural, 276.0, Distribution::Fill),
            [71.5, 73.5, 77.5, 53.5]
        );
        assert_eq!(
            widths(&natural, 276.0, Distribution::FillEqually),
            [69.0; 4]
        );
        let ratio = 276.0 / 222.0;
        let proportional = natural.map(|width| width * ratio);
        assert!(close(
            widths(&natural, 276.0, Distribution::FillProportionally),
            proportional
        ));
        // Of 111 points, short of their own: as their own share theirs.
        let halves = natural.map(|width| width / 2.0);
        for distribution in [
            Distribution::Fill,
            Distribution::FillEqually,
            Distribution::FillProportionally,
        ] {
            assert!(
                close(widths(&natural, 111.0, distribution), halves),
                "{distribution:?}"
            );
        }
        // Of no content, equal.
        assert_eq!(
            widths(&[0.0, 0.0], 10.0, Distribution::FillProportionally),
            [5.0, 5.0]
        );
    }

    #[test]
    fn a_control_that_fills_takes_the_view_s_width() {
        let mut view = settings(0);
        if let Kind::Tabs(tabs) = &mut view.children[0].kind {
            tabs.distribution = Distribution::FillEqually;
        }
        let mut tree = Tree::new();
        let changes = tree.update(&view).unwrap();
        let mut layout = Layout::new(1.0);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        let node = tree.by_key(&"settings".into()).unwrap();
        let control = layout.control(&tree, node).unwrap();
        assert_eq!((control.x, control.width), (12.0, 276.0));
        let segments = layout.segments(&tree, node);
        assert_eq!(
            segments.iter().map(|segment| segment.x).collect::<Vec<_>>(),
            [12.0, 81.0, 150.0, 219.0]
        );
        // Back to its content's width, in the middle, as a render asks.
        let changes = tree.update(&settings(0)).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        assert_eq!(layout.control(&tree, node).unwrap().x, 39.0);
    }

    #[test]
    fn a_click_on_a_tab_chooses_it_but_the_chosen_and_the_disabled() {
        let (tree, layout, node) = laid(1, 1.0);
        let click = |at| clicked(&tree, &layout, node, at).map(|event| event.kind);
        assert_eq!(click((50.0, 10.0)), Some(UiEventKind::Select { item: 0 }));
        assert_eq!(click((240.0, 10.0)), Some(UiEventKind::Select { item: 3 }));
        assert_eq!(click((120.0, 10.0)), None);
        assert_eq!(click((180.0, 10.0)), None);
        // On the edge between two tabs, the one after it: the chosen one.
        assert_eq!(click((97.0, 10.0)), None);
        assert_eq!(click((96.5, 10.0)), Some(UiEventKind::Select { item: 0 }));
        // Past the control, nothing: beside it, above it in the box, below
        // it.
        assert_eq!(click((20.0, 10.0)), None);
        assert_eq!(click((50.0, 4.0)), None);
        assert_eq!(click((50.0, 30.0)), None);
        // Through the pointer, a click's release over the tab.
        let mut pointer = Pointer::default();
        pointer.pressed(&tree, &layout, (50.0, 10.0), Button::Primary);
        let events = pointer.released(&tree, &layout, (50.0, 10.0), Button::Primary, 1);
        assert!(
            events
                .iter()
                .any(|event| event.kind == UiEventKind::Select { item: 0 }),
            "{events:?}"
        );
    }

    fn pressed(key: Key, mods: Mods, action: KeyAction) -> KeyEvent<'static> {
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
    fn the_arrows_choose_the_tab_beside_past_the_disabled_to_the_ends() {
        let press = |chosen: usize, key: Key| {
            let (tree, _, node) = laid(chosen, 1.0);
            tabs_key(&tree, node, &pressed(key, Mods::NONE, KeyAction::Press))
        };
        let select = |item| Some(vec![UiEventKind::Select { item }]);
        assert_eq!(press(1, Key::Right), select(3));
        assert_eq!(press(3, Key::Left), select(1));
        assert_eq!(press(1, Key::Left), select(0));
        // At the ends, nothing, the key taken.
        assert_eq!(press(0, Key::Left), Some(vec![]));
        assert_eq!(press(3, Key::Right), Some(vec![]));
        // A repeat chooses too; a release takes nothing; another key, or an
        // arrow with a modifier, is left.
        let (tree, _, node) = laid(0, 1.0);
        let shift = Mods {
            shift: true,
            ..Mods::NONE
        };
        let kinds = |key: KeyEvent<'_>| tabs_key(&tree, node, &key);
        assert_eq!(
            kinds(pressed(Key::Right, Mods::NONE, KeyAction::Repeat)),
            select(1)
        );
        assert_eq!(
            kinds(pressed(Key::Right, Mods::NONE, KeyAction::Release)),
            Some(vec![])
        );
        assert_eq!(kinds(pressed(Key::Right, shift, KeyAction::Press)), None);
        assert_eq!(
            kinds(pressed(Key::Down, Mods::NONE, KeyAction::Press)),
            None
        );
        // Of another node, nothing.
        let page = tree.by_key(&"page".into()).unwrap();
        assert_eq!(
            tabs_key(
                &tree,
                page,
                &pressed(Key::Right, Mods::NONE, KeyAction::Press)
            ),
            None
        );
    }

    #[test]
    fn a_surface_gives_the_arrows_to_its_view_of_tabs() {
        use crate::field::Clipboard;
        use crate::surface::Surface;
        struct Nothing;
        impl Clipboard for Nothing {
            fn read(&mut self) -> Option<String> {
                None
            }
            fn write(&mut self, _: &str) {}
        }
        let mut surface = Surface::new(1.0);
        surface.update(&settings(1)).unwrap();
        surface.lay_out((None, None), &mut Fixed);
        let node = surface.tree.by_key(&"settings".into()).unwrap();
        let right = pressed(Key::Right, Mods::NONE, KeyAction::Press);
        let events = surface.key(node, &right, &mut Nothing).unwrap();
        assert_eq!(events[0].kind, UiEventKind::Select { item: 3 });
    }

    /// The kinds of `key`'s events.
    fn tabs_key(tree: &Tree, node: NodeId, key: &KeyEvent<'_>) -> Option<Vec<UiEventKind>> {
        super::key(tree, node, key)
            .map(|events| events.into_iter().map(|event| event.kind).collect())
    }

    #[test]
    fn a_chosen_tab_past_the_tabs_is_refused() {
        let mut tree = Tree::new();
        assert!(tree.update(&settings(4)).is_err());
        assert!(tree.update(&settings(3)).is_ok());
        // Of no tab, any.
        let empty = Node::tabs(Vec::new(), 2, Node::container(vec![])).key("empty");
        assert!(tree.update(&empty).is_ok());
    }
}

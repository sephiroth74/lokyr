//! A window's surfaces in its tree of accessibility: each view tree's
//! nodes as AccessKit's, of a role and a label each, a widget's state, its
//! place in pixels from the window's content, and the actions that a
//! client may ask of it; and those actions, back to the surface's nodes.
//!
//! A node's role is its kind's, a container's, a text's or an icon's its
//! node's own where it gives one, and a panel's root a group, or a tooltip;
//! its label its node's own, or its text, or a button's or a toggle's
//! label, or its tooltip, which tells it otherwise. A spacer, a separator
//! and an icon of no label are left out, as decorations. A list's rows
//! that show are its options, of their items' texts, and a view of tabs'
//! tabs are its own, before its page.
//!
//! A node's id is its slot's in its surface's arena, of the slot's
//! generation, so that a client's element of a node gone finds no other; a
//! row's, of its list's slot and its item, and a tab's, of its view's and
//! its place.

use accesskit::{Action as AxAction, ActionData, ActionRequest, Node, NodeId, Rect, Role, Toggled};
use lokyr_plugin_sdk::view::{Kind, ProgressState, Role as ViewRole};

use super::WindowTree;
use crate::action::{self, Action};
use crate::focus::Focus;
use crate::layout::Frame;
use crate::panel;
use crate::pointer::NodeEvent;
use crate::surface::Surface;
use crate::tabs;
use crate::tree::{self, Tree};

/// The top bits of the id of a surface's node, a row of a list, and a tab
/// of a view of tabs: above every pane's.
const NODE: u64 = 1 << 62;
const ROW: u64 = 2 << 62;
const TAB: u64 = 3 << 62;

/// Where an id keeps its surface's number.
const SURFACE: u32 = 54;

/// The bits of an id that keep a node's slot's generation, or a row's
/// list's slot, or a tab's view's, above the 32 of the slot or of the
/// part.
const MIDDLE: u64 = (1 << 22) - 1;

/// A surface of a window, as its tree takes it: its content's, a slot's
/// of its chrome, or a panel's.
#[derive(Clone, Copy)]
pub struct SurfaceAccess<'a> {
    /// Its number, unique in its window, which its nodes' ids take.
    pub number: u8,
    /// The surface.
    pub surface: &'a Surface,
    /// Its top left corner, in pixels from the window's content's.
    pub corner: (f32, f32),
    /// The role of a panel's, whose root is then a group, or a tooltip;
    /// `None` of the content and of the chrome.
    pub panel: Option<panel::Role>,
}

/// The id of the node `node` of the surface `surface`, as the window's
/// tree gives it, which the keyboard's focus takes.
pub fn node_id(surface: u8, node: tree::NodeId) -> NodeId {
    NodeId(
        NODE | u64::from(surface) << SURFACE
            | (u64::from(node.generation()) & MIDDLE) << 32
            | u64::from(node.index()),
    )
}

/// The id of the part `part`, a row's item or a tab's place, of `node` of
/// the surface `surface`, of the kind `kind`.
fn part_id(kind: u64, surface: u8, node: tree::NodeId, part: usize) -> NodeId {
    // A list's items past `u32::MAX` take the ids of those before them,
    // which no rows that show together share.
    let part = part as u64 & u64::from(u32::MAX);
    NodeId(kind | u64::from(surface) << SURFACE | (u64::from(node.index()) & MIDDLE) << 32 | part)
}

/// The surface whose node, row or tab `node` is, if it is one of a
/// surface's.
pub fn surface_of(node: NodeId) -> Option<u8> {
    (node.0 >> 62 != 0).then_some((node.0 >> SURFACE) as u8)
}

/// What an id of a surface's names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    /// A node, by its slot and its slot's generation, of the id's bits.
    Node { index: u32, generation: u32 },
    /// A row of a list, by the list's slot and its item.
    Row { list: u32, item: usize },
    /// A tab of a view of tabs, by the view's slot and its place.
    Tab { view: u32, tab: usize },
}

/// What `node`, an id of a surface's, names.
fn part(node: NodeId) -> Option<Part> {
    let middle = ((node.0 >> 32) & MIDDLE) as u32;
    let low = (node.0 & u64::from(u32::MAX)) as u32;
    match node.0 & (3 << 62) {
        NODE => Some(Part::Node {
            index: low,
            generation: middle,
        }),
        ROW => Some(Part::Row {
            list: middle,
            item: low as usize,
        }),
        TAB => Some(Part::Tab {
            view: middle,
            tab: low as usize,
        }),
        _ => None,
    }
}

/// The node of `tree` in the slot `index`, if one is there and `takes` it.
fn found(tree: &Tree, index: u32, takes: impl Fn(tree::NodeId) -> bool) -> Option<tree::NodeId> {
    tree.at(index).filter(|&node| takes(node))
}

/// Does a client's `request` to `surface`, which `panel` is to the focus,
/// whose node, row or tab it names: a press, a focus or a value set, as
/// [`action::perform`] does them, a row's press its selection, and a tab's
/// its choice. The events of its nodes; a focus moves `focus`, whose own
/// events tell it.
pub fn perform<P: Copy + Eq>(
    request: &ActionRequest,
    (panel, surface): (P, &mut Surface),
    focus: &mut Focus<P>,
) -> Vec<NodeEvent> {
    let asked = match (request.action, &request.data) {
        (AxAction::Click, _) => Action::Press,
        (AxAction::Focus, _) => Action::Focus,
        (AxAction::SetValue, Some(ActionData::Value(value))) => Action::SetValue(value.to_string()),
        _ => return Vec::new(),
    };
    let Some(part) = part(request.target_node) else {
        return Vec::new();
    };
    let tree = &surface.tree;
    match part {
        Part::Node { index, generation } => {
            let Some(node) = found(tree, index, |node| {
                u64::from(node.generation()) & MIDDLE == u64::from(generation)
            }) else {
                return Vec::new();
            };
            action::perform(asked, (panel, tree, node), focus)
        }
        Part::Row { list, item } => {
            let Some(node) = found(tree, list, |node| {
                matches!(tree.kind(node), Some(Kind::List(_)))
            }) else {
                return Vec::new();
            };
            match asked {
                Action::Press => surface.lists.choose(tree, node, item),
                Action::Focus => action::perform(Action::Focus, (panel, tree, node), focus),
                Action::SetValue(_) => Vec::new(),
            }
        }
        Part::Tab { view, tab } => {
            let Some(node) = found(tree, view, |node| {
                matches!(tree.kind(node), Some(Kind::Tabs(_)))
            }) else {
                return Vec::new();
            };
            match (asked, tree.kind(node)) {
                (Action::Press, Some(Kind::Tabs(tabs))) => {
                    tabs::chosen(tree, node, tabs, tab).into_iter().collect()
                }
                (Action::Focus, _) => action::perform(Action::Focus, (panel, tree, node), focus),
                _ => Vec::new(),
            }
        }
    }
}

impl WindowTree {
    /// Adds the surface of `access`, over what the window has before it.
    pub fn surface(&mut self, access: &SurfaceAccess<'_>) {
        let Some(root) = access.surface.tree.root() else {
            return;
        };
        if let Some(id) = self.view_node(access, root, true) {
            self.children.push(id);
        }
    }

    /// Adds the node `id` of the surface of `access`, the surface's root
    /// if `root`, with its children, its rows and its tabs: its own id, or
    /// none of a node left out.
    fn view_node(
        &mut self,
        access: &SurfaceAccess<'_>,
        id: tree::NodeId,
        root: bool,
    ) -> Option<NodeId> {
        let surface = access.surface;
        let tree = &surface.tree;
        let kind = tree.kind(id)?;
        let label = label(tree, id, kind);
        let role = match (kind, tree.role(id)) {
            (Kind::Spacer | Kind::Separator, _) => return None,
            (Kind::Icon(_), None) if label.is_none() => return None,
            (Kind::Container | Kind::Text(_) | Kind::Icon(_) | Kind::Scroll, Some(role)) => {
                role_of(role)
            }
            (Kind::Container, None) if root => match access.panel {
                Some(panel::Role::Tooltip) => Role::Tooltip,
                Some(_) => Role::Group,
                None => Role::GenericContainer,
            },
            (Kind::Container, None) => Role::GenericContainer,
            (Kind::Text(_), None) => Role::Label,
            (Kind::Icon(_), None) => Role::Image,
            (Kind::Scroll, None) => Role::ScrollView,
            (Kind::Field(_), _) => Role::TextInput,
            (Kind::Button(_) | Kind::Toggle(_), _) => Role::Button,
            (Kind::List(_), _) => Role::ListBox,
            (Kind::Progress(_), _) => Role::ProgressIndicator,
            (Kind::Tabs(_), _) => Role::TabList,
        };
        let ax = node_id(access.number, id);
        let mut node = Node::new(role);
        if let Some(label) = &label {
            node.set_label(label.as_str());
        }
        if let Some(tooltip) = tree
            .tooltip(id)
            .filter(|&tooltip| Some(tooltip) != label.as_deref())
        {
            node.set_description(tooltip);
        }
        let frame = surface.layout.frame(tree, id);
        if let Some(frame) = frame {
            node.set_bounds(bounds(access.corner, frame));
        }
        if tree.focusable(id) {
            node.add_action(AxAction::Focus);
        }
        let mut children = Vec::new();
        match kind {
            Kind::Field(field) => {
                let text = tree
                    .key(id)
                    .and_then(|key| surface.fields.state(key))
                    .map_or("", |state| state.text());
                node.set_value(text);
                if !field.placeholder.is_empty() {
                    node.set_placeholder(field.placeholder.as_str());
                }
                node.add_action(AxAction::SetValue);
            }
            Kind::Button(button) => pressable(&mut node, button.disabled),
            Kind::Toggle(toggle) => {
                node.set_toggled(if toggle.on {
                    Toggled::True
                } else {
                    Toggled::False
                });
                pressable(&mut node, toggle.disabled);
            }
            Kind::Progress(progress) if progress.state != ProgressState::Indeterminate => {
                node.set_numeric_value(f64::from(progress.value.clamp(0.0, 1.0)) * 100.0);
                node.set_min_numeric_value(0.0);
                node.set_max_numeric_value(100.0);
            }
            Kind::List(list) => {
                let focusable = tree.focusable(id);
                let state = tree.key(id).and_then(|key| surface.lists.state(key));
                if let (Some(state), Some(frame)) = (state, frame) {
                    let scale = surface.layout.scale();
                    for (index, top) in state.visible(list) {
                        let row_id = part_id(ROW, access.number, id, index);
                        let mut row = Node::new(Role::ListBoxOption);
                        let item = index
                            .checked_sub(list.first)
                            .and_then(|at| list.items.get(at));
                        if let Some(item) = item {
                            row.set_label(item.texts.join(", "));
                        }
                        let selected = state.selected() == Some(index);
                        row.set_selected(selected);
                        row.set_position_in_set(index + 1);
                        row.set_size_of_set(list.count);
                        row.set_bounds(bounds(
                            access.corner,
                            Frame {
                                y: frame.y + top * scale,
                                height: state.height_of(list, index) * scale,
                                ..frame
                            },
                        ));
                        row.add_action(AxAction::Click);
                        if focusable {
                            row.add_action(AxAction::Focus);
                        }
                        if selected {
                            node.set_active_descendant(row_id);
                        }
                        children.push(row_id);
                        self.nodes.push((row_id, row));
                    }
                }
            }
            Kind::Tabs(view) => {
                let segments = surface.layout.segments(tree, id);
                for (place, tab) in view.tabs.iter().enumerate() {
                    let tab_id = part_id(TAB, access.number, id, place);
                    let mut node_of_tab = Node::new(Role::Tab);
                    node_of_tab.set_label(tab.label.as_str());
                    let chosen = place == view.chosen;
                    node_of_tab.set_selected(chosen);
                    if let Some(&segment) = segments.get(place) {
                        node_of_tab.set_bounds(bounds(access.corner, segment));
                    }
                    pressable(&mut node_of_tab, tab.disabled);
                    if chosen {
                        node.set_active_descendant(tab_id);
                    }
                    children.push(tab_id);
                    self.nodes.push((tab_id, node_of_tab));
                }
            }
            _ => {}
        }
        for &child in tree.children(id) {
            children.extend(self.view_node(access, child, false));
        }
        node.set_children(children);
        self.nodes.push((ax, node));
        Some(ax)
    }
}

/// What a client calls the node `id` of `tree`, of `kind`: its node's own
/// label, its text, a button's or a toggle's label, or its tooltip.
fn label(tree: &Tree, id: tree::NodeId, kind: &Kind) -> Option<String> {
    tree.label(id)
        .map(str::to_owned)
        .or_else(|| match kind {
            Kind::Text(text) => Some(text.clone()),
            Kind::Button(button) if !button.label.is_empty() => Some(button.label.clone()),
            Kind::Toggle(toggle) if !toggle.label.is_empty() => Some(toggle.label.clone()),
            _ => None,
        })
        .or_else(|| tree.tooltip(id).map(str::to_owned))
}

/// AccessKit's role of a node's `role`.
fn role_of(role: ViewRole) -> Role {
    match role {
        ViewRole::Group => Role::Group,
        ViewRole::Toolbar => Role::Toolbar,
        ViewRole::Status => Role::Status,
        ViewRole::Alert => Role::Alert,
        ViewRole::Heading => Role::Heading,
        ViewRole::Image => Role::Image,
    }
}

/// A press of `node`, as of a button: none of one `disabled`, which says
/// so.
fn pressable(node: &mut Node, disabled: bool) {
    if disabled {
        node.set_disabled();
    } else {
        node.add_action(AxAction::Click);
    }
}

/// The rectangle of `frame`, in pixels of a surface whose corner is at
/// `corner` in the window's content.
fn bounds((left, top): (f32, f32), frame: Frame) -> Rect {
    Rect {
        x0: f64::from(left + frame.x),
        y0: f64::from(top + frame.y),
        x1: f64::from(left + frame.x + frame.width),
        y1: f64::from(top + frame.y + frame.height),
    }
}

#[cfg(test)]
mod tests {
    use accesskit::TreeUpdate;
    use lokyr_plugin_sdk::view::{
        self, Direction, Length, ListItem, Node as ViewNode, ProgressShape, Tab, UiEventKind,
    };

    use super::*;
    use crate::focus::Target;
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

    /// A surface of `root`, at a scale of 2, laid out in 300 points.
    fn surface(root: &ViewNode) -> Surface {
        let mut surface = Surface::new(2.0);
        surface.update(root).unwrap();
        surface.lay_out((Some(600.0), None), &mut Fixed);
        surface
    }

    /// The window's tree of `surface`, a panel of `panel` at 10, 20.
    fn tree_of(surface: &Surface, panel: Option<panel::Role>) -> TreeUpdate {
        let mut tree = WindowTree::new();
        tree.surface(&SurfaceAccess {
            number: 3,
            surface,
            corner: (10.0, 20.0),
            panel,
        });
        tree.finish("Gallery", None)
    }

    fn node(update: &TreeUpdate, id: NodeId) -> &Node {
        &update.nodes.iter().find(|(at, _)| *at == id).unwrap().1
    }

    /// The node of the surface's node of `key`.
    fn keyed<'u>(update: &'u TreeUpdate, surface: &Surface, key: &str) -> &'u Node {
        node(
            update,
            node_id(3, surface.tree.by_key(&key.into()).unwrap()),
        )
    }

    /// A column of `children`.
    fn column(children: Vec<ViewNode>) -> ViewNode {
        ViewNode::container(children).layout(view::Layout {
            direction: Direction::Column,
            ..view::Layout::default()
        })
    }

    #[test]
    fn a_surface_s_nodes_are_of_their_roles_labels_and_states() {
        let mut disabled = ViewNode::button("Replace All").key("all");
        if let Kind::Button(button) = &mut disabled.kind {
            button.disabled = true;
        }
        let root = column(vec![
            ViewNode::text("Find").role(ViewRole::Heading),
            ViewNode::field("Search").key("find"),
            ViewNode::toggle("case-sensitive", true)
                .key("case")
                .label("Match Case"),
            ViewNode::toggle("regex", false)
                .key("regex")
                .tooltip("Regular Expression"),
            ViewNode::button("Replace")
                .key("replace")
                .tooltip("Replace the match"),
            disabled,
            ViewNode::icon_button("close").key("close").tooltip("Close"),
            ViewNode::icon("search"),
            ViewNode::icon("warning").key("warning").tooltip("Too many"),
            ViewNode::spacer(),
            ViewNode::separator(),
            ViewNode::progress(ProgressShape::Bar, ProgressState::Normal, 0.5),
            ViewNode::progress(ProgressShape::Ring, ProgressState::Indeterminate, 0.0),
        ]);
        let mut surface = surface(&root);
        surface.fields.set_text(&"find".into(), "needle");
        let update = tree_of(&surface, Some(panel::Role::Dialog));
        // The panel a group, of its nodes but those left out.
        let root = node(&update, node_id(3, surface.tree.root().unwrap()));
        assert_eq!(root.role(), Role::Group);
        assert_eq!(root.children().len(), 10);
        let window = node(&update, super::super::WINDOW);
        assert_eq!(
            window.children(),
            [node_id(3, surface.tree.root().unwrap())]
        );
        let roles: Vec<Role> = root
            .children()
            .iter()
            .map(|&id| node(&update, id).role())
            .collect();
        assert_eq!(
            roles,
            [
                Role::Heading,
                Role::TextInput,
                Role::Button,
                Role::Button,
                Role::Button,
                Role::Button,
                Role::Button,
                Role::Image,
                Role::ProgressIndicator,
                Role::ProgressIndicator,
            ]
        );
        let heading = node(&update, root.children()[0]);
        assert_eq!(heading.label(), Some("Find"));
        // The field, of its text, its placeholder, a focus and a value set.
        let field = keyed(&update, &surface, "find");
        assert_eq!(field.value(), Some("needle"));
        assert_eq!(field.placeholder(), Some("Search"));
        assert!(field.supports_action(AxAction::SetValue));
        assert!(field.supports_action(AxAction::Focus));
        // The toggle, of its node's label and its state.
        let case = keyed(&update, &surface, "case");
        assert_eq!(case.label(), Some("Match Case"));
        assert_eq!(case.toggled(), Some(Toggled::True));
        assert!(case.supports_action(AxAction::Click));
        let regex = keyed(&update, &surface, "regex");
        assert_eq!(regex.label(), Some("Regular Expression"));
        assert_eq!(regex.toggled(), Some(Toggled::False));
        // A button of its label, its tooltip told apart; one disabled; one
        // of its icon alone, of its tooltip.
        let replace = keyed(&update, &surface, "replace");
        assert_eq!(replace.label(), Some("Replace"));
        assert_eq!(replace.description(), Some("Replace the match"));
        assert_eq!(replace.toggled(), None);
        let all = keyed(&update, &surface, "all");
        assert!(all.is_disabled() && !all.supports_action(AxAction::Click));
        let close = keyed(&update, &surface, "close");
        assert_eq!((close.label(), close.description()), (Some("Close"), None));
        // The icon of a tooltip, an image; the one of none, left out.
        assert_eq!(
            keyed(&update, &surface, "warning").label(),
            Some("Too many")
        );
        // A progress halfway, of no value while indeterminate.
        let bar = node(&update, root.children()[8]);
        assert_eq!(bar.numeric_value(), Some(50.0));
        assert_eq!(
            (bar.min_numeric_value(), bar.max_numeric_value()),
            (Some(0.0), Some(100.0))
        );
        assert_eq!(node(&update, root.children()[9]).numeric_value(), None);
        // Each of its place in the window, of the surface's corner.
        let frame = surface
            .layout
            .frame(&surface.tree, surface.tree.by_key(&"find".into()).unwrap())
            .unwrap();
        let bounds = field.bounds().unwrap();
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (
                f64::from(10.0 + frame.x),
                f64::from(20.0 + frame.y),
                f64::from(10.0 + frame.x + frame.width),
                f64::from(20.0 + frame.y + frame.height)
            )
        );
        // A tooltip's panel is one; the content's root, a container.
        let update = tree_of(&surface, Some(panel::Role::Tooltip));
        assert_eq!(update.nodes.last().unwrap().1.role(), Role::Tooltip);
        let update = tree_of(&surface, None);
        assert_eq!(
            update.nodes.last().unwrap().1.role(),
            Role::GenericContainer
        );
    }

    #[test]
    fn a_node_s_own_role_is_a_container_s_a_text_s_or_an_icon_s() {
        let roles = [
            (ViewRole::Group, Role::Group),
            (ViewRole::Toolbar, Role::Toolbar),
            (ViewRole::Status, Role::Status),
            (ViewRole::Alert, Role::Alert),
        ];
        let mut children: Vec<ViewNode> = roles
            .iter()
            .enumerate()
            .map(|(at, &(role, _))| {
                ViewNode::container(vec![ViewNode::text("a")])
                    .key(format!("c{at}").as_str())
                    .role(role)
            })
            .collect();
        children.push(ViewNode::text("Title").key("t").role(ViewRole::Heading));
        children.push(
            ViewNode::icon("bell")
                .key("i")
                .label("Bell")
                .role(ViewRole::Image),
        );
        children.push(ViewNode::icon("bell").key("j").role(ViewRole::Image));
        // A widget keeps its own.
        children.push(ViewNode::button("Go").key("b").role(ViewRole::Group));
        let surface = surface(&column(children));
        let update = tree_of(&surface, None);
        for (at, &(_, role)) in roles.iter().enumerate() {
            assert_eq!(keyed(&update, &surface, &format!("c{at}")).role(), role);
        }
        assert_eq!(keyed(&update, &surface, "t").role(), Role::Heading);
        let bell = keyed(&update, &surface, "i");
        assert_eq!((bell.role(), bell.label()), (Role::Image, Some("Bell")));
        assert_eq!(keyed(&update, &surface, "j").role(), Role::Image);
        assert_eq!(keyed(&update, &surface, "b").role(), Role::Button);
    }

    /// A surface of a list of 1 000 items, 100 points high, of rows of 20,
    /// the items from 0 to 10 given.
    fn listed() -> Surface {
        listed_from(0)
    }

    /// A surface of a list of 1 000 items, 100 points high, of rows of 20,
    /// the ten items from `first` given.
    fn listed_from(first: usize) -> Surface {
        let items = (first..first + 10)
            .map(|index| ListItem {
                texts: vec![format!("row {index}"), "of the list".to_owned()],
            })
            .collect();
        let mut list = ViewNode::list(
            ViewNode::container(vec![ViewNode::text(""), ViewNode::text("")]),
            1000,
            (first, items),
        )
        .key("rows")
        .layout(view::Layout {
            height: Length::Points(100.0),
            ..view::Layout::default()
        });
        if let Kind::List(list) = &mut list.kind {
            list.row_height = Some(20.0);
        }
        surface(&column(vec![list]))
    }

    #[test]
    fn a_list_s_rows_that_show_are_its_options() {
        let mut surface = listed();
        let list = surface.tree.by_key(&"rows".into()).unwrap();
        surface.lists.select(&surface.tree, list, 2);
        let update = tree_of(&surface, None);
        let node_of_list = node(&update, node_id(3, list));
        assert_eq!(node_of_list.role(), Role::ListBox);
        let rows = node_of_list.children();
        assert_eq!(rows.len(), 5);
        let labels: Vec<_> = rows.iter().map(|&id| node(&update, id).label()).collect();
        assert_eq!(labels[0], Some("row 0, of the list"));
        let third = node(&update, rows[2]);
        assert_eq!(third.role(), Role::ListBoxOption);
        assert_eq!(third.is_selected(), Some(true));
        assert_eq!(node(&update, rows[1]).is_selected(), Some(false));
        assert_eq!(
            (third.position_in_set(), third.size_of_set()),
            (Some(3), Some(1000))
        );
        assert_eq!(node_of_list.active_descendant(), Some(rows[2]));
        assert!(third.supports_action(AxAction::Click) && third.supports_action(AxAction::Focus));
        let frame = surface.layout.frame(&surface.tree, list).unwrap();
        let bounds = third.bounds().unwrap();
        assert_eq!(
            (bounds.y0, bounds.y1),
            (
                f64::from(20.0 + frame.y + 80.0),
                f64::from(20.0 + frame.y + 120.0)
            )
        );
        // The rows of the items given alone, from theirs.
        let late = listed_from(2);
        let list_of_late = late.tree.by_key(&"rows".into()).unwrap();
        let update = tree_of(&late, None);
        let rows = node(&update, node_id(3, list_of_late)).children();
        assert_eq!(node(&update, rows[0]).label(), None);
        assert_eq!(node(&update, rows[2]).label(), Some("row 2, of the list"));
        // Rows past the items given, of no label yet.
        surface.lists.select(&surface.tree, list, 500);
        let update = tree_of(&surface, None);
        let rows = node(&update, node_id(3, list)).children();
        assert!(rows.iter().all(|&id| node(&update, id).label().is_none()));
        assert!(
            rows.iter()
                .any(|&id| node(&update, id).is_selected() == Some(true))
        );
    }

    /// A surface of a view of three tabs, the second disabled, of a page.
    fn tabbed() -> Surface {
        let mut tabs = vec![Tab::new("General"), Tab::new("Profiles"), Tab::new("Keys")];
        tabs[1].disabled = true;
        surface(&column(vec![
            ViewNode::tabs(tabs, 0, ViewNode::text("General's page")).key("views"),
        ]))
    }

    #[test]
    fn a_view_of_tabs_tabs_are_its_own_before_its_page() {
        let surface = tabbed();
        let update = tree_of(&surface, None);
        let view = keyed(&update, &surface, "views");
        assert_eq!(view.role(), Role::TabList);
        let children = view.children();
        assert_eq!(children.len(), 4);
        let tabs: Vec<&Node> = children[..3].iter().map(|&id| node(&update, id)).collect();
        assert!(tabs.iter().all(|tab| tab.role() == Role::Tab));
        assert_eq!(tabs[0].label(), Some("General"));
        assert_eq!(tabs[0].is_selected(), Some(true));
        assert_eq!(tabs[2].is_selected(), Some(false));
        assert!(tabs[1].is_disabled() && !tabs[1].supports_action(AxAction::Click));
        assert_eq!(view.active_descendant(), Some(children[0]));
        assert_eq!(node(&update, children[3]).label(), Some("General's page"));
        // Each of its segment's place, side by side, in the window.
        let (first, second) = (tabs[0].bounds().unwrap(), tabs[1].bounds().unwrap());
        assert_eq!(first.x1, second.x0);
        assert!(first.x0 < first.x1);
        let segment = surface
            .layout
            .segments(&surface.tree, surface.tree.by_key(&"views".into()).unwrap())[0];
        assert_eq!(
            (first.x0, first.y0),
            (f64::from(10.0 + segment.x), f64::from(20.0 + segment.y))
        );
    }

    fn request(action: AxAction, target: NodeId, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: accesskit::TreeId::ROOT,
            target_node: target,
            data,
        }
    }

    fn told(events: Vec<NodeEvent>) -> Vec<(String, UiEventKind)> {
        events
            .into_iter()
            .map(|event| (event.key.0, event.kind))
            .collect()
    }

    #[test]
    fn a_client_presses_focuses_and_fills_nodes_rows_and_tabs() {
        let root = column(vec![
            ViewNode::field("Search").key("find"),
            ViewNode::toggle("case-sensitive", false).key("case"),
        ]);
        let mut surface = surface(&root);
        let mut focus: Focus<u8> = Focus::new();
        let of =
            |surface: &Surface, key: &str| node_id(3, surface.tree.by_key(&key.into()).unwrap());
        // A press of the toggle switches it; a value set of the field
        // comes to it; a focus moves to it.
        let case = of(&surface, "case");
        let events = perform(
            &request(AxAction::Click, case, None),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(
            told(events),
            [("case".to_owned(), UiEventKind::Toggle { on: true })]
        );
        let find = of(&surface, "find");
        let value = Some(ActionData::Value("needle".into()));
        let events = perform(
            &request(AxAction::SetValue, find, value),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(
            told(events),
            [(
                "find".to_owned(),
                UiEventKind::SetValue {
                    value: "needle".to_owned()
                }
            )]
        );
        assert!(
            perform(
                &request(AxAction::Focus, find, None),
                (3, &mut surface),
                &mut focus
            )
            .is_empty()
        );
        assert_eq!(
            focus.target(),
            Target::Node {
                panel: 3,
                node: surface.tree.by_key(&"find".into()).unwrap()
            }
        );
        // A value set without its value, or another action, does nothing.
        assert!(
            perform(
                &request(AxAction::SetValue, find, None),
                (3, &mut surface),
                &mut focus
            )
            .is_empty()
        );
        assert!(
            perform(
                &request(AxAction::ScrollDown, case, None),
                (3, &mut surface),
                &mut focus
            )
            .is_empty()
        );
        // A node gone, whose slot another took, is no other's.
        let gone = of(&surface, "case");
        surface
            .update(&column(vec![ViewNode::field("Search").key("find")]))
            .unwrap();
        surface
            .update(&column(vec![
                ViewNode::field("Search").key("find"),
                ViewNode::button("New").key("new"),
            ]))
            .unwrap();
        let new = surface.tree.by_key(&"new".into()).unwrap();
        assert_eq!(new.index(), surface_index(gone));
        assert!(
            perform(
                &request(AxAction::Click, gone, None),
                (3, &mut surface),
                &mut focus
            )
            .is_empty()
        );
        let events = perform(
            &request(AxAction::Click, node_id(3, new), None),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(told(events), [("new".to_owned(), UiEventKind::Click)]);
        // A pane's id, or the window's, is none of a surface's.
        assert_eq!(surface_of(super::super::pane_id(0)), None);
        assert_eq!(surface_of(super::super::WINDOW), None);
        assert_eq!(surface_of(find), Some(3));
        assert_eq!(super::super::pane_of(find), None);
    }

    /// The slot of the id `node`.
    fn surface_index(node: NodeId) -> u32 {
        match part(node) {
            Some(Part::Node { index, .. }) => index,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_client_selects_a_row_and_chooses_a_tab() {
        let mut surface = listed();
        let mut focus: Focus<u8> = Focus::new();
        let list = surface.tree.by_key(&"rows".into()).unwrap();
        let update = tree_of(&surface, None);
        let rows = node(&update, node_id(3, list)).children().to_vec();
        let events = perform(
            &request(AxAction::Click, rows[3], None),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(
            told(events),
            [("rows".to_owned(), UiEventKind::Select { item: 3 })]
        );
        assert_eq!(
            surface.lists.state(&"rows".into()).unwrap().selected(),
            Some(3)
        );
        // Again, nothing; a focus of a row, its list's.
        assert!(
            perform(
                &request(AxAction::Click, rows[3], None),
                (3, &mut surface),
                &mut focus
            )
            .is_empty()
        );
        perform(
            &request(AxAction::Focus, rows[1], None),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(
            focus.target(),
            Target::Node {
                panel: 3,
                node: list
            }
        );
        // A tab: the third chosen; the chosen one and the disabled, nothing.
        let mut surface = tabbed();
        let mut focus: Focus<u8> = Focus::new();
        let view = surface.tree.by_key(&"views".into()).unwrap();
        let update = tree_of(&surface, None);
        let tabs = keyed(&update, &surface, "views").children().to_vec();
        let events = perform(
            &request(AxAction::Click, tabs[2], None),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(
            told(events),
            [("views".to_owned(), UiEventKind::Select { item: 2 })]
        );
        for tab in &tabs[..2] {
            assert!(
                perform(
                    &request(AxAction::Click, *tab, None),
                    (3, &mut surface),
                    &mut focus
                )
                .is_empty()
            );
        }
        perform(
            &request(AxAction::Focus, tabs[2], None),
            (3, &mut surface),
            &mut focus,
        );
        assert_eq!(
            focus.target(),
            Target::Node {
                panel: 3,
                node: view
            }
        );
        // A row's id of a tab's view, or a tab's of a list, finds nothing.
        let wrong = NodeId(tabs[2].0 & !(3 << 62) | ROW);
        assert!(
            perform(
                &request(AxAction::Click, wrong, None),
                (3, &mut surface),
                &mut focus
            )
            .is_empty()
        );
    }

    #[test]
    fn the_focus_is_a_node_of_the_tree_or_the_window_s() {
        let surface = tabbed();
        let view = surface.tree.by_key(&"views".into()).unwrap();
        let mut tree = WindowTree::new();
        tree.surface(&SurfaceAccess {
            number: 3,
            surface: &surface,
            corner: (0.0, 0.0),
            panel: None,
        });
        assert_eq!(
            tree.finish("", Some(node_id(3, view))).focus,
            node_id(3, view)
        );
        let mut tree = WindowTree::new();
        tree.surface(&SurfaceAccess {
            number: 3,
            surface: &surface,
            corner: (0.0, 0.0),
            panel: None,
        });
        assert_eq!(
            tree.finish("", Some(node_id(4, view))).focus,
            super::super::WINDOW
        );
    }
}

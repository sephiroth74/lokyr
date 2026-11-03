//! The floating panels of a window: each a view tree of its own, over the
//! window's content and over the panels opened before it, anchored below a
//! rectangle of the window, as a cell's or the cursor's, at a point, as the
//! pointer's, or in a corner of a rectangle, as the pane's or the window's;
//! kept within the window as it resizes, on the other side of its anchor
//! where the window has no room on its own.
//!
//! A panel's role tells what it is to the system, so that a platform can
//! show a menu as its own: a tooltip, which takes neither the pointer nor
//! the keyboard; a menu, which takes the keyboard as it opens, and which a
//! click outside it closes without the click going on; or a dialog, which
//! takes the keyboard as it opens. A click outside a panel, or Escape
//! within it, closes it where its plugin asks so.
//!
//! A menu's panel of menu items alone, buttons and toggles of labels,
//! separators, and submenus, each a container of a text, its title, then
//! its items, becomes the platform's own menu where it has one, which the
//! system places, out of the window too: [`native_menu`] gives its entries,
//! each of the event that its node's press would tell.

use lokyr_plugin_sdk::view::{Key as NodeKey, Kind, UiEventKind};
use lokyr_render_core::Rect;

use crate::layout::Frame;
use crate::tree::{NodeId, Tree};

/// Where a panel goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    /// Below a rectangle of the window, in pixels, as a cell's or the
    /// cursor's, its left edge at the rectangle's; above it where the
    /// window has no room below.
    Below(Frame),
    /// At a point of the window, in pixels, as the pointer's, its top left
    /// corner there; leftwards or upwards where the window has no room.
    At((f32, f32)),
    /// In a corner of a rectangle of the window, in pixels, as the pane's or
    /// the window's, a margin of pixels in from its edges.
    Corner(Frame, Corner, f32),
}

/// A corner of a rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    /// The top left one.
    TopLeft,
    /// The top right one.
    TopRight,
    /// The bottom left one.
    BottomLeft,
    /// The bottom right one.
    BottomRight,
}

/// What a panel is to the system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    /// A tooltip: it takes neither the pointer, which goes through it, nor
    /// the keyboard.
    Tooltip,
    /// A menu: it takes the keyboard as it opens, and a click outside it
    /// that closes it goes no further.
    Menu,
    /// A dialog, as a find bar: it takes the keyboard as it opens.
    #[default]
    Dialog,
}

impl Role {
    /// Whether a panel of the role takes the keyboard's focus as it opens.
    pub fn takes_focus(self) -> bool {
        self != Self::Tooltip
    }
}

/// What closes a panel, as its plugin asks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dismiss {
    /// A click outside it.
    pub outside: bool,
    /// Escape, while it has the keyboard's focus.
    pub escape: bool,
}

/// A panel's anchor, role and what closes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// Where it goes.
    pub anchor: Anchor,
    /// What it is.
    pub role: Role,
    /// What closes it.
    pub dismiss: Dismiss,
}

/// The place of a panel of `size`, in pixels, within a window of `window`,
/// as `anchor` says: its top left corner, kept within the window, its top
/// and its left edges first where the window is smaller than it.
pub fn place(anchor: &Anchor, size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let (width, height) = size;
    let (x, y) = match *anchor {
        Anchor::Below(rect) => {
            let below = rect.y + rect.height;
            let y = if below + height <= window.1 || rect.y - height < 0.0 {
                below
            } else {
                rect.y - height
            };
            (rect.x, y)
        }
        Anchor::At((x, y)) => {
            let x = if x + width <= window.0 { x } else { x - width };
            let y = if y + height <= window.1 {
                y
            } else {
                y - height
            };
            (x, y)
        }
        Anchor::Corner(rect, corner, margin) => {
            let left = rect.x + margin;
            let right = rect.x + rect.width - margin - width;
            let top = rect.y + margin;
            let bottom = rect.y + rect.height - margin - height;
            match corner {
                Corner::TopLeft => (left, top),
                Corner::TopRight => (right, top),
                Corner::BottomLeft => (left, bottom),
                Corner::BottomRight => (right, bottom),
            }
        }
    };
    let within = |at: f32, length: f32, room: f32| at.min(room - length).max(0.0).round();
    (within(x, width, window.0), within(y, height, window.1))
}

/// The panels of a window, `P` naming each, from the lowest to the topmost,
/// and where each was placed last.
#[derive(Debug)]
pub struct Panels<P> {
    stack: Vec<(P, Options, Option<Frame>)>,
}

impl<P> Default for Panels<P> {
    fn default() -> Self {
        Self { stack: Vec::new() }
    }
}

/// What a press outside the panels did: the panels that it closed, and
/// whether it goes no further, as a menu's closing takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outside<P> {
    /// The panels closed, from the topmost.
    pub closed: Vec<P>,
    /// Whether the press goes no further.
    pub taken: bool,
}

impl<P: Copy + Eq> Panels<P> {
    /// No panel.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens `panel` over the others, of `options`, or raises it with new
    /// ones if it is open.
    pub fn open(&mut self, panel: P, options: Options) {
        self.stack.retain(|(id, ..)| *id != panel);
        self.stack.push((panel, options, None));
    }

    /// Anchors `panel` anew, in its place among the others, as its anchor
    /// moved, a window's corner as it resizes: whether it is open.
    pub fn anchor(&mut self, panel: P, anchor: Anchor) -> bool {
        match self.stack.iter_mut().find(|(id, ..)| *id == panel) {
            Some((_, options, _)) => {
                options.anchor = anchor;
                true
            }
            None => false,
        }
    }

    /// Closes `panel`: whether it was open.
    pub fn close(&mut self, panel: P) -> bool {
        let before = self.stack.len();
        self.stack.retain(|(id, ..)| *id != panel);
        self.stack.len() != before
    }

    /// Whether `panel` is open.
    pub fn is_open(&self, panel: P) -> bool {
        self.stack.iter().any(|(id, ..)| *id == panel)
    }

    /// The options of `panel`, if it is open.
    pub fn options(&self, panel: P) -> Option<&Options> {
        self.stack
            .iter()
            .find(|(id, ..)| *id == panel)
            .map(|(_, options, _)| options)
    }

    /// The open panels, from the lowest to the topmost, as they draw.
    pub fn order(&self) -> impl DoubleEndedIterator<Item = P> + '_ {
        self.stack.iter().map(|(id, ..)| *id)
    }

    /// Places `panel`, of `size` in pixels, within a window of `window`, as
    /// its anchor says: its frame, if it is open.
    pub fn place(&mut self, panel: P, size: (f32, f32), window: (f32, f32)) -> Option<Frame> {
        let (_, options, placed) = self.stack.iter_mut().find(|(id, ..)| *id == panel)?;
        let (x, y) = place(&options.anchor, size, window);
        let frame = Frame {
            x,
            y,
            width: size.0.min(window.0).max(0.0),
            height: size.1.min(window.1).max(0.0),
        };
        *placed = Some(frame);
        Some(frame)
    }

    /// Where `panel` was placed last, if it is open and placed.
    pub fn frame(&self, panel: P) -> Option<Frame> {
        self.stack
            .iter()
            .find(|(id, ..)| *id == panel)
            .and_then(|(.., placed)| *placed)
    }

    /// The topmost panel under `at`, in pixels of the window, but the
    /// tooltips, which the pointer goes through.
    pub fn under(&self, at: (f32, f32)) -> Option<P> {
        self.stack
            .iter()
            .rev()
            .filter(|(_, options, _)| options.role != Role::Tooltip)
            .find(|(.., placed)| placed.is_some_and(|frame| holds(frame, at)))
            .map(|(id, ..)| *id)
    }

    /// The primary button went down at `at`, under no panel: the panels
    /// that a click outside them closes close, and a menu's closing takes
    /// the press.
    pub fn pressed_outside(&mut self, at: (f32, f32)) -> Outside<P> {
        let mut outside = Outside {
            closed: Vec::new(),
            taken: false,
        };
        if self.under(at).is_some() {
            return outside;
        }
        for (id, options, _) in self.stack.iter().rev() {
            if options.dismiss.outside {
                outside.closed.push(*id);
                outside.taken |= options.role == Role::Menu;
            }
        }
        self.stack
            .retain(|(_, options, _)| !options.dismiss.outside);
        outside
    }

    /// Escape went to `panel`, which has the keyboard's focus: whether it
    /// closed, as its plugin asks.
    pub fn escaped(&mut self, panel: P) -> bool {
        let closes = self
            .options(panel)
            .is_some_and(|options| options.dismiss.escape);
        closes && self.close(panel)
    }

    /// Escape went to no panel, the focus in none: the topmost menu closes,
    /// if it asks so, as a menu takes Escape wherever the focus is.
    pub fn escaped_menu(&mut self) -> Option<P> {
        let menu = self
            .stack
            .iter()
            .rev()
            .find(|(_, options, _)| options.role == Role::Menu)
            .map(|(id, ..)| *id)?;
        self.escaped(menu).then_some(menu)
    }

    /// Whether a panel above `below`, or any panel without it, covers any of
    /// `rect`, in pixels of the window, as a sweep under it would show over
    /// it.
    pub fn covers(&self, rect: Rect, below: Option<P>) -> bool {
        let above = match below {
            Some(panel) => match self.stack.iter().position(|(id, ..)| *id == panel) {
                Some(at) => &self.stack[at + 1..],
                None => &[],
            },
            None => &self.stack[..],
        };
        above
            .iter()
            .filter_map(|(.., placed)| *placed)
            .any(|frame| overlaps(frame, rect))
    }
}

/// An entry of a native menu, of a menu's panel.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    /// An item, of a button or a toggle: its node's key, its title, whether
    /// it is ticked, as a toggle that is on, and the event of its choice, as
    /// its node's press, none of a disabled one, which is dimmed.
    Item {
        /// Its node's key.
        key: NodeKey,
        /// Its title, its node's label.
        title: String,
        /// Whether it is ticked.
        ticked: bool,
        /// The event of its choice, `None` of a disabled one.
        event: Option<UiEventKind>,
    },
    /// A line between groups of items.
    Separator,
    /// A menu within the menu, of its title and its entries.
    Submenu {
        /// Its title.
        title: String,
        /// Its entries.
        entries: Vec<MenuEntry>,
    },
}

/// The entries of the native menu of a menu's panel of `tree`, if its
/// root's children are menu items alone: buttons and toggles of labels and
/// keys, separators, and submenus, containers of a text, their title, then
/// their items; `None` of any other node, as the panel then draws in the
/// window.
pub fn native_menu(tree: &Tree) -> Option<Vec<MenuEntry>> {
    let root = tree.root()?;
    let entries = entries(tree, tree.children(root))?;
    (!entries.is_empty()).then_some(entries)
}

/// The entries of the nodes `ids` of `tree`, if each is a menu's item.
fn entries(tree: &Tree, ids: &[NodeId]) -> Option<Vec<MenuEntry>> {
    ids.iter().map(|&id| entry(tree, id)).collect()
}

/// The entry of `id` of `tree`, if it is a menu's item.
fn entry(tree: &Tree, id: NodeId) -> Option<MenuEntry> {
    let item = |title: &str, ticked: bool, event: Option<UiEventKind>| {
        let key = tree.key(id)?.clone();
        (!title.is_empty()).then(|| MenuEntry::Item {
            key,
            title: title.to_owned(),
            ticked,
            event,
        })
    };
    match tree.kind(id)? {
        Kind::Button(button) => item(
            &button.label,
            false,
            (!button.disabled).then_some(UiEventKind::Click),
        ),
        Kind::Toggle(toggle) => item(
            &toggle.label,
            toggle.on,
            (!toggle.disabled).then_some(UiEventKind::Toggle { on: !toggle.on }),
        ),
        Kind::Separator => Some(MenuEntry::Separator),
        Kind::Container => {
            let (&first, items) = tree.children(id).split_first()?;
            let Some(Kind::Text(title)) = tree.kind(first) else {
                return None;
            };
            Some(MenuEntry::Submenu {
                title: title.clone(),
                entries: entries(tree, items)?,
            })
        }
        _ => None,
    }
}

/// Whether `frame` holds `at`.
fn holds(frame: Frame, (x, y): (f32, f32)) -> bool {
    x >= frame.x && x < frame.x + frame.width && y >= frame.y && y < frame.y + frame.height
}

/// Whether `frame` and `rect` overlap.
fn overlaps(frame: Frame, rect: Rect) -> bool {
    let (x, y) = (rect.x as f32, rect.y as f32);
    let (right, bottom) = (x + rect.width as f32, y + rect.height as f32);
    x < frame.x + frame.width && frame.x < right && y < frame.y + frame.height && frame.y < bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f32, y: f32, width: f32, height: f32) -> Frame {
        Frame {
            x,
            y,
            width,
            height,
        }
    }

    const WINDOW: (f32, f32) = (800.0, 600.0);
    const SIZE: (f32, f32) = (200.0, 100.0);

    #[test]
    fn a_panel_goes_below_its_rectangle_or_above_it_without_room() {
        let cell = frame(100.0, 40.0, 8.0, 16.0);
        assert_eq!(place(&Anchor::Below(cell), SIZE, WINDOW), (100.0, 56.0));
        // Near the bottom, above; near the right, kept within.
        let low = frame(700.0, 540.0, 8.0, 16.0);
        assert_eq!(place(&Anchor::Below(low), SIZE, WINDOW), (600.0, 440.0));
        // Of room neither below nor above, below, and kept within.
        let tall = (200.0, 590.0);
        assert_eq!(place(&Anchor::Below(cell), tall, WINDOW), (100.0, 10.0));
    }

    #[test]
    fn a_panel_at_a_point_turns_leftwards_and_upwards_without_room() {
        assert_eq!(place(&Anchor::At((50.0, 60.0)), SIZE, WINDOW), (50.0, 60.0));
        assert_eq!(
            place(&Anchor::At((750.0, 580.0)), SIZE, WINDOW),
            (550.0, 480.0)
        );
        // Of a point outside the window, within it.
        assert_eq!(place(&Anchor::At((-30.0, -5.0)), SIZE, WINDOW), (0.0, 0.0));
    }

    #[test]
    fn a_panel_in_a_corner_keeps_its_margin_and_the_window_as_it_shrinks() {
        let pane = frame(100.0, 50.0, 600.0, 500.0);
        let at = |corner| place(&Anchor::Corner(pane, corner, 10.0), SIZE, WINDOW);
        assert_eq!(at(Corner::TopLeft), (110.0, 60.0));
        assert_eq!(at(Corner::TopRight), (490.0, 60.0));
        assert_eq!(at(Corner::BottomLeft), (110.0, 440.0));
        assert_eq!(at(Corner::BottomRight), (490.0, 440.0));
        // A window that shrank past the pane: within the window still, of
        // its top left edges first where it is smaller than the panel.
        let small = (300.0, 150.0);
        let shrunk = place(
            &Anchor::Corner(pane, Corner::BottomRight, 10.0),
            SIZE,
            small,
        );
        assert_eq!(shrunk, (100.0, 50.0));
        let tiny = (150.0, 80.0);
        assert_eq!(
            place(&Anchor::Corner(pane, Corner::TopRight, 10.0), SIZE, tiny),
            (0.0, 0.0)
        );
    }

    fn options(role: Role, outside: bool, escape: bool) -> Options {
        Options {
            anchor: Anchor::At((0.0, 0.0)),
            role,
            dismiss: Dismiss { outside, escape },
        }
    }

    #[test]
    fn panels_stack_the_last_opened_on_top_and_the_pointer_finds_the_topmost() {
        let mut panels = Panels::new();
        panels.open(1, options(Role::Dialog, false, true));
        panels.open(2, options(Role::Menu, true, true));
        panels.open(3, options(Role::Tooltip, false, false));
        let place = |panels: &mut Panels<u8>, id, anchor| {
            assert!(panels.anchor(id, anchor));
            panels.place(id, SIZE, WINDOW)
        };
        place(&mut panels, 1, Anchor::At((0.0, 0.0)));
        place(&mut panels, 2, Anchor::At((100.0, 50.0)));
        place(&mut panels, 3, Anchor::At((150.0, 75.0)));
        assert_eq!(panels.order().collect::<Vec<_>>(), [1, 2, 3]);
        // Where all three overlap: the menu, as the tooltip lets the pointer
        // through.
        assert_eq!(panels.under((160.0, 80.0)), Some(2));
        assert_eq!(panels.under((50.0, 20.0)), Some(1));
        assert_eq!(panels.under((700.0, 500.0)), None);
        // Opened again, it goes on top.
        panels.open(1, options(Role::Dialog, false, true));
        assert_eq!(panels.order().collect::<Vec<_>>(), [2, 3, 1]);
        assert_eq!(panels.frame(1), None);
        place(&mut panels, 1, Anchor::At((0.0, 0.0)));
        assert_eq!(panels.under((160.0, 80.0)), Some(1));
        // Wider and taller than the window, of its size.
        let frame = panels.place(1, (900.0, 700.0), WINDOW).unwrap();
        assert_eq!((frame.width, frame.height), WINDOW);
    }

    #[test]
    fn a_click_outside_and_escape_close_the_panels_that_ask_so() {
        let mut panels = Panels::new();
        panels.open(1, options(Role::Dialog, false, true));
        panels.open(2, options(Role::Menu, true, true));
        panels.place(1, SIZE, WINDOW);
        panels.place(2, SIZE, WINDOW);
        // Inside one, nothing closes.
        assert_eq!(
            panels.pressed_outside((10.0, 10.0)),
            Outside {
                closed: vec![],
                taken: false
            }
        );
        // Outside: the menu, which takes the press; the dialog stays.
        assert_eq!(
            panels.pressed_outside((500.0, 500.0)),
            Outside {
                closed: vec![2],
                taken: true
            }
        );
        assert!(!panels.is_open(2) && panels.is_open(1));
        // A dialog that a click outside closes lets the press go on.
        panels.open(3, options(Role::Dialog, true, false));
        assert_eq!(
            panels.pressed_outside((500.0, 500.0)),
            Outside {
                closed: vec![3],
                taken: false
            }
        );
        // Escape over no panel closes the topmost menu alone, of those that
        // ask so.
        panels.open(5, options(Role::Menu, false, true));
        panels.open(6, options(Role::Menu, false, true));
        assert_eq!(panels.escaped_menu(), Some(6));
        assert_eq!(panels.escaped_menu(), Some(5));
        panels.open(7, options(Role::Menu, false, false));
        assert_eq!(panels.escaped_menu(), None);
        assert!(panels.close(7));
        assert_eq!(panels.escaped_menu(), None);
        assert!(Role::Menu.takes_focus() && Role::Dialog.takes_focus());
        assert!(!Role::Tooltip.takes_focus());
        // Escape closes the dialog that asks so, not one that does not.
        panels.open(4, options(Role::Dialog, false, false));
        assert!(!panels.escaped(4));
        assert!(panels.escaped(1));
        assert!(!panels.escaped(1));
        assert!(panels.close(4));
        assert!(!panels.close(4));
        assert!(!panels.anchor(4, Anchor::At((0.0, 0.0))));
    }

    #[test]
    fn a_menu_s_panel_of_menu_items_alone_becomes_a_native_menu() {
        use lokyr_plugin_sdk::view::{self, Node};
        let tree = |root: Node| {
            let mut tree = Tree::new();
            tree.update(&root).unwrap();
            tree
        };
        let disabled = Node::new(Kind::Button(view::Button {
            label: "Paste".to_owned(),
            disabled: true,
            ..view::Button::default()
        }))
        .key("paste");
        let hidden = Node::new(Kind::Toggle(view::Toggle {
            label: "Hidden Files".to_owned(),
            on: true,
            ..view::Toggle::default()
        }))
        .key("hidden");
        let menu = Node::container(vec![
            Node::button("Copy").key("copy"),
            disabled,
            Node::separator(),
            hidden,
            Node::container(vec![
                Node::text("Open Recent"),
                Node::button("lokyr").key("recent.lokyr"),
            ]),
        ]);
        let item = |key: &str, title: &str, ticked, event| MenuEntry::Item {
            key: NodeKey::from(key),
            title: title.to_owned(),
            ticked,
            event,
        };
        assert_eq!(
            native_menu(&tree(menu)),
            Some(vec![
                item("copy", "Copy", false, Some(UiEventKind::Click)),
                item("paste", "Paste", false, None),
                MenuEntry::Separator,
                item(
                    "hidden",
                    "Hidden Files",
                    true,
                    Some(UiEventKind::Toggle { on: false })
                ),
                MenuEntry::Submenu {
                    title: "Open Recent".to_owned(),
                    entries: vec![item(
                        "recent.lokyr",
                        "lokyr",
                        false,
                        Some(UiEventKind::Click)
                    )],
                },
            ])
        );
        // A node of no menu's item, a button of an icon alone, a container
        // of no title, a root of no children or that is no container: drawn.
        for root in [
            Node::container(vec![
                Node::button("Copy").key("copy"),
                Node::text("3 of 12"),
            ]),
            Node::container(vec![Node::icon_button("add").key("add")]),
            Node::container(vec![Node::container(vec![
                Node::button("Copy").key("copy"),
            ])]),
            Node::container(vec![]),
            Node::button("Copy").key("copy"),
        ] {
            assert_eq!(native_menu(&tree(root)), None);
        }
    }

    #[test]
    fn a_panel_covers_what_is_under_it() {
        let mut panels = Panels::new();
        panels.open(1, options(Role::Dialog, false, false));
        panels.open(2, options(Role::Dialog, false, false));
        panels.place(1, SIZE, WINDOW);
        panels.place(2, (50.0, 50.0), WINDOW);
        let rect = |x, y| Rect {
            x,
            y,
            width: 20,
            height: 4,
        };
        // Of the window's content, under both.
        assert!(panels.covers(rect(150, 10), None));
        assert!(!panels.covers(rect(300, 10), None));
        // Of the first panel's, under the second alone.
        assert!(panels.covers(rect(10, 10), Some(1)));
        assert!(!panels.covers(rect(150, 10), Some(1)));
        assert!(!panels.covers(rect(10, 10), Some(2)));
    }
}

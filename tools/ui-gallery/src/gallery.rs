//! The gallery's content, as a plugin's view would give it: `ui-core`'s
//! nodes in rows and columns, of the theme's tokens, and the nodes that
//! the pointer and the keyboard try, whose events the gallery shows and
//! draws again, as a plugin learns of them and renders anew.

use std::collections::BTreeSet;

use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, Justify, Layout, Length, Node, PointerShape, Shadow, Style,
    Token, UiEventKind,
};
use lokyr_ui_core::interface;

/// The icons a row of the icons' section.
const ICONS_A_ROW: usize = 4;

/// A family, an emoji of five code points.
const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

/// Italy's flag, of two regional indicators.
const ITALY: &str = "\u{1F1EE}\u{1F1F9}";

/// What the gallery shows: its appearance, and what the pointer and the
/// keyboard did.
#[derive(Debug)]
pub struct Gallery {
    dark: bool,
    hovered: BTreeSet<String>,
    last: String,
    focused: Option<String>,
    /// The last focus and blur, as their events told them.
    focus_events: String,
    /// What the drag did last.
    drag: String,
    /// What the scrolls did: the last, and their sum.
    scrolled: (String, f32, f32),
    /// The fields' last change of text, and their last submit.
    typed: (String, String),
    /// The texts that the gallery sets in its fields, as a plugin asks the
    /// host, by their keys, since the host took them last.
    texts: Vec<(String, String)>,
}

impl Gallery {
    /// The gallery, light or dark, before the pointer and the keyboard did
    /// anything.
    pub fn new(dark: bool) -> Self {
        Self {
            dark,
            hovered: BTreeSet::new(),
            last: "nothing yet".to_owned(),
            focused: None,
            focus_events: "none yet".to_owned(),
            drag: "drag the chip, or a tab".to_owned(),
            scrolled: ("scroll over the box".to_owned(), 0.0, 0.0),
            typed: ("none yet".to_owned(), "none yet".to_owned()),
            texts: vec![
                (
                    "name".to_owned(),
                    format!("Lokyr 中文 {FAMILY} {ITALY} cafe\u{301}"),
                ),
                (
                    "narrow".to_owned(),
                    "a text longer than its field, which scrolls".to_owned(),
                ),
            ],
        }
    }

    /// Whether it is dark.
    pub fn dark(&self) -> bool {
        self.dark
    }

    /// Switches it between light and dark.
    pub fn toggle_dark(&mut self) {
        self.dark = !self.dark;
    }

    /// What the pointer did to the node of `key`.
    pub fn event(&mut self, key: &str, kind: UiEventKind) {
        match kind {
            UiEventKind::HoverEnter => {
                self.hovered.insert(key.to_owned());
            }
            UiEventKind::HoverLeave => {
                self.hovered.remove(key);
            }
            UiEventKind::Click if key == "clear" => {
                self.last = "click on `clear`".to_owned();
                self.texts.push(("find".to_owned(), String::new()));
            }
            UiEventKind::Click => self.last = format!("click on `{key}`"),
            UiEventKind::DoubleClick => self.last = format!("double click on `{key}`"),
            UiEventKind::MiddleClick => self.last = format!("middle click on `{key}`"),
            UiEventKind::ContextMenu => self.last = format!("context menu on `{key}`"),
            UiEventKind::DragStart => self.drag = format!("dragging `{key}`"),
            UiEventKind::DragMove { x, y } => {
                self.drag = format!("dragging `{key}` by {x:.0}, {y:.0}");
            }
            UiEventKind::DragEnd { x, y } => {
                self.drag = format!("dropped `{key}` at {x:.0}, {y:.0}");
            }
            UiEventKind::DragCancel => self.drag = format!("drag of `{key}` cancelled"),
            UiEventKind::Scroll { x, y } => {
                let (_, across, down) = self.scrolled;
                self.scrolled = (format!("`{key}` by {x:.0}, {y:.0}"), across + x, down + y);
            }
            // A move's blur comes before its focus.
            UiEventKind::Focus if self.focus_events.starts_with("blur") => {
                self.focus_events = format!("{}, then focus of `{key}`", self.focus_events);
            }
            UiEventKind::Focus => self.focus_events = format!("focus of `{key}`"),
            UiEventKind::Blur => self.focus_events = format!("blur of `{key}`"),
            UiEventKind::SetValue { value } => self.last = format!("`{key}` set to {value}"),
            UiEventKind::InputChanged { text } => self.typed.0 = format!("`{key}`: {text}"),
            UiEventKind::Submit { text } => self.typed.1 = format!("`{key}`: {text}"),
        }
    }

    /// The texts that the gallery sets in its fields, by their keys, since
    /// it was last asked.
    pub fn take_texts(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.texts)
    }

    /// The node of `key` has the keyboard's focus, or none has it.
    pub fn focused(&mut self, key: Option<&str>) {
        self.focused = key.map(str::to_owned);
    }

    /// The key `name`, Space or Return, went to the node of `key`, which has
    /// the focus.
    pub fn pressed(&mut self, key: &str, name: &str) {
        self.last = format!("{name} on `{key}`");
    }

    /// The gallery's view.
    pub fn view(&self) -> Node {
        let left = column(
            16.0,
            vec![
                section("Rows: justified", rows()),
                section("Rows: aligned across", aligned()),
                section("Columns", columns()),
                section("Boxes and clips", boxes()),
                section("Drags and scrolls", self.drags()),
            ],
        );
        let right = column(
            16.0,
            vec![
                section("Icons", icons()),
                section("The pointer", self.pointer()),
                section("The keyboard's focus", self.focus()),
                section("Fields", self.fields()),
            ],
        );
        let halves = Node::container(vec![half(left), half(right)]).layout(Layout {
            gap: 16.0,
            align: Align::Start,
            ..Layout::default()
        });
        column(16.0, vec![self.header(), halves]).layout(Layout {
            direction: Direction::Column,
            gap: 16.0,
            padding: Edges::all(20.0),
            ..Layout::default()
        })
    }

    fn header(&self) -> Node {
        let appearance = if self.dark { "Dark" } else { "Light" };
        Node::container(vec![
            Node::icon("terminal"),
            Node::text("Lokyr's UI gallery"),
            Node::spacer(),
            secondary("⌘D light or dark · Tab, ⇧Tab and Esc move the focus · ⌘Q quits"),
            badge(appearance),
        ])
        .layout(Layout {
            gap: 8.0,
            align: Align::Center,
            ..Layout::default()
        })
    }

    fn pointer(&self) -> Node {
        let hovered = |key: &str| self.hovered.contains(key);
        let chips = Node::container(vec![
            chip("arrow", "Arrow", hovered("arrow"), PointerShape::Arrow),
            chip("i-beam", "I-beam", hovered("i-beam"), PointerShape::Text),
            chip("hand", "Hand", hovered("hand"), PointerShape::PointingHand),
        ])
        .layout(Layout {
            gap: 8.0,
            ..Layout::default()
        });
        let close = Node::icon("close").key("tab.close").style(Style {
            background: hovered("tab.close").then(|| Token::from("button.hover.bg")),
            radius: 4.0,
            pointer: Some(PointerShape::PointingHand),
            ..Style::default()
        });
        let tab = Node::container(vec![
            Node::icon("terminal"),
            Node::text("A tab, and its close"),
            Node::spacer(),
            close,
        ])
        .key("tab")
        .layout(Layout {
            gap: 6.0,
            padding: Edges::all(6.0),
            align: Align::Center,
            width: Length::Points(240.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(if hovered("tab") {
                "tab.hover.bg"
            } else {
                "tab.inactive.bg"
            })),
            border: Some(Border {
                color: Token::from("separator"),
                width: 1.0,
            }),
            radius: 6.0,
            color: Some(Token::from(if hovered("tab") {
                "tab.active.fg"
            } else {
                "tab.inactive.fg"
            })),
            ..Style::default()
        });
        let over = if self.hovered.is_empty() {
            "over: nothing".to_owned()
        } else {
            let keys: Vec<&str> = self.hovered.iter().map(String::as_str).collect();
            format!("over: {}", keys.join(", "))
        };
        column(
            8.0,
            vec![
                chips,
                tab,
                secondary(&over),
                secondary(&format!("last: {}", self.last)),
            ],
        )
    }

    fn drags(&self) -> Node {
        let draggable = chip(
            "drag",
            "Drag me",
            self.hovered.contains("drag"),
            PointerShape::PointingHand,
        );
        let scroll = Node::container(vec![Node::text("Scroll over me")])
            .key("scroll")
            .layout(Layout {
                padding: Edges::all(10.0),
                justify: Justify::Center,
                width: Length::Points(160.0),
                ..Layout::default()
            })
            .style(field());
        let (last, across, down) = &self.scrolled;
        column(
            8.0,
            vec![
                Node::container(vec![draggable, scroll]).layout(Layout {
                    gap: 12.0,
                    align: Align::Center,
                    ..Layout::default()
                }),
                secondary(&format!("drag: {}", self.drag)),
                secondary(&format!("scroll: {last}; in all {across:.0}, {down:.0}")),
            ],
        )
    }

    fn focus(&self) -> Node {
        let focusable = |key: &str, label: &str| {
            chip(key, label, false, PointerShape::PointingHand).focusable()
        };
        let group = Node::container(vec![
            Node::text("A group"),
            Node::icon("close")
                .key("group.close")
                .focusable()
                .style(Style {
                    radius: 4.0,
                    ..Style::default()
                }),
        ])
        .key("group")
        .focusable()
        .layout(Layout {
            gap: 6.0,
            padding: Edges::all(4.0),
            align: Align::Center,
            ..Layout::default()
        })
        .style(Style {
            border: Some(Border {
                color: Token::from("field.border"),
                width: 1.0,
            }),
            radius: 6.0,
            ..Style::default()
        });
        let nodes = Node::container(vec![
            focusable("first", "First"),
            focusable("second", "Second"),
            Node::text("a label").style(Style {
                color: Some(Token::from("text.secondary")),
                ..Style::default()
            }),
            focusable("third", "Third"),
            group,
        ])
        .layout(Layout {
            gap: 8.0,
            align: Align::Center,
            ..Layout::default()
        });
        let focus = match &self.focused {
            Some(key) => format!("focus: `{key}`; Space or Return press it"),
            None => "focus: none; Tab takes the first, a click the one under it".to_owned(),
        };
        column(
            8.0,
            vec![
                nodes,
                secondary(&focus),
                secondary(&format!("events: {}", self.focus_events)),
            ],
        )
    }

    fn fields(&self) -> Node {
        let find = Node::field("Find").key("find").layout(Layout {
            grow: 1.0,
            ..Layout::default()
        });
        let clear = chip(
            "clear",
            "Clear",
            self.hovered.contains("clear"),
            PointerShape::PointingHand,
        );
        let name = Node::field("Name").key("name").layout(Layout {
            width: Length::Fraction(1.0),
            ..Layout::default()
        });
        let narrow = Node::field("A narrow field").key("narrow").layout(Layout {
            width: Length::Points(120.0),
            ..Layout::default()
        });
        let (typed, submitted) = &self.typed;
        column(
            8.0,
            vec![
                Node::container(vec![find, clear]).layout(Layout {
                    gap: 8.0,
                    align: Align::Center,
                    ..Layout::default()
                }),
                name,
                narrow,
                secondary(&format!("input: {typed}")),
                secondary(&format!("submit: {submitted}")),
            ],
        )
    }
}

/// The rows of a folder's path, justified at the start, the centre, the
/// end and with the room between.
fn rows() -> Node {
    let justified = [
        ("start", Justify::Start),
        ("center", Justify::Center),
        ("end", Justify::End),
        ("space between", Justify::SpaceBetween),
    ];
    column(
        6.0,
        justified
            .into_iter()
            .map(|(name, justify)| {
                let path = Node::container(vec![
                    Node::icon("folder"),
                    Node::text("~/Projects/lokyr"),
                    Node::icon("chevron-right"),
                ])
                .layout(Layout {
                    gap: 6.0,
                    padding: Edges::all(4.0),
                    align: Align::Center,
                    justify,
                    grow: 1.0,
                    ..Layout::default()
                })
                .style(field());
                Node::container(vec![label(name), path]).layout(Layout {
                    gap: 8.0,
                    align: Align::Center,
                    ..Layout::default()
                })
            })
            .collect(),
    )
}

/// Boxes 44 points high, whose badge sits at their top, their middle, their
/// bottom, or across them.
fn aligned() -> Node {
    let aligns = [
        ("start", Align::Start),
        ("center", Align::Center),
        ("end", Align::End),
        ("stretch", Align::Stretch),
    ];
    Node::container(
        aligns
            .into_iter()
            .map(|(name, align)| {
                Node::container(vec![badge(name)])
                    .layout(Layout {
                        padding: Edges::all(4.0),
                        align,
                        justify: Justify::Center,
                        grow: 1.0,
                        width: Length::Points(0.0),
                        height: Length::Points(44.0),
                        ..Layout::default()
                    })
                    .style(field())
            })
            .collect(),
    )
    .layout(Layout {
        gap: 8.0,
        ..Layout::default()
    })
}

/// A list of texts, a list of icons and their names, and bars of fractions
/// of their column's width.
fn columns() -> Node {
    let texts = column(
        4.0,
        ["One", "Two", "Three", "Four"]
            .into_iter()
            .map(Node::text)
            .collect(),
    );
    let named = column(
        4.0,
        [
            ("folder", "Folder"),
            ("terminal", "Terminal"),
            ("settings", "Settings"),
            ("bell", "Bell"),
        ]
        .into_iter()
        .map(|(icon, name)| {
            Node::container(vec![Node::icon(icon), Node::text(name)]).layout(Layout {
                gap: 6.0,
                align: Align::Center,
                ..Layout::default()
            })
        })
        .collect(),
    );
    let bars = column(
        6.0,
        [1.0, 0.75, 0.5, 0.25]
            .into_iter()
            .map(|fraction| {
                Node::container(vec![])
                    .layout(Layout {
                        width: Length::Fraction(fraction),
                        height: Length::Points(10.0),
                        ..Layout::default()
                    })
                    .style(Style {
                        background: Some(Token::from("accent")),
                        radius: 5.0,
                        ..Style::default()
                    })
            })
            .collect(),
    )
    .layout(Layout {
        direction: Direction::Column,
        gap: 6.0,
        grow: 1.0,
        ..Layout::default()
    });
    Node::container(vec![texts, named, bars]).layout(Layout {
        gap: 24.0,
        ..Layout::default()
    })
}

/// Boxes of a border, of rounded corners and a shadow, a badge, a title
/// that its box clips, and a box that clips a larger one.
fn boxes() -> Node {
    let sized = |width: f32, height: f32| Layout {
        width: Length::Points(width),
        height: Length::Points(height),
        ..Layout::default()
    };
    let bordered = Node::container(vec![])
        .layout(sized(56.0, 36.0))
        .style(Style {
            background: Some(Token::from("button.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            ..Style::default()
        });
    let raised = Node::container(vec![])
        .layout(sized(56.0, 36.0))
        .style(Style {
            background: Some(Token::from("field.bg")),
            radius: 8.0,
            shadow: Some(Shadow::default()),
            ..Style::default()
        });
    let clipped = Node::container(vec![Node::text("A title much too long for its box")])
        .layout(Layout {
            padding: Edges::all(4.0),
            width: Length::Points(110.0),
            ..Layout::default()
        })
        .style(Style {
            clip: true,
            ..field()
        });
    let inner = Node::container(vec![])
        .layout(sized(90.0, 60.0))
        .style(Style {
            background: Some(Token::from("accent")),
            radius: 12.0,
            ..Style::default()
        });
    let clipping = Node::container(vec![inner])
        .layout(sized(56.0, 36.0))
        .style(Style {
            clip: true,
            ..field()
        });
    Node::container(vec![bordered, raised, badge("A badge"), clipped, clipping]).layout(Layout {
        gap: 12.0,
        align: Align::Center,
        ..Layout::default()
    })
}

/// Each of the host's icons, with its name.
fn icons() -> Node {
    let names: Vec<&str> = interface::icons().collect();
    column(
        6.0,
        names
            .chunks(ICONS_A_ROW)
            .map(|row| {
                Node::container(
                    row.iter()
                        .map(|&name| {
                            Node::container(vec![Node::icon(name), secondary(name)]).layout(
                                Layout {
                                    gap: 6.0,
                                    align: Align::Center,
                                    width: Length::Fraction(1.0 / ICONS_A_ROW as f32),
                                    ..Layout::default()
                                },
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

/// A column of `children`, `gap` points apart.
fn column(gap: f32, children: Vec<Node>) -> Node {
    Node::container(children).layout(Layout {
        direction: Direction::Column,
        gap,
        ..Layout::default()
    })
}

/// Half of the gallery's width.
fn half(content: Node) -> Node {
    let layout = Layout {
        width: Length::Points(0.0),
        grow: 1.0,
        ..content.layout
    };
    content.layout(layout)
}

/// A section: its title, over a panel of `body`.
fn section(title: &str, body: Node) -> Node {
    let panel = Node::container(vec![body])
        .layout(Layout {
            direction: Direction::Column,
            padding: Edges::all(12.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 8.0,
            ..Style::default()
        });
    column(6.0, vec![secondary(title), panel])
}

/// A text in the secondary text's color.
fn secondary(text: &str) -> Node {
    Node::text(text).style(Style {
        color: Some(Token::from("text.secondary")),
        ..Style::default()
    })
}

/// A label of a row, of a width of its own.
fn label(text: &str) -> Node {
    secondary(text).layout(Layout {
        width: Length::Points(96.0),
        ..Layout::default()
    })
}

/// A field's box.
fn field() -> Style {
    Style {
        background: Some(Token::from("field.bg")),
        border: Some(Border {
            color: Token::from("field.border"),
            width: 1.0,
        }),
        radius: 4.0,
        ..Style::default()
    }
}

/// A badge of the accent's color.
fn badge(text: &str) -> Node {
    Node::container(vec![Node::text(text)])
        .layout(Layout {
            padding: Edges {
                top: 2.0,
                right: 8.0,
                bottom: 2.0,
                left: 8.0,
            },
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("accent")),
            radius: 9.0,
            color: Some(Token::from("panel.bg")),
            ..Style::default()
        })
}

/// A chip of `key`, its label, lighter while the pointer is over it, of the
/// pointer's `shape`.
fn chip(key: &str, label: &str, hovered: bool, shape: PointerShape) -> Node {
    Node::container(vec![Node::text(label)])
        .key(key)
        .layout(Layout {
            padding: Edges {
                top: 4.0,
                right: 10.0,
                bottom: 4.0,
                left: 10.0,
            },
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(if hovered {
                "button.hover.bg"
            } else {
                "button.bg"
            })),
            radius: 6.0,
            pointer: Some(shape),
            ..Style::default()
        })
}

#[cfg(test)]
mod tests {
    use lokyr_ui_core::focus::{Focus, Target};
    use lokyr_ui_core::tree::{NodeId, Tree};
    use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, Mods};

    use super::*;

    fn find(tree: &Tree, key: &str) -> NodeId {
        fn walk(tree: &Tree, id: NodeId, key: &str) -> Option<NodeId> {
            if tree.key(id).is_some_and(|found| found.0 == key) {
                return Some(id);
            }
            tree.children(id)
                .iter()
                .find_map(|&child| walk(tree, child, key))
        }
        walk(tree, tree.root().unwrap(), key).unwrap()
    }

    #[test]
    fn the_view_is_a_tree_that_the_host_takes_in_each_state() {
        let mut gallery = Gallery::new(false);
        let mut tree = Tree::new();
        tree.update(&gallery.view()).unwrap();
        gallery.event("tab", UiEventKind::HoverEnter);
        gallery.event("tab.close", UiEventKind::HoverEnter);
        gallery.event("tab.close", UiEventKind::Click);
        gallery.event("drag", UiEventKind::DragMove { x: 12.0, y: 3.0 });
        gallery.event("scroll", UiEventKind::Scroll { x: 0.0, y: -16.0 });
        gallery.event("scroll", UiEventKind::Scroll { x: 0.0, y: -16.0 });
        gallery.event("first", UiEventKind::Blur);
        gallery.event("second", UiEventKind::Focus);
        gallery.focused(Some("second"));
        gallery.toggle_dark();
        tree.update(&gallery.view()).unwrap();
        assert_eq!(gallery.drag, "dragging `drag` by 12, 3");
        assert_eq!(gallery.scrolled.2, -32.0);
        assert_eq!(
            gallery.focus_events,
            "blur of `first`, then focus of `second`"
        );
        assert!(gallery.dark());
        assert_eq!(gallery.last, "click on `tab.close`");
    }

    #[test]
    fn tab_goes_through_the_focus_s_nodes_in_their_order() {
        let gallery = Gallery::new(false);
        let mut tree = Tree::new();
        tree.update(&gallery.view()).unwrap();
        let mut focus = Focus::new();
        assert!(focus.first(0, &tree));
        let tab = KeyEvent {
            key: Key::Tab,
            action: KeyAction::Press,
            mods: Mods::default(),
            text: "",
            unmodified: "",
            base_layout: None,
        };
        let mut order = Vec::new();
        for _ in 0..9 {
            let Target::Node { node, .. } = focus.target() else {
                panic!("the focus left the panel");
            };
            order.push(tree.key(node).unwrap().0.clone());
            focus.key(|_| Some(&tree), &tab);
        }
        assert_eq!(
            order,
            [
                "first",
                "second",
                "third",
                "group",
                "group.close",
                "find",
                "name",
                "narrow",
                "first"
            ]
        );
    }

    #[test]
    fn the_hover_restyles_its_node_without_laying_it_out_again() {
        let mut gallery = Gallery::new(false);
        let mut tree = Tree::new();
        tree.update(&gallery.view()).unwrap();
        gallery.event("hand", UiEventKind::HoverEnter);
        let changes = tree.update(&gallery.view()).unwrap();
        let hand = find(&tree, "hand");
        assert!(changes.restyled.contains(&hand));
        assert!(!changes.changed.contains(&hand));
        gallery.event("hand", UiEventKind::HoverLeave);
        tree.update(&gallery.view()).unwrap();
        assert_eq!(gallery.hovered.len(), 0);
    }
}

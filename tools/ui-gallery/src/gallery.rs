//! The gallery's content, as a plugin's view would give it: `ui-core`'s
//! nodes in rows and columns, of the theme's tokens, and the nodes that
//! the pointer and the keyboard try, whose events the gallery shows and
//! draws again, as a plugin learns of them and renders anew.

use std::collections::{BTreeMap, BTreeSet};

use lokyr_plugin_sdk::view::{
    self, Align, Border, Direction, Easing, Edges, Justify, Kind, Layout, Length, ListItem, Node,
    PointerShape, Progress, ProgressShape, ProgressState, Shadow, Style, Token, Transition,
    UiEventKind,
};
use lokyr_ui_core::chrome::TabBarPlace;
use lokyr_ui_core::interface;

/// The icons a row of the icons' section.
const ICONS_A_ROW: usize = 4;

/// A family, an emoji of five code points.
const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

/// The rows of the gallery's list.
const ROWS: usize = 100_000;

/// The keys of the gallery's buttons.
const BUTTONS: [&str; 5] = ["replace", "replace-all", "settings", "add", "trash"];

/// Italy's flag, of two regional indicators.
const ITALY: &str = "\u{1F1EE}\u{1F1F9}";

/// The most tabs of the tab bar's.
const TABS: usize = 6;

/// A slot of the window's chrome, which the gallery fills as a plugin
/// would.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    /// The tab bar, in the titlebar or below it.
    TabBar,
    /// The strip across the page's top, over it.
    WindowStrip,
    /// The status bar, across the window's bottom.
    StatusBar,
    /// The header of the page, as a pane's.
    PaneHeader,
}

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
    /// The toggles that are on, by their keys, and the buttons' last press.
    toggles: BTreeMap<String, bool>,
    pressed: String,
    /// The list's items that the host asked for last, from the first to
    /// the end, and what its selection did.
    window: (usize, usize),
    listed: String,
    /// How far the progresses are, in percents, and whether the custom bar
    /// is indeterminate.
    percent: u32,
    spinning: bool,
    /// The panels to open or to close, as a plugin asks the host, since
    /// the host took them last, and what they did.
    asks: Vec<PanelAsk>,
    paneled: String,
    /// Whether the note shows, which its button switches, as the pointer
    /// goes through it.
    note: bool,
    /// The order of the tabs that move, whether the box that fades shows,
    /// and whether both take ten times their time.
    order: Vec<usize>,
    boxed: bool,
    slow: bool,
    /// The tab bar's tabs, the one chosen, and whether the bar goes in
    /// the titlebar, or below a titlebar of the window's title.
    tabs: usize,
    chosen: usize,
    in_titlebar: bool,
    /// The tab chosen of the view of tabs.
    page: usize,
}

/// The gallery's menu, at the pointer.
pub const MENU: u8 = 1;

/// The gallery's find bar, in the window's top right corner.
pub const FIND: u8 = 2;

/// The gallery's panel below its button.
pub const BELOW: u8 = 3;

/// The gallery's note, a tooltip in the window's bottom left corner.
pub const NOTE: u8 = 4;

/// What the gallery asks of its panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAsk {
    /// To open one, or raise it.
    Open(u8),
    /// To close one.
    Close(u8),
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
            toggles: BTreeMap::from([("regex".to_owned(), true), ("pinned".to_owned(), true)]),
            pressed: "none yet".to_owned(),
            window: (0, 0),
            listed: "none yet".to_owned(),
            percent: 40,
            spinning: false,
            asks: Vec::new(),
            paneled: "none yet".to_owned(),
            note: false,
            order: vec![0, 1, 2, 3],
            boxed: true,
            slow: false,
            tabs: 3,
            chosen: 0,
            in_titlebar: true,
            page: 0,
        }
    }

    /// Where the tab bar goes: in the titlebar, as the compact and minimal
    /// styles have it, or below a titlebar of the window's title, as the
    /// regular one.
    pub fn tab_bar_place(&self) -> TabBarPlace {
        if self.in_titlebar {
            TabBarPlace::Titlebar
        } else {
            TabBarPlace::BelowTitlebar
        }
    }

    /// The view of the chrome's `slot`.
    pub fn slot(&self, slot: Slot) -> Node {
        match slot {
            Slot::TabBar => self.tab_bar(),
            Slot::WindowStrip => self.window_strip(),
            Slot::StatusBar => self.status_bar(),
            Slot::PaneHeader => Node::container(vec![secondary(
                "A pane's header: the page below is the pane's grid",
            )])
            .layout(Layout {
                padding: Edges {
                    top: 4.0,
                    right: 12.0,
                    bottom: 4.0,
                    left: 12.0,
                },
                justify: Justify::Center,
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("panel.bg")),
                ..Style::default()
            }),
        }
    }

    /// The tab bar: its tabs, the chosen one of the active tab's colors,
    /// the others lighter under the pointer, and a button of a new one.
    fn tab_bar(&self) -> Node {
        let mut children: Vec<Node> = (0..self.tabs)
            .map(|tab| {
                let key = format!("bar.tab-{tab}");
                let (background, color) = if tab == self.chosen {
                    ("tab.active.bg", "tab.active.fg")
                } else if self.hovered.contains(&key) {
                    ("tab.hover.bg", "tab.inactive.fg")
                } else {
                    ("tab.inactive.bg", "tab.inactive.fg")
                };
                Node::container(vec![Node::text(format!("Tab {}", tab + 1))])
                    .key(key.as_str())
                    .layout(Layout {
                        padding: Edges {
                            top: 5.0,
                            right: 14.0,
                            bottom: 5.0,
                            left: 14.0,
                        },
                        ..Layout::default()
                    })
                    .style(Style {
                        background: Some(Token::from(background)),
                        color: Some(Token::from(color)),
                        radius: 6.0,
                        ..Style::default()
                    })
            })
            .collect();
        let mut new = Node::icon_button("add").key("bar.new");
        if let Kind::Button(button) = &mut new.kind {
            button.disabled = self.tabs >= TABS;
        }
        children.push(new);
        Node::container(children)
            .layout(Layout {
                gap: 4.0,
                align: Align::Center,
                padding: Edges {
                    top: 4.0,
                    right: 8.0,
                    bottom: 4.0,
                    left: 8.0,
                },
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("tab.bar.bg")),
                ..Style::default()
            })
    }

    /// The window's strip: a hairline of the progress's value, or of no
    /// value as the spinning toggle is on.
    fn window_strip(&self) -> Node {
        let state = if self.spinning {
            ProgressState::Indeterminate
        } else {
            ProgressState::Normal
        };
        Node::progress(ProgressShape::Bar, state, self.percent as f32 / 100.0)
            .layout(Layout {
                width: Length::Fraction(1.0),
                height: Length::Points(2.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("transparent")),
                ..Style::default()
            })
    }

    /// The status bar: the toggle of the tab bar's place, and what each
    /// place is.
    fn status_bar(&self) -> Node {
        let place = if self.in_titlebar {
            "Compact: the tabs in the titlebar, the window's buttons centred on them"
        } else {
            "Regular: the tabs below a titlebar of the window's title"
        };
        Node::container(vec![
            Node::new(Kind::Toggle(view::Toggle {
                label: "Tabs in the titlebar".to_owned(),
                on: self.in_titlebar,
                ..view::Toggle::default()
            }))
            .key("status.titlebar"),
            Node::separator(),
            secondary(place),
            Node::spacer(),
            secondary("The window's strip, over the page's top, shows the progress"),
        ])
        .layout(Layout {
            gap: 10.0,
            align: Align::Center,
            padding: Edges {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 10.0,
            },
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("tab.bar.bg")),
            ..Style::default()
        })
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
            UiEventKind::Click if key.starts_with("open-") || self.panel_key(key) => {
                self.panel_click(key);
            }
            UiEventKind::Click if key == "anim.shuffle" => self.order.rotate_right(1),
            UiEventKind::Toggle { on } if key == "anim.boxed" => self.boxed = on,
            UiEventKind::Toggle { on } if key == "anim.slow" => self.slow = on,
            UiEventKind::Click if key == "bar.new" && self.tabs < TABS => {
                self.tabs += 1;
                self.chosen = self.tabs - 1;
            }
            UiEventKind::Click if key.starts_with("bar.tab-") => {
                if let Ok(tab) = key["bar.tab-".len()..].parse::<usize>() {
                    self.chosen = tab.min(self.tabs.saturating_sub(1));
                }
            }
            UiEventKind::Toggle { on } if key == "status.titlebar" => self.in_titlebar = on,
            UiEventKind::Click if key == "less" => self.percent = self.percent.saturating_sub(10),
            UiEventKind::Click if key == "more" => self.percent = (self.percent + 10).min(100),
            UiEventKind::Toggle { on } if key == "spin" => self.spinning = on,
            // A toggle of the menu, chosen, closes it, as a menu's item.
            UiEventKind::Toggle { on } if key.starts_with("menu.") => {
                self.toggles.insert(key.to_owned(), on);
                self.paneled = format!("`{key}` {}", if on { "on" } else { "off" });
                self.asks.push(PanelAsk::Close(MENU));
            }
            UiEventKind::Click if BUTTONS.contains(&key) => {
                self.pressed = format!("click on `{key}`");
            }
            UiEventKind::Select { item } if key == "views" => self.page = item,
            UiEventKind::ListRange { first, count } => self.window = (first, first + count),
            UiEventKind::Select { item } => self.listed = format!("selected item {item}"),
            UiEventKind::Activate { item } => self.listed = format!("activated item {item}"),
            UiEventKind::Toggle { on } => {
                self.toggles.insert(key.to_owned(), on);
                self.pressed = format!("`{key}` {}", if on { "on" } else { "off" });
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

    /// Whether `key` is of a node of a panel's, or of a button that opens
    /// one.
    fn panel_key(&self, key: &str) -> bool {
        ["menu.", "find-bar.", "below."]
            .iter()
            .any(|prefix| key.starts_with(prefix))
    }

    /// A click on `key`, of a panel's node or of a button that opens one.
    fn panel_click(&mut self, key: &str) {
        let ask = match key {
            "open-menu" => PanelAsk::Open(MENU),
            "open-find" => PanelAsk::Open(FIND),
            "open-below" => PanelAsk::Open(BELOW),
            "open-note" => {
                self.note = !self.note;
                if self.note {
                    PanelAsk::Open(NOTE)
                } else {
                    self.paneled = "closed the note".to_owned();
                    PanelAsk::Close(NOTE)
                }
            }
            "find-bar.close" => PanelAsk::Close(FIND),
            "below.close" => PanelAsk::Close(BELOW),
            chosen if chosen.starts_with("menu.") => {
                self.paneled = format!("chose `{chosen}`");
                PanelAsk::Close(MENU)
            }
            _ => return,
        };
        if let PanelAsk::Open(_) = ask {
            self.paneled = format!("opened by `{key}`");
        }
        self.asks.push(ask);
    }

    /// What the panels did last.
    #[cfg(test)]
    pub fn last_panel(&self) -> &str {
        &self.paneled
    }

    /// What the last scroll did.
    #[cfg(test)]
    pub fn scrolled(&self) -> &str {
        &self.scrolled.0
    }

    /// What the gallery asks of its panels, since it was last asked.
    pub fn take_panels(&mut self) -> Vec<PanelAsk> {
        std::mem::take(&mut self.asks)
    }

    /// A panel closed, as the host closed it, as by Escape or a click
    /// outside it.
    pub fn closed(&mut self, panel: u8) {
        self.paneled = format!("closed panel {panel}");
    }

    /// The view of the panel `panel`: its toggles of the state that their
    /// clicks asked for.
    pub fn panel(&self, panel: u8) -> Node {
        let on = |key: &str| self.toggles.get(key).copied().unwrap_or(false);
        let box_of = |children: Vec<Node>, direction: Direction| {
            Node::container(children)
                .layout(Layout {
                    direction,
                    gap: 6.0,
                    padding: Edges::all(8.0),
                    align: Align::Center,
                    ..Layout::default()
                })
                .style(Style {
                    background: Some(Token::from("panel.bg")),
                    border: Some(Border {
                        color: Token::from("panel.border"),
                        width: 1.0,
                    }),
                    radius: 8.0,
                    shadow: Some(Shadow::default()),
                    ..Style::default()
                })
        };
        match panel {
            MENU => {
                let item = |key: &str, label: &str| {
                    Node::button(label).key(key).layout(Layout {
                        width: Length::Fraction(1.0),
                        ..Layout::default()
                    })
                };
                let hidden = Node::new(Kind::Toggle(view::Toggle {
                    label: "Hidden Files".to_owned(),
                    on: on("menu.hidden"),
                    ..view::Toggle::default()
                }))
                .key("menu.hidden")
                .layout(Layout {
                    width: Length::Fraction(1.0),
                    ..Layout::default()
                });
                let recent = column(
                    4.0,
                    vec![
                        secondary("Open Recent"),
                        item("menu.recent.lokyr", "~/Projects/lokyr"),
                        item("menu.recent.notes", "~/Notes"),
                    ],
                )
                .layout(Layout {
                    direction: Direction::Column,
                    gap: 4.0,
                    width: Length::Fraction(1.0),
                    ..Layout::default()
                });
                box_of(
                    vec![
                        item("menu.copy", "Copy"),
                        item("menu.paste", "Paste"),
                        Node::separator(),
                        hidden,
                        item("menu.split", "Split Right"),
                        Node::separator(),
                        recent,
                    ],
                    Direction::Column,
                )
                .layout(Layout {
                    direction: Direction::Column,
                    gap: 4.0,
                    padding: Edges::all(6.0),
                    width: Length::Points(160.0),
                    ..Layout::default()
                })
            }
            FIND => box_of(
                vec![
                    Node::icon("search").style(Style {
                        color: Some(Token::from("text.secondary")),
                        ..Style::default()
                    }),
                    Node::field("Find").key("find-bar.field"),
                    Node::toggle("case-sensitive", on("find-bar.case"))
                        .key("find-bar.case")
                        .tooltip("Match Case"),
                    Node::toggle("regex", on("find-bar.regex"))
                        .key("find-bar.regex")
                        .tooltip("Regular Expression"),
                    Node::icon_button("close")
                        .key("find-bar.close")
                        .tooltip("Close"),
                ],
                Direction::Row,
            ),
            BELOW => box_of(
                vec![
                    Node::text("Anchored below its button"),
                    Node::field("Name").key("below.field"),
                    Node::button("Done").key("below.close"),
                ],
                Direction::Column,
            ),
            _ => box_of(
                vec![secondary(
                    "A note of the tooltip's role: the pointer goes through it",
                )],
                Direction::Row,
            ),
        }
    }

    fn animations(&self) -> Node {
        let times = if self.slow { 10.0 } else { 1.0 };
        let moving = Transition {
            duration: 0.3 * times,
            easing: Easing::EaseInOut,
        };
        let fading = Transition {
            duration: Transition::default().duration * times,
            ..Transition::default()
        };
        let tabs = self
            .order
            .iter()
            .map(|&tab| {
                chip(
                    &format!("anim.tab-{tab}"),
                    &format!("Tab {}", tab + 1),
                    false,
                    PointerShape::Arrow,
                )
                .style(Style {
                    background: Some(Token::from("tab.inactive.bg")),
                    border: Some(Border {
                        color: Token::from("separator"),
                        width: 1.0,
                    }),
                    radius: 6.0,
                    transition: Some(moving),
                    ..Style::default()
                })
            })
            .collect();
        let faded = Node::container(vec![Node::text("A box that fades")])
            .key("anim.box")
            .layout(Layout {
                padding: Edges::all(8.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("toggle.on.bg")),
                radius: 6.0,
                opacity: Some(if self.boxed { 1.0 } else { 0.0 }),
                transition: Some(fading),
                ..Style::default()
            });
        let controls = Node::container(vec![
            Node::button("Move the last first").key("anim.shuffle"),
            Node::new(Kind::Toggle(view::Toggle {
                label: "Show the box".to_owned(),
                on: self.boxed,
                ..view::Toggle::default()
            }))
            .key("anim.boxed"),
            Node::new(Kind::Toggle(view::Toggle {
                label: "Slow motion".to_owned(),
                on: self.slow,
                ..view::Toggle::default()
            }))
            .key("anim.slow"),
        ])
        .layout(Layout {
            gap: 8.0,
            align: Align::Center,
            ..Layout::default()
        });
        column(
            8.0,
            vec![
                Node::container(tabs).layout(Layout {
                    gap: 6.0,
                    ..Layout::default()
                }),
                controls,
                faded,
                secondary("Reduce motion in the system's settings makes them instant"),
            ],
        )
    }

    fn panels(&self) -> Node {
        let opener = |key: &str, label: &str| Node::button(label).key(key);
        column(
            8.0,
            vec![
                Node::container(vec![
                    opener("open-menu", "A menu at the pointer"),
                    opener("open-find", "A find bar"),
                    opener("open-below", "A panel below this"),
                    opener(
                        "open-note",
                        if self.note { "Hide the note" } else { "A note" },
                    ),
                ])
                .layout(Layout {
                    gap: 8.0,
                    align: Align::Center,
                    ..Layout::default()
                }),
                secondary(&format!("last: {}", self.paneled)),
                secondary("Escape closes them; a click outside, the menu and the panel below"),
            ],
        )
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
                section("Buttons, toggles and separators", self.buttons()),
                section("Progress", self.progress()),
                section("A view of tabs", self.views()),
            ],
        );
        let right = column(
            16.0,
            vec![
                section("Icons", icons()),
                section("The pointer", self.pointer()),
                section("The keyboard's focus", self.focus()),
                section("Fields", self.fields()),
                section("A list of 100 000 rows", self.list()),
                section("Floating panels", self.panels()),
                section("Animations", self.animations()),
            ],
        );
        let halves = Node::container(vec![half(left), half(right)]).layout(Layout {
            gap: 16.0,
            align: Align::Start,
            padding: Edges {
                top: 0.0,
                right: 20.0,
                bottom: 20.0,
                left: 20.0,
            },
            ..Layout::default()
        });
        // The page below the header scrolls, of the window's height.
        let page = Node::scroll(vec![halves]).key("page").layout(Layout {
            direction: Direction::Column,
            grow: 1.0,
            height: Length::Points(0.0),
            ..Layout::default()
        });
        let header = self.header().layout(Layout {
            gap: 8.0,
            align: Align::Center,
            padding: Edges {
                top: 20.0,
                right: 20.0,
                bottom: 0.0,
                left: 20.0,
            },
            ..Layout::default()
        });
        column(16.0, vec![header, page]).layout(Layout {
            direction: Direction::Column,
            gap: 16.0,
            ..Layout::default()
        })
    }

    fn progress(&self) -> Node {
        let value = self.percent as f32 / 100.0;
        let of = |progress: Progress| Node::new(Kind::Progress(progress));
        let bar = |state| {
            of(Progress {
                state,
                value,
                ..Progress::default()
            })
            .layout(Layout {
                grow: 1.0,
                ..Layout::default()
            })
        };
        let labelled = |name: &str, node: Node| {
            Node::container(vec![label(name), node]).layout(Layout {
                gap: 8.0,
                align: Align::Center,
                ..Layout::default()
            })
        };
        // A bar of its own looks: 10 points high, square, of the accent's
        // fill over a field's background, or indeterminate, of half its
        // width, quick.
        let custom = of(Progress {
            state: if self.spinning {
                ProgressState::Indeterminate
            } else {
                ProgressState::Normal
            },
            value,
            radius: Some(0.0),
            segment: 0.5,
            period: Some(0.8),
            ..Progress::default()
        })
        .layout(Layout {
            grow: 1.0,
            height: Length::Points(10.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("field.border")),
            color: Some(Token::from("accent")),
            ..Style::default()
        });
        // Hairlines over nothing: a point high, and half of one, a pixel at a
        // scale of 2, of no track, indeterminate with the custom bar.
        let hairline = |height: f32| {
            of(Progress {
                state: if self.spinning {
                    ProgressState::Indeterminate
                } else {
                    ProgressState::Normal
                },
                value,
                ..Progress::default()
            })
            .layout(Layout {
                grow: 1.0,
                height: Length::Points(height),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("transparent")),
                ..Style::default()
            })
        };
        let ring = |state, side: f32, thickness: f32| {
            of(Progress {
                state,
                value,
                shape: ProgressShape::Ring,
                thickness,
                ..Progress::default()
            })
            .layout(Layout {
                width: Length::Points(side),
                height: Length::Points(side),
                ..Layout::default()
            })
        };
        let rings = Node::container(vec![
            ring(ProgressState::Normal, 16.0, 2.0),
            ring(ProgressState::Error, 24.0, 3.0),
            ring(ProgressState::Paused, 32.0, 4.0),
            ring(ProgressState::Indeterminate, 16.0, 2.0),
            of(Progress {
                state: ProgressState::Indeterminate,
                shape: ProgressShape::Ring,
                thickness: 4.0,
                segment: 0.25,
                period: Some(0.7),
                ..Progress::default()
            })
            .layout(Layout {
                width: Length::Points(32.0),
                height: Length::Points(32.0),
                ..Layout::default()
            }),
        ])
        .layout(Layout {
            gap: 12.0,
            align: Align::Center,
            ..Layout::default()
        });
        let controls = Node::container(vec![
            Node::button("−10%").key("less"),
            Node::button("+10%").key("more"),
            Node::new(Kind::Toggle(view::Toggle {
                label: "Indeterminate".to_owned(),
                on: self.spinning,
                ..view::Toggle::default()
            }))
            .key("spin"),
            secondary(&format!("{}%", self.percent)),
        ])
        .layout(Layout {
            gap: 8.0,
            align: Align::Center,
            ..Layout::default()
        });
        column(
            8.0,
            vec![
                labelled("normal", bar(ProgressState::Normal)),
                labelled("error", bar(ProgressState::Error)),
                labelled("paused", bar(ProgressState::Paused)),
                labelled("indeterminate", bar(ProgressState::Indeterminate)),
                labelled("its own", custom),
                labelled("1 pt, no track", hairline(1.0)),
                labelled("1 px at 2x", hairline(0.5)),
                rings,
                controls,
            ],
        )
    }

    fn list(&self) -> Node {
        let row = Node::container(vec![
            Node::icon(""),
            Node::text(""),
            Node::spacer(),
            secondary(""),
        ])
        .layout(Layout {
            gap: 6.0,
            align: Align::Center,
            padding: Edges {
                top: 2.0,
                right: 8.0,
                bottom: 2.0,
                left: 8.0,
            },
            ..Layout::default()
        });
        let icons: Vec<&str> = interface::icons().collect();
        let (first, end) = self.window;
        let items = (first..end.min(ROWS))
            .map(|index| ListItem {
                texts: vec![
                    icons[index % icons.len()].to_owned(),
                    format!("Row {index}"),
                    format!("{:08x}", (index as u64).wrapping_mul(2_654_435_761) as u32),
                ],
            })
            .collect();
        let mut list = Node::list(row, ROWS, (first, items))
            .key("rows")
            .layout(Layout {
                height: Length::Points(220.0),
                ..Layout::default()
            })
            .style(field());
        if let Kind::List(list) = &mut list.kind {
            list.row_height = Some(22.0);
        }
        let (first, end) = self.window;
        column(
            8.0,
            vec![
                list,
                secondary(&format!("last: {}", self.listed)),
                secondary(&format!("items given: {first} to {end}")),
                secondary("the arrows, Page Up and Down, Home and End select; Return activates"),
            ],
        )
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

    /// A settings panel's view of tabs, the last disabled, of a page each,
    /// which a click or the arrows, while it has the focus, choose.
    fn views(&self) -> Node {
        let tab = |label: &str, icon: &str, disabled: bool| view::Tab {
            label: label.to_owned(),
            icon: icon.to_owned(),
            disabled,
        };
        let tabs = vec![
            tab("General", "settings", false),
            tab("Appearance", "sparkle", false),
            tab("Profiles", "terminal", false),
            tab("Advanced", "", true),
        ];
        let page = match self.page {
            0 => column(
                6.0,
                vec![
                    Node::text("New windows open at the folder of the last one"),
                    secondary("The general page"),
                ],
            ),
            1 => column(
                6.0,
                vec![
                    Node::text("The theme follows the system's appearance"),
                    secondary("The appearance's page"),
                ],
            ),
            _ => column(
                6.0,
                vec![
                    Node::text("Default: zsh, of the login's folder"),
                    secondary("The profiles' page"),
                ],
            ),
        };
        column(
            8.0,
            vec![
                Node::tabs(tabs, self.page, page.key("views.page")).key("views"),
                secondary(
                    "A click, or the arrows with the focus, choose a tab; Advanced is disabled",
                ),
            ],
        )
    }

    fn buttons(&self) -> Node {
        let button = |key: &str, label: &str, icon: &str, disabled: bool| {
            Node::new(Kind::Button(view::Button {
                label: label.to_owned(),
                icon: icon.to_owned(),
                disabled,
            }))
            .key(key)
        };
        let toggle = |key: &str, icon: &str, tooltip: &str, disabled: bool| {
            Node::new(Kind::Toggle(view::Toggle {
                icon: icon.to_owned(),
                on: self.toggles.get(key).copied().unwrap_or(false),
                disabled,
                ..view::Toggle::default()
            }))
            .key(key)
            .tooltip(tooltip)
        };
        let row = |children| {
            Node::container(children).layout(Layout {
                gap: 8.0,
                align: Align::Center,
                ..Layout::default()
            })
        };
        let buttons = row(vec![
            button("replace", "Replace", "", false).tooltip("Replace the match"),
            button("replace-all", "Replace All", "", true).tooltip("No match to replace"),
            Node::separator(),
            button("settings", "Settings", "settings", false),
            button("add", "", "add", false).tooltip("New Tab"),
            button("trash", "", "trash", true),
        ]);
        let toggles = row(vec![
            toggle("case-sensitive", "case-sensitive", "Match Case", false),
            toggle("regex", "regex", "Regular Expression", false),
            toggle("whole-word", "whole-word", "Whole Word", false),
            Node::separator(),
            toggle("pinned", "pin", "Pinned, and disabled", true),
            Node::new(Kind::Toggle(view::Toggle {
                label: "Wrap Around".to_owned(),
                on: self.toggles.get("wrap").copied().unwrap_or(false),
                ..view::Toggle::default()
            }))
            .key("wrap"),
        ]);
        let on: Vec<&str> = self
            .toggles
            .iter()
            .filter(|(_, on)| **on)
            .map(|(key, _)| key.as_str())
            .collect();
        column(
            8.0,
            vec![
                buttons,
                toggles,
                Node::separator(),
                secondary(&format!("last: {}", self.pressed)),
                secondary(&format!("on: {}", on.join(", "))),
                secondary("rest the pointer on a button or a toggle for its tooltip"),
                secondary("a click leaves the focus; Tab reaches them with Keyboard navigation on"),
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
        let tab = KeyEvent {
            key: Key::Tab,
            action: KeyAction::Press,
            mods: Mods::default(),
            text: "",
            unmodified: "",
            base_layout: None,
        };
        let order = |every: bool| {
            let mut focus = Focus::new();
            focus.set_every_control(every);
            assert!(focus.first(0, &tree));
            let mut order = Vec::new();
            loop {
                let Target::Node { node, .. } = focus.target() else {
                    panic!("the focus left the panel");
                };
                let key = tree.key(node).unwrap().0.clone();
                if order.first() == Some(&key) {
                    return order;
                }
                order.push(key);
                focus.key(|_| Some(&tree), &tab);
            }
        };
        let rest = [
            "first",
            "second",
            "third",
            "group",
            "group.close",
            "find",
            "name",
            "narrow",
            "rows",
        ];
        // Without the system's keyboard navigation, the buttons, the
        // toggles and the view of tabs are left out, the disabled ones
        // always.
        assert_eq!(order(false), rest);
        let controls = [
            "replace",
            "settings",
            "add",
            "case-sensitive",
            "regex",
            "whole-word",
            "wrap",
            "less",
            "more",
            "spin",
            "views",
        ];
        let openers = [
            "open-menu",
            "open-find",
            "open-below",
            "open-note",
            "anim.shuffle",
            "anim.boxed",
            "anim.slow",
        ];
        assert_eq!(
            order(true),
            [&controls[..], &rest[..], &openers[..]].concat()
        );
    }

    #[test]
    fn the_find_bar_s_toggles_take_the_state_that_their_clicks_ask_for() {
        let mut gallery = Gallery::new(false);
        let toggle = |gallery: &Gallery, key: &str| {
            let view = gallery.panel(FIND);
            let mut tree = Tree::new();
            tree.update(&view).unwrap();
            let node = tree.by_key(&key.into()).unwrap();
            match tree.kind(node) {
                Some(Kind::Toggle(toggle)) => toggle.on,
                _ => panic!("no toggle"),
            }
        };
        assert!(!toggle(&gallery, "find-bar.case"));
        gallery.event("find-bar.case", UiEventKind::Toggle { on: true });
        assert!(toggle(&gallery, "find-bar.case"));
        assert!(!toggle(&gallery, "find-bar.regex"));
        gallery.event("find-bar.regex", UiEventKind::Toggle { on: true });
        gallery.event("find-bar.case", UiEventKind::Toggle { on: false });
        assert!(toggle(&gallery, "find-bar.regex"));
        assert!(!toggle(&gallery, "find-bar.case"));
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

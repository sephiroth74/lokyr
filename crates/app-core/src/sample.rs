//! A plugin of a view in every slot, for a build of tests, as `--features
//! sample-slots` builds it: the window's tabs in the tab bar, each lighter
//! under the pointer, which a click activates, and a button of a new tab,
//! of a tooltip; an indeterminate progress in the strip; in the status bar,
//! the tabs' count, a field, and the text that it holds, as its keys type
//! it, and two buttons: one of a find panel above it, of a field, which
//! Escape and a click outside close, and one of a menu of two items, which
//! the platform shows as its own, whose choice the status bar tells; and
//! each pane's id in its header. Each change of the workspace renders its
//! views again. Its section of the config, `[plugins.sample-slots]`, takes
//! `title`, the status bar's first text, and `place`, where the tab bar
//! goes, `titlebar`, `below-titlebar`, `bottom`, `left` or `right`, of a
//! column `width` points wide, which lays its tabs down; the field's text
//! is its state, which the status bar tells again at the next run.

use lokyr_kernel::commands::Error;
use lokyr_kernel::plugins::host::{BarPlace, TabId, WindowId};
use lokyr_kernel::plugins::ui::{Anchor, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_kernel::plugins::view::{
    Align, Direction, Edges, Length, Node, PointerShape, ProgressShape, ProgressState, Style,
    Token, UiEventKind,
};
use lokyr_kernel::plugins::{Cx, Data, Event, Plugin, WorkspaceEvent};

/// Its manifest, of every slot.
pub(crate) const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/sample-slots\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"tab-bar\", \"window-strip\", \"status-bar\", \"pane-header\"]\n",
    "config-schema = \"sample.schema.json\"\n",
);

/// The plugin, the views that it rendered, which it renders again, the tab
/// under the pointer, the text of the status bar's field, and its config's
/// title.
#[derive(Default)]
pub(crate) struct SampleSlots {
    views: Vec<ViewId>,
    hovered: Option<String>,
    typed: String,
    title: String,
}

/// A bar of `children`, of a token's background, its texts in the middle
/// of its height, past a padding.
fn bar(children: Vec<Node>, background: &str) -> Node {
    let mut node = Node::container(children);
    node.layout.grow = 1.0;
    node.layout.align = Align::Center;
    node.layout.gap = 12.0;
    node.layout.padding = Edges::all(6.0);
    node.style = Style {
        background: Some(Token::from(background)),
        ..Style::default()
    };
    node
}

/// A text of a token's color.
fn text(text: String, color: &str) -> Node {
    let mut node = Node::text(text);
    node.style.color = Some(Token::from(color));
    node
}

/// The key of the tab `tab`'s node.
fn tab_key(tab: TabId) -> String {
    format!("tab-{}", tab.0)
}

impl SampleSlots {
    /// Renders its views again, but those of windows and panes that closed.
    fn render_again(&self, cx: &mut Cx<'_>) {
        for view in &self.views {
            let _ = cx.request_render(view.clone());
        }
    }

    /// Takes its config's title, and puts its tab bar at its place.
    fn configure(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Data::Map(config) = cx.config()? else {
            return Ok(());
        };
        if let Some(Data::Text(title)) = config.get("title") {
            self.title.clone_from(title);
        }
        let width = match config.get("width") {
            Some(Data::Number(width)) => *width as f32,
            Some(Data::Integer(width)) => *width as f32,
            _ => 180.0,
        };
        let place = match config.get("place") {
            Some(Data::Text(place)) => match place.as_str() {
                "below-titlebar" => BarPlace::BelowTitlebar,
                "bottom" => BarPlace::Bottom,
                "left" => BarPlace::Left { width },
                "right" => BarPlace::Right { width },
                _ => BarPlace::Titlebar,
            },
            _ => BarPlace::Titlebar,
        };
        cx.set_tab_bar_place(place)
    }
}

impl Plugin for SampleSlots {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.configure(cx)?;
        // The field's text of the last run, if the app keeps a state.
        if let Ok(Data::Map(state)) = cx.state()
            && let Some(Data::Text(typed)) = state.get("typed")
        {
            self.typed.clone_from(typed);
        }
        cx.subscribe(&WorkspaceEvent::NAMES)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        if events.contains(&Event::ConfigChanged) {
            self.configure(cx)?;
        }
        self.render_again(cx);
        Ok(())
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("sample.schema.json").into())
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        match &event.kind {
            UiEventKind::HoverEnter if key.starts_with("tab-") => {
                self.hovered = Some(key.to_owned());
            }
            UiEventKind::HoverLeave if self.hovered.as_deref() == Some(key) => {
                self.hovered = None;
            }
            UiEventKind::Click if key.starts_with("tab-") => {
                if let Some(tab) = key.strip_prefix("tab-").and_then(|id| id.parse().ok()) {
                    cx.activate_tab(TabId(tab))?;
                }
            }
            UiEventKind::Click if key == "new" => {
                if let ViewId::TabBar(window) = &event.view {
                    cx.new_tab(*window, None)?;
                }
            }
            UiEventKind::InputChanged { text, .. } if key == "note" || key == "query" => {
                self.typed.clone_from(text);
                let state = Data::Map([("typed".to_owned(), Data::Text(text.clone()))].into());
                // Without a state directory, the text stays for this run.
                let _ = cx.set_state(state);
            }
            UiEventKind::Click if key == "find" || key == "menu" => {
                let role = if key == "find" {
                    Role::Dialog
                } else {
                    Role::Menu
                };
                cx.open_panel(
                    key,
                    PanelOptions {
                        anchor: Anchor::Node {
                            view: event.view.clone(),
                            key: key.into(),
                        },
                        role,
                        close_outside: true,
                        close_escape: true,
                        takes_keys: false,
                    },
                )?;
            }
            UiEventKind::Click if key == "one" || key == "two" => {
                self.typed = format!("chose {key}");
            }
            _ => return Ok(()),
        }
        self.render_again(cx);
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if !self.views.contains(view) {
            self.views.push(view.clone());
        }
        match view {
            ViewId::TabBar(window) => Some(self.tab_bar(cx, *window)),
            ViewId::WindowStrip(_) => {
                let mut progress =
                    Node::progress(ProgressShape::Bar, ProgressState::Indeterminate, 0.0);
                progress.layout.grow = 1.0;
                progress.layout.height = Length::Points(3.0);
                let mut strip = Node::container(vec![progress]);
                strip.layout.grow = 1.0;
                Some(strip)
            }
            ViewId::StatusBar(window) => {
                let count = cx.tabs(*window).map_or(0, |tabs| tabs.len());
                let mut note = Node::field("Type here").key("note");
                note.layout.width = Length::Points(160.0);
                note.style.pointer = Some(PointerShape::Text);
                Some(bar(
                    vec![
                        text(format!("{}: {count} tab(s)", self.title), "text.secondary"),
                        note,
                        text(format!("typed: {}", self.typed), "text.secondary"),
                        Node::button("Find").key("find").tooltip("A find panel"),
                        Node::button("Menu").key("menu").tooltip("A native menu"),
                    ],
                    "tabs.bar.bg",
                ))
            }
            ViewId::PaneHeader(pane) => Some(bar(
                vec![text(format!("pane {}", pane.0), "text.secondary")],
                "tabs.inactive.bg",
            )),
            ViewId::Panel(PanelId(id)) if id == "menu" => Some(Node::container(vec![
                Node::button("One").key("one"),
                Node::button("Two").key("two"),
            ])),
            ViewId::Panel(_) => {
                let mut query = Node::field("Find").key("query");
                query.layout.width = Length::Points(200.0);
                query.style.pointer = Some(PointerShape::Text);
                let mut panel = Node::container(vec![
                    query,
                    text("Escape closes".to_owned(), "text.secondary"),
                ]);
                panel.layout.gap = 8.0;
                panel.layout.padding = Edges::all(8.0);
                panel.layout.align = Align::Center;
                panel.style = Style {
                    background: Some(Token::from("tabs.bar.bg")),
                    radius: 6.0,
                    ..Style::default()
                };
                Some(panel)
            }
            // The titlebar's buttons are not the sample's.
            ViewId::TitleButtons(_) => None,
        }
    }
}

impl SampleSlots {
    /// The tab bar of `window`: its tabs, each of a key, lighter under the
    /// pointer, the active one's text brighter, and the button of a new
    /// tab.
    fn tab_bar(&self, cx: &mut Cx<'_>, window: WindowId) -> Node {
        let tabs = cx.tabs(window).unwrap_or_default();
        let mut shown: Vec<Node> = tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let title = if tab.title.is_empty() {
                    format!("Tab {}", index + 1)
                } else {
                    tab.title.clone()
                };
                let color = if tab.active {
                    "tabs.active.fg"
                } else {
                    "tabs.inactive.fg"
                };
                let key = tab_key(tab.id);
                let background = if self.hovered.as_deref() == Some(key.as_str()) {
                    "tabs.hover.bg"
                } else if tab.active {
                    "tabs.active.bg"
                } else {
                    "tabs.inactive.bg"
                };
                let mut node = Node::container(vec![text(title, color)]).key(key.as_str());
                node.layout.padding = Edges::all(4.0);
                node.style = Style {
                    background: Some(Token::from(background)),
                    radius: 4.0,
                    pointer: Some(PointerShape::PointingHand),
                    ..Style::default()
                };
                node
            })
            .collect();
        shown.push(Node::button("+").key("new").tooltip("New Tab"));
        let mut bar = bar(shown, "tabs.bar.bg");
        // Down a column, of its place.
        if cx
            .tab_bar_place()
            .is_ok_and(|place| place.width().is_some())
        {
            bar.layout.direction = Direction::Column;
            bar.layout.align = Align::Stretch;
            bar.layout.gap = 4.0;
        }
        bar
    }
}

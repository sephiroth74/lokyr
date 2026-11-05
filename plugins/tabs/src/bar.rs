//! The tab bar: a window's tabs, each of its title, its color, the keys
//! that select it and its close button, and the button of a new tab, of
//! the theme's tokens, across the window or down a column; and its config,
//! of `[plugins.tabs]`.
//!
//! A tab's background lightens under the pointer and darkens as it is
//! pressed, which the host draws and fades, with no render; its close
//! button shows under the pointer over the tab, as the config says.

use lokyr_plugin_sdk::host::{BarPlace, COLUMN_WIDTHS, Color, TabInfo};
use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::view::{
    Align, Direction, Easing, Edges, Hover, Layout, Length, Node, PointerShape, Style, Token,
    Transition,
};

/// Where the bar goes, of the config's `position`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Position {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

/// The bar's look, of the config's `style`: `Regular`, below a titlebar of
/// the window's title; `Compact` and `Minimal`, in the titlebar at the top,
/// `Minimal` of the terminal's colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Look {
    Regular,
    #[default]
    Compact,
    Minimal,
}

/// When a tab shows its close button, of the config's `close-button`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CloseButton {
    Always,
    #[default]
    Hover,
    Never,
}

/// The bar's config, of `[plugins.tabs]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Settings {
    pub(crate) position: Position,
    pub(crate) look: Look,
    pub(crate) hide_when_single: bool,
    pub(crate) show_index: bool,
    pub(crate) close_button: CloseButton,
    /// The column's width, in points, on the left or the right.
    pub(crate) column_width: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: Position::Top,
            look: Look::Compact,
            hide_when_single: true,
            show_index: true,
            close_button: CloseButton::Hover,
            column_width: 200.0,
        }
    }
}

impl Settings {
    /// The settings of `config`, its section, which the host checked
    /// against the schema and gave its defaults.
    pub(crate) fn of(config: &Data) -> Self {
        let mut settings = Self::default();
        let Data::Map(config) = config else {
            return settings;
        };
        let text = |name: &str| match config.get(name) {
            Some(Data::Text(text)) => Some(text.as_str()),
            _ => None,
        };
        let flag = |name: &str, default: bool| match config.get(name) {
            Some(Data::Boolean(value)) => *value,
            _ => default,
        };
        settings.position = match text("position") {
            Some("bottom") => Position::Bottom,
            Some("left") => Position::Left,
            Some("right") => Position::Right,
            _ => Position::Top,
        };
        settings.look = match text("style") {
            Some("regular") => Look::Regular,
            Some("minimal") => Look::Minimal,
            _ => Look::Compact,
        };
        settings.close_button = match text("close-button") {
            Some("always") => CloseButton::Always,
            Some("never") => CloseButton::Never,
            _ => CloseButton::Hover,
        };
        settings.hide_when_single = flag("hide-when-single", true);
        settings.show_index = flag("show-index", true);
        settings.column_width = match config.get("column-width") {
            Some(Data::Number(width)) => *width as f32,
            Some(Data::Integer(width)) => *width as f32,
            _ => settings.column_width,
        };
        settings
    }

    /// The bar's place: at the top in the titlebar, of a compact or a
    /// minimal look, or below it, of a regular one; at the bottom; or down a
    /// column of `width` points, the user's drag's, or the config's.
    pub(crate) fn place(&self, width: Option<f32>) -> BarPlace {
        let (narrowest, widest) = COLUMN_WIDTHS;
        let width = width.unwrap_or(self.column_width).clamp(narrowest, widest);
        match (self.position, self.look) {
            (Position::Top, Look::Regular) => BarPlace::BelowTitlebar,
            (Position::Top, _) => BarPlace::Titlebar,
            (Position::Bottom, _) => BarPlace::Bottom,
            (Position::Left, _) => BarPlace::Left { width },
            (Position::Right, _) => BarPlace::Right { width },
        }
    }

    /// Whether the bar goes down a column.
    fn column(&self) -> bool {
        matches!(self.position, Position::Left | Position::Right)
    }
}

/// The key of the node of tab `id`.
pub(crate) fn tab_key(id: u64) -> String {
    format!("tab-{id}")
}

/// The key of the close button of tab `id`.
pub(crate) fn close_key(id: u64) -> String {
    format!("close-{id}")
}

/// The key of the button of a new tab.
pub(crate) const NEW_KEY: &str = "new";

/// The tokens of the bar's look: the bar's background, a tab's active and
/// inactive ones and their texts'.
struct Tokens {
    bar: &'static str,
    active: (&'static str, &'static str),
    inactive: (&'static str, &'static str),
}

impl Look {
    fn tokens(self) -> Tokens {
        match self {
            Look::Minimal => Tokens {
                bar: "terminal.bg",
                active: ("tab.hover.bg", "terminal.fg"),
                inactive: ("terminal.bg", "text.secondary"),
            },
            Look::Regular | Look::Compact => Tokens {
                bar: "tab.bar.bg",
                active: ("tab.active.bg", "tab.active.fg"),
                inactive: ("tab.inactive.bg", "tab.inactive.fg"),
            },
        }
    }
}

/// How long a tab's hover and press fade, in seconds.
const FADE: f32 = 0.15;

/// The bar of `tabs`, a window's, each of the keys that select it of
/// `keys`, as `settings` lay it out; `None` of no tabs, or of one while the
/// config hides the bar then.
pub(crate) fn bar(tabs: &[TabInfo], keys: &[Option<String>], settings: &Settings) -> Option<Node> {
    if tabs.is_empty() || (tabs.len() == 1 && settings.hide_when_single) {
        return None;
    }
    let tokens = settings.look.tokens();
    let column = settings.column();
    let mut children: Vec<Node> = tabs
        .iter()
        .zip(keys.iter().chain(std::iter::repeat(&None)))
        .enumerate()
        .map(|(at, (tab, keys))| one_tab(tab, at, keys.as_deref(), settings, &tokens))
        .collect();
    children.push(new_button());
    let mut bar = Node::container(children)
        .layout(Layout {
            direction: if column {
                Direction::Column
            } else {
                Direction::Row
            },
            gap: if column { 2.0 } else { 4.0 },
            align: if column {
                Align::Stretch
            } else {
                Align::Center
            },
            padding: Edges {
                top: 4.0,
                right: 8.0,
                bottom: 4.0,
                left: 8.0,
            },
            grow: 1.0,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(tokens.bar)),
            ..Style::default()
        });
    if column {
        bar.layout.padding = Edges::all(6.0);
    }
    Some(bar)
}

/// The node of `tab`, at `at` of its window's tabs, of the `keys` that
/// select it.
fn one_tab(
    tab: &TabInfo,
    at: usize,
    keys: Option<&str>,
    settings: &Settings,
    tokens: &Tokens,
) -> Node {
    let (background, color) = if tab.active {
        tokens.active
    } else {
        tokens.inactive
    };
    let title = if tab.title.is_empty() {
        format!("Tab {}", at + 1)
    } else {
        tab.title.clone()
    };
    let mut children = Vec::new();
    // The tab's own color, as its program or the user gave it.
    if let Some(Color::Rgb(red, green, blue)) = tab.color {
        children.push(
            Node::container(vec![])
                .style(Style {
                    background: Some(Token::from(
                        format!("#{red:02x}{green:02x}{blue:02x}").as_str(),
                    )),
                    radius: 3.0,
                    ..Style::default()
                })
                .layout(Layout {
                    width: Length::Points(6.0),
                    height: Length::Points(6.0),
                    ..Layout::default()
                }),
        );
    }
    let mut text = Node::text(title);
    text.layout.grow = 1.0;
    children.push(text);
    if settings.show_index
        && let Some(keys) = keys
    {
        children.push(Node::text(keys).style(Style {
            color: Some(Token::from("text.secondary")),
            ..Style::default()
        }));
    }
    if settings.close_button != CloseButton::Never {
        children.push(close_button(tab.id.0, settings.close_button));
    }
    Node::container(children)
        .key(tab_key(tab.id.0).as_str())
        .layout(Layout {
            gap: 6.0,
            align: Align::Center,
            padding: Edges {
                top: 4.0,
                right: 8.0,
                bottom: 4.0,
                left: 10.0,
            },
            grow: if settings.column() { 0.0 } else { 1.0 },
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(background)),
            color: Some(Token::from(color)),
            radius: 6.0,
            pointer: Some(PointerShape::Arrow),
            hover: (!tab.active).then(|| Hover {
                background: Some(Token::from("tab.hover.bg")),
                ..Hover::default()
            }),
            pressed: Some(Hover {
                background: Some(Token::from("button.pressed.bg")),
                ..Hover::default()
            }),
            transition: Some(fade()),
            ..Style::default()
        })
}

/// The close button of tab `id`, shown as `when` says.
fn close_button(id: u64, when: CloseButton) -> Node {
    let hidden = when == CloseButton::Hover;
    Node::icon("close")
        .key(close_key(id).as_str())
        .label("Close Tab")
        .style(Style {
            radius: 4.0,
            pointer: Some(PointerShape::PointingHand),
            opacity: hidden.then_some(0.0),
            hover: Some(Hover {
                background: Some(Token::from("button.hover.bg")),
                ..Hover::default()
            }),
            parent_hover: hidden.then(|| Hover {
                opacity: Some(1.0),
                ..Hover::default()
            }),
            pressed: Some(Hover {
                background: Some(Token::from("button.pressed.bg")),
                ..Hover::default()
            }),
            transition: Some(fade()),
            ..Style::default()
        })
}

/// The button of a new tab, at the tabs' end.
fn new_button() -> Node {
    Node::icon_button("add")
        .key(NEW_KEY)
        .label("New Tab")
        .tooltip("New Tab")
}

/// A tab's fade, of its hover and its press.
fn fade() -> Transition {
    Transition {
        duration: FADE,
        easing: Easing::EaseOut,
    }
}

#[cfg(test)]
mod tests;

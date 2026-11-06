//! The tab bar: a window's tabs, each of its title, its color, the keys
//! that select it and its close button, and the button of a new tab, of
//! the theme's tokens, across the window or down a column; and its config,
//! of `[plugins.tabs]`.
//!
//! Its looks, after macOS's tabs and the terminal's own colors:
//!
//! - `standard` and `unified`: the tabs side by side in a rounded track, of
//!   equal widths, each its title and its keys in its middle, the active
//!   one a pill raised over the track, of a border and a shadow; the button
//!   of a new tab a circle past the track. `standard` puts them in a strip
//!   below the titlebar, or at the window's bottom; `unified`, in the
//!   titlebar, after the window's shortcut.
//! - `seamless`: the tabs as cells of the bar's whole height, a line
//!   between two, the active one of no background, the window's showing
//!   through it as it does through the terminal, translucent and blurred
//!   as the config makes it, open to the terminal below it, and the others
//!   darker, of a veil, over a line; its title in its middle, its color and
//!   its keys at its end, and the button of a new tab a cell of its own.
//! - Down a column: a row each, its title at its start and its keys at its
//!   end, the active one raised, of rounded corners in `standard` and a
//!   stadium's round ends in `unified`, or of the window's background in
//!   `seamless`, the rest of the column under the veil; the button of a new
//!   tab a row of its sign in its middle, and a line along the column's
//!   edge by the panes.
//!
//! A tab's background lightens under the pointer and darkens as it is
//! pressed, which the host draws and fades, with no render; its close
//! button shows at its start under the pointer over the tab, as the config
//! says.

use lokyr_plugin_sdk::host::{BarPlace, COLUMN_WIDTHS, Color, TabInfo};
use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Easing, Edges, Hover, Justify, Layout, Length, Node, PointerShape,
    Shadow, Style, Token, Transition,
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

/// The bar's look, of the config's `style`: `Standard`, below a titlebar of
/// the window's title; `Unified` and `Seamless`, in the titlebar at the top,
/// `Seamless` of the terminal's colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Look {
    Standard,
    #[default]
    Unified,
    Seamless,
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
            look: Look::Unified,
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
            Some("standard") => Look::Standard,
            Some("seamless") => Look::Seamless,
            _ => Look::Unified,
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

    /// The bar's place: at the top in the titlebar, of a unified or a
    /// seamless look, or below it, of a standard one; at the bottom; or down a
    /// column of `width` points, the user's drag's, or the config's.
    pub(crate) fn place(&self, width: Option<f32>) -> BarPlace {
        let (narrowest, widest) = COLUMN_WIDTHS;
        let width = width.unwrap_or(self.column_width).clamp(narrowest, widest);
        match (self.position, self.look) {
            (Position::Top, Look::Standard) => BarPlace::BelowTitlebar,
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

/// How long a tab's hover and press fade, in seconds.
const FADE: f32 = 0.15;

/// A pill's height, in points: a tab's, across a bar of the standard or the
/// unified look.
const PILL: f32 = 24.0;

/// The room between the track's edge and its pills, and between two, in
/// points.
const INSET: f32 = 2.0;

/// The room around the track in a strip, above it and below it, in points.
const MARGIN: f32 = 6.0;

/// The room of a tab's close button, and of its balance at its other end,
/// which keeps its title in its middle, in points: its icon a little
/// smaller than a text's line.
const BUTTON: f32 = 14.0;

/// A seamless bar's height across the bottom, in points; in the titlebar,
/// the titlebar's.
const CELL: f32 = 30.0;

/// The width of a seamless bar's cell of a new tab, in points.
const NEW_CELL: f32 = 32.0;

/// A row's height down a column, in points.
const ROW: f32 = 28.0;

/// The token of the veil that darkens the window's background in the
/// seamless look, under its tabs but the active one.
const VEIL: &str = "tab.seamless.veil";

/// A tab as the bar shows it: its own, its title, or `Tab N` of none, and
/// the keys that select it, as the config shows them.
struct Shown<'a> {
    tab: &'a TabInfo,
    title: String,
    keys: Option<&'a str>,
}

/// The bar of `tabs`, a window's, each of the keys that select it of
/// `keys`, and of the keys that bring the window forward, `window`, which a
/// bar in the titlebar shows after the window's buttons, as `settings` lay
/// it out; `None` of no tabs, or of one while the config hides the bar
/// then.
pub(crate) fn bar(
    tabs: &[TabInfo],
    keys: &[Option<String>],
    window: Option<&str>,
    settings: &Settings,
) -> Option<Node> {
    if tabs.is_empty() || (tabs.len() == 1 && settings.hide_when_single) {
        return None;
    }
    let shown: Vec<Shown<'_>> = tabs
        .iter()
        .enumerate()
        .map(|(at, tab)| Shown {
            tab,
            title: if tab.title.is_empty() {
                format!("Tab {}", at + 1)
            } else {
                tab.title.clone()
            },
            keys: keys
                .get(at)
                .and_then(Option::as_deref)
                .filter(|_| settings.show_index),
        })
        .collect();
    let window = window.filter(|_| settings.place(None) == BarPlace::Titlebar);
    Some(match settings.look {
        _ if settings.column() => column(&shown, settings),
        Look::Seamless => cells(&shown, window, settings),
        Look::Standard | Look::Unified => pills(&shown, window, settings),
    })
}

/// A bar of the standard or the unified look: its pills in their track,
/// and the button of a new tab; in a strip of a line by the panes, or in
/// the titlebar, after the window's keys.
fn pills(tabs: &[Shown<'_>], window: Option<&str>, settings: &Settings) -> Node {
    let track = Node::container(tabs.iter().map(|tab| pill(tab, settings)).collect())
        .layout(Layout {
            gap: INSET,
            padding: Edges::all(INSET),
            align: Align::Stretch,
            grow: 1.0,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("tab.track.bg")),
            radius: PILL / 2.0 + INSET,
            ..Style::default()
        });
    let mut children = Vec::new();
    if let Some(keys) = window {
        children.push(secondary(keys));
    }
    children.push(track);
    children.push(new_circle());
    let row = Node::container(children).layout(Layout {
        gap: 8.0,
        align: Align::Center,
        padding: Edges {
            top: MARGIN,
            right: 8.0,
            bottom: MARGIN,
            left: 8.0,
        },
        grow: 1.0,
        ..Layout::default()
    });
    // In the titlebar, over its own background, the track in its middle.
    if settings.place(None) == BarPlace::Titlebar {
        return row;
    }
    let children = match settings.position {
        Position::Bottom => vec![Node::separator(), row],
        _ => vec![row, Node::separator()],
    };
    Node::container(children)
        .layout(Layout {
            direction: Direction::Column,
            align: Align::Stretch,
            grow: 1.0,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("tab.bar.bg")),
            ..Style::default()
        })
}

/// The pill of `tab`: its title and its keys in its middle, its close
/// button at its start; the active one raised over the track.
fn pill(tab: &Shown<'_>, settings: &Settings) -> Node {
    let mut middle = Vec::new();
    middle.extend(dot(tab.tab.color));
    middle.push(Node::text(tab.title.as_str()));
    middle.extend(tab.keys.map(keys));
    let middle = Node::container(middle)
        .layout(Layout {
            gap: 6.0,
            align: Align::Center,
            justify: Justify::Center,
            width: Length::Points(0.0),
            grow: 1.0,
            ..Layout::default()
        })
        .style(Style {
            clip: true,
            ..Style::default()
        });
    let active = tab.tab.active;
    let style = if active {
        raised()
    } else {
        Style {
            color: Some(Token::from("tab.inactive.fg")),
            ..Style::default()
        }
    };
    let ends = settings.close_button != CloseButton::Never;
    let children = if ends {
        vec![
            close_button(tab.tab.id.0, settings.close_button),
            middle,
            balance(),
        ]
    } else {
        vec![middle]
    };
    Node::container(children)
        .key(tab_key(tab.tab.id.0).as_str())
        .layout(Layout {
            gap: 4.0,
            align: Align::Center,
            padding: Edges {
                top: 0.0,
                right: 6.0,
                bottom: 0.0,
                left: 6.0,
            },
            width: Length::Points(0.0),
            height: Length::Points(PILL),
            grow: 1.0,
            ..Layout::default()
        })
        .style(tab_style(style, PILL / 2.0, active))
}

/// A bar of the seamless look: its cells of the bar's height, a line
/// between two, the active one of the window's background and open to the
/// terminal, the others under the veil and over a line, at the bar's bottom
/// at the top and at its top at the bottom; after the window's keys in the
/// titlebar, of the titlebar's background and of no line, as the room of
/// the window's buttons is. The bar draws no background of its own.
fn cells(tabs: &[Shown<'_>], window: Option<&str>, settings: &Settings) -> Node {
    let top = settings.position != Position::Bottom;
    let mut children = Vec::new();
    if let Some(keys) = window {
        let content = Node::container(vec![secondary(keys)]).layout(Layout {
            align: Align::Center,
            padding: Edges {
                top: 0.0,
                right: 8.0,
                bottom: 0.0,
                left: 4.0,
            },
            grow: 1.0,
            ..Layout::default()
        });
        children.push(lined(content, top, false, Layout::default()));
    }
    for tab in tabs {
        if !children.is_empty() {
            children.push(Node::separator());
        }
        children.push(cell(tab, top, settings));
    }
    children.push(Node::separator());
    let plus = Node::icon_button("add")
        .key(NEW_KEY)
        .label("New Tab")
        .tooltip("New Tab")
        .layout(Layout {
            grow: 1.0,
            ..Layout::default()
        })
        .style(tab_style(
            Style {
                background: Some(Token::from("transparent")),
                ..Style::default()
            },
            0.0,
            false,
        ));
    children.push(
        lined(
            plus,
            top,
            true,
            Layout {
                width: Length::Points(NEW_CELL),
                ..Layout::default()
            },
        )
        .style(veiled()),
    );
    Node::container(children).layout(Layout {
        align: Align::Stretch,
        height: if top {
            Length::Auto
        } else {
            Length::Points(CELL)
        },
        grow: 1.0,
        ..Layout::default()
    })
}

/// The style of a seamless bar's room that no active tab takes: under the
/// veil, which darkens the window's background.
fn veiled() -> Style {
    Style {
        background: Some(Token::from(VEIL)),
        ..Style::default()
    }
}

/// The cell of `tab` in a seamless bar, at its top or not: its close button
/// at its start, its title in its middle, its color and its keys at its
/// end.
fn cell(tab: &Shown<'_>, top: bool, settings: &Settings) -> Node {
    let active = tab.tab.active;
    let title = Node::container(vec![Node::text(tab.title.as_str())])
        .layout(Layout {
            align: Align::Center,
            justify: Justify::Center,
            width: Length::Points(0.0),
            grow: 1.0,
            ..Layout::default()
        })
        .style(Style {
            clip: true,
            ..Style::default()
        });
    let mut children = Vec::new();
    if settings.close_button != CloseButton::Never {
        children.push(close_button(tab.tab.id.0, settings.close_button));
    }
    children.push(title);
    children.extend(dot(tab.tab.color));
    children.extend(tab.keys.map(keys));
    let content = Node::container(children).layout(Layout {
        gap: 6.0,
        align: Align::Center,
        padding: Edges {
            top: 0.0,
            right: 10.0,
            bottom: 0.0,
            left: 8.0,
        },
        grow: 1.0,
        ..Layout::default()
    });
    let (background, color) = if active {
        (None, "terminal.fg")
    } else {
        (Some(Token::from(VEIL)), "tab.inactive.fg")
    };
    lined(
        content,
        top,
        !active,
        Layout {
            width: Length::Points(0.0),
            grow: 1.0,
            ..Layout::default()
        },
    )
    .key(tab_key(tab.tab.id.0).as_str())
    .style(tab_style(
        Style {
            background,
            color: Some(Token::from(color)),
            ..Style::default()
        },
        0.0,
        active,
    ))
}

/// `content` over a line at its bottom, or under one at its top where
/// `top` is false, as `lined` asks, in a container of `layout`, its
/// direction a column's; of no line, the room of one.
fn lined(content: Node, top: bool, lined: bool, layout: Layout) -> Node {
    let line = Node::container(Vec::new())
        .layout(Layout {
            height: Length::Points(1.0),
            ..Layout::default()
        })
        .style(Style {
            background: lined.then(|| Token::from("tab.seamless.line")),
            ..Style::default()
        });
    let children = if top {
        vec![content, line]
    } else {
        vec![line, content]
    };
    Node::container(children).layout(Layout {
        direction: Direction::Column,
        align: Align::Stretch,
        ..layout
    })
}

/// A bar down a column: a row of each tab, and one of a new tab, its
/// sign in its middle, and a line along the column's edge by the panes; in
/// the seamless look, of no background of its own, the room that no active
/// tab takes under the veil, down to the column's bottom.
fn column(tabs: &[Shown<'_>], settings: &Settings) -> Node {
    let seamless = settings.look == Look::Seamless;
    let mut rows: Vec<Node> = tabs.iter().map(|tab| row(tab, settings)).collect();
    rows.push(
        Node::container(vec![Node::icon("add")])
            .key(NEW_KEY)
            .label("New Tab")
            .tooltip("New Tab")
            .layout(Layout {
                align: Align::Center,
                justify: Justify::Center,
                height: Length::Points(ROW),
                ..Layout::default()
            })
            .style(tab_style(
                Style {
                    background: seamless.then(|| Token::from(VEIL)),
                    color: Some(Token::from("tab.inactive.fg")),
                    ..Style::default()
                },
                row_radius(settings.look),
                false,
            )),
    );
    if seamless {
        rows.push(
            Node::container(Vec::new())
                .layout(Layout {
                    grow: 1.0,
                    ..Layout::default()
                })
                .style(veiled()),
        );
    }
    // Of the column's width, whatever its titles' widths.
    let list = Node::container(rows).layout(Layout {
        direction: Direction::Column,
        width: Length::Points(0.0),
        gap: if seamless { 0.0 } else { 2.0 },
        padding: if seamless {
            Edges::default()
        } else {
            Edges::all(6.0)
        },
        align: Align::Stretch,
        grow: 1.0,
        ..Layout::default()
    });
    let children = match settings.position {
        Position::Right => vec![Node::separator(), list],
        _ => vec![list, Node::separator()],
    };
    Node::container(children)
        .layout(Layout {
            align: Align::Stretch,
            grow: 1.0,
            ..Layout::default()
        })
        .style(Style {
            background: (!seamless).then(|| Token::from("tab.bar.bg")),
            ..Style::default()
        })
}

/// The row of `tab` down a column: its close button at its start, its
/// title, its color and its keys at its end; the active one raised, of a
/// border in the standard look, a pill's in the unified one, or of the
/// window's background in a seamless column, and the others under its
/// veil.
fn row(tab: &Shown<'_>, settings: &Settings) -> Node {
    let seamless = settings.look == Look::Seamless;
    let active = tab.tab.active;
    let mut children = Vec::new();
    if settings.close_button != CloseButton::Never {
        children.push(close_button(tab.tab.id.0, settings.close_button));
    }
    let mut title = Node::text(tab.title.as_str());
    title.layout.grow = 1.0;
    children.push(title);
    children.extend(dot(tab.tab.color));
    children.extend(tab.keys.map(keys));
    let style = match (active, settings.look) {
        (true, Look::Seamless) => Style {
            color: Some(Token::from("terminal.fg")),
            ..Style::default()
        },
        (true, Look::Unified) => raised(),
        (true, Look::Standard) => Style {
            shadow: None,
            ..raised()
        },
        (false, _) => Style {
            background: seamless.then(|| Token::from(VEIL)),
            color: Some(Token::from("tab.inactive.fg")),
            ..Style::default()
        },
    };
    // A stadium's ends round further in.
    let inset = if settings.look == Look::Unified {
        4.0
    } else {
        0.0
    };
    Node::container(children)
        .key(tab_key(tab.tab.id.0).as_str())
        .layout(Layout {
            gap: 6.0,
            align: Align::Center,
            padding: Edges {
                top: 0.0,
                right: 10.0 + inset,
                bottom: 0.0,
                left: 6.0 + inset,
            },
            height: Length::Points(ROW),
            ..Layout::default()
        })
        .style(tab_style(style, row_radius(settings.look), active))
}

/// The radius of a row's corners down a column of `look`: none in the
/// seamless look, a stadium's, of round ends, in the unified one, as its
/// pills are.
fn row_radius(look: Look) -> f32 {
    match look {
        Look::Standard => 6.0,
        Look::Unified => ROW / 2.0,
        Look::Seamless => 0.0,
    }
}

/// The style of the active tab raised over the others: its background, its
/// border and its shadow, and its text's color.
fn raised() -> Style {
    Style {
        background: Some(Token::from("tab.active.bg")),
        border: Some(Border {
            color: Token::from("tab.active.border"),
            width: 1.0,
        }),
        shadow: Some(Shadow {
            color: Token::from("tab.active.shadow"),
            x: 0.0,
            y: 1.0,
            blur: 2.0,
        }),
        color: Some(Token::from("tab.active.fg")),
        ..Style::default()
    }
}

/// `style` of a tab, of corners of `radius`: its hover, the active tab's
/// of none, and its press, faded, and the arrow over it.
fn tab_style(style: Style, radius: f32, active: bool) -> Style {
    Style {
        radius,
        pointer: Some(PointerShape::Arrow),
        hover: (!active).then(|| Hover {
            background: Some(Token::from("tab.hover.bg")),
            ..Hover::default()
        }),
        pressed: Some(Hover {
            background: Some(Token::from("button.pressed.bg")),
            ..Hover::default()
        }),
        transition: Some(fade()),
        ..style
    }
}

/// The dot of a tab's own color, as its program or the user gave it.
fn dot(color: Option<Color>) -> Option<Node> {
    let Some(Color::Rgb(red, green, blue)) = color else {
        return None;
    };
    Some(
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
    )
}

/// The keys that select a tab, of its text's color, which keep their
/// width as a long title takes the room.
fn keys(keys: &str) -> Node {
    Node::container(vec![Node::text(keys)])
}

/// A text of the secondary text's color.
fn secondary(text: &str) -> Node {
    Node::text(text).style(Style {
        color: Some(Token::from("text.secondary")),
        ..Style::default()
    })
}

/// The close button of tab `id`, shown as `when` says, of its own room.
fn close_button(id: u64, when: CloseButton) -> Node {
    let hidden = when == CloseButton::Hover;
    Node::icon("close")
        .key(close_key(id).as_str())
        .label("Close Tab")
        .layout(Layout {
            width: Length::Points(BUTTON),
            height: Length::Points(BUTTON),
            ..Layout::default()
        })
        .style(Style {
            color: Some(Token::from("text.secondary")),
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

/// The room at a pill's end of its close button at its start, which keeps
/// its title in its middle.
fn balance() -> Node {
    Node::container(Vec::new()).layout(Layout {
        width: Length::Points(BUTTON),
        ..Layout::default()
    })
}

/// The button of a new tab past the track, a circle of the track's
/// color.
fn new_circle() -> Node {
    let side = PILL + 2.0 * INSET;
    Node::icon_button("add")
        .key(NEW_KEY)
        .label("New Tab")
        .tooltip("New Tab")
        .layout(Layout {
            width: Length::Points(side),
            height: Length::Points(side),
            ..Layout::default()
        })
        .style(tab_style(
            Style {
                background: Some(Token::from("tab.track.bg")),
                color: Some(Token::from("tab.inactive.fg")),
                ..Style::default()
            },
            side / 2.0,
            false,
        ))
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

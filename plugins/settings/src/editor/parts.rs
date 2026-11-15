//! The Editor's specimen of the interface: Lokyr's parts drawn of the
//! theme in force, each a card of its title and its sample, of the tokens
//! that it takes, as the widgets themselves show them, and as their states
//! that no pointer makes show, a hovered or pressed button's, a focused
//! field's, drawn of the same tokens.

use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, Layout, Length, Node, ProgressShape, ProgressState, Role,
    Shadow, Style, Tab, TextSize, Token,
};

/// A part's card's key, of its id.
pub(super) const PART: &str = "editor-part:";

/// The parts, each of its id, its title's message and the tokens that it
/// takes, in the cards' order.
pub(super) const PARTS: [(&str, &str, &[&str]); 12] = [
    (
        "texts",
        "part-texts",
        &[
            "text",
            "text.secondary",
            "text.disabled",
            "text.success",
            "text.warning",
        ],
    ),
    (
        "panel",
        "part-panel",
        &["panel.bg", "panel.border", "shadow", "separator"],
    ),
    (
        "buttons",
        "part-buttons",
        &["button.bg", "button.hover.bg", "button.pressed.bg", "text"],
    ),
    (
        "field",
        "part-field",
        &[
            "field.bg",
            "field.border",
            "field.focus",
            "field.selection",
            "focus.ring",
        ],
    ),
    ("toggles", "part-toggles", &["toggle.on.bg", "toggle.on.fg"]),
    (
        "list",
        "part-list",
        &[
            "list.selection.bg",
            "list.selection.fg",
            "list.selection.inactive.bg",
        ],
    ),
    (
        "segments",
        "part-segments",
        &[
            "segments.bg",
            "segments.chosen.bg",
            "segments.chosen.fg",
            "segments.box.bg",
            "segments.box.border",
        ],
    ),
    (
        "tabs",
        "part-tabs",
        &[
            "tabs.bar.bg",
            "tabs.track.bg",
            "tabs.active.bg",
            "tabs.active.fg",
            "tabs.active.border",
            "tabs.active.shadow",
            "tabs.inactive.bg",
            "tabs.inactive.fg",
            "tabs.hover.bg",
        ],
    ),
    (
        "tooltip",
        "part-tooltip",
        &["tooltip.bg", "tooltip.fg", "tooltip.border"],
    ),
    (
        "progress",
        "part-progress",
        &[
            "progress.track",
            "progress.normal",
            "progress.error",
            "progress.paused",
        ],
    ),
    (
        "scrollbar",
        "part-scrollbar",
        &[
            "scrollbar",
            "scrollbar.hover",
            "scrollbar.track",
            "scrollbar.mark",
            "scrollbar.mark.current",
        ],
    ),
    (
        "panes",
        "part-panes",
        &["pane.divider", "pane.drop", "accent"],
    ),
];

/// The messages of the parts' titles and samples, which the page reads
/// before it builds them.
pub(super) const WORDS: [&str; 30] = [
    "part-texts",
    "part-panel",
    "part-buttons",
    "part-field",
    "part-toggles",
    "part-list",
    "part-segments",
    "part-tabs",
    "part-tooltip",
    "part-progress",
    "part-scrollbar",
    "part-panes",
    "part-text",
    "part-secondary",
    "part-disabled",
    "part-success",
    "part-warning",
    "part-button",
    "part-hovered",
    "part-pressed",
    "part-selected",
    "part-row",
    "part-chosen",
    "part-inactive",
    "part-one",
    "part-two",
    "part-three",
    "part-active",
    "part-tooltip-text",
    "part-field-text",
];

/// The parts across a row of cards.
const ACROSS: usize = 2;

/// The tokens of the part `id`, none of no part.
pub(super) fn tokens_of(id: &str) -> &'static [&'static str] {
    PARTS
        .iter()
        .find(|(part, ..)| *part == id)
        .map_or(&[], |(_, _, tokens)| tokens)
}

/// A box of `background`, `border` and `radius`, round `children` in a row.
fn boxed(children: Vec<Node>, background: &str, border: Option<&str>, radius: f32) -> Node {
    Node::container(children)
        .layout(Layout {
            direction: Direction::Row,
            gap: 6.0,
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
            background: Some(Token::from(background)),
            border: border.map(|color| Border {
                color: Token::from(color),
                width: 1.0,
            }),
            radius,
            ..Style::default()
        })
}

/// A text of `color`, small.
fn small(text: &str, color: &str) -> Node {
    Node::text(text).style(Style {
        color: Some(Token::from(color)),
        text_size: Some(TextSize::Small),
        ..Style::default()
    })
}

/// A box of `width` by `height` points of `background`, of `radius`.
fn block(width: f32, height: f32, background: &str, radius: f32) -> Node {
    Node::container(Vec::new())
        .layout(Layout {
            width: Length::Points(width),
            min_width: Length::Points(width),
            height: Length::Points(height),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(background)),
            radius,
            ..Style::default()
        })
}

/// A column of `children`, `gap` apart.
fn column(children: Vec<Node>, gap: f32) -> Node {
    Node::container(children).layout(Layout {
        direction: Direction::Column,
        gap,
        ..Layout::default()
    })
}

/// A row of `children`, `gap` apart, in the middle of its height.
fn row(children: Vec<Node>, gap: f32) -> Node {
    Node::container(children).layout(Layout {
        direction: Direction::Row,
        gap,
        align: Align::Center,
        ..Layout::default()
    })
}

/// The sample of the part `id`, its texts of `words`, the messages of its
/// labels by their names.
fn sample(id: &str, words: &dyn Fn(&str) -> String) -> Node {
    match id {
        "texts" => column(
            vec![
                small(&words("part-text"), "text"),
                small(&words("part-secondary"), "text.secondary"),
                small(&words("part-disabled"), "text.disabled"),
                small(&words("part-success"), "text.success"),
                small(&words("part-warning"), "text.warning"),
            ],
            2.0,
        ),
        "panel" => Node::container(vec![
            small(&words("part-panel"), "text"),
            Node::separator(),
            small(&words("part-secondary"), "text.secondary"),
        ])
        .layout(Layout {
            direction: Direction::Column,
            gap: 4.0,
            padding: Edges::all(8.0),
            width: Length::Fraction(1.0),
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
        }),
        "buttons" => row(
            vec![
                Node::button(words("part-button").as_str()).key("editor-sample-button"),
                boxed(
                    vec![small(&words("part-hovered"), "text")],
                    "button.hover.bg",
                    None,
                    6.0,
                ),
                boxed(
                    vec![small(&words("part-pressed"), "text")],
                    "button.pressed.bg",
                    None,
                    6.0,
                ),
            ],
            6.0,
        ),
        "field" => column(
            vec![
                Node::field(words("part-field-text"))
                    .key("editor-sample-field")
                    .layout(Layout {
                        width: Length::Fraction(1.0),
                        ..Layout::default()
                    }),
                Node::container(vec![boxed(
                    vec![
                        Node::container(vec![small(&words("part-selected"), "text")])
                            .layout(Layout {
                                padding: Edges {
                                    top: 0.0,
                                    right: 2.0,
                                    bottom: 0.0,
                                    left: 2.0,
                                },
                                ..Layout::default()
                            })
                            .style(Style {
                                background: Some(Token::from("field.selection")),
                                ..Style::default()
                            }),
                    ],
                    "field.bg",
                    Some("field.focus"),
                    5.0,
                )])
                .layout(Layout {
                    padding: Edges::all(2.0),
                    ..Layout::default()
                })
                .style(Style {
                    border: Some(Border {
                        color: Token::from("focus.ring"),
                        width: 2.0,
                    }),
                    radius: 7.0,
                    ..Style::default()
                }),
            ],
            6.0,
        ),
        "toggles" => row(
            vec![
                Node::toggle("pin", true).key("editor-sample-on"),
                Node::toggle("pin", false).key("editor-sample-off"),
            ],
            6.0,
        ),
        "list" => column(
            vec![
                boxed(
                    vec![small(&words("part-row"), "text")],
                    "transparent",
                    None,
                    5.0,
                ),
                boxed(
                    vec![small(&words("part-chosen"), "list.selection.fg")],
                    "list.selection.bg",
                    None,
                    5.0,
                ),
                boxed(
                    vec![small(&words("part-inactive"), "text")],
                    "list.selection.inactive.bg",
                    None,
                    5.0,
                ),
            ],
            2.0,
        ),
        "segments" => Node::tabs(
            ["part-one", "part-two", "part-three"]
                .iter()
                .map(|name| Tab::new(words(name)))
                .collect(),
            0,
            Node::container(Vec::new()),
        )
        .key("editor-sample-segments"),
        "tabs" => boxed(
            vec![
                boxed(
                    vec![small(&words("part-active"), "tabs.active.fg")],
                    "tabs.active.bg",
                    Some("tabs.active.border"),
                    6.0,
                )
                .style(Style {
                    background: Some(Token::from("tabs.active.bg")),
                    border: Some(Border {
                        color: Token::from("tabs.active.border"),
                        width: 1.0,
                    }),
                    shadow: Some(Shadow {
                        color: Token::from("tabs.active.shadow"),
                        ..Shadow::default()
                    }),
                    radius: 6.0,
                    ..Style::default()
                }),
                boxed(
                    vec![small(&words("part-hovered"), "tabs.inactive.fg")],
                    "tabs.hover.bg",
                    None,
                    6.0,
                ),
                boxed(
                    vec![small(&words("part-inactive"), "tabs.inactive.fg")],
                    "tabs.inactive.bg",
                    None,
                    6.0,
                ),
            ],
            "tabs.track.bg",
            None,
            8.0,
        )
        .layout(Layout {
            direction: Direction::Row,
            gap: 2.0,
            align: Align::Center,
            padding: Edges::all(3.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("tabs.track.bg")),
            radius: 8.0,
            ..Style::default()
        }),
        "tooltip" => boxed(
            vec![small(&words("part-tooltip-text"), "tooltip.fg")],
            "tooltip.bg",
            Some("tooltip.border"),
            5.0,
        ),
        "progress" => column(
            [
                ProgressState::Normal,
                ProgressState::Error,
                ProgressState::Paused,
            ]
            .into_iter()
            .enumerate()
            .map(|(at, state)| {
                Node::progress(ProgressShape::Bar, state, 0.6)
                    .key(format!("editor-sample-progress:{at}").as_str())
                    .layout(Layout {
                        width: Length::Fraction(1.0),
                        height: Length::Points(4.0),
                        ..Layout::default()
                    })
            })
            .collect(),
            8.0,
        ),
        "scrollbar" => row(
            vec![
                Node::container(vec![
                    block(6.0, 10.0, "transparent", 0.0),
                    block(6.0, 24.0, "scrollbar", 3.0),
                    block(6.0, 6.0, "transparent", 0.0),
                    block(6.0, 24.0, "scrollbar.hover", 3.0),
                ])
                .layout(Layout {
                    direction: Direction::Column,
                    padding: Edges::all(2.0),
                    ..Layout::default()
                })
                .style(Style {
                    background: Some(Token::from("scrollbar.track")),
                    radius: 4.0,
                    ..Style::default()
                }),
                column(
                    vec![
                        block(10.0, 2.0, "scrollbar.mark", 1.0),
                        block(10.0, 2.0, "scrollbar.mark.current", 1.0),
                    ],
                    8.0,
                ),
            ],
            8.0,
        ),
        _ => row(
            vec![
                block(40.0, 40.0, "terminal.bg", 4.0),
                block(1.0, 40.0, "pane.divider", 0.0),
                Node::container(vec![block(20.0, 40.0, "pane.drop", 0.0)])
                    .layout(Layout {
                        width: Length::Points(40.0),
                        height: Length::Points(40.0),
                        ..Layout::default()
                    })
                    .style(Style {
                        background: Some(Token::from("terminal.bg")),
                        border: Some(Border {
                            color: Token::from("accent"),
                            width: 1.0,
                        }),
                        radius: 4.0,
                        ..Style::default()
                    }),
            ],
            0.0,
        ),
    }
}

/// The cards of the parts, in rows of [`ACROSS`], each of its title and
/// its sample, of the accent's border where `chosen`, the part chosen or a
/// part of the token chosen, its texts of `words`.
pub(super) fn cards(chosen: &dyn Fn(&str) -> bool, words: &dyn Fn(&str) -> String) -> Vec<Node> {
    let cards: Vec<Node> = PARTS
        .iter()
        .map(|(id, title, _)| {
            let marked = chosen(id);
            Node::container(vec![
                small(&words(title), "text.secondary"),
                sample(id, words),
            ])
            .key(format!("{PART}{id}").as_str())
            .role(Role::Group)
            .label(words(title))
            .layout(Layout {
                direction: Direction::Column,
                gap: 8.0,
                grow: 1.0,
                width: Length::Points(0.0),
                min_width: Length::Points(0.0),
                padding: Edges::all(10.0),
                ..Layout::default()
            })
            .style(Style {
                border: Some(Border {
                    color: Token::from(if marked { "accent" } else { "separator" }),
                    width: if marked { 2.0 } else { 1.0 },
                }),
                radius: 8.0,
                ..Style::default()
            })
        })
        .collect();
    let mut rows = Vec::new();
    let mut cards = cards.into_iter().peekable();
    let mut at = 0;
    while cards.peek().is_some() {
        let chunk: Vec<Node> = cards.by_ref().take(ACROSS).collect();
        rows.push(
            Node::container(chunk)
                .key(format!("editor-parts:{at}").as_str())
                .layout(Layout {
                    direction: Direction::Row,
                    gap: 10.0,
                    width: Length::Fraction(1.0),
                    ..Layout::default()
                }),
        );
        at += 1;
    }
    rows
}

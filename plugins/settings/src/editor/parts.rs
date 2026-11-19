//! The Editor's previews of the theme that it shows, each in a part of
//! that theme: a window's, the terminal's specimen under a tab bar, beside
//! a scroll bar and over a second pane; and a plugin's panel's, of a
//! content as a plugin would show it, of real controls, which take the
//! pointer, and of the states that no pointer makes show at once, a
//! focused field's or a selected row's, drawn of the same tokens. Their
//! elements are parts, each of the tokens that it takes, which a click
//! marks in the list.

use lokyr_plugin_sdk::view::{
    Align, Border, Button, ButtonVariant, Direction, Edges, Kind, Layout, Length, Node,
    ProgressShape, ProgressState, Role, Shadow, Style, Tab, TextSize, Toggle, Token,
};

/// A part's key, of its id.
pub(super) const PART: &str = "editor-part:";

/// The key of a control of a part, of the part's id, a colon and its own
/// name.
pub(super) const SAMPLE: &str = "editor-sample:";

/// The key of the part of the theme that the window's preview is drawn in,
/// and of the one of the panel's.
pub(super) const WINDOW: &str = "editor-window";
pub(super) const PANEL: &str = "editor-panel";

/// The parts, each of its id, its title's message and the tokens that it
/// takes: the window's, then the panel's.
pub(super) const PARTS: [(&str, &str, &[&str]); 13] = [
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
            "tabs.seamless.line",
            "tabs.seamless.veil",
        ],
    ),
    (
        "terminal",
        "part-terminal",
        &[
            "terminal.bg",
            "terminal.fg",
            "gutter.running",
            "gutter.succeeded",
            "gutter.failed",
            "search.match",
            "search.match.current",
            "cursor.guide",
            "hints.label.bg",
            "hints.label.fg",
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
    ("panes", "part-panes", &["pane.divider", "pane.drop"]),
    (
        "panel",
        "part-panel",
        &["panel.bg", "panel.border", "shadow", "separator"],
    ),
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
    (
        "toggles",
        "part-toggles",
        &[
            "toggle.on.bg",
            "toggle.on.fg",
            "switch.off.bg",
            "switch.on.bg",
            "switch.knob.bg",
            "switch.knob.shadow",
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
        "list",
        "part-list",
        &[
            "list.selection.bg",
            "list.selection.fg",
            "list.selection.inactive.bg",
        ],
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
        "tooltip",
        "part-tooltip",
        &["tooltip.bg", "tooltip.fg", "tooltip.border"],
    ),
    (
        "buttons",
        "part-buttons",
        &[
            "button.bg",
            "button.hover.bg",
            "button.pressed.bg",
            "button.default.bg",
            "button.default.pressed.bg",
            "button.default.fg",
            "button.destructive.fg",
            "accent",
        ],
    ),
];

/// The messages of the parts' titles and of the previews' texts, which the
/// page reads before it builds them.
pub(super) const WORDS: [&str; 32] = [
    "part-tabs",
    "part-terminal",
    "part-scrollbar",
    "part-panes",
    "part-panel",
    "part-texts",
    "part-field",
    "part-toggles",
    "part-segments",
    "part-list",
    "part-progress",
    "part-tooltip",
    "part-buttons",
    "sample-title",
    "sample-about",
    "sample-folder",
    "sample-compress",
    "sample-encrypt",
    "sample-files",
    "sample-schedule",
    "sample-log",
    "sample-uploading",
    "sample-paused",
    "sample-failed",
    "sample-succeeded",
    "sample-warning",
    "sample-never",
    "sample-tooltip",
    "sample-more",
    "sample-restore",
    "sample-cancel",
    "sample-back-up",
];

/// What the panel's controls show, as the pointer left them: its toggles,
/// and the segment chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Samples {
    pub(super) toggles: [bool; 3],
    pub(super) segment: usize,
}

impl Default for Samples {
    fn default() -> Self {
        Self {
            toggles: [true, false, true],
            segment: 0,
        }
    }
}

impl Samples {
    /// Takes the toggle of the control `name` of the panel's, `on`.
    pub(super) fn toggle(&mut self, name: &str, on: bool) {
        match name {
            "compress" => self.toggles[0] = on,
            "encrypt" => self.toggles[1] = on,
            "notify" => self.toggles[2] = on,
            _ => {}
        }
    }
}

/// The tokens of the part `id`, none of no part.
pub(super) fn tokens_of(id: &str) -> &'static [&'static str] {
    PARTS
        .iter()
        .find(|(part, ..)| *part == id)
        .map_or(&[], |(_, _, tokens)| tokens)
}

/// The id of the part of the control of `key`, a sample's, and the
/// control's name.
pub(super) fn sample_of(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix(SAMPLE)?.split_once(':')
}

/// The key of the control `name` of the part `id`.
fn sample(id: &str, name: &str) -> String {
    format!("{SAMPLE}{id}:{name}")
}

/// What the previews are built of: whether a part is marked, by its id,
/// and the messages of their texts, by their names.
struct Built<'a> {
    marked: &'a dyn Fn(&str) -> bool,
    words: &'a dyn Fn(&str) -> String,
}

impl Built<'_> {
    /// The part `id` round `node`: of its key, its title for a reader of
    /// the screen, and a line of the accent round it where it is marked.
    fn part(&self, id: &str, node: Node) -> Node {
        let title = PARTS
            .iter()
            .find(|(part, ..)| *part == id)
            .map_or_else(String::new, |(_, title, _)| (self.words)(title));
        let marked = (self.marked)(id);
        Node::container(vec![node])
            .key(format!("{PART}{id}").as_str())
            .role(Role::Group)
            .label(title)
            .layout(Layout {
                direction: Direction::Column,
                padding: Edges::all(2.0),
                ..Layout::default()
            })
            .style(Style {
                border: Some(Border {
                    color: Token::from(if marked { "accent" } else { "transparent" }),
                    width: 2.0,
                }),
                radius: 8.0,
                ..Style::default()
            })
    }

    /// A message of the previews' texts.
    fn word(&self, name: &str) -> String {
        (self.words)(name)
    }
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

/// A column of `children`, `gap` apart, across its room.
fn column(children: Vec<Node>, gap: f32) -> Node {
    Node::container(children).layout(Layout {
        direction: Direction::Column,
        gap,
        width: Length::Fraction(1.0),
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

/// A room that takes what its row leaves.
fn room() -> Node {
    Node::container(Vec::new()).layout(Layout {
        grow: 1.0,
        ..Layout::default()
    })
}

/// A button of `label`, of the part `id`, by its `name`: a link's, or
/// dimmed, as `look` says.
fn button(id: &str, name: &str, label: String, (link, disabled): (bool, bool)) -> Node {
    Node::new(Kind::Button(Button {
        label,
        icon: String::new(),
        disabled,
        link,
        ..Button::default()
    }))
    .key(sample(id, name).as_str())
}

/// [`button`] of `variant`, as a dialog's default button or its action that
/// removes something.
fn button_of(id: &str, name: &str, label: String, variant: ButtonVariant) -> Node {
    let mut node = button(id, name, label, (false, false));
    if let Kind::Button(own) = &mut node.kind {
        own.variant = variant;
    }
    node
}

/// The tab bar of the window's preview: a track of an active tab, one
/// under the pointer, one at rest and one under the veil of the others, and
/// its line below.
fn tab_bar() -> Node {
    let tab = |title: &str, background: &str, color: &str| {
        boxed(vec![small(title, color)], background, None, 6.0)
    };
    let active = boxed(
        vec![small("zsh", "tabs.active.fg")],
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
    });
    let track = Node::container(vec![
        active,
        tab("vim", "tabs.hover.bg", "tabs.inactive.fg"),
        tab("htop", "tabs.inactive.bg", "tabs.inactive.fg"),
        tab("ssh", "tabs.seamless.veil", "tabs.inactive.fg"),
    ])
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
    });
    let bar = Node::container(vec![track])
        .layout(Layout {
            direction: Direction::Row,
            width: Length::Fraction(1.0),
            padding: Edges::all(6.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("tabs.bar.bg")),
            ..Style::default()
        });
    let line = Node::container(Vec::new())
        .layout(Layout {
            width: Length::Fraction(1.0),
            height: Length::Points(1.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("tabs.seamless.line")),
            ..Style::default()
        });
    column(vec![bar, line], 0.0)
}

/// The scroll bar of the window's preview: its track, its thumb at rest and
/// under the pointer, and the marks of a search's matches along it.
fn scroll_bar() -> Node {
    Node::container(vec![
        block(6.0, 14.0, "transparent", 0.0),
        block(6.0, 2.0, "scrollbar.mark", 1.0),
        block(6.0, 10.0, "transparent", 0.0),
        block(6.0, 40.0, "scrollbar", 3.0),
        block(6.0, 8.0, "transparent", 0.0),
        block(6.0, 2.0, "scrollbar.mark.current", 1.0),
        block(6.0, 12.0, "transparent", 0.0),
        block(6.0, 28.0, "scrollbar.hover", 3.0),
    ])
    // As tall as the terminal beside it.
    .layout(Layout {
        direction: Direction::Column,
        grow: 1.0,
        min_width: Length::Points(10.0),
        padding: Edges::all(2.0),
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("scrollbar.track")),
        radius: 5.0,
        ..Style::default()
    })
}

/// The second pane of the window's preview, under the divider: its left
/// half lit as a tab dragged over it would light it.
fn second_pane() -> Node {
    let divider = Node::container(Vec::new())
        .layout(Layout {
            width: Length::Fraction(1.0),
            height: Length::Points(1.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("pane.divider")),
            ..Style::default()
        });
    let half = |background: &str| {
        Node::container(vec![small("$", "terminal.fg")])
            .layout(Layout {
                grow: 1.0,
                width: Length::Points(0.0),
                height: Length::Points(34.0),
                padding: Edges::all(8.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from(background)),
                ..Style::default()
            })
    };
    column(
        vec![
            divider,
            Node::container(vec![half("pane.drop"), half("transparent")]).layout(Layout {
                direction: Direction::Row,
                width: Length::Fraction(1.0),
                ..Layout::default()
            }),
        ],
        0.0,
    )
}

/// The window's preview, in a part of the theme `theme`: the tab bar, the
/// terminal's specimen beside the scroll bar, and a second pane under the
/// divider, of the terminal's background.
fn window(theme: &str, built: &Built<'_>) -> Node {
    let terminal = built.part(
        "terminal",
        // Of its own size, of the terminal's cells.
        Node::terminal_specimen()
            .key("editor-terminal")
            .label(built.word("part-terminal")),
    );
    // The scroll bar at the window's right edge, whatever its width.
    let panes = Node::container(vec![
        terminal,
        room(),
        built.part("scrollbar", scroll_bar()),
    ])
    .layout(Layout {
        direction: Direction::Row,
        gap: 2.0,
        width: Length::Fraction(1.0),
        ..Layout::default()
    });
    Node::themed(
        theme,
        vec![
            built.part("tabs", tab_bar()),
            panes,
            built.part("panes", second_pane()),
        ],
    )
    .key(WINDOW)
    .layout(Layout {
        direction: Direction::Column,
        width: Length::Fraction(1.0),
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("terminal.bg")),
        border: Some(Border {
            color: Token::from("panel.border"),
            width: 1.0,
        }),
        radius: 10.0,
        clip: true,
        ..Style::default()
    })
}

/// A field as the keyboard's focus and a selection show it, which no
/// pointer makes show beside the real one: its ring, its focused border
/// and its selected text.
fn focused_field() -> Node {
    let selected = Node::container(vec![small("nightly", "text")])
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
        });
    Node::container(vec![
        boxed(vec![selected, room()], "field.bg", Some("field.focus"), 5.0).layout(Layout {
            direction: Direction::Row,
            align: Align::Center,
            width: Length::Fraction(1.0),
            padding: Edges {
                top: 4.0,
                right: 8.0,
                bottom: 4.0,
                left: 8.0,
            },
            ..Layout::default()
        }),
    ])
    .layout(Layout {
        grow: 1.0,
        width: Length::Points(0.0),
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
    })
}

/// The rows of a list, as a list shows them: one at rest, the one
/// selected, and one selected in a list without the keyboard.
fn list_rows() -> Node {
    let item = |name: &str, color: &str, background: &str| {
        boxed(vec![small(name, color)], background, None, 5.0).layout(Layout {
            direction: Direction::Row,
            align: Align::Center,
            width: Length::Fraction(1.0),
            padding: Edges {
                top: 4.0,
                right: 8.0,
                bottom: 4.0,
                left: 8.0,
            },
            ..Layout::default()
        })
    };
    column(
        vec![
            item("sessions.tar", "text", "transparent"),
            item("config.toml", "list.selection.fg", "list.selection.bg"),
            item("themes/", "text", "list.selection.inactive.bg"),
        ],
        2.0,
    )
}

/// The lines of the panel's log, in an area that scrolls.
fn log() -> Node {
    let lines = [
        "02:00  sessions.tar   14 MB",
        "02:00  config.toml     3 KB",
        "02:01  themes/        48 KB",
        "02:01  history        2 MB",
        "02:02  plugins.lock    1 KB",
        "02:02  done in 94 s",
    ];
    Node::scroll(vec![column(
        lines
            .iter()
            .map(|line| small(line, "text.secondary"))
            .collect(),
        2.0,
    )])
    .key(sample("scrollbar", "log").as_str())
    .layout(Layout {
        direction: Direction::Column,
        width: Length::Fraction(1.0),
        height: Length::Points(44.0),
        ..Layout::default()
    })
}

/// The panel's preview, in a part of the theme `theme`, over the
/// terminal's background, as a plugin's panel shows over a pane: a title
/// and what it tells, a segmented control over a list, fields, toggles and
/// a switch, progresses of their states, texts of theirs, a log that scrolls, a
/// tooltip and buttons, as `samples` has its controls.
fn panel(theme: &str, built: &Built<'_>, samples: Samples) -> Node {
    let toggle = |name: &str, label: &str, on: bool| {
        Node::new(Kind::Toggle(Toggle {
            label: built.word(label),
            icon: String::new(),
            on,
            disabled: false,
            size: Default::default(),
        }))
        .key(sample("toggles", name).as_str())
    };
    let progress = |name: &str, label: &str, state: ProgressState, value: f32| {
        row(
            vec![
                small(&built.word(label), "text.secondary").layout(Layout {
                    width: Length::Points(84.0),
                    ..Layout::default()
                }),
                Node::progress(ProgressShape::Bar, state, value)
                    .key(sample("progress", name).as_str())
                    .layout(Layout {
                        grow: 1.0,
                        width: Length::Points(0.0),
                        height: Length::Points(4.0),
                        ..Layout::default()
                    }),
            ],
            8.0,
        )
        .layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        })
    };
    let header = built.part(
        "texts",
        column(
            vec![
                Node::text(built.word("sample-title")),
                small(&built.word("sample-about"), "text.secondary"),
                row(
                    vec![
                        small(&built.word("sample-succeeded"), "text.success"),
                        small(&built.word("sample-warning"), "text.warning"),
                        small(&built.word("sample-never"), "text.disabled"),
                    ],
                    12.0,
                ),
            ],
            4.0,
        ),
    );
    let fields = built.part(
        "field",
        row(
            vec![
                Node::field(built.word("sample-folder"))
                    .key(sample("field", "folder").as_str())
                    .layout(Layout {
                        grow: 1.0,
                        width: Length::Points(0.0),
                        ..Layout::default()
                    }),
                focused_field(),
            ],
            8.0,
        )
        .layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        }),
    );
    let toggles = built.part(
        "toggles",
        row(
            vec![
                toggle("compress", "sample-compress", samples.toggles[0]),
                toggle("encrypt", "sample-encrypt", samples.toggles[1]),
                room(),
                Node::switch(built.word("sample-notify"), samples.toggles[2])
                    .key(sample("toggles", "notify").as_str()),
            ],
            6.0,
        )
        .layout(Layout {
            direction: Direction::Row,
            gap: 6.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        }),
    );
    let segments = built.part(
        "segments",
        Node::tabs(
            ["sample-files", "sample-schedule", "sample-log"]
                .iter()
                .map(|name| Tab::new(built.word(name)))
                .collect(),
            samples.segment.min(2),
            Node::container(vec![built.part("list", list_rows())]).layout(Layout {
                direction: Direction::Column,
                width: Length::Fraction(1.0),
                ..Layout::default()
            }),
        )
        .key(sample("segments", "pages").as_str())
        .layout(Layout {
            width: Length::Fraction(1.0),
            ..Layout::default()
        }),
    );
    let progresses = built.part(
        "progress",
        column(
            vec![
                progress("uploading", "sample-uploading", ProgressState::Normal, 0.6),
                progress("paused", "sample-paused", ProgressState::Paused, 0.35),
                progress("failed", "sample-failed", ProgressState::Error, 0.8),
            ],
            8.0,
        ),
    );
    let tooltip = built.part(
        "tooltip",
        boxed(
            vec![small(&built.word("sample-tooltip"), "tooltip.fg")],
            "tooltip.bg",
            Some("tooltip.border"),
            5.0,
        ),
    );
    let buttons = built.part(
        "buttons",
        row(
            vec![
                button("buttons", "more", built.word("sample-more"), (true, false)),
                button(
                    "buttons",
                    "restore",
                    built.word("sample-restore"),
                    (false, true),
                ),
                button_of(
                    "buttons",
                    "delete",
                    built.word("sample-delete"),
                    ButtonVariant::Destructive,
                ),
                button(
                    "buttons",
                    "cancel",
                    built.word("sample-cancel"),
                    (false, false),
                ),
                button_of(
                    "buttons",
                    "back-up",
                    built.word("sample-back-up"),
                    ButtonVariant::Default,
                ),
            ],
            6.0,
        ),
    );
    let footer = Node::container(vec![tooltip, room(), buttons]).layout(Layout {
        direction: Direction::Row,
        gap: 8.0,
        align: Align::Center,
        width: Length::Fraction(1.0),
        ..Layout::default()
    });
    let content = Node::container(vec![
        header,
        Node::separator(),
        fields,
        toggles,
        segments,
        progresses,
        log(),
        Node::separator(),
        footer,
    ])
    .layout(Layout {
        direction: Direction::Column,
        gap: 8.0,
        width: Length::Fraction(1.0),
        padding: Edges::all(10.0),
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
    });
    Node::themed(theme, vec![built.part("panel", content)])
        .key(PANEL)
        .layout(Layout {
            direction: Direction::Column,
            width: Length::Fraction(1.0),
            padding: Edges::all(16.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("terminal.bg")),
            radius: 10.0,
            ..Style::default()
        })
}

/// The previews of the theme `theme`: the window's, then the panel's, of
/// the accent's line round the parts that `marked` says, by their ids, the
/// part chosen or the parts of the token chosen; their texts of `words`,
/// the panel's controls as `samples` has them.
pub(super) fn previews(
    theme: &str,
    marked: &dyn Fn(&str) -> bool,
    words: &dyn Fn(&str) -> String,
    samples: Samples,
) -> (Node, Node) {
    let built = Built { marked, words };
    (window(theme, &built), panel(theme, &built, samples))
}

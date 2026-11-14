//! The page's tree: its sidebar of its parts, and the part chosen beside
//! it, the installed plugins, the registry's, or a plugin's details, each
//! a column that scrolls, its plugins in groups of rows as macOS's
//! settings group theirs, each row's texts going onto more lines as its
//! room needs, its buttons at its right.

use std::cmp::Ordering;

use lokyr_plugin_sdk::host::{Decision, PluginInfo, PluginState};
use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::plugin::Cx;
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontWeight, Kind, Layout, Length, ListItem, Node,
    ProgressShape, ProgressState, Role, Style, TextSize, Token,
};

use super::registry::{self, Found, Info};
use super::{
    BACK, CANCEL, DETAILS, INSTALL, Manager, OWN, QUERY, REFRESH, REMOVE, SECTIONS, Section,
    TOGGLE, UPDATE, Work,
};

/// The sidebar's width, in points.
const SIDEBAR: f32 = 170.0;

/// The height of a row of the sidebar, in points, as macOS's sidebars'.
const ROW: f32 = 28.0;

/// The page's tree, of the plugins `plugins` as the host tells them.
pub fn tree(manager: &Manager, cx: &mut Cx<'_>, plugins: &[PluginInfo]) -> Node {
    let part = match (manager.section, &manager.detail) {
        (Section::Installed, _) => installed(manager, cx, plugins),
        (Section::Registry, Some((name, info))) => {
            details(manager, cx, plugins, name, info.as_ref())
        }
        (Section::Registry, None) => registry_part(manager, cx, plugins),
    };
    let part = Node::scroll(vec![Node::container(part).key("part").layout(Layout {
        direction: Direction::Column,
        gap: 10.0,
        padding: Edges::all(20.0),
        width: Length::Fraction(1.0),
        ..Layout::default()
    })])
    .key("plugins-scroll")
    .layout(Layout {
        // Down a column, of the page's height, past which its content
        // scrolls, beside the sidebar.
        direction: Direction::Column,
        grow: 1.0,
        min_width: Length::Points(0.0),
        height: Length::Fraction(1.0),
        ..Layout::default()
    });
    Node::container(vec![sidebar(cx), Node::separator(), part])
        .key("plugins-page")
        .layout(Layout {
            direction: Direction::Row,
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        })
}

/// The sidebar: a list of the page's parts, each a row of its icon and its
/// title, of a menu's look, its first selected as it shows.
fn sidebar(cx: &mut Cx<'_>) -> Node {
    let row = Node::container(vec![
        Node::icon("").style(Style {
            color: Some(Token::from("accent")),
            ..Style::default()
        }),
        Node::text(""),
    ])
    .layout(Layout {
        direction: Direction::Row,
        gap: 8.0,
        align: Align::Center,
        height: Length::Points(ROW),
        padding: Edges {
            top: 0.0,
            right: 8.0,
            bottom: 0.0,
            left: 8.0,
        },
        ..Layout::default()
    });
    let items = Section::ALL
        .iter()
        .map(|section| {
            let (icon, title) = match section {
                Section::Installed => ("check", "section-installed"),
                Section::Registry => ("search", "section-registry"),
            };
            ListItem {
                texts: vec![icon.to_owned(), message(cx, title, &[])],
            }
        })
        .collect();
    // Its selection tells where the keyboard is, of no ring, as macOS's
    // sidebars show none.
    let mut list = Node::list(row, Section::ALL.len(), (0, items))
        .key(SECTIONS)
        .label(message(cx, "page-plugins", &[]))
        .layout(Layout {
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        })
        .style(Style {
            hides_ring: true,
            ..Style::default()
        });
    if let Kind::List(list) = &mut list.kind {
        list.row_height = Some(ROW);
        list.menu = true;
    }
    // Of its own width alone, whatever the part beside it holds.
    Node::container(vec![list])
        .key("sidebar")
        .role(Role::Group)
        .layout(Layout {
            direction: Direction::Column,
            width: Length::Points(SIDEBAR),
            min_width: Length::Points(SIDEBAR),
            height: Length::Fraction(1.0),
            padding: Edges::all(10.0),
            ..Layout::default()
        })
}

/// The installed plugins, then Lokyr's own, each of its row.
fn installed(manager: &Manager, cx: &mut Cx<'_>, plugins: &[PluginInfo]) -> Vec<Node> {
    let (installed, built_in): (Vec<&PluginInfo>, Vec<&PluginInfo>) =
        plugins.iter().partition(|plugin| !plugin.built_in);
    let mut part = vec![title(message(cx, "plugins-installed", &[]), "title")];
    if installed.is_empty() {
        part.push(
            Node::text(message(cx, "plugins-none-installed", &[]))
                .key("none-installed")
                .style(secondary())
                .layout(wrapped()),
        );
    } else {
        let rows = installed
            .iter()
            .map(|plugin| installed_row(manager, cx, plugin))
            .collect();
        part.push(group(rows, "installed-plugins"));
    }
    part.push(heading(message(cx, "plugins-built-in", &[]), "built-in"));
    let rows = built_in
        .iter()
        .map(|plugin| installed_row(manager, cx, plugin))
        .collect();
    part.push(group(rows, "built-in-plugins"));
    part
}

/// The registry's plugins, which the field filters, of the state of their
/// reading: under way, failed, or of its notes.
fn registry_part(manager: &Manager, cx: &mut Cx<'_>, plugins: &[PluginInfo]) -> Vec<Node> {
    let mut part = vec![
        title(message(cx, "registry-title", &[]), "title"),
        Node::text(message(cx, "registry-intro", &[]))
            .key("intro")
            .style(small())
            .layout(wrapped()),
    ];
    let field = Node::field(message(cx, "registry-search", &[]))
        .key(QUERY)
        .label(message(cx, "registry-search", &[]))
        .layout(Layout {
            grow: 1.0,
            min_width: Length::Points(0.0),
            ..Layout::default()
        });
    let refresh = Node::button(message(cx, "registry-refresh", &[])).key(REFRESH);
    part.push(row_of(vec![field, refresh], 8.0).layout(Layout {
        direction: Direction::Row,
        gap: 8.0,
        align: Align::Center,
        width: Length::Fraction(1.0),
        ..Layout::default()
    }));
    if manager.reading() {
        part.push(
            row_of(
                vec![
                    spinner(),
                    Node::text(message(cx, "registry-reading", &[])).style(small()),
                ],
                6.0,
            )
            .key("reading"),
        );
    }
    if let Some(error) = &manager.registry_error {
        part.push(failure(
            message(cx, "registry-failed", &[("message", error.clone())]),
            "registry-failed",
        ));
    }
    for (at, note) in manager.notes.iter().enumerate() {
        part.push(
            Node::text(note.clone())
                .key(format!("note:{at}").as_str())
                .style(small())
                .layout(wrapped()),
        );
    }
    let Some(found) = &manager.found else {
        return part;
    };
    let query = manager.query.trim().to_lowercase();
    let shown: Vec<&Found> = found
        .iter()
        .filter(|found| {
            query.is_empty()
                || found.name.to_lowercase().contains(&query)
                || found.description.to_lowercase().contains(&query)
        })
        .collect();
    if shown.is_empty() {
        part.push(
            Node::text(message(cx, "registry-none", &[]))
                .key("none-found")
                .style(secondary()),
        );
    } else {
        let rows = shown
            .iter()
            .map(|found| found_row(manager, cx, plugins, found))
            .collect();
        part.push(group(rows, "found-plugins"));
    }
    part
}

/// A plugin's details: what it does, the commands that it runs, its
/// install, and its versions, each of what it asks for.
fn details(
    manager: &Manager,
    cx: &mut Cx<'_>,
    plugins: &[PluginInfo],
    name: &str,
    info: Option<&Info>,
) -> Vec<Node> {
    let mut back = Node::button(message(cx, "plugin-back", &[])).key(BACK);
    if let Kind::Button(button) = &mut back.kind {
        "chevron-left".clone_into(&mut button.icon);
    }
    let mut part = vec![row_of(vec![back], 0.0), title(name.to_owned(), "title")];
    let newest = info.and_then(|info| {
        info.versions
            .iter()
            .find(|version| !version.yanked && version.taken != Some(false))
    });
    let Some(info) = info else {
        if let Some(failure_text) = manager.failed.get(name) {
            part.push(failure(
                message(
                    cx,
                    "plugin-info-failed",
                    &[("message", failure_text.clone())],
                ),
                "info-failed",
            ));
        } else {
            part.push(
                row_of(
                    vec![
                        spinner(),
                        Node::text(message(cx, "plugin-reading", &[])).style(small()),
                    ],
                    6.0,
                )
                .key("reading"),
            );
        }
        return part;
    };
    if !info.description.is_empty() {
        part.push(
            Node::text(info.description.clone())
                .key("description")
                .layout(wrapped()),
        );
    }
    if !info.uses.is_empty() {
        part.push(
            Node::text(message(
                cx,
                "plugin-uses",
                &[("commands", info.uses.join(", "))],
            ))
            .key("uses")
            .style(small())
            .layout(wrapped()),
        );
    }
    let found = Found {
        name: name.to_owned(),
        version: newest
            .map(|version| version.version.clone())
            .unwrap_or_default(),
        description: String::new(),
        capabilities: newest
            .map(|version| version.capabilities.clone())
            .unwrap_or_default(),
        taken: newest.and_then(|version| version.taken),
    };
    part.push(row_of(actions_of_found(manager, cx, plugins, &found), 8.0).key("install-row"));
    if let Some(error) = manager.failed.get(name) {
        part.push(failure(
            message(cx, "plugin-change-failed", &[("message", error.clone())]),
            "failed",
        ));
    }
    part.push(heading(message(cx, "plugin-versions", &[]), "versions"));
    let rows = info
        .versions
        .iter()
        .map(|version| {
            let mut texts = vec![Node::text(version.version.clone()).style(Style {
                font_weight: Some(FontWeight::Semibold),
                ..Style::default()
            })];
            if version.yanked {
                texts.push(Node::text(message(cx, "version-yanked", &[])).style(warning()));
            } else if version.taken == Some(false) {
                texts.push(Node::text(message(cx, "version-not-taken", &[])).style(warning()));
            }
            let mut about = vec![row_of(texts, 6.0)];
            about.push(
                Node::text(asks(cx, &version.capabilities))
                    .style(small())
                    .layout(wrapped()),
            );
            row(
                about,
                Vec::new(),
                &format!("version:{}", version.version),
                &version.version,
            )
        })
        .collect();
    part.push(group(rows, "versions-list"));
    part
}

/// The row of an installed plugin or one of Lokyr's own: its name and
/// version, what it does, its state, an installed one's capabilities, and
/// its buttons, or its change under way, or the failure of the last.
fn installed_row(manager: &Manager, cx: &mut Cx<'_>, plugin: &PluginInfo) -> Node {
    let name = &plugin.name;
    let mut about = vec![name_line(name, plugin.version.as_deref())];
    if let Some(description) = &plugin.description {
        about.push(
            Node::text(description.clone())
                .key(format!("description:{name}").as_str())
                .style(small())
                .layout(wrapped()),
        );
    }
    // Active, whether it runs or waits for what starts it, or disabled,
    // turned off or stopped; a fault tells why on a line of its own.
    let active = matches!(plugin.state, PluginState::Active | PluginState::Waiting);
    let state = message(
        cx,
        if active {
            "plugin-state-active"
        } else {
            "plugin-state-disabled"
        },
        &[],
    );
    about.push(
        Node::text(state)
            .key(format!("state:{name}").as_str())
            .style(Style {
                color: Some(Token::from(if active {
                    "text.success"
                } else {
                    "text.warning"
                })),
                text_size: Some(TextSize::Small),
                ..Style::default()
            }),
    );
    if let (PluginState::Faulted, Some(fault)) = (plugin.state, &plugin.fault) {
        about.push(failure(
            message(cx, "plugin-fault", &[("fault", fault.clone())]),
            &format!("fault:{name}"),
        ));
    }
    // An installed plugin's capabilities, as the user decided them; a
    // built-in one holds them all.
    if !plugin.built_in && !plugin.capabilities.is_empty() {
        let decided: Vec<String> = plugin
            .capabilities
            .iter()
            .map(|decided| {
                let id = match decided.decision {
                    Decision::Granted => "plugin-decision-granted",
                    Decision::Refused => "plugin-decision-refused",
                    Decision::Undecided => "plugin-decision-undecided",
                };
                message(cx, id, &[("capability", decided.capability.clone())])
            })
            .collect();
        let capabilities = message(
            cx,
            "plugin-capabilities",
            &[("capabilities", decided.join("; "))],
        );
        about.push(Node::text(capabilities).style(small()).layout(wrapped()));
    }
    if let Some(error) = manager.failed.get(name) {
        about.push(failure(
            message(cx, "plugin-change-failed", &[("message", error.clone())]),
            &format!("failed:{name}"),
        ));
    }
    let actions = match manager.changing(name) {
        Some((job, work)) => under_way(cx, job, work),
        None => {
            let mut actions = Vec::new();
            // A newer version that the registry has and this Lokyr takes.
            if let Some(newer) = newer(manager, plugin) {
                actions.push(
                    Node::button(message(cx, "plugin-update", &[("version", newer)]))
                        .key(format!("{UPDATE}{name}").as_str()),
                );
            }
            if name != OWN {
                let id = if plugin.state == PluginState::Off {
                    "plugin-enable"
                } else {
                    "plugin-disable"
                };
                actions.push(
                    Node::button(message(cx, id, &[])).key(format!("{TOGGLE}{name}").as_str()),
                );
            }
            if !plugin.built_in {
                actions.push(
                    Node::button(message(cx, "plugin-remove", &[]))
                        .key(format!("{REMOVE}{name}").as_str()),
                );
            }
            actions
        }
    };
    row(about, actions, &format!("plugin:{name}"), name)
}

/// The row of a plugin of the registry: its name and version, what it
/// does, what it asks for, and its install, its update, or its change
/// under way, and its details' button.
fn found_row(manager: &Manager, cx: &mut Cx<'_>, plugins: &[PluginInfo], found: &Found) -> Node {
    let name = &found.name;
    let mut about = vec![name_line(name, Some(&found.version))];
    if found.taken == Some(false) {
        about.push(Node::text(message(cx, "version-not-taken", &[])).style(warning()));
    }
    if !found.description.is_empty() {
        about.push(
            Node::text(found.description.clone())
                .key(format!("description:{name}").as_str())
                .style(small())
                .layout(wrapped()),
        );
    }
    about.push(
        Node::text(asks(cx, &found.capabilities))
            .style(small())
            .layout(wrapped()),
    );
    if let Some(error) = manager.failed.get(name) {
        about.push(failure(
            message(cx, "plugin-change-failed", &[("message", error.clone())]),
            &format!("failed:{name}"),
        ));
    }
    let mut actions = actions_of_found(manager, cx, plugins, found);
    actions.push(
        Node::button(message(cx, "plugin-details", &[])).key(format!("{DETAILS}{name}").as_str()),
    );
    row(about, actions, &format!("found:{name}"), name)
}

/// The buttons of a plugin of the registry: its change under way; or its
/// update to the newer version found; or, installed, a text that says so;
/// or its install.
fn actions_of_found(
    manager: &Manager,
    cx: &mut Cx<'_>,
    plugins: &[PluginInfo],
    found: &Found,
) -> Vec<Node> {
    let name = &found.name;
    if let Some((job, work)) = manager.changing(name) {
        return under_way(cx, job, work);
    }
    let installed = plugins
        .iter()
        .find(|plugin| plugin.name == *name && !plugin.built_in);
    match installed {
        Some(plugin) => match plugin.version.as_deref() {
            Some(version)
                if !found.version.is_empty()
                    && found.taken != Some(false)
                    && registry::compare(&found.version, version) == Ordering::Greater =>
            {
                vec![
                    Node::button(message(
                        cx,
                        "plugin-update",
                        &[("version", found.version.clone())],
                    ))
                    .key(format!("{UPDATE}{name}").as_str()),
                ]
            }
            _ => vec![
                Node::text(message(cx, "plugin-installed", &[]))
                    .key(format!("installed:{name}").as_str())
                    .style(Style {
                        color: Some(Token::from("text.success")),
                        text_size: Some(TextSize::Small),
                        ..Style::default()
                    }),
            ],
        },
        None => vec![
            Node::button(message(cx, "plugin-install", &[]))
                .key(format!("{INSTALL}{name}").as_str()),
        ],
    }
}

/// The newer version of the installed `plugin` that the registry has and
/// this Lokyr takes, as its reading found, if it found one.
fn newer(manager: &Manager, plugin: &PluginInfo) -> Option<String> {
    let version = plugin.version.as_deref()?;
    let found = manager
        .found
        .as_ref()?
        .iter()
        .find(|found| found.name == plugin.name)?;
    (!plugin.built_in
        && found.taken != Some(false)
        && registry::compare(&found.version, version) == Ordering::Greater)
        .then(|| found.version.clone())
}

/// The buttons of a change under way, of the job `job`: a spinner, what it
/// does, and its cancel.
fn under_way(cx: &mut Cx<'_>, job: u64, work: &Work) -> Vec<Node> {
    let doing = match work {
        Work::Install(_) => "plugin-installing",
        Work::Update(_) => "plugin-updating",
        _ => "plugin-removing",
    };
    vec![
        spinner(),
        Node::text(message(cx, doing, &[])).style(small()),
        Node::button(message(cx, "plugin-cancel", &[])).key(format!("{CANCEL}{job}").as_str()),
    ]
}

/// What a version asks for: its capabilities, or none.
fn asks(cx: &mut Cx<'_>, capabilities: &[String]) -> String {
    if capabilities.is_empty() {
        message(cx, "plugin-asks-nothing", &[])
    } else {
        message(
            cx,
            "plugin-asks",
            &[("capabilities", capabilities.join(", "))],
        )
    }
}

/// The line of a plugin's name, of its version.
fn name_line(name: &str, version: Option<&str>) -> Node {
    let mut line = vec![Node::text(name.to_owned()).style(Style {
        font_weight: Some(FontWeight::Semibold),
        ..Style::default()
    })];
    if let Some(version) = version.filter(|version| !version.is_empty()) {
        line.push(Node::text(version.to_owned()).style(secondary()));
    }
    row_of(line, 6.0)
}

/// A plugin's row of the key `key`, named `label` for VoiceOver: `about`,
/// in the room that its buttons, `actions`, leave it, however narrow, its
/// texts going onto more lines, and its buttons at its right, of their own
/// width, across the group's whole width.
fn row(about: Vec<Node>, actions: Vec<Node>, key: &str, label: &str) -> Node {
    let about = Node::container(about).layout(Layout {
        direction: Direction::Column,
        gap: 3.0,
        grow: 1.0,
        min_width: Length::Points(0.0),
        ..Layout::default()
    });
    let actions = row_of(actions, 8.0).key(format!("actions:{label}").as_str());
    Node::container(vec![about, actions])
        .key(key)
        .role(Role::Group)
        .label(label.to_owned())
        .layout(Layout {
            direction: Direction::Row,
            gap: 12.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            padding: Edges {
                top: 10.0,
                right: 12.0,
                bottom: 10.0,
                left: 12.0,
            },
            ..Layout::default()
        })
}

/// The message `id` of the plugin's catalogs, of the texts `args`.
pub fn message(cx: &mut Cx<'_>, id: &str, args: &[(&str, String)]) -> String {
    let args: Vec<(&str, Data)> = args
        .iter()
        .map(|(name, value)| (*name, Data::Text(value.clone())))
        .collect();
    cx.message(id, &args).unwrap_or_else(|_| id.to_owned())
}

/// An indeterminate spinner, of a small text's height.
fn spinner() -> Node {
    Node::progress(ProgressShape::Ring, ProgressState::Indeterminate, 0.0).layout(Layout {
        width: Length::Points(14.0),
        height: Length::Points(14.0),
        ..Layout::default()
    })
}

/// A failure's text, of the key `key`.
fn failure(text: String, key: &str) -> Node {
    Node::text(text)
        .key(key)
        .style(Style {
            color: Some(Token::from("progress.error")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
        .layout(wrapped())
}

/// A row of `children`, `gap` points apart, in the middle of its height.
fn row_of(children: Vec<Node>, gap: f32) -> Node {
    Node::container(children).layout(Layout {
        direction: Direction::Row,
        gap,
        align: Align::Center,
        ..Layout::default()
    })
}

/// The part's title, `text`, of the key `key`.
fn title(text: String, key: &str) -> Node {
    Node::text(text).key(key).role(Role::Heading).style(Style {
        text_size: Some(TextSize::Large),
        font_weight: Some(FontWeight::Bold),
        ..Style::default()
    })
}

/// A section's heading, `text`, of the key `key`.
fn heading(text: String, key: &str) -> Node {
    Node::text(text)
        .key(key)
        .role(Role::Heading)
        .style(Style {
            font_weight: Some(FontWeight::Semibold),
            color: Some(Token::from("text.secondary")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
        .layout(Layout {
            padding: Edges {
                top: 8.0,
                right: 0.0,
                bottom: 0.0,
                left: 4.0,
            },
            ..Layout::default()
        })
}

/// A group of `rows`, of the key `key`, in a rounded box, a line between
/// each two, as macOS's settings group their rows.
fn group(rows: Vec<Node>, key: &str) -> Node {
    let mut children = Vec::new();
    for (at, row) in rows.into_iter().enumerate() {
        if at > 0 {
            children.push(Node::separator());
        }
        children.push(row);
    }
    Node::container(children)
        .key(key)
        .role(Role::Group)
        .layout(Layout {
            direction: Direction::Column,
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
            ..Style::default()
        })
}

/// The layout of a text of as many lines as its width needs.
fn wrapped() -> Layout {
    Layout {
        lines: Some(0),
        ..Layout::default()
    }
}

/// The style of a secondary text.
fn secondary() -> Style {
    Style {
        color: Some(Token::from("text.secondary")),
        ..Style::default()
    }
}

/// The style of a small secondary text.
fn small() -> Style {
    Style {
        text_size: Some(TextSize::Small),
        ..secondary()
    }
}

/// The style of a small text of a warning.
fn warning() -> Style {
    Style {
        color: Some(Token::from("text.warning")),
        text_size: Some(TextSize::Small),
        ..Style::default()
    }
}

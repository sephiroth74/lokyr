//! Built-in plugin of the settings window's first page, Settings: a sidebar
//! of the config's sections, as its schema has them, then of the plugins'
//! of a config schema, and beside it the chosen section's keys, each of
//! its name, its platforms, what it does and its value in force, marked as
//! its default where it is. The page reads the settings as it renders, of
//! the capability `config`, which Lokyr's own plugins alone may hold, and
//! follows the config as it applies anew. Like every built-in plugin, it
//! depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::host::{SettingKey, Settings, SettingsSection};
use lokyr_plugin_sdk::message::{Data, Event, SETTINGS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontFamily, FontWeight, Kind, Layout, Length, ListItem, Node,
    Role, Style, TextSize, Token, UiEventKind,
};

lokyr_plugin_sdk::export_plugin!(SettingsPage);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/settings.ftl")),
    ("it", include_str!("../locales/it/settings.ftl")),
];

/// Its page of the settings window.
const PAGE: &str = "settings";

/// The key of the sidebar's list of the sections.
const SECTIONS: &str = "sections";

/// The sidebar's width, in points.
const SIDEBAR: f32 = 170.0;

/// The height of a row of the sidebar, in points, as macOS's sidebars'.
const ROW: f32 = 28.0;

/// The plugin: the section chosen, by its place in the settings'.
#[derive(Default)]
pub struct SettingsPage {
    chosen: usize,
}

/// The page's view.
fn page() -> ViewId {
    ViewId::Page(PAGE.to_owned())
}

impl Plugin for SettingsPage {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.subscribe(&[SETTINGS_CHANGED])
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let mut again = false;
        for event in events {
            match event {
                // The page shows at its first section, as its sidebar does.
                Event::PageShown(_) => {
                    self.chosen = 0;
                    again = true;
                }
                Event::SettingsChanged => again = true,
                _ => {}
            }
        }
        if again {
            cx.request_render(page())?;
        }
        Ok(())
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let (UiEventKind::Select { item } | UiEventKind::Activate { item }) = &event.kind else {
            return Ok(());
        };
        if event.key.0 != SECTIONS || *item == self.chosen {
            return Ok(());
        }
        self.chosen = *item;
        cx.request_render(page())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view != page() {
            return None;
        }
        let settings = cx.settings().ok()?;
        Some(tree(cx, &settings, self.chosen))
    }
}

/// The page's tree of `settings`, of the section `chosen`.
fn tree(cx: &mut Cx<'_>, settings: &Settings, chosen: usize) -> Node {
    let part = match settings.sections.get(chosen) {
        Some(section) => section_part(cx, section),
        None => vec![
            Node::text(message(cx, "settings-none", &[]))
                .key("none")
                .style(secondary()),
        ],
    };
    let part = Node::scroll(vec![Node::container(part).key("part").layout(Layout {
        direction: Direction::Column,
        gap: 10.0,
        padding: Edges::all(20.0),
        width: Length::Fraction(1.0),
        ..Layout::default()
    })])
    .key("settings-scroll")
    .layout(Layout {
        // Down a column, of the page's height, past which its content
        // scrolls, beside the sidebar.
        direction: Direction::Column,
        grow: 1.0,
        min_width: Length::Points(0.0),
        height: Length::Fraction(1.0),
        ..Layout::default()
    });
    Node::container(vec![sidebar(cx, settings), Node::separator(), part])
        .key("settings-page")
        .layout(Layout {
            direction: Direction::Row,
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        })
}

/// The sidebar: a list of the sections, each a row of its icon and its
/// title, of a menu's look, its first selected as it shows, of no ring of
/// the keyboard's focus, as macOS's sidebars show none, and of its own
/// width alone.
fn sidebar(cx: &mut Cx<'_>, settings: &Settings) -> Node {
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
    let items = settings
        .sections
        .iter()
        .map(|section| {
            let icon = if section.plugin.is_some() {
                "extensions"
            } else {
                "settings"
            };
            ListItem {
                texts: vec![icon.to_owned(), title(cx, section)],
            }
        })
        .collect();
    let mut list = Node::list(row, settings.sections.len(), (0, items))
        .key(SECTIONS)
        .label(message(cx, "page-settings", &[]))
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

/// A section's title: its catalogs' message, or its name.
fn title(cx: &mut Cx<'_>, section: &SettingsSection) -> String {
    let id = match section.plugin {
        Some(_) => format!("section-plugin-{}", section.name),
        None => format!("section-{}", section.name),
    };
    let text = message(cx, &id, &[]);
    if text == id {
        section.name.clone()
    } else {
        text
    }
}

/// The part of the chosen section: its title, its plugin's name, what it
/// holds, and its keys in a group.
fn section_part(cx: &mut Cx<'_>, section: &SettingsSection) -> Vec<Node> {
    let mut part = vec![
        Node::text(title(cx, section))
            .key("title")
            .role(Role::Heading)
            .style(Style {
                text_size: Some(TextSize::Large),
                font_weight: Some(FontWeight::Bold),
                ..Style::default()
            }),
    ];
    if let Some(plugin) = &section.plugin {
        part.push(
            Node::text(message(
                cx,
                "settings-plugin",
                &[("plugin", plugin.clone())],
            ))
            .key("plugin")
            .style(small()),
        );
    }
    if !section.description.is_empty() {
        part.push(
            Node::text(section.description.clone())
                .key("description")
                .style(secondary())
                .layout(wrapped()),
        );
    }
    let rows = section.keys.iter().map(|key| key_row(cx, key)).collect();
    part.push(group(rows, "keys"));
    part
}

/// The row of `key`: its name, its platforms and what it does, and at its
/// right its value in force, marked as its default where it is, or that it
/// is not set.
fn key_row(cx: &mut Cx<'_>, key: &SettingKey) -> Node {
    let name = &key.name;
    let mut title = vec![Node::text(name.clone()).style(Style {
        font_weight: Some(FontWeight::Semibold),
        ..Style::default()
    })];
    if !key.platforms.is_empty() {
        let platforms: Vec<&str> = key.platforms.iter().map(|name| platform(name)).collect();
        title.push(
            Node::text(message(
                cx,
                "setting-platforms",
                &[("platforms", platforms.join(", "))],
            ))
            .style(small()),
        );
    }
    let mut about = vec![row_of(title, 6.0)];
    if !key.description.is_empty() {
        about.push(
            Node::text(key.description.clone())
                .key(format!("description:{name}").as_str())
                .style(small())
                .layout(wrapped()),
        );
    }
    let about = Node::container(about).layout(Layout {
        direction: Direction::Column,
        gap: 3.0,
        grow: 1.0,
        min_width: Length::Points(0.0),
        ..Layout::default()
    });
    let mut value = vec![match &key.value {
        Some(value) => Node::text(shown(value))
            .key(format!("value:{name}").as_str())
            .style(Style {
                font_family: Some(FontFamily::Terminal),
                ..Style::default()
            }),
        None => Node::text(message(cx, "setting-unset", &[]))
            .key(format!("value:{name}").as_str())
            .style(secondary()),
    }];
    if key.value.is_some() && key.value == key.default {
        value.push(
            Node::text(message(cx, "setting-default", &[]))
                .key(format!("default:{name}").as_str())
                .style(small()),
        );
    }
    let value = Node::container(value)
        .key(format!("values:{name}").as_str())
        .layout(Layout {
            direction: Direction::Column,
            gap: 2.0,
            align: Align::End,
            ..Layout::default()
        });
    Node::container(vec![about, value])
        .key(format!("key:{name}").as_str())
        .role(Role::Group)
        .label(name.clone())
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

/// A platform's name, as macOS's.
fn platform(name: &str) -> &str {
    match name {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    }
}

/// `value` as the config writes it: a text in quotes, a list in brackets.
fn shown(value: &Data) -> String {
    match value {
        Data::Nil => String::new(),
        Data::Boolean(value) => value.to_string(),
        Data::Integer(value) => value.to_string(),
        Data::Number(value) => {
            if value.fract() == 0.0 && value.abs() < 1e15 {
                format!("{value:.1}")
            } else {
                value.to_string()
            }
        }
        Data::Text(text) => format!("\"{text}\""),
        Data::List(items) => {
            let items: Vec<String> = items.iter().map(shown).collect();
            format!("[{}]", items.join(", "))
        }
        Data::Map(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(key, value)| format!("{key} = {}", shown(value)))
                .collect();
            format!("{{ {} }}", entries.join(", "))
        }
    }
}

/// The message `id` of the plugin's catalogs, of the texts `args`.
fn message(cx: &mut Cx<'_>, id: &str, args: &[(&str, String)]) -> String {
    let args: Vec<(&str, Data)> = args
        .iter()
        .map(|(name, value)| (*name, Data::Text(value.clone())))
        .collect();
    cx.message(id, &args).unwrap_or_else(|_| id.to_owned())
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

#[cfg(test)]
mod tests;

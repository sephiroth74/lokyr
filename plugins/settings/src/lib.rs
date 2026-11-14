//! Built-in plugin of the settings window's first page, Settings: a sidebar
//! of the config's sections, as its schema has them, then of the plugins'
//! of a config schema, and beside it the chosen section's keys, each of
//! its name, its platforms, what it does and a control of its value in
//! force, marked as its default where it is: a toggle of a boolean, a menu
//! of a key's values, a field of a number, with its steps' buttons, or of
//! a text, and a button that takes the key back to its default. A change
//! writes the key into the config file, the rest of the file and its
//! comments kept, and a value that the key does not take tells why under
//! it. The page reads the settings as it renders, of the capability
//! `config`, which Lokyr's own plugins alone may hold, and follows the
//! config as it applies anew, of a change of the file too. Like every
//! built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::host::{SettingKey, SettingKind, Settings, SettingsSection};
use lokyr_plugin_sdk::message::{Data, Event, SETTINGS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId};
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

/// The prefix of the key of a boolean's toggle.
const SET: &str = "set:";

/// The prefix of the key of the button of a key's menu of values.
const CHOOSE: &str = "choose:";

/// The prefix of the panel of a key's menu of values.
const CHOICES: &str = "choices:";

/// The prefix of the key of a value's item of a key's menu.
const CHOICE: &str = "choice:";

/// The prefix of the key of a number's or a text's field.
const FIELD: &str = "field:";

/// The prefix of the key of a number's button of a step less.
const LESS: &str = "less:";

/// The prefix of the key of a number's button of a step more.
const MORE: &str = "more:";

/// The prefix of the key of a key's button back to its default.
const RESET: &str = "reset:";

/// The plugin: whether its page shows, the section chosen, by its place in
/// the settings', and why each key's last write was refused, by its name,
/// until it is written or another section shows.
#[derive(Default)]
pub struct SettingsPage {
    shown: bool,
    chosen: usize,
    refused: BTreeMap<String, String>,
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
                    self.shown = true;
                    self.chosen = 0;
                    again = true;
                }
                Event::PageHidden(_) => self.shown = false,
                Event::SettingsChanged => again = true,
                _ => {}
            }
        }
        // Its view is there while its page shows alone.
        if again && self.shown {
            cx.request_render(page())?;
        }
        Ok(())
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        if let (UiEventKind::Select { item } | UiEventKind::Activate { item }, SECTIONS) =
            (&event.kind, key)
        {
            if *item == self.chosen {
                return Ok(());
            }
            self.chosen = *item;
            self.refused.clear();
            return cx.request_render(page());
        }
        let settings = cx.settings()?;
        let Some(section) = settings.sections.get(self.chosen) else {
            return Ok(());
        };
        // An item of a key's menu of values, its panel of its key's name.
        if let ViewId::Panel(PanelId(panel)) = &event.view {
            let (Some(name), Some(at)) = (panel.strip_prefix(CHOICES), key.strip_prefix(CHOICE))
            else {
                return Ok(());
            };
            let (Some(setting), Ok(at)) = (find(section, name), at.parse::<usize>()) else {
                return Ok(());
            };
            let SettingKind::Choice { choices } = &setting.kind else {
                return Ok(());
            };
            if matches!(event.kind, UiEventKind::Click | UiEventKind::Toggle { .. })
                && let Some(choice) = choices.get(at)
            {
                return self.write(cx, section, setting, Some(choice.clone()));
            }
            return Ok(());
        }
        let Some((prefix, name)) = key.split_once(':') else {
            return Ok(());
        };
        let Some(setting) = find(section, name) else {
            return Ok(());
        };
        let prefix = format!("{prefix}:");
        match (&event.kind, prefix.as_str()) {
            (UiEventKind::Toggle { on }, SET) => {
                self.write(cx, section, setting, Some(Data::Boolean(*on)))
            }
            (UiEventKind::Click, CHOOSE) => cx.open_panel(
                format!("{CHOICES}{name}").as_str(),
                PanelOptions {
                    anchor: Anchor::Node {
                        view: page(),
                        key: key.into(),
                    },
                    role: PanelRole::Menu,
                    close_outside: true,
                    close_escape: true,
                    takes_keys: false,
                    close_typing: false,
                },
            ),
            (UiEventKind::Submit { text, .. }, FIELD) => match value_of(cx, &setting.kind, text) {
                Ok(value) => self.write(cx, section, setting, value),
                Err(why) => {
                    self.refused.insert(name.to_owned(), why);
                    cx.request_render(page())
                }
            },
            (UiEventKind::Click, LESS | MORE) => {
                let toward = if prefix == MORE { 1.0 } else { -1.0 };
                match stepped(setting, toward) {
                    Some(value) => self.write(cx, section, setting, Some(value)),
                    None => Ok(()),
                }
            }
            (UiEventKind::Click, RESET) => self.write(cx, section, setting, None),
            _ => Ok(()),
        }
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let settings = cx.settings().ok()?;
        // A key's menu of values, each an item ticked where it is in force.
        if let ViewId::Panel(PanelId(panel)) = view {
            let name = panel.strip_prefix(CHOICES)?;
            let setting = find(settings.sections.get(self.chosen)?, name)?;
            let SettingKind::Choice { choices } = &setting.kind else {
                return None;
            };
            let items = choices
                .iter()
                .enumerate()
                .map(|(at, choice)| {
                    let mut item = Node::toggle("", setting.value.as_ref() == Some(choice))
                        .key(format!("{CHOICE}{at}").as_str());
                    if let Kind::Toggle(toggle) = &mut item.kind {
                        toggle.label = plain(choice);
                    }
                    item
                })
                .collect();
            return Some(Node::container(items));
        }
        if *view != page() {
            return None;
        }
        Some(tree(cx, &settings, self.chosen, &self.refused))
    }
}

impl SettingsPage {
    /// Writes `value` to the key `setting` of `section`, or takes it away of
    /// none: the page follows as the config applies anew, or tells why the
    /// key does not take it.
    fn write(
        &mut self,
        cx: &mut Cx<'_>,
        section: &SettingsSection,
        setting: &SettingKey,
        value: Option<Data>,
    ) -> Result<(), Error> {
        let written = cx.set_setting(
            &section.name,
            section.plugin.as_deref(),
            &setting.name,
            value,
        );
        let changed = match written {
            Ok(()) => self.refused.remove(&setting.name).is_some(),
            Err(error) => {
                self.refused.insert(setting.name.clone(), error.message);
                true
            }
        };
        if changed {
            cx.request_render(page())?;
        }
        Ok(())
    }
}

/// The key `name` of `section`, if it has one.
fn find<'s>(section: &'s SettingsSection, name: &str) -> Option<&'s SettingKey> {
    section.keys.iter().find(|key| key.name == name)
}

/// The value of a key of `kind` that a field's `text` gives: a number of
/// a number's, whole of an integer's, the text of a text's; of an empty
/// field, none, the key back to its default; or why not.
fn value_of(cx: &mut Cx<'_>, kind: &SettingKind, text: &str) -> Result<Option<Data>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match kind {
        SettingKind::Number { integer, .. } => {
            let Ok(number) = text.parse::<f64>() else {
                return Err(message(cx, "setting-not-a-number", &[]));
            };
            if !number.is_finite() {
                return Err(message(cx, "setting-not-a-number", &[]));
            }
            if number.fract() == 0.0 && number.abs() < 9.0e15 {
                return Ok(Some(Data::Integer(number as i64)));
            }
            if *integer {
                return Err(message(cx, "setting-not-whole", &[]));
            }
            Ok(Some(Data::Number(number)))
        }
        _ => Ok(Some(Data::Text(text.to_owned()))),
    }
}

/// The value of the number `setting` a step toward `toward`, more of a
/// positive one, within its range: its value in force, or its range's
/// least, moved by its step; none of a key of no step.
fn stepped(setting: &SettingKey, toward: f64) -> Option<Data> {
    let SettingKind::Number {
        minimum,
        maximum,
        step: Some(step),
        ..
    } = setting.kind
    else {
        return None;
    };
    let now = match &setting.value {
        Some(Data::Integer(value)) => *value as f64,
        Some(Data::Number(value)) => *value,
        _ => minimum.unwrap_or(0.0),
    };
    let mut next = now + step * toward;
    if let Some(least) = minimum {
        next = next.max(least);
    }
    if let Some(most) = maximum {
        next = next.min(most);
    }
    // A whole number as an integer, as the settings tell it.
    Some(if next.fract() == 0.0 && next.abs() < 9.0e15 {
        Data::Integer(next as i64)
    } else {
        Data::Number(next)
    })
}

/// The page's tree of `settings`, of the section `chosen`, its keys'
/// writes refused of why, `refused`.
fn tree(
    cx: &mut Cx<'_>,
    settings: &Settings,
    chosen: usize,
    refused: &BTreeMap<String, String>,
) -> Node {
    let part = match settings.sections.get(chosen) {
        Some(section) => section_part(cx, section, refused),
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
/// holds, and its keys in a group, each write refused of why, `refused`.
fn section_part(
    cx: &mut Cx<'_>,
    section: &SettingsSection,
    refused: &BTreeMap<String, String>,
) -> Vec<Node> {
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
    let rows = section
        .keys
        .iter()
        .map(|key| key_row(cx, key, refused.get(&key.name)))
        .collect();
    part.push(group(rows, "keys"));
    part
}

/// The row of `key`: its name, its platforms and what it does, why its last
/// write was refused, `refused`, and at its right the control of its value
/// in force, marked as its default where it is, or that it is not set, and
/// the button that takes it back to its default where it is another.
fn key_row(cx: &mut Cx<'_>, key: &SettingKey, refused: Option<&String>) -> Node {
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
    if let Some(why) = refused {
        about.push(
            Node::text(why.clone())
                .key(format!("refused:{name}").as_str())
                .style(Style {
                    color: Some(Token::from("progress.error")),
                    text_size: Some(TextSize::Small),
                    ..Style::default()
                })
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
    let mut value = vec![control(cx, key)];
    let mut marks = Vec::new();
    if key.value.is_some() && key.value == key.default {
        marks.push(
            Node::text(message(cx, "setting-default", &[]))
                .key(format!("default:{name}").as_str())
                .style(small()),
        );
    } else if key.value.is_some() && writable(&key.kind) {
        marks.push(
            Node::button(message(cx, "setting-reset", &[])).key(format!("{RESET}{name}").as_str()),
        );
    }
    if !marks.is_empty() {
        value.push(row_of(marks, 6.0));
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

/// Whether the page writes a key of `kind`: a boolean, a value of its
/// choices, a number or a text; a list and another value it shows alone.
fn writable(kind: &SettingKind) -> bool {
    !matches!(kind, SettingKind::List { .. } | SettingKind::Other)
}

/// The control of `key`'s value: a toggle of a boolean, the button of a
/// menu of its values, a field of a number, between the buttons of its
/// steps, or of a text; a list's or another value's text, as the config
/// writes it.
fn control(cx: &mut Cx<'_>, key: &SettingKey) -> Node {
    let name = &key.name;
    let terminal = Style {
        font_family: Some(FontFamily::Terminal),
        ..Style::default()
    };
    match &key.kind {
        SettingKind::Boolean => {
            let on = key.value == Some(Data::Boolean(true));
            let label = message(cx, if on { "setting-on" } else { "setting-off" }, &[]);
            let mut toggle = Node::toggle("", on)
                .key(format!("{SET}{name}").as_str())
                .label(name.clone());
            if let Kind::Toggle(own) = &mut toggle.kind {
                own.label = label;
            }
            toggle
        }
        SettingKind::Choice { .. } => {
            let shown = key
                .value
                .as_ref()
                .map_or_else(|| message(cx, "setting-unset", &[]), plain);
            let mut button = Node::button(shown)
                .key(format!("{CHOOSE}{name}").as_str())
                .label(name.clone());
            if let Kind::Button(own) = &mut button.kind {
                "chevron-down".clone_into(&mut own.icon);
            }
            button
        }
        SettingKind::Number { step, .. } => {
            let text = key.value.as_ref().map(plain).unwrap_or_default();
            let field = Node::field(message(cx, "setting-unset", &[]))
                .key(format!("{FIELD}{name}").as_str())
                .label(name.clone())
                .holding(text)
                .style(terminal)
                .layout(Layout {
                    width: Length::Points(90.0),
                    ..Layout::default()
                });
            if step.is_none() {
                return field;
            }
            row_of(
                vec![
                    Node::button(message(cx, "setting-less", &[]))
                        .key(format!("{LESS}{name}").as_str()),
                    field,
                    Node::button(message(cx, "setting-more", &[]))
                        .key(format!("{MORE}{name}").as_str()),
                ],
                4.0,
            )
        }
        SettingKind::Text { examples, .. } => {
            let text = key.value.as_ref().map(plain).unwrap_or_default();
            let placeholder = examples
                .first()
                .cloned()
                .unwrap_or_else(|| message(cx, "setting-unset", &[]));
            Node::field(placeholder)
                .key(format!("{FIELD}{name}").as_str())
                .label(name.clone())
                .holding(text)
                .style(terminal)
                .layout(Layout {
                    width: Length::Points(220.0),
                    ..Layout::default()
                })
        }
        SettingKind::List { .. } | SettingKind::Other => match &key.value {
            Some(value) => Node::text(shown(value))
                .key(format!("value:{name}").as_str())
                .style(terminal),
            None => Node::text(message(cx, "setting-unset", &[]))
                .key(format!("value:{name}").as_str())
                .style(secondary()),
        },
    }
}

/// `value` as a person writes it in a field or reads it in a menu: a text
/// of no quotes, a number as it is.
fn plain(value: &Data) -> String {
    match value {
        Data::Text(text) => text.clone(),
        other => shown(other),
    }
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

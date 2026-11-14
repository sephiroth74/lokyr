//! Built-in plugin of the settings window's first page, Settings: a sidebar
//! of the config's sections, as its schema has them, then of the plugins'
//! of a config schema, and beside it the chosen section's keys, each of
//! its name, its platforms, what it does and a control of its value in
//! force, marked as its default where it is: a toggle of a boolean, a menu
//! of a key's values, of the system's font families, of a family's
//! styles, of the themes or of the theme's colors, a field of a number,
//! which the arrow keys step, or of a text, written as typing pauses, as
//! Return is pressed and as it is left, and a link that takes the key back
//! to its default. A part of the bindings of keys lists the config's,
//! each of its command's title, to take away, and adds one. A change
//! writes the key into the config file, the rest of the file and its
//! comments kept, and a value that the key does not take tells why under
//! it; a file that does not read shows its error, and the page writes
//! nothing until it reads. The page reads the settings as it renders, of
//! the capability `config`, which Lokyr's own plugins alone may hold, and
//! follows the config as it applies anew, of a change of the file too.
//! Like every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::host::{SettingBinding, SettingKey, SettingKind, Settings, SettingsSection};
use lokyr_plugin_sdk::message::{Data, Event, SETTINGS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::theme::{TOKENS, ThemeVariant};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontFamily, FontWeight, Justify, Kind, Layout, Length,
    ListItem, Node, Role, Style, TextSize, Token, UiEventKind,
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

/// The prefix of the key of a key's button back to its default.
const RESET: &str = "reset:";

/// The prefix of the key of a binding's button that takes it away.
const UNBIND: &str = "unbind:";

/// The key of the field of a new binding's keys.
const NEW_KEYS: &str = "new-keys";

/// The key of the field of a new binding's command.
const NEW_COMMAND: &str = "new-command";

/// The section of the bindings of keys, as the config names it.
const KEYS: &str = "keys";

/// How long typing pauses before a field's value is written.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(600);

/// The plugin: whether its page shows, the part chosen, by its place in the
/// sidebar, why each key's last write was refused, by its name, until it is
/// written or another part shows, the new binding's fields' texts, each of
/// its revision, the texts typed into the fields and not written yet, each
/// of its key's section and plugin, by the key's name, and the timer of
/// typing's pause.
#[derive(Default)]
pub struct SettingsPage {
    shown: bool,
    chosen: usize,
    refused: BTreeMap<String, String>,
    new_binding: BTreeMap<String, (String, u64)>,
    typed: BTreeMap<String, Typed>,
    pause: Option<u64>,
}

/// A text typed into a key's field and not written yet: the key's section
/// and plugin, and the text.
#[derive(Clone, Debug)]
struct Typed {
    section: String,
    plugin: Option<String>,
    text: String,
}

/// A part of the page, as its sidebar lists it: a section of the settings,
/// or the bindings of keys, after the app's sections.
#[derive(Clone, Copy)]
enum Part<'s> {
    Section(&'s SettingsSection),
    Keys,
}

/// The parts of `settings`, in the sidebar's order: the app's sections, the
/// bindings of keys, then the plugins' sections.
fn parts(settings: &Settings) -> Vec<Part<'_>> {
    let (app, plugins): (Vec<&SettingsSection>, Vec<&SettingsSection>) = settings
        .sections
        .iter()
        .partition(|section| section.plugin.is_none());
    app.into_iter()
        .map(Part::Section)
        .chain(std::iter::once(Part::Keys))
        .chain(plugins.into_iter().map(Part::Section))
        .collect()
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
                Event::PageHidden(_) => {
                    self.shown = false;
                    self.write_typed(cx, None)?;
                }
                Event::SettingsChanged => again = true,
                // Typing paused: what it typed is written, of a value whole.
                Event::Timer { id } if self.pause == Some(*id) => {
                    self.pause = None;
                    self.write_paused(cx)?;
                }
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
            // What the part shown typed is written before another shows.
            self.write_typed(cx, None)?;
            self.chosen = *item;
            self.refused.clear();
            return cx.request_render(page());
        }
        let settings = cx.settings()?;
        let section = match parts(&settings).get(self.chosen).copied() {
            Some(Part::Section(section)) => section,
            Some(Part::Keys) => return self.binding_event(cx, event),
            None => return Ok(()),
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
            let Some(values) = menu_values(cx, section, setting) else {
                return Ok(());
            };
            if matches!(event.kind, UiEventKind::Click | UiEventKind::Toggle { .. })
                && let Some(value) = values.get(at)
            {
                return self.write(cx, section, setting, Some(value.clone()));
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
            // Typed: written as typing pauses, as Return is pressed, or as
            // the field is left.
            (UiEventKind::InputChanged { text, .. }, FIELD) => {
                self.typed.insert(
                    name.to_owned(),
                    Typed {
                        section: section.name.clone(),
                        plugin: section.plugin.clone(),
                        text: text.clone(),
                    },
                );
                if let Some(pause) = self.pause.take() {
                    cx.cancel_timer(pause)?;
                }
                self.pause = Some(cx.start_timer(PAUSE, false)?);
                Ok(())
            }
            (UiEventKind::Submit { text, .. }, FIELD) => {
                self.typed.insert(
                    name.to_owned(),
                    Typed {
                        section: section.name.clone(),
                        plugin: section.plugin.clone(),
                        text: text.clone(),
                    },
                );
                self.write_typed(cx, Some(name))
            }
            (UiEventKind::Blur, FIELD) => self.write_typed(cx, Some(name)),
            (UiEventKind::Step { up }, FIELD) => {
                let toward = if *up { 1.0 } else { -1.0 };
                self.typed.remove(name);
                match step_of(setting, toward) {
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
            let Part::Section(section) = *parts(&settings).get(self.chosen)? else {
                return None;
            };
            let setting = find(section, name)?;
            let choices = menu_values(cx, section, setting)?;
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
    /// An event of the part of the bindings of keys: a binding taken away,
    /// or the new one's fields typed in, and written as Return is pressed
    /// in either, both filled.
    fn binding_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        match &event.kind {
            UiEventKind::Click => {
                let Some(keys) = key.strip_prefix(UNBIND) else {
                    return Ok(());
                };
                let written = cx.set_setting(KEYS, None, keys, None);
                self.told(cx, KEYS, written)
            }
            UiEventKind::InputChanged { text, revision }
                if [NEW_KEYS, NEW_COMMAND].contains(&key) =>
            {
                self.new_binding
                    .insert(key.to_owned(), (text.clone(), *revision));
                Ok(())
            }
            UiEventKind::Submit { text, .. } if [NEW_KEYS, NEW_COMMAND].contains(&key) => {
                let revision = self
                    .new_binding
                    .get(key)
                    .map_or(0, |(_, revision)| *revision);
                self.new_binding
                    .insert(key.to_owned(), (text.clone(), revision));
                let given = |field: &str| {
                    self.new_binding
                        .get(field)
                        .map(|(text, _)| text.trim().to_owned())
                        .filter(|text| !text.is_empty())
                };
                let (Some(keys), Some(command)) = (given(NEW_KEYS), given(NEW_COMMAND)) else {
                    return Ok(());
                };
                let written = cx.set_setting(KEYS, None, &keys, Some(Data::Text(command)));
                let fine = written.is_ok();
                self.told(cx, KEYS, written)?;
                // Written, its fields empty again, for the next.
                if fine {
                    for field in [NEW_KEYS, NEW_COMMAND] {
                        let revision = self
                            .new_binding
                            .remove(field)
                            .map_or(0, |(_, revision)| revision);
                        cx.set_text(page(), field, "", revision)?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Takes what a write of the key `name` did, `written`: why it was
    /// refused, shown until a write of it goes, the page rendered again.
    fn told(
        &mut self,
        cx: &mut Cx<'_>,
        name: &str,
        written: Result<(), Error>,
    ) -> Result<(), Error> {
        let changed = match written {
            Ok(()) => self.refused.remove(name).is_some(),
            Err(error) => {
                self.refused.insert(name.to_owned(), error.message);
                true
            }
        };
        if changed {
            cx.request_render(page())?;
        }
        Ok(())
    }
}

impl SettingsPage {
    /// Writes the texts typed into the fields and not written yet as typing
    /// pauses: each a value whole, as it writes it, that its key takes and
    /// that differs from its value in force; the others, an empty field or
    /// one half typed, as `13.` on the way to `13.5`, wait for Return or the
    /// field's leaving, of no reason told meanwhile.
    fn write_paused(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let settings = cx.settings()?;
        let names: Vec<String> = self.typed.keys().cloned().collect();
        for name in names {
            let Some(typed) = self.typed.get(&name).cloned() else {
                continue;
            };
            let Some(section) = settings
                .sections
                .iter()
                .find(|section| section.name == typed.section && section.plugin == typed.plugin)
            else {
                continue;
            };
            let Some(setting) = find(section, &name) else {
                continue;
            };
            let text = typed.text.as_str();
            let now = setting.value.as_ref().map(plain).unwrap_or_default();
            let Ok(Some(value)) = value_of(cx, &setting.kind, text) else {
                continue;
            };
            if plain(&value) != text || text == now {
                continue;
            }
            let written = cx.set_setting(
                &section.name,
                section.plugin.as_deref(),
                &setting.name,
                Some(value),
            );
            if written.is_ok() {
                self.typed.remove(&name);
                if self.refused.remove(&name).is_some() {
                    cx.request_render(page())?;
                }
            }
        }
        Ok(())
    }

    /// Writes the texts typed into the fields and not written yet, of the
    /// key `only` alone where given, each that differs from its key's value
    /// in force: a value of its key's kind, or why not under it.
    fn write_typed(&mut self, cx: &mut Cx<'_>, only: Option<&str>) -> Result<(), Error> {
        let names: Vec<String> = match only {
            Some(name) => vec![name.to_owned()],
            None => self.typed.keys().cloned().collect(),
        };
        let mut pending = Vec::new();
        for name in names {
            if let Some(typed) = self.typed.remove(&name) {
                pending.push((name, typed));
            }
        }
        if self.typed.is_empty()
            && let Some(pause) = self.pause.take()
        {
            cx.cancel_timer(pause)?;
        }
        if pending.is_empty() {
            return Ok(());
        }
        let settings = cx.settings()?;
        for (name, typed) in pending {
            let Some(section) = settings
                .sections
                .iter()
                .find(|section| section.name == typed.section && section.plugin == typed.plugin)
            else {
                continue;
            };
            let Some(setting) = find(section, &name) else {
                continue;
            };
            // Once of a value: none of the value in force.
            let text = typed.text.trim();
            let now = setting.value.as_ref().map(plain).unwrap_or_default();
            if text == now || (text.is_empty() && setting.value == setting.default) {
                if self.refused.remove(&name).is_some() {
                    cx.request_render(page())?;
                }
                continue;
            }
            match value_of(cx, &setting.kind, text) {
                Ok(value) => self.write(cx, section, setting, value)?,
                Err(why) => {
                    self.refused.insert(name, why);
                    cx.request_render(page())?;
                }
            }
        }
        Ok(())
    }

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
        self.told(cx, &setting.name, written)
    }
}

/// The key `name` of `section`, if it has one.
fn find<'s>(section: &'s SettingsSection, name: &str) -> Option<&'s SettingKey> {
    section.keys.iter().find(|key| key.name == name)
}

/// The values of the menu of `key` of `section`, if a menu shows them: a
/// choice's; of its control, the system's monospaced font families or
/// every one, the styles of the section's family, as it names it or as it
/// is in force, the themes of the key's appearance, or the theme's colors.
fn menu_values(cx: &mut Cx<'_>, section: &SettingsSection, key: &SettingKey) -> Option<Vec<Data>> {
    if let SettingKind::Choice { choices } = &key.kind {
        return Some(choices.clone());
    }
    let texts: Vec<String> = match key.control.as_deref()? {
        "monospaced-family" => cx.fonts().ok()?.families,
        "font-family" => cx.fonts().ok()?.all,
        "font-style" => {
            let family = match find(section, "family").and_then(|family| family.value.as_ref()) {
                Some(Data::Text(family)) => family.clone(),
                _ => cx.fonts().ok()?.current,
            };
            cx.font_styles(&family).ok()?
        }
        "theme" => {
            let variant = match key.name.as_str() {
                "light" => ThemeVariant::Light,
                _ => ThemeVariant::Dark,
            };
            let mut names: Vec<String> = cx
                .themes()
                .ok()?
                .themes
                .into_iter()
                .filter(|theme| theme.variant == variant)
                .map(|theme| theme.name)
                .collect();
            names.dedup();
            names
        }
        "token" => TOKENS.iter().map(|token| (*token).to_owned()).collect(),
        _ => return None,
    };
    Some(texts.into_iter().map(Data::Text).collect())
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
/// least, moved by its step, 1 of none; none of a key of another kind.
fn step_of(setting: &SettingKey, toward: f64) -> Option<Data> {
    let SettingKind::Number {
        minimum,
        maximum,
        step,
        ..
    } = setting.kind
    else {
        return None;
    };
    let step = step.unwrap_or(1.0);
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
    let mut part = match parts(settings).get(chosen).copied() {
        Some(Part::Section(section)) => section_part(cx, section, refused),
        Some(Part::Keys) => keys_part(cx, &settings.keys, refused),
        None => vec![
            Node::text(message(cx, "settings-none", &[]))
                .key("none")
                .style(secondary()),
        ],
    };
    // A file that does not read: its error, over every part.
    if let Some(problem) = &settings.error {
        let text = match (problem.line, problem.column) {
            (Some(line), Some(column)) => message(
                cx,
                "settings-file-error-at",
                &[
                    ("line", line.to_string()),
                    ("column", column.to_string()),
                    ("message", problem.message.clone()),
                ],
            ),
            _ => message(
                cx,
                "settings-file-error",
                &[("message", problem.message.clone())],
            ),
        };
        part.insert(
            0,
            Node::text(text)
                .key("file-error")
                .role(Role::Group)
                .style(Style {
                    color: Some(Token::from("text.warning")),
                    background: Some(Token::from("panel.bg")),
                    border: Some(Border {
                        color: Token::from("text.warning"),
                        width: 1.0,
                    }),
                    radius: 8.0,
                    ..Style::default()
                })
                .layout(Layout {
                    lines: Some(0),
                    padding: Edges::all(10.0),
                    width: Length::Fraction(1.0),
                    ..Layout::default()
                }),
        );
    }
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
    let parts = parts(settings);
    let items: Vec<ListItem> = parts
        .iter()
        .map(|part| match part {
            Part::Section(section) => {
                let icon = if section.plugin.is_some() {
                    "extensions"
                } else {
                    "settings"
                };
                ListItem {
                    texts: vec![icon.to_owned(), title(cx, section)],
                }
            }
            Part::Keys => ListItem {
                texts: vec!["terminal".to_owned(), message(cx, "section-keys", &[])],
            },
        })
        .collect();
    let mut list = Node::list(row, items.len(), (0, items))
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
    // At the bottom right: the link back to the default, dimmed where the
    // key is there already.
    let mut marks = Vec::new();
    let at_default = key.value.is_none() || key.value == key.default;
    if writable(&key.kind) {
        let mut reset = Node::button(message(cx, "setting-reset", &[]))
            .key(format!("{RESET}{name}").as_str())
            .as_link()
            .style(Style {
                text_size: Some(TextSize::Small),
                ..Style::default()
            });
        if let Kind::Button(own) = &mut reset.kind {
            own.disabled = at_default;
        }
        marks.push(reset);
    }
    if !marks.is_empty() {
        value.push(row_of(marks, 6.0).key(format!("marks:{name}").as_str()));
    }
    let value = Node::container(value)
        .key(format!("values:{name}").as_str())
        .layout(Layout {
            direction: Direction::Column,
            gap: 6.0,
            align: Align::End,
            justify: Justify::SpaceBetween,
            ..Layout::default()
        });
    // The control at the top right, the link back to the default at the
    // bottom right, of the row's height.
    Node::container(vec![about, value])
        .key(format!("key:{name}").as_str())
        .role(Role::Group)
        .label(name.clone())
        .layout(Layout {
            direction: Direction::Row,
            gap: 12.0,
            align: Align::Stretch,
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
    let terminal = code();
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
        SettingKind::Choice { .. } => menu_button(cx, key),
        SettingKind::Text { .. } if menu_control(key) => {
            let button = menu_button(cx, key);
            // A theme's color, of a swatch of it beside its name.
            match (key.control.as_deref(), &key.value) {
                (Some("token"), Some(Data::Text(token))) => row_of(
                    vec![
                        Node::container(Vec::new())
                            .key(format!("swatch:{name}").as_str())
                            .layout(Layout {
                                width: Length::Points(14.0),
                                height: Length::Points(14.0),
                                ..Layout::default()
                            })
                            .style(Style {
                                background: Some(Token::from(token.as_str())),
                                border: Some(Border {
                                    color: Token::from("separator"),
                                    width: 1.0,
                                }),
                                radius: 3.0,
                                ..Style::default()
                            }),
                        button,
                    ],
                    6.0,
                ),
                _ => button,
            }
        }
        SettingKind::Number { .. } => {
            let text = key.value.as_ref().map(plain).unwrap_or_default();
            Node::field(message(cx, "setting-unset", &[]))
                .key(format!("{FIELD}{name}").as_str())
                .label(name.clone())
                .holding(text)
                .stepped()
                .style(field_text())
                .layout(Layout {
                    width: Length::Points(64.0),
                    ..Layout::default()
                })
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
                .style(field_text())
                .layout(Layout {
                    width: Length::Points(180.0),
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

/// Whether `key` of a text shows its values in a menu, as its control
/// tells: a font's family or style, a theme, or a theme's color.
fn menu_control(key: &SettingKey) -> bool {
    matches!(
        key.control.as_deref(),
        Some("monospaced-family" | "font-family" | "font-style" | "theme" | "token")
    )
}

/// The button of `key`'s menu of values, of its value in force.
fn menu_button(cx: &mut Cx<'_>, key: &SettingKey) -> Node {
    let shown = key
        .value
        .as_ref()
        .map_or_else(|| message(cx, "setting-unset", &[]), plain);
    let mut button = Node::button(shown)
        .key(format!("{CHOOSE}{}", key.name).as_str())
        .label(key.name.clone());
    if let Kind::Button(own) = &mut button.kind {
        "chevron-down".clone_into(&mut own.icon);
    }
    button
}

/// The part of the bindings of keys: what it holds, each binding of
/// `bindings` of its keys, its command's title and name, or that it takes
/// nothing, and its button that takes it away, and the fields of a new
/// one, of why its last write was refused, `refused`.
fn keys_part(
    cx: &mut Cx<'_>,
    bindings: &[SettingBinding],
    refused: &BTreeMap<String, String>,
) -> Vec<Node> {
    let mut part = vec![
        Node::text(message(cx, "section-keys", &[]))
            .key("title")
            .role(Role::Heading)
            .style(Style {
                text_size: Some(TextSize::Large),
                font_weight: Some(FontWeight::Bold),
                ..Style::default()
            }),
        Node::text(message(cx, "keys-description", &[]))
            .key("description")
            .style(secondary())
            .layout(wrapped()),
    ];
    let terminal = code();
    let rows: Vec<Node> = bindings
        .iter()
        .map(|binding| {
            let keys = &binding.keys;
            let does = match (&binding.command, &binding.title) {
                (Some(_), Some(title)) => title.clone(),
                (Some(command), None) => command.clone(),
                (None, _) => message(cx, "binding-nothing", &[]),
            };
            let mut about = vec![
                Node::text(keys.clone())
                    .key(format!("keys:{keys}").as_str())
                    .style(terminal.clone()),
                Node::text(does)
                    .key(format!("command:{keys}").as_str())
                    .layout(wrapped()),
            ];
            if let (Some(command), Some(_)) = (&binding.command, &binding.title) {
                about.push(Node::text(command.clone()).style(small()));
            }
            if binding.detailed {
                about.push(
                    Node::text(message(cx, "binding-detailed", &[]))
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
            Node::container(vec![
                about,
                Node::button(message(cx, "binding-remove", &[]))
                    .key(format!("{UNBIND}{keys}").as_str()),
            ])
            .key(format!("binding:{keys}").as_str())
            .role(Role::Group)
            .label(keys.clone())
            .layout(row_layout())
        })
        .collect();
    if rows.is_empty() {
        part.push(
            Node::text(message(cx, "bindings-none", &[]))
                .key("no-bindings")
                .style(secondary())
                .layout(wrapped()),
        );
    } else {
        part.push(group(rows, "bindings"));
    }
    // A new binding: its keys and its command, written as Return is
    // pressed.
    part.push(
        Node::container(vec![
            Node::field("cmd+shift+n")
                .key(NEW_KEYS)
                .label(message(cx, "binding-new-keys", &[]))
                .style(terminal.clone())
                .layout(Layout {
                    width: Length::Points(160.0),
                    ..Layout::default()
                }),
            Node::field("window.new")
                .key(NEW_COMMAND)
                .label(message(cx, "binding-new-command", &[]))
                .style(terminal)
                .layout(Layout {
                    grow: 1.0,
                    min_width: Length::Points(0.0),
                    ..Layout::default()
                }),
        ])
        .key("new-binding")
        .layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        }),
    );
    part.push(
        Node::text(message(cx, "binding-new", &[]))
            .key("new-binding-help")
            .style(small())
            .layout(wrapped()),
    );
    if let Some(why) = refused.get(KEYS) {
        part.push(
            Node::text(why.clone())
                .key("refused:keys")
                .style(Style {
                    color: Some(Token::from("progress.error")),
                    text_size: Some(TextSize::Small),
                    ..Style::default()
                })
                .layout(wrapped()),
        );
    }
    part
}

/// The layout of a row of a group: its texts at its left, growing, and its
/// controls at its right.
fn row_layout() -> Layout {
    Layout {
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

/// The style of a field's text: the interface's, small, whatever the
/// terminal's font's size.
fn field_text() -> Style {
    Style {
        text_size: Some(TextSize::Small),
        ..Style::default()
    }
}

/// The style of a value written as the config writes it: the terminal's
/// family, at a size of its own, whatever the terminal's.
fn code() -> Style {
    Style {
        font_family: Some(FontFamily::Terminal),
        font_size: Some(12.0),
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

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

use lokyr_plugin_sdk::command::{Category, Error, Value};
use lokyr_plugin_sdk::host::{
    FontFace, SettingBinding, SettingKey, SettingKind, Settings, SettingsSection,
};
use lokyr_plugin_sdk::message::{Data, Event, SETTINGS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::theme::{TOKENS, ThemeVariant};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontFamily, FontWeight, Hover, Justify, Kind, Layout, Length,
    ListItem, Node, PointerShape, Role, Style, TextSize, Token, UiEventKind,
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

/// The prefix of the key of a binding's keys, by its place among the
/// settings' bindings, whose click lets them be changed.
const BINDING_KEYS: &str = "keys:";

/// The prefix of the key of the field of a binding's keys, as they change.
const KEYS_FIELD: &str = "keys-field:";

/// The prefix of the key of the link that gives a default back, of keys
/// that the config takes away.
const RESTORE: &str = "restore:";

/// The keys of the buttons that open the panel of a new binding, above
/// the bindings and below them.
const ADD_BINDING: &str = "add-binding";
const ADD_BINDING_BELOW: &str = "add-binding-below";

/// The panel of a new binding, and its menu of the commands.
const NEW_BINDING: &str = "new-binding";
const COMMANDS: &str = "commands";

/// The keys of the new binding's panel's nodes: its recorder of keys, its
/// button of the commands' menu, Cancel and Add.
const RECORDER: &str = "recorder";
const CHOOSE_COMMAND: &str = "choose-command";
const CANCEL: &str = "cancel";
const CONFIRM: &str = "confirm";

/// The prefix of the key of an item of the commands' menu, of its
/// command's name.
const COMMAND_ITEM: &str = "command-item:";

/// The key of the new binding's panel's refusal, which the page's
/// refusals keep.
const NEW_REFUSED: &str = "new";

/// The section of the bindings of keys, as the config names it.
const KEYS: &str = "keys";

/// The prefix of the key of a key's link to its text in the guide.
const GUIDE: &str = "guide:";

/// The prefix of the key of a list's value's button that takes it away,
/// `remove:<key>:<index>`.
const REMOVE: &str = "remove:";

/// The prefix of the key of a list's field that adds a value.
const ADD: &str = "add:";

/// The key of the sidebar's search field.
const SEARCH: &str = "search";

/// The key of the search field's button that empties it.
const SEARCH_CLEAR: &str = "search-clear";

/// The side of the search field's magnifying glass and of its button, in
/// points.
const IN_SEARCH: f32 = 18.0;

/// The platforms that Lokyr runs on, of which a key that applies on every
/// one names none.
const PLATFORMS: &[&str] = &["macos"];

/// How long typing pauses before a field's value is written.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(600);

/// A new binding as its panel makes it: its keys, as the recorder took
/// them and as a config writes them, and its command, by its name.
#[derive(Clone, Debug, Default)]
struct Adding {
    keys: Option<String>,
    command: Option<String>,
}

/// The plugin: whether its page shows, the part chosen, by its place among
/// the parts, the search's words, why each key's last write was refused, by
/// its name, until it is written or another part shows, the binding whose
/// keys change, the new binding that Add's panel makes, the texts typed
/// into the fields and not written yet, each of its key's section and
/// plugin, by the key's name, and the timer of typing's pause.
#[derive(Default)]
pub struct SettingsPage {
    shown: bool,
    chosen: usize,
    query: String,
    refused: BTreeMap<String, String>,
    /// The binding whose keys change, by its place among the settings'
    /// bindings, whose field records them.
    editing: Option<usize>,
    /// The new binding that Add's panel makes, while it shows.
    adding: Option<Adding>,
    /// Whether a font's family or style shows in the panes as a menu's
    /// item is highlighted, until the menu closes.
    previewing: bool,
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
                // The new binding's panel closed, by Escape, Cancel or Add.
                Event::PanelClosed(PanelId(panel)) if panel == NEW_BINDING => {
                    self.adding = None;
                    self.refused.remove(NEW_REFUSED);
                }
                // A menu of values closed: the font that it showed goes,
                // the one in force back, or the one chosen.
                Event::PanelClosed(PanelId(panel))
                    if panel.starts_with(CHOICES) && self.previewing =>
                {
                    self.previewing = false;
                    cx.preview_font(None)?;
                }
                // The bindings' places change with the settings: a change of
                // keys under way ends.
                Event::SettingsChanged => {
                    self.editing = None;
                    again = true;
                }
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
        if let ViewId::Panel(PanelId(panel)) = &event.view
            && [NEW_BINDING, COMMANDS].contains(&panel.as_str())
        {
            return self.adding_event(cx, event);
        }
        // The search's words, which filter the parts and their keys.
        match (&event.kind, key) {
            (UiEventKind::InputChanged { text, .. }, SEARCH) => {
                return self.search(cx, text);
            }
            (UiEventKind::Click, SEARCH_CLEAR) => {
                cx.set_text(page(), SEARCH, "", u64::MAX)?;
                return self.search(cx, "");
            }
            _ => {}
        }
        if let (UiEventKind::Select { item } | UiEventKind::Activate { item }, SECTIONS) =
            (&event.kind, key)
        {
            // The sidebar's item, of the parts that the search shows.
            let settings = cx.settings()?;
            let words = words(&self.query);
            let shown = shown_parts(cx, &settings, &words);
            let Some(&item) = shown.get(*item) else {
                return Ok(());
            };
            if item == self.chosen {
                return Ok(());
            }
            // What the part shown typed is written before another shows.
            self.write_typed(cx, None)?;
            self.chosen = item;
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
            // A font's family or a face's style shown in the panes as its
            // item is highlighted.
            if event.kind == UiEventKind::HoverEnter
                && let Some(Data::Text(value)) = values.get(at)
                && let Some(preview) = font_preview(section, setting)
            {
                match preview {
                    FontPreview::Family => cx.preview_font(Some(value))?,
                    FontPreview::Style(face) => cx.preview_font_style(face, value)?,
                }
                self.previewing = true;
            }
            return Ok(());
        }
        // A list's value taken away, or one added.
        if let (UiEventKind::Click, Some(rest)) = (&event.kind, key.strip_prefix(REMOVE)) {
            let Some((name, at)) = rest.rsplit_once(':') else {
                return Ok(());
            };
            let (Some(setting), Ok(at)) = (find(section, name), at.parse::<usize>()) else {
                return Ok(());
            };
            let mut texts = list_texts(setting.value.as_ref());
            if at >= texts.len() {
                return Ok(());
            }
            texts.remove(at);
            let value = Data::List(texts.into_iter().map(Data::Text).collect());
            return self.write(cx, section, setting, Some(value));
        }
        if let (UiEventKind::Submit { text, .. }, Some(name)) = (&event.kind, key.strip_prefix(ADD))
        {
            let Some(setting) = find(section, name) else {
                return Ok(());
            };
            let added = text.trim();
            let mut texts = list_texts(setting.value.as_ref());
            if added.is_empty() || texts.iter().any(|text| text == added) {
                return Ok(());
            }
            texts.push(added.to_owned());
            let value = Data::List(texts.into_iter().map(Data::Text).collect());
            let written = cx.set_setting(
                &section.name,
                section.plugin.as_deref(),
                &setting.name,
                Some(value),
            );
            let fine = written.is_ok();
            self.told(cx, &setting.name, written)?;
            // Added, its field empty again, for the next.
            if fine {
                cx.set_text(page(), key, "", u64::MAX)?;
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
            (UiEventKind::Click, GUIDE) => match &setting.guide {
                Some(address) => cx.open(address),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let settings = cx.settings().ok()?;
        match view {
            ViewId::Panel(PanelId(panel)) if panel == NEW_BINDING => {
                let adding = self.adding.clone()?;
                return Some(adding_tree(cx, &settings, &adding, &self.refused));
            }
            ViewId::Panel(PanelId(panel)) if panel == COMMANDS => {
                return Some(commands_menu(cx));
            }
            _ => {}
        }
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
        Some(tree(
            cx,
            &settings,
            (self.chosen, self.editing),
            &words(&self.query),
            &self.refused,
        ))
    }
}

impl SettingsPage {
    /// Takes the search's text `text`: the parts that hold its words show,
    /// the part chosen the first of them where it holds none, the sidebar's
    /// selection at it.
    fn search(&mut self, cx: &mut Cx<'_>, text: &str) -> Result<(), Error> {
        if text == self.query {
            return Ok(());
        }
        text.clone_into(&mut self.query);
        let settings = cx.settings()?;
        let shown = shown_parts(cx, &settings, &words(&self.query));
        if !shown.contains(&self.chosen)
            && let Some(&first) = shown.first()
        {
            self.write_typed(cx, None)?;
            self.chosen = first;
            self.refused.clear();
        }
        cx.request_render(page())?;
        if let Some(at) = shown.iter().position(|&part| part == self.chosen) {
            cx.select(page(), SECTIONS, at)?;
        }
        Ok(())
    }

    /// An event of the part of the bindings of keys: a binding's keys
    /// clicked, their field shown and focused, which records the keys
    /// pressed, written as it records them, the new keys of the command and
    /// `false` for a default's old ones, or a binding of the config's
    /// moved, and which its leaving puts away; a default taken away,
    /// `false`, a binding of the config's taken away, and keys that the
    /// config takes away given back; and Add, which opens the panel of a
    /// new binding.
    fn binding_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        let at = |prefix: &str| {
            key.strip_prefix(prefix)
                .and_then(|at| at.parse::<usize>().ok())
        };
        match &event.kind {
            // A dialog in the middle of the settings' window.
            UiEventKind::Click if [ADD_BINDING, ADD_BINDING_BELOW].contains(&key) => {
                self.adding = Some(Adding::default());
                self.refused.remove(NEW_REFUSED);
                cx.open_panel(
                    NEW_BINDING,
                    PanelOptions {
                        anchor: Anchor::ViewWindow {
                            view: page(),
                            corner: lokyr_plugin_sdk::ui::Corner::Center,
                        },
                        role: PanelRole::Dialog,
                        close_outside: false,
                        close_escape: true,
                        takes_keys: false,
                        close_typing: false,
                    },
                )
            }
            UiEventKind::Click => {
                if let Some(at) = at(BINDING_KEYS) {
                    self.editing = Some(at);
                    cx.request_render(page())?;
                    return cx.focus_node(page(), format!("{KEYS_FIELD}{at}").as_str());
                }
                let settings = cx.settings()?;
                if let Some(binding) = at(UNBIND).and_then(|at| settings.keys.get(at)) {
                    // A default's keys take nothing; the config's own go.
                    let value = binding.default.then_some(Data::Boolean(false));
                    let written = cx.set_setting(KEYS, None, &binding.keys, value);
                    return self.told(cx, KEYS, written);
                }
                if let Some(binding) = at(RESTORE).and_then(|at| settings.keys.get(at)) {
                    let written = cx.set_setting(KEYS, None, &binding.keys, None);
                    return self.told(cx, KEYS, written);
                }
                Ok(())
            }
            UiEventKind::Recorded { keys: Some(keys) } => match at(KEYS_FIELD) {
                Some(at) if self.editing == Some(at) => self.change_keys(cx, at, keys),
                _ => Ok(()),
            },
            UiEventKind::Blur if at(KEYS_FIELD).is_some() && self.editing == at(KEYS_FIELD) => {
                self.editing = None;
                self.refused
                    .remove(&format!("binding:{}", at(KEYS_FIELD).unwrap_or_default()));
                cx.request_render(page())
            }
            _ => Ok(()),
        }
    }

    /// An event of the panel of a new binding, or of its menu of the
    /// commands: the keys recorded, or taken away; the commands' menu
    /// opened, and a command chosen; Cancel, which closes the panel; and
    /// Add, which writes the binding of the keys and the command, and
    /// closes the panel, or tells why it was refused.
    fn adding_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        let Some(adding) = &mut self.adding else {
            return Ok(());
        };
        let panel = || ViewId::Panel(PanelId::from(NEW_BINDING));
        match &event.kind {
            UiEventKind::Recorded { keys } if key == RECORDER => {
                adding.keys.clone_from(keys);
                self.refused.remove(NEW_REFUSED);
                cx.request_render(panel())
            }
            UiEventKind::Click if key == CHOOSE_COMMAND => cx.open_panel(
                COMMANDS,
                PanelOptions {
                    anchor: Anchor::Node {
                        view: panel(),
                        key: CHOOSE_COMMAND.into(),
                    },
                    role: PanelRole::Menu,
                    close_outside: true,
                    close_escape: true,
                    takes_keys: false,
                    close_typing: false,
                },
            ),
            UiEventKind::Click | UiEventKind::Toggle { .. } if key.starts_with(COMMAND_ITEM) => {
                adding.command = key.strip_prefix(COMMAND_ITEM).map(str::to_owned);
                self.refused.remove(NEW_REFUSED);
                cx.request_render(panel())
            }
            UiEventKind::Click if key == CANCEL => cx.close_panel(NEW_BINDING),
            UiEventKind::Click if key == CONFIRM => {
                let (Some(keys), Some(command)) = (adding.keys.clone(), adding.command.clone())
                else {
                    return Ok(());
                };
                match cx.set_setting(KEYS, None, &keys, Some(Data::Text(command))) {
                    Ok(()) => cx.close_panel(NEW_BINDING),
                    Err(error) => {
                        self.refused.insert(NEW_REFUSED.to_owned(), error.message);
                        cx.request_render(panel())
                    }
                }
            }
            _ => Ok(()),
        }
    }

    /// Changes the keys of the binding at `at` among the settings' bindings
    /// to `text`, as the config writes keys: its command at the new keys,
    /// then `false` for a default's old keys, or the config's own taken
    /// away from them; the same keys, or none, change nothing, and keys
    /// that do not read are told under the row, their field staying.
    fn change_keys(&mut self, cx: &mut Cx<'_>, at: usize, text: &str) -> Result<(), Error> {
        let settings = cx.settings()?;
        let refusal = format!("binding:{at}");
        let (Some(binding), text) = (settings.keys.get(at), text.trim()) else {
            return Ok(());
        };
        let Some(command) = &binding.command else {
            return Ok(());
        };
        let same = lokyr_plugin_sdk::keys::parse_keys(text).ok()
            == lokyr_plugin_sdk::keys::parse_keys(&binding.keys).ok();
        if text.is_empty() || same {
            self.editing = None;
            self.refused.remove(&refusal);
            return cx.request_render(page());
        }
        // macOS's own keys, which no binding of the settings' takes.
        if let Ok(combos) = lokyr_plugin_sdk::keys::parse_keys(text)
            && lokyr_plugin_sdk::keys::is_system(&combos, None)
        {
            self.refused
                .insert(refusal, lokyr_plugin_sdk::keys::system_error(text));
            return cx.request_render(page());
        }
        if let Err(why) = lokyr_plugin_sdk::keys::parse_keys(text) {
            self.refused.insert(refusal, why);
            return cx.request_render(page());
        }
        let written = cx.set_setting(KEYS, None, text, Some(binding_value(binding, command)));
        if let Err(error) = written {
            self.refused.insert(refusal, error.message);
            return cx.request_render(page());
        }
        let old = binding.default.then_some(Data::Boolean(false));
        let written = cx.set_setting(KEYS, None, &binding.keys, old);
        self.editing = None;
        self.refused.remove(&refusal);
        self.told(cx, KEYS, written)?;
        cx.request_render(page())
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

/// What a menu of the terminal's font shows in the panes as its item is
/// highlighted: the family, or a face's style.
#[derive(Clone, Copy)]
enum FontPreview {
    Family,
    Style(FontFace),
}

/// The preview of the app's key `setting` of `section`, of the terminal's
/// font, as its menu's item is highlighted; `None` of a key of no preview.
fn font_preview(section: &SettingsSection, setting: &SettingKey) -> Option<FontPreview> {
    if section.name != "font" || section.plugin.is_some() {
        return None;
    }
    Some(match setting.name.as_str() {
        "family" => FontPreview::Family,
        "style" => FontPreview::Style(FontFace::Regular),
        "bold-style" => FontPreview::Style(FontFace::Bold),
        "italic-style" => FontPreview::Style(FontFace::Italic),
        "bold-italic-style" => FontPreview::Style(FontFace::BoldItalic),
        _ => return None,
    })
}

/// The search's words of `query`, in lower case.
fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether every one of `words` is in one of `texts`, their case aside.
fn holds(words: &[String], texts: &[&str]) -> bool {
    let texts: Vec<String> = texts.iter().map(|text| text.to_lowercase()).collect();
    words
        .iter()
        .all(|word| texts.iter().any(|text| text.contains(word.as_str())))
}

/// Whether the key `key` of `section` holds `words`, of its name, what it
/// does and its section's title.
fn key_holds(
    cx: &mut Cx<'_>,
    section: &SettingsSection,
    key: &SettingKey,
    words: &[String],
) -> bool {
    let title = title(cx, section);
    holds(words, &[&key.name, &key.description, &title])
}

/// Whether the binding `binding` holds `words`, of its keys, as the config
/// and as macOS write them, its command's name and title, and the part's
/// title.
fn binding_holds(cx: &mut Cx<'_>, binding: &SettingBinding, words: &[String]) -> bool {
    let shown = lokyr_plugin_sdk::keys::macos_text(&binding.keys).unwrap_or_default();
    let part = message(cx, "section-keys", &[]);
    let category = message(cx, category_message(binding.category), &[]);
    holds(
        words,
        &[
            &binding.keys,
            &shown,
            binding.command.as_deref().unwrap_or_default(),
            binding.title.as_deref().unwrap_or_default(),
            binding.when.as_deref().unwrap_or_default(),
            &category,
            &part,
        ],
    )
}

/// The commands' categories, in the order that the Keys part shows them,
/// then the bindings of none.
const CATEGORIES: [Option<Category>; 10] = [
    Some(Category::App),
    Some(Category::Windows),
    Some(Category::Tabs),
    Some(Category::Panes),
    Some(Category::Edit),
    Some(Category::Clipboard),
    Some(Category::Find),
    Some(Category::View),
    Some(Category::Input),
    None,
];

/// The message of the title of `category`, or of the bindings of none.
fn category_message(category: Option<Category>) -> &'static str {
    match category {
        Some(Category::App) => "category-app",
        Some(Category::Windows) => "category-windows",
        Some(Category::Tabs) => "category-tabs",
        Some(Category::Panes) => "category-panes",
        Some(Category::Input) => "category-input",
        Some(Category::Clipboard) => "category-clipboard",
        Some(Category::Edit) => "category-edit",
        Some(Category::Find) => "category-find",
        Some(Category::View) => "category-view",
        None => "category-other",
    }
}

/// The text of a command's arguments, `args`, as `index = 1, mode = "line"`.
fn args_text(args: &lokyr_plugin_sdk::command::Args) -> String {
    args.iter()
        .map(|(name, value)| match value {
            Value::String(text) => format!("{name} = {text:?}"),
            Value::Integer(number) => format!("{name} = {number}"),
            Value::Number(number) => format!("{name} = {number}"),
            Value::Boolean(on) => format!("{name} = {on}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The value that the config writes of `binding`'s command at other keys:
/// its name alone, or a table of its arguments and its context too.
fn binding_value(binding: &SettingBinding, command: &str) -> Data {
    if binding.args.is_empty() && binding.when.is_none() {
        return Data::Text(command.to_owned());
    }
    let mut table = BTreeMap::new();
    table.insert("command".to_owned(), Data::Text(command.to_owned()));
    if !binding.args.is_empty() {
        let args = binding
            .args
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    Value::String(text) => Data::Text(text.clone()),
                    Value::Integer(number) => Data::Integer(*number),
                    Value::Number(number) => Data::Number(*number),
                    Value::Boolean(on) => Data::Boolean(*on),
                };
                (name.clone(), value)
            })
            .collect();
        table.insert("args".to_owned(), Data::Map(args));
    }
    if let Some(when) = &binding.when {
        table.insert("when".to_owned(), Data::Text(when.clone()));
    }
    Data::Map(table)
}

/// The places among the parts of `settings` of those that the search's
/// `words` show: every one of no words, else each that holds them, of a
/// key or a binding of its own or of its title.
fn shown_parts(cx: &mut Cx<'_>, settings: &Settings, words: &[String]) -> Vec<usize> {
    parts(settings)
        .into_iter()
        .enumerate()
        .filter(|(_, part)| {
            words.is_empty()
                || match part {
                    Part::Section(section) => section
                        .keys
                        .iter()
                        .any(|key| key_holds(cx, section, key, words)),
                    Part::Keys => {
                        let title = message(cx, "section-keys", &[]);
                        holds(words, &[&title])
                            || settings
                                .keys
                                .iter()
                                .any(|binding| binding_holds(cx, binding, words))
                    }
                }
        })
        .map(|(at, _)| at)
        .collect()
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
    (chosen, editing): (usize, Option<usize>),
    words: &[String],
    refused: &BTreeMap<String, String>,
) -> Node {
    let shown = shown_parts(cx, settings, words);
    let chosen_part = parts(settings)
        .get(chosen)
        .copied()
        .filter(|_| shown.contains(&chosen));
    let mut part = match chosen_part {
        Some(Part::Section(section)) => section_part(cx, section, words, refused),
        Some(Part::Keys) => keys_part(cx, &settings.keys, words, editing, refused),
        None if !words.is_empty() => vec![
            Node::text(message(cx, "settings-no-match", &[]))
                .key("no-match")
                .style(secondary()),
        ],
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
    Node::container(vec![
        sidebar(cx, settings, &shown, words),
        Node::separator(),
        part,
    ])
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
fn sidebar(cx: &mut Cx<'_>, settings: &Settings, shown: &[usize], words: &[String]) -> Node {
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
    let items: Vec<ListItem> = shown
        .iter()
        .filter_map(|&at| parts.get(at))
        .map(|part| match part {
            Part::Section(section) => ListItem {
                texts: vec![icon_of(section).to_owned(), title(cx, section)],
            },
            Part::Keys => ListItem {
                texts: vec!["keyboard".to_owned(), message(cx, "section-keys", &[])],
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
    // The search at the sidebar's top, as macOS's search field: a box of
    // its magnifying glass, its text, of no box of its own, and, while it
    // holds words, a button at its end that empties it, the ring of its
    // text's focus round the whole box.
    let side = || Layout {
        width: Length::Points(IN_SEARCH),
        min_width: Length::Points(IN_SEARCH),
        height: Length::Points(IN_SEARCH),
        justify: Justify::Center,
        align: Align::Center,
        ..Layout::default()
    };
    let mut search = vec![
        Node::icon("search").layout(side()).style(Style {
            color: Some(Token::from("text.secondary")),
            ..Style::default()
        }),
        Node::field(message(cx, "settings-search", &[]))
            .key(SEARCH)
            .label(message(cx, "settings-search", &[]))
            .style(Style {
                background: Some(Token::from("transparent")),
                ..field_text()
            })
            .layout(Layout {
                grow: 1.0,
                width: Length::Points(0.0),
                min_width: Length::Points(0.0),
                ..Layout::default()
            }),
    ];
    if !words.is_empty() {
        let clear = message(cx, "settings-search-clear", &[]);
        search.push(
            Node::icon_button("close")
                .key(SEARCH_CLEAR)
                .label(clear.as_str())
                .tooltip(clear)
                .layout(side())
                .style(Style {
                    background: Some(Token::from("transparent")),
                    color: Some(Token::from("text.secondary")),
                    radius: 4.0,
                    // The arrow, over the field's I-beam around it.
                    pointer: Some(PointerShape::Arrow),
                    hover: Some(Hover {
                        background: Some(Token::from("button.hover.bg")),
                        ..Hover::default()
                    }),
                    pressed: Some(Hover {
                        background: Some(Token::from("button.pressed.bg")),
                        ..Hover::default()
                    }),
                    ..Style::default()
                }),
        );
    }
    let search = Node::container(search)
        .key("search-row")
        .layout(Layout {
            direction: Direction::Row,
            gap: 2.0,
            padding: Edges::all(2.0),
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("field.bg")),
            border: Some(Border {
                color: Token::from("field.border"),
                width: 1.0,
            }),
            radius: 6.0,
            holds_ring: true,
            pointer: Some(PointerShape::Text),
            ..Style::default()
        });
    Node::container(vec![search, list])
        .key("sidebar")
        .role(Role::Group)
        .layout(Layout {
            direction: Direction::Column,
            gap: 8.0,
            width: Length::Points(SIDEBAR),
            min_width: Length::Points(SIDEBAR),
            height: Length::Fraction(1.0),
            padding: Edges::all(10.0),
            ..Layout::default()
        })
}

/// The icon of `section` in the sidebar: one of its own, of the app's
/// sections and Lokyr's plugins', and the plugins' of others.
fn icon_of(section: &SettingsSection) -> &'static str {
    if section.plugin.is_some() {
        return match section.name.as_str() {
            "tabs" => "browser",
            "splits" => "split-horizontal",
            "quick-terminal" => "zap",
            "search" => "search",
            "command-header" => "pinned",
            "quick-tips" => "lightbulb",
            _ => "extensions",
        };
    }
    match section.name.as_str() {
        "font" => "text-size",
        "text" => "bold",
        "cursor" => "edit",
        "window" => "window",
        "interface" => "layout",
        "slots" => "layout-panel",
        "terminal" => "terminal",
        "scrollback" => "history",
        "keyboard" => "case-sensitive",
        "clipboard" => "clippy",
        "links" => "link",
        "bell" => "bell",
        "notifications" => "megaphone",
        "agents" => "hubot",
        "theme" => "symbol-color",
        "app" => "rocket",
        "updates" => "cloud-download",
        "debug" => "debug",
        _ => "settings",
    }
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
    words: &[String],
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
    // The keys that the search's words show, every one of none.
    let keys: Vec<&SettingKey> = section
        .keys
        .iter()
        .filter(|key| words.is_empty() || key_holds(cx, section, key, words))
        .collect();
    let rows = keys
        .into_iter()
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
    // Its platforms where it does not apply on every one that Lokyr runs on.
    let everywhere = PLATFORMS
        .iter()
        .all(|platform| key.platforms.iter().any(|own| own == platform));
    if !key.platforms.is_empty() && !everywhere {
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
    // Its text in the guide, which a browser opens.
    if key.guide.is_some() {
        about.push(row_of(
            vec![
                Node::button(message(cx, "setting-more-info", &[]))
                    .key(format!("{GUIDE}{name}").as_str())
                    .as_link()
                    .style(Style {
                        text_size: Some(TextSize::Small),
                        ..Style::default()
                    }),
            ],
            0.0,
        ));
    }
    // A list's values, under what it does.
    if matches!(key.kind, SettingKind::List { .. }) {
        about.push(list_editor(cx, key));
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
    let mut value: Vec<Node> = control(cx, key).into_iter().collect();
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
    !matches!(kind, SettingKind::Other)
}

/// The texts of a list's value, `value`; none of another.
fn list_texts(value: Option<&Data>) -> Vec<String> {
    match value {
        Some(Data::List(items)) => items.iter().map(plain).collect(),
        _ => Vec::new(),
    }
}

/// A list's values: each of its text and a link that takes it away, and a
/// field that adds one as Return is pressed.
fn list_editor(cx: &mut Cx<'_>, key: &SettingKey) -> Node {
    let name = &key.name;
    let mut rows: Vec<Node> = list_texts(key.value.as_ref())
        .into_iter()
        .enumerate()
        .map(|(at, text)| {
            row_of(
                vec![
                    Node::text(text)
                        .key(format!("item:{name}:{at}").as_str())
                        .style(code())
                        .layout(Layout {
                            grow: 1.0,
                            min_width: Length::Points(0.0),
                            ..Layout::default()
                        }),
                    Node::button(message(cx, "setting-list-remove", &[]))
                        .key(format!("{REMOVE}{name}:{at}").as_str())
                        .as_link()
                        .style(Style {
                            text_size: Some(TextSize::Small),
                            ..Style::default()
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
        })
        .collect();
    rows.push(
        Node::field(message(cx, "setting-list-add", &[]))
            .key(format!("{ADD}{name}").as_str())
            .label(name.clone())
            .style(field_text())
            .layout(Layout {
                width: Length::Points(180.0),
                ..Layout::default()
            }),
    );
    Node::container(rows)
        .key(format!("list:{name}").as_str())
        .layout(Layout {
            direction: Direction::Column,
            gap: 4.0,
            width: Length::Fraction(1.0),
            padding: Edges {
                top: 4.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            ..Layout::default()
        })
}

/// The control of `key`'s value: a toggle of a boolean, the button of a
/// menu of its values, a field of a number or of a text; another value's
/// text, as the config writes it; none of a list, whose values show under
/// what it does.
fn control(cx: &mut Cx<'_>, key: &SettingKey) -> Option<Node> {
    let name = &key.name;
    let terminal = code();
    Some(match &key.kind {
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
        // A list's values show under what it does.
        SettingKind::List { .. } => return None,
        SettingKind::Other => match &key.value {
            Some(value) => Node::text(shown(value))
                .key(format!("value:{name}").as_str())
                .style(terminal),
            None => Node::text(message(cx, "setting-unset", &[]))
                .key(format!("value:{name}").as_str())
                .style(secondary()),
        },
    })
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

/// The part of the bindings of keys: every one in force, the defaults and
/// the config's own, and the keys that the config takes away, of those
/// that the search's `words` show, by their commands' categories, each of
/// its keys as macOS writes them, which a click lets change in a field,
/// the one of `editing`, its command's title, its name, arguments and
/// context, and a link that takes it away or gives a default back; then
/// the fields of a new binding.
fn keys_part(
    cx: &mut Cx<'_>,
    bindings: &[SettingBinding],
    words: &[String],
    editing: Option<usize>,
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
        add_row(cx, ADD_BINDING),
    ];
    let terminal = code();
    let shown: Vec<(usize, &SettingBinding)> = bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| words.is_empty() || binding_holds(cx, binding, words))
        .collect();
    for category in CATEGORIES {
        let mut of: Vec<(usize, &SettingBinding)> = shown
            .iter()
            .copied()
            .filter(|(_, binding)| binding.category == category)
            .collect();
        if of.is_empty() {
            continue;
        }
        of.sort_by(|(_, a), (_, b)| {
            let named = |binding: &SettingBinding| {
                binding
                    .title
                    .clone()
                    .or_else(|| binding.command.clone())
                    .unwrap_or_default()
                    .to_lowercase()
            };
            named(a).cmp(&named(b)).then_with(|| a.keys.cmp(&b.keys))
        });
        let name = category_message(category);
        part.push(
            Node::text(message(cx, name, &[]))
                .key(name)
                .role(Role::Heading)
                .style(Style {
                    font_weight: Some(FontWeight::Semibold),
                    ..Style::default()
                }),
        );
        let rows = of
            .into_iter()
            .map(|(at, binding)| {
                binding_row(cx, (at, binding), editing == Some(at), refused, &terminal)
            })
            .collect();
        part.push(group(rows, &format!("bindings:{name}")));
    }
    if shown.is_empty() {
        part.push(
            Node::text(message(cx, "bindings-none", &[]))
                .key("no-bindings")
                .style(secondary())
                .layout(wrapped()),
        );
    }
    // Add again below the bindings, as above them.
    if !shown.is_empty() {
        part.push(add_row(cx, ADD_BINDING_BELOW));
    }
    if let Some(why) = refused.get(KEYS) {
        part.push(refusal(why, "refused:keys"));
    }
    part
}

/// The row of Add, of `key`, at the part's right, which opens the panel of
/// a new binding.
fn add_row(cx: &mut Cx<'_>, key: &str) -> Node {
    Node::container(vec![Node::button(message(cx, "binding-add", &[])).key(key)])
        .key(format!("{key}-row").as_str())
        .layout(Layout {
            direction: Direction::Row,
            justify: Justify::End,
            width: Length::Fraction(1.0),
            ..Layout::default()
        })
}

/// The row of the binding `binding`, at `at` among the settings' bindings:
/// its command's title, or what it takes away, its name, arguments and
/// context, and whether it is the config's; at its right its keys, a
/// button that lets them change, or their field of `terminal`'s text where
/// `editing`, and its link; why a change was refused under it.
fn binding_row(
    cx: &mut Cx<'_>,
    (at, binding): (usize, &SettingBinding),
    editing: bool,
    refused: &BTreeMap<String, String>,
    terminal: &Style,
) -> Node {
    let keys = &binding.keys;
    let does = match (&binding.command, &binding.title) {
        (Some(_), Some(title)) => title.clone(),
        (Some(command), None) => command.clone(),
        (None, Some(title)) => message(cx, "binding-removed", &[("title", title.clone())]),
        (None, None) => message(cx, "binding-nothing", &[]),
    };
    let mut details = Vec::new();
    if let Some(command) = &binding.command {
        details.push(if binding.args.is_empty() {
            command.clone()
        } else {
            format!("{command} ({})", args_text(&binding.args))
        });
    }
    if let Some(when) = &binding.when {
        details.push(message(cx, "binding-when", &[("when", when.clone())]));
    }
    if binding.system {
        details.push(message(cx, "binding-system", &[]));
    } else if !binding.default {
        details.push(message(cx, "binding-yours", &[]));
    }
    let mut about = vec![
        Node::text(does)
            .key(format!("command:{at}").as_str())
            .layout(wrapped()),
    ];
    if !details.is_empty() {
        about.push(
            Node::text(details.join(" · "))
                .key(format!("details:{at}").as_str())
                .style(small())
                .layout(wrapped()),
        );
    }
    if let Some(why) = refused.get(&format!("binding:{at}")) {
        about.push(refusal(why, &format!("refused:binding:{at}")));
    }
    let about = Node::container(about).layout(Layout {
        direction: Direction::Column,
        gap: 3.0,
        grow: 1.0,
        min_width: Length::Points(0.0),
        ..Layout::default()
    });
    let shown = lokyr_plugin_sdk::keys::macos_text(keys).unwrap_or_else(|| keys.clone());
    // The keys of a command, alone or of its arguments and context, which
    // change; neither macOS's own, of its app menu, nor `false`, nor the
    // config's commands of several contexts, as the file tells them.
    let changes =
        binding.command.is_some() && !binding.system && (binding.default || !binding.detailed);
    let keys_node = if editing {
        Node::field(message(cx, "binding-press", &[]))
            .key(format!("{KEYS_FIELD}{at}").as_str())
            .label(message(cx, "binding-keys", &[]))
            .records()
            .holding(shown.as_str())
            .style(terminal.clone())
            .layout(Layout {
                width: Length::Points(150.0),
                ..Layout::default()
            })
    } else if changes {
        Node::button(shown.as_str())
            .key(format!("{BINDING_KEYS}{at}").as_str())
            .label(message(cx, "binding-change", &[("keys", shown.clone())]))
            .tooltip(message(cx, "binding-change", &[("keys", shown.clone())]))
            .style(terminal.clone())
    } else {
        Node::text(shown.as_str())
            .key(format!("{BINDING_KEYS}{at}").as_str())
            .style(terminal.clone())
    };
    let mut row = vec![about, keys_node];
    if binding.command.is_none() {
        row.push(
            Node::button(message(cx, "binding-restore", &[]))
                .key(format!("{RESTORE}{at}").as_str())
                .as_link(),
        );
    } else if !binding.system {
        row.push(
            Node::button(message(cx, "binding-remove", &[]))
                .key(format!("{UNBIND}{at}").as_str())
                .as_link(),
        );
    }
    Node::container(row)
        .key(format!("binding:{at}").as_str())
        .role(Role::Group)
        .label(keys.clone())
        .layout(row_layout())
}

/// The panel of a new binding, of `adding`: its title, the recorder of its
/// keys, shown as macOS writes them, the button of the commands' menu, of
/// the command chosen, what a binding of `settings` of the same keys
/// does, which the new one takes the place of, or that macOS's own are
/// none to take, why a write was refused, of `refused`, and Cancel and
/// Add, which waits for both the keys and a command.
fn adding_tree(
    cx: &mut Cx<'_>,
    settings: &Settings,
    adding: &Adding,
    refused: &BTreeMap<String, String>,
) -> Node {
    let label = |cx: &mut Cx<'_>, id: &str| {
        Node::text(message(cx, id, &[])).layout(Layout {
            width: Length::Points(90.0),
            min_width: Length::Points(90.0),
            ..Layout::default()
        })
    };
    let line = |nodes: Vec<Node>| {
        Node::container(nodes).layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        })
    };
    let shown = adding
        .keys
        .as_deref()
        .map(|keys| lokyr_plugin_sdk::keys::macos_text(keys).unwrap_or_else(|| keys.to_owned()))
        .unwrap_or_default();
    let recorder = Node::field(message(cx, "binding-press", &[]))
        .key(RECORDER)
        .label(message(cx, "binding-keys", &[]))
        .records()
        .holding(shown.as_str())
        .style(code())
        .layout(Layout {
            grow: 1.0,
            min_width: Length::Points(0.0),
            ..Layout::default()
        });
    let title = adding.command.as_deref().map(|name| {
        cx.commands()
            .ok()
            .and_then(|commands| commands.into_iter().find(|command| command.name == name))
            .map_or_else(|| name.to_owned(), |command| command.title)
    });
    let choose = Node::button(title.unwrap_or_else(|| message(cx, "binding-choose", &[])))
        .key(CHOOSE_COMMAND)
        .label(message(cx, "binding-command", &[]))
        .layout(Layout {
            grow: 1.0,
            min_width: Length::Points(0.0),
            ..Layout::default()
        });
    let mut children = vec![
        Node::text(message(cx, "binding-new-title", &[]))
            .key("new-binding-title")
            .role(Role::Heading)
            .style(Style {
                font_weight: Some(FontWeight::Bold),
                ..Style::default()
            }),
        line(vec![label(cx, "binding-shortcut"), recorder]),
        line(vec![label(cx, "binding-command"), choose]),
    ];
    // The binding of the same keys, which the new one takes the place of,
    // or macOS's own, which none takes.
    let combos = adding
        .keys
        .as_deref()
        .and_then(|keys| lokyr_plugin_sdk::keys::parse_keys(keys).ok());
    let system = combos
        .as_ref()
        .is_some_and(|combos| lokyr_plugin_sdk::keys::is_system(combos, None));
    let taken = combos.as_ref().and_then(|combos| {
        settings.keys.iter().find(|binding| {
            binding.command.is_some()
                && lokyr_plugin_sdk::keys::parse_keys(&binding.keys)
                    .ok()
                    .as_ref()
                    == Some(combos)
        })
    });
    if system {
        children.push(refusal(
            &message(cx, "binding-taken-system", &[("keys", shown.clone())]),
            "taken",
        ));
    } else if let Some(binding) = taken {
        let does = binding
            .title
            .clone()
            .or_else(|| binding.command.clone())
            .unwrap_or_default();
        children.push(
            Node::text(message(
                cx,
                "binding-taken",
                &[("keys", shown.clone()), ("title", does)],
            ))
            .key("taken")
            .style(Style {
                color: Some(Token::from("text.warning")),
                text_size: Some(TextSize::Small),
                ..Style::default()
            })
            .layout(wrapped()),
        );
    }
    if let Some(why) = refused.get(NEW_REFUSED) {
        children.push(refusal(why, "refused:new"));
    }
    let ready = adding.keys.is_some() && adding.command.is_some() && !system;
    let mut add = Node::button(message(cx, "binding-confirm", &[])).key(CONFIRM);
    if let Kind::Button(button) = &mut add.kind {
        button.disabled = !ready;
    }
    children.push(
        Node::container(vec![
            Node::button(message(cx, "binding-cancel", &[])).key(CANCEL),
            add,
        ])
        .layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            justify: Justify::End,
            width: Length::Fraction(1.0),
            ..Layout::default()
        }),
    );
    Node::container(children)
        .key("new-binding-panel")
        .label(message(cx, "binding-new-title", &[]))
        .layout(Layout {
            direction: Direction::Column,
            gap: 12.0,
            width: Length::Points(400.0),
            padding: Edges::all(16.0),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 10.0,
            shadow: Some(lokyr_plugin_sdk::view::Shadow::default()),
            ..Style::default()
        })
}

/// The menu of the commands that a binding can run of no arguments, by
/// their categories, each a submenu of their titles.
fn commands_menu(cx: &mut Cx<'_>) -> Node {
    let mut commands = cx.commands().unwrap_or_default();
    commands.retain(|command| command.params.iter().all(|param| param.default.is_some()));
    commands.sort_by_key(|command| command.title.to_lowercase());
    let mut groups = Vec::new();
    for category in CATEGORIES.into_iter().flatten() {
        let items: Vec<Node> = commands
            .iter()
            .filter(|command| command.category == category)
            .map(|command| {
                Node::button(command.title.as_str())
                    .key(format!("{COMMAND_ITEM}{}", command.name).as_str())
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        let mut group = vec![Node::text(message(
            cx,
            category_message(Some(category)),
            &[],
        ))];
        group.extend(items);
        groups.push(Node::container(group));
    }
    Node::container(groups).key("commands-menu")
}

/// The text of `why`, a write refused, under its control, of `key`.
fn refusal(why: &str, key: &str) -> Node {
    Node::text(why)
        .key(key)
        .style(Style {
            color: Some(Token::from("progress.error")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
        .layout(wrapped())
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

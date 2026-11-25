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

use std::collections::{BTreeMap, BTreeSet};

use lokyr_plugin_sdk::command::{Args, Category, Error, Value};
use lokyr_plugin_sdk::host::{
    FontFace, SettingBinding, SettingField, SettingKey, SettingKind, Settings, SettingsSection,
};
use lokyr_plugin_sdk::message::{Data, Event, SETTINGS_CHANGED, THEME_CHANGED, THEMES_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::theme::{TOKENS, ThemeVariant};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, ButtonVariant, ControlSize, Direction, Edges, FontFamily, FontWeight, Hover,
    Justify, Kind, Layout, Length, ListItem, Node, PointerShape, ProgressShape, ProgressState,
    Role, Style, TextSize, Token, Tone, UiEventKind,
};

lokyr_plugin_sdk::export_plugin!(SettingsPage);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/settings.ftl")),
    ("it", include_str!("../locales/it/settings.ftl")),
];

/// Its page of the settings window.
const PAGE: &str = "settings";

/// The id of its page of the themes.
const THEMES: &str = "themes";

/// The id of its page of the bindings of keys.
const KEYS_PAGE: &str = "keys";

/// The prefix of the key of a theme's card on the themes' page, of its
/// appearance and its name, as `theme:dark:Lokyr Dark`.
const THEME_CARD: &str = "theme:";

/// The panel of a theme's card's context menu, of its items' keys: the
/// theme used, shown in the Editor, or a new theme made of it.
const THEME_MENU: &str = "theme-menu";
const THEME_USE: &str = "theme-use";
const THEME_EDIT: &str = "theme-edit";
const THEME_DUPLICATE: &str = "theme-duplicate";

/// The themes' page's button of the themes imported, and the app's command
/// that imports them.
const THEMES_IMPORT: &str = "themes-import";
const IMPORT_THEMES: &str = "app.import-themes";

/// The themes' page's button of a new theme, of Lokyr's own values of the
/// appearance that it shows.
const THEMES_NEW: &str = "themes-new";

/// The themes' page's button of the themes' folder, which the app shows
/// in the file manager.
const THEMES_FOLDER: &str = "themes-folder";
const OPEN_THEMES: &str = "app.open-themes-folder";

/// How many themes' cards a row of the themes' page holds.
const CARDS_ACROSS: usize = 3;

/// The key of the themes' page's sidebar's list of the appearances.
/// A row of the themes page's sidebar, of an appearance's word or of the
/// Editor.
const SIDE: &str = "themes-side:";

/// The sidebar's row of the Theme Editor.
const SIDE_EDITOR: &str = "editor";

/// The height of a theme's card's preview, in points.
const PREVIEW_HEIGHT: f32 = 92.0;

/// The key of the sidebar's list of the sections.
const SECTIONS: &str = "sections";

/// The sidebar's width, in points.
const SIDEBAR: f32 = 160.0;

/// The room between a sidebar's edges and its search field and its rows'
/// bands, at its left and at its right alike, in points: as much as the
/// host insets a menu's list's bands from the list's sides, of a list as
/// wide as its sidebar.
const SIDE_MARGIN: f32 = 6.0;

/// The room inside a sidebar's row's band, before its icon, which lines up
/// with the search field's, and after its title, in points.
const BAND_PADDING: (f32, f32) = (4.0, 6.0);

/// The height of a row of the sidebar, in points, as macOS's sidebars'.
const ROW: f32 = 28.0;

/// The prefix of the key of a boolean's toggle.
const SET: &str = "set:";

/// The prefix of the key of the pop-up button of a key's menu of values.
const CHOOSE: &str = "choose:";

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

/// The prefix of the key of a binding's keys that do not change, a text.
const KEYS_TEXT: &str = "keys-text:";

/// The prefix of the key of the link that gives a default back, of keys
/// that the config takes away.
const RESTORE: &str = "restore:";

/// The prefix of the key of the warning of keys recorded that another
/// binding takes, by the binding's place, and of its place among the
/// page's refusals.
const TAKEN: &str = "taken:binding:";

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

/// The prefix of the switch that turns a plugin's section's plugin on or
/// off, of the section's name, and of the reason why its last write was
/// refused.
const PLUGIN_ON: &str = "plugin-on:";

/// The prefix of the key of a key's link to the documentation's page of its
/// topic.
const DOCS: &str = "docs:";

/// The prefix of the key of a list's value's button that takes it away,
/// `remove:<key>:<index>`.
const REMOVE: &str = "remove:";

/// The prefix of the key of a list's field that adds a value.
const ADD: &str = "add:";

/// The prefixes of the keys of a map's editor, of the key's name, its
/// entry's and its field's, parted by [`PART`]: an entry's name, a field of
/// an entry, an entry's Remove, a field of the new entry, of its name of an
/// empty field's, and Add.
const ENTRY_NAME: &str = "entry-name:";
const ENTRY_FIELD: &str = "entry-field:";
const ENTRY_REMOVE: &str = "entry-remove:";
const NEW_ENTRY: &str = "new-entry:";
const ADD_ENTRY: &str = "add-entry:";

/// What parts a key's name, an entry's and a field's in the keys of a
/// map's editor: no name of an entry holds it, as none holds a control
/// character.
const PART: char = '\u{1f}';

/// The key of the sidebar's search field.
const SEARCH: &str = "search";

/// The key of the Keys part's field that filters its bindings.
const KEYS_FILTER: &str = "keys-filter";

/// The platforms that Lokyr runs on, of which a key that applies on every
/// one names none.
const PLATFORMS: &[&str] = &["macos"];

/// How long typing pauses before a field's value is written.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(600);

/// How long a menu's highlighted item rests before its font shows in the
/// panes, so that the arrows held, or a pointer that passes, show none of
/// the items that they go through.
const PREVIEW_PAUSE: std::time::Duration = std::time::Duration::from_millis(150);

/// A new binding as its panel makes it: its keys, as the recorder took
/// them and as a config writes them, and its command, by its name.
#[derive(Clone, Debug, Default)]
struct Adding {
    keys: Option<String>,
    command: Option<String>,
}

/// The Theme Editor's previews of the theme `theme`, the window's and the
/// panel's, each in a part of that theme, as the Editor first shows them:
/// of no part marked, their texts of `words`, the messages of the plugin's
/// catalogs by their names.
pub fn theme_previews(theme: &str, words: &dyn Fn(&str) -> String) -> (Node, Node) {
    editor::previews(theme, words)
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
    /// Whether its page of the bindings of keys shows, and the words of its
    /// filter, which the page as it shows anew empties.
    keys_shown: bool,
    keys_filter: String,
    refused: BTreeMap<String, String>,
    /// The binding whose keys change, by its place among the settings'
    /// bindings, whose field records them.
    editing: Option<usize>,
    /// The keys recorded for the binding at its place that another binding
    /// takes, told, which a second press of them writes.
    confirming: Option<(usize, String)>,
    /// The new binding that Add's panel makes, while it shows.
    adding: Option<Adding>,
    /// Whether a font's family or style shows in the panes as a menu's
    /// item is highlighted, until the menu closes.
    previewing: bool,
    /// Whether its page of the themes shows, and the appearance whose
    /// themes it shows, the one in force's until its sidebar chooses one.
    themes_shown: bool,
    themes_variant: Option<ThemeVariant>,
    /// The Theme Editor, of the themes page's sidebar, and whether it
    /// shows there in place of an appearance's themes.
    editor: editor::Editor,
    in_editor: bool,
    /// The theme of the card whose context menu shows, of its appearance.
    theme_menu: Option<(ThemeVariant, String)>,
    /// The question of a new theme of New…, whose answer, its name, the
    /// Editor shows.
    creating: Option<u64>,
    /// The question of a theme of the appearance not in force, for it
    /// alone or both, of the theme's appearance and name.
    using: Option<(u64, ThemeVariant, String)>,
    /// The font of the item highlighted last, which shows once the timer
    /// of its rest fires.
    pending_preview: Option<(FontPreview, String)>,
    preview_timer: Option<u64>,
    typed: BTreeMap<String, Typed>,
    pause: Option<u64>,
    /// The texts typed into the fields of each map's new entry, by the
    /// map's name, each by its field's, its name's of an empty one, which
    /// its fields hold.
    new_entries: NewEntries,
    /// The fields of maps' entries whose texts their map refused, by their
    /// nodes' keys, whose texts go as the map is written or the settings
    /// change, the reason under the map gone with them.
    refused_texts: BTreeSet<String>,
}

/// The texts typed into the fields of each map's new entry, by the map's
/// name, each by its field's, its name's of an empty one.
type NewEntries = BTreeMap<String, BTreeMap<String, String>>;

/// The texts that the page's tree shows beside the settings: why each
/// key's last write was refused, by its name, the texts typed and not
/// written yet, and the new entries' texts.
type Texts<'a> = (
    &'a BTreeMap<String, String>,
    &'a BTreeMap<String, Typed>,
    &'a NewEntries,
);

/// A text typed into a key's field and not written yet: the key's section
/// and plugin, the text, and the entry of a map whose field it is, by the
/// key of its field.
#[derive(Clone, Debug)]
struct Typed {
    section: String,
    plugin: Option<String>,
    text: String,
    entry: Option<EntryAt>,
}

/// A field of an entry of a map: the map's key, the entry's name, and the
/// field's, of none the field of the entry's name.
#[derive(Clone, Debug)]
struct EntryAt {
    key: String,
    entry: String,
    field: Option<String>,
}

/// A part of the page, as its sidebar lists it: a section, and the
/// plugins' sections of its name that it shows after its own keys, as the
/// `links` plugin's hints in the app's Links, each key written to its own
/// section.
struct Part<'s> {
    section: &'s SettingsSection,
    joined: Vec<&'s SettingsSection>,
}

impl<'s> Part<'s> {
    /// Its sections, its own first.
    fn sections(&self) -> impl Iterator<Item = &'s SettingsSection> + '_ {
        std::iter::once(self.section).chain(self.joined.iter().copied())
    }

    /// The key `name` of one of its sections, and that section.
    fn find(&self, name: &str) -> Option<(&'s SettingsSection, &'s SettingKey)> {
        self.sections()
            .find_map(|section| find(section, name).map(|key| (section, key)))
    }
}

/// The prefix of the names of Lokyr's own plugins, whose sections join the
/// app's of their names.
const OWN_PLUGINS: &str = "lokyr/";

/// The parts of the page, as its sidebar lists them, the sections of
/// `settings`: the app's, each with Lokyr's own plugins' of its name whose
/// keys none of its own has, then Lokyr's own plugins' other sections,
/// then the other plugins', which the sidebar divides from them.
fn parts(settings: &Settings) -> Vec<Part<'_>> {
    let (app, plugins): (Vec<&SettingsSection>, Vec<&SettingsSection>) = settings
        .sections
        .iter()
        .partition(|section| section.plugin.is_none());
    let mut taken = vec![false; plugins.len()];
    let mut parts: Vec<Part<'_>> = app
        .into_iter()
        .map(|section| {
            let mut names: Vec<&str> = section.keys.iter().map(|key| key.name.as_str()).collect();
            let mut joined = Vec::new();
            for (at, &plugin) in plugins.iter().enumerate() {
                // Lokyr's own plugins alone, so that another's section
                // never shows as a part of the app's own.
                if !taken[at]
                    && plugin
                        .plugin
                        .as_deref()
                        .is_some_and(|name| name.starts_with(OWN_PLUGINS))
                    && plugin.name == section.name
                    && plugin
                        .keys
                        .iter()
                        .all(|key| !names.contains(&key.name.as_str()))
                {
                    names.extend(plugin.keys.iter().map(|key| key.name.as_str()));
                    joined.push(plugin);
                    taken[at] = true;
                }
            }
            Part { section, joined }
        })
        .collect();
    let mut rest: Vec<&SettingsSection> = plugins
        .into_iter()
        .zip(taken)
        .filter(|(_, taken)| !taken)
        .map(|(section, _)| section)
        .collect();
    // Lokyr's own first, in their order, then the others.
    rest.sort_by_key(|section| !is_own(section));
    parts.extend(rest.into_iter().map(|section| Part {
        section,
        joined: Vec::new(),
    }));
    parts
}

/// Whether `section` is the app's or one of Lokyr's own plugins'.
fn is_own(section: &SettingsSection) -> bool {
    section
        .plugin
        .as_deref()
        .is_none_or(|name| name.starts_with(OWN_PLUGINS))
}

/// The page's view.
fn page() -> ViewId {
    ViewId::Page(PAGE.to_owned())
}

/// The view of its page of the themes.
fn themes_page() -> ViewId {
    ViewId::Page(THEMES.to_owned())
}

/// The view of its page of the bindings of keys.
fn keys_page() -> ViewId {
    ViewId::Page(KEYS_PAGE.to_owned())
}

/// The settings, as the page shows them: the themes of `[theme]` left to
/// the themes' page, their key of the contrast kept.
fn page_settings(cx: &mut Cx<'_>) -> Result<Settings, Error> {
    let mut settings = cx.settings()?;
    for section in &mut settings.sections {
        if section.name == "theme" && section.plugin.is_none() {
            section
                .keys
                .retain(|key| !["light", "dark"].contains(&key.name.as_str()));
        }
    }
    Ok(settings)
}

impl Plugin for SettingsPage {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.subscribe(&[SETTINGS_CHANGED, THEMES_CHANGED, THEME_CHANGED])
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let mut again = false;
        for event in events {
            // The Theme Editor's.
            if self.editor.on_event(cx, event)? {
                continue;
            }
            match event {
                // The page shows at its first section, as its sidebar does.
                Event::PageShown(page) if page == THEMES => {
                    self.themes_shown = true;
                    self.themes_variant = None;
                    self.in_editor = false;
                    cx.request_render(themes_page())?;
                }
                // A theme of the appearance not in force: named for it
                // alone, for both, or not.
                Event::Chosen { id, button }
                    if self.using.as_ref().is_some_and(|using| using.0 == *id) =>
                {
                    if let Some((_, variant, name)) = self.using.take() {
                        match button {
                            Some(0) => cx.set_theme(variant, &name)?,
                            Some(1) => {
                                cx.set_theme(ThemeVariant::Light, &name)?;
                                cx.set_theme(ThemeVariant::Dark, &name)?;
                            }
                            _ => {}
                        }
                    }
                }
                // A new theme of New… made: shown in the Editor.
                Event::Answered { id, text } if self.creating == Some(*id) => {
                    self.creating = None;
                    if let (Some(name), true) = (text, self.themes_shown) {
                        self.edit_theme(cx, name)?;
                    }
                }
                // The themes' list read, or anew: their cards, of a page
                // that its window's closing in the same batch may have
                // taken, which fails no write of the call.
                Event::ThemesChanged if self.themes_shown => {
                    let _ = cx.request_render(themes_page());
                }
                Event::PageHidden(page) if page == THEMES => {
                    self.themes_shown = false;
                    if std::mem::take(&mut self.in_editor) {
                        self.editor.hide(cx)?;
                    }
                }
                // The page of the keys shows anew: of no filter, change of
                // keys nor refusal.
                Event::PageShown(page) if page == KEYS_PAGE => {
                    self.keys_shown = true;
                    self.keys_filter.clear();
                    self.editing = None;
                    self.confirming = None;
                    self.refused.clear();
                    again = true;
                }
                Event::PageHidden(page) if page == KEYS_PAGE => {
                    self.keys_shown = false;
                    self.editing = None;
                    self.confirming = None;
                }
                // The page shows anew, as its fields do: at its first
                // section, of no search nor refusal.
                Event::PageShown(page) if page == PAGE => {
                    self.shown = true;
                    self.chosen = 0;
                    self.query.clear();
                    self.typed.clear();
                    self.refused.clear();
                    self.new_entries.clear();
                    self.refused_texts.clear();
                    again = true;
                }
                Event::PageHidden(page) if page == PAGE => {
                    self.shown = false;
                    self.write_typed(cx, None)?;
                }
                // The new binding's panel closed, by Escape, Cancel or Add.
                Event::PanelClosed(PanelId(panel)) if panel == NEW_BINDING => {
                    self.adding = None;
                    self.refused.remove(NEW_REFUSED);
                }
                Event::PanelClosed(PanelId(panel)) if panel == THEME_MENU => {
                    self.theme_menu = None;
                }
                // A highlighted item rested: its font shows.
                Event::Timer { id } if self.preview_timer == Some(*id) => {
                    self.preview_timer = None;
                    if let Some((preview, value)) = self.pending_preview.take() {
                        match preview {
                            FontPreview::Family => cx.preview_font(Some(&value))?,
                            FontPreview::Style(face) => cx.preview_font_style(face, &value)?,
                        }
                        self.previewing = true;
                    }
                }
                // The bindings' places change with the settings: a change of
                // keys under way ends; a text that its key refused goes, of
                // its reason, the field showing the value in force anew,
                // and one typed and not written yet stays.
                Event::SettingsChanged => {
                    self.editing = None;
                    self.confirming = None;
                    let (refused, refused_texts) = (&self.refused, &self.refused_texts);
                    self.typed.retain(|name, _| {
                        !refused.contains_key(name) && !refused_texts.contains(name)
                    });
                    self.refused.clear();
                    self.refused_texts.clear();
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
        // Its views are there while their pages show alone.
        if again && self.shown {
            self.render_page(cx)?;
        }
        if again && self.keys_shown {
            self.render_keys(cx)?;
        }
        // The themes' page marks the themes that `[theme]` names.
        if again && self.themes_shown {
            cx.request_render(themes_page())?;
        }
        Ok(())
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if self.editor.on_ui_event(cx, event)? {
            return Ok(());
        }
        let key = event.key.0.as_str();
        // An item of a theme's card's context menu: the theme used, shown
        // in the Editor, or a copy of it, as the app asks its name.
        if event.view == ViewId::Panel(PanelId::from(THEME_MENU)) {
            if event.kind != UiEventKind::Click {
                return Ok(());
            }
            let Some((variant, name)) = self.theme_menu.take() else {
                return Ok(());
            };
            let _ = cx.close_panel(THEME_MENU);
            return match key {
                THEME_USE => self.use_theme(cx, variant, &name),
                THEME_EDIT => self.edit_theme(cx, &name),
                // A copy, its name asked, used for its appearance, its card
                // chosen, of no editor opened: Edit shows it in the Editor.
                THEME_DUPLICATE => cx.copy_theme(&name).map(|_| ()),
                _ => Ok(()),
            };
        }
        // A theme's card: the theme named in `[theme]` for its appearance,
        // or its context menu; an appearance of the sidebar, whose themes
        // show.
        if event.view == themes_page() {
            if let (UiEventKind::Click, Some(row)) = (&event.kind, key.strip_prefix(SIDE)) {
                let variant = match row {
                    "light" => Some(ThemeVariant::Light),
                    "dark" => Some(ThemeVariant::Dark),
                    _ => None,
                };
                if row == SIDE_EDITOR {
                    if !self.in_editor {
                        self.in_editor = true;
                        self.editor.show(cx, None)?;
                    }
                } else if variant.is_some() {
                    if std::mem::take(&mut self.in_editor) {
                        self.editor.hide(cx)?;
                    }
                    self.themes_variant = variant;
                }
                return cx.request_render(themes_page());
            }
            if event.kind == UiEventKind::Click {
                match key {
                    THEMES_IMPORT => return cx.run_command(IMPORT_THEMES, Args::new()),
                    THEMES_FOLDER => return cx.run_command(OPEN_THEMES, Args::new()),
                    // A new theme of the appearance that the page shows, its
                    // name asked, shown in the Editor once made.
                    THEMES_NEW => {
                        let variant = match self.themes_variant {
                            Some(variant) => variant,
                            None => cx.themes()?.appearance,
                        };
                        self.creating = Some(cx.new_theme(variant)?);
                        return Ok(());
                    }
                    _ => {}
                }
            }
            let Some((variant, name)) = key
                .strip_prefix(THEME_CARD)
                .and_then(|rest| rest.split_once(':'))
            else {
                return Ok(());
            };
            let variant = match variant {
                "light" => ThemeVariant::Light,
                _ => ThemeVariant::Dark,
            };
            match event.kind {
                UiEventKind::Click => self.use_theme(cx, variant, name)?,
                UiEventKind::ContextMenu => {
                    self.theme_menu = Some((variant, name.to_owned()));
                    // Another card's menu, open still, gives way.
                    let _ = cx.close_panel(THEME_MENU);
                    cx.open_panel(
                        THEME_MENU,
                        PanelOptions {
                            anchor: Anchor::ViewPointer {
                                view: themes_page(),
                            },
                            role: PanelRole::Menu,
                            close_outside: true,
                            close_escape: true,
                            takes_keys: false,
                            close_typing: false,
                        },
                    )?;
                }
                _ => {}
            }
            return Ok(());
        }
        if let ViewId::Panel(PanelId(panel)) = &event.view
            && [NEW_BINDING, COMMANDS].contains(&panel.as_str())
        {
            return self.adding_event(cx, event);
        }
        // The page of the keys: its filter, of its bindings, and the
        // bindings.
        if event.view == keys_page() {
            return match (&event.kind, key) {
                (UiEventKind::InputChanged { text, .. }, KEYS_FILTER) => {
                    if *text == self.keys_filter {
                        return Ok(());
                    }
                    text.clone_into(&mut self.keys_filter);
                    self.render_keys(cx)
                }
                _ => self.binding_event(cx, event),
            };
        }
        // The search's words, which filter the parts and their keys.
        if let (UiEventKind::InputChanged { text, .. }, SEARCH) = (&event.kind, key) {
            return self.search(cx, text);
        }
        if let (UiEventKind::Select { item } | UiEventKind::Activate { item }, SECTIONS) =
            (&event.kind, key)
        {
            // The sidebar's item, of the parts that the search shows.
            let settings = page_settings(cx)?;
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
            return self.render_page(cx);
        }
        let settings = page_settings(cx)?;
        let Some(part) = parts(&settings).into_iter().nth(self.chosen) else {
            return Ok(());
        };
        if matches!(event.view, ViewId::Panel(_)) {
            return Ok(());
        }
        // A plugin turned on or off, as its section's `enabled` says.
        if let (UiEventKind::Toggle { on }, Some(name)) = (&event.kind, key.strip_prefix(PLUGIN_ON))
        {
            let Some(section) = part
                .sections()
                .find(|section| section.name == name && section.plugin.is_some())
            else {
                return Ok(());
            };
            let written = cx.set_setting(
                &section.name,
                section.plugin.as_deref(),
                "enabled",
                Some(Data::Boolean(*on)),
            );
            return self.told(cx, key, written);
        }
        // A map's entry taken away, changed, renamed or added.
        if key.starts_with(ENTRY_REMOVE)
            || key.starts_with(ENTRY_NAME)
            || key.starts_with(ENTRY_FIELD)
            || key.starts_with(NEW_ENTRY)
            || key.starts_with(ADD_ENTRY)
        {
            return self.entry_event(cx, &part, event);
        }
        // A list's value taken away, or one added.
        if let (UiEventKind::Click, Some(rest)) = (&event.kind, key.strip_prefix(REMOVE)) {
            let Some((name, at)) = rest.rsplit_once(':') else {
                return Ok(());
            };
            let (Some((section, setting)), Ok(at)) = (part.find(name), at.parse::<usize>()) else {
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
            let Some((section, setting)) = part.find(name) else {
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
        let Some((section, setting)) = part.find(name) else {
            return Ok(());
        };
        let prefix = format!("{prefix}:");
        match (&event.kind, prefix.as_str()) {
            (UiEventKind::Toggle { on }, SET) => {
                self.write(cx, section, setting, Some(Data::Boolean(*on)))
            }
            // A value chosen in a key's menu, which shows as the config
            // takes it: no preview comes after it.
            (UiEventKind::Select { item }, CHOOSE) => {
                let Some(value) =
                    menu_values(cx, section, setting).and_then(|values| values.get(*item).cloned())
                else {
                    return Ok(());
                };
                self.pending_preview = None;
                if let Some(timer) = self.preview_timer.take() {
                    cx.cancel_timer(timer)?;
                }
                self.write(cx, section, setting, Some(value))
            }
            // A font's family or a face's style shown in the panes once the
            // item that the menu highlights rests; of none, as the menu
            // closes, the font that it showed goes, the one in force back,
            // or the one chosen.
            (UiEventKind::Highlight { item: None }, CHOOSE) => self.end_preview(cx),
            (UiEventKind::Highlight { item: Some(item) }, CHOOSE) => {
                let value =
                    menu_values(cx, section, setting).and_then(|values| values.get(*item).cloned());
                if let (Some(Data::Text(value)), Some(preview)) =
                    (value, font_preview(section, setting))
                {
                    self.pending_preview = Some((preview, value));
                    if let Some(timer) = self.preview_timer.take() {
                        cx.cancel_timer(timer)?;
                    }
                    self.preview_timer = Some(cx.start_timer(PREVIEW_PAUSE, false)?);
                }
                Ok(())
            }
            // Typed: written as typing pauses, as Return is pressed, or as
            // the field is left.
            (UiEventKind::InputChanged { text, .. }, FIELD) => {
                self.typed.insert(
                    name.to_owned(),
                    Typed {
                        section: section.name.clone(),
                        plugin: section.plugin.clone(),
                        text: text.clone(),
                        entry: None,
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
                        entry: None,
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
            (UiEventKind::Click, RESET) => {
                self.typed.remove(name);
                self.write(cx, section, setting, None)
            }
            (UiEventKind::Click, GUIDE) => match &setting.guide {
                Some(address) => cx.open(address),
                None => Ok(()),
            },
            (UiEventKind::Click, DOCS) => match &setting.docs {
                Some(address) => cx.open(address),
                None => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if let Some(tree) = self.editor.render(cx, view) {
            return tree;
        }
        if *view == themes_page() {
            let editor = self.in_editor.then_some(&self.editor);
            return Some(themes_tree(cx, self.themes_variant, editor));
        }
        if *view == keys_page() {
            let settings = page_settings(cx).ok()?;
            let filter = words(&self.keys_filter);
            return Some(keys_tree(
                cx,
                &settings,
                &filter,
                self.editing,
                &self.refused,
            ));
        }
        if *view == ViewId::Panel(PanelId::from(THEME_MENU)) {
            let (variant, name) = self.theme_menu.as_ref()?;
            // Edit dimmed of a theme that the app holds or builds in, which
            // a copy edits.
            let read_only = cx.themes().ok().is_none_or(|themes| {
                !themes.themes.iter().any(|theme| {
                    theme.name == *name && theme.variant == *variant && !theme.is_read_only()
                })
            });
            let mut edit = Node::button(message(cx, "theme-edit", &[]).as_str()).key(THEME_EDIT);
            if let Kind::Button(button) = &mut edit.kind {
                button.disabled = read_only;
            }
            return Some(Node::container(vec![
                Node::button(message(cx, "theme-use", &[]).as_str()).key(THEME_USE),
                edit,
                Node::button(message(cx, "theme-duplicate", &[]).as_str()).key(THEME_DUPLICATE),
            ]));
        }
        let settings = page_settings(cx).ok()?;
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
        if *view != page() {
            return None;
        }
        Some(tree(
            cx,
            &settings,
            self.chosen,
            &words(&self.query),
            (&self.refused, &self.typed, &self.new_entries),
        ))
    }
}

impl SettingsPage {
    /// Names the theme `name` in `[theme]` for its appearance `variant`:
    /// at once, of the appearance in force; else of a question, of the
    /// theme for its appearance alone, the default, or for both, as its
    /// appearance's theme would not show until the appearance changes.
    fn use_theme(
        &mut self,
        cx: &mut Cx<'_>,
        variant: ThemeVariant,
        name: &str,
    ) -> Result<(), Error> {
        if cx.themes()?.appearance == variant {
            return cx.set_theme(variant, name);
        }
        let (word, only) = match variant {
            ThemeVariant::Light => ("light", "theme-light-only"),
            ThemeVariant::Dark => ("dark", "theme-dark-only"),
        };
        let args = [("name", name.to_owned()), ("variant", word.to_owned())];
        let asked = message(cx, "theme-both-message", &args);
        let detail = message(cx, "theme-both-detail", &args);
        let buttons = [
            message(cx, only, &[]),
            message(cx, "theme-both", &[]),
            message(cx, "theme-both-cancel", &[]),
        ];
        let buttons: Vec<&str> = buttons.iter().map(String::as_str).collect();
        let id = cx.ask_choice(None, (&asked, &detail), &buttons)?;
        self.using = Some((id, variant, name.to_owned()));
        Ok(())
    }

    /// Shows the theme `name` in the Editor, the themes page's part that
    /// shows it then.
    fn edit_theme(&mut self, cx: &mut Cx<'_>, name: &str) -> Result<(), Error> {
        if self.in_editor {
            self.editor.choose(cx, name)?;
        } else {
            self.in_editor = true;
            self.editor.show(cx, Some(name))?;
        }
        cx.request_render(themes_page())
    }

    /// Ends the font's preview of a menu that closed: the item that waits
    /// to rest shows not, and the font in force shows again.
    fn end_preview(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.pending_preview = None;
        if let Some(timer) = self.preview_timer.take() {
            cx.cancel_timer(timer)?;
        }
        if std::mem::take(&mut self.previewing) {
            cx.preview_font(None)?;
        }
        Ok(())
    }

    /// Takes the search's text `text`: the parts that hold its words show,
    /// the part chosen the first of them where it holds none, the sidebar's
    /// selection at it.
    fn search(&mut self, cx: &mut Cx<'_>, text: &str) -> Result<(), Error> {
        if text == self.query {
            return Ok(());
        }
        text.clone_into(&mut self.query);
        let settings = page_settings(cx)?;
        let shown = shown_parts(cx, &settings, &words(&self.query));
        if !shown.contains(&self.chosen)
            && let Some(&first) = shown.first()
        {
            self.write_typed(cx, None)?;
            self.chosen = first;
            self.refused.clear();
        }
        self.render_page(cx)?;
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
                            view: keys_page(),
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
                let settings = page_settings(cx)?;
                if let Some(at) = at(BINDING_KEYS) {
                    if !settings.keys.get(at).is_some_and(changes) {
                        return Ok(());
                    }
                    self.editing = Some(at);
                    self.render_keys(cx)?;
                    return cx.focus_node(keys_page(), format!("{KEYS_FIELD}{at}").as_str());
                }
                if let Some((at, binding)) =
                    at(UNBIND).and_then(|at| settings.keys.get(at).map(|binding| (at, binding)))
                {
                    // A default's keys that others share, which `false`
                    // would take from them too, stay, as told.
                    if let Some(why) = shared_refusal(cx, &settings, at, binding) {
                        self.refused.insert(format!("binding:{at}"), why);
                        return self.render_keys(cx);
                    }
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
                let at = at(KEYS_FIELD).unwrap_or_default();
                self.editing = None;
                self.confirming = None;
                self.refused.remove(&format!("binding:{at}"));
                self.refused.remove(&format!("{TAKEN}{at}"));
                self.render_keys(cx)
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
                let settings = page_settings(cx).ok();
                if settings
                    .as_ref()
                    .is_some_and(|settings| runs_already(settings, adding))
                {
                    return Ok(());
                }
                // Of the config's own entry's spelling where the config has
                // one of the same keys, so that no second entry stays.
                let target = settings
                    .as_ref()
                    .and_then(|settings| {
                        settings
                            .keys
                            .iter()
                            .rev()
                            .find(|taken| !taken.default && same_keys(&taken.keys, &keys))
                    })
                    .map_or_else(|| keys.clone(), |taken| taken.keys.clone());
                match cx.set_setting(KEYS, None, &target, Some(Data::Text(command))) {
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
        let settings = page_settings(cx)?;
        let refusal = format!("binding:{at}");
        let warning = format!("{TAKEN}{at}");
        let (Some(binding), text) = (settings.keys.get(at), text.trim()) else {
            return Ok(());
        };
        let Some(command) = binding.command.as_ref().filter(|_| changes(binding)) else {
            return Ok(());
        };
        // A global key stays one.
        let keys = if binding.keys.starts_with(lokyr_plugin_sdk::keys::GLOBAL) {
            format!("{}{text}", lokyr_plugin_sdk::keys::GLOBAL)
        } else {
            text.to_owned()
        };
        if text.is_empty() || same_keys(&keys, &binding.keys) {
            self.editing = None;
            self.confirming = None;
            self.refused.remove(&refusal);
            self.refused.remove(&warning);
            return self.render_keys(cx);
        }
        // macOS's own keys, which no binding of the settings' takes.
        if let Ok(combos) = lokyr_plugin_sdk::keys::parse_keys(text)
            && lokyr_plugin_sdk::keys::is_system(&combos, None)
        {
            self.refused
                .insert(refusal, lokyr_plugin_sdk::keys::system_error(text));
            return self.render_keys(cx);
        }
        if let Err(why) = lokyr_plugin_sdk::keys::parse_keys(text) {
            self.refused.insert(refusal, why);
            return self.render_keys(cx);
        }
        // A default's old keys that others share, which `false` would take
        // from them too.
        if let Some(why) = shared_refusal(cx, &settings, at, binding) {
            self.refused.insert(refusal, why);
            return self.render_keys(cx);
        }
        // Keys that another binding takes: told, a second press of them
        // taking its place, of the config's own entry's spelling where it
        // is the config's, so that no second entry of the same keys stays.
        let taken = settings
            .keys
            .iter()
            .enumerate()
            .rev()
            .find(|(other, taken)| {
                *other != at && taken.command.is_some() && same_keys(&taken.keys, &keys)
            })
            .map(|(_, taken)| taken);
        let mut target = keys.clone();
        if let Some(taken) = taken {
            if self.confirming.as_ref() != Some(&(at, keys.clone())) {
                let shown =
                    lokyr_plugin_sdk::keys::macos_text(&keys).unwrap_or_else(|| keys.clone());
                let why = message(
                    cx,
                    "binding-taken-again",
                    &[("keys", shown), ("title", title_of(taken))],
                );
                self.confirming = Some((at, keys));
                self.refused.remove(&refusal);
                self.refused.insert(warning, why);
                return self.render_keys(cx);
            }
            if !taken.default {
                target.clone_from(&taken.keys);
            }
        }
        self.confirming = None;
        self.refused.remove(&warning);
        let written = cx.set_setting(KEYS, None, &target, Some(binding_value(binding, command)));
        if let Err(error) = written {
            self.refused.insert(refusal, error.message);
            return self.render_keys(cx);
        }
        let old = binding.default.then_some(Data::Boolean(false));
        let written = cx.set_setting(KEYS, None, &binding.keys, old);
        self.editing = None;
        self.refused.remove(&refusal);
        self.told(cx, KEYS, written)?;
        self.render_keys(cx)
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
            Ok(()) => {
                // A map written: its entries' texts that it refused go, of
                // their reason, their fields showing the values in force.
                let gone: Vec<String> = self
                    .refused_texts
                    .iter()
                    .filter(|node| {
                        self.typed
                            .get(*node)
                            .and_then(|typed| typed.entry.as_ref())
                            .is_some_and(|at| at.key == name)
                    })
                    .cloned()
                    .collect();
                for node in &gone {
                    self.typed.remove(node);
                    self.refused_texts.remove(node);
                }
                self.refused.remove(name).is_some() || !gone.is_empty()
            }
            Err(error) => {
                self.refused.insert(name.to_owned(), error.message);
                true
            }
        };
        // The page that wrote it, of the keys' or of the bindings'.
        if changed {
            self.render_page(cx)?;
            self.render_keys(cx)?;
        }
        Ok(())
    }
}

impl SettingsPage {
    /// Renders the page anew while it shows; as it hides, as of a text
    /// typed written then, no render is asked, which would fail the call
    /// and drop its writes.
    fn render_page(&self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.shown {
            cx.request_render(page())
        } else {
            Ok(())
        }
    }

    /// Renders the page of the keys anew while it shows, as
    /// [`render_page`](Self::render_page) does the page's.
    fn render_keys(&self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.keys_shown {
            cx.request_render(keys_page())
        } else {
            Ok(())
        }
    }

    /// Writes the texts typed into the fields and not written yet as typing
    /// pauses: each a value whole, as it writes it, that its key takes and
    /// that differs from its value in force; the others, an empty field or
    /// one half typed, as `13.` on the way to `13.5`, wait for Return or the
    /// field's leaving, of no reason told meanwhile.
    fn write_paused(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let settings = page_settings(cx)?;
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
            // An entry's field as typing pauses, of a value that it takes,
            // never its name, whose field the rename would take away.
            if let Some(at) = &typed.entry {
                if at.field.is_none() {
                    continue;
                }
                let Some(setting) = find(section, &at.key) else {
                    continue;
                };
                // Of a value whole, as the field shows it: never a text
                // that a space ends, as one typed, nor a number's part.
                if !whole_in_field(cx, setting, at, &typed.text) {
                    continue;
                }
                if let Ok(Some(value)) = entry_written(cx, setting, at, &typed.text) {
                    let written = cx.set_setting(
                        &section.name,
                        section.plugin.as_deref(),
                        &setting.name,
                        value,
                    );
                    if written.is_ok() {
                        self.typed.remove(&name);
                        self.refused_texts.remove(&name);
                        if self.refused.remove(&at.key).is_some() {
                            self.render_page(cx)?;
                        }
                    }
                }
                continue;
            }
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
                    self.render_page(cx)?;
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
        let settings = page_settings(cx)?;
        for (name, typed) in pending {
            let Some(section) = settings
                .sections
                .iter()
                .find(|section| section.name == typed.section && section.plugin == typed.plugin)
            else {
                continue;
            };
            // An entry's field, or its name, of the value that it takes, or
            // why not under its map.
            if let Some(at) = &typed.entry {
                let Some(setting) = find(section, &at.key) else {
                    continue;
                };
                match entry_written(cx, setting, at, &typed.text) {
                    Ok(Some(value)) => {
                        self.refused_texts.remove(&name);
                        self.write(cx, section, setting, value)?;
                    }
                    Ok(None) => {
                        self.refused_texts.remove(&name);
                        if self.refused.remove(&at.key).is_some() {
                            self.render_page(cx)?;
                        }
                    }
                    Err(why) => {
                        self.refused.insert(at.key.clone(), why);
                        self.refused_texts.insert(name.clone());
                        self.typed.insert(name, typed.clone());
                        self.render_page(cx)?;
                    }
                }
                continue;
            }
            let Some(setting) = find(section, &name) else {
                continue;
            };
            // Once of a value: none of the value in force.
            let text = typed.text.trim();
            let now = setting.value.as_ref().map(plain).unwrap_or_default();
            if text == now || (text.is_empty() && setting.value == setting.default) {
                if self.refused.remove(&name).is_some() {
                    self.render_page(cx)?;
                }
                continue;
            }
            // A text refused stays in its field, beside why, to be mended.
            match value_of(cx, &setting.kind, text) {
                Ok(value) => {
                    self.write(cx, section, setting, value)?;
                    if self.refused.contains_key(&name) {
                        self.typed.insert(name, typed);
                    }
                }
                Err(why) => {
                    self.refused.insert(name.clone(), why);
                    self.typed.insert(name, typed);
                    self.render_page(cx)?;
                }
            }
        }
        Ok(())
    }

    /// An event of a map's editor in `section`: an entry taken away, a text
    /// typed into an entry's field or its name, written as the page's other
    /// fields write theirs, its name as Return is pressed or as it is left
    /// alone, or the new entry's fields typed into and added.
    fn entry_event(
        &mut self,
        cx: &mut Cx<'_>,
        part: &Part<'_>,
        event: &UiEvent,
    ) -> Result<(), Error> {
        let key = event.key.0.as_str();
        let (prefix, rest) = [ENTRY_REMOVE, ENTRY_NAME, ENTRY_FIELD, NEW_ENTRY, ADD_ENTRY]
            .into_iter()
            .find_map(|prefix| key.strip_prefix(prefix).map(|rest| (prefix, rest)))
            .unwrap_or_default();
        let mut parts = rest.split(PART);
        let Some((section, setting)) = parts.next().and_then(|name| part.find(name)) else {
            return Ok(());
        };
        let entry = parts.next().map(str::to_owned);
        let field = parts.next().map(str::to_owned);
        match (prefix, &event.kind) {
            (ENTRY_REMOVE, UiEventKind::Click) => {
                let Some(entry) = entry else {
                    return Ok(());
                };
                let mut entries = entries_of(setting.value.as_ref());
                if entries.remove(&entry).is_none() {
                    return Ok(());
                }
                self.typed.retain(|_, typed| {
                    typed
                        .entry
                        .as_ref()
                        .is_none_or(|at| at.key != setting.name || at.entry != entry)
                });
                self.write(cx, section, setting, map_value(entries))
            }
            (ENTRY_NAME | ENTRY_FIELD, kind) => {
                let Some(entry) = entry else {
                    return Ok(());
                };
                let at = EntryAt {
                    key: setting.name.clone(),
                    entry,
                    field: if prefix == ENTRY_FIELD { field } else { None },
                };
                let typed = |text: &str| Typed {
                    section: section.name.clone(),
                    plugin: section.plugin.clone(),
                    text: text.to_owned(),
                    entry: Some(at.clone()),
                };
                match kind {
                    UiEventKind::InputChanged { text, .. } => {
                        self.typed.insert(key.to_owned(), typed(text));
                        // A name waits for Return or its field left.
                        if at.field.is_some() {
                            if let Some(pause) = self.pause.take() {
                                cx.cancel_timer(pause)?;
                            }
                            self.pause = Some(cx.start_timer(PAUSE, false)?);
                        }
                        Ok(())
                    }
                    UiEventKind::Submit { text, .. } => {
                        self.typed.insert(key.to_owned(), typed(text));
                        self.write_typed(cx, Some(key))
                    }
                    UiEventKind::Blur => self.write_typed(cx, Some(key)),
                    _ => Ok(()),
                }
            }
            (NEW_ENTRY, UiEventKind::InputChanged { text, .. }) => {
                self.new_entries
                    .entry(setting.name.clone())
                    .or_default()
                    .insert(entry.unwrap_or_default(), text.clone());
                Ok(())
            }
            (NEW_ENTRY, UiEventKind::Submit { text, .. }) => {
                self.new_entries
                    .entry(setting.name.clone())
                    .or_default()
                    .insert(entry.unwrap_or_default(), text.clone());
                self.add_entry(cx, section, setting)
            }
            (ADD_ENTRY, UiEventKind::Click) => self.add_entry(cx, section, setting),
            _ => Ok(()),
        }
    }

    /// Adds the new entry of the map `setting` of `section`, of the texts
    /// typed into its fields: of a name that no other entry has, and of its
    /// required fields, each of a value that its kind takes; or why not,
    /// under the map. Added, its fields are empty again, for the next.
    fn add_entry(
        &mut self,
        cx: &mut Cx<'_>,
        section: &SettingsSection,
        setting: &SettingKey,
    ) -> Result<(), Error> {
        let SettingKind::Map { fields } = &setting.kind else {
            return Ok(());
        };
        let texts = self
            .new_entries
            .get(&setting.name)
            .cloned()
            .unwrap_or_default();
        let text = |field: &str| texts.get(field).map_or("", |text| text.trim());
        let mut entries = entries_of(setting.value.as_ref());
        let name = text("");
        let mut own = BTreeMap::new();
        let refused = if name.is_empty() {
            Some(message(cx, "setting-entry-unnamed", &[]))
        } else if entries.contains_key(name) {
            Some(message(
                cx,
                "setting-entry-taken",
                &[("name", name.to_owned())],
            ))
        } else {
            let mut refused = None;
            for field in fields {
                match field_value(cx, &field.kind, text(&field.name)) {
                    Ok(Some(value)) => {
                        own.insert(field.name.clone(), value);
                    }
                    Ok(None) if field.required => {
                        refused = Some(message(
                            cx,
                            "setting-entry-required",
                            &[("field", field.name.clone())],
                        ));
                    }
                    Ok(None) => {}
                    Err(why) => refused = Some(format!("{}: {why}", field.name)),
                }
                if refused.is_some() {
                    break;
                }
            }
            refused
        };
        if let Some(why) = refused {
            self.refused.insert(setting.name.clone(), why);
            return self.render_page(cx);
        }
        entries.insert(name.to_owned(), own);
        let written = cx.set_setting(
            &section.name,
            section.plugin.as_deref(),
            &setting.name,
            map_value(entries),
        );
        let fine = written.is_ok();
        self.told(cx, &setting.name, written)?;
        if fine {
            self.new_entries.remove(&setting.name);
            for field in std::iter::once("").chain(fields.iter().map(|field| field.name.as_str())) {
                let key = format!("{NEW_ENTRY}{}{PART}{field}", setting.name);
                cx.set_text(page(), key.as_str(), "", u64::MAX)?;
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
/// key of its own.
fn shown_parts(cx: &mut Cx<'_>, settings: &Settings, words: &[String]) -> Vec<usize> {
    parts(settings)
        .into_iter()
        .enumerate()
        .filter(|(_, part)| {
            words.is_empty()
                || part.sections().any(|section| {
                    section.keys.iter().any(|key| {
                        key_holds(cx, section, key, words)
                            || key_holds(cx, part.section, key, words)
                    })
                })
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

/// The most decimals that a number's step keeps.
const MOST_DECIMALS: i32 = 6;

/// The decimals that `number` writes, as its shortest text has them, the
/// most of one in an exponent's form.
fn decimals(number: f64) -> i32 {
    let text = number.to_string();
    if text.contains(['e', 'E']) {
        return MOST_DECIMALS;
    }
    text.split_once('.').map_or(0, |(_, fraction)| {
        i32::try_from(fraction.len()).unwrap_or(MOST_DECIMALS)
    })
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
    // Of the decimals of the step and of the value, as 0.95 less 0.05 is
    // 0.9, not 0.8999999999999999.
    let decimals = decimals(step).max(decimals(now)).min(MOST_DECIMALS);
    let scale = 10f64.powi(decimals);
    next = (next * scale).round() / scale;
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
    words: &[String],
    (refused, typed, new_entries): Texts<'_>,
) -> Node {
    let shown = shown_parts(cx, settings, words);
    let parts = parts(settings);
    let chosen_part = parts.get(chosen).filter(|_| shown.contains(&chosen));
    let mut part = match chosen_part {
        Some(part) => section_part(cx, part, words, (refused, typed, new_entries)),
        None if !words.is_empty() => vec![empty(
            "search",
            (message(cx, "settings-no-match", &[]), "no-match"),
            message(cx, "settings-no-match-line", &[]),
        )],
        None => vec![
            Node::text(message(cx, "settings-none", &[]))
                .key("none")
                .style(secondary()),
        ],
    };
    // A file that does not read: its error, over every part.
    if let Some(error) = file_error(cx, settings) {
        part.insert(0, error);
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
    Node::container(vec![sidebar(cx, settings, &shown), Node::separator(), part])
        .key("settings-page")
        .layout(Layout {
            direction: Direction::Row,
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        })
}

/// The error of a config file that does not read, which every page shows
/// over its parts, of its line and column where it tells them.
fn file_error(cx: &mut Cx<'_>, settings: &Settings) -> Option<Node> {
    let problem = settings.error.as_ref()?;
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
    Some(callout(Tone::Warning, &text, "file-error").role(Role::Group))
}

/// The page of the bindings of keys, of `settings`: their part, of the
/// filter's `words`, the binding of `editing` recording, each write refused
/// of why, `refused`, down a column that scrolls, the error of a file that
/// does not read over it.
fn keys_tree(
    cx: &mut Cx<'_>,
    settings: &Settings,
    words: &[String],
    editing: Option<usize>,
    refused: &BTreeMap<String, String>,
) -> Node {
    let mut part = keys_part(cx, &settings.keys, words, editing, refused);
    if let Some(error) = file_error(cx, settings) {
        part.insert(0, error);
    }
    Node::scroll(vec![Node::container(part).key("part").layout(Layout {
        direction: Direction::Column,
        gap: 10.0,
        padding: Edges::all(20.0),
        width: Length::Fraction(1.0),
        ..Layout::default()
    })])
    .key("keys-page")
    .layout(Layout {
        direction: Direction::Column,
        width: Length::Fraction(1.0),
        height: Length::Fraction(1.0),
        ..Layout::default()
    })
}

/// The sidebar: a list of the sections, each a row of its icon and its
/// title, of a menu's look, its first selected as it shows, of no ring of
/// the keyboard's focus, as macOS's sidebars show none, and of its own
/// width alone.
fn sidebar(cx: &mut Cx<'_>, settings: &Settings, shown: &[usize]) -> Node {
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
            right: SIDE_MARGIN + BAND_PADDING.1,
            bottom: 0.0,
            left: SIDE_MARGIN + BAND_PADDING.0,
        },
        ..Layout::default()
    });
    let parts = parts(settings);
    let shown_parts: Vec<&Part<'_>> = shown.iter().filter_map(|&at| parts.get(at)).collect();
    // A line between Lokyr's own sections and the other plugins', over the
    // first of those, where one of Lokyr's shows before it.
    let first_other = shown_parts
        .iter()
        .position(|part| !is_own(part.section))
        .filter(|&at| at > 0);
    let items: Vec<ListItem> = shown_parts
        .iter()
        .enumerate()
        .map(|(at, part)| {
            let divider = first_other == Some(at);
            ListItem {
                texts: vec![icon_of(part.section).to_owned(), title(cx, part.section)],
                divider,
            }
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
    // The search at the sidebar's top, as macOS's search field.
    let search = search_field(cx, SEARCH, "settings-search").layout(Layout {
        width: Length::Fraction(1.0),
        ..Layout::default()
    });
    // The field as far from the sidebar's edges as the rows' bands, of a
    // list as wide as the sidebar.
    let search = Node::container(vec![search]).layout(Layout {
        width: Length::Fraction(1.0),
        padding: Edges {
            top: 0.0,
            right: SIDE_MARGIN,
            bottom: 0.0,
            left: SIDE_MARGIN,
        },
        ..Layout::default()
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
            padding: Edges {
                top: 6.0,
                right: 0.0,
                bottom: 6.0,
                left: 0.0,
            },
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
            "blocks" => "pin",
            "quick-tips" => "lightbulb",
            "links" => "link",
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
        "sessions" => "folder",
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

/// The chosen part: its section's title, its plugin's name, what it holds,
/// and its keys in a group, each write refused of why, `refused`; then
/// each plugin's section that it shows, of a heading of its title, its
/// plugin's name, what it holds and its keys.
fn section_part(
    cx: &mut Cx<'_>,
    shown: &Part<'_>,
    words: &[String],
    (refused, typed, new_entries): Texts<'_>,
) -> Vec<Node> {
    let section = shown.section;
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
    // A replaced plugin's keys, which apply once its replacer is off.
    if let Some(replacer) = &section.replaced_by {
        let text = message(cx, "settings-replaced", &[("plugin", replacer.clone())]);
        part.push(callout(Tone::Info, &text, "replaced"));
    }
    // The switch of its plugin, whose keys dim while it is off.
    if let Some(switch) = plugin_switch(cx, section, refused) {
        part.push(switch);
    }
    let off = section.enabled == Some(false);
    // The keys that the search's words show, every one of none.
    let keys: Vec<&SettingKey> = section
        .keys
        .iter()
        .filter(|key| words.is_empty() || key_holds(cx, section, key, words))
        .collect();
    let rows: Vec<Node> = keys
        .into_iter()
        .map(|key| {
            let row = key_row(
                cx,
                (section, key),
                (refused.get(&key.name), typed, new_entries),
            );
            if off { dimmed(row) } else { row }
        })
        .collect();
    // Of none that the search shows, no group, as the joined sections'
    // keys may be the part's alone that it shows.
    if !rows.is_empty() || words.is_empty() {
        part.push(group(rows, "keys"));
    }
    for joined in &shown.joined {
        let keys: Vec<&SettingKey> = joined
            .keys
            .iter()
            .filter(|key| {
                words.is_empty()
                    || key_holds(cx, joined, key, words)
                    || key_holds(cx, section, key, words)
            })
            .collect();
        if keys.is_empty() {
            continue;
        }
        let at = |what: &str| format!("{what}:{}", joined.name);
        part.push(
            Node::text(title(cx, joined))
                .key(at("joined-title").as_str())
                .role(Role::Heading)
                .style(Style {
                    font_weight: Some(FontWeight::Bold),
                    ..Style::default()
                })
                .layout(Layout {
                    padding: Edges {
                        top: 12.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 0.0,
                    },
                    ..Layout::default()
                }),
        );
        if let Some(plugin) = &joined.plugin {
            part.push(
                Node::text(message(
                    cx,
                    "settings-plugin",
                    &[("plugin", plugin.clone())],
                ))
                .key(at("joined-plugin").as_str())
                .style(small()),
            );
        }
        if !joined.description.is_empty() {
            part.push(
                Node::text(joined.description.clone())
                    .key(at("joined-description").as_str())
                    .style(secondary())
                    .layout(wrapped()),
            );
        }
        if let Some(switch) = plugin_switch(cx, joined, refused) {
            part.push(switch);
        }
        let off = joined.enabled == Some(false);
        let rows = keys
            .into_iter()
            .map(|key| {
                let row = key_row(
                    cx,
                    (joined, key),
                    (refused.get(&key.name), typed, new_entries),
                );
                if off { dimmed(row) } else { row }
            })
            .collect();
        part.push(group(rows, &at("keys")));
    }
    part
}

/// The row of `key` of `section`: its name, its platforms and what it does, why its last
/// write was refused, `refused`, and at its right the control of its value
/// in force, or of the text typed into its field and not written yet, of
/// the texts typed, `typed`, marked as its default where it is, or that it
/// is not set, and the button that takes it back to its default where it
/// is another.
fn key_row(
    cx: &mut Cx<'_>,
    (section, key): (&SettingsSection, &SettingKey),
    (refused, typed, new_entries): (Option<&String>, &BTreeMap<String, Typed>, &NewEntries),
) -> Node {
    let name = &key.name;
    let all_typed = typed;
    let typed = typed.get(name).map(|typed| typed.text.as_str());
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
    // Its text in the guide, and the documentation's page of its topic,
    // which a browser opens.
    let link = |text: String, key: String| {
        Node::button(text).key(key.as_str()).as_link().style(Style {
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
    };
    let mut links = Vec::new();
    if key.guide.is_some() {
        links.push(link(
            message(cx, "setting-more-info", &[]),
            format!("{GUIDE}{name}"),
        ));
    }
    if key.docs.is_some() {
        links.push(link(
            message(cx, "setting-docs", &[]),
            format!("{DOCS}{name}"),
        ));
    }
    if !links.is_empty() {
        about.push(row_of(links, 12.0));
    }
    // A list's values, or a map's entries, under what it does.
    if matches!(key.kind, SettingKind::List { .. }) {
        about.push(list_editor(cx, key));
    }
    if let SettingKind::Map { fields } = &key.kind {
        let empty = BTreeMap::new();
        let new_texts = new_entries.get(&key.name).unwrap_or(&empty);
        about.push(map_editor(cx, key, fields, (all_typed, new_texts)));
    }
    if let Some(why) = refused {
        about.push(refusal(why, &format!("refused:{name}")));
    }
    let about = Node::container(about).layout(Layout {
        direction: Direction::Column,
        gap: 3.0,
        grow: 1.0,
        min_width: Length::Points(0.0),
        ..Layout::default()
    });
    let mut value: Vec<Node> = control(cx, (section, key), typed).into_iter().collect();
    // At the bottom right: the link back to the default, of none where the
    // key is there already.
    let mut marks = Vec::new();
    let at_default = key.value.is_none() || key.value == key.default;
    // A map's entries go one at a time, by their Remove.
    if writable(&key.kind) && !matches!(key.kind, SettingKind::Map { .. }) && !at_default {
        let reset = Node::button(message(cx, "setting-reset", &[]))
            .key(format!("{RESET}{name}").as_str())
            .as_link()
            .style(Style {
                text_size: Some(TextSize::Small),
                ..Style::default()
            });
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

/// The group of the switch that turns the plugin of `section` on or off,
/// as its section's `enabled` does, of what it does and why its last write
/// was refused, of `refused`; none of an app's section.
fn plugin_switch(
    cx: &mut Cx<'_>,
    section: &SettingsSection,
    refused: &BTreeMap<String, String>,
) -> Option<Node> {
    let on = section.enabled?;
    section.plugin.as_ref()?;
    let id = format!("{PLUGIN_ON}{}", section.name);
    let name = title(cx, section);
    let mut about = vec![
        Node::text(message(cx, "settings-plugin-on", &[])).style(Style {
            font_weight: Some(FontWeight::Semibold),
            ..Style::default()
        }),
        Node::text(message(
            cx,
            "settings-plugin-on-about",
            &[("plugin", name.clone())],
        ))
        .key(format!("{id}:about").as_str())
        .style(small())
        .layout(wrapped()),
    ];
    if let Some(why) = refused.get(&id) {
        about.push(refusal(why, &format!("{id}:refused")));
    }
    let about = Node::container(about).layout(Layout {
        direction: Direction::Column,
        gap: 3.0,
        grow: 1.0,
        min_width: Length::Points(0.0),
        ..Layout::default()
    });
    let mut switch = Node::switch("", on).key(id.as_str()).label(name);
    if let Kind::Switch(own) = &mut switch.kind {
        own.size = ControlSize::Small;
    }
    let row = Node::container(vec![about, switch])
        .role(Role::Group)
        .layout(Layout {
            direction: Direction::Row,
            gap: 12.0,
            align: Align::Start,
            width: Length::Fraction(1.0),
            padding: Edges {
                top: 10.0,
                right: 12.0,
                bottom: 10.0,
                left: 12.0,
            },
            ..Layout::default()
        });
    Some(group(vec![row], &format!("{id}:group")))
}

/// `row`, a key's row of a plugin that is off: its controls disabled, so
/// that nothing changes its value, and its texts dimmed, its links to the
/// guide and to the documentation kept.
fn dimmed(mut row: Node) -> Node {
    fn walk(node: &mut Node) {
        let link = node
            .key
            .as_ref()
            .is_some_and(|key| key.0.starts_with(GUIDE) || key.0.starts_with(DOCS));
        match &mut node.kind {
            _ if link => {}
            Kind::Button(button) => button.disabled = true,
            Kind::Toggle(toggle) => toggle.disabled = true,
            Kind::Switch(switch) => switch.disabled = true,
            Kind::Checkbox(checkbox) => checkbox.disabled = true,
            Kind::Radios(radios) => radios.disabled = true,
            Kind::PopUp(pop_up) => pop_up.disabled = true,
            Kind::Field(field) => field.disabled = true,
            Kind::Text(_) => node.style.color = Some(Token::from("text.disabled")),
            _ => {}
        }
        for child in &mut node.children {
            walk(child);
        }
    }
    walk(&mut row);
    row
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

/// The entries of a map's value, `value`, each of its fields by their
/// names; none of another value.
fn entries_of(value: Option<&Data>) -> BTreeMap<String, BTreeMap<String, Data>> {
    match value {
        Some(Data::Map(entries)) => entries
            .iter()
            .filter_map(|(name, entry)| match entry {
                Data::Map(fields) => Some((name.clone(), fields.clone())),
                _ => None,
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// The value of a map of `entries`: none of no entry, so that the key goes
/// back to its default, an empty map, and leaves the file.
fn map_value(entries: BTreeMap<String, BTreeMap<String, Data>>) -> Option<Data> {
    (!entries.is_empty()).then(|| {
        Data::Map(
            entries
                .into_iter()
                .map(|(name, fields)| (name, Data::Map(fields)))
                .collect(),
        )
    })
}

/// The value of a field of a map's entry of the kind `kind`, of `text`:
/// none of an empty one, `true` or `false` of a boolean, a number of a
/// number, a text of another kind; or why not.
fn field_value(cx: &mut Cx<'_>, kind: &SettingKind, text: &str) -> Result<Option<Data>, String> {
    match (kind, text.trim()) {
        (SettingKind::Boolean, "") => Ok(None),
        (SettingKind::Boolean, "true") => Ok(Some(Data::Boolean(true))),
        (SettingKind::Boolean, "false") => Ok(Some(Data::Boolean(false))),
        (SettingKind::Boolean, _) => Err(message(cx, "setting-not-a-boolean", &[])),
        _ => value_of(cx, kind, text),
    }
}

/// Whether `text`, typed into the field of `at` of the map `setting`, is a
/// value whole as the field shows it: none of an empty one, or one whose
/// text is `text` itself, never a text that a space ends nor a number's
/// part, as `13.`, which typing goes on from.
fn whole_in_field(cx: &mut Cx<'_>, setting: &SettingKey, at: &EntryAt, text: &str) -> bool {
    let SettingKind::Map { fields } = &setting.kind else {
        return false;
    };
    let Some(field) = at
        .field
        .as_ref()
        .and_then(|name| fields.iter().find(|own| own.name == *name))
    else {
        return false;
    };
    match field_value(cx, &field.kind, text) {
        Ok(None) => text.is_empty(),
        Ok(Some(value)) => plain(&value) == text,
        Err(_) => false,
    }
}

/// The value that the map `setting` takes of `text`, typed into the field
/// of `at`: its entry renamed, or its field changed, an empty one of no
/// value taken away; none where nothing changes, or where the entry is no
/// longer there; or why not, as an empty name, a name that another entry
/// has, or an empty field that every entry has.
fn entry_written(
    cx: &mut Cx<'_>,
    setting: &SettingKey,
    at: &EntryAt,
    text: &str,
) -> Result<Option<Option<Data>>, String> {
    let SettingKind::Map { fields } = &setting.kind else {
        return Ok(None);
    };
    let mut entries = entries_of(setting.value.as_ref());
    let Some(mut own) = entries.remove(&at.entry) else {
        return Ok(None);
    };
    let text = text.trim();
    match &at.field {
        None => {
            if text == at.entry {
                return Ok(None);
            }
            if text.is_empty() {
                return Err(message(cx, "setting-entry-unnamed", &[]));
            }
            if entries.contains_key(text) {
                return Err(message(
                    cx,
                    "setting-entry-taken",
                    &[("name", text.to_owned())],
                ));
            }
            entries.insert(text.to_owned(), own);
        }
        Some(field) => {
            let Some(kind) = fields.iter().find(|own| own.name == *field) else {
                return Ok(None);
            };
            match field_value(cx, &kind.kind, text) {
                Err(why) => return Err(format!("{field}: {why}")),
                Ok(None) if kind.required => {
                    return Err(message(
                        cx,
                        "setting-entry-required",
                        &[("field", field.clone())],
                    ));
                }
                Ok(None) => {
                    if own.remove(field).is_none() {
                        return Ok(None);
                    }
                }
                Ok(Some(value)) => {
                    if own.get(field) == Some(&value) {
                        return Ok(None);
                    }
                    own.insert(field.clone(), value);
                }
            }
            entries.insert(at.entry.clone(), own);
        }
    }
    Ok(Some(map_value(entries)))
}

/// A map's entries, each of its name's field, its Remove, and a field of
/// each of its fields, of the text typed into it and not written yet, of
/// `typed`, or its value; then the fields of a new entry, its name's and
/// each field's, and Add.
fn map_editor(
    cx: &mut Cx<'_>,
    key: &SettingKey,
    fields: &[SettingField],
    (typed, new_texts): (&BTreeMap<String, Typed>, &BTreeMap<String, String>),
) -> Node {
    let name = &key.name;
    let shown = |node: &str| typed.get(node).map(|typed| typed.text.clone());
    let placeholders: Vec<String> = fields
        .iter()
        .map(|field| {
            let id = if field.required {
                "setting-entry-field-required"
            } else {
                "setting-entry-field-optional"
            };
            message(cx, id, &[])
        })
        .collect();
    // A field's row: its name, which tells what it does as the pointer
    // rests on it, of a key of its field's, and its field.
    let field_row = |field: &SettingField, node: Node| {
        let mut label = Node::text(field.name.clone())
            .style(small())
            .layout(Layout {
                width: Length::Points(56.0),
                ..Layout::default()
            });
        if !field.description.is_empty()
            && let Some(key) = &node.key
        {
            label = label
                .key(format!("label:{}", key.0).as_str())
                .tooltip(field.description.as_str());
        }
        Node::container(vec![label, node]).layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            width: Length::Fraction(1.0),
            ..Layout::default()
        })
    };
    let grown = || Layout {
        grow: 1.0,
        min_width: Length::Points(0.0),
        ..Layout::default()
    };
    let mut rows = Vec::new();
    for (entry, values) in entries_of(key.value.as_ref()) {
        let named = format!("{ENTRY_NAME}{name}{PART}{entry}");
        let mut block = vec![
            row_of(
                vec![
                    Node::field(message(cx, "setting-entry-name", &[]))
                        .key(named.as_str())
                        .label(message(
                            cx,
                            "setting-entry-name-label",
                            &[("key", name.clone())],
                        ))
                        .holding(shown(&named).unwrap_or_else(|| entry.clone()))
                        .style(Style {
                            font_weight: Some(FontWeight::Semibold),
                            ..field_text()
                        })
                        .layout(Layout {
                            width: Length::Points(180.0),
                            ..Layout::default()
                        }),
                    Node::container(Vec::new()).layout(grown()),
                    Node::button(message(cx, "setting-list-remove", &[]))
                        .key(format!("{ENTRY_REMOVE}{name}{PART}{entry}").as_str())
                        .label(message(
                            cx,
                            "setting-entry-remove-label",
                            &[("name", entry.clone())],
                        ))
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
            }),
        ];
        for (field, placeholder) in fields.iter().zip(&placeholders) {
            let node = format!("{ENTRY_FIELD}{name}{PART}{entry}{PART}{}", field.name);
            let text = shown(&node)
                .or_else(|| values.get(&field.name).map(plain))
                .unwrap_or_default();
            block.push(field_row(
                field,
                Node::field(placeholder.clone())
                    .key(node.as_str())
                    .label(format!("{entry} {}", field.name))
                    .holding(text)
                    .style(field_text())
                    .layout(grown()),
            ));
        }
        rows.push(
            Node::container(block)
                .key(format!("entry:{name}{PART}{entry}").as_str())
                .role(Role::Group)
                .label(entry.clone())
                .layout(Layout {
                    direction: Direction::Column,
                    gap: 4.0,
                    width: Length::Fraction(1.0),
                    ..Layout::default()
                }),
        );
    }
    // The new entry, of its name and its fields, and Add.
    let mut block = vec![
        row_of(
            vec![
                Node::field(message(cx, "setting-entry-new", &[]))
                    .key(format!("{NEW_ENTRY}{name}{PART}").as_str())
                    .holding(new_texts.get("").cloned().unwrap_or_default())
                    .label(message(
                        cx,
                        "setting-entry-new-label",
                        &[("key", name.clone())],
                    ))
                    .style(field_text())
                    .layout(Layout {
                        width: Length::Points(180.0),
                        ..Layout::default()
                    }),
                Node::container(Vec::new()).layout(grown()),
                Node::button(message(cx, "setting-entry-add", &[]))
                    .key(format!("{ADD_ENTRY}{name}").as_str()),
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
    ];
    for (field, placeholder) in fields.iter().zip(&placeholders) {
        block.push(field_row(
            field,
            Node::field(placeholder.clone())
                .key(format!("{NEW_ENTRY}{name}{PART}{}", field.name).as_str())
                .holding(new_texts.get(&field.name).cloned().unwrap_or_default())
                .label(message(
                    cx,
                    "setting-entry-new-field-label",
                    &[("field", field.name.clone())],
                ))
                .style(field_text())
                .layout(grown()),
        ));
    }
    rows.push(
        Node::container(block)
            .key(format!("new-entry:{name}").as_str())
            .role(Role::Group)
            .label(message(cx, "setting-entry-new", &[]))
            .layout(Layout {
                direction: Direction::Column,
                gap: 4.0,
                width: Length::Fraction(1.0),
                ..Layout::default()
            }),
    );
    Node::container(rows)
        .key(format!("map:{name}").as_str())
        .layout(Layout {
            direction: Direction::Column,
            gap: 12.0,
            width: Length::Fraction(1.0),
            padding: Edges {
                top: 6.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            ..Layout::default()
        })
}

/// The control of `key`'s value, of `section`: a switch of a boolean, of
/// the key's name to accessibility, the pop-up button of a menu of its
/// values, a field of a number or of a text; another value's text, as the
/// config writes it; none of a list, whose values show under what it does.
fn control(
    cx: &mut Cx<'_>,
    (section, key): (&SettingsSection, &SettingKey),
    typed: Option<&str>,
) -> Option<Node> {
    let name = &key.name;
    let terminal = code();
    Some(match &key.kind {
        SettingKind::Boolean => {
            let mut switch = Node::switch("", key.value == Some(Data::Boolean(true)))
                .key(format!("{SET}{name}").as_str())
                .label(name.clone());
            if let Kind::Switch(own) = &mut switch.kind {
                own.size = ControlSize::Small;
            }
            switch
        }
        SettingKind::Choice { .. } => menu_pop_up(cx, section, key),
        SettingKind::Text { .. } if menu_control(key) => {
            let button = menu_pop_up(cx, section, key);
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
            let text = typed.map_or_else(
                || key.value.as_ref().map(plain).unwrap_or_default(),
                str::to_owned,
            );
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
            let text = typed.map_or_else(
                || key.value.as_ref().map(plain).unwrap_or_default(),
                str::to_owned,
            );
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
        // A list's values, and a map's entries, show under what it does.
        SettingKind::List { .. } | SettingKind::Map { .. } => return None,
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

/// The pop-up button of the menu of values of `key` of `section`, small as
/// the page's controls, its value in force chosen; of the value itself
/// where none of the menu's is it, or of no value; of a field's width where
/// its control lists values of their own, as fonts or themes, so that the
/// longest of them does not widen it.
fn menu_pop_up(cx: &mut Cx<'_>, section: &SettingsSection, key: &SettingKey) -> Node {
    let values = menu_values(cx, section, key).unwrap_or_default();
    let chosen = key
        .value
        .as_ref()
        .and_then(|value| values.iter().position(|item| item == value));
    let placeholder = key
        .value
        .as_ref()
        .map_or_else(|| message(cx, "setting-unset", &[]), plain);
    let mut pop_up = Node::pop_up(values.iter().map(plain).collect(), chosen)
        .key(format!("{CHOOSE}{}", key.name).as_str())
        .label(key.name.clone());
    if let Kind::PopUp(own) = &mut pop_up.kind {
        own.placeholder = placeholder;
        own.size = ControlSize::Small;
    }
    if menu_control(key) {
        pop_up = pop_up.layout(Layout {
            width: Length::Points(180.0),
            ..Layout::default()
        });
    }
    pop_up
}

/// A search field as macOS's, of the key `field` and the message of its
/// placeholder, `placeholder`, of the small text of the page's fields: the
/// host draws its magnifying glass and the button that empties it, which
/// tells an empty text, as Escape does.
fn search_field(cx: &mut Cx<'_>, field: &str, placeholder: &str) -> Node {
    let text = message(cx, placeholder, &[]);
    Node::field(text.clone())
        .key(field)
        .label(text)
        .search()
        .style(field_text())
}

/// The part of the bindings of keys: every one in force, the defaults and
/// the config's own, and the keys that the config takes away, of those
/// that the filter's words, `filter`, show, by their commands' categories, each of
/// its keys as macOS writes them, which a click lets change in a field,
/// the one of `editing`, its command's title, its name, arguments and
/// context, and a link that takes it away or gives a default back; then
/// the fields of a new binding.
fn keys_part(
    cx: &mut Cx<'_>,
    bindings: &[SettingBinding],
    filter: &[String],
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
        // The filter of the bindings, as the sidebar's search, beside Add.
        Node::container(vec![
            search_field(cx, KEYS_FILTER, "keys-filter").layout(Layout {
                width: Length::Points(280.0),
                min_width: Length::Points(0.0),
                ..Layout::default()
            }),
            Node::button(message(cx, "binding-add", &[])).key(ADD_BINDING),
        ])
        .key(format!("{ADD_BINDING}-row").as_str())
        .layout(Layout {
            direction: Direction::Row,
            justify: Justify::SpaceBetween,
            align: Align::Center,
            gap: 12.0,
            width: Length::Fraction(1.0),
            ..Layout::default()
        }),
    ];
    let terminal = code();
    let shown: Vec<(usize, &SettingBinding)> = bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| filter.is_empty() || binding_holds(cx, binding, filter))
        .collect();
    if shown.is_empty() && !filter.is_empty() {
        part.push(empty(
            "keyboard",
            (message(cx, "keys-filter-none", &[]), "keys-no-match"),
            message(cx, "keys-filter-none-line", &[]),
        ));
    }
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

/// Whether the keys of `binding` change in the Keys part: a command's,
/// alone or of its arguments and context; neither macOS's own, of its app
/// menu, nor `false`, nor the config's commands of several contexts, as
/// the file tells them.
fn changes(binding: &SettingBinding) -> bool {
    binding.command.is_some() && !binding.system && (binding.default || !binding.detailed)
}

/// Whether `a` and `b` are the same keys, as they parse, whatever their
/// spelling, as `cmd+}` and `cmd+shift+]`, both global or neither.
fn same_keys(a: &str, b: &str) -> bool {
    let parsed = |keys: &str| match keys.strip_prefix(lokyr_plugin_sdk::keys::GLOBAL) {
        Some(keys) => (lokyr_plugin_sdk::keys::parse_keys(keys).ok(), true),
        None => (lokyr_plugin_sdk::keys::parse_keys(keys).ok(), false),
    };
    match (parsed(a), parsed(b)) {
        ((Some(a), one), (Some(b), two)) => one == two && lokyr_plugin_sdk::keys::same_keys(&a, &b),
        _ => false,
    }
}

/// The binding of `settings` that runs `keys` now: of those of a command,
/// the last, as the last wins.
fn running<'a>(settings: &'a Settings, keys: &str) -> Option<&'a SettingBinding> {
    settings
        .keys
        .iter()
        .rev()
        .find(|binding| binding.command.is_some() && same_keys(&binding.keys, keys))
}

/// Whether the new binding of `adding` is the one that runs its keys now,
/// of the same command, of no arguments nor context: Add would write
/// nothing new.
fn runs_already(settings: &Settings, adding: &Adding) -> bool {
    let (Some(keys), Some(command)) = (adding.keys.as_deref(), adding.command.as_deref()) else {
        return false;
    };
    running(settings, keys).is_some_and(|binding| {
        binding.command.as_deref() == Some(command)
            && binding.args.is_empty()
            && binding.when.is_none()
    })
}

/// What `binding` does, as its row tells it: its command's title, or its
/// command.
fn title_of(binding: &SettingBinding) -> String {
    binding
        .title
        .clone()
        .or_else(|| binding.command.clone())
        .unwrap_or_default()
}

/// Why the keys of the default `binding`, at `at` among the bindings of
/// `settings`, cannot be taken away: the other defaults of the same keys,
/// in their contexts, which the `false` at them would take away too.
fn shared_refusal(
    cx: &mut Cx<'_>,
    settings: &Settings,
    at: usize,
    binding: &SettingBinding,
) -> Option<String> {
    if !binding.default {
        return None;
    }
    let others: Vec<String> = settings
        .keys
        .iter()
        .enumerate()
        .filter(|(other, shared)| {
            *other != at
                && shared.default
                && shared.command.is_some()
                && same_keys(&shared.keys, &binding.keys)
        })
        .map(|(_, shared)| title_of(shared))
        .collect();
    if others.is_empty() {
        return None;
    }
    let shown =
        lokyr_plugin_sdk::keys::macos_text(&binding.keys).unwrap_or_else(|| binding.keys.clone());
    Some(message(
        cx,
        "binding-shared",
        &[("keys", shown), ("titles", others.join(", "))],
    ))
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
    if let Some(why) = refused.get(&format!("{TAKEN}{at}")) {
        about.push(
            Node::text(why.clone())
                .key(format!("{TAKEN}{at}").as_str())
                .style(Style {
                    color: Some(Token::from("text.warning")),
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
    let shown = lokyr_plugin_sdk::keys::macos_text(keys).unwrap_or_else(|| keys.clone());
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
    } else if changes(binding) {
        // Its keys as key caps, a button that lets them change.
        let mut button = Node::button(shown.as_str())
            .key(format!("{BINDING_KEYS}{at}").as_str())
            .label(message(cx, "binding-change", &[("keys", shown.clone())]))
            .tooltip(message(cx, "binding-change", &[("keys", shown.clone())]));
        if let Kind::Button(own) = &mut button.kind {
            own.keycaps = true;
        }
        button
    } else {
        Node::keys(shown.as_str()).key(format!("{KEYS_TEXT}{at}").as_str())
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
/// the command chosen, what the binding of `settings` that runs the same
/// keys now does, which the new one takes the place of, or that it does
/// what the new one would, or that macOS's own are none to take, why a
/// write was refused, of `refused`, and Cancel and Add, which waits for
/// both the keys and a command.
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
    // The binding that runs the same keys now, which the new one takes the
    // place of, or macOS's own, which none takes.
    let combos = adding
        .keys
        .as_deref()
        .and_then(|keys| lokyr_plugin_sdk::keys::parse_keys(keys).ok());
    let system = combos
        .as_ref()
        .is_some_and(|combos| lokyr_plugin_sdk::keys::is_system(combos, None));
    let taken = adding
        .keys
        .as_deref()
        .and_then(|keys| running(settings, keys));
    let already = runs_already(settings, adding);
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
        let id = if already {
            "binding-already"
        } else {
            "binding-taken"
        };
        children.push(
            Node::text(message(cx, id, &[("keys", shown.clone()), ("title", does)]))
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
    let ready = adding.keys.is_some() && adding.command.is_some() && !system && !already;
    // The panel's default button, which Return presses.
    let mut add = Node::button(message(cx, "binding-confirm", &[])).key(CONFIRM);
    if let Kind::Button(button) = &mut add.kind {
        button.disabled = !ready;
        button.variant = ButtonVariant::Default;
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

/// The appearances, of `in_force` first, as the themes' page's sidebar
/// lists them.
fn variants_of(in_force: ThemeVariant) -> [ThemeVariant; 2] {
    match in_force {
        ThemeVariant::Light => [ThemeVariant::Light, ThemeVariant::Dark],
        ThemeVariant::Dark => [ThemeVariant::Dark, ThemeVariant::Light],
    }
}

/// The page of the themes: a sidebar of the appearances, of the one in
/// force first, and the themes of `chosen`, or of the one in force, each a
/// card of a preview of its colors and its name, the one that `[theme]`
/// names marked, in rows of [`CARDS_ACROSS`], which a click names in
/// `[theme]`; a progress while the app reads their list.
fn themes_tree(
    cx: &mut Cx<'_>,
    chosen: Option<ThemeVariant>,
    editor: Option<&editor::Editor>,
) -> Node {
    let themes = cx.themes().ok();
    let settings = cx.settings().ok();
    let in_force = themes
        .as_ref()
        .map_or(ThemeVariant::Dark, |themes| themes.appearance);
    let variant = chosen.unwrap_or(in_force);
    let words = |variant: ThemeVariant| match variant {
        ThemeVariant::Light => ("light", "themes-light", "circle"),
        ThemeVariant::Dark => ("dark", "themes-dark", "circle-filled"),
    };
    // The sidebar: the appearances, the one in force first, a line, and
    // the Editor, each a row of a menu's look, the one shown selected.
    let mut rows: Vec<Node> = variants_of(in_force)
        .into_iter()
        .map(|each| {
            let (word, title, icon) = words(each);
            let selected = editor.is_none() && each == variant;
            side_row(cx, (word, title, icon), selected)
        })
        .collect();
    rows.push(Node::container(vec![Node::separator()]).layout(Layout {
        direction: Direction::Column,
        width: Length::Fraction(1.0),
        padding: Edges {
            top: 6.0,
            right: 0.0,
            bottom: 6.0,
            left: 0.0,
        },
        ..Layout::default()
    }));
    rows.push(side_row(
        cx,
        (SIDE_EDITOR, "themes-editor", "layout"),
        editor.is_some(),
    ));
    let sidebar = Node::container(rows)
        .key("themes-sidebar")
        .role(Role::Group)
        .label(message(cx, "page-themes", &[]))
        .layout(Layout {
            direction: Direction::Column,
            gap: 2.0,
            width: Length::Points(SIDEBAR),
            min_width: Length::Points(SIDEBAR),
            height: Length::Fraction(1.0),
            padding: Edges::all(SIDE_MARGIN),
            ..Layout::default()
        });
    if let Some(editor) = editor {
        let content = editor.tree(cx);
        return Node::container(vec![sidebar, Node::separator(), content])
            .key("themes-page")
            .layout(Layout {
                direction: Direction::Row,
                width: Length::Fraction(1.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            });
    }
    let (word, heading, _) = words(variant);
    // The title, and Show in Finder, Import… and New… at the row's end.
    let header = Node::container(vec![
        Node::text(message(cx, heading, &[]))
            .key("title")
            .role(Role::Heading)
            .style(Style {
                text_size: Some(TextSize::Large),
                font_weight: Some(FontWeight::Bold),
                ..Style::default()
            }),
        Node::spacer(),
        Node::button(message(cx, "themes-folder", &[]).as_str()).key(THEMES_FOLDER),
        Node::button(message(cx, "themes-import", &[]).as_str()).key(THEMES_IMPORT),
        Node::button(message(cx, "themes-new", &[]).as_str()).key(THEMES_NEW),
    ])
    .layout(Layout {
        direction: Direction::Row,
        align: Align::Center,
        gap: 8.0,
        width: Length::Fraction(1.0),
        ..Layout::default()
    });
    let mut children = vec![
        header,
        Node::text(message(cx, "themes-description", &[]))
            .key("description")
            .style(secondary())
            .layout(wrapped()),
    ];
    if themes.as_ref().is_none_or(|themes| themes.loading) {
        // A progress while the app reads the themes' list.
        children.push(
            Node::container(vec![
                Node::progress(ProgressShape::Ring, ProgressState::Indeterminate, 0.0).layout(
                    Layout {
                        width: Length::Points(16.0),
                        height: Length::Points(16.0),
                        ..Layout::default()
                    },
                ),
                Node::text(message(cx, "themes-loading", &[])).style(secondary()),
            ])
            .key("themes-loading")
            .layout(Layout {
                direction: Direction::Row,
                gap: 8.0,
                align: Align::Center,
                ..Layout::default()
            }),
        );
    } else {
        let named = settings
            .as_ref()
            .and_then(|settings| {
                settings
                    .sections
                    .iter()
                    .find(|section| section.name == "theme" && section.plugin.is_none())
            })
            .and_then(|section| section.keys.iter().find(|key| key.name == word))
            .and_then(|key| match &key.value {
                Some(Data::Text(name)) => Some(name.clone()),
                _ => None,
            });
        let names: Vec<&str> = themes
            .iter()
            .flat_map(|themes| &themes.themes)
            .filter(|theme| theme.variant == variant)
            .map(|theme| theme.name.as_str())
            .collect();
        for (row, chunk) in names.chunks(CARDS_ACROSS).enumerate() {
            let mut cards: Vec<Node> = chunk
                .iter()
                .map(|&name| theme_card(word, name, named.as_deref() == Some(name)))
                .collect();
            // The last row's room of the cards that it lacks, so that its
            // cards keep the others' width.
            cards.extend((chunk.len()..CARDS_ACROSS).map(|_| {
                Node::container(Vec::new()).layout(Layout {
                    grow: 1.0,
                    width: Length::Points(0.0),
                    ..Layout::default()
                })
            }));
            children.push(
                Node::container(cards)
                    .key(format!("themes-row:{word}:{row}").as_str())
                    .layout(Layout {
                        direction: Direction::Row,
                        gap: 12.0,
                        width: Length::Fraction(1.0),
                        ..Layout::default()
                    }),
            );
        }
    }
    let column = Node::container(children).layout(Layout {
        direction: Direction::Column,
        gap: 12.0,
        width: Length::Fraction(1.0),
        padding: Edges::all(20.0),
        ..Layout::default()
    });
    let content = Node::scroll(vec![column])
        .key("themes-content")
        .layout(Layout {
            direction: Direction::Column,
            grow: 1.0,
            width: Length::Points(0.0),
            min_width: Length::Points(0.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        });
    Node::container(vec![sidebar, Node::separator(), content])
        .key("themes-page")
        .layout(Layout {
            direction: Direction::Row,
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        })
}

/// A row of the themes page's sidebar, of its `word`, its title's message
/// and its icon, of a menu's look: of the accent's band where `selected`,
/// hovered otherwise.
fn side_row(cx: &mut Cx<'_>, (word, title, icon): (&str, &str, &str), selected: bool) -> Node {
    let (fg, background) = if selected {
        (
            Token::from("list.selection.fg"),
            Some(Token::from("list.selection.bg")),
        )
    } else {
        (Token::from("text"), None)
    };
    let title = message(cx, title, &[]);
    Node::container(vec![
        Node::icon(icon).style(Style {
            color: Some(if selected {
                fg.clone()
            } else {
                Token::from("accent")
            }),
            ..Style::default()
        }),
        Node::text(title.as_str()).style(Style {
            color: Some(fg),
            ..Style::default()
        }),
    ])
    .key(format!("{SIDE}{word}").as_str())
    .role(Role::Group)
    .label(title)
    .layout(Layout {
        direction: Direction::Row,
        gap: 8.0,
        align: Align::Center,
        height: Length::Points(ROW),
        width: Length::Fraction(1.0),
        padding: Edges {
            top: 0.0,
            right: BAND_PADDING.1,
            bottom: 0.0,
            left: BAND_PADDING.0,
        },
        ..Layout::default()
    })
    .style(Style {
        background,
        radius: 6.0,
        hover: (!selected).then(|| Hover {
            background: Some(Token::from("button.hover.bg")),
            ..Hover::default()
        }),
        ..Style::default()
    })
}

/// The card of the theme `name` of the appearance `variant`, `light` or
/// `dark`: its preview and its name, of the accent's border where it is
/// `chosen`, as `[theme]` names it.
fn theme_card(variant: &str, name: &str, chosen: bool) -> Node {
    let border = if chosen {
        Border {
            color: Token::from("accent"),
            width: 2.0,
        }
    } else {
        Border {
            color: Token::from("separator"),
            width: 1.0,
        }
    };
    Node::container(vec![
        Node::theme_preview(name).layout(Layout {
            width: Length::Fraction(1.0),
            height: Length::Points(PREVIEW_HEIGHT),
            ..Layout::default()
        }),
        Node::text(name).style(small()).layout(Layout {
            lines: Some(1),
            ..Layout::default()
        }),
    ])
    .key(format!("{THEME_CARD}{variant}:{name}").as_str())
    .label(name)
    .layout(Layout {
        direction: Direction::Column,
        gap: 6.0,
        grow: 1.0,
        width: Length::Points(0.0),
        min_width: Length::Points(0.0),
        padding: Edges::all(6.0),
        ..Layout::default()
    })
    .style(Style {
        border: Some(border),
        radius: 8.0,
        pointer: Some(PointerShape::PointingHand),
        hover: Some(Hover {
            background: Some(Token::from("button.hover.bg")),
            ..Hover::default()
        }),
        ..Style::default()
    })
}

/// The text of `why`, a write refused, under its control, of `key`.
fn refusal(why: &str, key: &str) -> Node {
    callout(Tone::Error, why, key)
}

/// A callout of `tone` and `text`, its text of the key `key`.
fn callout(tone: Tone, text: &str, key: &str) -> Node {
    let mut callout = Node::callout(tone, text);
    if let Some(text) = callout.children.get_mut(1) {
        text.key = Some(key.into());
    }
    callout
}

/// An empty state of the icon `icon`, of the title `title`, of the key
/// `key`, and of the line `line`.
fn empty(icon: &str, (title, key): (String, &str), line: String) -> Node {
    let mut empty = Node::empty_state(icon, title, line);
    if let Some(title) = empty.children.get_mut(1) {
        title.key = Some(key.into());
    }
    empty
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

/// A group box of `rows`, of the key `key`, as macOS's settings group
/// their rows: the host draws its rounded fill and a hairline between each
/// two rows.
fn group(rows: Vec<Node>, key: &str) -> Node {
    Node::group(rows).key(key)
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

mod editor;

#[cfg(test)]
mod tests;

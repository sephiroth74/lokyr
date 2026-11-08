//! Built-in plugin of the command palette: Command-Shift-P opens a panel at
//! the top of the focused window, of a large field and, below a thin line,
//! of the list of the commands that run without arguments and that the
//! menus enable, each of its title and its keys, which a fuzzy search ranks
//! as the user types, by their titles, their names,
//! their keys and their plugins, the recent ones first. The arrows move
//! through the list while the field takes the keys; Return, or a click,
//! runs the selected command as the user's, of `user-commands`, and closes
//! the palette; Escape, or a click outside it, closes it. Its theme picker
//! lists the themes of the appearance in force, previews each as it is
//! selected, and names the one chosen in the config's `[theme]` for that
//! appearance, Escape showing the config's again. Like every built-in
//! plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

mod rank;

use lokyr_plugin_sdk::command::{Args, Category, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::host::CommandInfo;
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::theme::ThemeVariant;
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, Layout, Length, ListItem, Node, Shadow, Style, TextSize,
    Token, UiEventKind,
};

use rank::Entry;

lokyr_plugin_sdk::export_plugin!(Palette);

/// Opens the palette in the focused window, its field taking the keys;
/// open, its field takes them again.
const OPEN: &str = "command-palette.open";

/// Opens the palette's theme picker, of the themes of the appearance in
/// force.
const THEME: &str = "command-palette.theme";

/// Moves the list's selection by `delta` items, as the keymap's arrows do
/// while the palette has the keyboard: of an argument that it needs, the
/// menus and the palette list it not.
const MOVE: &str = "command-palette.move";

/// The palette's panel, and the keys of its nodes.
const PANEL: &str = "command-palette";
const FIELD: &str = "query";
const LIST: &str = "commands";
const NONE: &str = "none";

/// The key of the plugin's state that keeps the recent commands' names,
/// the latest first, and how many it keeps.
const STATE_RECENT: &str = "recent";
const MOST_RECENT: usize = 20;

/// The palette's width, a row's height, and the most rows that its list
/// shows at once, past which it scrolls, in points.
const WIDTH: f32 = 480.0;
const ROW: f32 = 30.0;
const MOST_ROWS: usize = 10;

/// The plugin's catalogs, of its commands' titles and its texts.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/command-palette.ftl")),
    ("it", include_str!("../locales/it/command-palette.ftl")),
];

/// The palette's texts, in English, where the host gives none.
const ENGLISH: [(&str, &str); 6] = [
    ("palette-placeholder", "Execute Command…"),
    ("palette-none", "No commands"),
    ("palette-commands", "Commands"),
    ("palette-theme-placeholder", "Choose a Theme…"),
    ("palette-themes", "Themes"),
    ("palette-current", "Current"),
];

/// The text of the message `id`, in the language shown, or English's where
/// the host gives none.
fn message(cx: &mut Cx<'_>, id: &str) -> String {
    if let Ok(text) = cx.message(id, &[])
        && text != id
    {
        return text;
    }
    ENGLISH
        .iter()
        .find(|(english, _)| *english == id)
        .map_or_else(|| id.to_owned(), |(_, text)| (*text).to_owned())
}

/// The palette's view.
fn panel() -> ViewId {
    ViewId::Panel(PanelId::from(PANEL))
}

/// The plugin of the command palette.
#[derive(Debug, Default)]
pub struct Palette {
    /// The recent commands' names, the latest first.
    recent: Vec<String>,
    /// The palette, while it is open.
    open: Option<Open>,
    /// The closings of its panel that it asked for, as it ran a command,
    /// whose events it leaves, as the command may open it anew.
    closing: usize,
}

/// What an open palette lists: the commands, or the themes of an
/// appearance, whose picker previews the selected one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Commands,
    Themes(ThemeVariant),
}

/// An open palette: its commands, as the registry gave them as it opened,
/// the field's text, the commands' places that it ranks, the best first,
/// and the place among them of the selected one.
#[derive(Debug)]
struct Open {
    mode: Mode,
    commands: Vec<Entry>,
    query: String,
    ranked: Vec<usize>,
    selected: usize,
}

impl Open {
    /// The palette of `mode`, of `commands`, of no text, of the recent ones
    /// `recent`.
    fn new(mode: Mode, commands: Vec<Entry>, recent: &[String]) -> Self {
        let ranked = rank::rank(&commands, "", recent);
        Self {
            mode,
            commands,
            query: String::new(),
            ranked,
            selected: 0,
        }
    }

    /// The selected command, if one is.
    fn chosen(&self) -> Option<&Entry> {
        self.ranked.get(self.selected).map(|&at| &self.commands[at])
    }
}

/// Whether `command` is one that the palette lists: of no argument that
/// it needs, enabled by the menus, and not the palette's own opening.
fn listed(command: &CommandInfo) -> bool {
    command.params.iter().all(|param| param.default.is_some())
        && command.enabled != Some(false)
        && command.name != OPEN
}

impl Plugin for Palette {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if let Ok(Data::Map(state)) = cx.state()
            && let Some(Data::List(recent)) = state.get(STATE_RECENT)
        {
            self.recent = recent
                .iter()
                .filter_map(|name| match name {
                    Data::Text(name) if !name.is_empty() => Some(name.clone()),
                    _ => None,
                })
                .take(MOST_RECENT)
                .collect();
        }
        cx.register_command(Command {
            name: OPEN.to_owned(),
            title: "Command Palette…".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.register_command(Command {
            name: THEME.to_owned(),
            title: "Choose Theme…".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.register_command(Command {
            name: MOVE.to_owned(),
            title: "Move in the Command Palette".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: vec![Param {
                name: "delta".to_owned(),
                kind: Kind::Integer,
                default: None,
            }],
            when: Some(format!("{PANEL}.focused")),
        })?;
        Ok(())
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        match name {
            OPEN => self.open(cx),
            THEME => self.open_themes(cx),
            MOVE => {
                let delta = match args.get("delta") {
                    Some(Value::Integer(delta)) => *delta,
                    _ => 0,
                };
                self.step(cx, delta)
            }
            _ => Ok(()),
        }
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view != panel() {
            return None;
        }
        let open = self.open.as_ref()?;
        let texts = match open.mode {
            Mode::Commands => Texts {
                placeholder: message(cx, "palette-placeholder"),
                none: message(cx, "palette-none"),
                commands: message(cx, "palette-commands"),
            },
            Mode::Themes(_) => Texts {
                placeholder: message(cx, "palette-theme-placeholder"),
                none: message(cx, "palette-none"),
                commands: message(cx, "palette-themes"),
            },
        };
        Some(palette(open, &texts))
    }

    /// The field's text ranks the commands as it changes, and Return runs
    /// the selected one; a click on the list runs its command, and the
    /// keys move its selection alone while the list has the keyboard.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if event.view != panel() {
            return Ok(());
        }
        let Some(open) = &mut self.open else {
            return Ok(());
        };
        match (&event.kind, event.key.0.as_str()) {
            (UiEventKind::InputChanged { text, .. }, FIELD) => {
                open.query.clone_from(text);
                let recent: &[String] = match open.mode {
                    Mode::Commands => &self.recent,
                    Mode::Themes(_) => &[],
                };
                open.ranked = rank::rank(&open.commands, text, recent);
                open.selected = 0;
                cx.request_render(panel())?;
                if !open.ranked.is_empty() {
                    cx.select(panel(), LIST, 0)?;
                }
                self.preview(cx)
            }
            (UiEventKind::Submit { .. }, FIELD) => self.run(cx),
            // The list's own keys, while it has the keyboard, and a click
            // move the selection, which a theme previews; a click, which
            // leaves the keyboard where it is, Return and a double click run
            // the command, as the list's menu activates its row.
            (UiEventKind::Select { item }, LIST) => {
                open.selected = (*item).min(open.ranked.len().saturating_sub(1));
                self.preview(cx)
            }
            (UiEventKind::Activate { item }, LIST) => {
                open.selected = (*item).min(open.ranked.len().saturating_sub(1));
                self.run(cx)
            }
            _ => Ok(()),
        }
    }

    /// The palette's closing, as Escape or a click outside it closes it.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if let Event::PanelClosed(id) = event
                && *id == PanelId::from(PANEL)
            {
                if self.closing > 0 {
                    self.closing -= 1;
                    continue;
                }
                // Closed without a choice, a theme's preview ends.
                if let Some(Open {
                    mode: Mode::Themes(_),
                    ..
                }) = self.open.take()
                {
                    cx.preview_theme(None)?;
                }
            }
        }
        Ok(())
    }
}

impl Palette {
    /// Opens the palette in the focused window, of the commands that it
    /// lists, as the registry gives them now, the recent ones first; open,
    /// its field takes the keys again.
    fn open(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.open.is_some() {
            return cx.focus_node(panel(), FIELD);
        }
        let commands = cx
            .commands()?
            .into_iter()
            .filter(listed)
            .map(|command| Entry {
                name: command.name,
                title: command.title,
                keys: command.keys,
                plugin: command.plugin,
            })
            .collect();
        self.show(cx, Mode::Commands, commands)
    }

    /// Opens the theme picker in the focused window, of the themes of the
    /// appearance in force, its theme first, which it shows; open, its
    /// field takes the keys again.
    fn open_themes(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.open.is_some() {
            return cx.focus_node(panel(), FIELD);
        }
        let themes = cx.themes()?;
        let current = message(cx, "palette-current");
        let mut entries: Vec<Entry> = themes
            .themes
            .into_iter()
            .filter(|theme| theme.variant == themes.appearance)
            .map(|theme| Entry {
                keys: (theme.name == themes.current).then(|| current.clone()),
                title: theme.name.clone(),
                name: theme.name,
                plugin: None,
            })
            .collect();
        // The theme in force first, selected as the picker opens.
        if let Some(at) = entries.iter().position(|entry| entry.keys.is_some()) {
            let first = entries.remove(at);
            entries.insert(0, first);
        }
        self.show(cx, Mode::Themes(themes.appearance), entries)
    }

    /// Opens the palette's panel in the focused window, of `mode` and
    /// `entries`.
    fn show(&mut self, cx: &mut Cx<'_>, mode: Mode, entries: Vec<Entry>) -> Result<(), Error> {
        let Some(window) = cx.focused_window() else {
            return Ok(());
        };
        cx.open_panel(
            PANEL,
            PanelOptions {
                anchor: Anchor::WindowCorner {
                    window,
                    corner: Corner::Top,
                },
                role: Role::Dialog,
                close_outside: true,
                close_escape: true,
                takes_keys: false,
            },
        )?;
        let recent: &[String] = match mode {
            Mode::Commands => &self.recent,
            Mode::Themes(_) => &[],
        };
        self.open = Some(Open::new(mode, entries, recent));
        Ok(())
    }

    /// Of the theme picker, previews the selected theme, or the config's
    /// of none.
    fn preview(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(open) = &self.open else {
            return Ok(());
        };
        if !matches!(open.mode, Mode::Themes(_)) {
            return Ok(());
        }
        cx.preview_theme(open.chosen().map(|entry| entry.name.as_str()))
    }

    /// Moves the selection by `delta` places, within the list.
    fn step(&mut self, cx: &mut Cx<'_>, delta: i64) -> Result<(), Error> {
        let Some(open) = &mut self.open else {
            return Ok(());
        };
        let Some(last) = open.ranked.len().checked_sub(1) else {
            return Ok(());
        };
        let at = i64::try_from(open.selected)
            .unwrap_or(0)
            .saturating_add(delta);
        open.selected = usize::try_from(at.max(0)).unwrap_or(0).min(last);
        cx.select(panel(), LIST, open.selected)?;
        self.preview(cx)
    }

    /// Runs the selected command as the user's, once the palette closes,
    /// and puts it first among the recent ones, which the state keeps.
    fn run(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some((mode, name)) = self
            .open
            .as_ref()
            .and_then(|open| Some((open.mode, open.chosen()?.name.clone())))
        else {
            return Ok(());
        };
        cx.close_panel(PANEL)?;
        self.closing += 1;
        self.open = None;
        // Of the theme picker, the theme named for its appearance.
        if let Mode::Themes(variant) = mode {
            return cx.set_theme(variant, &name);
        }
        self.recent.retain(|kept| *kept != name);
        self.recent.insert(0, name.clone());
        self.recent.truncate(MOST_RECENT);
        let recent = self.recent.iter().cloned().map(Data::Text).collect();
        // Without a state directory, they stay for this run.
        let _ = cx.set_state(Data::Map(
            [(STATE_RECENT.to_owned(), Data::List(recent))].into(),
        ));
        cx.run_command(&name, Args::new())
    }
}

/// The palette's texts, in the language shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Texts {
    placeholder: String,
    none: String,
    commands: String,
}

/// The palette's view of `open`: its field, large, of no box and no ring,
/// a thin line, and the list of the commands that it ranks, each of its
/// title and its keys at its end, of a menu's look, or a line of none.
fn palette(open: &Open, texts: &Texts) -> Node {
    let field = Node::container(vec![
        Node::field(texts.placeholder.as_str())
            .key(FIELD)
            .layout(Layout {
                width: Length::Fraction(1.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("transparent")),
                text_size: Some(TextSize::Large),
                hides_ring: true,
                ..Style::default()
            }),
    ])
    .layout(Layout {
        padding: Edges {
            top: 10.0,
            right: 14.0,
            bottom: 10.0,
            left: 14.0,
        },
        ..Layout::default()
    });
    let secondary = |text: &str| {
        Node::text(text).style(Style {
            color: Some(Token::from("text.secondary")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
    };
    // A row: its title, and its keys at its end.
    let row = Node::container(vec![Node::text(""), Node::spacer(), secondary("")]).layout(Layout {
        gap: 8.0,
        align: Align::Center,
        padding: Edges {
            top: 4.0,
            right: 16.0,
            bottom: 4.0,
            left: 16.0,
        },
        ..Layout::default()
    });
    let items = open
        .ranked
        .iter()
        .map(|&at| ListItem {
            texts: vec![
                open.commands[at].title.clone(),
                open.commands[at].keys.clone().unwrap_or_default(),
            ],
        })
        .collect();
    let shown = open.ranked.len().min(MOST_ROWS);
    let mut list = Node::list(row, open.ranked.len(), (0, items))
        .key(LIST)
        .label(texts.commands.as_str())
        .layout(Layout {
            width: Length::Fraction(1.0),
            height: Length::Points(shown as f32 * ROW),
            ..Layout::default()
        });
    if let lokyr_plugin_sdk::view::Kind::List(list) = &mut list.kind {
        list.row_height = Some(ROW);
        list.menu = true;
    }
    let below = if open.ranked.is_empty() {
        Node::container(vec![list, secondary(&texts.none).key(NONE)]).layout(Layout {
            direction: Direction::Column,
            padding: Edges {
                top: 10.0,
                right: 20.0,
                bottom: 12.0,
                left: 20.0,
            },
            ..Layout::default()
        })
    } else {
        Node::container(vec![list]).layout(Layout {
            direction: Direction::Column,
            padding: Edges {
                top: 6.0,
                right: 0.0,
                bottom: 6.0,
                left: 0.0,
            },
            ..Layout::default()
        })
    };
    Node::container(vec![field, Node::separator(), below])
        .layout(Layout {
            direction: Direction::Column,
            width: Length::Points(WIDTH),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("panel.border"),
                width: 1.0,
            }),
            radius: 10.0,
            shadow: Some(Shadow::default()),
            clip: true,
            ..Style::default()
        })
}

#[cfg(test)]
mod tests;

//! Built-in plugin of the command palette: Command-Shift-P opens a panel at
//! the top of the focused window, of a field and of the list of the
//! commands that run without arguments and that the menus enable, which a
//! fuzzy search ranks as the user types, by their titles, their names,
//! their keys and their plugins, the recent ones first. The arrows move
//! through the list; Return, or a double click, runs the selected command
//! as the user's, of `user-commands`, and closes the palette; Escape, or a
//! click outside it, closes it. Like every built-in plugin, it depends only
//! on the plugin SDK.

#![forbid(unsafe_code)]

mod rank;

use lokyr_plugin_sdk::command::{Args, Category, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::host::CommandInfo;
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, Layout, Length, Node, Shadow, Style, TextSize, Token,
    UiEventKind,
};

use rank::Entry;

lokyr_plugin_sdk::export_plugin!(Palette);

/// Opens the palette in the focused window, its field taking the keys;
/// open, its field takes them again.
const OPEN: &str = "command-palette.open";

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
const WIDTH: f32 = 520.0;
const ROW: f32 = 26.0;
const MOST_ROWS: usize = 12;

/// The plugin's catalogs, of its commands' titles and its texts.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/command-palette.ftl")),
    ("it", include_str!("../locales/it/command-palette.ftl")),
];

/// The palette's texts, in English, where the host gives none.
const ENGLISH: [(&str, &str); 3] = [
    ("palette-placeholder", "Type a command"),
    ("palette-none", "No commands"),
    ("palette-commands", "Commands"),
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
}

/// An open palette: its commands, as the registry gave them as it opened,
/// the field's text, the commands' places that it ranks, the best first,
/// and the place among them of the selected one.
#[derive(Debug)]
struct Open {
    commands: Vec<Entry>,
    query: String,
    ranked: Vec<usize>,
    selected: usize,
}

impl Open {
    /// The palette of `commands`, of no text, of the recent ones `recent`.
    fn new(commands: Vec<Entry>, recent: &[String]) -> Self {
        let ranked = rank::rank(&commands, "", recent);
        Self {
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
/// it needs, enabled by the menus, and not the palette's own.
fn listed(command: &CommandInfo) -> bool {
    command.params.iter().all(|param| param.default.is_some())
        && command.enabled != Some(false)
        && !command.name.starts_with("command-palette.")
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
        let texts = Texts {
            placeholder: message(cx, "palette-placeholder"),
            none: message(cx, "palette-none"),
            commands: message(cx, "palette-commands"),
        };
        Some(palette(open, &texts))
    }

    /// The field's text ranks the commands as it changes, and Return runs
    /// the selected one; the list's selection follows a click, and a double
    /// click runs its command.
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
                open.ranked = rank::rank(&open.commands, text, &self.recent);
                open.selected = 0;
                cx.request_render(panel())?;
                if !open.ranked.is_empty() {
                    cx.select(panel(), LIST, 0)?;
                }
                Ok(())
            }
            (UiEventKind::Submit { .. }, FIELD) => self.run(cx),
            (UiEventKind::Select { item }, LIST) => {
                open.selected = (*item).min(open.ranked.len().saturating_sub(1));
                Ok(())
            }
            (UiEventKind::Activate { item }, LIST) => {
                open.selected = (*item).min(open.ranked.len().saturating_sub(1));
                self.run(cx)
            }
            _ => Ok(()),
        }
    }

    /// The palette's closing, as Escape or a click outside it closes it.
    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if let Event::PanelClosed(id) = event
                && *id == PanelId::from(PANEL)
            {
                self.open = None;
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
        let Some(window) = cx.focused_window() else {
            return Ok(());
        };
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
            },
        )?;
        self.open = Some(Open::new(commands, &self.recent));
        Ok(())
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
        cx.select(panel(), LIST, open.selected)
    }

    /// Runs the selected command as the user's, once the palette closes,
    /// and puts it first among the recent ones, which the state keeps.
    fn run(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(name) = self
            .open
            .as_ref()
            .and_then(Open::chosen)
            .map(|entry| entry.name.clone())
        else {
            return Ok(());
        };
        self.recent.retain(|kept| *kept != name);
        self.recent.insert(0, name.clone());
        self.recent.truncate(MOST_RECENT);
        let recent = self.recent.iter().cloned().map(Data::Text).collect();
        // Without a state directory, they stay for this run.
        let _ = cx.set_state(Data::Map(
            [(STATE_RECENT.to_owned(), Data::List(recent))].into(),
        ));
        cx.close_panel(PANEL)?;
        self.open = None;
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

/// The palette's view of `open`: its field, and the list of the commands
/// that it ranks, each of its title and its keys, or a line of none.
fn palette(open: &Open, texts: &Texts) -> Node {
    let field = Node::field(texts.placeholder.as_str())
        .key(FIELD)
        .layout(Layout {
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
            ..Style::default()
        });
    let secondary = |text: &str| {
        Node::text(text).style(Style {
            color: Some(Token::from("text.secondary")),
            text_size: Some(TextSize::Small),
            ..Style::default()
        })
    };
    // The list, of no item where none matches, and its line of none then.
    let row = Node::container(vec![Node::text(""), Node::spacer(), secondary("")]).layout(Layout {
        gap: 8.0,
        align: Align::Center,
        padding: Edges {
            top: 2.0,
            right: 8.0,
            bottom: 2.0,
            left: 8.0,
        },
        ..Layout::default()
    });
    let items = open
        .ranked
        .iter()
        .map(|&at| {
            let entry = &open.commands[at];
            lokyr_plugin_sdk::view::ListItem {
                texts: vec![entry.title.clone(), entry.keys.clone().unwrap_or_default()],
            }
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
    }
    let mut children = vec![field, list];
    if open.ranked.is_empty() {
        children.push(secondary(&texts.none).key(NONE));
    }
    Node::container(children)
        .layout(Layout {
            direction: Direction::Column,
            width: Length::Points(WIDTH),
            gap: 6.0,
            padding: Edges::all(6.0),
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
            ..Style::default()
        })
}

#[cfg(test)]
mod tests;

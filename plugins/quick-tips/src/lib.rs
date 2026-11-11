//! Built-in plugin of the quick tips: at the app's start, once a week at
//! most, a card at the focused window's bottom right tells one tip, a
//! shortcut as the keymap binds it now, a feature that shows nowhere, or a
//! command of an installed plugin, each once before any comes again.
//!
//! The card waits a moment after the start, so that the window and its
//! prompt show first, and shows only while no notice of the app's does. It
//! goes after a while, held while the pointer rests on it, at the first key
//! typed into a pane, or by its button; its checkbox stops the tips for
//! good, which its plugin's state keeps. Like every built-in plugin, it
//! depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::time::Duration;

use lokyr_plugin_sdk::command::{Args, Category, Command, Error, Value};
use lokyr_plugin_sdk::host::WindowId;
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{
    Anchor, Corner, PanelId, PanelOptions, Role as PanelRole, UiEvent, ViewId,
};
use lokyr_plugin_sdk::view::{
    Align, Border, Direction, Edges, FontWeight, Hover, Justify, Kind, Layout, Length, Node, Role,
    Shadow, Style, TextSize, Toggle, Token, Transition, UiEventKind,
};

lokyr_plugin_sdk::export_plugin!(Tips);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/quick-tips.ftl")),
    ("it", include_str!("../locales/it/quick-tips.ftl")),
];

/// The card's panel.
const PANEL: &str = "quick-tips";

/// The command that shows the next tip at once, whatever the week.
const SHOW: &str = "quick-tips.show";

/// The command palette's command, whose keys a tip of a command of no keys
/// of its own names.
const PALETTE: &str = "command-palette.open";

/// How long after the start the tip shows: the window and its prompt first,
/// and a notice of the start before it, which it then gives way to.
const DELAY: Duration = Duration::from_secs(2);

/// How long the card shows, unless the pointer rests on it.
const SHOWS: Duration = Duration::from_secs(12);

/// How long after its first render the card slides into place.
const ENTERS: Duration = Duration::from_millis(50);

/// The days between two tips at the start.
const EVERY: u64 = 7;

/// The most commands of installed plugins that the tips name.
const MOST_PLUGIN_TIPS: usize = 32;

/// The card's width, its padding, the room below it that it rises through,
/// and the checkbox's side, in points.
const CARD: f32 = 340.0;
const PADDING: f32 = 12.0;
const RISE: f32 = 8.0;
const BOX: f32 = 14.0;

/// The keys of the plugin's state: the day of the last tip at the start,
/// the tips shown since all were last, and whether the tips stopped.
const STATE_LAST: &str = "last";
const STATE_SHOWN: &str = "shown";
const STATE_NEVER: &str = "never";

/// The namespaces of the app's commands and of its own plugins', whose
/// tips are the curated ones: a command of another is an installed
/// plugin's.
const FIRST_PARTY: [&str; 11] = [
    "app",
    "command-palette",
    "copy-mode",
    "font",
    "links",
    "pane",
    "quick-tips",
    "search",
    "splits",
    "tabs",
    "window",
];

/// The argument of a tip's command, of its keys.
#[derive(Clone, Copy, Debug)]
enum Arg {
    /// None.
    None,
    /// A text, by its name.
    Text(&'static str, &'static str),
    /// A whole number, by its name.
    Integer(&'static str, i64),
}

impl Arg {
    fn args(self) -> Args {
        match self {
            Self::None => Args::new(),
            Self::Text(name, value) => {
                Args::from([(name.to_owned(), Value::String(value.to_owned()))])
            }
            Self::Integer(name, value) => Args::from([(name.to_owned(), Value::Integer(value))]),
        }
    }
}

/// A curated tip: its id, which the state keeps, its text's message, and
/// the command whose keys it names, as `$keys`, with its argument, which
/// shows it only while they are bound; or none, of a feature that needs no
/// keys.
#[derive(Clone, Copy, Debug)]
struct Curated {
    id: &'static str,
    message: &'static str,
    command: Option<(&'static str, Arg)>,
}

/// The curated tips, the most useful first.
const CURATED: [Curated; 21] = [
    command("palette", "tip-palette", PALETTE, Arg::None),
    command("search", "tip-search", "search.open", Arg::None),
    command(
        "split",
        "tip-split",
        "splits.split",
        Arg::Text("direction", "right"),
    ),
    plain("links", "tip-links"),
    command("prompts", "tip-prompts", "pane.prompt-previous", Arg::None),
    command("copy-mode", "tip-copy-mode", "copy-mode.toggle", Arg::None),
    plain("tab-drag", "tip-tab-drag"),
    command("zoom", "tip-zoom", "splits.zoom-toggle", Arg::None),
    command(
        "focus-pane",
        "tip-focus-pane",
        "splits.focus",
        Arg::Text("direction", "right"),
    ),
    plain("tab-menu", "tip-tab-menu"),
    command(
        "goto-tab",
        "tip-goto-tab",
        "tabs.goto",
        Arg::Integer("index", 1),
    ),
    plain("themes", "tip-themes"),
    command(
        "quick-terminal",
        "tip-quick-terminal",
        "window.toggle-quick-terminal",
        Arg::None,
    ),
    plain("long-command", "tip-long-command"),
    command("clear", "tip-clear", "pane.clear", Arg::None),
    plain("look-up", "tip-look-up"),
    plain("services", "tip-services"),
    command("font-size", "tip-font-size", "font.bigger", Arg::None),
    plain("itermcolors", "tip-itermcolors"),
    plain("float", "tip-float"),
    plain("sleip", "tip-sleip"),
];

/// A curated tip of a command's keys.
const fn command(id: &'static str, message: &'static str, name: &'static str, arg: Arg) -> Curated {
    Curated {
        id,
        message,
        command: Some((name, arg)),
    }
}

/// A curated tip of a feature that needs no keys.
const fn plain(id: &'static str, message: &'static str) -> Curated {
    Curated {
        id,
        message,
        command: None,
    }
}

/// A tip that can show now: its id, which the state keeps, and its text's
/// message with the message's arguments.
#[derive(Clone, Debug, PartialEq)]
struct Ready {
    id: String,
    message: &'static str,
    args: Vec<(&'static str, String)>,
}

/// The state that the plugin keeps between the app's runs.
#[derive(Clone, Debug, Default, PartialEq)]
struct Saved {
    last: Option<u64>,
    shown: Vec<String>,
    never: bool,
}

impl Saved {
    /// The state that the host keeps for the plugin; none of a value that
    /// it does not read.
    fn read(cx: &mut Cx<'_>) -> Self {
        let Ok(Data::Map(state)) = cx.state() else {
            return Self::default();
        };
        let last = match state.get(STATE_LAST) {
            Some(Data::Integer(day)) => u64::try_from(*day).ok(),
            _ => None,
        };
        let shown = match state.get(STATE_SHOWN) {
            Some(Data::List(ids)) => ids
                .iter()
                .filter_map(|id| match id {
                    Data::Text(id) => Some(id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let never = matches!(state.get(STATE_NEVER), Some(Data::Boolean(true)));
        Self { last, shown, never }
    }

    /// Gives the state to the host, which writes it.
    fn write(&self, cx: &mut Cx<'_>) {
        let mut state = BTreeMap::from([
            (
                STATE_SHOWN.to_owned(),
                Data::List(self.shown.iter().cloned().map(Data::Text).collect()),
            ),
            (STATE_NEVER.to_owned(), Data::Boolean(self.never)),
        ]);
        if let Some(day) = self.last.and_then(|day| i64::try_from(day).ok()) {
            state.insert(STATE_LAST.to_owned(), Data::Integer(day));
        }
        let _ = cx.set_state(Data::Map(state));
    }
}

/// The card that shows: its tip's text, whether its checkbox is on, whether
/// it is still to slide into place, and its timers, of its slide and of its
/// going.
#[derive(Debug)]
struct Card {
    text: String,
    never: bool,
    entering: bool,
    enters: Option<u64>,
    goes: Option<u64>,
}

/// The plugin of the quick tips: the timer of the start's tip until it
/// fires, the card that shows, and the closings of the card that it asked
/// for, whose events tell nothing more.
#[derive(Debug, Default)]
pub struct Tips {
    start: Option<u64>,
    card: Option<Card>,
    closing: usize,
}

impl Plugin for Tips {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: SHOW.to_owned(),
            title: "Show a Tip".to_owned(),
            category: Category::App,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        // A start of no tip due waits for nothing, and wakes the app for
        // nothing.
        if due(cx).is_some() {
            self.start = Some(cx.start_timer(DELAY, false)?);
        }
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _args: Args) -> Result<(), Error> {
        if name == SHOW {
            let saved = Saved::read(cx);
            self.show(cx, saved, None);
        }
        Ok(())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            match event {
                Event::Timer { id } if self.start == Some(*id) => {
                    self.start = None;
                    self.at_start(cx);
                }
                Event::Timer { id } => {
                    let Some(card) = self.card.as_mut() else {
                        continue;
                    };
                    if card.enters == Some(*id) {
                        card.enters = None;
                        card.entering = false;
                        cx.request_render(view())?;
                    } else if card.goes == Some(*id) {
                        self.close(cx);
                    }
                }
                Event::PanelClosed(id) if id.0 == PANEL => {
                    if self.closing > 0 {
                        self.closing -= 1;
                    } else if let Some(card) = self.card.take() {
                        // A key typed, or its window closed.
                        stop_timers(cx, &card);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::Panel(id) = view else {
            return None;
        };
        let card = self.card.as_ref().filter(|_| id.0 == PANEL)?;
        let texts = Texts {
            title: cx.message("card-title", &[]).unwrap_or_default(),
            dismiss: cx.message("card-dismiss", &[]).unwrap_or_default(),
            never: cx.message("card-never", &[]).unwrap_or_default(),
        };
        Some(card_tree(&texts, card))
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if event.view != view() {
            return Ok(());
        }
        match (&event.kind, event.key.0.as_str()) {
            (UiEventKind::Click, "dismiss") => self.close(cx),
            (UiEventKind::Toggle { on }, "never" | "never-label") => {
                let Some(card) = self.card.as_mut() else {
                    return Ok(());
                };
                card.never = *on;
                let mut saved = Saved::read(cx);
                saved.never = *on;
                saved.write(cx);
                cx.request_render(view())?;
            }
            (UiEventKind::HoverEnter, "card") => {
                if let Some(card) = self.card.as_mut()
                    && let Some(goes) = card.goes.take()
                {
                    let _ = cx.cancel_timer(goes);
                }
            }
            (UiEventKind::HoverLeave, "card") => {
                if let Some(card) = self.card.as_mut()
                    && card.goes.is_none()
                {
                    card.goes = cx.start_timer(SHOWS, false).ok();
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Tips {
    /// The start's tip, if one is due and no notice of the app's shows.
    fn at_start(&mut self, cx: &mut Cx<'_>) {
        let Some((saved, day)) = due(cx) else {
            return;
        };
        if cx.notice_shows().unwrap_or(true) {
            return;
        }
        self.show(cx, saved, Some(day));
    }

    /// Shows the first tip that can show now and has not shown since all
    /// did, in the focused window, of the state `saved`, which keeps it,
    /// and `day` as the last tip's at the start, of one.
    fn show(&mut self, cx: &mut Cx<'_>, mut saved: Saved, day: Option<u64>) {
        if self.card.is_some() {
            return;
        }
        let Some(window) = cx.focused_window() else {
            return;
        };
        let tips = ready(cx);
        // The tips that went, as a plugin removed, leave the round.
        saved
            .shown
            .retain(|id| tips.iter().any(|tip| tip.id == *id));
        let next = tips
            .iter()
            .find(|tip| !saved.shown.contains(&tip.id))
            .or_else(|| {
                saved.shown.clear();
                tips.first()
            });
        let Some(tip) = next.cloned() else {
            return;
        };
        let args: Vec<(&str, Data)> = tip
            .args
            .iter()
            .map(|(name, value)| (*name, Data::Text(value.clone())))
            .collect();
        let Ok(text) = cx.message(tip.message, &args) else {
            return;
        };
        if open(cx, window).is_err() {
            return;
        }
        saved.shown.push(tip.id);
        if day.is_some() {
            saved.last = day;
        }
        saved.write(cx);
        self.card = Some(Card {
            text,
            never: saved.never,
            entering: true,
            enters: cx.start_timer(ENTERS, false).ok(),
            goes: cx.start_timer(SHOWS, false).ok(),
        });
    }

    /// Closes the card that shows.
    fn close(&mut self, cx: &mut Cx<'_>) {
        let Some(card) = self.card.take() else {
            return;
        };
        stop_timers(cx, &card);
        if cx.close_panel(PANEL).is_ok() {
            self.closing += 1;
        }
    }
}

/// The state and the day of a tip due at the start: the host keeps the
/// state, as it does not without a state's folder, where a tip would show
/// at every start; the tips did not stop; and a week passed since the last
/// one, or none showed yet.
fn due(cx: &mut Cx<'_>) -> Option<(Saved, u64)> {
    if !cx.keeps_state().unwrap_or(false) {
        return None;
    }
    let saved = Saved::read(cx);
    if saved.never {
        return None;
    }
    let day = cx.day().ok()?;
    if saved
        .last
        .is_some_and(|last| day < last.saturating_add(EVERY))
    {
        return None;
    }
    Some((saved, day))
}

/// The card's view.
fn view() -> ViewId {
    ViewId::Panel(PanelId::from(PANEL))
}

/// Opens the card's panel at `window`'s bottom right, over the panes,
/// which takes no keyboard and goes at the first key typed into a pane.
fn open(cx: &mut Cx<'_>, window: WindowId) -> Result<(), Error> {
    cx.open_panel(
        PANEL,
        PanelOptions {
            anchor: Anchor::WindowCorner {
                window,
                corner: Corner::BottomRight,
            },
            role: PanelRole::Overlay,
            close_outside: false,
            close_escape: false,
            takes_keys: false,
            close_typing: true,
        },
    )
}

/// Stops `card`'s timers.
fn stop_timers(cx: &mut Cx<'_>, card: &Card) {
    for timer in [card.enters, card.goes].into_iter().flatten() {
        let _ = cx.cancel_timer(timer);
    }
}

/// The tips that can show now, in their order: the curated ones whose
/// commands' keys are bound, or of none, then the installed plugins'
/// commands of no parameters, of their keys or, of none, of the palette's.
fn ready(cx: &mut Cx<'_>) -> Vec<Ready> {
    let mut tips = Vec::new();
    for tip in CURATED {
        let message = tip.message;
        match tip.command {
            None => tips.push(Ready {
                id: tip.id.to_owned(),
                message,
                args: Vec::new(),
            }),
            Some((name, arg)) => {
                if let Ok(Some(keys)) = cx.keys_of(name, arg.args()) {
                    tips.push(Ready {
                        id: tip.id.to_owned(),
                        message,
                        args: vec![("keys", keys)],
                    });
                }
            }
        }
    }
    let palette = cx.keys_of(PALETTE, Args::new()).ok().flatten();
    let mut plugins = 0;
    for command in cx.commands().unwrap_or_default() {
        if plugins == MOST_PLUGIN_TIPS {
            break;
        }
        let namespace = command.name.split('.').next().unwrap_or_default();
        if !command.params.is_empty() || FIRST_PARTY.contains(&namespace) {
            continue;
        }
        let keys = cx.keys_of(&command.name, Args::new()).ok().flatten();
        let (message, args) = match (keys, &palette) {
            (Some(keys), _) => ("tip-plugin-keys", vec![("keys", keys)]),
            (None, Some(palette)) => ("tip-plugin-palette", vec![("keys", palette.clone())]),
            (None, None) => continue,
        };
        let mut args = args;
        args.push(("title", command.title.clone()));
        tips.push(Ready {
            id: format!("command:{}", command.name),
            message,
            args,
        });
        plugins += 1;
    }
    tips
}

/// The card's texts in the language that the app shows.
struct Texts {
    title: String,
    dismiss: String,
    never: String,
}

/// The card of `card`'s tip: a lightbulb of the accent's color, its title
/// of the same color and its text; below, the checkbox that stops the tips
/// and the button that dismisses it. It rises into place from a little
/// lower, faded, as it first shows.
fn card_tree(texts: &Texts, card: &Card) -> Node {
    let title = Node::text(texts.title.as_str()).style(Style {
        font_weight: Some(FontWeight::Semibold),
        color: Some(Token::from("accent")),
        ..Style::default()
    });
    let text = Node::text(card.text.as_str());
    let words = Node::container(vec![title, text]).layout(Layout {
        direction: Direction::Column,
        gap: 4.0,
        grow: 1.0,
        width: Length::Points(CARD - 2.0 * PADDING - 16.0 - 10.0),
        ..Layout::default()
    });
    let bulb = Node::icon("lightbulb").style(Style {
        color: Some(Token::from("accent")),
        ..Style::default()
    });
    let top = Node::container(vec![bulb, words]).layout(Layout {
        direction: Direction::Row,
        gap: 10.0,
        align: Align::Start,
        ..Layout::default()
    });
    let bottom = Node::container(vec![
        checkbox(card.never, &texts.never),
        never_label(card.never, &texts.never),
        Node::spacer(),
        Node::button(texts.dismiss.as_str())
            .key("dismiss")
            .style(Style {
                text_size: Some(TextSize::Small),
                ..Style::default()
            }),
    ])
    .layout(Layout {
        direction: Direction::Row,
        gap: 6.0,
        align: Align::Center,
        padding: Edges {
            left: 26.0,
            ..Edges::default()
        },
        ..Layout::default()
    });
    let body = Node::container(vec![top, bottom])
        .key("card")
        .role(Role::Status)
        .label(format!("{}: {}", texts.title, card.text))
        .layout(Layout {
            direction: Direction::Column,
            gap: 10.0,
            width: Length::Points(CARD),
            padding: Edges::all(PADDING),
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            border: Some(Border {
                color: Token::from("accent"),
                width: 1.0,
            }),
            radius: 12.0,
            shadow: Some(Shadow::default()),
            opacity: Some(if card.entering { 0.0 } else { 1.0 }),
            transition: Some(Transition::default()),
            hover: Some(Hover::default()),
            ..Style::default()
        });
    // The room below it, which it rises through as it shows.
    Node::container(vec![body]).key("tip").layout(Layout {
        padding: Edges {
            bottom: if card.entering { 0.0 } else { RISE },
            ..Edges::default()
        },
        ..Layout::default()
    })
}

/// The checkbox that stops the tips, `on` or off, which `label` names: a
/// small square, of a tick on the accent where on.
fn checkbox(on: bool, label: &str) -> Node {
    Node::new(Kind::Toggle(Toggle {
        icon: if on { "check" } else { "" }.to_owned(),
        on,
        ..Toggle::default()
    }))
    .key("never")
    .label(label)
    .layout(Layout {
        width: Length::Points(BOX),
        height: Length::Points(BOX),
        justify: Justify::Center,
        align: Align::Center,
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from(if on { "accent" } else { "field.bg" })),
        color: on.then(|| Token::from("list.selection.fg")),
        border: Some(Border {
            color: Token::from(if on { "accent" } else { "field.border" }),
            width: 1.0,
        }),
        radius: 3.5,
        text_size: Some(TextSize::Small),
        ..Style::default()
    })
}

/// The checkbox's label, which a click toggles as the box's, as the
/// system's checkboxes have it.
fn never_label(on: bool, label: &str) -> Node {
    Node::new(Kind::Toggle(Toggle {
        label: label.to_owned(),
        on,
        ..Toggle::default()
    }))
    .key("never-label")
    .style(Style {
        background: Some(Token::from("transparent")),
        color: Some(Token::from("text.secondary")),
        text_size: Some(TextSize::Small),
        ..Style::default()
    })
}

#[cfg(test)]
mod tests;

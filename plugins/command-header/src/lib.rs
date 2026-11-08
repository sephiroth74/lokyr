//! Built-in plugin of the command's header: the command whose output a
//! pane's view shows, at the pane's top, while the command's line is above
//! the view, as the shell marks its commands. It sits over the pane's first
//! row, a pill of the command's line on a hairline across the pane, and
//! takes none of the grid's rows, so that the grid keeps its size as it
//! shows and hides. As the view scrolls up to the command's line, the
//! header goes, and the command before it takes its place once that one's
//! line is above the view; down, the next one does. A click scrolls the
//! view to the command's prompt. The alternate screen shows none. Its text
//! is the terminal's font's; its config names the theme's tokens of its
//! colors, and sets its text's size and weight, its pill's padding and
//! where the pill sits. Like every built-in plugin, it depends only on the
//! plugin SDK.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{PaneId, Scroll, ShellCommand, Viewport};
use lokyr_plugin_sdk::message::{COMMAND_ENDED, COMMAND_STARTED, Data, Event, VIEWPORT_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Edges, FontFamily, FontWeight, Hover, Layout, Length, Node, PointerShape, Style,
    Token, Transition, UiEventKind,
};

lokyr_plugin_sdk::export_plugin!(Header);

/// The plugin's catalogs, of the header's label and tooltip.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/command-header.ftl")),
    ("it", include_str!("../locales/it/command-header.ftl")),
];

/// The prefix of a header's panel's id, of its pane's id.
const PANEL: &str = "command-header-";

/// The key of a header's pill, which a click scrolls by.
const PILL: &str = "command";

/// The most characters of a command's line that a header shows, past which
/// the pill ends where the pane does.
const MOST_LINE: usize = 500;

/// The rows of a command's logical line that a read of its line reaches,
/// past its input's.
const LINE_REACH: usize = 16;

/// The length of the hairline at the pill's left, in points.
const LEAD: f32 = 12.0;

/// How long the pill's hover fades, in seconds.
const FADE: f32 = 0.12;

/// The pill's corners' radius, in points, past half its height, as the
/// renderer takes half the box's height at most: round at either end.
const PILL_RADIUS: f32 = 100.0;

/// The plugin, its settings, and what it follows of each pane whose view
/// it heard of.
#[derive(Debug, Default)]
pub struct Header {
    settings: Settings,
    panes: BTreeMap<PaneId, Followed>,
}

/// Where a header's pill sits along its hairline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Place {
    /// A little in from the pane's left.
    #[default]
    Left,
    /// In the middle.
    Center,
    /// A little in from the pane's right.
    Right,
}

/// The plugin's settings, of its config's section: the theme's tokens of
/// its colors, by their names, its text's size and weight, its pill's
/// padding, and where its pill sits.
#[derive(Clone, Debug, PartialEq)]
struct Settings {
    /// The pill's background.
    background: String,
    /// Its text.
    text: String,
    /// Its border.
    border: String,
    /// The hairline across the pane.
    line: String,
    /// The pill's background under the pointer, over its own.
    hover: String,
    /// The text's size in points; `None` for the terminal's.
    font_size: Option<f32>,
    /// The text's weight.
    font_weight: FontWeight,
    /// The pill's padding, across and down, in points.
    padding: (f32, f32),
    place: Place,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: "panel.bg".to_owned(),
            text: "text".to_owned(),
            border: "panel.border".to_owned(),
            line: "panel.border".to_owned(),
            hover: "button.hover.bg".to_owned(),
            font_size: None,
            font_weight: FontWeight::Regular,
            padding: (8.0, 1.0),
            place: Place::Left,
        }
    }
}

impl Settings {
    /// The settings of `config`, its section, which the host checked
    /// against the schema and gave its defaults.
    fn of(config: &Data) -> Self {
        let mut settings = Self::default();
        let Data::Map(config) = config else {
            return settings;
        };
        let text = |key: &str| match config.get(key) {
            Some(Data::Text(text)) if !text.is_empty() => Some(text.clone()),
            _ => None,
        };
        let number = |key: &str| match config.get(key) {
            // Within the schema's bounds, an `f32`'s.
            Some(Data::Number(number)) => Some(*number as f32),
            Some(Data::Integer(number)) => Some(*number as f32),
            _ => None,
        };
        for (key, place) in [
            ("background", &mut settings.background),
            ("text", &mut settings.text),
            ("border", &mut settings.border),
            ("line", &mut settings.line),
            ("hover", &mut settings.hover),
        ] {
            if let Some(token) = text(key) {
                *place = token;
            }
        }
        // 0, or none, for the terminal's size.
        settings.font_size = number("font-size").filter(|&size| size > 0.0);
        if let Some(weight) = text("font-weight") {
            settings.font_weight = match weight.as_str() {
                "light" => FontWeight::Light,
                "medium" => FontWeight::Medium,
                "semibold" => FontWeight::Semibold,
                "bold" => FontWeight::Bold,
                "heavy" => FontWeight::Heavy,
                _ => FontWeight::Regular,
            };
        }
        if let Some(across) = number("padding-x") {
            settings.padding.0 = across;
        }
        if let Some(down) = number("padding-y") {
            settings.padding.1 = down;
        }
        if let Some(align) = text("align") {
            settings.place = match align.as_str() {
                "center" => Place::Center,
                "right" => Place::Right,
                _ => Place::Left,
            };
        }
        settings
    }
}

/// What the plugin follows of a pane: its view, and the header that its
/// panel shows, if one does.
#[derive(Debug, Default)]
struct Followed {
    viewport: Option<Viewport>,
    shown: Option<Shown>,
}

/// A header: its command's id, the row of its prompt, which a click
/// scrolls to the view's top, and its line.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shown {
    command: u64,
    row: i64,
    line: String,
}

impl Plugin for Header {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.settings = Settings::of(&cx.config()?);
        cx.subscribe(&[
            VIEWPORT_CHANGED,
            COMMAND_STARTED,
            COMMAND_ENDED,
            "pane-closed",
            "pane-moved",
        ])
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn config_schema(&self) -> Option<&'static str> {
        Some(include_str!("../command-header.schema.json"))
    }

    /// A pane's view, or its commands, changed: its header follows them. A
    /// pane that closed takes its header away, and one that moved to
    /// another tab opens it anew, in its window. A change of the config
    /// renders the headers that show again.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let mut looked = BTreeSet::new();
        for event in events {
            match *event {
                Event::ConfigChanged => {
                    let settings = Settings::of(&cx.config()?);
                    if settings != self.settings {
                        self.settings = settings;
                        for (&pane, followed) in &self.panes {
                            if followed.shown.is_some() {
                                cx.request_render(ViewId::Panel(panel(pane)))?;
                            }
                        }
                    }
                }
                Event::ViewportChanged { pane, viewport } => {
                    self.panes.entry(pane).or_default().viewport = Some(viewport);
                    looked.insert(pane);
                }
                Event::CommandStarted { pane, .. } | Event::CommandEnded { pane, .. } => {
                    if self.panes.contains_key(&pane) {
                        looked.insert(pane);
                    }
                }
                Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => {
                    if let Some(followed) = self.panes.remove(&pane)
                        && followed.shown.is_some()
                    {
                        // The host lets it go with its pane, if it did not yet.
                        let _ = cx.close_panel(panel(pane));
                    }
                    looked.remove(&pane);
                }
                Event::Workspace(WorkspaceEvent::PaneMoved { pane, .. }) => {
                    if let Some(followed) = self.panes.get_mut(&pane)
                        && followed.shown.take().is_some()
                    {
                        let _ = cx.close_panel(panel(pane));
                        looked.insert(pane);
                    }
                }
                _ => {}
            }
        }
        for pane in looked {
            self.look(cx, pane)?;
        }
        Ok(())
    }

    /// A header's panel: the pill of its command's line on a hairline.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::Panel(id) = view else {
            return None;
        };
        let shown = self.panes.get(&pane_of(id)?)?.shown.as_ref()?;
        let label = cx.message("header-label", &[]).unwrap_or_default();
        let tooltip = cx.message("header-tooltip", &[]).unwrap_or_default();
        Some(header(&shown.line, (&label, &tooltip), &self.settings))
    }

    /// A click on the pill scrolls the view to the command's prompt, at
    /// its top, which takes the header away.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let (ViewId::Panel(id), UiEventKind::Click) = (&event.view, &event.kind) else {
            return Ok(());
        };
        if event.key.0 != PILL {
            return Ok(());
        }
        let Some(pane) = pane_of(id) else {
            return Ok(());
        };
        let Some(shown) = self
            .panes
            .get(&pane)
            .and_then(|followed| followed.shown.as_ref())
        else {
            return Ok(());
        };
        cx.scroll(pane, Scroll::Row(shown.row))
    }
}

impl Header {
    /// Shows `pane`'s header of the command whose output its view's top row
    /// shows, while the command's line is above the view; takes it away
    /// otherwise.
    fn look(&mut self, cx: &mut Cx<'_>, pane: PaneId) -> Result<(), Error> {
        let Some(followed) = self.panes.get(&pane) else {
            return Ok(());
        };
        let wanted = match followed.viewport {
            Some(viewport) if !viewport.alternate => {
                shown_at(cx, pane, viewport, followed.shown.as_ref())
            }
            _ => None,
        };
        let Some(followed) = self.panes.get_mut(&pane) else {
            return Ok(());
        };
        match (&followed.shown, wanted) {
            (None, None) => {}
            (Some(_), None) => {
                followed.shown = None;
                cx.close_panel(panel(pane))?;
            }
            (None, Some(wanted)) => {
                followed.shown = Some(wanted);
                cx.open_panel(
                    panel(pane),
                    PanelOptions {
                        anchor: Anchor::PaneTop(pane),
                        role: Role::Overlay,
                        close_outside: false,
                        close_escape: false,
                        takes_keys: false,
                    },
                )?;
            }
            (Some(before), Some(wanted)) => {
                if *before != wanted {
                    followed.shown = Some(wanted);
                    cx.request_render(ViewId::Panel(panel(pane)))?;
                }
            }
        }
        Ok(())
    }
}

/// The header of `pane`, of `viewport`, if one shows: of the command whose
/// output the view's top row shows, whose line is above it, and that
/// started; its line read anew unless `before` is the same command's.
fn shown_at(
    cx: &mut Cx<'_>,
    pane: PaneId,
    viewport: Viewport,
    before: Option<&Shown>,
) -> Option<Shown> {
    let top = viewport.top;
    let commands = cx.shell_commands(pane, top, top + 1).ok()?;
    let command = commands.first()?;
    command.output?;
    let (_, row) = command.input?;
    if row >= top {
        return None;
    }
    // The top row past its output, of the next prompt's line or of what
    // came after its end.
    if command
        .end
        .is_some_and(|(column, end)| end < top || (end == top && column == 0))
    {
        return None;
    }
    let line = match before {
        Some(before) if before.command == command.id => before.line.clone(),
        _ => command_line(cx, pane, command)?,
    };
    Some(Shown {
        command: command.id,
        row: command.prompt.map_or(row, |(_, prompt)| prompt),
        line,
    })
}

/// The line of `command`, as the user typed it: its text from its input's
/// mark to its output's, of the input's logical line, without the spaces
/// around it; none of no text.
fn command_line(cx: &mut Cx<'_>, pane: PaneId, command: &ShellCommand) -> Option<String> {
    let (column, row) = command.input?;
    let line = cx.line_text(pane, row, LINE_REACH).ok()??;
    // In reading order: a row's number, then a column.
    let from = (row, column);
    let to = command.output.map(|(column, row)| (row, column));
    let text: String = line
        .text
        .chars()
        .zip(line.cells)
        .filter(|&(_, (column, row))| {
            (row, column) >= from && to.is_none_or(|to| (row, column) < to)
        })
        .map(|(character, _)| character)
        .take(MOST_LINE)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The id of `pane`'s header's panel.
fn panel(pane: PaneId) -> PanelId {
    PanelId(format!("{PANEL}{}", pane.0))
}

/// The pane of a header's panel, by its id.
fn pane_of(panel: &PanelId) -> Option<PaneId> {
    panel.0.strip_prefix(PANEL)?.parse().ok().map(PaneId)
}

/// A header of `line`, of `label` to accessibility and of `tooltip`: a pill
/// of the line, in the terminal's font, on a hairline across the pane, of
/// the theme's tokens that `settings` name, its text of their size and
/// weight, its padding theirs, and its place along the line theirs.
fn header(line: &str, (label, tooltip): (&str, &str), settings: &Settings) -> Node {
    let hairline = |width: Length, grow: f32| {
        Node::container(Vec::new())
            .layout(Layout {
                width,
                grow,
                height: Length::Points(1.0),
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from(settings.line.as_str())),
                ..Style::default()
            })
    };
    let (across, down) = settings.padding;
    let pill = Node::container(vec![Node::text(line)])
        .key(PILL)
        .label(label)
        .tooltip(tooltip)
        .layout(Layout {
            padding: Edges {
                top: down,
                right: across,
                bottom: down,
                left: across,
            },
            align: Align::Center,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from(settings.background.as_str())),
            border: Some(Border {
                color: Token::from(settings.border.as_str()),
                width: 1.0,
            }),
            color: Some(Token::from(settings.text.as_str())),
            // Round at either end, as the renderer takes a radius of half
            // the box's height at most.
            radius: PILL_RADIUS,
            font_family: Some(FontFamily::Terminal),
            font_size: settings.font_size,
            font_weight: Some(settings.font_weight),
            pointer: Some(PointerShape::PointingHand),
            transition: Some(Transition {
                duration: FADE,
                ..Transition::default()
            }),
            hover: Some(Hover {
                background: Some(Token::from(settings.hover.as_str())),
                ..Hover::default()
            }),
            ..Style::default()
        });
    let lead = || hairline(Length::Points(LEAD), 0.0);
    let rest = || hairline(Length::Auto, 1.0);
    let children = match settings.place {
        Place::Left => vec![lead(), pill, rest()],
        Place::Center => vec![rest(), pill, rest()],
        Place::Right => vec![rest(), pill, lead()],
    };
    Node::container(children).layout(Layout {
        align: Align::Center,
        grow: 1.0,
        ..Layout::default()
    })
}

#[cfg(test)]
mod tests;

//! The command's header: the command whose output a pane's view shows, at
//! the pane's top, while the command's line is above the view, as the
//! shell marks its commands. It sits over the pane's first row, a pill of
//! the command's line on a hairline across the pane, and takes none of the
//! grid's rows, so that the grid keeps its size as it shows and hides. As
//! the view scrolls up to the command's line, the header goes, and the
//! command before it takes its place once that one's line is above the
//! view; down, the next one does. A click scrolls the view to the
//! command's prompt; a header of a button to its block's end, while the
//! view does not show that end, scrolls there at the button's click. The
//! alternate screen shows none. Its text is the terminal's font's; its
//! config names the theme's tokens of its colors, and sets its text's size
//! and weight, its pill's padding and where the pill sits.
//!
//! The plugins that show the header hold this file as a module of their
//! own, by its path, as a built-in plugin depends on the plugin SDK alone.

use std::collections::{BTreeMap, BTreeSet};

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{PaneId, Scroll, Viewport};
use lokyr_plugin_sdk::message::{COMMAND_ENDED, COMMAND_STARTED, Data, Event, VIEWPORT_CHANGED};
use lokyr_plugin_sdk::plugin::Cx;
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Border, Edges, FontFamily, FontWeight, Hover, Layout, Length, Node, PointerShape, Style,
    Token, Transition, UiEventKind,
};

/// The key of a header's pill, which a click scrolls by.
pub(crate) const PILL: &str = "command";

/// The key of a header's button to its block's end.
pub(crate) const END: &str = "end";

/// The most characters of a command's line that a header shows, past which
/// the pill ends where the pane does.
const MOST_LINE: usize = 500;

/// The length of the hairline at the pill's left, in points.
pub(crate) const LEAD: f32 = 12.0;

/// The length of the hairline between the pill and the button to the
/// block's end, in points.
pub(crate) const BETWEEN: f32 = 4.0;

/// How long the pill's hover fades, in seconds.
const FADE: f32 = 0.12;

/// The pill's corners' radius, in points, past half its height, as the
/// renderer takes half the box's height at most: round at either end.
const PILL_RADIUS: f32 = 100.0;

/// A plugin's headers: the prefix of their panels' ids, whether they have
/// a button to their blocks' ends, the settings, and what they follow of
/// each pane whose view the plugin heard of.
#[derive(Debug)]
pub(crate) struct Headers {
    prefix: &'static str,
    ends: bool,
    settings: Settings,
    pub(crate) panes: BTreeMap<PaneId, Followed>,
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

/// The headers' settings, of the plugin's config's section: the theme's
/// tokens of their colors, by their names, their text's size and weight,
/// their pill's padding, and where their pill sits.
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
    /// The most characters of the line shown.
    max_length: usize,
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
            max_length: 80,
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
        // Within the schema's bounds, of 1 at least.
        if let Some(most) = number("max-length") {
            settings.max_length = (most.max(1.0)) as usize;
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

/// What the headers follow of a pane: its view, and the header that its
/// panel shows, if one does.
#[derive(Debug, Default)]
pub(crate) struct Followed {
    viewport: Option<Viewport>,
    shown: Option<Shown>,
}

/// A header: its command's id, the row of its prompt, which a click
/// scrolls to the view's top, its line, and the scroll to its block's end,
/// of a header of a button to it while the view does not show that end.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Shown {
    command: u64,
    row: i64,
    line: String,
    end: Option<Scroll>,
}

impl Headers {
    /// Headers whose panels' ids are `prefix` and their panes' ids, of a
    /// button to their blocks' ends if `ends`.
    pub(crate) fn new(prefix: &'static str, ends: bool) -> Self {
        Self {
            prefix,
            ends,
            settings: Settings::default(),
            panes: BTreeMap::new(),
        }
    }

    /// Takes the config's settings, and subscribes to the panes' views and
    /// commands.
    pub(crate) fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.settings = Settings::of(&cx.config()?);
        cx.subscribe(&[
            VIEWPORT_CHANGED,
            COMMAND_STARTED,
            COMMAND_ENDED,
            "pane-closed",
            "pane-moved",
        ])
    }

    /// A pane's view, or its commands, changed: its header follows them. A
    /// pane that closed takes its header away, and one that moved to
    /// another tab opens it anew, in its window, as does a header that the
    /// host closed, as its tab went to another window. A change of the
    /// config renders the headers that show again.
    pub(crate) fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let mut looked = BTreeSet::new();
        for event in events {
            match *event {
                Event::ConfigChanged => {
                    let settings = Settings::of(&cx.config()?);
                    if settings != self.settings {
                        self.settings = settings;
                        for (&pane, followed) in &self.panes {
                            if followed.shown.is_some() {
                                cx.request_render(ViewId::Panel(self.panel(pane)))?;
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
                        let _ = cx.close_panel(self.panel(pane));
                    }
                    looked.remove(&pane);
                }
                // The host closed a header, as its tab went to another
                // window: it opens anew where its pane is, as it looks.
                Event::PanelClosed(ref id) => {
                    if let Some(pane) = self.pane_of(id)
                        && let Some(followed) = self.panes.get_mut(&pane)
                        && followed.shown.take().is_some()
                    {
                        looked.insert(pane);
                    }
                }
                Event::Workspace(WorkspaceEvent::PaneMoved { pane, .. }) => {
                    let panel = self.panel(pane);
                    if let Some(followed) = self.panes.get_mut(&pane)
                        && followed.shown.take().is_some()
                    {
                        let _ = cx.close_panel(panel);
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

    /// A header's panel: the pill of its command's line on a hairline, and
    /// its button to its block's end, if it has one.
    pub(crate) fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::Panel(id) = view else {
            return None;
        };
        let shown = self.panes.get(&self.pane_of(id)?)?.shown.as_ref()?;
        // Its whole line to accessibility and in its tooltips.
        let whole = [("line", Data::Text(shown.line.clone()))];
        let label = cx.message("header-label", &whole).unwrap_or_default();
        let tooltip = cx.message("header-tooltip", &whole).unwrap_or_default();
        let end = shown
            .end
            .map(|_| cx.message("header-end", &whole).unwrap_or_default());
        let line = cut(&shown.line, self.settings.max_length);
        Some(header(
            &line,
            (&label, &tooltip),
            end.as_deref(),
            &self.settings,
        ))
    }

    /// A click on the pill scrolls the view to the command's prompt, at
    /// its top, which takes the header away; one on the button to the
    /// block's end, to that end.
    pub(crate) fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let (ViewId::Panel(id), UiEventKind::Click) = (&event.view, &event.kind) else {
            return Ok(());
        };
        let Some(pane) = self.pane_of(id) else {
            return Ok(());
        };
        let Some(shown) = self
            .panes
            .get(&pane)
            .and_then(|followed| followed.shown.as_ref())
        else {
            return Ok(());
        };
        let scroll = match event.key.0.as_str() {
            PILL => Scroll::Row(shown.row),
            END => match shown.end {
                Some(end) => end,
                None => return Ok(()),
            },
            _ => return Ok(()),
        };
        cx.scroll(pane, scroll)
    }

    /// Shows `pane`'s header of the command whose output its view's top row
    /// shows, while the command's line is above the view; takes it away
    /// otherwise.
    fn look(&mut self, cx: &mut Cx<'_>, pane: PaneId) -> Result<(), Error> {
        let Some(followed) = self.panes.get(&pane) else {
            return Ok(());
        };
        let wanted = match followed.viewport {
            Some(viewport) if !viewport.alternate => {
                shown_at(cx, pane, viewport, followed.shown.as_ref(), self.ends)
            }
            _ => None,
        };
        let panel = self.panel(pane);
        let Some(followed) = self.panes.get_mut(&pane) else {
            return Ok(());
        };
        match (&followed.shown, wanted) {
            (None, None) => {}
            (Some(_), None) => {
                followed.shown = None;
                cx.close_panel(panel)?;
            }
            (None, Some(wanted)) => {
                followed.shown = Some(wanted);
                cx.open_panel(
                    panel,
                    PanelOptions {
                        anchor: Anchor::PaneTop(pane),
                        role: Role::Overlay,
                        close_outside: false,
                        close_escape: false,
                        takes_keys: false,
                        close_typing: false,
                    },
                )?;
            }
            (Some(before), Some(wanted)) => {
                if *before != wanted {
                    followed.shown = Some(wanted);
                    cx.request_render(ViewId::Panel(panel))?;
                }
            }
        }
        Ok(())
    }

    /// The id of `pane`'s header's panel.
    pub(crate) fn panel(&self, pane: PaneId) -> PanelId {
        PanelId(format!("{}{}", self.prefix, pane.0))
    }

    /// The pane of a header's panel, by its id.
    fn pane_of(&self, panel: &PanelId) -> Option<PaneId> {
        panel.0.strip_prefix(self.prefix)?.parse().ok().map(PaneId)
    }
}

/// The header of `pane`, of `viewport`, if one shows: of the command whose
/// output the view's top row shows, whose line is above it, and that
/// started; its line read anew unless `before` is the same command's; the
/// scroll to its block's end, if `ends`, while the view does not show it.
fn shown_at(
    cx: &mut Cx<'_>,
    pane: PaneId,
    viewport: Viewport,
    before: Option<&Shown>,
    ends: bool,
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
    // As the shell told it, which leaves its right prompt out, or as its
    // text reads; the lines of a command of several on one.
    let line = match before {
        Some(before) if before.command == command.id => before.line.clone(),
        _ => cx
            .command_line(pane, command.id)
            .ok()??
            .chars()
            .take(MOST_LINE)
            .map(|character| if character == '\n' { ' ' } else { character })
            .collect(),
    };
    Some(Shown {
        command: command.id,
        row: command.prompt.map_or(row, |(_, prompt)| prompt),
        line,
        end: if ends {
            to_end(command.end, viewport)
        } else {
            None
        },
    })
}

/// The scroll to the end of a block whose command ended at `end`, of
/// `viewport`, if the view does not show it: the block's last row at the
/// view's bottom; the screen's, which the view then follows, of a command
/// that has not ended.
fn to_end(end: Option<(u16, i64)>, viewport: Viewport) -> Option<Scroll> {
    let rows = i64::from(viewport.rows);
    match end {
        Some((column, row)) => {
            // An end at a row's start, as an output's last new line leaves
            // it, follows the block's last row.
            let last = if column == 0 { row - 1 } else { row };
            (last >= viewport.top + rows).then_some(Scroll::Row(last + 1 - rows))
        }
        None => (viewport.top < viewport.screen).then_some(Scroll::Bottom),
    }
}

/// `line`, of `most` characters at most: its first ones and `…` past them,
/// the spaces before the ellipsis left out.
fn cut(line: &str, most: usize) -> String {
    if line.chars().count() <= most {
        return line.to_owned();
    }
    let kept: String = line.chars().take(most).collect();
    format!("{}…", kept.trim_end())
}

/// A header of `line`, of `label` to accessibility and of `tooltip`: a pill
/// of the line, in the terminal's font, on a hairline across the pane, of
/// the theme's tokens that `settings` name, its text of their size and
/// weight, its padding theirs, and its place along the line theirs; the
/// pill within the pane, its line cut of an ellipsis where the pane is
/// narrower, as the layout shrinks it. A button to the block's end, of
/// `end` to accessibility and in its tooltip, follows the pill, if given.
fn header(
    line: &str,
    (label, tooltip): (&str, &str),
    end: Option<&str>,
    settings: &Settings,
) -> Node {
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
    // A pill's look, of the theme's tokens, round at either end, as the
    // renderer takes a radius of half the box's height at most.
    let look = || Style {
        background: Some(Token::from(settings.background.as_str())),
        border: Some(Border {
            color: Token::from(settings.border.as_str()),
            width: 1.0,
        }),
        color: Some(Token::from(settings.text.as_str())),
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
    };
    // Of no least width but their padding's, so that the layout shrinks
    // them within the pane, which the host's ellipsis cuts.
    let text = Node::text(line).layout(Layout {
        min_width: Length::Points(0.0),
        ..Layout::default()
    });
    let pill = Node::container(vec![text])
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
            min_width: Length::Points(0.0),
            ..Layout::default()
        })
        .style(look());
    // A disc of the arrow, of the pill's height, as the icon takes a square
    // of the text's line.
    let end = end.map(|end| {
        Node::container(vec![Node::icon("arrow-down")])
            .key(END)
            .label(end)
            .tooltip(end)
            .layout(Layout {
                padding: Edges::all(down),
                align: Align::Center,
                ..Layout::default()
            })
            .style(look())
    });
    let lead = || hairline(Length::Points(LEAD), 0.0);
    let rest = || hairline(Length::Auto, 1.0);
    let mut beads = vec![pill];
    if let Some(end) = end {
        beads.extend([hairline(Length::Points(BETWEEN), 0.0), end]);
    }
    let (before, after) = match settings.place {
        Place::Left => (lead(), rest()),
        Place::Center => (rest(), rest()),
        Place::Right => (rest(), lead()),
    };
    let mut children = vec![before];
    children.extend(beads);
    children.push(after);
    Node::container(children).layout(Layout {
        align: Align::Center,
        grow: 1.0,
        ..Layout::default()
    })
}

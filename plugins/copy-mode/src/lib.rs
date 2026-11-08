//! Built-in plugin of the copy mode: a cursor of the keyboard's over a
//! pane's scrollback and screen, which vi's keys move, as tmux's copy mode
//! has them, and which selects from where a selection started, of cells,
//! of whole lines or of a block, and copies. It starts at the terminal's
//! cursor, or at the view's last row while the view is up in the
//! scrollback; while it runs, a badge at the pane's bottom right tells its
//! mode, and the keys that no binding takes go nowhere, rather than to the
//! program, the pane keeping the keyboard. It ends as another pane of its
//! tab takes the keyboard, as the alternate screen shows, and as a resize
//! lays the rows out anew. Like every built-in plugin, it depends only on
//! the plugin SDK.

#![forbid(unsafe_code)]

mod motion;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{LineText, PaneId, Scroll, SelectionKind, TabId, Viewport};
use lokyr_plugin_sdk::message::{Event, VIEWPORT_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{
    Align, Edges, FontWeight, Layout, Node, PointerShape, Style, TextSize, Token, UiEventKind,
};

use motion::{Bounds, Edge, Text};

lokyr_plugin_sdk::export_plugin!(CopyMode);

/// The plugin's catalogs, of its commands' titles and its badge's labels.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/copy-mode.ftl")),
    ("it", include_str!("../locales/it/copy-mode.ftl")),
];

/// The command that starts the copy mode in the focused pane, or ends it.
pub const TOGGLE: &str = "copy-mode.toggle";

/// The command that moves the cursor, `to` a place of [`MOTIONS`].
pub const MOVE: &str = "copy-mode.move";

/// The command that starts a selection at the cursor, of a `kind` of
/// [`KINDS`], or changes its kind, or takes it away, of its own kind.
pub const SELECT: &str = "copy-mode.select";

/// The command that adds a `digit` to the count of the next move, or moves
/// to the line's start, a 0 of no count.
pub const COUNT: &str = "copy-mode.count";

/// The command of an `action` of [`ACTIONS`].
pub const ACT: &str = "copy-mode.act";

/// The context of the keys of the copy mode: while its badge shows, and the
/// pane has the keyboard, not a panel's field.
pub const WHEN: &str = "copy-mode.open && pane.focused";

/// The id of the badge's panel.
const BADGE: &str = "copy-mode";

/// The key of the badge's node, which a click ends the mode by.
const BADGE_KEY: &str = "badge";

/// The most that a count reaches.
const MOST_COUNT: u32 = 99_999;

/// The badge's corners' radius, in points, past half its height: round at
/// either end.
const BADGE_RADIUS: f32 = 100.0;

/// Where a move takes the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordNext,
    WordPrevious,
    WordEnd,
    BigWordNext,
    BigWordPrevious,
    BigWordEnd,
    LineStart,
    LineFirst,
    LineEnd,
    Top,
    Bottom,
    ViewTop,
    ViewMiddle,
    ViewBottom,
    HalfPageUp,
    HalfPageDown,
    PageUp,
    PageDown,
    ParagraphPrevious,
    ParagraphNext,
}

/// The places of a move, as its `to` names them.
pub const MOTIONS: [&str; 24] = [
    "left",
    "right",
    "up",
    "down",
    "word-next",
    "word-previous",
    "word-end",
    "big-word-next",
    "big-word-previous",
    "big-word-end",
    "line-start",
    "line-first",
    "line-end",
    "top",
    "bottom",
    "view-top",
    "view-middle",
    "view-bottom",
    "half-page-up",
    "half-page-down",
    "page-up",
    "page-down",
    "paragraph-previous",
    "paragraph-next",
];

/// The moves of [`MOTIONS`], in their order.
const MOTION_OF: [Motion; 24] = [
    Motion::Left,
    Motion::Right,
    Motion::Up,
    Motion::Down,
    Motion::WordNext,
    Motion::WordPrevious,
    Motion::WordEnd,
    Motion::BigWordNext,
    Motion::BigWordPrevious,
    Motion::BigWordEnd,
    Motion::LineStart,
    Motion::LineFirst,
    Motion::LineEnd,
    Motion::Top,
    Motion::Bottom,
    Motion::ViewTop,
    Motion::ViewMiddle,
    Motion::ViewBottom,
    Motion::HalfPageUp,
    Motion::HalfPageDown,
    Motion::PageUp,
    Motion::PageDown,
    Motion::ParagraphPrevious,
    Motion::ParagraphNext,
];

/// The kinds of a selection, as its `kind` names them: of cells, of whole
/// lines, or of a block.
pub const KINDS: [&str; 3] = ["cells", "lines", "block"];

/// The actions, as an `action` names them: copy the selection, or the
/// cursor's line without one, and end; copy the cursor's line and end; swap
/// the selection's ends; take the selection away, or end without one; end.
pub const ACTIONS: [&str; 5] = ["copy", "copy-line", "swap", "back", "exit"];

/// The plugin, and the copy mode that runs, if one does.
#[derive(Debug, Default)]
pub struct CopyMode {
    copying: Option<Copying>,
}

/// A copy mode: its pane and the pane's tab, its cursor, its selection,
/// the count of the next move, and the epoch of the rows' numbers, which a
/// resize changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Copying {
    pane: PaneId,
    tab: Option<TabId>,
    at: (u16, i64),
    selection: Option<Selection>,
    count: Option<u32>,
    epoch: u64,
}

/// A selection: the cell where it started, and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    anchor: (u16, i64),
    kind: SelectionKind,
}

impl Plugin for CopyMode {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let param = |name: &str, kind: Kind| Param {
            name: name.to_owned(),
            kind,
            default: None,
        };
        let commands = [
            (TOGGLE, "Copy Mode", None),
            (MOVE, "Move in Copy Mode", Some(param("to", Kind::String))),
            (
                SELECT,
                "Select in Copy Mode",
                Some(param("kind", Kind::String)),
            ),
            (
                COUNT,
                "Count in Copy Mode",
                Some(param("digit", Kind::Integer)),
            ),
            (ACT, "Act in Copy Mode", Some(param("action", Kind::String))),
        ];
        for (name, title, param) in commands {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category: Category::Edit,
                capabilities: Vec::new(),
                when: param.is_some().then(|| WHEN.to_owned()),
                params: param.into_iter().collect(),
            })?;
        }
        cx.subscribe(&[
            VIEWPORT_CHANGED,
            "pane-focused",
            "pane-closed",
            "pane-moved",
        ])
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        if name == TOGGLE {
            return match self.copying.as_ref().map(|copying| copying.pane) {
                Some(pane) if cx.focused_pane() == Some(pane) => self.end(cx, true),
                _ => {
                    self.end(cx, true)?;
                    self.start(cx)
                }
            };
        }
        if self.copying.is_none() {
            return Ok(());
        }
        match name {
            MOVE => {
                let to = text_arg(&args, "to")?;
                let motion = MOTIONS
                    .iter()
                    .position(|name| *name == to)
                    .map(|at| MOTION_OF[at])
                    .ok_or_else(|| wrong(format!("`{to}` is not a place of a move")))?;
                self.moved(cx, motion)
            }
            SELECT => {
                let kind = match text_arg(&args, "kind")? {
                    "cells" => SelectionKind::Cells,
                    "lines" => SelectionKind::Lines,
                    "block" => SelectionKind::Block,
                    other => return Err(wrong(format!("`{other}` is not a selection's kind"))),
                };
                self.select(cx, kind)
            }
            COUNT => {
                let digit = match args.get("digit") {
                    Some(Value::Integer(digit @ 0..=9)) => *digit as u32,
                    _ => return Err(wrong("a digit is from 0 to 9".to_owned())),
                };
                self.counted(cx, digit)
            }
            ACT => match text_arg(&args, "action")? {
                "copy" => self.copy(cx, false),
                "copy-line" => self.copy(cx, true),
                "swap" => self.swap(cx),
                "back" => self.back(cx),
                "exit" => self.end(cx, true),
                other => Err(wrong(format!("`{other}` is not an action"))),
            },
            _ => Ok(()),
        }
    }

    /// Another pane of its tab that takes the keyboard ends it, and so do
    /// the alternate screen, which has a cursor of its own, a resize, which
    /// numbers the rows anew, its pane's move to another tab or its
    /// closing, and its badge's closing by the host, as its tab goes to
    /// another window.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            let Some(copying) = &self.copying else {
                return Ok(());
            };
            let ends = match event {
                Event::ViewportChanged { pane, viewport } => {
                    *pane == copying.pane && (viewport.alternate || viewport.epoch != copying.epoch)
                }
                Event::Workspace(WorkspaceEvent::PaneFocused { pane, tab }) => {
                    *pane != copying.pane && Some(*tab) == copying.tab
                }
                Event::Workspace(WorkspaceEvent::PaneMoved { pane, .. }) => *pane == copying.pane,
                Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => *pane == copying.pane,
                // The host closed the badge, as its pane closed or its tab
                // went to another window: the terminal's cursor back.
                Event::PanelClosed(id) => {
                    if *id == badge() {
                        let pane = copying.pane;
                        self.copying = None;
                        // A pane that closed took its cursor with it.
                        let _ = cx.show_cursor(pane, None);
                    }
                    false
                }
                _ => false,
            };
            if ends {
                self.end(cx, false)?;
            }
        }
        Ok(())
    }

    /// The badge: the mode, and the count typed, if one.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view != ViewId::Panel(badge()) {
            return None;
        }
        let copying = self.copying.as_ref()?;
        let mode = match copying.selection.map(|selection| selection.kind) {
            None => "badge-copy",
            Some(SelectionKind::Lines) => "badge-lines",
            Some(SelectionKind::Block) => "badge-block",
            Some(_) => "badge-cells",
        };
        let mut text = cx.message(mode, &[]).unwrap_or_default();
        if let Some(count) = copying.count {
            text = format!("{text} {count}");
        }
        let tooltip = cx.message("badge-tooltip", &[]).unwrap_or_default();
        Some(badge_view(&text, &tooltip))
    }

    /// A click on the badge ends the mode.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if event.view == ViewId::Panel(badge())
            && event.key.0 == BADGE_KEY
            && event.kind == UiEventKind::Click
        {
            return self.end(cx, true);
        }
        Ok(())
    }
}

impl CopyMode {
    /// Starts the copy mode in the focused pane, at the terminal's cursor,
    /// or at the first cell of the view's last row, of a view up in the
    /// scrollback; on the alternate screen, none.
    fn start(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        let viewport = cx.viewport(pane)?;
        if viewport.alternate {
            return Ok(());
        }
        let (columns, _) = cx.pane_grid(pane)?;
        let bottom = viewport.top + i64::from(viewport.rows) - 1;
        let at = match cx.cursor_cell(pane)? {
            Some((column, row)) if (viewport.top..=bottom).contains(&row) => {
                (column.min(columns.saturating_sub(1)), row)
            }
            _ => (0, bottom),
        };
        let tab = match cx.focused_window() {
            Some(window) => cx
                .tabs(window)?
                .into_iter()
                .find(|tab| tab.panes.contains(&pane))
                .map(|tab| tab.id),
            None => None,
        };
        self.copying = Some(Copying {
            pane,
            tab,
            at,
            selection: None,
            count: None,
            epoch: viewport.epoch,
        });
        cx.show_cursor(pane, Some(at))?;
        cx.open_panel(
            badge(),
            PanelOptions {
                anchor: Anchor::PaneCorner {
                    pane,
                    corner: Corner::BottomRight,
                },
                role: Role::Overlay,
                close_outside: false,
                close_escape: false,
                takes_keys: true,
            },
        )
    }

    /// Ends the copy mode: the terminal's cursor back, the selection taken
    /// away as `deselect` says, and the badge closed.
    fn end(&mut self, cx: &mut Cx<'_>, deselect: bool) -> Result<(), Error> {
        let Some(copying) = self.copying.take() else {
            return Ok(());
        };
        // A pane that closed took its cursor and its selection with it.
        let _ = cx.show_cursor(copying.pane, None);
        if deselect && copying.selection.is_some() {
            let _ = cx.deselect(copying.pane);
        }
        let _ = cx.close_panel(badge());
        Ok(())
    }

    /// Moves the cursor as `motion` says, as many times as the count says,
    /// within the rows that the pane holds and its columns.
    fn moved(&mut self, cx: &mut Cx<'_>, motion: Motion) -> Result<(), Error> {
        let Some(copying) = self.copying.as_mut() else {
            return Ok(());
        };
        let count = copying.count.take().unwrap_or(1);
        let pane = copying.pane;
        let viewport = cx.viewport(pane)?;
        let (columns, _) = cx.pane_grid(pane)?;
        let bounds = Bounds {
            oldest: viewport.oldest,
            last: viewport.screen + i64::from(viewport.rows) - 1,
        };
        let rows = i64::from(viewport.rows);
        let last_column = columns.saturating_sub(1);
        let mut at = clamped(copying.at, bounds, last_column);
        let mut scroll = None;
        let times = i64::from(count);
        let mut text = Reader { cx, pane };
        match motion {
            Motion::Left => {
                at.0 =
                    at.0.saturating_sub(u16::try_from(count).unwrap_or(u16::MAX));
            }
            Motion::Right => {
                let right = u16::try_from(count).unwrap_or(u16::MAX);
                at.0 = at.0.saturating_add(right).min(last_column);
            }
            Motion::Up => at.1 -= times,
            Motion::Down => at.1 += times,
            Motion::WordNext | Motion::BigWordNext => {
                let big = motion == Motion::BigWordNext;
                at = repeated(count, at, |at| {
                    motion::word_next(&mut text, bounds, at, big)
                });
            }
            Motion::WordPrevious | Motion::BigWordPrevious => {
                let big = motion == Motion::BigWordPrevious;
                at = repeated(count, at, |at| {
                    motion::word_previous(&mut text, bounds, at, big)
                });
            }
            Motion::WordEnd | Motion::BigWordEnd => {
                let big = motion == Motion::BigWordEnd;
                at = repeated(count, at, |at| motion::word_end(&mut text, bounds, at, big));
            }
            Motion::LineStart => at = motion::line_edge(&mut text, at, Edge::Start),
            Motion::LineFirst => at = motion::line_edge(&mut text, at, Edge::First),
            // A count goes down as many lines less one, as vi's `$` does.
            Motion::LineEnd => {
                at.1 += times - 1;
                at = clamped(at, bounds, last_column);
                at = motion::line_edge(&mut text, at, Edge::Last);
            }
            Motion::Top => at = (0, bounds.oldest),
            Motion::Bottom => {
                let cursor = text
                    .cx
                    .cursor_cell(pane)?
                    .map_or(bounds.last, |cell| cell.1);
                at = motion::bottom(&mut text, bounds, cursor);
            }
            // A count takes the row as many less one from the view's edge.
            Motion::ViewTop => at.1 = viewport.top + times - 1,
            Motion::ViewMiddle => at.1 = viewport.top + (rows - 1) / 2,
            Motion::ViewBottom => at.1 = viewport.top + rows - times,
            Motion::HalfPageUp | Motion::HalfPageDown | Motion::PageUp | Motion::PageDown => {
                let page = match motion {
                    Motion::HalfPageUp | Motion::HalfPageDown => (rows / 2).max(1),
                    _ => rows.max(1),
                };
                let by = match motion {
                    Motion::HalfPageUp | Motion::PageUp => -page * times,
                    _ => page * times,
                };
                at.1 += by;
                scroll = Some(viewport.top + by);
            }
            Motion::ParagraphPrevious | Motion::ParagraphNext => {
                let up = motion == Motion::ParagraphPrevious;
                at = repeated(count, at, |at| motion::paragraph(&mut text, bounds, at, up));
            }
        }
        let at = clamped(at, bounds, last_column);
        self.place(cx, at, scroll, viewport)
    }

    /// Puts the cursor at `at`, the view first at the row `scroll` at its
    /// top, if one, then brought to the cursor's row, and the selection
    /// from its start to the cursor, if one.
    fn place(
        &mut self,
        cx: &mut Cx<'_>,
        at: (u16, i64),
        scroll: Option<i64>,
        viewport: Viewport,
    ) -> Result<(), Error> {
        let Some(copying) = self.copying.as_mut() else {
            return Ok(());
        };
        copying.at = at;
        let pane = copying.pane;
        cx.show_cursor(pane, Some(at))?;
        let rows = i64::from(viewport.rows);
        let top = scroll.map_or(viewport.top, |top| {
            top.clamp(viewport.oldest, viewport.screen)
        });
        if let Some(top) = scroll {
            cx.scroll(pane, Scroll::Row(top))?;
        }
        if let Some(selection) = copying.selection {
            // The selection brings its last cell, the cursor, into the view.
            return cx.select_range(pane, selection.anchor, at, selection.kind);
        }
        if at.1 < top {
            cx.scroll(pane, Scroll::Row(at.1))?;
        } else if at.1 > top + rows - 1 {
            cx.scroll(pane, Scroll::Row(at.1 - rows + 1))?;
        }
        Ok(())
    }

    /// Adds `digit` to the count; a 0 of no count moves to the line's
    /// start, as vi's `0` does.
    fn counted(&mut self, cx: &mut Cx<'_>, digit: u32) -> Result<(), Error> {
        let Some(copying) = self.copying.as_mut() else {
            return Ok(());
        };
        if digit == 0 && copying.count.is_none() {
            return self.moved(cx, Motion::LineStart);
        }
        let count = copying.count.unwrap_or(0);
        copying.count = Some((count * 10 + digit).min(MOST_COUNT));
        cx.request_render(ViewId::Panel(badge()))
    }

    /// Starts a selection of `kind` at the cursor, or changes the
    /// selection's kind, or takes it away, of its own kind.
    fn select(&mut self, cx: &mut Cx<'_>, kind: SelectionKind) -> Result<(), Error> {
        let Some(copying) = self.copying.as_mut() else {
            return Ok(());
        };
        copying.count = None;
        let pane = copying.pane;
        match &mut copying.selection {
            Some(selection) if selection.kind == kind => {
                copying.selection = None;
                cx.deselect(pane)?;
            }
            Some(selection) => {
                selection.kind = kind;
                cx.select_range(pane, selection.anchor, copying.at, kind)?;
            }
            None => {
                copying.selection = Some(Selection {
                    anchor: copying.at,
                    kind,
                });
                cx.select_range(pane, copying.at, copying.at, kind)?;
            }
        }
        cx.request_render(ViewId::Panel(badge()))
    }

    /// Copies the selection, or the cursor's logical line, of `line` or
    /// without a selection, without the blanks at its end, and ends; the
    /// selection stays, as a copy leaves it, but of a line's copy.
    fn copy(&mut self, cx: &mut Cx<'_>, line: bool) -> Result<(), Error> {
        let Some(copying) = self.copying.as_ref() else {
            return Ok(());
        };
        let pane = copying.pane;
        let selected = copying.selection.is_some() && !line;
        let text = if selected {
            cx.selection_text(pane)?
        } else {
            cx.line_text(pane, copying.at.1, motion::LINE_REACH)?
                .map(|line| line.text.trim_end().to_owned())
        };
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            cx.set_clipboard_text(&text)?;
        }
        self.end(cx, !selected)
    }

    /// Swaps the selection's ends: the cursor goes to where it started.
    fn swap(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(copying) = self.copying.as_mut() else {
            return Ok(());
        };
        copying.count = None;
        let Some(selection) = &mut copying.selection else {
            return Ok(());
        };
        let at = std::mem::replace(&mut selection.anchor, copying.at);
        let viewport = cx.viewport(copying.pane)?;
        self.place(cx, at, None, viewport)
    }

    /// Takes the selection away, or the count, or ends without either.
    fn back(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let Some(copying) = self.copying.as_mut() else {
            return Ok(());
        };
        if copying.count.take().is_some() {
            return cx.request_render(ViewId::Panel(badge()));
        }
        if copying.selection.take().is_some() {
            cx.deselect(copying.pane)?;
            return cx.request_render(ViewId::Panel(badge()));
        }
        self.end(cx, false)
    }
}

/// `step` taken `count` times from `at`.
fn repeated(
    count: u32,
    at: (u16, i64),
    mut step: impl FnMut((u16, i64)) -> (u16, i64),
) -> (u16, i64) {
    (0..count).fold(at, |at, _| step(at))
}

/// The reads of a pane's text that the moves take.
struct Reader<'c, 'h> {
    cx: &'c mut Cx<'h>,
    pane: PaneId,
}

impl Text for Reader<'_, '_> {
    fn line(&mut self, row: i64, reach: usize) -> Option<LineText> {
        self.cx.line_text(self.pane, row, reach).ok().flatten()
    }
}

/// `at` within `bounds`' rows and the columns to `last_column`.
fn clamped((column, row): (u16, i64), bounds: Bounds, last_column: u16) -> (u16, i64) {
    (
        column.min(last_column),
        row.clamp(bounds.oldest, bounds.last),
    )
}

/// The text argument `name` of `args`.
fn text_arg<'a>(args: &'a Args, name: &str) -> Result<&'a str, Error> {
    match args.get(name) {
        Some(Value::String(text)) => Ok(text),
        _ => Err(Error::new(
            Code::MissingArgument,
            format!("the argument `{name}`, a text, is needed"),
        )),
    }
}

fn wrong(message: String) -> Error {
    Error::new(Code::Failed, message)
}

/// The id of the badge's panel.
fn badge() -> PanelId {
    PanelId::from(BADGE)
}

/// The badge of `text`, and of `tooltip`: a pill of the accent's colors,
/// of the interface's small text, which takes the pointing hand.
fn badge_view(text: &str, tooltip: &str) -> Node {
    Node::container(vec![Node::text(text)])
        .key(BADGE_KEY)
        .label(text)
        .tooltip(tooltip)
        .layout(Layout {
            padding: Edges {
                top: 2.0,
                right: 8.0,
                bottom: 2.0,
                left: 8.0,
            },
            align: Align::Center,
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("list.selection.bg")),
            color: Some(Token::from("list.selection.fg")),
            radius: BADGE_RADIUS,
            text_size: Some(TextSize::Small),
            font_weight: Some(FontWeight::Semibold),
            pointer: Some(PointerShape::PointingHand),
            ..Style::default()
        })
}

#[cfg(test)]
mod tests;

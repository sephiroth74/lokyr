//! Built-in plugin of the splits.
//!
//! Its commands split the focused pane, right or down; move the keyboard to
//! the pane beside it in a direction, or to the next or the previous of its
//! tab's; move a divider by cells; zoom the pane, or end the zoom; share
//! every split by its panes again; swap the pane with the one beside it;
//! and move it out to a tab or a window of its own. The system's keymap
//! binds their keys. Its config fades the panes that have not the
//! keyboard, has the keyboard follow the pointer, and shows each pane's
//! title in its header. The tabs' layouts are the host's, which the
//! commands change through the HostApi. Like every built-in plugin, it
//! depends only on the plugin SDK.

#![forbid(unsafe_code)]

mod geometry;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{Direction as Lie, Edge, MenuItem, MenuPlace, PaneId, Side, TabInfo};
use lokyr_plugin_sdk::message::{Data, Event};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::ViewId;
use lokyr_plugin_sdk::view::{Align, Edges, Justify, Layout, Node, Style, Token};

use crate::geometry::{Towards, rects};

lokyr_plugin_sdk::export_plugin!(Splits);

/// Splits the focused pane, `right`, `left`, `down` or `up`, the new pane
/// taking the keyboard on that side.
const SPLIT: &str = "splits.split";
/// Moves the keyboard to the pane beside the focused one, `left`, `right`,
/// `up` or `down`.
const FOCUS: &str = "splits.focus";
/// Moves the keyboard to the next pane of the tab, or the previous, round.
const FOCUS_NEXT: &str = "splits.focus-next";
const FOCUS_PREV: &str = "splits.focus-prev";
/// Moves a divider of the focused pane `cells` cells towards `direction`.
const RESIZE: &str = "splits.resize";
/// Zooms the focused pane, or ends the tab's zoom.
const ZOOM: &str = "splits.zoom-toggle";
/// Shares every split of the tab by its panes again.
const EQUALIZE: &str = "splits.equalize";
/// Swaps the focused pane with the one beside it towards `direction`.
const SWAP: &str = "splits.swap";
/// Moves the focused pane to a tab of its own, after its tab.
const MOVE_TO_TAB: &str = "splits.move-to-tab";
/// Moves the focused pane to a window of its own.
const MOVE_TO_WINDOW: &str = "splits.move-to-window";
/// Closes the focused pane of a tab of more than one, asking first while a
/// program other than the shell runs in it.
const CLOSE: &str = "splits.close";

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/splits.ftl")),
    ("it", include_str!("../locales/it/splits.ftl")),
];

/// A command: its name, its English title, and its parameters, each of its
/// name, its kind and its default.
type Spec = (
    &'static str,
    &'static str,
    &'static [(&'static str, Kind, Option<i64>)],
);

/// The commands, in the order of the menus.
const COMMANDS: [Spec; 11] = [
    (SPLIT, "Split Pane", &[("direction", Kind::String, None)]),
    (FOCUS, "Select Pane", &[("direction", Kind::String, None)]),
    (FOCUS_NEXT, "Select Next Pane", &[]),
    (FOCUS_PREV, "Select Previous Pane", &[]),
    (
        RESIZE,
        "Resize Pane",
        &[
            ("direction", Kind::String, None),
            ("cells", Kind::Integer, Some(1)),
        ],
    ),
    (ZOOM, "Zoom Pane", &[]),
    (EQUALIZE, "Equalize Panes", &[]),
    (SWAP, "Swap Pane", &[("direction", Kind::String, None)]),
    (MOVE_TO_TAB, "Move Pane to New Tab", &[]),
    (MOVE_TO_WINDOW, "Move Pane to New Window", &[]),
    (CLOSE, "Close Pane", &[]),
];

/// The menus' items: each one's menu, its command, its direction, if it
/// takes one, and the message of its title, if not the command's.
const ITEMS: [(MenuPlace, &str, Option<&str>, Option<&str>); 15] = [
    (BAR, SPLIT, Some("right"), Some("menu-split-right")),
    (BAR, SPLIT, Some("left"), Some("menu-split-left")),
    (BAR, SPLIT, Some("down"), Some("menu-split-down")),
    (BAR, SPLIT, Some("up"), Some("menu-split-up")),
    (BAR, FOCUS, Some("left"), Some("menu-select-left")),
    (BAR, FOCUS, Some("right"), Some("menu-select-right")),
    (BAR, FOCUS, Some("up"), Some("menu-select-up")),
    (BAR, FOCUS, Some("down"), Some("menu-select-down")),
    (CONTEXT, SPLIT, Some("right"), Some("menu-split-right")),
    (CONTEXT, SPLIT, Some("left"), Some("menu-split-left")),
    (CONTEXT, SPLIT, Some("down"), Some("menu-split-down")),
    (CONTEXT, SPLIT, Some("up"), Some("menu-split-up")),
    (CONTEXT, MOVE_TO_TAB, None, None),
    (CONTEXT, MOVE_TO_WINDOW, None, None),
    (CONTEXT, CLOSE, None, None),
];

/// The panes' menu of the menu bar, and the pane's context menu.
const BAR: MenuPlace = MenuPlace::Bar(Category::Panes);
const CONTEXT: MenuPlace = MenuPlace::PaneContext;

/// The plugin of the splits: its config, and the panes' headers that it
/// rendered, each of its last tree, which it renders again as their trees
/// change; the layouts are the host's.
#[derive(Debug, Default)]
pub struct Splits {
    settings: Settings,
    headers: Vec<(PaneId, Option<Node>)>,
}

/// Its config, as `[plugins.splits]` gives it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Settings {
    /// How much the panes that have not the keyboard fade, from 0 to 1.
    dim: f32,
    /// Whether each pane of a tab of more than one shows its title.
    titles: bool,
    /// Whether the keyboard follows the pointer.
    follows: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dim: 0.15,
            titles: false,
            follows: false,
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
        settings.dim = match config.get("dim-inactive") {
            Some(Data::Number(dim)) => *dim as f32,
            Some(Data::Integer(dim)) => *dim as f32,
            _ => settings.dim,
        }
        .clamp(0.0, 1.0);
        let flag = |name: &str| matches!(config.get(name), Some(Data::Boolean(true)));
        settings.titles = flag("pane-titles");
        settings.follows = flag("focus-follows-mouse");
        settings
    }
}

impl Plugin for Splits {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.configure(cx)?;
        cx.subscribe(&WorkspaceEvent::NAMES)?;
        for (name, title, params) in COMMANDS {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category: Category::Panes,
                capabilities: Vec::new(),
                params: params
                    .iter()
                    .map(|&(name, kind, default)| Param {
                        name: name.to_owned(),
                        kind,
                        default: default.map(Value::Integer),
                    })
                    .collect(),
                when: None,
            })?;
        }
        for (place, command, direction, title) in ITEMS {
            cx.add_menu_item(MenuItem {
                place,
                command: command.to_owned(),
                args: direction.map(towards).unwrap_or_default(),
                title: title.map(str::to_owned),
                ticked: false,
            })?;
        }
        Ok(())
    }

    /// Whether a command would change the focused tab: a pane beside to
    /// select or swap with, a tab of more than one pane to zoom, equalize,
    /// close or move a pane out of, and a window of more than one pane to
    /// move one out of; a split, and a resize, always.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        let Ok(Some((tab, pane))) = focused(cx) else {
            return false;
        };
        let several = tab.panes.len() > 1;
        match name {
            FOCUS | SWAP => direction(args)
                .and_then(|towards| beside(cx, &tab, pane, towards))
                .is_ok_and(|beside| beside.is_some()),
            FOCUS_NEXT | FOCUS_PREV => several && tab.zoomed.is_none(),
            ZOOM => several || tab.zoomed.is_some(),
            EQUALIZE | MOVE_TO_TAB | CLOSE => several,
            MOVE_TO_WINDOW => several || cx.tabs(tab.window).is_ok_and(|tabs| tabs.len() > 1),
            _ => true,
        }
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("../splits.schema.json").into())
    }

    /// A pane's header: its title, while the config shows them.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::PaneHeader(pane) = view else {
            return None;
        };
        let node = self.header(cx, *pane);
        match self.headers.iter_mut().find(|(known, _)| known == pane) {
            Some((_, last)) => last.clone_from(&node),
            None => self.headers.push((*pane, node.clone())),
        }
        node
    }

    /// A change of its config takes it; a change of the workspace renders
    /// again the headers whose trees it changed.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let mut again = false;
        for event in events {
            match event {
                Event::ConfigChanged => {
                    self.configure(cx)?;
                    again = true;
                }
                Event::Workspace(WorkspaceEvent::PaneClosed { pane, .. }) => {
                    self.headers.retain(|(known, _)| known != pane);
                    again = true;
                }
                Event::Workspace(_) => again = true,
                _ => {}
            }
        }
        if again {
            self.render_again(cx);
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        let Some((tab, pane)) = focused(cx)? else {
            return Ok(());
        };
        match name {
            SPLIT => {
                let (lie, side) = match direction(&args)? {
                    Towards::Right => (Lie::Horizontal, Side::Second),
                    Towards::Left => (Lie::Horizontal, Side::First),
                    Towards::Down => (Lie::Vertical, Side::Second),
                    Towards::Up => (Lie::Vertical, Side::First),
                };
                let new = cx.split(pane, lie, side)?;
                cx.focus_pane(new)
            }
            FOCUS => match beside(cx, &tab, pane, direction(&args)?)? {
                Some(next) => cx.focus_pane(next),
                None => Ok(()),
            },
            // Zoomed, the keyboard stays with the pane that shows, as it
            // does by direction, rather than go to one that the zoom hides.
            FOCUS_NEXT | FOCUS_PREV if tab.zoomed.is_some() => Ok(()),
            FOCUS_NEXT | FOCUS_PREV => {
                let panes = &tab.panes;
                let Some(at) = panes.iter().position(|&shown| shown == pane) else {
                    return Ok(());
                };
                let count = panes.len();
                let next = if name == FOCUS_NEXT {
                    (at + 1) % count
                } else {
                    (at + count - 1) % count
                };
                if panes[next] == pane {
                    return Ok(());
                }
                cx.focus_pane(panes[next])
            }
            RESIZE => resize(cx, &tab, pane, &args),
            ZOOM => {
                let zoomed = if tab.zoomed.is_some() {
                    None
                } else {
                    Some(pane)
                };
                if zoomed.is_none() && tab.zoomed.is_none() {
                    return Ok(());
                }
                // A tab of one pane has nothing to zoom.
                if zoomed.is_some() && tab.panes.len() < 2 {
                    return Ok(());
                }
                cx.zoom(tab.id, zoomed)
            }
            EQUALIZE => cx.equalize(tab.id),
            SWAP => match beside(cx, &tab, pane, direction(&args)?)? {
                Some(other) => cx.swap_panes(pane, other),
                None => Ok(()),
            },
            MOVE_TO_TAB => {
                // A pane alone in its tab is a tab of its own already.
                if tab.panes.len() < 2 {
                    return Ok(());
                }
                let index = cx
                    .tabs(tab.window)?
                    .iter()
                    .position(|shown| shown.id == tab.id)
                    .map_or(usize::MAX, |at| at + 1);
                cx.pane_to_new_tab(pane, tab.window, index)
            }
            MOVE_TO_WINDOW => {
                let alone = tab.panes.len() < 2 && cx.tabs(tab.window)?.len() < 2;
                if alone {
                    return Ok(());
                }
                cx.pane_to_new_window(pane)
            }
            // A pane alone closes with its tab.
            CLOSE if tab.panes.len() < 2 => Ok(()),
            CLOSE => cx.close_pane(pane, true),
            _ => Err(Error::new(
                Code::NotFound,
                format!("splits has no command `{name}`"),
            )),
        }
    }
}

impl Splits {
    /// Takes its section of the config, and gives the host the panes' fade
    /// and the keyboard's following the pointer.
    fn configure(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.settings = Settings::of(&cx.config()?);
        cx.set_inactive_dim(self.settings.dim)?;
        cx.set_focus_follows_mouse(self.settings.follows)
    }

    /// The header of `pane`, while the config shows the titles and its tab
    /// shows more than one pane: its title, in its middle, of the text's
    /// color in the pane that has the keyboard, of the secondary text's in
    /// the others, over the bar's background.
    fn header(&self, cx: &mut Cx<'_>, pane: PaneId) -> Option<Node> {
        if !self.settings.titles {
            return None;
        }
        let tab = tab_of(cx, pane)?;
        if tab.panes.len() < 2 || tab.zoomed.is_some() {
            return None;
        }
        let title = cx.pane_title(pane).ok().flatten().unwrap_or_default();
        Some(title_header(&title, tab.focused == pane))
    }

    /// Renders again the headers whose trees changed, so that the host
    /// draws no frame of the same; those of panes gone go.
    fn render_again(&mut self, cx: &mut Cx<'_>) {
        let headers = std::mem::take(&mut self.headers);
        for (pane, last) in headers {
            let view = ViewId::PaneHeader(pane);
            if self.header(cx, pane) != last && cx.request_render(view).is_err() {
                continue;
            }
            self.headers.push((pane, last));
        }
    }
}

/// A pane's header of `title`, of the pane that has the keyboard or not.
#[doc(hidden)]
pub fn title_header(title: &str, focused: bool) -> Node {
    let color = if focused { "text" } else { "text.secondary" };
    Node::container(vec![Node::text(title).style(Style {
        color: Some(Token::from(color)),
        ..Style::default()
    })])
    .layout(Layout {
        align: Align::Center,
        justify: Justify::Center,
        padding: Edges {
            top: 3.0,
            right: 8.0,
            bottom: 3.0,
            left: 8.0,
        },
        grow: 1.0,
        ..Layout::default()
    })
    .style(Style {
        background: Some(Token::from("tabs.bar.bg")),
        clip: true,
        ..Style::default()
    })
}

/// The tab that holds `pane`, of the windows' tabs.
fn tab_of(cx: &mut Cx<'_>, pane: PaneId) -> Option<TabInfo> {
    let windows = cx.windows().ok()?;
    windows.into_iter().find_map(|window| {
        cx.tabs(window.id)
            .ok()?
            .into_iter()
            .find(|tab| tab.panes.contains(&pane))
    })
}

/// The focused window's shown tab and its pane with the keyboard, if a
/// window has the focus.
fn focused(cx: &mut Cx<'_>) -> Result<Option<(TabInfo, PaneId)>, Error> {
    let Some(window) = cx.focused_window() else {
        return Ok(None);
    };
    let tab = cx.tabs(window)?.into_iter().find(|tab| tab.active);
    Ok(tab.map(|tab| {
        let pane = tab.focused;
        (tab, pane)
    }))
}

/// The direction of `args`, as its `direction` names it.
fn direction(args: &Args) -> Result<Towards, Error> {
    match args.get("direction") {
        Some(Value::String(name)) => Towards::of(name).ok_or_else(|| {
            wrong(format!(
                "`{name}` is not a direction: left, right, up or down"
            ))
        }),
        _ => Err(wrong(
            "a direction is needed: left, right, up or down".to_owned(),
        )),
    }
}

/// The arguments of the direction `name`.
fn towards(name: &str) -> Args {
    Args::from([("direction".to_owned(), Value::String(name.to_owned()))])
}

fn wrong(message: String) -> Error {
    Error::new(Code::Failed, message)
}

/// The pane of `tab` beside `pane` towards `towards`, of the layout as it
/// lies: the nearest whose side faces the pane's, of the most room shared
/// along it; none at the tab's edge, or while a pane is zoomed.
fn beside(
    cx: &mut Cx<'_>,
    tab: &TabInfo,
    pane: PaneId,
    towards: Towards,
) -> Result<Option<PaneId>, Error> {
    if tab.zoomed.is_some() {
        return Ok(None);
    }
    let rooms = rects(&cx.layout(tab.id)?);
    Ok(geometry::beside(&rooms, pane, towards))
}

/// Moves the divider of `pane` towards `args`' direction by its `cells`
/// cells: the one on that side of the pane, or the other where the pane is
/// at that edge of its tab, as iTerm2 moves it, of the share of its split's
/// room that the cells take, as the pane's grid gives a cell.
fn resize(cx: &mut Cx<'_>, tab: &TabInfo, pane: PaneId, args: &Args) -> Result<(), Error> {
    let towards = direction(args)?;
    let cells = match args.get("cells") {
        Some(Value::Integer(cells)) => *cells,
        _ => 1,
    };
    if cells <= 0 || tab.zoomed.is_some() {
        return Ok(());
    }
    let layout = cx.layout(tab.id)?;
    let rooms = rects(&layout);
    let Some(room) = rooms
        .iter()
        .find(|(id, _)| *id == pane)
        .map(|&(_, room)| room)
    else {
        return Ok(());
    };
    // The divider on the pane's right, or its bottom, where it has one, as
    // iTerm2 moves it; on its left, or its top, otherwise.
    let across = matches!(towards, Towards::Left | Towards::Right);
    let (near, far) = if across {
        (Edge::Left, Edge::Right)
    } else {
        (Edge::Top, Edge::Bottom)
    };
    let Some((edge, split)) = [far, near]
        .into_iter()
        .find_map(|edge| Some((edge, geometry::split_of(&layout, pane, edge)?)))
    else {
        return Ok(());
    };
    // The split's room in cells, of the pane's grid along it.
    let (columns, rows) = cx.pane_grid(pane)?;
    let (pane_cells, pane_share) = if across {
        (f64::from(columns), room.width)
    } else {
        (f64::from(rows), room.height)
    };
    let split_share = if across { split.width } else { split.height };
    if pane_cells <= 0.0 || pane_share <= 0.0 {
        return Ok(());
    }
    let split_cells = pane_cells * split_share / pane_share;
    let by = (cells as f64 / split_cells).min(1.0);
    let by = match towards {
        Towards::Right | Towards::Down => by,
        Towards::Left | Towards::Up => -by,
    };
    cx.resize_pane(pane, edge, by)
}

#[cfg(test)]
mod tests;

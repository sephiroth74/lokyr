//! Built-in plugin of the splits.
//!
//! Its commands split the focused pane, right or down; move the keyboard to
//! the pane beside it in a direction, or to the next or the previous of its
//! tab's; move a divider by cells; zoom the pane, or end the zoom; share
//! every split by its panes again; swap the pane with the one beside it;
//! and move it out to a tab or a window of its own. The system's keymap
//! binds their keys. The tabs' layouts are the host's, which the commands
//! change through the HostApi. Like every built-in plugin, it depends only
//! on the plugin SDK.

#![forbid(unsafe_code)]

mod geometry;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::host::{Direction as Lie, Edge, PaneId, Side, TabInfo};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

use crate::geometry::{Towards, rects};

lokyr_plugin_sdk::export_plugin!(Splits);

/// Splits the focused pane, `right` or `down`, the new pane taking the
/// keyboard.
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
const COMMANDS: [Spec; 10] = [
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
];

/// The plugin of the splits, of no state of its own: the layouts are the
/// host's.
#[derive(Debug, Default)]
pub struct Splits;

impl Plugin for Splits {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
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
        Ok(())
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        let Some((tab, pane)) = focused(cx)? else {
            return Ok(());
        };
        match name {
            SPLIT => {
                let lie = match direction(&args)? {
                    Towards::Right => Lie::Horizontal,
                    Towards::Down => Lie::Vertical,
                    other => {
                        return Err(wrong(format!(
                            "a pane splits right or down, not {}",
                            other.name()
                        )));
                    }
                };
                let new = cx.split(pane, lie, Side::Second)?;
                cx.focus_pane(new)
            }
            FOCUS => match beside(cx, &tab, pane, direction(&args)?)? {
                Some(next) => cx.focus_pane(next),
                None => Ok(()),
            },
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
            _ => Err(Error::new(
                Code::NotFound,
                format!("splits has no command `{name}`"),
            )),
        }
    }
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

//! Built-in plugin that opens links with a click of the mouse.
//!
//! Its command, which the system's keymap binds to ⌘-click, opens the OSC 8
//! link at the cell that the click was on; else the URL, or the path, that the cell's line holds
//! around it, a relative path from the working directory of the program in
//! the pane's foreground. A file opens in its application, a directory in
//! the file manager. Its other commands of a cell, which the pane's context
//! menu lists, copy what would open, and open a window or a tab in the
//! folder of the path there. Like every built-in plugin, it depends only on
//! the plugin SDK.

#![forbid(unsafe_code)]

mod scan;

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind, Param, Value,
};
use lokyr_plugin_sdk::host::{DecorationLook, PaneId, Underline};
use lokyr_plugin_sdk::message::{Event, POINTER};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

pub use scan::{Found, find};

lokyr_plugin_sdk::export_plugin!(Links);

/// Opens the link at the cell of its `column` and `row`, the row's number,
/// in the focused pane.
const OPEN: &str = "links.open";
/// Copies what [`OPEN`] would open at the cell: the link, the URL, or the
/// path from its folder.
const COPY: &str = "links.copy";
/// Opens a window whose shell starts in the folder of the path at the
/// cell, or in the folder that holds its file.
const NEW_WINDOW: &str = "links.new-window";
/// Opens a tab after the others, as [`NEW_WINDOW`] does a window.
const NEW_TAB: &str = "links.new-tab";

/// The rows before the clicked one and after it that the read of its line
/// takes: a URL or a path of thousands of characters, and no more of a line
/// of many rows.
const REACH: usize = 32;

/// The plugin's catalogs, of its command's title.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/links.ftl")),
    ("it", include_str!("../locales/it/links.ftl")),
];

/// How a link under the pointer looks while Command is held: underlined,
/// in its text's color, with the pointing hand over it.
const UNDERLINED: DecorationLook = DecorationLook {
    underline: Some(Underline::Single),
    underline_color: None,
    background: None,
    pointing: true,
};

/// The most cells around the pointer's that an OSC 8 link's underline
/// takes, so that a link of many cells costs no more.
const MOST_LINK_CELLS: usize = 512;

/// The cells of a link's text, its first and its last, each by its column
/// and its row's number.
type Span = ((u16, i64), (u16, i64));

/// The plugin of the links, and the link that it underlines, while
/// Command is held over it: its pane, its cells and its decoration.
#[derive(Debug, Default)]
pub struct Links {
    underlined: Option<(PaneId, Span, u64)>,
}

impl Plugin for Links {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let param = |name: &str| Param {
            name: name.to_owned(),
            kind: Kind::Integer,
            default: None,
        };
        let reads = [Capability::TerminalRead, Capability::ProcessRead];
        let commands = [
            (OPEN, "Open Link", Category::View, Some(Capability::Open)),
            (
                COPY,
                "Copy Link Address",
                Category::Clipboard,
                Some(Capability::ClipboardWrite),
            ),
            (NEW_WINDOW, "New Window at Folder", Category::Windows, None),
            (NEW_TAB, "New Tab at Folder", Category::Tabs, None),
        ];
        for (name, title, category, capability) in commands {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category,
                capabilities: reads.iter().cloned().chain(capability).collect(),
                params: vec![param("column"), param("row")],
                when: None,
            })?;
        }
        cx.subscribe(&[POINTER])
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    /// The pointer over a link that opens, with Command held, underlines
    /// it, as long as both stay; the underline goes as either does.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let Some((pane, at, mods)) = events.iter().rev().find_map(|event| match event {
            Event::Pointer { pane, at, mods } => Some((*pane, *at, *mods)),
            _ => None,
        }) else {
            return Ok(());
        };
        let wanted = match at {
            Some(at) if mods.command => hovered(cx, pane, at)?.map(|span| (pane, span)),
            _ => None,
        };
        if self.underlined.map(|(pane, span, _)| (pane, span)) == wanted {
            return Ok(());
        }
        if let Some((pane, _, id)) = self.underlined.take() {
            // A pane that closed took its decorations.
            let _ = cx.undecorate(pane, id);
        }
        if let Some((pane, (from, to))) = wanted {
            let id = cx.decorate(pane, from, to, UNDERLINED)?;
            self.underlined = Some((pane, (from, to), id));
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        match name {
            OPEN => match target(cx, &args)? {
                Some(target) => cx.open(&target),
                None => Ok(()),
            },
            COPY => match opening(cx, &args)? {
                Some(target) => cx.set_clipboard_text(&target),
                None => Ok(()),
            },
            NEW_WINDOW => match folder(cx, &args)? {
                Some(folder) => cx.open_window_in(&folder),
                None => Ok(()),
            },
            NEW_TAB => {
                let (Some(folder), Some(window)) = (folder(cx, &args)?, cx.focused_window()) else {
                    return Ok(());
                };
                let (tab, _) = cx.new_tab_in(window, None, &folder)?;
                cx.activate_tab(tab)
            }
            _ => Err(Error::new(
                Code::NotFound,
                format!("links has no command `{name}`"),
            )),
        }
    }

    /// Whether the cell holds a link, a URL or a path that is there, which
    /// opening and its copy take; and a path there, of a folder or of a
    /// file, which a window or a tab opens in.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        match name {
            OPEN | COPY => opening(cx, args).ok().flatten().is_some(),
            NEW_WINDOW | NEW_TAB => folder(cx, args).ok().flatten().is_some(),
            _ => false,
        }
    }
}

/// What a click on the cell of `args`' `column` and `row` opens, in the
/// focused pane: the cell's link, or the URL or the path around it, with
/// the working directory before a relative one; of the text between quotes
/// and the word around it, the first that is there, else the word's;
/// `None` for nothing.
fn target(cx: &mut Cx<'_>, args: &Args) -> Result<Option<String>, Error> {
    // The registry checked that both are there, whole numbers; a row that
    // the pane no longer holds finds nothing.
    let number = |name| match args.get(name) {
        Some(&Value::Integer(value)) => Some(value),
        _ => None,
    };
    let column = number("column").and_then(|column| u16::try_from(column).ok());
    let (Some(column), Some(row)) = (column, number("row")) else {
        return Ok(None);
    };
    let Some(pane) = cx.focused_pane() else {
        return Ok(None);
    };
    let mut candidates = candidates(cx, pane, (column, row))?;
    // An OSC 8 link, alone; else the word's, the last, which a click tries
    // even when it is not there.
    let last = candidates.pop();
    for (target, _) in candidates {
        if cx.opens(&target).unwrap_or(false) {
            return Ok(Some(target));
        }
    }
    Ok(last.map(|(target, _)| target))
}

/// What the cell of `args` holds that opens, as [`target`] finds it, if
/// it opens.
fn opening(cx: &mut Cx<'_>, args: &Args) -> Result<Option<String>, Error> {
    let target = target(cx, args)?;
    Ok(target.filter(|target| cx.opens(target).unwrap_or(false)))
}

/// The path at the cell of `args` that is there, which a window or a tab
/// opens in: a path, or an OSC 8 link of a local file's URL; `None` for a
/// URL of another place, or nothing there.
fn folder(cx: &mut Cx<'_>, args: &Args) -> Result<Option<String>, Error> {
    Ok(opening(cx, args)?.and_then(|target| local_path(&target)))
}

/// The path that `target` names on this machine: itself, absolute or from
/// `~`, or a `file:` URL's of no host or `localhost`, its escapes decoded;
/// `None` for another URL.
fn local_path(target: &str) -> Option<String> {
    if target.starts_with('/') || target == "~" || target.starts_with("~/") {
        return Some(target.to_owned());
    }
    let rest = target
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("file://"))
        .map(|_| &target[7..])?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    path.starts_with('/').then(|| decoded(path))?
}

/// `text` with its `%` escapes decoded, as a URL's path writes its
/// bytes; `None` where they make no UTF-8.
fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let escaped = (bytes[at] == b'%')
            .then(|| text.get(at + 1..at + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                at += 3;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// The link that the cell `at` of `pane` holds and that opens, which
/// Command underlines: the cells of its text; `None` for none.
fn hovered(cx: &mut Cx<'_>, pane: PaneId, at: (u16, i64)) -> Result<Option<Span>, Error> {
    for (target, span) in candidates(cx, pane, at)? {
        if cx.opens(&target)? {
            return Ok(span);
        }
    }
    Ok(None)
}

/// What the cell `at` of `pane` holds that a click could open, each with
/// the cells of its text, where the line holds its text: the cell's OSC 8
/// link alone; else the URLs and the paths around it, of the text between
/// quotes and of the word, the word's last, with the working directory
/// before a relative one.
fn candidates(
    cx: &mut Cx<'_>,
    pane: PaneId,
    at: (u16, i64),
) -> Result<Vec<(String, Option<Span>)>, Error> {
    let (column, row) = at;
    let Some(line) = cx.line_text(pane, row, REACH)? else {
        return Ok(Vec::new());
    };
    // The character whose cell holds the column: the last one that starts
    // at it or before it, as a wide one covers two.
    let Some(index) = line
        .cells
        .iter()
        .rposition(|&(x, y)| y == row && x <= column)
    else {
        return Ok(Vec::new());
    };
    if let Some(link) = cx.link_at(pane, column, row)? {
        let span = link_span(cx, pane, &line.cells, index, &link)?;
        return Ok(vec![(link, Some(span))]);
    }
    let found = find(&line.text, index);
    let relative = |path: &str| !(path.starts_with('/') || path == "~" || path.starts_with("~/"));
    let directory = if found
        .iter()
        .any(|found| matches!(found, Found::Path(path) if relative(path)))
    {
        cx.working_directory(pane)?
    } else {
        None
    };
    Ok(found
        .into_iter()
        .filter_map(|found| {
            let (text, target) = match found {
                Found::Url(url) => (url.clone(), url),
                Found::Path(path) if relative(&path) => {
                    let directory = directory.as_ref()?;
                    let target = normal(&format!("{directory}/{path}"));
                    (path, target)
                }
                Found::Path(path) => {
                    let target = normal(&path);
                    (path, target)
                }
            };
            // A path of escaped spaces, as a shell's completion writes it,
            // is found without its backslashes, which the line holds.
            let span = span_of(&line.text, &text, index)
                .or_else(|| span_of(&line.text, &text.replace(' ', "\\ "), index))
                .map(|(first, end)| cells_of(&line.cells, first, end));
            Some((target, span))
        })
        .collect())
}

/// The cells of the OSC 8 link `link`, around the character `index` of a
/// line whose characters' cells are `cells`: those of the same link next
/// to it, [`MOST_LINK_CELLS`] at most.
fn link_span(
    cx: &mut Cx<'_>,
    pane: PaneId,
    cells: &[(u16, i64)],
    index: usize,
    link: &str,
) -> Result<Span, Error> {
    let same = |cx: &mut Cx<'_>, at: usize| -> Result<bool, Error> {
        let (column, row) = cells[at];
        Ok(cx.link_at(pane, column, row)?.as_deref() == Some(link))
    };
    let lowest = index.saturating_sub(MOST_LINK_CELLS / 2);
    let mut first = index;
    while first > lowest && same(cx, first - 1)? {
        first -= 1;
    }
    let highest = (index + MOST_LINK_CELLS / 2).min(cells.len() - 1);
    let mut last = index;
    while last < highest && same(cx, last + 1)? {
        last += 1;
    }
    Ok(cells_of(cells, first, last + 1))
}

/// The characters, from the first to the one before the end, that `found`
/// takes of `text` around its character `at`: the occurrence that holds
/// it, or else the last that starts before it, as a path's line and column
/// after it are not part of it.
fn span_of(text: &str, found: &str, at: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let needle: Vec<char> = found.chars().collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return None;
    }
    let mut span = None;
    for first in 0..=chars.len() - needle.len() {
        if first > at {
            break;
        }
        if chars[first..first + needle.len()] == needle[..] {
            span = Some((first, first + needle.len()));
            if first + needle.len() > at {
                break;
            }
        }
    }
    span
}

/// The cells of the characters from `first` to the one before `end`, of a
/// line whose characters' cells are `cells`: the first's, and the last's,
/// with the second half of a wide one.
fn cells_of(cells: &[(u16, i64)], first: usize, end: usize) -> Span {
    let (column, row) = cells[end - 1];
    let last = match cells.get(end) {
        // The next character on the same row, past a wide one's second
        // half.
        Some(&(next, next_row)) if next_row == row && next > column + 1 => (next - 1, row),
        _ => (column, row),
    };
    (cells[first], last)
}

/// `path` without its `.` and `..` parts, nor its empty ones: a `..` takes
/// the part before it away, but none of the root; above `~`, it stays.
fn normal(path: &str) -> String {
    let (root, rest) = if let Some(rest) = path.strip_prefix('/') {
        ("/", rest)
    } else if path == "~" {
        ("~", "")
    } else if let Some(rest) = path.strip_prefix("~/") {
        ("~/", rest)
    } else {
        ("", path)
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            // Above the home directory, which only the host knows, `..`
            // stays.
            ".." if root == "~/" && parts.last().is_none_or(|last| *last == "..") => {
                parts.push("..");
            }
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    format!("{root}{}", parts.join("/"))
}

#[cfg(test)]
mod tests;

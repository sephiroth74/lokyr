//! Built-in plugin that opens links with a click of the mouse.
//!
//! Its command, bound to ⌘-click, opens the OSC 8 link at the cell that the
//! click was on; else the URL, or the path, that the cell's line holds
//! around it, a relative path from the working directory of the program in
//! the pane's foreground. A file opens in its application, a directory in
//! the file manager. Like every built-in plugin, it depends only on the
//! plugin SDK.

#![forbid(unsafe_code)]

mod scan;

use lokyr_plugin_sdk::command::{
    Args, Capability, Category, Code, Command, Error, Kind, Param, Value,
};
use lokyr_plugin_sdk::host::Binding;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

pub use scan::{Found, find};

/// Opens the link at the cell of its `column` and `row`, the row's number,
/// in the focused pane.
const OPEN: &str = "links.open";

/// The rows before the clicked one and after it that the read of its line
/// takes: a URL or a path of thousands of characters, and no more of a line
/// of many rows.
const REACH: usize = 32;

/// The keys of [`OPEN`].
const KEYS: &str = "cmd+click";

/// The plugin's catalogs, of its command's title.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/links.ftl")),
    ("it", include_str!("../locales/it/links.ftl")),
];

/// The plugin of the links.
#[derive(Debug, Default)]
pub struct Links;

impl Plugin for Links {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let param = |name: &str| Param {
            name: name.to_owned(),
            kind: Kind::Integer,
            default: None,
        };
        cx.register_command(Command {
            name: OPEN.to_owned(),
            title: "Open Link".to_owned(),
            category: Category::View,
            capabilities: vec![
                Capability::TerminalRead,
                Capability::Open,
                Capability::ProcessRead,
            ],
            params: vec![param("column"), param("row")],
            when: None,
        })?;
        cx.bind(Binding {
            keys: KEYS.to_owned(),
            command: OPEN.to_owned(),
            args: Args::new(),
            when: None,
        })
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        if name != OPEN {
            return Err(Error::new(
                Code::NotFound,
                format!("links has no command `{name}`"),
            ));
        }
        match target(cx, &args)? {
            Some(target) => cx.open(&target),
            None => Ok(()),
        }
    }

    /// Whether the cell holds a link, a URL or a path that is there, which
    /// the command would open.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        name == OPEN
            && target(cx, args)
                .ok()
                .flatten()
                .is_some_and(|target| cx.opens(&target).unwrap_or(false))
    }
}

/// What a click on the cell of `args`' `column` and `row` opens: the cell's
/// link, or the URL or the path around it, with the working directory
/// before a relative one; of the text between quotes and the word around
/// it, the first that is there, else the word's; `None` for nothing.
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
    if let Some(link) = cx.link_at(pane, column, row)? {
        return Ok(Some(link));
    }
    let Some(line) = cx.line_text(pane, row, REACH)? else {
        return Ok(None);
    };
    // The character whose cell holds the column: the last one that starts
    // at it or before it, as a wide one covers two.
    let Some(at) = line
        .cells
        .iter()
        .rposition(|&(x, y)| y == row && x <= column)
    else {
        return Ok(None);
    };
    let found = find(&line.text, at);
    let relative = |path: &str| !(path.starts_with('/') || path == "~" || path.starts_with("~/"));
    let directory = if found
        .iter()
        .any(|found| matches!(found, Found::Path(path) if relative(path)))
    {
        cx.working_directory(pane)?
    } else {
        None
    };
    let mut targets: Vec<String> = found
        .into_iter()
        .filter_map(|found| match found {
            Found::Url(url) => Some(url),
            Found::Path(path) if relative(&path) => directory
                .as_ref()
                .map(|directory| normal(&format!("{directory}/{path}"))),
            Found::Path(path) => Some(normal(&path)),
        })
        .collect();
    // The word's, the last, which a click tries even when it is not there.
    let last = targets.pop();
    for target in targets {
        if cx.opens(&target).unwrap_or(false) {
            return Ok(Some(target));
        }
    }
    Ok(last)
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

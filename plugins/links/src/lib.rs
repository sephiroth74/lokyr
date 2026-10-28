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

/// Opens the link at the cell of its `column` and `row`, in the focused
/// pane's view.
const OPEN: &str = "links.open";

/// The keys of [`OPEN`].
const KEYS: &str = "cmd+click";

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
        })?;
        cx.bind(Binding {
            keys: KEYS.to_owned(),
            command: OPEN.to_owned(),
            args: Args::new(),
            when: None,
        })
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        if name != OPEN {
            return Err(Error::new(
                Code::NotFound,
                format!("links has no command `{name}`"),
            ));
        }
        // The registry checked that both are there, whole numbers; one past
        // the view's cells finds nothing.
        let cell = |name| match args.get(name) {
            Some(&Value::Integer(value)) => u16::try_from(value).ok(),
            _ => None,
        };
        let (Some(column), Some(row)) = (cell("column"), cell("row")) else {
            return Ok(());
        };
        let Some(pane) = cx.focused_pane() else {
            return Ok(());
        };
        if let Some(link) = cx.link_at(pane, column, row)? {
            return cx.open(&link);
        }
        let Some(line) = cx.line_text(pane, row)? else {
            return Ok(());
        };
        // The character whose cell holds the column: the last one that
        // starts at it or before it, as a wide one covers two.
        let Some(at) = line
            .cells
            .iter()
            .rposition(|&(x, y)| y == i64::from(row) && x <= column)
        else {
            return Ok(());
        };
        match find(&line.text, at) {
            Some(Found::Url(url)) => cx.open(&url),
            Some(Found::Path(path))
                if path.starts_with('/') || path == "~" || path.starts_with("~/") =>
            {
                cx.open(&normal(&path))
            }
            Some(Found::Path(path)) => match cx.working_directory(pane)? {
                Some(directory) => cx.open(&normal(&format!("{directory}/{path}"))),
                None => Ok(()),
            },
            None => Ok(()),
        }
    }
}

/// `path` without its `.` and `..` parts, nor its empty ones: a `..` takes
/// the part before it away, but none of the root or of `~`.
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

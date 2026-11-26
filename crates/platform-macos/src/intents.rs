//! The App Intents' entry, of C's calling convention, which their
//! declarations of Swift call on the main thread as Shortcuts runs one: a
//! request of the automation's, of a kind and its arguments, whose answer
//! comes back through a function of the intents', once, as the app gives
//! it.

use std::ffi::{CStr, CString, c_char, c_void};

use lokyr_platform_core::automation::{
    Answer, Arg, Command, Edge, Mouse, MouseAction, MouseButton, Request, Target,
};

/// The app that asks, as the automation's question about a pane's text
/// names it.
const CLIENT: &str = "Shortcuts";

/// The kinds of the intents' requests.
const NEW_WINDOW: i32 = 1;
const NEW_TAB: i32 = 2;
const SPLIT: i32 = 3;
const SEND_TEXT: i32 = 4;
const RUN: i32 = 5;
const PANE_TEXT: i32 = 6;
const WORKSPACE: i32 = 7;
const SWITCH_SESSION: i32 = 8;
const CLOSE: i32 = 9;
const FOCUS: i32 = 10;
const COMMANDS: i32 = 11;
const SEND_KEY: i32 = 12;
const RUN_LINE: i32 = 13;
const SEND_MOUSE: i32 = 14;

/// The registry's code of a failure, of an answer that the app could not
/// take or give.
const FAILED: i32 = 201;

/// The intents' function that takes an answer: their context, the status,
/// 0 of an answer or the registry's code of an error, and the answer's
/// text or the error's message, which lives through the call.
pub type AnswerFn = extern "C" fn(context: *mut c_void, status: i32, text: *const c_char);

/// Makes the automation's request of `kind`, of the ids `first` and
/// `second`, none of 0, and the texts `text`, `more` and `variables`, none
/// of a null pointer, and gives its answer to `answer` of `context`, once:
/// at once of a request that the app cannot take, later otherwise. Whether
/// the app took it.
///
/// - `NEW_WINDOW`: `text` the folder, `more` the command line, `variables`
///   the environment's, a line each of `NAME=value`; the ids of the
///   window, its tab and its pane, apart by spaces.
/// - `NEW_TAB`: `first` the window, `text` the folder, `more` the command
///   line, `variables` alike; the ids alike.
/// - `SPLIT`: `first` the pane, `second` the edge (0 right, 1 left, 2 down,
///   3 up), `text` the folder, `more` the command line, `variables` alike;
///   the ids alike.
/// - `SEND_TEXT`: `first` the pane, `text` the text.
/// - `RUN`: `text` the command's name, or a palette's entry as `COMMANDS`
///   gives it, `more` its arguments, a line each of `name=value`, each
///   value a text, over the entry's.
/// - `PANE_TEXT`: `first` the pane, `second` 1 of its selection; the text.
/// - `WORKSPACE`: every pane, a line each of its entity's id (this start's
///   token, a dot and the pane's id), its title, its folder, its tab's id
///   and its window's, apart by tabs.
/// - `SWITCH_SESSION`: `text` the session's name, which it makes first if
///   there is none of that name.
/// - `CLOSE`: `first` the pane, which it closes.
/// - `FOCUS`: `first` the pane, which it brings forward.
/// - `SEND_KEY`: `first` the pane, `text` the key and its modifiers, as
///   a binding writes them, as `ctrl+c`.
/// - `RUN_LINE`: `first` the pane, `text` the command line, which its
///   shell runs at its prompt.
/// - `SEND_MOUSE`: `first` the pane, `text` the action (`press`,
///   `release`, `move`, `scroll-up`, `scroll-down`, `scroll-left`,
///   `scroll-right`), `more` its fields, a line each of `name=value`:
///   `button` (`left`, `middle`, `right`), `column` and `row` from 0,
///   `steps`, and `shift`, `control` and `alt` of `true`.
/// - `COMMANDS`: the command palette's entries, a line each of its id and
///   its title, apart by a tab: the command's name, then its entry's
///   arguments, if any, as a URL's query, `splits.split?direction=right`.
#[unsafe(no_mangle)]
pub extern "C" fn lokyr_intent_request(
    kind: i32,
    first: u64,
    second: u64,
    text: *const c_char,
    more: *const c_char,
    variables: *const c_char,
    context: *mut c_void,
    answer: AnswerFn,
) -> i32 {
    // SAFETY: the intents give C strings, terminated, that live through
    // the call, or null pointers.
    let (text, more, variables) = unsafe { (owned(text), owned(more), owned(variables)) };
    let environment = match variables_of(variables.as_deref().unwrap_or_default().lines()) {
        Ok(environment) => environment,
        Err(message) => {
            give(answer, context, FAILED, &message);
            return 0;
        }
    };
    let id = |id: u64| (id != 0).then_some(id);
    let request = match kind {
        NEW_WINDOW => Request::NewWindow {
            folder: text,
            command: more,
            environment,
        },
        NEW_TAB => Request::NewTab {
            window: id(first),
            folder: text,
            command: more,
            environment,
        },
        SPLIT => Request::Split {
            pane: id(first),
            edge: match second {
                1 => Edge::Left,
                2 => Edge::Down,
                3 => Edge::Up,
                _ => Edge::Right,
            },
            folder: text,
            command: more,
            environment,
        },
        SEND_TEXT => Request::SendText {
            pane: id(first),
            text: text.unwrap_or_default(),
        },
        RUN => {
            let text = text.unwrap_or_default();
            let (command, query) = text.split_once('?').unwrap_or((&text, ""));
            let given = more
                .as_deref()
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(name, value)| (name.trim().to_owned(), value.to_owned()));
            Request::Run {
                command: command.to_owned(),
                args: entry_args(query)
                    .chain(given)
                    .map(|(name, value)| (name, Arg::Text(value)))
                    .collect(),
            }
        }
        PANE_TEXT => Request::PaneText {
            pane: id(first),
            selection: second == 1,
        },
        WORKSPACE => Request::Workspace,
        SWITCH_SESSION => Request::SwitchSession {
            name: text.unwrap_or_default(),
        },
        CLOSE => Request::Close(Target::Pane(first)),
        FOCUS => Request::Focus(Target::Pane(first)),
        COMMANDS => Request::Commands,
        SEND_KEY => Request::SendKey {
            pane: id(first),
            keys: text.unwrap_or_default(),
        },
        RUN_LINE => Request::RunLine {
            pane: id(first),
            line: text.unwrap_or_default(),
        },
        SEND_MOUSE => match mouse_of(text.as_deref().unwrap_or_default(), more.as_deref()) {
            Ok(mouse) => Request::SendMouse {
                pane: id(first),
                mouse,
            },
            Err(message) => {
                give(answer, context, FAILED, &message);
                return 0;
            }
        },
        _ => {
            give(
                answer,
                context,
                FAILED,
                &format!("no intent's request of the kind {kind}"),
            );
            return 0;
        }
    };
    let context = Context(context);
    let posted = crate::app::post_automation(CLIENT, request, move |reply| {
        let (status, text) = answer_of(reply);
        give(answer, context.0, status, &text);
    });
    if !posted {
        give(
            answer,
            context.0,
            FAILED,
            "Lokyr cannot take the request now",
        );
    }
    i32::from(posted)
}

/// The environment variables of `entries`, each `NAME=value`, a blank one
/// left out; why not of an entry of no `=` or of no name.
pub(crate) fn variables_of<'a>(
    entries: impl Iterator<Item = &'a str>,
) -> Result<Vec<(String, String)>, String> {
    let mut variables = Vec::new();
    for entry in entries {
        if entry.trim().is_empty() {
            continue;
        }
        match entry.split_once('=') {
            Some((name, value)) if !name.trim().is_empty() => {
                variables.push((name.trim().to_owned(), value.to_owned()));
            }
            _ => {
                return Err(format!(
                    "`{entry}` is no environment variable: write it as NAME=value"
                ));
            }
        }
    }
    Ok(variables)
}

/// The mouse event of `action` and of the fields of `fields`, a line each
/// of `name=value`; why not of an action, a button or a number that does
/// not read.
fn mouse_of(action: &str, fields: Option<&str>) -> Result<Mouse, String> {
    let action = match action.trim() {
        "press" => MouseAction::Press,
        "release" => MouseAction::Release,
        "move" => MouseAction::Move,
        "scroll-up" => MouseAction::ScrollUp,
        "scroll-down" => MouseAction::ScrollDown,
        "scroll-left" => MouseAction::ScrollLeft,
        "scroll-right" => MouseAction::ScrollRight,
        other => return Err(format!("`{other}` is no mouse's action")),
    };
    let mut mouse = Mouse {
        action,
        button: None,
        column: 0,
        row: 0,
        shift: false,
        control: false,
        alt: false,
        steps: None,
    };
    let number = |name: &str, value: &str| {
        value
            .trim()
            .parse::<u16>()
            .map_err(|_| format!("the mouse's {name} `{value}` is no whole number"))
    };
    for (name, value) in fields
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('='))
    {
        match name.trim() {
            "button" => {
                mouse.button = Some(match value.trim() {
                    "left" => MouseButton::Left,
                    "middle" => MouseButton::Middle,
                    "right" => MouseButton::Right,
                    other => return Err(format!("`{other}` is no mouse's button")),
                });
            }
            "column" => mouse.column = number("column", value)?,
            "row" => mouse.row = number("row", value)?,
            "steps" => mouse.steps = Some(number("steps", value)?),
            "shift" => mouse.shift = value.trim() == "true",
            "control" => mouse.control = value.trim() == "true",
            "alt" => mouse.alt = value.trim() == "true",
            _ => {}
        }
    }
    Ok(mouse)
}

/// The intents' context, which goes back to them as it came.
struct Context(*mut c_void);

/// The text of `text`, a C string or a null pointer.
///
/// # Safety
///
/// `text` is a C string, terminated, that lives through the call, or a
/// null pointer.
unsafe fn owned(text: *const c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: as the function's contract says.
    Some(
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// Gives `answer` of `context` its status and text.
fn give(answer: AnswerFn, context: *mut c_void, status: i32, text: &str) {
    // A text holds no zero, which would end its C string.
    let text = CString::new(text.replace('\0', " ")).unwrap_or_default();
    answer(context, status, text.as_ptr());
}

/// This start's token, which a terminal's entity's id begins with: the
/// panes' ids start anew at each start, so that a shortcut that keeps a
/// terminal of another start finds none, rather than another pane of its
/// number.
fn start_token() -> &'static str {
    static TOKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TOKEN.get_or_init(|| {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        format!("{:x}{:x}", std::process::id(), nanos)
    })
}

/// The id of the palette's entry of `command`: its name, then its
/// arguments, if any, as a URL's query, each name and value encoded.
fn entry_id(command: &Command) -> String {
    let mut id = clean(&command.name);
    for (at, (name, arg)) in command.args.iter().enumerate() {
        let value = match arg {
            Arg::Text(text) => text.clone(),
            Arg::Integer(number) => number.to_string(),
            Arg::Bool(yes) => yes.to_string(),
        };
        id.push(if at == 0 { '?' } else { '&' });
        id.push_str(&encoded(name));
        id.push('=');
        id.push_str(&encoded(&value));
    }
    id
}

/// The arguments of an entry's id's `query`, each name and value decoded;
/// none of a pair that does not decode.
fn entry_args(query: &str) -> impl Iterator<Item = (String, String)> + '_ {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter_map(|(name, value)| Some((decoded(name)?, decoded(value)?)))
}

/// `text` of every byte but a letter, a digit, `-`, `.`, `_` and `~` as
/// `%` and its two hexadecimal digits.
fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `text` of its `%` and two hexadecimal digits as their bytes; none of a
/// `%` of no two digits, or of bytes of no UTF-8.
fn decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let digits = std::str::from_utf8(bytes.get(at + 1..at + 3)?).ok()?;
            out.push(u8::from_str_radix(digits, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `text` of no tab nor line break, which part an answer's fields and
/// lines.
fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// The status and the text of the automation's `answer`.
fn answer_of(answer: Answer) -> (i32, String) {
    match answer {
        Answer::Done => (0, String::new()),
        Answer::Opened { window, tab, pane } => (0, format!("{window} {tab} {pane}")),
        Answer::Text(text) => (0, text),
        Answer::Workspace(windows) => {
            let mut lines = Vec::new();
            for window in &windows {
                for tab in &window.tabs {
                    for pane in &tab.panes {
                        lines.push(format!(
                            "{}.{}\t{}\t{}\t{}\t{}",
                            start_token(),
                            pane.id,
                            clean(&pane.title),
                            clean(pane.folder.as_deref().unwrap_or_default()),
                            tab.id,
                            window.id
                        ));
                    }
                }
            }
            (0, lines.join("\n"))
        }
        Answer::Commands(commands) => {
            let lines: Vec<String> = commands
                .iter()
                .map(|command| format!("{}\t{}", entry_id(command), clean(&command.title)))
                .collect();
            (0, lines.join("\n"))
        }
        Answer::Error { code, message } => (i32::from(code), message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lokyr_platform_core::automation::{Pane, Tab, Window};

    #[test]
    fn an_answer_goes_to_the_intents_as_its_status_and_its_text() {
        assert_eq!(
            answer_of(Answer::Opened {
                window: 1,
                tab: 2,
                pane: 3
            }),
            (0, "1 2 3".to_owned())
        );
        assert_eq!(
            answer_of(Answer::Error {
                code: 101,
                message: "no pane".to_owned()
            }),
            (101, "no pane".to_owned())
        );
        let pane = |id: u64, title: &str| Pane {
            id,
            title: title.to_owned(),
            folder: Some("/tmp".to_owned()),
            focused: false,
        };
        let workspace = Answer::Workspace(vec![Window {
            id: 1,
            title: "w".to_owned(),
            focused: true,
            tabs: vec![Tab {
                id: 2,
                title: "t".to_owned(),
                active: true,
                panes: vec![pane(3, "zsh"), pane(4, "a\tb")],
            }],
        }]);
        let token = start_token();
        assert_eq!(
            answer_of(workspace),
            (
                0,
                format!("{token}.3\tzsh\t/tmp\t2\t1\n{token}.4\ta b\t/tmp\t2\t1")
            )
        );
        // The same start's token each time, of no dot, so that the pane's id
        // follows the last dot.
        assert_eq!(start_token(), token);
        assert!(!token.contains('.'));
        let commands = Answer::Commands(vec![
            Command {
                name: "tabs.new".to_owned(),
                title: "New\tTab".to_owned(),
                args: Vec::new(),
            },
            Command {
                name: "splits.split".to_owned(),
                title: "Split Right".to_owned(),
                args: vec![
                    ("direction".to_owned(), Arg::Text("right".to_owned())),
                    ("folder".to_owned(), Arg::Text("~/a b&c=d".to_owned())),
                ],
            },
        ]);
        assert_eq!(
            answer_of(commands),
            (
                0,
                "tabs.new\tNew Tab\nsplits.split?direction=right&folder=~%2Fa%20b%26c%3Dd\tSplit Right"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_mouse_event_is_its_action_and_the_lines_of_its_fields() {
        assert_eq!(
            mouse_of("press", Some("button=right\ncolumn=3\nrow=2\nshift=true")),
            Ok(Mouse {
                action: MouseAction::Press,
                button: Some(MouseButton::Right),
                column: 3,
                row: 2,
                shift: true,
                control: false,
                alt: false,
                steps: None,
            })
        );
        assert_eq!(
            mouse_of("scroll-down", Some("steps=5")).map(|mouse| mouse.steps),
            Ok(Some(5))
        );
        assert!(mouse_of("wiggle", None).is_err());
        assert!(mouse_of("press", Some("button=fourth")).is_err());
        assert!(mouse_of("move", Some("column=-1")).is_err());
    }

    #[test]
    fn the_variables_are_lines_of_a_name_and_its_value() {
        assert_eq!(
            variables_of("A=1\n\n B = x=y\nC=".lines()),
            Ok(vec![
                ("A".to_owned(), "1".to_owned()),
                ("B".to_owned(), " x=y".to_owned()),
                ("C".to_owned(), String::new()),
            ])
        );
        assert!(variables_of(["PATH"].into_iter()).is_err());
        assert!(variables_of(["=x"].into_iter()).is_err());
    }

    #[test]
    fn an_entry_s_arguments_come_back_of_its_id_under_the_ones_given() {
        let id = "splits.split?direction=right&folder=~%2Fa%20b%26c%3Dd";
        let (command, query) = id.split_once('?').unwrap();
        assert_eq!(command, "splits.split");
        assert_eq!(
            entry_args(query).collect::<Vec<_>>(),
            [
                ("direction".to_owned(), "right".to_owned()),
                ("folder".to_owned(), "~/a b&c=d".to_owned()),
            ]
        );
        assert_eq!(decoded("%zz"), None);
        assert_eq!(decoded("%e2%82%ac"), Some("€".to_owned()));
    }
}

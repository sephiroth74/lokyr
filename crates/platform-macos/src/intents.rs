//! The App Intents' entry, of C's calling convention, which their
//! declarations of Swift call on the main thread as Shortcuts runs one: a
//! request of the automation's, of a kind and its arguments, whose answer
//! comes back through a function of the intents', once, as the app gives
//! it.

use std::ffi::{CStr, CString, c_char, c_void};

use lokyr_platform_core::automation::{Answer, Edge, Request};

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

/// The registry's code of a failure, of an answer that the app could not
/// take or give.
const FAILED: i32 = 201;

/// The intents' function that takes an answer: their context, the status,
/// 0 of an answer or the registry's code of an error, and the answer's
/// text or the error's message, which lives through the call.
pub type AnswerFn = extern "C" fn(context: *mut c_void, status: i32, text: *const c_char);

/// Makes the automation's request of `kind`, of the ids `first` and
/// `second`, none of 0, and the texts `text` and `more`, none of a null
/// pointer, and gives its answer to `answer` of `context`, once: at once of
/// a request that the app cannot take, later otherwise. Whether the app
/// took it.
///
/// - `NEW_WINDOW`: `text` the folder, `more` the command line; the ids of
///   the window, its tab and its pane, apart by spaces.
/// - `NEW_TAB`: `first` the window, `text` the folder, `more` the command
///   line; the ids alike.
/// - `SPLIT`: `first` the pane, `second` the edge (0 right, 1 left, 2 down,
///   3 up), `text` the folder, `more` the command line; the ids alike.
/// - `SEND_TEXT`: `first` the pane, `text` the text.
/// - `RUN`: `text` the command's name, `more` its arguments, a line each
///   of `name=value`, each value a text.
/// - `PANE_TEXT`: `first` the pane, `second` 1 of its selection; the text.
/// - `WORKSPACE`: every pane, a line each of its id, its title and its
///   folder, apart by tabs.
/// - `SWITCH_SESSION`: `text` the session's name, which it makes first if
///   there is none of that name.
#[unsafe(no_mangle)]
pub extern "C" fn lokyr_intent_request(
    kind: i32,
    first: u64,
    second: u64,
    text: *const c_char,
    more: *const c_char,
    context: *mut c_void,
    answer: AnswerFn,
) -> i32 {
    // SAFETY: the intents give C strings, terminated, that live through
    // the call, or null pointers.
    let (text, more) = unsafe { (owned(text), owned(more)) };
    let id = |id: u64| (id != 0).then_some(id);
    let request = match kind {
        NEW_WINDOW => Request::NewWindow {
            folder: text,
            command: more,
        },
        NEW_TAB => Request::NewTab {
            window: id(first),
            folder: text,
            command: more,
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
        },
        SEND_TEXT => Request::SendText {
            pane: id(first),
            text: text.unwrap_or_default(),
        },
        RUN => Request::Run {
            command: text.unwrap_or_default(),
            args: more
                .as_deref()
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(name, value)| {
                    (
                        name.trim().to_owned(),
                        lokyr_platform_core::automation::Arg::Text(value.to_owned()),
                    )
                })
                .collect(),
        },
        PANE_TEXT => Request::PaneText {
            pane: id(first),
            selection: second == 1,
        },
        WORKSPACE => Request::Workspace,
        SWITCH_SESSION => Request::SwitchSession {
            name: text.unwrap_or_default(),
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

/// The status and the text of the automation's `answer`.
fn answer_of(answer: Answer) -> (i32, String) {
    match answer {
        Answer::Done => (0, String::new()),
        Answer::Opened { window, tab, pane } => (0, format!("{window} {tab} {pane}")),
        Answer::Text(text) => (0, text),
        Answer::Workspace(windows) => {
            let lines: Vec<String> = windows
                .iter()
                .flat_map(|window| &window.tabs)
                .flat_map(|tab| &tab.panes)
                .map(|pane| {
                    let clean = |text: &str| text.replace(['\t', '\n'], " ");
                    format!(
                        "{}\t{}\t{}",
                        pane.id,
                        clean(&pane.title),
                        clean(pane.folder.as_deref().unwrap_or_default())
                    )
                })
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
        assert_eq!(
            answer_of(workspace),
            (0, "3\tzsh\t/tmp\n4\ta b\t/tmp".to_owned())
        );
    }
}

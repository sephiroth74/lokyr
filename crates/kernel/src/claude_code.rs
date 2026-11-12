//! The hooks of Claude Code's settings by which it tells Lokyr its state,
//! exactly, rather than as its output and its quiet let Lokyr guess: at
//! work as the user submits a request, waiting as it asks for a permission
//! or an answer, done as it ends its answer, failed as an error ends it.
//!
//! Each hook writes a notification of OSC 777 of a title of
//! [`HOOKED`], through the `terminalSequence` field of its output, which
//! Claude Code writes to its terminal itself
//! (<https://code.claude.com/docs/en/hooks>), and only while it runs in
//! Lokyr, as `TERM_PROGRAM` tells, so that another terminal shows nothing.
//! [`merged`] adds them to a `settings.json`, after taking away those that
//! an earlier version added, and keeps the rest of the file as it was
//! written, its other keys in their order and their text.

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::Value;
use serde_json::value::RawValue;

pub use crate::workspace::agent::HOOKED;
use crate::workspace::agent::MESSAGE;

/// The events of Claude Code's hooks that tell a state, the matcher of
/// each, and the state that it tells.
const EVENTS: [(&str, Option<&str>, &str); 4] = [
    ("UserPromptSubmit", None, "working"),
    (
        "Notification",
        Some("permission_prompt|elicitation_dialog|agent_needs_input"),
        "waiting",
    ),
    ("Stop", None, "done"),
    ("StopFailure", None, "error"),
];

/// Why a `settings.json` cannot take the hooks.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// It is not JSON, as the parser tells.
    Invalid(String),
    /// It is JSON of another kind than an object, or its `hooks` is.
    NotAnObject,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(error) => write!(formatter, "it is not JSON: {error}"),
            Self::NotAnObject => formatter.write_str("it is not a JSON object of hooks"),
        }
    }
}

impl std::error::Error for Error {}

/// The shell's command of the hook that tells `state`: nothing outside
/// Lokyr; of a waiting state, the message of Claude Code's notification,
/// which `jq` reads of the hook's input, of no control character, C0, DEL
/// or C1, which would end the sequence and run the rest as the terminal's,
/// and of [`MESSAGE`] characters at most; or none where `jq` is missing.
fn command(state: &str) -> String {
    let plain =
        format!(r#"printf '{{"terminalSequence":"\\u001b]777;notify;{HOOKED}{state};\\u0007"}}'"#);
    let told = if state == "waiting" {
        format!(
            r#"/usr/bin/jq -c '{{terminalSequence: ("\u001b]777;notify;{HOOKED}{state};" + ((.message // "") | tostring | explode | map(select(. >= 32 and (. < 127 or . > 159))) | .[0:{MESSAGE}] | implode) + "\u0007")}}' 2>/dev/null || {plain}"#
        )
    } else {
        plain
    };
    format!(r#"if [ "$TERM_PROGRAM" = lokyr ]; then {told}; fi"#)
}

/// The hooks that the settings take, as `settings.json`'s `hooks` writes
/// them: each event's matcher group of its one command.
pub fn hooks() -> Vec<(&'static str, Value)> {
    EVENTS
        .iter()
        .map(|&(event, matcher, state)| {
            let mut group = serde_json::Map::new();
            if let Some(matcher) = matcher {
                group.insert("matcher".to_owned(), Value::from(matcher));
            }
            group.insert(
                "hooks".to_owned(),
                serde_json::json!([{ "type": "command", "command": command(state) }]),
            );
            (event, Value::Object(group))
        })
        .collect()
}

/// An object's members in their order, each of its text as written.
struct Ordered(Vec<(String, Box<RawValue>)>);

impl<'de> Deserialize<'de> for Ordered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Members;
        impl<'de> Visitor<'de> for Members {
            type Value = Ordered;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Ordered, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry::<String, Box<RawValue>>()? {
                    members.push(member);
                }
                Ok(Ordered(members))
            }
        }
        deserializer.deserialize_map(Members)
    }
}

/// The object of `text`'s members, or why it is none.
fn ordered(text: &str) -> Result<Ordered, Error> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| Error::Invalid(error.to_string()))?;
    if !value.is_object() {
        return Err(Error::NotAnObject);
    }
    serde_json::from_str(text).map_err(|error| Error::Invalid(error.to_string()))
}

/// Whether a matcher group is one of Lokyr's: each of its hooks a command
/// that tells a state.
fn is_lokyr_s(group: &Value) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            !hooks.is_empty()
                && hooks.iter().all(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|command| command.contains(HOOKED))
                })
        })
}

/// `text`, JSON of `indent` spaces, its lines after the first indented by
/// `by` more.
fn indented(text: &str, by: usize) -> String {
    let pad = " ".repeat(by);
    text.lines()
        .enumerate()
        .map(|(at, line)| {
            if at == 0 {
                line.to_owned()
            } else {
                format!("{pad}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The members `members` as an object, two spaces a level, at the level
/// `level`.
fn object(members: &[(String, String)], level: usize) -> String {
    if members.is_empty() {
        return "{}".to_owned();
    }
    let inner = " ".repeat((level + 1) * 2);
    let outer = " ".repeat(level * 2);
    let lines: Vec<String> = members
        .iter()
        .map(|(key, value)| {
            let key = serde_json::to_string(key).unwrap_or_default();
            format!("{inner}{key}: {value}")
        })
        .collect();
    format!("{{\n{}\n{outer}}}", lines.join(",\n"))
}

/// `settings`, the text of a `settings.json`, or none of a file not there,
/// with Lokyr's hooks, those that an earlier version added taken away: its
/// other keys, events and groups as they were, in their order.
///
/// # Errors
///
/// [`Error`] of a file that is not a JSON object, or whose `hooks` is not
/// one, which it leaves as it is.
pub fn merged(settings: Option<&str>) -> Result<String, Error> {
    let settings = settings
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("{}");
    let top = ordered(settings)?;
    let hooks_of = top.0.iter().find(|(key, _)| key == "hooks");
    let mut events: Vec<(String, String)> = match hooks_of {
        Some((_, raw)) => ordered(raw.get())?
            .0
            .into_iter()
            .map(|(event, raw)| (event, raw.get().to_owned()))
            .collect(),
        None => Vec::new(),
    };
    for (event, group) in hooks() {
        let at = events.iter().position(|(name, _)| name == event);
        let mut groups: Vec<Value> = match at {
            Some(at) => {
                serde_json::from_str::<Vec<Value>>(&events[at].1).map_err(|_| Error::NotAnObject)?
            }
            None => Vec::new(),
        };
        groups.retain(|group| !is_lokyr_s(group));
        groups.push(group);
        let text = indented(
            &serde_json::to_string_pretty(&groups).unwrap_or_default(),
            4,
        );
        match at {
            Some(at) => events[at].1 = text,
            None => events.push((event.to_owned(), text)),
        }
    }
    let hooks = object(&events, 1);
    let mut members: Vec<(String, String)> = top
        .0
        .iter()
        .map(|(key, raw)| {
            let value = if key == "hooks" {
                hooks.clone()
            } else {
                raw.get().to_owned()
            };
            (key.clone(), value)
        })
        .collect();
    if hooks_of.is_none() {
        members.push(("hooks".to_owned(), hooks));
    }
    Ok(format!("{}\n", object(&members, 0)))
}

/// Whether `settings` holds Lokyr's hooks as this version writes them.
pub fn has_hooks(settings: &str) -> bool {
    merged(Some(settings)).is_ok_and(|with| {
        let parse = |text: &str| serde_json::from_str::<Value>(text).ok();
        parse(&with) == parse(settings)
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn the_waiting_hook_s_message_keeps_no_control_character() {
        let command = command("waiting");
        // A message of a bell, an escape and a C1 control, as the hook's
        // input gives it, through the shell, in Lokyr.
        let input = r#"{"message":"ok\u0007\u001b]52;c;eA==\u0007 done\u009b è"}"#;
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", &command])
            .env("TERM_PROGRAM", "lokyr")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        let told: Value = serde_json::from_slice(&output.stdout).unwrap();
        let sequence = told["terminalSequence"].as_str().unwrap();
        let body = sequence
            .strip_prefix(&format!("\u{1b}]777;notify;{HOOKED}waiting;"))
            .and_then(|rest| rest.strip_suffix('\u{7}'))
            .unwrap();
        assert!(!body.chars().any(|c| c.is_control()), "{body:?}");
        // Where jq is, the message's printable rest; where not, none.
        if Path::new("/usr/bin/jq").exists() {
            assert_eq!(body, "ok]52;c;eA== done è");
        }
        assert!(command.contains(&format!(".[0:{MESSAGE}]")));
    }

    #[test]
    fn a_new_file_takes_the_hooks_alone() {
        let text = merged(None).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let hooks = value["hooks"].as_object().unwrap();
        let events: Vec<&str> = hooks.keys().map(String::as_str).collect();
        assert_eq!(events.len(), 4);
        for (event, matcher, state) in EVENTS {
            let group = &hooks[event][0];
            assert_eq!(group.get("matcher").and_then(Value::as_str), matcher);
            let command = group["hooks"][0]["command"].as_str().unwrap();
            assert!(command.starts_with(r#"if [ "$TERM_PROGRAM" = lokyr ]; then "#));
            assert!(command.contains(&format!("{HOOKED}{state};")), "{command}");
        }
        assert!(has_hooks(&text));
        assert!(!has_hooks("{}"));
    }

    #[test]
    fn a_file_keeps_its_keys_their_order_and_their_text_and_other_hooks() {
        let settings = r#"{
  "permissions": {
    "allow": ["Bash(git status)"],
    "defaultMode": "default"
  },
  "hooks": {
    "PostToolUse": [{"matcher": "Edit", "hooks": [{"type": "command", "command": "fmt"}]}],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "say done"
          }
        ]
      }
    ]
  },
  "model": "opus"
}"#;
        let text = merged(Some(settings)).unwrap();
        // The other keys, in their order and as written.
        let permissions = text.find("\"permissions\"").unwrap();
        let hooks = text.find("\"hooks\"").unwrap();
        let model = text.find("\"model\"").unwrap();
        assert!(permissions < hooks && hooks < model, "{text}");
        assert!(text.contains(r#""allow": ["Bash(git status)"],"#), "{text}");
        assert!(text.contains(
            r#"[{"matcher": "Edit", "hooks": [{"type": "command", "command": "fmt"}]}]"#
        ));
        // The user's own Stop hook stays, before Lokyr's.
        let value: Value = serde_json::from_str(&text).unwrap();
        let stop = value["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0]["hooks"][0]["command"], "say done");
        assert!(is_lokyr_s(&stop[1]));
        // Again: the same, no hook twice.
        assert_eq!(merged(Some(&text)).unwrap(), text);
        assert!(has_hooks(&text));
    }

    #[test]
    fn an_earlier_version_s_hooks_are_replaced_and_a_wrong_file_left_alone() {
        let old = format!(
            r#"{{"hooks": {{"Stop": [{{"hooks": [{{"type": "command", "command": "printf {HOOKED}done-old"}}]}}]}}}}"#
        );
        let value: Value = serde_json::from_str(&merged(Some(&old)).unwrap()).unwrap();
        let stop = value["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(stop[0]["hooks"][0]["command"], command("done"));
        assert!(matches!(merged(Some("{ not json")), Err(Error::Invalid(_))));
        assert_eq!(merged(Some("[1, 2]")), Err(Error::NotAnObject));
        assert_eq!(merged(Some(r#"{"hooks": 3}"#)), Err(Error::NotAnObject));
    }

    #[test]
    fn a_hook_s_command_writes_the_sequence_that_the_terminal_reads() {
        // The shell's printf turns `\\u001b` into the JSON escape of ESC.
        let done = command("done");
        assert!(
            done.contains(
                r#"printf '{"terminalSequence":"\\u001b]777;notify;lokyr-agent:done;\\u0007"}'"#
            ),
            "{done}"
        );
        let waiting = command("waiting");
        assert!(waiting.contains("/usr/bin/jq -c"), "{waiting}");
        assert!(waiting.contains(r#"(.message // "")"#), "{waiting}");
    }
}

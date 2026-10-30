//! Pastes into a program that takes their line breaks as Return: when the
//! user is asked first, what a single line keeps of its line break, and the
//! text of the question, its lines and its preview, or as one line.

use std::borrow::Cow;

use crate::config::ConfirmPaste;

/// What a paste does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Paste<'a> {
    /// It pastes this text now.
    Now(Cow<'a, str>),
    /// It asks the user first.
    Ask,
}

/// What a paste of `text` does, into a program that asks for bracketed
/// paste when `bracketed`, as `confirm` says: with `Auto`, a single line
/// with a line break after it, and nothing but spaces after that, pastes
/// without them, so that its command waits for the user's Return.
pub fn decide(text: &str, bracketed: bool, confirm: ConfirmPaste) -> Paste<'_> {
    let breaks = text.contains(['\n', '\r']);
    match confirm {
        ConfirmPaste::Never => Paste::Now(Cow::Borrowed(text)),
        _ if !breaks => Paste::Now(Cow::Borrowed(text)),
        ConfirmPaste::Auto if bracketed => Paste::Now(Cow::Borrowed(text)),
        ConfirmPaste::Auto => match line_ending(text) {
            Some(line) => Paste::Now(Cow::Borrowed(line)),
            None => Paste::Ask,
        },
        ConfirmPaste::Always => Paste::Ask,
    }
}

/// The line of `text` when it is one with a single line break after it,
/// CR, LF or CR LF, and nothing but spaces and tabs after that.
fn line_ending(text: &str) -> Option<&str> {
    let end = text.find(['\n', '\r'])?;
    let rest = &text[end..];
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix(['\n', '\r']))?;
    rest.chars()
        .all(|c| c == ' ' || c == '\t')
        .then_some(&text[..end])
}

/// How many lines `text` has: its line breaks, CR LF as one, and the line
/// after the last, if it is not empty.
pub fn lines(text: &str) -> usize {
    let breaks =
        text.matches('\n').count() + text.matches('\r').count() - text.matches("\r\n").count();
    breaks + usize::from(!text.ends_with(['\n', '\r']))
}

/// `text` as one line: a space for each line break, CR LF as one, and none
/// for the last one.
pub fn one_line(text: &str) -> String {
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix(['\n', '\r']))
        .unwrap_or(text);
    text.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

/// The first `most` lines of `text` that are not empty, as they would
/// reach a program, without the control characters that a paste drops,
/// each cut to `width` characters, with `…` for what is left out.
pub fn preview(text: &str, most: usize, width: usize) -> String {
    let mut lines: Vec<String> = text
        .split(['\n', '\r'])
        .filter(|line| !line.is_empty())
        .take(most + 1)
        .map(|line| {
            let kept: String = line
                .chars()
                .filter(|&c| c == '\t' || !c.is_control())
                .collect();
            match kept.char_indices().nth(width) {
                Some((at, _)) => format!("{}…", &kept[..at]),
                None => kept,
            }
        })
        .collect();
    if lines.len() > most {
        lines.truncate(most);
        lines.push("…".to_owned());
    }
    lines.join("\n")
}

/// The text that a drop of files at `paths` pastes: each path as a shell
/// takes it, one argument of its own, between single quotes unless it is
/// only of letters, digits and `_-./+,:@%=^`, a single quote within as
/// `'\''`; the paths apart by spaces, and a space after the last, so that
/// what is typed next is an argument of its own.
pub fn dropped_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> String {
    let plain = |character: char| character.is_alphanumeric() || "_-./+,:@%=^".contains(character);
    let mut text = String::new();
    for path in paths {
        if !path.is_empty() && path.chars().all(plain) {
            text.push_str(path);
        } else {
            text.push('\'');
            text.push_str(&path.replace('\'', "'\\''"));
            text.push('\'');
        }
        text.push(' ');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(text: &str) -> Paste<'_> {
        Paste::Now(Cow::Borrowed(text))
    }

    #[test]
    fn dropped_paths_are_a_shell_s_arguments() {
        assert_eq!(dropped_paths(["/usr/bin", "/tmp/a,b@c=d"]), "/usr/bin /tmp/a,b@c=d ");
        // Spaces, quotes, the shell's own characters, and a single quote.
        assert_eq!(
            dropped_paths(["/tmp/a file.txt", "/tmp/$HOME", "/tmp/it's"]),
            "'/tmp/a file.txt' '/tmp/$HOME' '/tmp/it'\\''s' "
        );
        // Letters of other scripts are plain; a newline is quoted.
        assert_eq!(dropped_paths(["/tmp/città", "/tmp/a\nb"]), "/tmp/città '/tmp/a\nb' ");
        assert_eq!(dropped_paths([]), "");
    }

    #[test]
    fn a_paste_asks_as_the_config_says() {
        use ConfirmPaste::{Always, Auto, Never};
        // Without a line break, never.
        for confirm in [Auto, Always, Never] {
            assert_eq!(decide("ls -la", false, confirm), now("ls -la"));
        }
        // Several lines ask, unless the program brackets them.
        assert_eq!(decide("a\nb", false, Auto), Paste::Ask);
        assert_eq!(decide("a\rb", false, Auto), Paste::Ask);
        assert_eq!(decide("a\nb", true, Auto), now("a\nb"));
        assert_eq!(decide("a\nb", true, Always), Paste::Ask);
        assert_eq!(decide("a\nb", false, Never), now("a\nb"));
        // A line and its line break: the line alone, but bracketed.
        for text in ["ls\n", "ls\r\n", "ls\r", "ls\n  \t"] {
            assert_eq!(decide(text, false, Auto), now("ls"), "{text:?}");
        }
        assert_eq!(decide("ls\n", true, Auto), now("ls\n"));
        assert_eq!(decide("ls\n", false, Always), Paste::Ask);
        assert_eq!(decide("ls\n\n", false, Auto), Paste::Ask);
        assert_eq!(decide("ls\nx", false, Auto), Paste::Ask);
        assert_eq!(decide("\n", false, Auto), now(""));
    }

    #[test]
    fn a_paste_s_lines_its_preview_and_its_one_line() {
        for (text, count) in [
            ("ls\n", 1),
            ("a\nb", 2),
            ("a\r\nb\r\n", 2),
            ("a\rb", 2),
            ("\n", 1),
            ("a\n\n", 2),
        ] {
            assert_eq!(lines(text), count, "{text:?}");
        }
        // What the program would get: no escape, the tab kept.
        assert_eq!(preview("a\x1b[31mb\tc\nd\n", 3, 60), "a[31mb\tc\nd");
        // Long lines cut, at a character's boundary, and lines past the
        // most left out.
        let long = "é".repeat(70);
        assert_eq!(preview(&long, 3, 60), format!("{}…", "é".repeat(60)));
        assert_eq!(preview("1\n2\n3\n4\n5", 3, 60), "1\n2\n3\n…");
        assert_eq!(preview("1\n2\n3\n", 3, 60), "1\n2\n3");
        // A space for each line break, none for the last.
        assert_eq!(one_line("a\nb\r\nc\rd\n"), "a b c d");
        assert_eq!(one_line("a\n\nb"), "a  b");
        assert_eq!(one_line("a\r\n"), "a");
        assert_eq!(one_line("a"), "a");
    }
}

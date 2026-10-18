//! The terminfo entry that describes the terminal, read from the source
//! that `tic` compiles, for XTGETTCAP to answer from.

use std::sync::LazyLock;

/// The entry's source.
const SOURCE: &str = include_str!("../../terminfo/xterm-bolt.terminfo");

/// What the entry says of a capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Capability {
    /// A boolean, which the entry has.
    Flag,
    /// A number, in decimal, or a string: the bytes that the terminal reads
    /// or sends, without the padding that only `tputs` reads, or, for a
    /// string with parameters, the string as the source writes it, for the
    /// program to fill in.
    Value(Vec<u8>),
}

/// The entry's capabilities, sorted by name.
static CAPABILITIES: LazyLock<Vec<(String, Capability)>> = LazyLock::new(|| parse(SOURCE));

/// The capability of the entry whose terminfo name is `name`.
pub(super) fn capability(name: &[u8]) -> Option<&'static Capability> {
    let capabilities = &*CAPABILITIES;
    let at = capabilities
        .binary_search_by(|(other, _)| other.as_bytes().cmp(name))
        .ok()?;
    Some(&capabilities[at].1)
}

/// The capabilities of the one entry in `source`, sorted by name, without
/// those that it cancels with `@`.
fn parse(source: &str) -> Vec<(String, Capability)> {
    let text = source
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let mut capabilities: Vec<(String, Capability)> = fields(&text)
        // The entry's names.
        .skip(1)
        .filter_map(|field| {
            Some(if let Some((name, value)) = field.split_once('=') {
                let value = if value.contains('%') {
                    value.as_bytes().to_vec()
                } else {
                    without_padding(&unescape(value))
                };
                (name.to_owned(), Capability::Value(value))
            } else if let Some((name, number)) = field.split_once('#') {
                let number: u32 = number.parse().expect("a number in decimal");
                (
                    name.to_owned(),
                    Capability::Value(number.to_string().into()),
                )
            } else if field.is_empty() || field.ends_with('@') {
                return None;
            } else {
                (field.to_owned(), Capability::Flag)
            })
        })
        .collect();
    capabilities.sort_by(|a, b| a.0.cmp(&b.0));
    capabilities
}

/// The fields of `text`, trimmed: what comes before each comma that no
/// backslash escapes.
fn fields(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let bytes = rest.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'\\' => at += 2,
                b',' => {
                    let field = rest[..at].trim();
                    rest = &rest[at + 1..];
                    return Some(field);
                }
                _ => at += 1,
            }
        }
        None
    })
}

/// `bytes` without padding, which terminfo(5) writes as `$<5>` or `$<100/>`:
/// a delay for `tputs` to make, which the terminal never receives.
fn without_padding(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some(at) = rest.windows(2).position(|pair| pair == b"$<") {
        let delay = rest[at + 2..]
            .iter()
            .position(|&byte| byte == b'>')
            .filter(|&len| {
                let delay = &rest[at + 2..at + 2 + len];
                delay.iter().any(u8::is_ascii_digit)
                    && delay
                        .iter()
                        .all(|byte| matches!(byte, b'0'..=b'9' | b'.' | b'*' | b'/'))
            });
        let end = delay.map_or(at + 2, |len| at + 2 + len + 1);
        out.extend_from_slice(&rest[..at]);
        if delay.is_none() {
            out.extend_from_slice(b"$<");
        }
        rest = &rest[end..];
    }
    out.extend_from_slice(rest);
    out
}

/// The bytes of a string in the source, its escapes undone, as
/// terminfo(5) lists them: `\E` for ESC, `^X` for a control character,
/// three octal digits, and a backslash before a character that stands for
/// itself.
fn unescape(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        at += 1;
        match byte {
            b'\\' => {
                let Some(&escaped) = bytes.get(at) else {
                    out.push(b'\\');
                    break;
                };
                at += 1;
                out.push(match escaped {
                    b'E' | b'e' => 0x1B,
                    b'n' | b'l' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'f' => 0x0C,
                    b's' => b' ',
                    b'0'..=b'7' => {
                        let digits = bytes[at - 1..]
                            .iter()
                            .take(3)
                            .take_while(|digit| matches!(digit, b'0'..=b'7'))
                            .count();
                        let octal = bytes[at - 1..at - 1 + digits]
                            .iter()
                            .fold(0_u8, |n, digit| {
                                n.wrapping_mul(8).wrapping_add(digit - b'0')
                            });
                        at += digits - 1;
                        // A string ends at a NUL, so `\0` stands for 0x80.
                        if octal == 0 { 0x80 } else { octal }
                    }
                    other => other,
                });
            }
            b'^' => match bytes.get(at) {
                Some(&control) => {
                    at += 1;
                    out.push(if control == b'?' {
                        0x7F
                    } else {
                        control & 0x1F
                    });
                }
                None => out.push(b'^'),
            },
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(name: &str) -> Vec<u8> {
        match capability(name.as_bytes()) {
            Some(Capability::Value(value)) => value.clone(),
            other => panic!("{name}: {other:?}"),
        }
    }

    #[test]
    fn strings_come_as_the_terminal_sends_them() {
        assert_eq!(value("kbs"), b"\x7f");
        assert_eq!(value("bel"), b"\x07");
        assert_eq!(value("kcuu1"), b"\x1bOA");
        assert_eq!(value("dsl"), b"\x1b]2;\x07");
        assert_eq!(value("Se"), b"\x1b[0 q");
        assert_eq!(value("flash"), b"\x1b[?5h\x1b[?5l", "without padding");
        assert_eq!(
            value("acsc"),
            b"``aaffggiijjkkllmmnnooppqqrrssttuuvvwwxxyyzz{{||}}~~"
        );
    }

    #[test]
    fn strings_with_parameters_come_as_the_source_writes_them() {
        assert_eq!(value("Smulx"), br"\E[4:%p1%dm");
        assert_eq!(value("cup"), br"\E[%i%p1%d;%p2%dH");
        assert_eq!(value("Sync"), br"\E[?2026%?%p1%{1}%-%tl%eh%;");
    }

    #[test]
    fn numbers_and_booleans() {
        assert_eq!(value("colors"), b"256");
        assert_eq!(value("pairs"), b"32767");
        for flag in ["Tc", "Su", "am", "bce", "XT"] {
            assert_eq!(
                capability(flag.as_bytes()),
                Some(&Capability::Flag),
                "{flag}"
            );
        }
        for missing in ["RGB", "smm", "mc0", "mc5i", "xterm-bolt", "bolt", ""] {
            assert_eq!(capability(missing.as_bytes()), None, "{missing}");
        }
    }

    #[test]
    fn the_source_has_each_capability_once() {
        assert!(SOURCE.is_ascii());
        let names: Vec<&str> = CAPABILITIES.iter().map(|(name, _)| name.as_str()).collect();
        assert!(
            names.windows(2).all(|pair| pair[0] < pair[1]),
            "a name repeats"
        );
        // One capability on each line that starts with a tab.
        let lines = SOURCE.lines().filter(|line| line.starts_with('\t')).count();
        assert_eq!(names.len(), lines);
    }

    #[test]
    fn a_source_has_comments_cancelled_capabilities_and_escaped_commas() {
        let source = "# a comment, with commas\nname|other|A terminal,\n\tam,,\n# more\n\tbw@, cols#80,\n\tx=a\\,b, y=^?^[\\s\\:\\\\\\,\\0\\101\\1, z=\\E[%p1%d@,\n";
        assert_eq!(
            parse(source),
            [
                ("am".to_owned(), Capability::Flag),
                ("cols".to_owned(), Capability::Value(b"80".to_vec())),
                ("x".to_owned(), Capability::Value(b"a,b".to_vec())),
                (
                    "y".to_owned(),
                    Capability::Value(b"\x7f\x1b :\\,\x80A\x01".to_vec())
                ),
                ("z".to_owned(), Capability::Value(br"\E[%p1%d@".to_vec())),
            ]
        );
        assert_eq!(unescape("\\"), b"\\");
        assert_eq!(unescape("^"), b"^");
        // Three octal digits at most.
        assert_eq!(unescape("\\0101"), b"\x081");
        assert_eq!(unescape("\\n\\l\\r\\t\\b\\f\\e"), b"\n\n\r\t\x08\x0c\x1b");
        // Padding goes, and what only looks like it stays.
        assert_eq!(
            without_padding(b"a$<5>b$<10.5*/>c$<x>d$<>e$<3"),
            b"abc$<x>d$<>e$<3"
        );
    }
}

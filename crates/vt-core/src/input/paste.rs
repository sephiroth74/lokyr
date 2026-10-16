//! Pastes, which bracketed paste (mode 2004) sets apart from typing:
//! <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Bracketed-Paste-Mode>.

use crate::terminal::InputModes;

/// Appends to `out` what pasting `text` sends: with bracketed paste, the
/// text between `CSI 200 ~` and `CSI 201 ~`; without, the text with each
/// line break, CR LF, LF or CR, sent as Return sends it, CR.
///
/// Either way, the control characters other than tab and the line breaks
/// are left out, C0, DEL and C1 alike, so that a paste can neither end the
/// bracket early, with its own `ESC [ 201 ~`, nor run a program's commands,
/// as ESC does in an editor.
pub fn encode_paste(text: &str, modes: &InputModes, out: &mut Vec<u8>) {
    let bracketed = modes.bracketed_paste;
    if bracketed {
        out.extend_from_slice(b"\x1b[200~");
    }
    // Runs of text that need no change go in whole.
    let mut run = 0;
    let mut after_cr = false;
    for (at, c) in text.char_indices() {
        let kept = match c {
            '\t' => true,
            '\r' | '\n' => bracketed,
            _ => !c.is_control(),
        };
        if !kept {
            out.extend_from_slice(&text.as_bytes()[run..at]);
            run = at + c.len_utf8();
            // Line breaks are left out only unbracketed, to be sent as CR,
            // with CR LF as one break.
            if c == '\r' || (c == '\n' && !after_cr) {
                out.push(b'\r');
            }
        }
        after_cr = c == '\r';
    }
    out.extend_from_slice(&text.as_bytes()[run..]);
    if bracketed {
        out.extend_from_slice(b"\x1b[201~");
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn paste(text: &str, bracketed: bool) -> String {
        let modes = InputModes {
            bracketed_paste: bracketed,
            ..InputModes::default()
        };
        let mut out = Vec::new();
        encode_paste(text, &modes, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn bracketed_paste_sets_the_text_apart() {
        assert_eq!(paste("hello", true), "\x1b[200~hello\x1b[201~");
        assert_eq!(paste("", true), "\x1b[200~\x1b[201~");
        assert_eq!(
            paste("a\nb\r\nc\rd\te", true),
            "\x1b[200~a\nb\r\nc\rd\te\x1b[201~"
        );
        assert_eq!(paste("é😀", true), "\x1b[200~é😀\x1b[201~");
    }

    #[test]
    fn a_paste_cannot_end_the_bracket() {
        let sent = paste("ls\x1b[201~rm -rf ~\r", true);
        assert_eq!(sent, "\x1b[200~ls[201~rm -rf ~\r\x1b[201~");
        // Nor with CSI as a C1 control.
        let sent = paste("ls\u{9b}201~rm", true);
        assert_eq!(sent, "\x1b[200~ls201~rm\x1b[201~");
    }

    #[test]
    fn control_characters_are_left_out() {
        let controls = "\0\x07\x08\x0b\x0c\x1b\x7f\u{80}\u{85}\u{9b}\u{9f}";
        assert_eq!(
            paste(&format!("a{controls}\tb"), true),
            "\x1b[200~a\tb\x1b[201~"
        );
        assert_eq!(paste(&format!("a{controls}\tb"), false), "a\tb");
        assert_eq!(paste("x\x1b[31my", false), "x[31my");
        // Characters past C1 stay.
        assert_eq!(paste("\u{a0}\u{2028}", false), "\u{a0}\u{2028}");
    }

    #[test]
    fn unbracketed_line_breaks_are_returns() {
        assert_eq!(paste("a\nb\r\nc\rd", false), "a\rb\rc\rd");
        assert_eq!(paste("\n\n", false), "\r\r");
        assert_eq!(paste("\r\n\r\n", false), "\r\r");
        assert_eq!(paste("\r\r\n", false), "\r\r");
        assert_eq!(
            paste("\r\x07\n", false),
            "\r\r",
            "a dropped control parts CR and LF"
        );
        assert_eq!(paste("", false), "");
        assert_eq!(paste("plain", false), "plain");
    }

    proptest! {
        #[test]
        fn nothing_inside_the_bracket_is_a_control_but_tabs_and_breaks(text in ".*|[\\x00-\\x1f\\x7f-\\u{9f}\\[0-9~a]*") {
            let sent = paste(&text, true);
            let inner = sent
                .strip_prefix("\x1b[200~")
                .and_then(|rest| rest.strip_suffix("\x1b[201~"))
                .expect("the bracket");
            prop_assert!(inner.chars().all(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r')));
            let unbracketed = paste(&text, false);
            prop_assert!(unbracketed.chars().all(|c| !c.is_control() || matches!(c, '\t' | '\r')));
        }
    }
}

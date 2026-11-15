//! What a person reads of a text: the characters that hide or move the
//! text around them, which the texts of plugins and of packages refuse.

/// Whether `character` hides or moves the text around it, which a text that
/// a person reads, as a consent's question or a terminal, would show
/// otherwise than it is: a format character of Unicode (`Cf`), the changes
/// of direction and the characters of no width among them, but the
/// joiners, which scripts and emoji take, and the marks of direction,
/// which override no strong character and Arabic and Hebrew texts take;
/// and the separators of lines and paragraphs.
pub fn is_hidden(character: char) -> bool {
    if matches!(
        character,
        '\u{200C}' | '\u{200D}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    ) {
        return false;
    }
    matches!(
        u32::from(character),
        0x00AD
            | 0x0600..=0x0605
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// The most characters of a text that a plugin declares for the menus or
/// the titlebar, as a command's or a menu item's title.
pub const LONGEST_LABEL: usize = 256;

/// Whether `text`, `what` of a plugin's, as "a command's title", is a text
/// that it may declare, which the app keeps and shows in its menus: of
/// [`LONGEST_LABEL`] characters at most, of no control character nor one
/// that hides or moves the text around it; the error that tells it is not.
pub fn check_label(text: &str, what: &str) -> Result<(), crate::command::Error> {
    let fine = text.chars().count() <= LONGEST_LABEL
        && !text.chars().any(|c| c.is_control() || is_hidden(c));
    if fine {
        return Ok(());
    }
    Err(crate::command::Error::new(
        crate::command::Code::InvalidSchema,
        format!(
            "{what} has {LONGEST_LABEL} characters at most, of no control character nor one that \
             hides the text around it"
        ),
    ))
}

/// `text` as a person may read it of a plugin, as a question's: of no
/// control character but its line breaks where `lines` keeps them, nor one
/// that hides or moves the text around it, and of `most` characters at
/// most, its end cut.
pub fn cleaned(text: &str, most: usize, lines: bool) -> String {
    text.chars()
        .filter(|&c| !(c.is_control() && !(lines && c == '\n')) && !is_hidden(c))
        .take(most)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cleaned_text_keeps_what_it_shows_its_lines_where_asked_within_its_bound() {
        let text = "Use\u{202E}txt.exe\u{0007} it?\nYes";
        assert_eq!(cleaned(text, 100, true), "Usetxt.exe it?\nYes");
        assert_eq!(cleaned(text, 100, false), "Usetxt.exe it?Yes");
        assert_eq!(cleaned(text, 3, true), "Use");
        // The joiners of an emoji, and the marks of an Arabic text, stay.
        let kept = "👩\u{200D}💻 مرحبا\u{200F}";
        assert_eq!(cleaned(kept, 100, false), kept);
    }

    #[test]
    fn a_label_is_short_and_shows_what_it_says() {
        for fine in [
            "Split Right",
            "Spezza a destra…",
            "مرحبا\u{200F}",
            "👩\u{200D}💻",
            "",
        ] {
            assert_eq!(check_label(fine, "a title"), Ok(()), "{fine:?}");
        }
        let long = "x".repeat(LONGEST_LABEL + 1);
        for wrong in [long.as_str(), "Co\u{200B}py", "a\nb", "\u{202E}ypoC"] {
            let error = check_label(wrong, "a title").unwrap_err();
            assert!(error.message.starts_with("a title has"), "{wrong:?}");
        }
        assert_eq!(check_label(&"x".repeat(LONGEST_LABEL), "a title"), Ok(()));
    }

    #[test]
    fn changes_of_direction_and_characters_of_no_width_hide_and_joiners_do_not() {
        for hidden in [
            '\u{202E}',
            '\u{2066}',
            '\u{200B}',
            '\u{2060}',
            '\u{FEFF}',
            '\u{00AD}',
            '\u{2028}',
            '\u{2029}',
            '\u{E0041}',
            '\u{FFF9}',
        ] {
            assert!(is_hidden(hidden), "{hidden:?}");
        }
        for shown in [
            'a', 'é', '\u{200C}', '\u{200D}', '\u{200E}', '\u{200F}', '\u{061C}', '😀',
        ] {
            assert!(!is_hidden(shown), "{shown:?}");
        }
    }
}

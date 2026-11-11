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

#[cfg(test)]
mod tests {
    use super::*;

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

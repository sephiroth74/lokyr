//! The Unicode tables: their shape, and the properties of characters that
//! the grid depends on.

use super::*;

/// Every range table, with its name, as plain ranges.
fn ranges() -> Vec<(&'static str, Vec<(u32, u32)>)> {
    fn plain<T>(table: &[(u32, u32, T)]) -> Vec<(u32, u32)> {
        table.iter().map(|&(start, end, _)| (start, end)).collect()
    }
    vec![
        ("EAST_ASIAN_WIDTH", plain(tables::EAST_ASIAN_WIDTH)),
        (
            "LEGACY_EAST_ASIAN_WIDTH",
            plain(tables::LEGACY_EAST_ASIAN_WIDTH),
        ),
        ("MARKS", plain(tables::MARKS)),
        ("FORMAT", tables::FORMAT.to_vec()),
        ("DEFAULT_IGNORABLE", tables::DEFAULT_IGNORABLE.to_vec()),
        ("EMOJI_PRESENTATION", tables::EMOJI_PRESENTATION.to_vec()),
        (
            "EXTENDED_PICTOGRAPHIC",
            tables::EXTENDED_PICTOGRAPHIC.to_vec(),
        ),
        ("GRAPHEME_BREAK", plain(tables::GRAPHEME_BREAK)),
        ("INDIC_CONJUNCT_BREAK", plain(tables::INDIC_CONJUNCT_BREAK)),
    ]
}

#[test]
fn the_tables_hold_ranges_apart_and_in_order() {
    for (name, ranges) in ranges() {
        assert!(!ranges.is_empty(), "{name} is empty");
        for &(start, end) in &ranges {
            assert!(
                start <= end && end <= 0x10_FFFF,
                "{name}: {start:X}..{end:X}"
            );
        }
        for pair in ranges.windows(2) {
            assert!(
                pair[0].1 < pair[1].0,
                "{name}: {:X?} and {:X?}",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn the_version_is_pinned() {
    assert_eq!(UNICODE_VERSION, (18, 0, 0));
}

#[test]
fn cjk_is_wide() {
    for c in ['中', '㐀', 'あ', 'ア', '한', '\u{FF21}', '\u{3000}'] {
        assert_eq!(east_asian_width(c), EastAsianWidth::Wide, "{c}");
    }
    // Unassigned code points of the CJK blocks and of planes 2 and 3 are
    // wide too.
    for c in ['\u{2FFFD}', '\u{3FFFD}', '\u{9FFF}'] {
        assert_eq!(
            east_asian_width(c),
            EastAsianWidth::Wide,
            "{:X}",
            u32::from(c)
        );
    }
    // Halfwidth forms are narrow.
    assert_eq!(east_asian_width('\u{FF71}'), EastAsianWidth::Narrow);
}

#[test]
fn some_characters_are_ambiguous() {
    for c in ['¡', '①', '※', '○', 'α', 'Ж', '\u{2500}'] {
        assert_eq!(east_asian_width(c), EastAsianWidth::Ambiguous, "{c}");
    }
    assert_eq!(east_asian_width('a'), EastAsianWidth::Narrow);
}

#[test]
fn emoji_are_wide_since_unicode_9() {
    for c in ['😀', '⌚', '🎉'] {
        assert_eq!(east_asian_width(c), EastAsianWidth::Wide, "{c}");
        assert_eq!(legacy_east_asian_width(c), EastAsianWidth::Narrow, "{c}");
        assert!(has_emoji_presentation(c), "{c}");
        assert!(is_extended_pictographic(c), "{c}");
    }
    // A heart is pictographic, but shows as text unless VS16 follows it.
    assert!(is_extended_pictographic('\u{2764}'));
    assert!(!has_emoji_presentation('\u{2764}'));
    assert_eq!(east_asian_width('\u{2764}'), EastAsianWidth::Narrow);
    // CJK was wide in Unicode 8 too.
    assert_eq!(legacy_east_asian_width('中'), EastAsianWidth::Wide);
}

#[test]
fn combining_characters_are_marks_that_extend() {
    assert_eq!(mark('\u{301}'), Some(Mark::Nonspacing));
    assert_eq!(mark('\u{20DD}'), Some(Mark::Enclosing));
    assert_eq!(mark('\u{903}'), Some(Mark::Spacing));
    assert_eq!(mark('a'), None);
    assert_eq!(grapheme_break('\u{301}'), GraphemeBreak::Extend);
    assert_eq!(grapheme_break('\u{903}'), GraphemeBreak::SpacingMark);
}

#[test]
fn zwj_and_variation_selectors_join_and_show_nothing() {
    assert!(is_format('\u{200D}'));
    assert!(is_default_ignorable('\u{200D}'));
    assert_eq!(grapheme_break('\u{200D}'), GraphemeBreak::ZWJ);
    for selector in ['\u{FE0E}', '\u{FE0F}'] {
        assert!(is_default_ignorable(selector));
        assert_eq!(mark(selector), Some(Mark::Nonspacing));
        assert_eq!(grapheme_break(selector), GraphemeBreak::Extend);
    }
}

#[test]
fn grapheme_breaks_cover_controls_hangul_and_flags() {
    assert_eq!(grapheme_break('\r'), GraphemeBreak::CR);
    assert_eq!(grapheme_break('\n'), GraphemeBreak::LF);
    assert_eq!(grapheme_break('\u{7}'), GraphemeBreak::Control);
    assert_eq!(grapheme_break('\u{1100}'), GraphemeBreak::L);
    assert_eq!(grapheme_break('\u{1161}'), GraphemeBreak::V);
    assert_eq!(grapheme_break('\u{11A8}'), GraphemeBreak::T);
    assert_eq!(grapheme_break('가'), GraphemeBreak::LV);
    assert_eq!(grapheme_break('각'), GraphemeBreak::LVT);
    assert_eq!(
        grapheme_break('\u{1F1EE}'),
        GraphemeBreak::RegionalIndicator
    );
    assert_eq!(grapheme_break('\u{600}'), GraphemeBreak::Prepend);
    assert_eq!(grapheme_break('a'), GraphemeBreak::Other);
}

#[test]
fn indic_conjuncts_have_linkers_and_consonants() {
    assert_eq!(indic_conjunct_break('\u{94D}'), IndicConjunctBreak::Linker);
    assert_eq!(
        indic_conjunct_break('\u{915}'),
        IndicConjunctBreak::Consonant
    );
    assert_eq!(indic_conjunct_break('\u{93C}'), IndicConjunctBreak::Extend);
    assert_eq!(indic_conjunct_break('a'), IndicConjunctBreak::None);
}

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
fn the_properties_table_agrees_with_the_range_tables() {
    for c in (0..=0x10_FFFF).filter_map(char::from_u32) {
        let expected = Properties {
            east_asian_width: east_asian_width(c),
            legacy_east_asian_width: legacy_east_asian_width(c),
            grapheme_break: grapheme_break(c),
            indic_conjunct_break: indic_conjunct_break(c),
            mark: mark(c),
            format: is_format(c),
            default_ignorable: is_default_ignorable(c),
            extended_pictographic: is_extended_pictographic(c),
        };
        assert_eq!(properties(c), expected, "U+{:04X}", u32::from(c));
    }
}

#[test]
fn the_properties_table_stays_small() {
    let bytes = size_of_val(&tables::PROPERTY_INDEX)
        + size_of_val(&tables::PROPERTY_BLOCKS)
        + size_of_val(&tables::PROPERTIES);
    assert!(bytes <= 64 * 1024, "{bytes} bytes");
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

/// The conformance test of the UCD for grapheme cluster boundaries.
const BREAK_TEST: &str = include_str!("../../testdata/GraphemeBreakTest.txt");

#[test]
fn segmentation_passes_the_unicode_conformance_test() {
    let mut cases = 0;
    for (number, line) in (1..).zip(BREAK_TEST.lines()) {
        let case = line.split('#').next().unwrap_or_default().trim();
        if case.is_empty() {
            continue;
        }
        // `÷ 0020 × 0308 ÷`: a mark before each codepoint, and one at the end.
        let mut expected = Vec::new();
        let mut chars = Vec::new();
        let mut starts = true;
        for token in case.split_whitespace() {
            match token {
                "÷" => starts = true,
                "×" => starts = false,
                hex => {
                    let code = u32::from_str_radix(hex, 16).expect("a codepoint");
                    chars.push(char::from_u32(code).expect("a scalar value"));
                    expected.push(starts);
                }
            }
        }
        let mut segmenter = Segmenter::new();
        let found: Vec<bool> = chars.iter().map(|&c| segmenter.starts_cluster(c)).collect();
        assert_eq!(found, expected, "line {number}: {line}");
        cases += 1;
    }
    assert!(cases > 800, "only {cases} cases");
}

#[test]
fn a_segmenter_can_start_over() {
    let mut segmenter = Segmenter::new();
    assert!(segmenter.starts_cluster('e'));
    assert!(!segmenter.starts_cluster('\u{301}'));
    segmenter.reset();
    assert!(
        segmenter.starts_cluster('\u{301}'),
        "a mark at the start of the text"
    );
}

#[test]
fn ascii_starts_a_cluster_as_its_characters_do() {
    // Text that leaves the segmenter in each of its states.
    let before = [
        "",
        "a",
        "\r",
        "\n",
        "\u{1}",
        "e\u{301}",
        "a\u{200D}",
        "\u{1F1EE}",
        "\u{1F1EE}\u{1F1F9}",
        "\u{600}",
        "\u{D4E}",
        "\u{915}\u{93E}",
        "\u{1100}",
        "\u{1161}",
        "\u{11A8}",
        "\u{AC00}",
        "\u{AC01}",
        "\u{915}\u{94D}",
        "\u{1F600}",
        "\u{1F600}\u{200D}",
    ];
    for text in before {
        let mut segmenter = Segmenter::new();
        for c in text.chars() {
            segmenter.starts_cluster(c);
        }
        for byte in 0x20_u8..=0x7E {
            let (mut fast, mut slow) = (segmenter, segmenter);
            assert_eq!(
                fast.ascii_starts_cluster(),
                slow.starts_cluster(char::from(byte)),
                "{byte:#x} after {text:?}"
            );
            assert_eq!(fast, slow, "{byte:#x} after {text:?}");
        }
    }
}

/// The clusters of `text`, as the segmenter finds them.
fn clusters(text: &str) -> Vec<Vec<char>> {
    let mut segmenter = Segmenter::new();
    let mut clusters: Vec<Vec<char>> = Vec::new();
    for c in text.chars() {
        let starts = segmenter.starts_cluster(c);
        match clusters.last_mut() {
            Some(cluster) if !starts => cluster.push(c),
            _ => clusters.push(vec![c]),
        }
    }
    clusters
}

/// The cells of `text`, cluster by cluster, as unicode mode counts them.
fn unicode_widths(text: &str, widths: Widths) -> Vec<u8> {
    clusters(text)
        .iter()
        .map(|cluster| widths.cluster_width(cluster[0], &cluster[1..]))
        .collect()
}

/// The cells of `text`, codepoint by codepoint, as legacy mode counts them.
fn legacy_widths(text: &str, widths: Widths) -> Vec<u8> {
    text.chars().map(|c| widths.char_width(c)).collect()
}

const CURRENT: Widths = Widths {
    mode: Mode::Unicode,
    tables: Tables::Current,
    ambiguous_wide: false,
};

const UNICODE_8: Widths = Widths {
    tables: Tables::Unicode8,
    ..CURRENT
};

const LEGACY: Widths = Widths {
    mode: Mode::Legacy,
    ..CURRENT
};

#[test]
fn the_default_widths_are_unicode_mode_with_current_tables() {
    assert_eq!(Widths::default(), CURRENT);
}

#[test]
fn single_characters_take_their_east_asian_width() {
    for widths in [CURRENT, LEGACY] {
        assert_eq!(widths.char_width('a'), 1);
        assert_eq!(widths.char_width('中'), 2);
        assert_eq!(widths.char_width('\u{FF71}'), 1, "halfwidth katakana");
        assert_eq!(widths.char_width('😀'), 2);
        assert_eq!(widths.char_width('①'), 1, "ambiguous");
    }
    assert_eq!(
        UNICODE_8.char_width('😀'),
        1,
        "emoji were narrow in Unicode 8"
    );
    assert_eq!(UNICODE_8.char_width('中'), 2);
}

#[test]
fn ambiguous_characters_can_count_as_wide() {
    for tables in [Tables::Current, Tables::Unicode8] {
        let wide = Widths {
            tables,
            ambiguous_wide: true,
            ..CURRENT
        };
        for c in ['①', '※', '○', 'α', '\u{2500}'] {
            assert_eq!(wide.char_width(c), 2, "{c}");
        }
        assert_eq!(wide.char_width('a'), 1);
    }
}

#[test]
fn characters_of_no_width() {
    for c in [
        '\u{301}', '\u{20DD}', '\u{200B}', '\u{200D}', '\u{FE0F}', '\u{1160}', '\u{11A8}', '\0',
        // Format characters that are not default ignorable.
        '\u{FFF9}', '\u{FFFA}',
    ] {
        assert_eq!(CURRENT.char_width(c), 0, "{:X}", u32::from(c));
    }
    // A prepended concatenation mark, a format character that shows,
    // takes a cell in the unicode mode with the current tables alone.
    assert_eq!(CURRENT.char_width('\u{600}'), 1);
    assert_eq!(LEGACY.char_width('\u{600}'), 0);
    assert_eq!(UNICODE_8.char_width('\u{600}'), 0);
    assert_eq!(CURRENT.char_width('\u{AD}'), 1, "the soft hyphen shows");
    assert_eq!(
        CURRENT.char_width('\u{903}'),
        1,
        "a spacing mark takes a cell"
    );
}

#[test]
fn the_hangul_fillers_take_their_east_asian_width() {
    assert_eq!(CURRENT.char_width('\u{115F}'), 2, "the choseong filler");
    assert_eq!(CURRENT.char_width('\u{3164}'), 2, "the filler");
    assert_eq!(CURRENT.char_width('\u{FFA0}'), 1, "the halfwidth filler");
    assert_eq!(
        CURRENT.char_width('\u{1160}'),
        0,
        "the jungseong filler joins"
    );
    assert_eq!(unicode_widths("\u{115F}\u{1161}", CURRENT), [2]);
}

#[test]
fn a_prepended_character_and_the_one_after_it_take_two_cells() {
    // The sign shows over the digit after it, a cell of its own, as
    // `wcwidth` 0.9 and Ghostty count it.
    assert_eq!(CURRENT.cluster_width('\u{600}', &['1']), 2);
    assert_eq!(CURRENT.cluster_width('\u{600}', &['中']), 2);
    assert_eq!(CURRENT.cluster_width('\u{600}', &['\u{301}', '1']), 2);
    assert_eq!(CURRENT.cluster_width('\u{600}', &[]), 1);
    assert_eq!(unicode_widths("\u{6DD}\u{661}\u{662}", CURRENT), [2, 1]);
    assert_eq!(
        CURRENT.cluster_width('\u{600}', &['\u{1F1FA}', '\u{1F1F8}']),
        2
    );
    assert_eq!(
        CURRENT.cluster_width('\u{600}', &['\u{2764}', '\u{FE0F}']),
        2
    );
    // With the tables of Unicode 8, the sign has no width, and a cluster
    // takes the width of the codepoint after it.
    assert_eq!(UNICODE_8.cluster_width('\u{600}', &['1']), 1);
    assert_eq!(UNICODE_8.cluster_width('\u{600}', &['中']), 2);
    assert_eq!(UNICODE_8.cluster_width('\u{600}', &['\u{301}', '中']), 2);
    assert_eq!(UNICODE_8.cluster_width('\u{600}', &[]), 0);
}

#[test]
fn clusters_take_the_width_of_their_first_codepoint() {
    assert_eq!(unicode_widths("e\u{301}a", CURRENT), [1, 1]);
    assert_eq!(unicode_widths("中\u{301}", CURRENT), [2]);
    // Hangul jamo make one syllable, as wide as its leading consonant.
    assert_eq!(unicode_widths("\u{1100}\u{1161}\u{11A8}", CURRENT), [2]);
    // A consonant with vowel signs that do not space.
    assert_eq!(unicode_widths("\u{915}\u{941}", CURRENT), [1], "कु");
    assert_eq!(unicode_widths("\u{E17}\u{E35}\u{E48}", CURRENT), [1], "ที่");
}

#[test]
fn codepoints_that_space_make_a_cluster_two_cells() {
    // Vowel signs that space, as `wcwidth` 0.9 and Ghostty count them, one
    // of them a mark that extends, Bengali's া.
    assert_eq!(unicode_widths("\u{915}\u{93E}", CURRENT), [2], "का");
    assert_eq!(unicode_widths("\u{915}\u{93F}", CURRENT), [2], "कि");
    assert_eq!(unicode_widths("\u{995}\u{9BE}", CURRENT), [2], "কা");
    assert_eq!(unicode_widths("\u{B95}\u{BBE}", CURRENT), [2], "கா");
    assert_eq!(unicode_widths("\u{D15}\u{D4A}", CURRENT), [2], "കൊ");
    // Conjuncts that a virama joins into one cluster, of two consonants or
    // three, and with a vowel sign; Tamil's virama ends its cluster.
    assert_eq!(unicode_widths("\u{915}\u{94D}\u{937}", CURRENT), [2], "क्ष");
    assert_eq!(
        unicode_widths("\u{938}\u{94D}\u{924}\u{94D}\u{930}", CURRENT),
        [2],
        "स्त्र"
    );
    assert_eq!(
        unicode_widths("\u{915}\u{94D}\u{937}\u{93F}", CURRENT),
        [2],
        "क्षि"
    );
    assert_eq!(
        unicode_widths("\u{B95}\u{BCD}\u{BB7}", CURRENT),
        [1, 1],
        "க்ஷ"
    );
    // A voiced mark of halfwidth katakana, which spaces.
    assert_eq!(unicode_widths("\u{FF76}\u{FF9E}", CURRENT), [2], "ｶﾞ");
    // A skin tone colours the emoji before it, and takes no cells of its
    // own; nor does a prepended character after the first.
    assert_eq!(CURRENT.cluster_width('a', &['\u{1F3FD}']), 1);
    assert_eq!(CURRENT.cluster_width('\u{D4E}', &['\u{D4E}']), 1);
    assert_eq!(CURRENT.cluster_width('\u{D4E}', &['\u{D4E}', 'a']), 2);
    // A wide cluster stays two cells.
    assert_eq!(unicode_widths("中\u{93E}", CURRENT), [2]);
    // The tables of Unicode 8, and the legacy mode, are as they were.
    assert_eq!(unicode_widths("\u{915}\u{94D}\u{937}", UNICODE_8), [1]);
    assert_eq!(legacy_widths("\u{915}\u{93E}", LEGACY), [1, 1]);
    // What spaces in a cluster, and what leaves a narrow one so: VS16 has
    // no width, but widens an emoji.
    for (c, spaces, keeps) in [
        ('\u{93E}', true, false),
        ('\u{301}', false, true),
        ('\u{200D}', false, true),
        ('\u{FE0F}', false, false),
    ] {
        let properties = crate::unicode::properties(c);
        assert_eq!(
            CURRENT.spaces_in_cluster(c, properties),
            spaces,
            "{:X}",
            u32::from(c)
        );
        assert_eq!(
            CURRENT.keeps_cells(1, c, properties),
            keeps,
            "{:X}",
            u32::from(c)
        );
    }
}

#[test]
fn emoji_presentation_sets_the_width_of_a_cluster() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    assert_eq!(unicode_widths(family, CURRENT), [2], "a ZWJ sequence");
    assert_eq!(unicode_widths("\u{2764}", CURRENT), [1]);
    assert_eq!(unicode_widths("\u{2764}\u{FE0F}", CURRENT), [2], "VS16");
    assert_eq!(unicode_widths("\u{231A}\u{FE0E}", CURRENT), [1], "VS15");
    assert_eq!(
        unicode_widths("1\u{FE0F}\u{20E3}", CURRENT),
        [2],
        "a keycap"
    );
    assert_eq!(unicode_widths("\u{1F1EE}\u{1F1F9}", CURRENT), [2], "a flag");
    // Half a flag takes two cells alone too, as `wcwidth` 0.9 counts it.
    assert_eq!(unicode_widths("\u{1F1EE}", CURRENT), [2], "half a flag");
    for after in ['\u{301}', '\u{200D}', '\u{FE0F}'] {
        assert_eq!(
            CURRENT.cluster_width('\u{1F1EE}', &[after]),
            2,
            "half a flag and {after:?}"
        );
    }
    assert_eq!(UNICODE_8.char_width('\u{1F1EE}'), 1);
    assert_eq!(
        unicode_widths("\u{1F1EE}\u{1F1F9}\u{1F1EB}\u{1F1F7}", CURRENT),
        [2, 2],
        "two flags"
    );
    assert_eq!(
        unicode_widths("\u{1F44D}\u{1F3FD}", CURRENT),
        [2],
        "a skin tone"
    );
    // Without emoji presentation, VS16 only joins.
    assert_eq!(unicode_widths("a\u{FE0F}", CURRENT), [1]);
}

#[test]
fn unicode_8_knew_no_emoji_presentation() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    assert_eq!(unicode_widths(family, UNICODE_8), [1]);
    assert_eq!(unicode_widths("\u{2764}\u{FE0F}", UNICODE_8), [1]);
    assert_eq!(unicode_widths("\u{1F1EE}\u{1F1F9}", UNICODE_8), [1]);
    assert_eq!(unicode_widths("中", UNICODE_8), [2]);
}

#[test]
fn legacy_mode_counts_codepoint_by_codepoint() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    assert_eq!(legacy_widths(family, LEGACY), [2, 0, 2, 0, 2]);
    assert_eq!(legacy_widths("\u{2764}\u{FE0F}", LEGACY), [1, 0]);
    assert_eq!(legacy_widths("e\u{301}", LEGACY), [1, 0]);
    assert_eq!(legacy_widths("\u{1F1EE}\u{1F1F9}", LEGACY), [1, 1]);
    assert_eq!(legacy_widths("\u{1100}\u{1161}", LEGACY), [2, 0]);
    let old = Widths {
        tables: Tables::Unicode8,
        ..LEGACY
    };
    assert_eq!(legacy_widths(family, old), [1, 0, 1, 0, 1]);
}

use super::*;

#[test]
fn the_licenses_entries_read_as_the_collection_writes_them() {
    let text = "# A comment.\n\n\
                '0x96f':\n  license: MIT\n  source: https://github.com/filipjanevski/0x96f-term-theme\n\
                13th Floor:\n  license: unknown\n  source: ''\n  note: 'Not covered by the research in #638.'\n\
                Jack O' Lantern:\n  license: unknown\n  source: ''\n\
                Alabaster:\n  license: MIT\n  source: 'https://github.com/mkaschenko/iterm2-theme-alabaster\n\n    https://github.com/tonsky/sublime-scheme-alabaster'\n\
                Quoted:\n  license: MIT OR Apache-2.0\n  source: 'it''s\n    folded'\n";
    let read = entries(text).unwrap();
    let entry = |license: &str, source: &str| Entry {
        license: license.to_owned(),
        source: source.to_owned(),
    };
    assert_eq!(
        read["0x96f"],
        entry("MIT", "https://github.com/filipjanevski/0x96f-term-theme")
    );
    assert_eq!(read["13th Floor"], entry("unknown", ""));
    assert_eq!(read["Jack O' Lantern"], entry("unknown", ""));
    // An empty line of a scalar of several lines a line feed, a line break
    // a space.
    assert_eq!(
        read["Alabaster"],
        entry(
            "MIT",
            "https://github.com/mkaschenko/iterm2-theme-alabaster\nhttps://github.com/tonsky/sublime-scheme-alabaster"
        )
    );
    assert_eq!(read["Quoted"], entry("MIT OR Apache-2.0", "it's folded"));
    assert_eq!(read.len(), 5);
    // What the collection does not write is refused.
    for wrong in [
        "A:\n license: MIT\n",
        "A:\n  license: MIT\nA:\n  license: MIT\n",
        "A:\n  source: x\n",
        "  license: MIT\n",
        "A:\n  license: \"MIT\"\n",
        "A:\n  license: 'MIT\n",
    ] {
        assert!(entries(wrong).is_err(), "{wrong:?}");
    }
}

/// A scheme of grays, its colors of `space`, its foreground white.
fn scheme(space: &str) -> String {
    let entry = |gray: f64, space: &str| {
        format!(
            "<dict><key>Red Component</key><real>{gray}</real><key>Green Component</key>\
             <real>{gray}</real><key>Blue Component</key><real>{gray}</real>\
             <key>Color Space</key><string>{space}</string></dict>"
        )
    };
    let mut body = String::new();
    for index in 0..16 {
        body.push_str(&format!(
            "<key>Ansi {index} Color</key>{}",
            entry(f64::from(index) / 15.0, space)
        ));
    }
    body.push_str(&format!("<key>Background Color</key>{}", entry(0.2, space)));
    body.push_str(&format!(
        "<key>Foreground Color</key>{}",
        entry(1.0, "sRGB")
    ));
    format!("<plist version=\"1.0\"><dict>{body}</dict></plist>")
}

/// The theme of [`scheme`] of sRGB colors.
fn grays() -> Theme {
    let text = scheme("sRGB");
    themes::itermcolors::read(text.as_bytes(), "Grays", Path::new("Grays.itermcolors"))
        .unwrap()
        .remove(0)
}

#[test]
fn a_theme_differs_from_its_ghostty_file_where_their_colors_do() {
    assert!(of_srgb_or_p3(scheme("sRGB").as_bytes()));
    assert!(of_srgb_or_p3(scheme("P3").as_bytes()));
    assert!(!of_srgb_or_p3(scheme("Calibrated").as_bytes()));
    assert!(!of_srgb_or_p3(b"no plist"));
    let mut file = String::new();
    for index in 0..16 {
        let gray = (f64::from(index) / 15.0 * 255.0).round() as u8;
        file.push_str(&format!(
            "palette = {index}=#{gray:02x}{gray:02x}{gray:02x}\n"
        ));
    }
    // Of the colors that both give, whatever their case.
    file.push_str("background = #333333\nforeground = #FFFFFF\ncursor-color = #ff0000\n");
    let theme = grays();
    assert_eq!(differences(&theme, &file, false), Vec::<String>::new());
    // Within a unit of a byte.
    let near = file.replace("background = #333333", "background = #343332");
    assert_eq!(differences(&theme, &near, false), Vec::<String>::new());
    let wrong = file.replace("background = #333333", "background = #353535");
    assert_eq!(
        differences(&theme, &wrong, false),
        ["background is #333333, where Ghostty's file has #353535"]
    );
    // Raised as the collection's tools raise them, the grays too near the
    // background differ.
    let raised = differences(&theme, &file, true);
    assert_eq!(
        raised[0],
        "palette 1 is #5e5e5e, where Ghostty's file has #111111"
    );
    assert_eq!(raised.len(), 5, "{raised:?}");
}

#[test]
fn a_color_of_too_little_contrast_is_raised_as_the_collection_s_tools_raise_it() {
    // Of the collection's own algorithm, run on the same colors.
    for (color, background, raised_to) in [
        ([0x11; 3], [0x33; 3], [0x5e; 3]),
        ([0x55; 3], [0x33; 3], [0x62; 3]),
        ([0xee, 0xee, 0xe0], [0xff; 3], [0xbb, 0xbb, 0xad]),
        ([0x80, 0x10, 0x10], [0x20; 3], [0x8d, 0x1d, 0x1d]),
        ([0x00; 3], [0x00; 3], [0x40; 3]),
        ([0xff; 3], [0x00; 3], [0xff; 3]),
    ] {
        assert_eq!(
            raised(color, background),
            raised_to,
            "{color:?} on {background:?}"
        );
    }
}

#[test]
fn the_notices_list_each_theme_with_its_license_and_source() {
    let mut theme = grays();
    theme.name = "One | Two".to_owned();
    let held = Held {
        theme,
        scheme: "One | Two".to_owned(),
        license: "MIT".to_owned(),
        source: "a\nb".to_owned(),
    };
    let text = readme(&[held]);
    assert!(text.contains(COMMIT));
    assert!(text.contains("| One \\| Two | MIT | a<br>b |"), "{text}");
}

#[test]
fn every_license_of_a_held_theme_has_its_text() {
    let texts = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(TEXTS);
    for license in PERMISSIVE.iter().flat_map(|license| license.split(" OR ")) {
        let path = texts.join(format!("{license}.txt"));
        assert!(path.is_file(), "{}", path.display());
    }
    for (name, license, _) in CHECKED {
        assert!(PERMISSIVE.contains(&license), "{name}");
        let path = texts.join("themes").join(format!("{name}.txt"));
        assert!(path.is_file(), "{}", path.display());
    }
}

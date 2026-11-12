use super::*;

/// A color's dictionary of `rgb` in `space`, as an `.itermcolors` file
/// writes it, of no space where `space` is `None`.
fn entry(rgb: [f64; 3], space: Option<&str>) -> String {
    let space = space.map_or_else(String::new, |space| {
        format!("<key>Color Space</key><string>{space}</string>")
    });
    format!(
        "<dict><key>Red Component</key><real>{}</real><key>Green Component</key><real>{}</real>\
         <key>Blue Component</key><real>{}</real>{space}</dict>",
        rgb[0], rgb[1], rgb[2]
    )
}

/// The color of `rgb` in `space`, as the importer reads it.
fn read_color(rgb: [f64; 3], space: Option<&str>) -> [u8; 3] {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\">{}</plist>",
        entry(rgb, space)
    );
    let value = Value::from_reader(Cursor::new(xml.as_bytes())).unwrap();
    let color = component_color(&value).unwrap();
    [color.red, color.green, color.blue]
}

/// Whether `got` is `want`, AppKit's, of components from 0 to 1, within a
/// unit of a byte.
fn near(got: [u8; 3], want: [f64; 3]) -> bool {
    got.iter()
        .zip(want)
        .all(|(&got, want)| (f64::from(got) - want * 255.0).abs() <= 1.0)
}

/// The colors of `NSColor`'s conversion to sRGB, of components in each
/// space and of what AppKit gives of them.
const SAMPLES: [[f64; 3]; 6] = [
    [0.5, 0.5, 0.5],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [0.2, 0.4, 0.6],
    [0.9, 0.1, 0.3],
];

#[test]
fn each_color_space_converts_as_appkit_converts_it() {
    let calibrated = [
        [0.57231, 0.57231, 0.57231],
        [1.0, 0.14910, 0.0],
        [0.0, 0.97679, 0.0],
        [0.01684, 0.19834, 1.0],
        [0.25279, 0.48029, 0.66490],
        [0.92989, 0.20538, 0.37092],
    ];
    let p3 = [
        [0.5, 0.5, 0.5],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.00028, 1.0],
        [0.10399, 0.40594, 0.61699],
        [0.98325, 0.0, 0.28103],
    ];
    for (index, sample) in SAMPLES.into_iter().enumerate() {
        let srgb = read_color(sample, Some("sRGB"));
        assert!(near(srgb, sample), "sRGB {sample:?}: {srgb:?}");
        let got = read_color(sample, Some("P3"));
        assert!(near(got, p3[index]), "P3 {sample:?}: {got:?}");
        // Calibrated, Device and none alike, as iTerm2 reads them.
        for space in [Some("Calibrated"), Some("Device"), None] {
            let got = read_color(sample, space);
            assert!(
                near(got, calibrated[index]),
                "{space:?} {sample:?}: {got:?}"
            );
        }
    }
}

/// A file of the 16 colors and the background and foreground of `ansi`,
/// each of a gray of its index, sRGB, and of `extra` entries.
fn file(extra: &str) -> Vec<u8> {
    let mut body = String::new();
    for index in 0..16 {
        let gray = f64::from(index) / 15.0;
        body.push_str(&format!(
            "<key>Ansi {index} Color</key>{}",
            entry([gray; 3], Some("sRGB"))
        ));
    }
    body.push_str(&format!(
        "<key>Background Color</key>{}",
        entry([0.1; 3], Some("sRGB"))
    ));
    body.push_str(&format!(
        "<key>Foreground Color</key>{}",
        entry([0.9; 3], Some("sRGB"))
    ));
    body.push_str(extra);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\
         <plist version=\"1.0\"><dict>{body}</dict></plist>"
    )
    .into_bytes()
}

#[test]
fn a_file_s_keys_give_the_terminal_s_colors_and_the_search_s_match() {
    let extra = format!(
        "<key>Cursor Color</key>{}<key>Selection Color</key>{}<key>Link Color</key>{}\
         <key>Match Background Color</key>{}<key>Badge Color</key>{}",
        entry([1.0, 0.5, 0.0], Some("sRGB")),
        entry([0.2, 0.3, 0.4], Some("sRGB")),
        entry([0.0, 0.0, 1.0], Some("sRGB")),
        entry([1.0, 1.0, 0.0], Some("sRGB")),
        entry([1.0, 0.0, 0.0], Some("sRGB")),
    );
    let themes = read(&file(&extra), "Grays", Path::new("Grays.itermcolors")).unwrap();
    assert_eq!(themes.len(), 1);
    let theme = &themes[0];
    assert_eq!(theme.name, "Grays");
    assert_eq!(theme.variant, ThemeVariant::Dark);
    assert_eq!(theme.terminal.palette[15], ThemeColor::rgb(255, 255, 255));
    assert_eq!(theme.terminal.background, ThemeColor::rgb(26, 26, 26));
    assert_eq!(theme.terminal.cursor, Some(ThemeColor::rgb(255, 128, 0)));
    assert_eq!(theme.terminal.link, Some(ThemeColor::rgb(0, 0, 255)));
    assert_eq!(theme.terminal.bold, None);
    assert_eq!(theme.tokens["search.match"], ThemeColor::rgb(255, 255, 0));
}

#[test]
fn a_file_of_the_light_and_dark_keys_is_two_themes() {
    let extra = format!(
        "<key>Background Color (Light)</key>{}<key>Foreground Color (Light)</key>{}\
         <key>Background Color (Dark)</key>{}",
        entry([1.0; 3], Some("sRGB")),
        entry([0.0; 3], Some("sRGB")),
        entry([0.0; 3], Some("sRGB")),
    );
    let themes = read(&file(&extra), "Pair", Path::new("Pair.itermcolors")).unwrap();
    let names: Vec<&str> = themes.iter().map(|theme| theme.name.as_str()).collect();
    assert_eq!(names, ["Pair (Light)", "Pair (Dark)"]);
    let (light, dark) = (&themes[0], &themes[1]);
    assert_eq!(light.variant, ThemeVariant::Light);
    assert_eq!(light.terminal.background, ThemeColor::rgb(255, 255, 255));
    assert_eq!(light.terminal.foreground, ThemeColor::rgb(0, 0, 0));
    // The dark one's own background, and the plain foreground of none of
    // its own.
    assert_eq!(dark.variant, ThemeVariant::Dark);
    assert_eq!(dark.terminal.background, ThemeColor::rgb(0, 0, 0));
    assert_eq!(dark.terminal.foreground, ThemeColor::rgb(230, 230, 230));
}

#[test]
fn a_file_that_is_no_list_of_colors_or_lacks_one_is_refused() {
    let path = Path::new("Wrong.itermcolors");
    assert!(read(b"not a plist", "Wrong", path).is_err());
    let lacking = String::from_utf8(file(""))
        .unwrap()
        .replace("Ansi 7 Color", "Ansi 7 Colour");
    let error = read(lacking.as_bytes(), "Wrong", path).unwrap_err();
    assert!(
        error.message.contains("no `Ansi 7 Color`"),
        "{}",
        error.message
    );
    assert_eq!(error.path, path);
}

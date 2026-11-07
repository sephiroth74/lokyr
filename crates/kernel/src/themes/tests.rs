use super::*;

/// A themes' folder of the tests' own, of `files`, each a name and its
/// text, which `test` names.
fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("lokyr-themes-{test}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    for (name, text) in files {
        fs::write(directory.join(format!("{name}.toml")), text).unwrap();
    }
    directory
}

/// A built-in theme of the tests', dark, of a token of its own.
fn builtin() -> Theme {
    let mut theme = Theme::of_terminal(
        DARK,
        ThemeVariant::Dark,
        TerminalColors {
            background: ThemeColor::hex(0x1e1e20),
            foreground: ThemeColor::hex(0xe6e6e6),
            palette: [ThemeColor::hex(0x45454b); 16],
            cursor: None,
            cursor_text: None,
            selection: Some(ThemeColor::hex(0x34445f)),
            selection_text: None,
            bold: None,
            link: None,
            underline: None,
        },
    );
    theme
        .tokens
        .insert("panel.bg".to_owned(), ThemeColor::hex(0x2a2a2d));
    theme
}

const PALETTE: &str = r##"palette = ["#000000", "#110000", "#001100", "#111100", "#000011", "#110011", "#001111", "#111111",
           "#222222", "#330000", "#003300", "#333300", "#000033", "#330033", "#003333", "#ffffff"]"##;

#[test]
fn a_file_gives_its_terminal_its_search_and_its_sections_tokens() {
    let text = format!(
        r##"[theme]
name = "Nightfall"
author = "Ada"
license = "MIT"
variant = "dark"

[terminal]
background = "#101012"
foreground = "#e0e0e0"
{PALETTE}
cursor = "#ff8800"
selection = "#334455cc"
bold = "#ffffff"

[search]
match = "#ffd60a4d"
current = "#ff7a0080"

[interface]
"panel.bg" = "#202024"
field = {{ bg = "#101012" }}

[tabs]
bar.bg = "#18181a"

[tabs.active]
bg = "#101012"
fg = "#ffffff"
"##
    );
    let directory = folder("whole", &[("Nightfall", &text)]);
    let resolved = resolve("Nightfall", Some(&directory), &[builtin()]).unwrap();
    let theme = resolved.theme;
    assert_eq!(theme.name, "Nightfall");
    assert_eq!(theme.variant, ThemeVariant::Dark);
    assert_eq!(theme.terminal.background, ThemeColor::hex(0x101012));
    assert_eq!(theme.terminal.palette[15], ThemeColor::hex(0xffffff));
    assert_eq!(theme.terminal.cursor, Some(ThemeColor::hex(0xff8800)));
    assert_eq!(
        theme.terminal.selection,
        Some(ThemeColor::hex(0x334455).with_alpha(0xcc))
    );
    let token = |name: &str| theme.tokens.get(name).copied();
    assert_eq!(
        token("search.match"),
        Some(ThemeColor::hex(0xffd60a).with_alpha(0x4d))
    );
    assert_eq!(
        token("search.match.current"),
        Some(ThemeColor::hex(0xff7a00).with_alpha(0x80))
    );
    assert_eq!(token("panel.bg"), Some(ThemeColor::hex(0x202024)));
    assert_eq!(token("field.bg"), Some(ThemeColor::hex(0x101012)));
    assert_eq!(token("tabs.bar.bg"), Some(ThemeColor::hex(0x18181a)));
    assert_eq!(token("tabs.active.bg"), Some(ThemeColor::hex(0x101012)));
    assert_eq!(token("tabs.active.fg"), Some(ThemeColor::hex(0xffffff)));
    assert_eq!(resolved.files, [directory.join("Nightfall.toml")]);
}

#[test]
fn a_theme_extends_another_whose_values_its_own_override() {
    let directory = folder(
        "extends",
        &[
            (
                "Dim",
                "[theme]\nextends = \"Lokyr Dark\"\n\n[terminal]\nbackground = \"#000000\"\n",
            ),
            (
                "Dimmer",
                "[theme]\nextends = \"Dim\"\n\n[interface]\n\"panel.bg\" = \"#050505\"\n",
            ),
        ],
    );
    let resolved = resolve("Dimmer", Some(&directory), &[builtin()]).unwrap();
    let theme = resolved.theme;
    // Its name its own, its background Dim's, the rest the built-in one's.
    assert_eq!(theme.name, "Dimmer");
    assert_eq!(theme.terminal.background, ThemeColor::hex(0x000000));
    assert_eq!(theme.terminal.foreground, ThemeColor::hex(0xe6e6e6));
    assert_eq!(theme.terminal.selection, Some(ThemeColor::hex(0x34445f)));
    assert_eq!(theme.tokens["panel.bg"], ThemeColor::hex(0x050505));
    assert_eq!(theme.variant, ThemeVariant::Dark);
    // Every file looked for, the built-in one's of no file too.
    assert_eq!(
        resolved.files,
        [
            directory.join("Dimmer.toml"),
            directory.join("Dim.toml"),
            directory.join("Lokyr Dark.toml"),
            directory.join("Lokyr Dark.itermcolors"),
        ]
    );
}

#[test]
fn a_file_of_a_built_in_theme_s_name_takes_its_place_and_a_variant_of_none_follows_its_colors() {
    let text =
        format!("[terminal]\nbackground = \"#fafafa\"\nforeground = \"#1e1e20\"\n{PALETTE}\n");
    let directory = folder("own", &[(DARK, &text)]);
    let theme = resolve(DARK, Some(&directory), &[builtin()]).unwrap().theme;
    assert_eq!(theme.terminal.background, ThemeColor::hex(0xfafafa));
    assert_eq!(theme.variant, ThemeVariant::Light);
    assert!(theme.tokens.is_empty(), "the built-in one's tokens gone");
    // Without the folder, the built-in one.
    let theme = resolve(DARK, None, &[builtin()]).unwrap().theme;
    assert_eq!(theme, builtin());
}

#[test]
fn a_file_s_errors_tell_their_places() {
    let wrong = |text: &str| {
        let directory = folder("wrong", &[("Wrong", text)]);
        resolve("Wrong", Some(&directory), &[builtin()]).unwrap_err()
    };
    let error = wrong("[terminal]\nbackground = \"#10101\"\n");
    assert_eq!(
        error.position,
        Some(Position {
            line: 2,
            column: 14
        })
    );
    assert!(error.message.contains("is no color"), "{}", error.message);
    let error = wrong("[terminal]\nbackdrop = \"#101010\"\n");
    assert_eq!(error.position.map(|p| p.line), Some(2));
    assert!(
        error
            .message
            .starts_with("unknown key `backdrop` in `[terminal]`")
    );
    let error = wrong("[terminal]\npalette = [\"#000000\"]\n");
    assert!(error.message.contains("not 1"), "{}", error.message);
    let error = wrong("name = \"x\"\n");
    assert!(error.message.contains("of no section"), "{}", error.message);
    let error = wrong("[theme]\nvariant = \"dim\"\n");
    assert_eq!(
        error.position,
        Some(Position {
            line: 2,
            column: 11
        })
    );
    let error = wrong("[tabs]\nbar.bg = 3\n");
    assert!(error.message.contains("is a string"), "{}", error.message);
    // A theme that gives no background, nor its base.
    let error = wrong("[interface]\n\"panel.bg\" = \"#000000\"\n");
    assert!(
        error.message.contains("gives no background"),
        "{}",
        error.message
    );
    assert!(wrong("[terminal\n").position.is_some());
}

#[test]
fn a_chain_that_loops_or_goes_too_deep_and_a_name_of_nothing_are_errors() {
    let directory = folder(
        "loop",
        &[
            ("A", "[theme]\nextends = \"B\"\n"),
            ("B", "[theme]\nextends = \"A\"\n"),
        ],
    );
    let error = resolve("A", Some(&directory), &[builtin()]).unwrap_err();
    assert!(
        error.message.contains("which extends it"),
        "{}",
        error.message
    );
    assert_eq!(error.path, directory.join("B.toml"));
    let chain: Vec<(String, String)> = (0..10)
        .map(|n| {
            (
                format!("T{n}"),
                format!("[theme]\nextends = \"T{}\"\n", n + 1),
            )
        })
        .collect();
    let files: Vec<(&str, &str)> = chain
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let directory = folder("deep", &files);
    let error = resolve("T0", Some(&directory), &[builtin()]).unwrap_err();
    assert!(
        error.message.contains("8 others at most"),
        "{}",
        error.message
    );
    let error = resolve("Nowhere", Some(&directory), &[builtin()]).unwrap_err();
    assert!(error.message.contains("there is no theme `Nowhere`"));
    for name in ["", "../config", ".hidden", "a/b"] {
        assert!(
            resolve(name, Some(&directory), &[builtin()]).is_err(),
            "{name}"
        );
    }
}

/// An `.itermcolors` file of grays, of a background of `background`, and of
/// the light and the dark appearance's backgrounds too with `pair`.
fn itermcolors(background: f64, pair: bool) -> String {
    let entry = |gray: f64| {
        format!(
            "<dict><key>Red Component</key><real>{gray}</real><key>Green Component</key>\
             <real>{gray}</real><key>Blue Component</key><real>{gray}</real>\
             <key>Color Space</key><string>sRGB</string></dict>"
        )
    };
    let mut body = String::new();
    for index in 0..16 {
        body.push_str(&format!(
            "<key>Ansi {index} Color</key>{}",
            entry(f64::from(index) / 15.0)
        ));
    }
    body.push_str(&format!("<key>Background Color</key>{}", entry(background)));
    body.push_str(&format!("<key>Foreground Color</key>{}", entry(0.9)));
    if pair {
        body.push_str(&format!(
            "<key>Background Color (Light)</key>{}",
            entry(1.0)
        ));
        body.push_str(&format!(
            "<key>Foreground Color (Light)</key>{}",
            entry(0.0)
        ));
    }
    format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict>{body}</dict></plist>")
}

#[test]
fn an_itermcolors_file_of_the_folder_is_a_theme_as_it_is_and_a_native_one_extends_it() {
    let directory = folder(
        "itermcolors",
        &[(
            "Mine",
            "[theme]\nextends = \"Grays\"\n\n[interface]\n\"panel.bg\" = \"#010203\"\n",
        )],
    );
    fs::write(directory.join("Grays.itermcolors"), itermcolors(0.0, false)).unwrap();
    fs::write(directory.join("Pair.itermcolors"), itermcolors(0.0, true)).unwrap();
    let grays = resolve("Grays", Some(&directory), &[builtin()]).unwrap();
    assert_eq!(grays.theme.terminal.background, ThemeColor::rgb(0, 0, 0));
    assert_eq!(
        grays.files,
        [
            directory.join("Grays.toml"),
            directory.join("Grays.itermcolors")
        ]
    );
    let mine = resolve("Mine", Some(&directory), &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(mine.terminal.palette[15], ThemeColor::rgb(255, 255, 255));
    assert_eq!(mine.tokens["panel.bg"], ThemeColor::hex(0x010203));
    // A file of both appearances is a theme of each, named.
    let light = resolve("Pair (Light)", Some(&directory), &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(light.terminal.background, ThemeColor::rgb(255, 255, 255));
    assert_eq!(light.variant, ThemeVariant::Light);
    let dark = resolve("Pair (Dark)", Some(&directory), &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(dark.terminal.background, ThemeColor::rgb(0, 0, 0));
    let error = resolve("Pair", Some(&directory), &[builtin()]).unwrap_err();
    assert!(
        error.message.contains("`Pair (Light)` and `Pair (Dark)`"),
        "{}",
        error.message
    );
    // A native file before an `.itermcolors` one of its name.
    let text =
        format!("[terminal]\nbackground = \"#123456\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n");
    fs::write(directory.join("Grays.toml"), text).unwrap();
    let native = resolve("Grays", Some(&directory), &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(native.terminal.background, ThemeColor::hex(0x123456));
}

#[test]
fn a_file_to_install_tells_its_themes_and_their_variants() {
    let text =
        format!("[terminal]\nbackground = \"#fafafa\"\nforeground = \"#1e1e20\"\n{PALETTE}\n");
    let directory = folder(
        "install",
        &[
            ("Paper", &text),
            ("Plain", "[theme]\nextends = \"Lokyr Dark\"\n"),
        ],
    );
    fs::write(directory.join("Pair.itermcolors"), itermcolors(0.0, true)).unwrap();
    fs::write(directory.join("notes.txt"), "").unwrap();
    assert_eq!(
        themes_of(&directory.join("Paper.toml")).unwrap(),
        [("Paper".to_owned(), ThemeVariant::Light)]
    );
    assert_eq!(
        themes_of(&directory.join("Plain.toml")).unwrap(),
        [("Plain".to_owned(), ThemeVariant::Dark)]
    );
    assert_eq!(
        themes_of(&directory.join("Pair.itermcolors")).unwrap(),
        [
            ("Pair (Light)".to_owned(), ThemeVariant::Light),
            ("Pair (Dark)".to_owned(), ThemeVariant::Dark)
        ]
    );
    let error = themes_of(&directory.join("notes.txt")).unwrap_err();
    assert!(
        error.message.contains(".toml or .itermcolors"),
        "{}",
        error.message
    );
    fs::write(
        directory.join("Broken.toml"),
        "[terminal]\nbackground = 1\n",
    )
    .unwrap();
    assert!(
        themes_of(&directory.join("Broken.toml"))
            .unwrap_err()
            .position
            .is_some()
    );
}

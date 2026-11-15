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
        .insert("panel.bg".to_owned(), ThemeColor::hex(0x2a2a2d).into());
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
    let resolved = resolve("Nightfall", Some(&directory), None, &[builtin()]).unwrap();
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
    let token = |name: &str| match theme.tokens.get(name) {
        Some(TokenValue::Color(color)) => Some(*color),
        _ => None,
    };
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
    let resolved = resolve("Dimmer", Some(&directory), None, &[builtin()]).unwrap();
    let theme = resolved.theme;
    // Its name its own, its background Dim's, the rest the built-in one's.
    assert_eq!(theme.name, "Dimmer");
    assert_eq!(theme.terminal.background, ThemeColor::hex(0x000000));
    assert_eq!(theme.terminal.foreground, ThemeColor::hex(0xe6e6e6));
    assert_eq!(theme.terminal.selection, Some(ThemeColor::hex(0x34445f)));
    assert_eq!(
        theme.tokens["panel.bg"],
        TokenValue::Color(ThemeColor::hex(0x050505))
    );
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
    let theme = resolve(DARK, Some(&directory), None, &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(theme.terminal.background, ThemeColor::hex(0xfafafa));
    assert_eq!(theme.variant, ThemeVariant::Light);
    assert!(theme.tokens.is_empty(), "the built-in one's tokens gone");
    // Without the folder, the built-in one.
    let theme = resolve(DARK, None, None, &[builtin()]).unwrap().theme;
    assert_eq!(theme, builtin());
}

#[test]
fn a_file_s_errors_tell_their_places() {
    let wrong = |text: &str| {
        let directory = folder("wrong", &[("Wrong", text)]);
        resolve("Wrong", Some(&directory), None, &[builtin()]).unwrap_err()
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
    let error = resolve("A", Some(&directory), None, &[builtin()]).unwrap_err();
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
    let error = resolve("T0", Some(&directory), None, &[builtin()]).unwrap_err();
    assert!(
        error.message.contains("8 others at most"),
        "{}",
        error.message
    );
    let error = resolve("Nowhere", Some(&directory), None, &[builtin()]).unwrap_err();
    assert!(error.message.contains("there is no theme `Nowhere`"));
    for name in ["", "../config", ".hidden", "a/b"] {
        assert!(
            resolve(name, Some(&directory), None, &[builtin()]).is_err(),
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
    let grays = resolve("Grays", Some(&directory), None, &[builtin()]).unwrap();
    assert_eq!(grays.theme.terminal.background, ThemeColor::rgb(0, 0, 0));
    assert_eq!(
        grays.files,
        [
            directory.join("Grays.toml"),
            directory.join("Grays.itermcolors")
        ]
    );
    let mine = resolve("Mine", Some(&directory), None, &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(mine.terminal.palette[15], ThemeColor::rgb(255, 255, 255));
    assert_eq!(
        mine.tokens["panel.bg"],
        TokenValue::Color(ThemeColor::hex(0x010203))
    );
    // A file of both appearances is a theme of each, named.
    let light = resolve("Pair (Light)", Some(&directory), None, &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(light.terminal.background, ThemeColor::rgb(255, 255, 255));
    assert_eq!(light.variant, ThemeVariant::Light);
    let dark = resolve("Pair (Dark)", Some(&directory), None, &[builtin()])
        .unwrap()
        .theme;
    assert_eq!(dark.terminal.background, ThemeColor::rgb(0, 0, 0));
    let error = resolve("Pair", Some(&directory), None, &[builtin()]).unwrap_err();
    assert!(
        error.message.contains("`Pair (Light)` and `Pair (Dark)`"),
        "{}",
        error.message
    );
    // The file of a theme, and every theme that it holds, its own first.
    assert_eq!(
        itermcolors_file(&directory, "Pair (Dark)"),
        Some((
            directory.join("Pair.itermcolors"),
            vec!["Pair (Dark)".to_owned(), "Pair (Light)".to_owned()]
        ))
    );
    assert_eq!(
        itermcolors_file(&directory, "Grays"),
        Some((
            directory.join("Grays.itermcolors"),
            vec!["Grays".to_owned()]
        ))
    );
    assert_eq!(itermcolors_file(&directory, "Pair"), None);
    assert_eq!(itermcolors_file(&directory, "Mine"), None);
    // A native file before an `.itermcolors` one of its name.
    let text =
        format!("[terminal]\nbackground = \"#123456\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n");
    fs::write(directory.join("Grays.toml"), text).unwrap();
    let native = resolve("Grays", Some(&directory), None, &[builtin()])
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

#[cfg(unix)]
#[test]
fn a_theme_s_file_is_a_regular_file_within_its_bound() {
    let directory = folder(
        "bounds",
        &[("Plain", "[theme]\nextends = \"Lokyr Dark\"\n")],
    );
    // A pipe of a theme's name refused at once, of no writer.
    let pipe = directory.join("Pipe.toml");
    let made = std::process::Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .unwrap();
    assert!(made.success());
    let started = std::time::Instant::now();
    let error = themes_of(&pipe).unwrap_err();
    assert!(
        error.message.contains("no regular file"),
        "{}",
        error.message
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    // A link to a device, and a folder.
    std::os::unix::fs::symlink("/dev/zero", directory.join("Zero.toml")).unwrap();
    let error = themes_of(&directory.join("Zero.toml")).unwrap_err();
    assert!(
        error.message.contains("no regular file"),
        "{}",
        error.message
    );
    fs::create_dir(directory.join("Folder.itermcolors")).unwrap();
    assert!(themes_of(&directory.join("Folder.itermcolors")).is_err());
    // Past 1 MiB, refused; the rest read.
    let large = format!(
        "[theme]\nextends = \"Lokyr Dark\"\n# {}\n",
        "x".repeat(usize::try_from(MOST_THEME_BYTES).unwrap())
    );
    fs::write(directory.join("Large.toml"), large).unwrap();
    let error = themes_of(&directory.join("Large.toml")).unwrap_err();
    assert!(
        error.message.contains("1024 KiB at most"),
        "{}",
        error.message
    );
    assert_eq!(
        themes_of(&directory.join("Plain.toml")).unwrap(),
        [("Plain".to_owned(), ThemeVariant::Dark)]
    );
    // A theme of a pipe's name resolves to none, as it does not wait.
    let error = resolve("Pipe", Some(&directory), None, &[]).unwrap_err();
    assert!(
        error.message.contains("no regular file"),
        "{}",
        error.message
    );
    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn a_theme_s_tokens_of_expressions_read_and_what_colors_nothing_is_told_at_its_line() {
    let text = format!(
        r##"[theme]
name = "Told"

[terminal]
background = "#101012"
foreground = "#e0e0e0"
{PALETTE}

[interface]
"accent" = "#c678dd"
"focus.ring" = "accent / 40%"
"separator" = "nothing / 10%"

[tabs]
active.bg = "mix(background, foreground, 8%)"
actve.bg = "#ff0000"
hover.bg = "mix(text"

[button]
bg = "button.hover.bg"
hover.bg = "button.bg"

[command-header]
pill = "panel.bg"
"##
    );
    let directory = folder("told", &[("Told", &text)]);
    let resolved = resolve("Told", Some(&directory), None, &[]).unwrap();
    let theme = &resolved.theme;
    let expr = |text: &str| TokenValue::Expr(text.to_owned());
    assert_eq!(theme.tokens["focus.ring"], expr("accent / 40%"));
    assert_eq!(
        theme.tokens["tabs.active.bg"],
        expr("mix(background, foreground, 8%)")
    );
    // A plugin's section, of its own names, as it is.
    assert_eq!(theme.tokens["command-header.pill"], expr("panel.bg"));
    // What colors nothing, each told at its line, and left out but a name
    // that Lokyr does not know; the rest in force.
    let told: Vec<(usize, &str)> = resolved
        .notices
        .iter()
        .map(|notice| (notice.position.unwrap().line, notice.message.as_str()))
        .collect();
    let line = |line: usize, part: &str| {
        assert!(
            told.iter()
                .any(|(at, message)| *at == line && message.contains(part)),
            "{line} {part}: {told:?}"
        );
    };
    line(13, "`nothing` names no color");
    line(17, "`tabs.actve.bg` is no token of Lokyr's");
    line(18, "`tabs.hover.bg` is left out");
    line(22, "tokens name each other");
    assert_eq!(told.len(), 4, "{told:?}");
    assert!(
        resolved
            .notices
            .iter()
            .all(|notice| notice.path == directory.join("Told.toml"))
    );
    assert!(!theme.tokens.contains_key("separator"));
    assert!(!theme.tokens.contains_key("tabs.hover.bg"));
    assert!(!theme.tokens.contains_key("button.hover.bg"));
    assert_eq!(theme.tokens["button.bg"], expr("button.hover.bg"));
    assert!(theme.tokens.contains_key("tabs.actve.bg"));
    // A theme that extends it tells its notices too, of its file.
    let child = "[theme]\nextends = \"Told\"\n\n[interface]\n\"pane.divider\" = \"zz\"\n\"pane.zz\" = \"#000000\"\n";
    fs::write(directory.join("Child.toml"), child).unwrap();
    let resolved = resolve("Child", Some(&directory), None, &[]).unwrap();
    assert_eq!(resolved.notices.len(), 6, "{:?}", resolved.notices);
    assert!(
        resolved
            .notices
            .iter()
            .any(|notice| notice.message.contains("`pane.zz` is no token")),
        "{:?}",
        resolved.notices
    );
    let last = resolved
        .notices
        .iter()
        .find(|notice| notice.message.contains("`zz`"))
        .unwrap();
    assert_eq!(last.path, directory.join("Child.toml"));
    assert_eq!(last.position.map(|at| at.line), Some(5));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_theme_written_in_the_native_format_reads_back_the_same() {
    let mut theme = builtin();
    theme.name = "Ada's \"Night\" \\ Day".to_owned();
    theme.terminal.palette[3] = ThemeColor::hex(0x123456);
    theme.terminal.cursor = Some(ThemeColor::hex(0xffcc00));
    theme.terminal.link = Some(ThemeColor {
        alpha: 0x80,
        ..ThemeColor::hex(0x3366ff)
    });
    theme
        .tokens
        .insert("search.match".to_owned(), ThemeColor::hex(0xffff00).into());
    theme.tokens.insert(
        "search.match.current".to_owned(),
        ThemeColor::hex(0xff9900).into(),
    );
    theme
        .tokens
        .insert("tabs.bar.bg".to_owned(), ThemeColor::hex(0x101010).into());
    theme.tokens.insert(
        "focus.ring".to_owned(),
        TokenValue::Expr("accent / 40%".to_owned()),
    );
    let credits = Credits {
        author: Some("Ada"),
        license: Some("MIT OR Apache-2.0"),
        source: Some("https://example.com/night\nhttps://example.com/day"),
    };
    let text = native(&theme, credits);
    assert!(text.contains("license = \"MIT OR Apache-2.0\""), "{text}");
    assert!(
        text.contains("source = \"https://example.com/night\\nhttps://example.com/day\""),
        "{text}"
    );
    let directory = folder("native", &[("Round", &text)]);
    let read = resolve("Round", Some(&directory), None, &[]).unwrap().theme;
    assert_eq!(
        read,
        Theme {
            name: "Round".to_owned(),
            ..theme
        }
    );
    let _ = fs::remove_dir_all(directory);
}

/// A theme's file of `background`, of a light foreground.
fn theme_file(background: &str) -> String {
    format!("[terminal]\nbackground = \"{background}\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n")
}

#[test]
fn a_held_theme_comes_after_the_folder_s_and_before_the_built_in_ones() {
    let held = folder(
        "held",
        &[
            ("Paper", &theme_file("#fafafa")),
            (DARK, &theme_file("#202020")),
        ],
    );
    let directory = folder("held-folder", &[]);
    // Of the held folder, before the built-in theme of its name.
    let paper = resolve("Paper", Some(&directory), Some(&held), &[builtin()]).unwrap();
    assert_eq!(paper.theme.terminal.background, ThemeColor::hex(0xfafafa));
    let dark = resolve(DARK, Some(&directory), Some(&held), &[builtin()]).unwrap();
    assert_eq!(dark.theme.terminal.background, ThemeColor::hex(0x202020));
    // Its files are not watched, as they never change.
    assert!(
        paper.files.iter().all(|file| file.starts_with(&directory)),
        "{:?}",
        paper.files
    );
    // A file of the folder of its name takes its place, and cannot extend
    // it; one of another name can.
    fs::write(
        directory.join("Paper.toml"),
        "[theme]\nextends = \"Paper\"\n[terminal]\nforeground = \"#111111\"\n",
    )
    .unwrap();
    let error = resolve("Paper", Some(&directory), Some(&held), &[builtin()]).unwrap_err();
    assert!(error.message.contains("extends this theme"), "{error}");
    fs::remove_file(directory.join("Paper.toml")).unwrap();
    fs::write(
        directory.join("Mine.toml"),
        "[theme]\nextends = \"Paper\"\n",
    )
    .unwrap();
    let mine = resolve("Mine", Some(&directory), Some(&held), &[builtin()]).unwrap();
    assert_eq!(mine.theme.terminal.background, ThemeColor::hex(0xfafafa));
    let _ = fs::remove_dir_all(held);
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_theme_s_credits_come_of_its_file_or_of_those_that_it_extends_and_its_copy_is_whole() {
    let held = folder(
        "credits-held",
        &[(
            "Harbor",
            &format!(
                "[theme]\nauthor = \"Ada\"\nlicense = \"MIT\"\nsource = \"https://example.com\"\n[search]\nmatch = \"#ffd60a4d\"\n{}",
                theme_file("#101820")
            ),
        )],
    );
    let directory = folder(
        "credits-folder",
        &[(
            "Mine",
            "[theme]\nextends = \"Harbor\"\nauthor = \"Grace\"\n[terminal]\nforeground = \"#fefefe\"\n",
        )],
    );
    let resolved = |name: &str| resolve(name, Some(&directory), Some(&held), &[builtin()]).unwrap();
    let harbor = resolved("Harbor");
    assert_eq!(harbor.credits.author.as_deref(), Some("Ada"));
    assert_eq!(harbor.credits.license.as_deref(), Some("MIT"));
    assert_eq!(
        harbor.credits.source.as_deref(),
        Some("https://example.com")
    );
    // Its own author, the license and the source of the theme that it
    // extends, as its colors.
    let mine = resolved("Mine");
    assert_eq!(mine.credits.author.as_deref(), Some("Grace"));
    assert_eq!(mine.credits.license.as_deref(), Some("MIT"));
    assert_eq!(resolved(DARK).credits, ThemeCredits::default());
    // A copy reads back as the same theme of its new name, whole, of no
    // theme to extend, its credits kept.
    let text = copy_of(&mine, "Mine Copy");
    let other = folder("credits-copy", &[("Mine Copy", &text)]);
    let copy = resolve("Mine Copy", Some(&other), None, &[]).unwrap();
    assert_eq!(copy.theme.name, "Mine Copy");
    assert_eq!(copy.theme.terminal, mine.theme.terminal);
    assert_eq!(copy.theme.tokens, mine.theme.tokens);
    assert_eq!(copy.theme.variant, mine.theme.variant);
    assert_eq!(copy.credits, mine.credits);
    assert!(text.contains("name = \"Mine Copy\""), "{text}");
    assert!(!text.contains("extends"), "{text}");
    // The tokens that it leaves out, comments of their rules.
    for token in lokyr_plugin_sdk::theme::TOKENS {
        let given = mine.theme.tokens.contains_key(token);
        assert_eq!(
            text.contains(&format!("# \"{token}\" = ")),
            !given,
            "{token}"
        );
    }
    for folder in [held, directory, other] {
        let _ = fs::remove_dir_all(folder);
    }
}

#[test]
fn a_new_theme_of_a_built_in_one_writes_every_key_its_optional_ones_as_comments() {
    let mut base = builtin();
    base.tokens
        .insert("search.match".to_owned(), ThemeColor::hex(0xffd60a).into());
    let text = new_theme_of(&base, "New Dark Theme");
    // The optional keys that it leaves out, commented, of their values
    // then, in `[terminal]`, before the next section.
    for line in [
        "# cursor = \"#e6e6e6\"",
        "# cursor-text = \"#1e1e20\"",
        "# selection-text = \"#e6e6e6\"",
        "# bold = \"#e6e6e6\"",
        "# link = \"#45454b\"",
        "# underline = \"#e6e6e6\"",
    ] {
        assert!(text.contains(line), "{line} in {text}");
    }
    let terminal = text.find("[terminal]").unwrap();
    let search = text.find("[search]").unwrap();
    let cursor = text.find("# cursor = ").unwrap();
    assert!(terminal < cursor && cursor < search, "{text}");
    assert!(text.contains("selection = \"#34445f\"\n"), "{text}");
    assert!(!text.contains("# selection = "), "{text}");
    // It reads back as the same theme, of its new name, whole.
    let folder = folder("new-theme", &[("New Dark Theme", &text)]);
    let read = resolve("New Dark Theme", Some(&folder), None, &[])
        .unwrap()
        .theme;
    assert_eq!(read.name, "New Dark Theme");
    assert_eq!(read.terminal, base.terminal);
    assert_eq!(read.tokens, base.tokens);
    // Every token that it leaves out, a comment of its rule in
    // `[interface]`, after those that it gives.
    for token in lokyr_plugin_sdk::theme::TOKENS {
        let given = base.tokens.contains_key(token);
        let commented = text.contains(&format!("# \"{token}\" = \""));
        assert!(given != commented, "{token}: {text}");
    }
    let interface = text.find("[interface]").unwrap();
    assert!(text.find("# \"text\" = \"foreground\"").unwrap() > interface);
    // Uncommented, the rules color as the comments did.
    let uncommented = text.replace("# \"", "\"");
    let open = super::tests::folder("new-theme-open", &[("Open", &uncommented)]);
    let opened = resolve("Open", Some(&open), None, &[]).unwrap();
    assert!(opened.notices.is_empty(), "{:?}", opened.notices);
    assert_eq!(
        rules::resolve(&opened.theme.terminal, &opened.theme.tokens),
        rules::resolve(&base.terminal, &base.tokens)
    );
    // Of no token, every token commented in an `[interface]` of its own,
    // after the terminal's optional keys.
    let mut bare = builtin();
    bare.tokens.clear();
    let text = new_theme_of(&bare, "Bare");
    assert!(text.find("the text's when left out").unwrap() < text.find("\n[interface]\n").unwrap());
    assert!(
        text.trim_end()
            .ends_with("\"cursor.guide\" = \"foreground / 10%\""),
        "{text}"
    );
    let _ = fs::remove_dir_all(folder);
    let _ = fs::remove_dir_all(open);
}

#[test]
fn the_guide_s_example_theme_holds_every_key_and_token() {
    let text = include_str!("../../../docs/themes/example.toml");
    let file = parse(text, Path::new("docs/themes/example.toml")).unwrap();
    let partial = &file.partial;
    // Every key of `[theme]`, those that tell people too, which the
    // parser keeps not.
    let document: toml_edit::DocumentMut = text.parse().unwrap();
    let theme = document["theme"].as_table().unwrap();
    for key in THEME_KEYS {
        assert!(theme.contains_key(key), "[theme] {key}");
    }
    assert_eq!(partial.variant, Some(ThemeVariant::Dark));
    assert_eq!(
        file.extends.as_ref().map(|(name, _)| name.as_str()),
        Some(DARK)
    );
    // Every color of `[terminal]`, and its palette.
    let terminal = document["terminal"].as_table().unwrap();
    for key in TERMINAL_COLORS.iter().chain(&["palette"]) {
        assert!(terminal.contains_key(key), "[terminal] {key}");
    }
    assert!(partial.palette.is_some());
    // The search's, and every token of the host's.
    for token in ["search.match", "search.match.current"]
        .into_iter()
        .chain(lokyr_plugin_sdk::theme::TOKENS)
    {
        assert!(partial.tokens.contains_key(token), "the token {token}");
    }
    // A plugin's, of a section of its own name.
    assert!(partial.tokens.contains_key("command-header.pill"));
    // And it resolves whole, of the theme that it extends, of nothing
    // told, as Lokyr Dark colors, its colors of other colors too.
    let dark = lokyr_plugin_sdk::theme::builtin::dark();
    let resolved = resolve(
        "Example",
        Some(&folder("example", &[("Example", text)])),
        None,
        std::slice::from_ref(&dark),
    )
    .unwrap();
    assert!(resolved.notices.is_empty(), "{:?}", resolved.notices);
    let example = rules::resolve(&resolved.theme.terminal, &resolved.theme.tokens);
    let built_in = rules::resolve(&dark.terminal, &dark.tokens);
    for (token, color) in &built_in {
        assert_eq!(example.get(token), Some(color), "{token}");
    }
}

#[test]
fn the_guide_s_smallest_theme_and_its_extension_resolve() {
    let guide = include_str!("../../../docs/themes/authoring.md");
    // The guide's blocks of TOML that make a theme, by their names.
    let blocks: Vec<&str> = guide
        .split("```toml\n")
        .skip(1)
        .filter_map(|rest| rest.split("```").next())
        .filter(|block| block.starts_with("[theme]\nname = "))
        .collect();
    let named = |name: &str| {
        blocks
            .iter()
            .find(|block| block.starts_with(&format!("[theme]\nname = \"{name}\"\n")))
            .copied()
            .unwrap_or_else(|| panic!("the guide's theme `{name}`"))
    };
    let directory = folder(
        "guide",
        &[
            ("Harbor", named("Harbor")),
            ("Harbor Bright", named("Harbor Bright")),
        ],
    );
    let harbor = resolve("Harbor", Some(&directory), None, &[])
        .unwrap()
        .theme;
    assert_eq!(harbor.variant, ThemeVariant::Dark);
    let bright = resolve("Harbor Bright", Some(&directory), None, &[])
        .unwrap()
        .theme;
    assert_eq!(bright.terminal.background, ThemeColor::hex(0x0b0e13));
    assert_eq!(bright.terminal.palette, harbor.terminal.palette);
}

#[test]
fn each_token_given_tells_its_source_its_file_and_line_or_its_theme() {
    let base = format!(
        "[theme]\nname = \"Base\"\n\n[terminal]\nbackground = \"#101012\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n\n[interface]\n\"accent\" = \"#c678dd\"\n\"panel.bg\" = \"#202024\"\n"
    );
    let child = "[theme]\nextends = \"Base\"\n\n[interface]\n\"panel.bg\" = \"#303034\"\n";
    let directory = folder("sources", &[("Base", &base), ("Child", child)]);
    let resolved = resolve("Child", Some(&directory), None, &[builtin()]).unwrap();
    let line = |token: &str| match &resolved.sources[token] {
        Source::File(path, position) => (
            path.file_name().unwrap().to_str().unwrap().to_owned(),
            position.map(|at| at.line),
        ),
        Source::Theme(name) => (name.clone(), None),
    };
    assert_eq!(line("panel.bg"), ("Child.toml".to_owned(), Some(5)));
    assert_eq!(line("accent"), ("Base.toml".to_owned(), Some(11)));
    assert_eq!(resolved.sources.len(), 2);
    // The terminal's colors of the base's file, each of its line.
    let terminal = |key: &str| match &resolved.terminal_sources[key] {
        Source::File(path, position) => (
            path.file_name().unwrap().to_str().unwrap().to_owned(),
            position.map(|at| at.line),
        ),
        Source::Theme(name) => (name.clone(), None),
    };
    assert_eq!(terminal("background"), ("Base.toml".to_owned(), Some(5)));
    assert_eq!(terminal("palette.0"), ("Base.toml".to_owned(), Some(7)));
    assert_eq!(terminal("palette.15"), ("Base.toml".to_owned(), Some(8)));
    // A built-in theme's own tokens, of its name.
    let mut built_in = builtin();
    built_in.tokens.insert(
        "accent".to_owned(),
        TokenValue::Color(ThemeColor::hex(0x123456)),
    );
    let name = built_in.name.clone();
    let resolved = resolve(&name, None, None, &[built_in]).unwrap();
    assert_eq!(resolved.sources["accent"], Source::Theme(name));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn a_value_set_or_taken_away_keeps_the_file_s_comments_order_and_other_keys() {
    let path = Path::new("/themes/Mine.toml");
    let text = format!(
        "# Mine.\n[theme]\nname = \"Mine\"\n\n[terminal]\nbackground = \"#101010\"   # the default background\nforeground = \"#e0e0e0\"\n{PALETTE}\n\n[search]\nmatch = \"#ffd60a4d\"\n\n[button]\nhover.bg = \"#ffffff1f\"   # hovered\n\n[interface]\n\"accent\" = \"#4c9bff\"\n# Every other token, of Lokyr's rule: a line uncommented gives it.\n# \"focus.ring\" = \"accent / 60%\"\n"
    );
    let theme = builtin();
    let set = |text: &str, name: &str, value: Option<&str>| {
        set_value(text, path, name, value, &theme).unwrap()
    };
    // A key of `[terminal]`, its comment kept; one new; one taken away.
    let edited = set(&text, "background", Some("#202020"));
    assert!(
        edited.contains("background = \"#202020\"   # the default background\n"),
        "{edited}"
    );
    assert!(edited.starts_with("# Mine.\n[theme]\n"), "{edited}");
    let edited = set(&edited, "cursor", Some("#ff0000"));
    assert!(edited.contains("cursor = \"#ff0000\""), "{edited}");
    let edited = set(&edited, "cursor", None);
    assert!(!edited.contains("cursor ="), "{edited}");
    // A palette's color in its place, the others as they are.
    let edited = set(&text, "bright-white", Some("#fefefe"));
    assert!(edited.contains("\"#003333\", \"#fefefe\"]"), "{edited}");
    // A token where the file writes it: its section, `[search]`,
    // `[interface]`.
    let edited = set(&text, "button.hover.bg", Some("accent / 20%"));
    assert!(
        edited.contains("hover.bg = \"accent / 20%\"   # hovered\n"),
        "{edited}"
    );
    let edited = set(&edited, "search.match", Some("#ff000080"));
    assert!(edited.contains("match = \"#ff000080\""), "{edited}");
    let edited = set(&edited, "accent", Some("#00ff00"));
    assert!(edited.contains("\"accent\" = \"#00ff00\"\n"), "{edited}");
    // A new one in `[interface]`, its rule's comment gone.
    let edited = set(&edited, "focus.ring", Some("accent / 40%"));
    assert!(
        edited.contains("\"focus.ring\" = \"accent / 40%\""),
        "{edited}"
    );
    let edited = set(&edited, "separator", Some("#ffffff1a"));
    assert!(
        edited.contains("\n\"separator\" = \"#ffffff1a\"\n"),
        "{edited}"
    );
    assert!(!edited.contains("# \"focus.ring\""), "{edited}");
    let read = parse(&edited, path).unwrap().partial;
    assert_eq!(
        read.tokens["focus.ring"],
        TokenValue::Expr("accent / 40%".to_owned())
    );
    assert_eq!(
        read.tokens["button.hover.bg"],
        TokenValue::Expr("accent / 20%".to_owned())
    );
    // Taken away: of its section, the table that it empties too.
    let edited = set(&edited, "button.hover.bg", None);
    assert!(!edited.contains("hover"), "{edited}");
    let edited = set(&edited, "focus.ring", None);
    assert!(!edited.contains("focus.ring"), "{edited}");
    // A file of no `[interface]` nor palette: both made.
    let bare = "[theme]\nextends = \"Lokyr Dark\"\n";
    let edited = set(bare, "text.secondary", Some("#999999"));
    assert!(
        edited.ends_with("[interface]\n\"text.secondary\" = \"#999999\"\n"),
        "{edited}"
    );
    let edited = set(bare, "red", Some("#ff0000"));
    let read = parse(&edited, path).unwrap().partial;
    let palette = read.palette.unwrap();
    assert_eq!(palette[1], ThemeColor::hex(0xff0000));
    assert_eq!(palette[0], theme.terminal.palette[0]);
    // Refused: a color that does not read, an expression that does not
    // read, a palette's color taken away alone.
    for (name, value) in [
        ("background", Some("accent")),
        ("accent", Some("mix(")),
        ("red", None),
    ] {
        let error = set_value(&text, path, name, value, &theme).unwrap_err();
        assert_eq!(error.path, path, "{name}");
    }
}

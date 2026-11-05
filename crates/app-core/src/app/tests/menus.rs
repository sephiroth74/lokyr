//! The tests of the menu bar and of the context menu's commands.

use super::*;

#[test]
fn starting_opens_a_window_under_the_menus_of_the_commands() {
    let (fake, _) = started();
    let windows: Vec<_> = fake.windows().collect();
    assert_eq!(windows.len(), 1);
    // Of 100 columns of 4 points and 30 rows of 8, with 4 of padding at
    // each edge.
    assert_eq!(
        fake.window(windows[0]).unwrap().size.points,
        Size {
            width: 408.0,
            height: 248.0
        }
    );
    let menu = fake.main_menu().unwrap();
    let titles: Vec<&str> = menu.menus.iter().map(|menu| menu.title.as_str()).collect();
    assert_eq!(titles, ["Lokyr", "Shell", "Edit", "View", "Window", "Help"]);
    let roles: Vec<Option<MenuRole>> = menu.menus.iter().map(|menu| menu.role).collect();
    assert_eq!(
        roles,
        [
            None,
            None,
            None,
            None,
            Some(MenuRole::Window),
            Some(MenuRole::Help)
        ]
    );
    assert_eq!(
        menu_items(&fake, 0),
        [
            "About Lokyr",
            "Check for Updates…",
            "-",
            "Settings… cmd+,",
            "Secure Keyboard Entry",
            "-",
            "Hide Lokyr cmd+h",
            "Hide Others alt+cmd+h",
            "Show All",
            "-",
            "Quit Lokyr cmd+q"
        ]
    );
    assert_eq!(
        menu_items(&fake, 1),
        [
            "New Window cmd+n",
            "Close Window cmd+w",
            "Set Title… shift+cmd+i",
            "Quick Terminal"
        ]
    );
    assert_eq!(
        menu_items(&fake, 2),
        [
            "Copy cmd+c",
            "Paste cmd+v",
            "-",
            "Select All cmd+a",
            "Clear cmd+k",
            "-",
            "Emoji & Symbols ctrl+cmd+ "
        ]
    );
    // The last does the system's own, which opens its palette.
    assert!(matches!(
        menu.menus[2].items.last(),
        Some(MenuItem::Standard {
            action: lokyr_platform_core::StandardAction::CharacterPalette,
            ..
        })
    ));
    // The commands that take an argument that they need have no item,
    // as `links.open` and `pane.send-text`.
    assert_eq!(
        menu_items(&fake, 3),
        [
            "Bigger cmd+=",
            "Smaller cmd+-",
            "Default Size cmd+0",
            "-",
            "Scroll Up a Line cmd+ArrowUp",
            "Scroll Down a Line cmd+ArrowDown",
            "Scroll Up a Page cmd+PageUp",
            "Scroll Down a Page cmd+PageDown",
            "Scroll to the Top cmd+Home",
            "Scroll to the Bottom cmd+End",
            "-",
            "Appearance",
            "-",
            "Show Performance"
        ]
    );
    assert_eq!(
        menu_items(&fake, 4),
        ["Minimize cmd+m", "Zoom", "-", "Bring All to Front"]
    );
    assert_eq!(menu_items(&fake, 5), ["Report an Issue…"]);
}

#[test]
fn the_menus_follow_the_keys_of_the_config() {
    let keys = concat!(
        "\"cmd+n\" = false\n",
        "\"cmd+y\" = \"window.new\"\n",
        "\"cmd+j\" = { command = \"pane.clear\", when = \"selection\" }\n",
        "\"cmd+w\" = \"pane.select-all\"\n",
    );
    let (fake, _, _) = bound(keys, reader(""));
    // A binding's key, the one that a key bound later takes away, whose
    // command's next key shows, and one that holds in a context alone,
    // which no item shows.
    assert_eq!(
        menu_items(&fake, 1),
        [
            "New Window cmd+y",
            "Close Window shift+cmd+w",
            "Set Title… shift+cmd+i",
            "Quick Terminal"
        ]
    );
    assert_eq!(
        menu_items(&fake, 2),
        [
            "Copy cmd+c",
            "Paste cmd+v",
            "-",
            "Select All cmd+a",
            "Clear cmd+k",
            "-",
            "Emoji & Symbols ctrl+cmd+ "
        ]
    );
}

#[test]
fn the_menus_follow_a_plugin_s_binding_and_a_reloaded_config() {
    let (mut fake, mut app, _, _) = with_tester();
    assert_eq!(menu_items(&fake, 1)[0], "New Window cmd+n");
    app.run(&mut fake, "test.bind", Args::new());
    assert_eq!(menu_items(&fake, 1)[0], "New Window");
    assert!(menu_items(&fake, 3).contains(&"test.size cmd+n".to_owned()));
    // A config that takes Command-W away, as it reloads, and then its
    // second key.
    let (mut fake, mut app, path) = with_file("menu-keys", "");
    assert_eq!(menu_items(&fake, 1)[1], "Close Window cmd+w");
    std::fs::write(&path, "[keys]\n\"cmd+w\" = false\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(menu_items(&fake, 1)[1], "Close Window shift+cmd+w");
    std::fs::write(
        &path,
        "[keys]\n\"cmd+w\" = false\n\"cmd+shift+w\" = false\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(menu_items(&fake, 1)[1], "Close Window");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn without_the_system_s_keymap_the_menus_keep_their_own_keys_alone() {
    let (mut fake, mut app, path) =
        with_file("no-keymap", "[plugins.keymap-macos]\nenabled = false\n");
    assert!(app.keymap.bindings().is_empty());
    let keyed = |fake: &Fake| -> Vec<String> {
        (0..6)
            .flat_map(|menu| menu_items(fake, menu))
            .filter(|item| item.contains("cmd+"))
            .collect()
    };
    assert_eq!(
        keyed(&fake),
        [
            "Hide Lokyr cmd+h",
            "Hide Others alt+cmd+h",
            "Emoji & Symbols ctrl+cmd+ ",
            "Minimize cmd+m"
        ]
    );
    // Command-N reaches the program, rather than opening a window.
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, command_key(KeyCode::KeyN, "n"));
    assert_eq!(fake.windows().count(), 1);
    // On again, as the config is saved.
    std::fs::write(&path, "").unwrap();
    change(&mut fake, &mut app);
    let (normal, _) = started();
    assert_eq!(keyed(&fake), keyed(&normal));
    assert_eq!(menu_items(&fake, 1)[0], "New Window cmd+n");
    fake.send(&mut app, window, command_key(KeyCode::KeyN, "n"));
    assert_eq!(fake.windows().count(), 2);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn the_built_in_plugins_bind_the_commands_that_are_there_alone() {
    let (_, app) = started();
    let missing: Vec<&str> = app
        .plugins
        .bindings()
        .map(|binding| binding.invocation.command.as_str())
        .filter(|command| app.commands.get(command).is_none())
        .collect();
    assert_eq!(missing, Vec::<&str>::new());
    assert_eq!(app.keymap.bindings().len(), 24);
}

#[test]
fn copy_s_item_is_enabled_with_a_selection_alone() {
    let (mut fake, mut app, window) = focused_with(reader(""));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!fake.command_enabled("pane.copy"));
    assert!(fake.command_enabled("pane.paste"));
    fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(fake.command_enabled("pane.copy"));
    // A click takes the selection away.
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, cell(2, 1), none, 1));
    fake.send(&mut app, window, click(false, cell(2, 1), none, 1));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!fake.command_enabled("pane.copy"));
}

#[test]
fn the_app_s_commands_register_with_their_categories_and_titles() {
    let registry = commands(&Texts::new("en", &CATALOGS), &Plugins::new());
    let registered: Vec<(&str, &str, Category)> = registry
        .commands()
        .map(|command| {
            (
                command.name.as_str(),
                command.title.as_str(),
                command.category,
            )
        })
        .collect();
    assert_eq!(
        registered,
        [
            ("app.about", "About Lokyr", Category::App),
            ("app.appearance-dark", "Dark Appearance", Category::View),
            ("app.appearance-light", "Light Appearance", Category::View),
            (
                "app.appearance-system",
                "Automatic Appearance",
                Category::View
            ),
            ("app.check-for-updates", "Check for Updates…", Category::App),
            ("app.performance", "Show Performance", Category::View),
            ("app.quit", "Quit Lokyr", Category::App),
            ("app.report-issue", "Report an Issue…", Category::App),
            (
                "app.secure-keyboard-entry",
                "Secure Keyboard Entry",
                Category::App
            ),
            ("app.settings", "Settings…", Category::App),
            ("pane.look-up", "Look Up", Category::Edit),
            ("pane.man-page", "Open man Page", Category::Windows),
            ("window.close", "Close Window", Category::Windows),
            ("window.new", "New Window", Category::Windows),
            ("window.set-title", "Set Title…", Category::Windows),
            (
                "window.toggle-quick-terminal",
                "Quick Terminal",
                Category::Windows
            ),
        ]
    );
    let texts = Texts::new("en", &CATALOGS);
    let menu = main_menu(&registry, &Keymap::default(), &texts);
    let titles: Vec<&str> = menu.menus[1]
        .items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Command { title, .. } => Some(title.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        titles,
        ["New Window", "Close Window", "Set Title…", "Quick Terminal"]
    );
    // A command that takes no argument rejects one, and does nothing.
    let mut args = Args::new();
    args.insert(
        "now".to_owned(),
        lokyr_kernel::commands::Value::Boolean(true),
    );
    let error = registry.check(QUIT, args.clone()).unwrap_err();
    assert_eq!(error.code.number(), 102);
    let (mut fake, mut app, _) = started_running(idle());
    app.run(&mut fake, QUIT, args);
    assert!(!fake.has_quit());
    app.run(&mut fake, QUIT, Args::new());
    assert!(fake.has_quit());
}

#[test]
fn the_menus_take_the_config_s_language_and_change_with_it() {
    let (mut fake, mut app, path) = with_file("language", "[app]\nlanguage = \"it\"\n");
    assert_eq!(
        menu_titles(&fake),
        [
            "Nuova finestra",
            "Nascondi Lokyr",
            "Esci da Lokyr",
            "Composizione"
        ]
    );
    assert_eq!(
        menu_items(&fake, 0)[..4],
        [
            "Informazioni su Lokyr",
            "Controlla aggiornamenti…",
            "-",
            "Impostazioni… cmd+,"
        ]
    );
    // The plugins' commands too, from their own catalogs.
    assert_eq!(
        menu_items(&fake, 2),
        [
            "Copia cmd+c",
            "Incolla cmd+v",
            "-",
            "Seleziona tutto cmd+a",
            "Cancella cmd+k",
            "-",
            "Emoji e simboli ctrl+cmd+ "
        ]
    );
    assert_eq!(menu_items(&fake, 3)[0], "Aumenta cmd+=");
    std::fs::write(&path, "[app]\nlanguage = \"en\"\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(
        menu_titles(&fake),
        ["New Window", "Hide Lokyr", "Quit Lokyr", "Edit"]
    );
    assert_eq!(menu_items(&fake, 2)[0], "Copy cmd+c");
    assert_eq!(app.texts.language(), "en");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn the_appearance_s_menu_is_in_the_config_s_language() {
    let (fake, _app, path) = with_file("appearance-it", "[app]\nlanguage = \"it\"\n");
    let view = &fake.main_menu().unwrap().menus[3];
    let Some(menu) = view.items.iter().find_map(|item| match item {
        MenuItem::Submenu(menu) => Some(menu),
        _ => None,
    }) else {
        panic!("{view:?}");
    };
    assert_eq!(menu.title, "Aspetto");
    assert_eq!(
        appearances(&fake),
        [
            ("Automatico".to_owned(), true),
            ("Chiaro".to_owned(), false),
            ("Scuro".to_owned(), false)
        ]
    );
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_right_click_selects_and_opens_the_context_menu_at_the_pointer() {
    let (mut fake, mut app, window) = selecting("", "");
    let none = Modifiers::NONE;
    // On a word, which it selects; its program has no link there.
    right_click(&mut fake, &mut app, window, cell(8, 0), none);
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    let (position, items, dimmed) = context_menu(&fake, window);
    assert_eq!(position, cell(8, 0));
    assert_eq!(
        items,
        [
            "Open Link",
            "Open man Page for “world”",
            "-",
            "Look Up “world”",
            "-",
            "Copy",
            "Paste",
            "-",
            "Select All",
            "Clear"
        ]
    );
    assert_eq!(dimmed, ["links.open"]);
    // Within the selection, which stays.
    select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(3, 0)), none);
    right_click(&mut fake, &mut app, window, cell(1, 0), none);
    assert_eq!(selected_text(&app, window).as_deref(), Some("hel"));
    assert_eq!(context_menu(&fake, window).1[3], "Look Up “hel”");
    assert_eq!(context_menu(&fake, window).2, ["links.open"]);
    // On a blank cell, which takes it away, and dims Copy and Look Up.
    right_click(&mut fake, &mut app, window, cell(5, 0), none);
    assert_eq!(selected_text(&app, window), None);
    assert!(app.windows[&window].selecting.is_none());
    assert_eq!(
        context_menu(&fake, window).1[1..4],
        ["Open man Page", "-", "Look Up"]
    );
    assert_eq!(
        context_menu(&fake, window).2,
        ["links.open", "pane.man-page", "pane.look-up", "pane.copy"]
    );
    // A Control click opens it too, and starts no selection that the
    // pointer would drag.
    let control = Modifiers {
        control: true,
        ..none
    };
    fake.send(&mut app, window, click(true, cell(2, 1), control, 1));
    fake.send(&mut app, window, moved(cell(9, 1), control));
    fake.send(&mut app, window, click(false, cell(9, 1), control, 1));
    assert_eq!(selected_text(&app, window).as_deref(), Some("second"));
    assert_eq!(context_menu(&fake, window).0, cell(2, 1));
    assert_eq!(fake.context_menus().len(), 4);
    // Its items run in its window's pane, which has no focus.
    assert_eq!(fake.focused_window(), None);
    let chosen = |name: &str| WindowEvent::ContextCommand(name.to_owned());
    fake.send(&mut app, window, chosen("pane.copy"));
    assert_eq!(fake.clipboard_text().as_deref(), Some("second"));
    fake.send(&mut app, window, chosen("pane.select-all"));
    assert!(
        selected_text(&app, window).is_some_and(|text| text.starts_with("hello world\n")),
        "{:?}",
        selected_text(&app, window)
    );
}

#[test]
fn look_up_shows_the_definition_of_the_selection_of_one_line() {
    let (mut fake, mut app, window) = selecting("", "");
    let none = Modifiers::NONE;
    let look_up = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::ContextCommand(LOOK_UP.to_owned()));
    };
    right_click(&mut fake, &mut app, window, cell(8, 0), none);
    look_up(&mut fake, &mut app);
    // Over the word's first cell, at its baseline, in a window of
    // cells 4 points wide and 8 high, without the drawing's font.
    assert_eq!(
        fake.definitions(),
        [FakeDefinition {
            window,
            text: "world".to_owned(),
            baseline: Point { x: 24.0, y: 6.0 },
            font: None,
        }]
    );
    // A selection of two lines has none, and its item names nothing.
    select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(5, 1)), none);
    right_click(&mut fake, &mut app, window, cell(2, 0), none);
    let (_, items, dimmed) = context_menu(&fake, window);
    assert_eq!(items[3], "Look Up");
    assert!(dimmed.contains(&LOOK_UP.to_owned()), "{dimmed:?}");
    look_up(&mut fake, &mut app);
    assert_eq!(fake.definitions().len(), 1);
    // On the second row, without the space before it, at the cell after
    // the space.
    select_drag(&mut fake, &mut app, window, (cell(6, 1), cell(12, 1)), none);
    right_click(&mut fake, &mut app, window, cell(8, 1), none);
    look_up(&mut fake, &mut app);
    let last = fake.definitions().last().unwrap();
    assert_eq!(
        (last.text.as_str(), last.baseline),
        ("line", Point { x: 28.0, y: 14.0 })
    );
}

#[test]
fn the_man_page_of_the_selection_opens_in_a_window_of_its_own() {
    let (mut fake, mut app, window) = selecting("", "");
    // A man that tells its arguments, gives an empty title, prints a
    // page of 100 lines, and ends.
    let directory = std::env::temp_dir().join(format!("lokyr-man-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let man = directory.join("man");
    std::fs::write(
        &man,
        "#!/bin/sh\necho \"man $*\"\nprintf '\\033]2;\\007'\ni=1\nwhile [ $i -le 100 ]; do echo \"line $i\"; i=$((i+1)); done\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&man, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    app.man = Some(man);
    let none = Modifiers::NONE;
    let open = |fake: &mut Fake, app: &mut App| {
        fake.send(
            app,
            window,
            WindowEvent::ContextCommand(MAN_PAGE.to_owned()),
        );
    };
    right_click(&mut fake, &mut app, window, cell(8, 0), none);
    open(&mut fake, &mut app);
    let opened: Vec<WindowId> = fake.windows().filter(|&other| other != window).collect();
    let [page] = opened[..] else {
        panic!("one window more: {opened:?}");
    };
    // Of 80 columns and 48 rows, cells 4 points wide and 8 high, its
    // title the page's, in the man page's colors.
    let shown = fake.window(page).unwrap();
    assert_eq!(shown.title, "man world — 80×48");
    // Light, as the app is; it neither keeps its frame nor restores
    // one.
    assert_eq!(shown.appearance, None);
    assert!(!shown.remember && !shown.restore && !shown.restore_size);
    assert_eq!(
        shown.size.points,
        Size {
            width: 320.0,
            height: 384.0
        }
    );
    let grid = app.pane(page).grid();
    assert_eq!((grid.columns, grid.rows), (80, 48));
    let yellow = app.theme.man_page.colors.background;
    assert_eq!(background(&mut fake, &mut app, page), yellow);
    // Once `man` has ended, the window keeps the page, from its top.
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(page).is_kept() && row(app, page, 0) == "man -P ul -- world"
    });
    assert!(fake.windows().any(|other| other == page));
    assert_eq!(row(&app, page, 1), "line 1");
    // An empty title of the program's leaves the page's.
    assert_eq!(fake.window(page).unwrap().title, "man world — 80×48");
    // A pager's keys scroll it, and others do nothing.
    fake.send(&mut app, page, typed(KeyCode::ArrowDown, "", false));
    fake.send(&mut app, page, WindowEvent::Frame);
    assert_eq!(row(&app, page, 0), "line 1");
    fake.send(&mut app, page, typed(KeyCode::KeyQ, "q", false));
    fake.send(&mut app, page, typed(KeyCode::End, "", false));
    fake.send(&mut app, page, WindowEvent::Frame);
    assert!(fake.windows().any(|other| other == page));
    assert_eq!(app.pane(page).view_offset(), 0);
    // The appearance leaves its colors, and its title reads over them:
    // dark, in the light appearance of its own.
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    assert_eq!(background(&mut fake, &mut app, page), yellow);
    assert_ne!(background(&mut fake, &mut app, window), yellow);
    assert_eq!(
        fake.window(page).unwrap().appearance,
        Some(Appearance::Light)
    );
    assert_eq!(fake.window(window).unwrap().appearance, None);
    // Two words name no page: the item is dimmed, and names nothing.
    select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(11, 0)), none);
    right_click(&mut fake, &mut app, window, cell(2, 0), none);
    let (_, items, dimmed) = context_menu(&fake, window);
    assert_eq!(items[1], "Open man Page");
    assert!(dimmed.contains(&MAN_PAGE.to_owned()), "{dimmed:?}");
    assert_eq!(items[3], "Look Up “hello world”");
    open(&mut fake, &mut app);
    assert_eq!(fake.windows().count(), 2);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_man_page_s_name_takes_its_section_and_no_option() {
    let args = |text: &str| man_page(text).map(|args| args.join(" "));
    for (text, wanted) in [
        ("ls", "-- ls"),
        ("ls(1)", "-- 1 ls"),
        ("File::Spec(3pm)", "-- 3pm File::Spec"),
        ("c++", "-- c++"),
        ("git-commit", "-- git-commit"),
        ("pthread_create(3)", "-- 3 pthread_create"),
        ("printf(n)", "-- n printf"),
    ] {
        assert_eq!(args(text).as_deref(), Some(wanted), "{text}");
    }
    for text in [
        "-P",
        ".",
        "..",
        "./x.1",
        "a/b",
        "a b",
        "x;rm",
        "$(x)",
        "ls(",
        "(1)",
        "ls()",
        "ls(x)",
        "ls(123456789)",
        "",
        &"a".repeat(MAN_PAGE_NAME + 1),
    ] {
        assert_eq!(args(text), None, "{text}");
    }
    assert!(man_page(&"a".repeat(MAN_PAGE_NAME)).is_some());
    // Through the login shell, which takes them as they are.
    let args = man_page("ls(1)").unwrap();
    assert_eq!(
        man_program(None, &args),
        Program::LoginCommand {
            command: "exec man -P ul -- 1 ls".into()
        }
    );
    assert_eq!(
        man_program(Some(std::path::Path::new("/bin/echo")), &args),
        Program::command("/bin/echo", ["-P", "ul", "--", "1", "ls"])
    );
}

#[test]
fn a_title_shortens_past_its_characters() {
    assert_eq!(shortened("abcdef", 3), "abc…");
    assert_eq!(shortened("abc", 3), "abc");
    assert_eq!(shortened("àèìòù", 2), "àè…");
    assert_eq!(shortened("", 2), "");
}

#[test]
fn the_context_menu_opens_the_link_at_its_cell() {
    let (mut fake, mut app, window, _) = pasting(
        "context",
        "[links]\nurls = \"open\"\n",
        shell(concat!(
            "stty raw -echo; printf 'see https://lokyr.dev/x here\\r\\n'; ",
            "echo ready; cat -v"
        )),
    );
    fake.send(&mut app, window, WindowEvent::Focused(false));
    // On the URL, which names no man page.
    right_click(&mut fake, &mut app, window, cell(10, 0), Modifiers::NONE);
    assert_eq!(context_menu(&fake, window).2, [MAN_PAGE]);
    right_click(&mut fake, &mut app, window, cell(1, 0), Modifiers::NONE);
    assert_eq!(context_menu(&fake, window).2, ["links.open"]);
    // The item takes the cell that the menu opened on.
    right_click(&mut fake, &mut app, window, cell(10, 0), Modifiers::NONE);
    let chosen = WindowEvent::ContextCommand("links.open".to_owned());
    fake.send(&mut app, window, chosen);
    assert_eq!(fake.opened(), ["https://lokyr.dev/x"]);
}

#[test]
fn the_context_menu_s_link_is_told_by_its_row_s_number_after_a_scroll() {
    let (mut fake, mut app, window, _) = pasting(
        "context-scrolled",
        "[links]\nurls = \"open\"\n",
        shell(concat!(
            "seq 1 80; printf 'see https://lokyr.dev/x here\\n'; echo ready; ",
            "stty raw -echo; cat -v"
        )),
    );
    fake.send(&mut app, window, WindowEvent::Focused(false));
    // The terminal scrolled: the URL's row is far from its number.
    assert!(app.pane(window).origin().unwrap().top > 10);
    let rows = app.pane(window).grid().rows;
    let url = (0..rows)
        .find(|&y| row(&app, window, y).starts_with("see https://lokyr.dev/x"))
        .expect("the URL on screen");
    // On the URL, Open Link is enabled; on the word before it, dimmed.
    right_click(&mut fake, &mut app, window, cell(10, url), Modifiers::NONE);
    assert_eq!(context_menu(&fake, window).2, [MAN_PAGE]);
    right_click(&mut fake, &mut app, window, cell(1, url), Modifiers::NONE);
    assert_eq!(context_menu(&fake, window).2, ["links.open"]);
}

#[test]
fn command_underlines_the_link_under_the_pointer_still_until_it_goes_up() {
    let (mut fake, mut app, window, _) = pasting(
        "underlined",
        "",
        shell("stty raw -echo; printf 'see https://lokyr.dev/x here\\r\\n'; echo ready; cat -v"),
    );
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let underlined = |app: &App| {
        app.pane(window)
            .snapshot()
            .decorations()
            .iter()
            .map(|span| (span.row, span.first, span.last))
            .collect::<Vec<_>>()
    };
    // The pointer over the URL, without Command: nothing.
    let over = cell(10, 0);
    fake.send(&mut app, window, moved(over, Modifiers::NONE));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(underlined(&app).is_empty());
    // Command down, the pointer still: the URL's cells underlined.
    let command = |pressed: bool| {
        key_event(
            KeyCode::SuperLeft,
            "",
            if pressed { CMD } else { Modifiers::NONE },
            pressed,
        )
    };
    let tracks = |fake: &Fake| fake.window(window).unwrap().mouse_moves;
    assert!(!tracks(&fake));
    fake.send(&mut app, window, command(true));
    run_until(&mut fake, &mut app, |_, app| !underlined(app).is_empty());
    assert_eq!(underlined(&app), [(0, 4, 22)]);
    // The pointer's moves wake the app while Command is held, and not
    // after.
    assert!(tracks(&fake));
    // Up, it goes.
    fake.send(&mut app, window, command(false));
    run_until(&mut fake, &mut app, |_, app| underlined(app).is_empty());
    assert!(!tracks(&fake));
}

/// The app of a window that shows 100 lines and a link below them, a
/// keymap's ⌘E that sends `e` to its program, which `then` runs after it
/// reads a byte, with Command held and the pointer still over the link,
/// underlined; and the link's row.
fn underlined_still(test: &str, then: &str) -> (Fake, App, WindowId, u16) {
    let program = format!(
        "seq 1 100; stty raw -echo; printf 'see https://lokyr.dev/x here\\r\\n'; echo ready; head -c 1 >/dev/null; {then}; cat -v"
    );
    let keys = "[keys]\n\"cmd+e\" = { command = \"pane.send-text\", args = { text = \"e\" } }\n";
    let (mut fake, mut app, window, _) = pasting(test, keys, shell(&program));
    let row = screen(&app, window)
        .iter()
        .position(|line| line.contains("https"))
        .unwrap();
    let row = u16::try_from(row).unwrap();
    fake.send(&mut app, window, moved(cell(10, row), Modifiers::NONE));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::SuperLeft, "", CMD, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        decorated(app, window) == [(row, 4, 22)]
    });
    (fake, app, window, row)
}

/// The decorations' spans that `window` shows, each of its row and its
/// first and last columns.
fn decorated(app: &App, window: WindowId) -> Vec<(u16, u16, u16)> {
    app.pane(window)
        .snapshot()
        .decorations()
        .iter()
        .map(|span| (span.row, span.first, span.last))
        .collect()
}

#[test]
fn the_underline_goes_as_the_program_erases_the_link_under_a_still_pointer() {
    // Up to the link's line, which goes, as a shell's ⌘⌫ takes the line.
    let (mut fake, mut app, window, row) =
        underlined_still("erased", "printf '\\033[2A\\033[2K'; echo erased");
    fake.send(&mut app, window, key_event(KeyCode::KeyE, "e", CMD, true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "erased"));
    run_until(&mut fake, &mut app, |_, app| {
        decorated(app, window).is_empty()
    });
    assert!(!screen(&app, window)[usize::from(row)].contains("https"));
    // Command is still held.
    assert!(app.command_held);
}

#[test]
fn the_underline_goes_as_the_view_scrolls_the_link_from_under_a_still_pointer() {
    let (mut fake, mut app, window, row) = underlined_still("scrolled", "true");
    // A line up, with Command held: the pointer is over the line before
    // the link's.
    fake.send(&mut app, window, key_event(KeyCode::ArrowUp, "", CMD, true));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).snapshot().offset() == 1
    });
    run_until(&mut fake, &mut app, |_, app| {
        decorated(app, window).is_empty()
    });
    assert!(screen(&app, window)[usize::from(row) + 1].contains("https"));
    // Back down, the link is under it again, and underlined.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowDown, "", CMD, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        decorated(app, window) == [(row, 4, 22)]
    });
}

#[test]
fn the_pointer_is_a_hand_over_the_link_that_command_underlines() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let program =
        shell("stty raw -echo; printf 'see https://lokyr.dev/x here\\r\\n'; echo ready; cat -v");
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    let cell = *app.drawing.as_ref().unwrap().font.cell();
    let scale = fake.window(window).unwrap().size.scale;
    let (width, height) = (
        f64::from(cell.width) / scale,
        f64::from(cell.height) / scale,
    );
    let hands = |fake: &Fake| {
        fake.window(window)
            .unwrap()
            .pointer_areas
            .iter()
            .filter(|area| area.shape == PointerShape::PointingHand)
            .map(|area| {
                (
                    area.origin.x,
                    area.origin.y,
                    area.size.width,
                    area.size.height,
                )
            })
            .collect::<Vec<_>>()
    };
    // The pointer over the URL, then Command down: the hand over its 19
    // cells from the 5th, as long as Command is held.
    let over = Point {
        x: 10.5 * width,
        y: 0.5 * height,
    };
    fake.send(&mut app, window, moved(over, Modifiers::NONE));
    let command = |pressed: bool| {
        key_event(
            KeyCode::SuperLeft,
            "",
            if pressed { CMD } else { Modifiers::NONE },
            pressed,
        )
    };
    fake.send(&mut app, window, command(true));
    run_until(&mut fake, &mut app, |fake, _| !hands(fake).is_empty());
    assert_eq!(hands(&fake), [(4.0 * width, 0.0, 19.0 * width, height)]);
    fake.send(&mut app, window, command(false));
    run_until(&mut fake, &mut app, |fake, _| hands(fake).is_empty());
}

#[test]
fn a_command_click_opens_the_link_that_the_drawn_frame_showed() {
    let (mut fake, mut app, window, _) = pasting(
        "click-drawn",
        "[links]\nurls = \"open\"\n",
        shell(concat!(
            "stty -echo; printf 'see https://lokyr.dev/x here\\n'; echo ready; ",
            "read line; seq 1 70; echo https://lokyr.dev/y; read line"
        )),
    );
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert_eq!(
        row(&app, window, 0).trim_end(),
        "see https://lokyr.dev/x here"
    );
    // Output scrolls the terminal's view before the next frame, which
    // still shows the URL at its first row.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    let start = std::time::Instant::now();
    while !app
        .pane(window)
        .line_text(72, 0)
        .is_some_and(|(text, _)| text.starts_with("https://lokyr.dev/y"))
    {
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let drawn = app.pane(window).snapshot().origin().unwrap();
    assert_eq!(drawn.top, 0);
    assert!(app.pane(window).origin().unwrap().top > 0);
    // A Command-click on the URL's cell, as the frame shows it.
    fake.send(&mut app, window, click(true, cell(10, 0), CMD, 1));
    fake.send(&mut app, window, click(false, cell(10, 0), CMD, 1));
    assert_eq!(fake.opened(), ["https://lokyr.dev/x"]);
}

#[test]
fn the_context_menu_s_link_is_its_text_s_where_output_moved_it() {
    let (mut fake, mut app, window, _) = pasting(
        "context-moved",
        "[links]\nurls = \"ask\"\n",
        shell(concat!(
            "stty -echo; printf 'see https://lokyr.dev/x here\\n'; echo ready; ",
            "read line; seq 1 70; echo https://lokyr.dev/y; echo done; read line; ",
            "yes https://lokyr.dev/z | head -100; echo https://lokyr.dev/z/end; read line"
        )),
    );
    let chosen = || WindowEvent::ContextCommand("links.open".to_owned());
    // The program's last line written whole, its newline too, which a
    // read of the terminal can bring after the line.
    let ended = |app: &App, last: &str| {
        let at = app.pane(window).snapshot().cursor().position;
        at.x == 0 && at.y > 0 && row(app, window, at.y - 1) == last
    };
    fake.send(&mut app, window, WindowEvent::Focused(false));
    right_click(&mut fake, &mut app, window, cell(10, 0), Modifiers::NONE);
    assert!(
        !context_menu(&fake, window)
            .2
            .contains(&"links.open".to_owned())
    );
    // Output scrolls the URL out of the view while the menu is open.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| ended(app, "done"));
    assert_ne!(row(&app, window, 0), "see https://lokyr.dev/x here");
    fake.send(&mut app, window, chosen());
    // The question is the menu's window's, which has no focus.
    let [ask] = fake.alerts() else {
        panic!("one question: {:?}", fake.alerts());
    };
    assert_eq!(
        (ask.window, ask.alert.detail.as_str()),
        (Some(window), "https://lokyr.dev/x")
    );
    fake.answer_alert_with(&mut app, ask.id, 1, false);
    // The menu's item runs once.
    fake.send(&mut app, window, chosen());
    assert!(fake.alerts().is_empty());
    // Its text scrolled into the scrollback, which a clear takes away,
    // it runs nothing, though links came to every row around its own,
    // which a resize that a pane puts off can move by one.
    // The rows that the snapshot has, which a resize put off leaves one
    // short of the grid's.
    let rows = app.pane(window).snapshot().rows();
    let y = (0..rows)
        .find(|&r| row(&app, window, r) == "https://lokyr.dev/y")
        .unwrap();
    right_click(&mut fake, &mut app, window, cell(10, y), Modifiers::NONE);
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        ended(app, "https://lokyr.dev/z/end")
    });
    app.pane(window).clear_scrollback();
    // The rows again, which the resize put off, done meanwhile, can have
    // made one fewer, and the clicked row's place among them.
    let rows = app.pane(window).snapshot().rows();
    let y = y.min(rows - 1);
    let around: Vec<String> = (y.saturating_sub(2)..rows)
        .map(|r| row(&app, window, r))
        .collect();
    assert!(
        row(&app, window, y).starts_with("https://lokyr.dev/z"),
        "row {y} of {rows}, the cursor at {:?}: {around:?}",
        app.pane(window).snapshot().cursor().position
    );
    fake.send(&mut app, window, chosen());
    assert!(fake.alerts().is_empty() && fake.opened().is_empty());
}

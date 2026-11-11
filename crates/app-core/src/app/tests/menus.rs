//! The tests of the menu bar and of the context menu's commands.

use lokyr_kernel::config::SearchEngine;

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
            "Make Lokyr the Default Terminal",
            "Install Command Line Tools…",
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
            "Close Window shift+cmd+w",
            "Set Title… shift+cmd+i",
            "Quick Terminal",
            "-",
            "New Tab cmd+t",
            "Close Tab cmd+w",
            "Select Next Tab shift+cmd+]",
            "Select Previous Tab shift+cmd+[",
            "Rename Tab…",
            "Move Tab to New Window",
            "-",
            "Select Next Pane cmd+]",
            "Select Previous Pane cmd+[",
            "Zoom Pane shift+cmd+Enter",
            "Equalize Panes",
            "Move Pane to New Tab",
            "Move Pane to New Window",
            "Close Pane",
            "-",
            "Split Right cmd+d",
            "Split Left",
            "Split Down shift+cmd+d",
            "Split Up",
            "Select Pane Left alt+cmd+ArrowLeft",
            "Select Pane Right alt+cmd+ArrowRight",
            "Select Pane Above alt+cmd+ArrowUp",
            "Select Pane Below alt+cmd+ArrowDown"
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
            "Copy Mode shift+cmd+c",
            "-",
            "Find… cmd+f",
            "Find Next cmd+g",
            "Find Previous shift+cmd+g",
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
            "Scroll Up a Line shift+cmd+ArrowUp",
            "Scroll Down a Line shift+cmd+ArrowDown",
            "Scroll Up a Page cmd+PageUp",
            "Scroll Down a Page cmd+PageDown",
            "Scroll to the Top cmd+Home",
            "Scroll to the Bottom cmd+End",
            "Jump to Previous Prompt cmd+ArrowUp",
            "Jump to Next Prompt cmd+ArrowDown",
            "-",
            "Command Palette… shift+cmd+p",
            "Choose Theme…",
            "-",
            "Appearance",
            "-",
            "Show Performance"
        ]
    );
    assert_eq!(
        menu_items(&fake, 4),
        [
            "Minimize cmd+m",
            "Zoom",
            "-",
            "Float on Top",
            "-",
            "Bring All to Front"
        ]
    );
    assert_eq!(menu_items(&fake, 5), ["Show a Tip", "Report an Issue…"]);
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
            "Quick Terminal",
            "-",
            "New Tab cmd+t",
            "Close Tab",
            "Select Next Tab shift+cmd+]",
            "Select Previous Tab shift+cmd+[",
            "Rename Tab…",
            "Move Tab to New Window",
            "-",
            "Select Next Pane cmd+]",
            "Select Previous Pane cmd+[",
            "Zoom Pane shift+cmd+Enter",
            "Equalize Panes",
            "Move Pane to New Tab",
            "Move Pane to New Window",
            "Close Pane",
            "-",
            "Split Right cmd+d",
            "Split Left",
            "Split Down shift+cmd+d",
            "Split Up",
            "Select Pane Left alt+cmd+ArrowLeft",
            "Select Pane Right alt+cmd+ArrowRight",
            "Select Pane Above alt+cmd+ArrowUp",
            "Select Pane Below alt+cmd+ArrowDown"
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
            "Copy Mode shift+cmd+c",
            "-",
            "Find… cmd+f",
            "Find Next cmd+g",
            "Find Previous shift+cmd+g",
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
    // Command-W is the tab's, the window's next key its own; a config that
    // binds Command-W to the window, as it reloads, takes it from the tab,
    // the window's item showing its first key that no later binding takes;
    // and then takes the window's away.
    let (mut fake, mut app, path) = with_file("menu-keys", "");
    let close = |fake: &Fake| {
        (
            menu_items(fake, 1)[1].clone(),
            menu_items(fake, 1)[6].clone(),
        )
    };
    let shown = |window: &str, tab: &str| (window.to_owned(), tab.to_owned());
    assert_eq!(
        close(&fake),
        shown("Close Window shift+cmd+w", "Close Tab cmd+w")
    );
    std::fs::write(&path, "[keys]\n\"cmd+w\" = \"window.close\"\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(close(&fake), shown("Close Window shift+cmd+w", "Close Tab"));
    std::fs::write(
        &path,
        "[keys]\n\"cmd+w\" = false\n\"cmd+shift+w\" = false\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(close(&fake), shown("Close Window", "Close Tab"));
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
    assert_eq!(app.keymap.bindings().len(), 123);
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
            (
                "app.install-command-line-tools",
                "Install Command Line Tools…",
                Category::App
            ),
            (
                "app.make-default-terminal",
                "Make Lokyr the Default Terminal",
                Category::App
            ),
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
            (
                "pane.man-search",
                "Search man Page Index",
                Category::Windows
            ),
            ("pane.search-web", "Search the Web", Category::Edit),
            ("window.close", "Close Window", Category::Windows),
            ("window.goto", "Go to Window", Category::Windows),
            ("window.new", "New Window", Category::Windows),
            ("window.set-title", "Set Title…", Category::Windows),
            (
                "window.toggle-float-on-top",
                "Float on Top",
                Category::Windows
            ),
        ]
    );
    let texts = Texts::new("en", &CATALOGS);
    let menu = main_menu(&registry, &Keymap::default(), &texts, &[]);
    let titles: Vec<&str> = menu.menus[1]
        .items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Command { title, .. } => Some(title.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(titles, ["New Window", "Close Window", "Set Title…"]);
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
            "Modalità copia shift+cmd+c",
            "-",
            "Trova… cmd+f",
            "Trova successivo cmd+g",
            "Trova precedente shift+cmd+g",
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
            "Copy Link Address",
            "New Window at Folder",
            "New Tab at Folder",
            "Open man Page for “world”",
            "Search man Page Index for “world”",
            "-",
            "Look Up “world”",
            "Search the Web for “world”",
            "-",
            "Copy",
            "Paste",
            "-",
            "Select All",
            "Clear",
            "Copy Mode",
            "-",
            "Rename Tab…",
            "-",
            "Split Right",
            "Split Left",
            "Split Down",
            "Split Up",
            "Move Pane to New Tab",
            "Move Pane to New Window",
            "Close Pane"
        ]
    );
    // A pane alone in its window moves nowhere, nor closes as a split.
    let alone = [
        "Move Pane to New Tab",
        "Move Pane to New Window",
        "Close Pane",
    ];
    let links = [
        "links.open",
        "links.copy",
        "links.new-window",
        "links.new-tab",
    ];
    assert_eq!(dimmed[..], [&links[..], &alone].concat());
    // Within the selection, which stays.
    select_drag(&mut fake, &mut app, window, (cell(0, 0), cell(3, 0)), none);
    right_click(&mut fake, &mut app, window, cell(1, 0), none);
    assert_eq!(selected_text(&app, window).as_deref(), Some("hel"));
    assert_eq!(context_menu(&fake, window).1[7], "Look Up “hel”");
    assert_eq!(context_menu(&fake, window).2[..4], links);
    // On a blank cell, which takes it away, and dims Copy and Look Up.
    right_click(&mut fake, &mut app, window, cell(5, 0), none);
    assert_eq!(selected_text(&app, window), None);
    assert!(app.windows[&window].selecting.is_none());
    assert_eq!(
        context_menu(&fake, window).1[4..9],
        [
            "Open man Page",
            "Search man Page Index",
            "-",
            "Look Up",
            "Search the Web"
        ]
    );
    let selection = [
        "pane.man-page",
        "pane.man-search",
        "pane.look-up",
        "pane.search-web",
        "pane.copy",
    ];
    assert_eq!(
        context_menu(&fake, window).2[..9],
        [&links[..], &selection].concat()
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
    assert_eq!(items[7], "Look Up");
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
    assert_eq!(items[4], "Open man Page");
    assert!(dimmed.contains(&MAN_PAGE.to_owned()), "{dimmed:?}");
    assert_eq!(items[7], "Look Up “hello world”");
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
    assert_eq!(
        context_menu(&fake, window).2[..3],
        ["links.new-window", "links.new-tab", MAN_PAGE]
    );
    right_click(&mut fake, &mut app, window, cell(1, 0), Modifiers::NONE);
    assert_eq!(
        context_menu(&fake, window).2[..4],
        [
            "links.open",
            "links.copy",
            "links.new-window",
            "links.new-tab"
        ]
    );
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
    assert_eq!(
        context_menu(&fake, window).2[..3],
        ["links.new-window", "links.new-tab", MAN_PAGE]
    );
    right_click(&mut fake, &mut app, window, cell(1, url), Modifiers::NONE);
    assert_eq!(
        context_menu(&fake, window).2[..4],
        [
            "links.open",
            "links.copy",
            "links.new-window",
            "links.new-tab"
        ]
    );
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
    // A line up, with Command held, and Shift: the pointer is over the line
    // before the link's.
    let shift_cmd = Modifiers { shift: true, ..CMD };
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowUp, "", shift_cmd, true),
    );
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
        key_event(KeyCode::ArrowDown, "", shift_cmd, true),
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

/// The id of the item of `title` of `window`'s last context menu.
fn context_item(fake: &Fake, window: WindowId, title: &str) -> String {
    let (over, _, menu) = fake.context_menus().last().expect("a context menu");
    assert_eq!(*over, window);
    menu.items
        .iter()
        .find_map(|item| match item {
            MenuItem::Command {
                title: shown,
                command,
                ..
            } if shown == title => Some(command.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no item “{title}”"))
}

/// The id of the item of `title` of the menu bar's menu at `index`.
fn bar_item(fake: &Fake, index: usize, title: &str) -> String {
    fake.main_menu().unwrap().menus[index]
        .items
        .iter()
        .find_map(|item| match item {
            MenuItem::Command {
                title: shown,
                command,
                ..
            } if shown == title => Some(command.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no item “{title}”"))
}

#[test]
fn a_plugin_s_items_run_their_commands_with_their_arguments() {
    use lokyr_kernel::workspace::{Direction, Layout};
    let (mut fake, mut app, window) = selecting("", "");
    let shown = app.windows[&window].window;
    let layout = |app: &App| {
        let tab = app.workspace.window(shown).unwrap().active_tab();
        app.workspace.tab(tab).unwrap().layout().clone()
    };
    let first = app.workspace.panes_of(shown)[0];
    // The context menu's Split Left, in its window, which has no focus: the
    // new pane on the left, with the keyboard.
    right_click(&mut fake, &mut app, window, cell(8, 0), Modifiers::NONE);
    let item = context_item(&fake, window, "Split Left");
    assert!(item.starts_with("menu-item:"), "{item}");
    fake.send(&mut app, window, WindowEvent::ContextCommand(item));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.panes_of(shown).len() == 2
    });
    let Layout::Split {
        direction: Direction::Horizontal,
        first: left,
        second: right,
        ..
    } = layout(&app)
    else {
        panic!("{:?}", layout(&app));
    };
    assert_eq!(*right, Layout::Leaf(first));
    let Layout::Leaf(new) = *left else {
        panic!("{left:?}");
    };
    // The menu bar's Split Up, of its window with the focus: a new pane
    // over the left one.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let item = bar_item(&fake, 1, "Split Up");
    fake.send_app(&mut app, AppEvent::Command(item));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.panes_of(shown).len() == 3
    });
    let Layout::Split { first: left, .. } = layout(&app) else {
        panic!("{:?}", layout(&app));
    };
    let Layout::Split {
        direction: Direction::Vertical,
        second: under,
        ..
    } = *left
    else {
        panic!("{left:?}");
    };
    assert_eq!(*under, Layout::Leaf(new));
}

#[test]
fn the_context_menu_opens_a_window_or_a_tab_in_the_folder_under_the_pointer() {
    let base = real_directory("at-folder");
    let folder = base.join("in");
    std::fs::create_dir(&folder).unwrap();
    let shown = folder.display().to_string();
    let home = std::env::home_dir().unwrap().display().to_string();
    // Each pane tells where it started; the first prints the folder's path
    // on its second row.
    let script = format!("pwd; [ \"$PWD\" = {home:?} ] && printf '%s\\n' {shown:?}; exec sleep 10");
    let mut fake = Fake::default();
    let mut app = app(unpadded(), shell(&script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    assert_eq!(started_in(&mut fake, &mut app, window), home);
    run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == shown);
    // A tab, in the window, which shows it.
    right_click(&mut fake, &mut app, window, cell(2, 1), Modifiers::NONE);
    let (_, items, dimmed) = context_menu(&fake, window);
    assert_eq!(
        items[..4],
        [
            "Open Link",
            "Copy Link Address",
            "New Window at Folder",
            "New Tab at Folder"
        ]
    );
    assert!(
        !dimmed.iter().any(|name| name.starts_with("links.")),
        "{dimmed:?}"
    );
    let chosen = |name: &str| WindowEvent::ContextCommand(name.to_owned());
    fake.send(&mut app, window, chosen("links.new-tab"));
    let tabs = |app: &App| {
        let shown = app.windows[&window].window;
        app.workspace.window(shown).unwrap().tabs().len()
    };
    run_until(&mut fake, &mut app, |_, app| {
        tabs(app) == 2 && app.pane(window).snapshot().rows() > 0 && row(app, window, 0) == shown
    });
    // A window, of the first tab's path.
    let first = app
        .workspace
        .window(app.windows[&window].window)
        .unwrap()
        .tabs()[0];
    app.workspace.activate_tab(first);
    run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == shown);
    right_click(&mut fake, &mut app, window, cell(2, 1), Modifiers::NONE);
    fake.send(&mut app, window, chosen("links.copy"));
    assert_eq!(fake.clipboard_text().as_deref(), Some(shown.as_str()));
    right_click(&mut fake, &mut app, window, cell(2, 1), Modifiers::NONE);
    fake.send(&mut app, window, chosen("links.new-window"));
    let opened = fake
        .windows()
        .find(|other| *other != window)
        .expect("a new window");
    assert_eq!(started_in(&mut fake, &mut app, opened), shown);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_man_pages_index_lists_the_pages_of_the_selection_s_words_in_a_window() {
    let (mut fake, mut app, window) = selecting("", "");
    // A man that tells its arguments and ends.
    let directory = std::env::temp_dir().join(format!("lokyr-man-k-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let man = directory.join("man");
    std::fs::write(&man, "#!/bin/sh\necho \"man $*\"\n").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&man, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    app.man = Some(man);
    // Of the two words of the first line.
    select_drag(
        &mut fake,
        &mut app,
        window,
        (cell(0, 0), cell(11, 0)),
        Modifiers::NONE,
    );
    right_click(&mut fake, &mut app, window, cell(2, 0), Modifiers::NONE);
    let (_, items, dimmed) = context_menu(&fake, window);
    assert_eq!(items[5], "Search man Page Index for “hello world”");
    assert!(!dimmed.contains(&MAN_SEARCH.to_owned()), "{dimmed:?}");
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand(MAN_SEARCH.to_owned()),
    );
    let opened: Vec<WindowId> = fake.windows().filter(|&other| other != window).collect();
    let [listed] = opened[..] else {
        panic!("one window more: {opened:?}");
    };
    assert_eq!(
        fake.window(listed).unwrap().title,
        "man -k hello world — 80×48"
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(listed).is_kept() && row(app, listed, 0) == "man -P ul -k -- hello world"
    });
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn the_man_pages_index_takes_words_that_a_shell_takes_without_quotes() {
    let words = |text: &str| man_keywords(text).map(|words| words.join(" "));
    for (text, wanted) in [
        ("printf", "printf"),
        ("  file   system ", "file system"),
        ("File::Spec c++ x.y_z", "File::Spec c++ x.y_z"),
    ] {
        assert_eq!(words(text).as_deref(), Some(wanted), "{text}");
    }
    for text in [
        "",
        "-k",
        "a -k",
        ".x",
        "a/b",
        "x;rm",
        "$(x)",
        "it's",
        "a b c d e f g h i",
    ] {
        assert_eq!(words(text), None, "{text}");
    }
    assert!(words(&"a".repeat(100)).is_some());
    assert_eq!(words(&"a".repeat(101)), None);
}

#[test]
fn the_web_s_search_opens_the_engine_s_page_of_the_selection() {
    let (mut fake, mut app, window) = selecting("", "");
    let search = |fake: &mut Fake, app: &mut App| {
        right_click(fake, app, window, cell(8, 0), Modifiers::NONE);
        fake.send(
            app,
            window,
            WindowEvent::ContextCommand(SEARCH_WEB.to_owned()),
        );
    };
    right_click(&mut fake, &mut app, window, cell(8, 0), Modifiers::NONE);
    assert_eq!(
        context_menu(&fake, window).1[8],
        "Search the Web for “world”"
    );
    search(&mut fake, &mut app);
    assert_eq!(fake.opened(), ["https://www.google.com/search?q=world"]);
    app.config.links.search_engine =
        SearchEngine::try_from("https://search.example/?s={query}&x=1".to_owned()).unwrap();
    search(&mut fake, &mut app);
    assert_eq!(fake.opened()[1], "https://search.example/?s=world&x=1");
    // Nothing selected: dimmed, and nothing opens.
    right_click(&mut fake, &mut app, window, cell(5, 0), Modifiers::NONE);
    assert!(
        context_menu(&fake, window)
            .2
            .contains(&SEARCH_WEB.to_owned())
    );
    app.search_web(&mut fake);
    assert_eq!(fake.opened().len(), 2);
}

#[test]
fn the_copy_mode_takes_the_keys_of_a_man_page_s_window() {
    let (mut fake, mut app, window) = selecting("", "");
    let directory = std::env::temp_dir().join(format!("lokyr-man-copy-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let man = directory.join("man");
    std::fs::write(
        &man,
        "#!/bin/sh\necho \"man $*\"\ni=1\nwhile [ $i -le 100 ]; do echo \"line $i\"; i=$((i+1)); done\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&man, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    app.man = Some(man);
    right_click(&mut fake, &mut app, window, cell(8, 0), Modifiers::NONE);
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand(MAN_PAGE.to_owned()),
    );
    let page = fake.windows().find(|&other| other != window).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(page).is_kept() && row(app, page, 0) == "man -P ul -- world"
    });
    fake.send(&mut app, page, WindowEvent::Focused(true));
    fake.send_app(&mut app, AppEvent::Command("copy-mode.toggle".to_owned()));
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&page]
            .chrome
            .iter()
            .any(|view| view.plugin == "lokyr/copy-mode")
    });
    // To the page's top, its first line copied, rather than the page
    // scrolled.
    let press = |fake: &mut Fake, app: &mut App, code: KeyCode, plain: &str, mods: Modifiers| {
        fake.send(app, page, key_event(code, plain, mods, true));
        fake.send(app, page, key_event(code, plain, mods, false));
    };
    press(&mut fake, &mut app, KeyCode::KeyG, "g", Modifiers::NONE);
    press(&mut fake, &mut app, KeyCode::KeyG, "g", Modifiers::NONE);
    press(&mut fake, &mut app, KeyCode::KeyY, "y", SHIFT);
    run_until(&mut fake, &mut app, |fake, _| {
        fake.clipboard_text().as_deref() == Some("man -P ul -- world")
    });
    let _ = std::fs::remove_dir_all(directory);
}

/// A plugin of a command of an argument, `menuer.go`, which has no item of
/// its own, and items of it in the Edit menu and the context menu, of
/// titles of the app's items and of its own.
struct Menuer;

impl plugins::Plugin for Menuer {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "menuer.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::Clipboard,
            capabilities: Vec::new(),
            params: vec![lokyr_kernel::commands::Param {
                name: "which".to_owned(),
                kind: lokyr_kernel::commands::Kind::String,
                default: None,
            }],
            when: None,
        })?;
        for (place, title) in [
            (plugins::host::MenuPlace::Bar(Category::Clipboard), "Copy"),
            (plugins::host::MenuPlace::Bar(Category::Clipboard), "copy…"),
            (
                plugins::host::MenuPlace::Bar(Category::Clipboard),
                "Copy as HTML",
            ),
            (plugins::host::MenuPlace::PaneContext, "Paste"),
        ] {
            cx.add_menu_item(plugins::host::MenuItem {
                place,
                command: "menuer.go".to_owned(),
                args: Args::from([(
                    "which".to_owned(),
                    lokyr_kernel::commands::Value::String(title.to_owned()),
                )]),
                title: Some(title.to_owned()),
                submenu: None,
                ticked: false,
            })?;
        }
        Ok(())
    }
}

#[test]
fn a_plugin_s_menu_item_of_an_app_s_title_is_left_out_and_its_others_stand_apart() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet()).with_plugin("menuer", Box::new(Menuer));
    fake.start(&mut app);
    let edit = menu_items(&fake, 2);
    // The app's Copy alone, and the plugin's own title after a line of its
    // own, after the clipboard's commands.
    assert!(!edit.iter().any(|item| item == "Copy"), "{edit:?}");
    let at = edit
        .iter()
        .position(|item| item == "Copy as HTML")
        .expect("its item");
    assert_eq!(edit[..at], ["Copy cmd+c", "Paste cmd+v", "-"], "{edit:?}");
    assert!(!edit.iter().any(|item| item == "copy…"), "{edit:?}");
    // Of the context menu, as of any menu: no line first, after another,
    // nor last.
    let item = |title: &str, command: &str| MenuItem::Command {
        title: title.to_owned(),
        command: command.to_owned(),
        shortcut: None,
    };
    let titles = |items: Vec<MenuItem>| -> Vec<String> {
        items
            .into_iter()
            .map(|item| match item {
                MenuItem::Command { title, .. } => title,
                MenuItem::Separator => "-".to_owned(),
                _ => "?".to_owned(),
            })
            .collect()
    };
    let menu = vec![
        item("Copy", "pane.copy"),
        item("Paste", "pane.paste"),
        MenuItem::Separator,
        item(" PASTE ", "menu-item:0"),
        MenuItem::Separator,
        item("Paste and Go", "menu-item:1"),
        MenuItem::Separator,
        item("Copy...", "menu-item:2"),
    ];
    assert_eq!(
        titles(unique_titles(menu)),
        ["Copy", "Paste", "-", "Paste and Go"]
    );
    // A title that only looks as the app's, of Cyrillic's letters or
    // full-width ones, is taken for it; one that hides characters, of a
    // plugin's catalogs, is left out, and a plugin's title in another
    // script stays.
    let menu = vec![
        item("Copy", "pane.copy"),
        item("Сору", "menu-item:0"),
        item("Ｃｏｐｙ", "menu-item:1"),
        item("Co\u{200B}py Link", "menu-item:2"),
        item("Копировать ссылку", "menu-item:3"),
    ];
    assert_eq!(titles(unique_titles(menu)), ["Copy", "Копировать ссылку"]);
}

/// A plugin of an item in the View menu that its command ticks and unticks
/// in turn.
#[derive(Default)]
struct Ticker {
    ticked: bool,
}

impl Ticker {
    fn item(&self) -> plugins::host::MenuItem {
        plugins::host::MenuItem {
            place: plugins::host::MenuPlace::Bar(Category::View),
            command: "ticker.go".to_owned(),
            args: Args::new(),
            title: Some("Keep Going".to_owned()),
            submenu: None,
            ticked: self.ticked,
        }
    }
}

impl plugins::Plugin for Ticker {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "ticker.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        // An item of the app's Copy, of another title, which leaves the
        // app's own.
        cx.add_menu_item(plugins::host::MenuItem {
            place: plugins::host::MenuPlace::Bar(Category::Clipboard),
            command: "pane.copy".to_owned(),
            args: Args::new(),
            title: Some("Copy Again".to_owned()),
            submenu: None,
            ticked: false,
        })?;
        cx.add_menu_item(self.item())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        self.ticked = !self.ticked;
        cx.add_menu_item(self.item())
    }
}

#[test]
fn a_plugin_s_menu_bar_item_shows_the_tick_that_its_plugin_gives() {
    let mut fake = Fake::default();
    let mut app =
        app(Config::default(), quiet()).with_plugin("ticker", Box::new(Ticker::default()));
    fake.start(&mut app);
    let item = |fake: &Fake| -> String {
        fake.main_menu()
            .unwrap()
            .menus
            .iter()
            .flat_map(|menu| &menu.items)
            .find_map(|item| match item {
                MenuItem::Command { title, command, .. } if title == "Keep Going" => {
                    Some(command.clone())
                }
                _ => None,
            })
            .expect("its item")
    };
    let id = item(&fake);
    assert!(id.starts_with("menu-item:"), "{id}");
    assert!(!fake.command_checked(&id));
    // Its own item takes its command's place: no Go of its own; the app's
    // Copy stays beside the plugin's item of it.
    let titles: Vec<String> = fake
        .main_menu()
        .unwrap()
        .menus
        .iter()
        .flat_map(|menu| &menu.items)
        .filter_map(|item| match item {
            MenuItem::Command { title, .. } => Some(title.clone()),
            _ => None,
        })
        .collect();
    assert!(!titles.iter().any(|title| title == "Go"), "{titles:?}");
    assert!(titles.iter().any(|title| title == "Copy"), "{titles:?}");
    assert!(
        titles.iter().any(|title| title == "Copy Again"),
        "{titles:?}"
    );
    fake.send_app(&mut app, AppEvent::Command(id));
    run_until(&mut fake, &mut app, |fake, _| {
        fake.command_checked(&item(fake))
    });
    // The menu made anew keeps it ticked, at its new place.
    fake.send_app(&mut app, AppEvent::Command(item(&fake)));
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.command_checked(&item(fake))
    });
}

/// A plugin of items in submenus: in the View menu, First, Second, ticked,
/// and Third in its submenu Nest, Loose between them out of it, and Hidden
/// in a submenu of the app's Appearance; in the context menu, Left and
/// Right in its submenu More.
struct Nester;

impl plugins::Plugin for Nester {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "nester.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: vec![lokyr_kernel::commands::Param {
                name: "which".to_owned(),
                kind: lokyr_kernel::commands::Kind::String,
                default: None,
            }],
            when: None,
        })?;
        let bar = plugins::host::MenuPlace::Bar(Category::View);
        let context = plugins::host::MenuPlace::PaneContext;
        for (place, title, submenu) in [
            (bar, "First", Some("Nest")),
            (bar, "Loose", None),
            (bar, "Second", Some("Nest")),
            (bar, "Third", Some("Nest")),
            (bar, "Hidden", Some("appearance…")),
            (context, "Left", Some("More")),
            (context, "Right", Some("More")),
        ] {
            cx.add_menu_item(plugins::host::MenuItem {
                place,
                command: "nester.go".to_owned(),
                args: Args::from([(
                    "which".to_owned(),
                    lokyr_kernel::commands::Value::String(title.to_owned()),
                )]),
                title: Some(title.to_owned()),
                submenu: submenu.map(str::to_owned),
                ticked: title == "Second",
            })?;
        }
        Ok(())
    }
}

#[test]
fn a_plugin_s_items_of_a_submenu_go_into_it_where_the_first_would_be() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet()).with_plugin("nester", Box::new(Nester));
    fake.start(&mut app);
    let titles = |items: &[MenuItem]| -> Vec<String> {
        items
            .iter()
            .map(|item| match item {
                MenuItem::Command { title, .. } => title.clone(),
                MenuItem::Submenu(menu) => format!("{} >", menu.title),
                MenuItem::Separator => "-".to_owned(),
                MenuItem::Standard { title, .. } => title.clone(),
            })
            .collect()
    };
    let main = fake.main_menu().unwrap();
    let view = &main.menus[3].items;
    let nest = view
        .iter()
        .position(|item| matches!(item, MenuItem::Submenu(menu) if menu.title == "Nest"))
        .expect("its submenu");
    // The submenu where First would be, Loose after it, then the app's
    // Appearance alone, as the plugin's of its title is left out.
    assert_eq!(
        titles(&view[nest - 1..nest + 4]),
        ["-", "Nest >", "Loose", "-", "Appearance >"],
        "{:?}",
        titles(view)
    );
    let MenuItem::Submenu(menu) = &view[nest] else {
        unreachable!()
    };
    assert_eq!(titles(&menu.items), ["First", "Second", "Third"]);
    // Second ticked, of its id within the submenu.
    let ids: Vec<&str> = menu
        .items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Command { command, .. } => Some(command.as_str()),
            _ => None,
        })
        .collect();
    let ticked: Vec<bool> = ids.iter().map(|id| fake.command_checked(id)).collect();
    assert_eq!(ticked, [false, true, false]);
    // The context menu's submenu, of the plugin's group.
    let window = fake.windows().next().unwrap();
    right_click(&mut fake, &mut app, window, cell(8, 0), Modifiers::NONE);
    let (_, _, context) = fake.context_menus().last().expect("a context menu");
    let more = context
        .items
        .iter()
        .find_map(|item| match item {
            MenuItem::Submenu(menu) if menu.title == "More" => Some(menu),
            _ => None,
        })
        .expect("its submenu");
    assert_eq!(titles(&more.items), ["Left", "Right"]);
    assert!(matches!(context.items.last(), Some(MenuItem::Submenu(_))));
}

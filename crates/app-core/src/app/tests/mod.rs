use lokyr_platform_core::fake::{Fake, FakeDefinition};
use lokyr_platform_core::{Dropped, Ime, KeyEvent, KeyState, Modifiers, MouseButton, ScrollDelta};
use lokyr_ui_core::cursor::CursorConfig;
use lokyr_ui_core::pane::Preedit;

use super::*;
use crate::fakes;

mod input;

/// The app of the tests, which draws with the tests' drawing, if at all.
type App = super::App<fakes::Gpu>;
type Drawing = super::Drawing<fakes::Gpu>;

/// A program that runs until its terminal hangs up, and writes nothing.
fn quiet() -> Program {
    Program::command("/bin/cat", [] as [&str; 0])
}

/// The app, of `config`, whose windows run `program`.
fn app(config: Config, program: Program) -> App {
    App::new(None, config, None).with_program(program)
}

fn started() -> (Fake, App) {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet());
    fake.start(&mut app);
    (fake, app)
}

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
            "Clear cmd+k"
        ]
    );
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
    assert!(menu_items(&fake, 5).is_empty());
}

/// The items of the main menu's menu `index` that `fake` has: each
/// one's title and its keys, as `cmd+n`, or a line, `-`.
fn menu_items(fake: &Fake, index: usize) -> Vec<String> {
    let keys = |shortcut: &Shortcut| {
        let mut keys = String::new();
        for (held, name) in [
            (shortcut.control, "ctrl+"),
            (shortcut.alt, "alt+"),
            (shortcut.shift, "shift+"),
            (shortcut.command, "cmd+"),
        ] {
            if held {
                keys.push_str(name);
            }
        }
        match shortcut.key {
            ShortcutKey::Char(key) => keys.push(key),
            ShortcutKey::Code(code) => keys.push_str(&format!("{code:?}")),
        }
        keys
    };
    fake.main_menu().unwrap().menus[index]
        .items
        .iter()
        .map(|item| match item {
            MenuItem::Command {
                title, shortcut, ..
            }
            | MenuItem::Standard {
                title, shortcut, ..
            } => match shortcut {
                Some(shortcut) => format!("{title} {}", keys(shortcut)),
                None => title.clone(),
            },
            MenuItem::Separator => "-".to_owned(),
            MenuItem::Submenu(menu) => menu.title.clone(),
        })
        .collect()
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
    // A binding's key, the one that a key bound later takes away, and
    // one that holds in a context alone, which no item shows.
    assert_eq!(
        menu_items(&fake, 1),
        [
            "New Window cmd+y",
            "Close Window",
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
            "Clear cmd+k"
        ]
    );
}

#[test]
fn settings_opens_the_config_in_the_text_editor_made_first() {
    let (mut fake, mut app, path) = with_file("settings", "");
    std::fs::remove_file(&path).unwrap();
    // The application's menu starts with it, of its key.
    assert_eq!(
        menu_items(&fake, 0)[..3],
        ["Settings… cmd+,", "Secure Keyboard Entry", "-"]
    );
    assert_eq!(menu_items(&fake, 0).last().unwrap(), "Quit Lokyr cmd+q");
    // A file that is not there is made of the template, then opens.
    fake.send_app(&mut app, AppEvent::Command(SETTINGS.to_owned()));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::template());
    assert_eq!(fake.edited(), std::slice::from_ref(&path));
    // One that is there stays as it is.
    std::fs::write(&path, "[font]\nsize = 14\n").unwrap();
    fake.send_app(&mut app, AppEvent::Command(SETTINGS.to_owned()));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[font]\nsize = 14\n"
    );
    assert_eq!(fake.edited(), [path.clone(), path.clone()]);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    // Without a file's path, nothing opens.
    let (mut fake, mut app) = started();
    fake.send_app(&mut app, AppEvent::Command(SETTINGS.to_owned()));
    assert!(fake.edited().is_empty());
}

#[test]
fn settings_tells_why_it_opens_nothing() {
    let settings = |fake: &mut Fake, app: &mut App| {
        fake.send_app(app, AppEvent::Command(SETTINGS.to_owned()));
    };
    let told = |fake: &Fake| {
        fake.alerts()
            .iter()
            .map(|shown| (shown.alert.message.clone(), shown.alert.detail.clone()))
            .collect::<Vec<_>>()
    };
    let message = "Lokyr cannot open its config file".to_owned();
    // A directory that cannot be made: a link to one that is gone.
    let (mut fake, mut app, path) = with_file("settings-failed", "");
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let directory = path.parent().unwrap().to_owned();
    std::fs::remove_dir_all(&directory).unwrap();
    std::os::unix::fs::symlink(directory.with_extension("gone"), &directory).unwrap();
    settings(&mut fake, &mut app);
    let shown = told(&fake);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].0, message);
    assert!(
        shown[0].1.contains(&path.display().to_string()),
        "{shown:?}"
    );
    assert!(fake.edited().is_empty());
    // On the window that has the focus.
    assert_eq!(fake.alerts()[0].window, Some(window));
    std::fs::remove_file(&directory).unwrap();
    // An editor that the platform does not find.
    std::fs::create_dir_all(&directory).unwrap();
    fake.fail_editing(Some("no application opens text"));
    settings(&mut fake, &mut app);
    let error = format!(
        "{} could not be opened: no application opens text",
        path.display()
    );
    assert_eq!(told(&fake)[1], (message.clone(), error));
    // One that the system tells of after the open, for the config file;
    // another target's goes to the log alone.
    fake.fail_editing(None);
    settings(&mut fake, &mut app);
    let late = AppEvent::OpenFailed {
        target: path.display().to_string(),
        error: "it is damaged".to_owned(),
    };
    fake.send_app(&mut app, late);
    assert_eq!(
        told(&fake)[2],
        (message.clone(), "it is damaged".to_owned())
    );
    let other = AppEvent::OpenFailed {
        target: "/elsewhere".to_owned(),
        error: "it is damaged".to_owned(),
    };
    fake.send_app(&mut app, other);
    assert_eq!(told(&fake).len(), 3);
    let _ = std::fs::remove_dir_all(&directory);
    // Without a home directory.
    let (mut fake, mut app) = started();
    settings(&mut fake, &mut app);
    let no_home =
        "Without a home directory, there is no config file: the defaults apply.".to_owned();
    assert_eq!(told(&fake), [(message, no_home)]);
}

#[test]
fn the_performance_s_panel_shows_from_the_menu_and_the_config() {
    let (mut fake, mut app, path) = with_file("performance", "");
    let window = fake.windows().next().unwrap();
    // Hidden: no timer of its own, and the View menu's last item.
    assert!(app.performance.is_none());
    assert!(!fake.command_checked(PERFORMANCE));
    assert_eq!(menu_items(&fake, 3).last().unwrap(), "Show Performance");
    let timers = fake.timers().count();
    // The menu shows it, ticked, and its timer measures each second.
    fake.set_usage(lokyr_platform_core::Usage {
        idle_wakeups: 0,
        interrupt_wakeups: 0,
        footprint: 1_000_000,
    });
    fake.send_app(&mut app, AppEvent::Command(PERFORMANCE.to_owned()));
    assert!(fake.command_checked(PERFORMANCE));
    assert_eq!(fake.timers().count(), timers + 1);
    let shown = app.performance.as_ref().unwrap();
    assert_eq!(shown.lines(window), None);
    let (timer, at) = fake
        .timers()
        .find(|&(timer, _)| shown.timer == Some(timer))
        .unwrap();
    fake.set_usage(lokyr_platform_core::Usage {
        idle_wakeups: 3,
        interrupt_wakeups: 4,
        footprint: 2_000_000,
    });
    fake.send(&mut app, window, WindowEvent::Frame);
    fake.send_app(&mut app, AppEvent::Timer(timer));
    assert!(fake.window(window).unwrap().frame_requested);
    let lines = app.performance.as_ref().unwrap().lines(window).unwrap();
    assert_eq!(lines.len(), 7);
    assert_eq!(lines[5], "Memory 2.0 MB");
    let next = app.performance.as_ref().unwrap().timer.unwrap();
    assert!(
        fake.timers()
            .any(|(timer, when)| timer == next && when > at)
    );
    // A window that opens shows its lines once it had a whole second.
    fake.send_app(&mut app, AppEvent::Command("window.new".to_owned()));
    let opened = fake.windows().find(|&other| other != window).unwrap();
    fake.send_app(&mut app, AppEvent::Timer(next));
    let shown = app.performance.as_ref().unwrap();
    assert!(shown.lines(window).is_some() && shown.lines(opened).is_none());
    let next = shown.timer.unwrap();
    fake.send_app(&mut app, AppEvent::Timer(next));
    assert!(app.performance.as_ref().unwrap().lines(opened).is_some());
    let next = app.performance.as_ref().unwrap().timer.unwrap();
    // Again, it hides, and its timer goes.
    fake.send_app(&mut app, AppEvent::Command(PERFORMANCE.to_owned()));
    assert!(app.performance.is_none() && !fake.command_checked(PERFORMANCE));
    assert!(!fake.timers().any(|(timer, _)| timer == next));
    // The config's key shows it, and a change of it takes the menu's
    // choice away.
    fake.send_app(&mut app, AppEvent::Command(PERFORMANCE.to_owned()));
    let mut config = app.config.clone();
    config.debug.performance_overlay = true;
    app.apply(&mut fake, config.clone());
    assert!(app.performance.is_some());
    config.debug.performance_overlay = false;
    app.apply(&mut fake, config);
    assert!(app.performance.is_none() && !fake.command_checked(PERFORMANCE));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn the_menus_follow_a_plugin_s_binding_and_a_reloaded_config() {
    let (mut fake, mut app, _, _) = with_tester();
    assert_eq!(menu_items(&fake, 1)[0], "New Window cmd+n");
    app.run(&mut fake, "test.bind", Args::new());
    assert_eq!(menu_items(&fake, 1)[0], "New Window");
    assert!(menu_items(&fake, 3).contains(&"test.size cmd+n".to_owned()));
    // A config that takes Command-W away, as it reloads.
    let (mut fake, mut app, path) = with_file("menu-keys", "");
    assert_eq!(menu_items(&fake, 1)[1], "Close Window cmd+w");
    std::fs::write(&path, "[keys]\n\"cmd+w\" = false\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(menu_items(&fake, 1)[1], "Close Window");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
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
            ("app.appearance-dark", "Dark Appearance", Category::View),
            ("app.appearance-light", "Light Appearance", Category::View),
            (
                "app.appearance-system",
                "Automatic Appearance",
                Category::View
            ),
            ("app.performance", "Show Performance", Category::View),
            ("app.quit", "Quit Lokyr", Category::App),
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

/// The titles of the main menu that `fake` has that the app's catalogs
/// give: the shell's first item, the application's first and last, and
/// the edit's menu.
fn menu_titles(fake: &Fake) -> Vec<String> {
    let menus = &fake.main_menu().unwrap().menus;
    let title = |item: &MenuItem| match item {
        MenuItem::Command { title, .. } | MenuItem::Standard { title, .. } => title.clone(),
        _ => String::new(),
    };
    // The first of the system's items of the application's menu.
    let standard = menus[0]
        .items
        .iter()
        .find(|item| matches!(item, MenuItem::Standard { .. }))
        .unwrap();
    vec![
        title(&menus[1].items[0]),
        title(standard),
        title(menus[0].items.last().unwrap()),
        menus[2].title.clone(),
    ]
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
    assert_eq!(menu_items(&fake, 0)[0], "Impostazioni… cmd+,");
    // The plugins' commands too, from their own catalogs.
    assert_eq!(
        menu_items(&fake, 2),
        [
            "Copia cmd+c",
            "Incolla cmd+v",
            "-",
            "Seleziona tutto cmd+a",
            "Cancella cmd+k"
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

/// The commands of the View menu's Appearance, and whether each one's
/// item is ticked.
fn appearances(fake: &Fake) -> Vec<(String, bool)> {
    let view = &fake.main_menu().unwrap().menus[3];
    let Some(menu) = view.items.iter().find_map(|item| match item {
        MenuItem::Submenu(menu) => Some(menu),
        _ => None,
    }) else {
        panic!("{view:?}");
    };
    menu.items
        .iter()
        .map(|item| match item {
            MenuItem::Command { title, command, .. } => {
                (title.clone(), fake.command_checked(command))
            }
            other => panic!("{other:?}"),
        })
        .collect()
}

/// The default background of `window`'s pane, once a frame shows it.
fn background(fake: &mut Fake, app: &mut App, window: WindowId) -> lokyr_vt_core::page::Rgb {
    fake.send(app, window, WindowEvent::Frame);
    app.pane(window).snapshot().colors().background
}

#[test]
fn the_appearance_follows_the_config_and_the_menu() {
    let (mut fake, mut app, path) = with_file(
        "appearance",
        "# Mine.\n[app]\nappearance = \"dark\" # at night\n",
    );
    let window = fake.windows().next().unwrap();
    let (light, dark) = (
        app.colors(Appearance::Light).background,
        app.colors(Appearance::Dark).background,
    );
    assert_eq!(fake.own_appearance(), Some(Appearance::Dark));
    assert_eq!(
        appearances(&fake),
        [
            ("Automatic".to_owned(), false),
            ("Light".to_owned(), false),
            ("Dark".to_owned(), true)
        ]
    );
    assert_eq!(background(&mut fake, &mut app, window), dark);
    // The system's change does not reach it.
    assert!(!fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Light)));
    assert_eq!(background(&mut fake, &mut app, window), dark);
    // The menu's choice applies, and goes into the file.
    let choose = |fake: &mut Fake, app: &mut App, command: &str| {
        fake.send_app(app, AppEvent::Command(command.to_owned()));
    };
    choose(&mut fake, &mut app, "app.appearance-light");
    assert_eq!(fake.own_appearance(), Some(Appearance::Light));
    assert_eq!(background(&mut fake, &mut app, window), light);
    assert_eq!(
        appearances(&fake)
            .iter()
            .map(|(_, ticked)| *ticked)
            .collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# Mine.\n[app]\nappearance = \"light\" # at night\n"
    );
    // The system's, which is light, then dark.
    choose(&mut fake, &mut app, "app.appearance-system");
    assert_eq!(fake.own_appearance(), None);
    assert!(fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark)));
    assert_eq!(background(&mut fake, &mut app, window), dark);
    assert!(fake.command_checked("app.appearance-system"));
    // The file that the menu wrote changes nothing as it reloads; set by
    // hand, it applies.
    change(&mut fake, &mut app);
    assert_eq!(fake.own_appearance(), None);
    std::fs::write(&path, "[app]\nappearance = \"light\"\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(fake.own_appearance(), Some(Appearance::Light));
    assert!(fake.command_checked("app.appearance-light"));
    assert!(!fake.command_checked("app.appearance-system"));
    // A file that does not read keeps the choice until the key changes.
    std::fs::write(&path, "[app\n").unwrap();
    choose(&mut fake, &mut app, "app.appearance-dark");
    assert_eq!(fake.own_appearance(), Some(Appearance::Dark));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "[app\n");
    std::fs::write(&path, "[app]\nappearance = \"light\"\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(fake.own_appearance(), Some(Appearance::Dark));
    std::fs::write(&path, "[app]\nappearance = \"system\"\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(fake.own_appearance(), None);
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
fn the_system_s_language_shows_unless_the_config_names_one() {
    let mut fake = Fake::default();
    fake.set_locales(&["de_CH", "it_CH"]);
    let mut first = app(Config::default(), quiet());
    fake.start(&mut first);
    assert_eq!(
        menu_titles(&fake),
        [
            "Nuova finestra",
            "Nascondi Lokyr",
            "Esci da Lokyr",
            "Composizione"
        ]
    );
    // A language that Lokyr lacks takes the system's.
    let mut fake = Fake::default();
    fake.set_locales(&["it_IT"]);
    let mut config = Config::default();
    config.app.language = "fr".to_owned();
    let mut second = app(config, quiet());
    fake.start(&mut second);
    assert_eq!(second.texts.language(), "it");
    // Without a language of Lokyr's, English.
    let (fake, _) = started();
    assert_eq!(
        menu_titles(&fake),
        ["New Window", "Hide Lokyr", "Quit Lokyr", "Edit"]
    );
}

#[test]
fn the_app_s_catalogs_match_their_source() {
    let pairs = [
        (
            include_str!("../../../resources/locales/en/app.ftl"),
            include_str!("../../../resources/locales/it/app.ftl"),
        ),
        (
            include_str!("../../../resources/locales/en/infoplist.ftl"),
            include_str!("../../../resources/locales/it/infoplist.ftl"),
        ),
    ];
    for (source, italian) in pairs {
        assert_eq!(l10n::check(source, italian), Ok(()));
    }
    // Every command's title is a message of the catalogs.
    let texts = Texts::new("it", &CATALOGS);
    for command in commands(&texts, &Plugins::new()).commands() {
        assert!(!command.title.starts_with("command-"), "{}", command.title);
    }
    // The built-in plugins' catalogs, each language's against English's,
    // whose titles are those that the commands register.
    let builtins: [Box<dyn plugins::Plugin>; 2] = [
        Box::new(lokyr_core_actions::CoreActions),
        Box::new(lokyr_links::Links),
    ];
    for plugin in &builtins {
        let catalogs = plugin.catalogs();
        let source = catalogs
            .iter()
            .find(|(language, _)| *language == l10n::SOURCE)
            .expect("an English catalog")
            .1;
        assert!(catalogs.len() > 1, "{catalogs:?}");
        for (_, other) in catalogs {
            assert_eq!(l10n::check(source, other), Ok(()));
        }
    }
    let (_, app) = started();
    let registered: Vec<(String, String)> = app
        .plugins
        .commands()
        .map(|command| (command.name.clone(), command.title.clone()))
        .collect();
    let english: Vec<(String, String)> = app
        .plugins
        .commands_in("en")
        .into_iter()
        .map(|command| (command.name, command.title))
        .collect();
    assert_eq!(english, registered);
    assert!(registered.iter().any(|(name, _)| name == "links.open"));
}

#[test]
fn two_windows_run_two_shells() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell("echo pid:$$; read line"));
    fake.start(&mut app);
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let windows: Vec<WindowId> = fake.windows().collect();
    assert_eq!(windows.len(), 2);
    assert_eq!(app.workspace.windows().count(), 2);
    let pid = |app: &App, window| {
        let line = row(app, window, 0);
        line.strip_prefix("pid:").map(str::to_owned)
    };
    run_until(&mut fake, &mut app, |_, app| {
        windows.iter().all(|&window| pid(app, window).is_some())
    });
    assert_ne!(pid(&app, windows[0]), pid(&app, windows[1]));
    // Closing one leaves the other, with its pane.
    fake.send(&mut app, windows[0], WindowEvent::CloseRequested);
    assert_eq!(fake.windows().collect::<Vec<_>>(), [windows[1]]);
    assert_eq!(app.workspace.windows().count(), 1);
    assert_eq!(app.workspace.panes().count(), 1);
    assert!(!fake.has_quit());
}

/// A shell that waits for a line, as at its prompt: no program runs.
fn idle() -> Program {
    shell("echo ready; read line")
}

/// The app started, its window running `program`, once it shows
/// `ready`, focused.
fn started_running(program: Program) -> (Fake, App, WindowId) {
    let (mut fake, mut app, window) = started_with(program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    (fake, app, window)
}

#[test]
fn quit_from_the_menu_or_from_the_system_quits() {
    let (mut fake, mut app, _) = started_running(idle());
    fake.send_app(&mut app, AppEvent::Command(QUIT.to_owned()));
    assert!(fake.has_quit());
    assert!(fake.alerts().is_empty());

    let (mut fake, mut app, _) = started_running(idle());
    fake.send_app(&mut app, AppEvent::QuitRequested);
    assert!(fake.has_quit());
    assert_eq!(fake.quit_refusals(), 0);
}

#[test]
fn a_window_s_state_follows_it_from_its_opening_to_its_closing() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(idle());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    // Its window and pane in the workspace.
    let state = &app.windows[&window];
    let (opened, pane) = (state.window, state.pane);
    assert_eq!(app.workspace.active_pane(opened), Some(pane));
    assert!(!state.focused);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert!(app.windows[&window].focused);
    // A frame of its size and scale, into its surface, which the fake
    // platform does not give.
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    let shown = |app: &App| {
        let shown = app.windows[&window].shown.as_ref().expect("a frame");
        (shown.pixels, shown.scale)
    };
    let frames = |app: &App| {
        let surface = app.windows[&window].surface.as_ref().unwrap();
        surface.frames.load(std::sync::atomic::Ordering::Relaxed)
    };
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(frames(&app), 1);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(shown(&app), ((800, 600), 2.0));
    assert_eq!(frames(&app), 2);
    // Closed, with its state and its workspace's window.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert!(app.windows.is_empty());
    assert!(app.workspace.window(opened).is_none());
    assert!(app.workspace.pane(pane).is_none());
}

#[test]
fn closing_the_last_window_quits() {
    let (mut fake, mut app, window) = started_running(idle());
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().count(), 0);
    assert!(fake.has_quit());
}

#[test]
fn closing_a_window_whose_program_runs_asks_first() {
    let (mut fake, mut app, window) = started_running(shell("echo ready; exec sleep 30"));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).running().as_deref() == Some("sleep")
    });
    // The close button, then ⌘W: one question, cancelled.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    assert_eq!(fake.alerts().len(), 1);
    let open = &fake.alerts()[0];
    assert_eq!(open.window, Some(window));
    assert_eq!(open.alert.message, app.texts.text("close-running-message"));
    assert_eq!(
        open.alert.detail,
        app.texts
            .text_with("close-running-detail", &[("program", Arg::Text("sleep"))])
    );
    assert_eq!(
        open.alert.buttons,
        [
            app.texts.text("close-running-close"),
            app.texts.text("close-running-cancel")
        ]
    );
    let id = open.id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(fake.windows().count(), 1);
    // Confirmed, it closes, and the app quits with its last window.
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    assert_eq!(fake.windows().count(), 0);
    assert!(fake.has_quit());
}

#[test]
fn closing_a_window_asks_for_a_job_behind_the_shell() {
    // A job in the background, behind a shell that reads, as at its
    // prompt.
    let (mut fake, mut app, window) = started_running(shell("sleep 30 & echo ready; read line"));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).running().as_deref() == Some("sleep")
    });
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    assert_eq!(fake.alerts().len(), 1);
    assert_eq!(
        fake.alerts()[0].alert.detail,
        app.texts
            .text_with("close-running-detail", &[("program", Arg::Text("sleep"))])
    );
}

#[test]
fn quitting_while_programs_run_asks_and_the_system_waits_for_the_answer() {
    let (mut fake, mut app, busy) = started_running(shell("echo ready; exec sleep 30"));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(busy).running().as_deref() == Some("sleep")
    });
    // Another window, whose shell waits: it runs nothing.
    app.program = idle();
    app.run(&mut fake, NEW_WINDOW, Args::new());
    // ⌘Q: a question without a window, cancelled.
    fake.send(&mut app, busy, command_key(KeyCode::KeyQ, "q"));
    assert_eq!(fake.alerts().len(), 1);
    let open = &fake.alerts()[0];
    assert_eq!(open.window, None);
    assert_eq!(
        open.alert.detail,
        app.texts.text_with(
            "quit-running-detail",
            &[
                ("windows", Arg::Number(1)),
                ("programs", Arg::Text("sleep"))
            ]
        )
    );
    let id = open.id;
    fake.answer_alert(&mut app, id, 1);
    assert!(!fake.has_quit());
    assert_eq!(fake.quit_refusals(), 0);
    // The system's request: cancelled, it is refused; a second one
    // while the question is open waits for the same answer.
    fake.send_app(&mut app, AppEvent::QuitRequested);
    fake.send_app(&mut app, AppEvent::QuitRequested);
    assert_eq!(fake.alerts().len(), 1);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(fake.quit_refusals(), 1);
    assert!(!fake.has_quit());
    // The system's request while ⌘Q's question is open takes its
    // answer.
    fake.send(&mut app, busy, command_key(KeyCode::KeyQ, "q"));
    fake.send_app(&mut app, AppEvent::QuitRequested);
    assert_eq!(fake.alerts().len(), 1);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(fake.quit_refusals(), 2);
    // Confirmed, the app quits.
    fake.send_app(&mut app, AppEvent::QuitRequested);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    assert!(fake.has_quit());
    assert_eq!(fake.quit_refusals(), 2);
}

#[test]
fn programs_of_several_windows_take_one_question() {
    let (mut fake, mut app, first) = started_running(shell("echo ready; exec sleep 30"));
    app.run(&mut fake, NEW_WINDOW, Args::new());
    app.program = shell("echo ready; exec cat");
    app.run(&mut fake, NEW_WINDOW, Args::new());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows
            .values()
            .filter(|state| {
                app.workspace
                    .pane(state.pane)
                    .and_then(Pane::running)
                    .is_some()
            })
            .count()
            == 3
    });
    fake.send(&mut app, first, command_key(KeyCode::KeyQ, "q"));
    assert_eq!(
        fake.alerts()[0].alert.detail,
        app.texts.text_with(
            "quit-running-detail",
            &[
                ("windows", Arg::Number(3)),
                ("programs", Arg::Text("sleep, cat"))
            ]
        )
    );
    // A program whose name is not known is "a program".
    assert_eq!(
        app.program_name(String::new()),
        app.texts.text("program-unknown")
    );
}

#[test]
fn a_window_without_a_layer_draws_nothing() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    assert!(app.windows[&window].surface.is_none());
    let size = WindowSize::new(
        Size {
            width: 900.0,
            height: 600.0,
        },
        1.0,
    );
    assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
    assert!(fake.send(&mut app, window, WindowEvent::ScaleChanged(size)));
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn a_wake_asks_for_the_frames_of_the_panes_that_woke_it() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    if fake.window(window).unwrap().frame_requested {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    // A wake of nothing that changed asks for nothing.
    fake.send_app(&mut app, AppEvent::Wake);
    assert!(!fake.window(window).unwrap().frame_requested);
    let pane = app.windows[&window].pane;
    app.pane_wakes[&pane].wake();
    fake.send_app(&mut app, AppEvent::Wake);
    assert!(fake.window(window).unwrap().frame_requested);
    assert!(fake.send(&mut app, window, WindowEvent::Frame));
    assert!(!fake.window(window).unwrap().frame_requested);
}

/// The app started, its window focused and drawn once.
fn focused() -> (Fake, App, WindowId) {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    assert!(fake.send(&mut app, window, WindowEvent::Focused(true)));
    assert!(fake.window(window).unwrap().frame_requested);
    assert!(fake.send(&mut app, window, WindowEvent::Frame));
    (fake, app, window)
}

#[test]
fn the_focused_window_s_cursor_blinks_on_its_timer() {
    let (mut fake, mut app, window) = focused();
    let timers: Vec<_> = fake.timers().collect();
    assert_eq!(timers.len(), 1);
    let (timer, at) = timers[0];
    assert_eq!(app.blink_timer, Some((timer, at)));
    // The timer asks the window for the frame of the change.
    assert_eq!(fake.fire_timers(&mut app, at), 1);
    assert_eq!(app.blink_timer, None);
    assert!(fake.window(window).unwrap().frame_requested);
    // Off, then on, as the clock goes.
    let now = at;
    assert_eq!(
        app.cursor(&mut fake, window, now).map(|(shape, _)| shape),
        None
    );
    assert_eq!(
        app.cursor(&mut fake, window, now + cursor::BLINK_INTERVAL)
            .map(|(shape, _)| shape),
        Some(CursorShape::Block)
    );
    assert_eq!(fake.timers().count(), 1);
}

#[test]
fn blinking_stops_on_after_its_timeout() {
    let (mut fake, mut app, window) = focused();
    let later = Instant::now() + cursor::BLINK_TIMEOUT;
    assert_eq!(
        app.cursor(&mut fake, window, later).map(|(shape, _)| shape),
        Some(CursorShape::Block)
    );
    assert!(!app.windows[&window].blinking);
    // Its last timer asks for no frame.
    let fired = fake.fire_timers(&mut app, later + cursor::BLINK_TIMEOUT);
    assert_eq!(fired, 1);
    assert!(!fake.window(window).unwrap().frame_requested);
    assert_eq!(fake.timers().count(), 0);
}

#[test]
fn a_window_without_the_focus_shows_a_hollow_block_that_does_not_blink() {
    let (mut fake, mut app, window) = focused();
    assert!(fake.send(&mut app, window, WindowEvent::Focused(false)));
    let now = Instant::now();
    for millis in [0, 600, 1200] {
        let at = now + std::time::Duration::from_millis(millis);
        assert_eq!(
            app.cursor(&mut fake, window, at).map(|(shape, _)| shape),
            Some(CursorShape::Hollow)
        );
    }
    assert!(!app.windows[&window].blinking);
}

#[test]
fn a_cursor_on_its_layer_blinks_without_a_timer_but_its_end() {
    let mut fake = Fake::default();
    let mut config = Config::default();
    config.cursor.smooth_blink = cursor::SmoothBlink::Breathing;
    let mut app = app(config, quiet());
    app.layers = true;
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, WindowEvent::Frame);
    let now = Instant::now();
    let (shape, blink) = app.cursor(&mut fake, window, now).unwrap();
    assert_eq!(shape, CursorShape::Block);
    // Breathing's, from the clock's start, which the layer runs.
    let blink = blink.expect("a blink");
    let millis = Duration::from_millis;
    assert_eq!(
        (blink.visible, blink.fade_out, blink.hidden, blink.fade_in),
        (millis(500), millis(900), millis(500), millis(900))
    );
    assert_eq!(
        (blink.fade_out_curve, blink.fade_in_curve),
        (Easing::EaseInOut, Easing::EaseInOut)
    );
    let since = app.windows[&window].blink.since();
    assert_eq!(blink.start, since);
    // One timer alone, of the blinking's end.
    let timers: Vec<Instant> = fake.timers().map(|(_, at)| at).collect();
    assert_eq!(timers, [since + cursor::BLINK_TIMEOUT]);
    // At its end, a frame, and the cursor steady, without a timer.
    assert_eq!(fake.fire_timers(&mut app, timers[0]), 1);
    assert!(fake.window(window).unwrap().frame_requested);
    let (_, blink) = app.cursor(&mut fake, window, timers[0]).unwrap();
    assert_eq!(blink, None);
    assert_eq!(fake.timers().count(), 0);
    // Off switches, on and off for the interval each.
    app.config.cursor.smooth_blink = cursor::SmoothBlink::Off;
    app.windows
        .get_mut(&window)
        .unwrap()
        .blink
        .restart(Instant::now());
    let (_, blink) = app.cursor(&mut fake, window, Instant::now()).unwrap();
    let blink = blink.expect("a blink");
    assert_eq!((blink.visible, blink.hidden), (millis(500), millis(500)));
    assert_eq!(
        (blink.fade_out, blink.fade_in),
        (Duration::ZERO, Duration::ZERO)
    );
    // Without the focus, a hollow block, steady.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let (shape, blink) = app.cursor(&mut fake, window, Instant::now()).unwrap();
    assert_eq!((shape, blink), (CursorShape::Hollow, None));
}

#[test]
fn the_output_bench_of_a_program_that_cannot_start_ends_failing() {
    let mut fake = Fake::default();
    let program = Program::command("/no/such/program", Vec::<String>::new());
    let mut bench = app(Config::default(), quiet()).with_output_bench(program, None);
    fake.start(&mut bench);
    assert!(fake.has_quit());
    assert!(bench.output_bench.is_none());
    assert!(crate::BENCH_FAILED.load(Ordering::Relaxed));
}

#[test]
fn the_output_bench_s_window_takes_the_config_s_grid_not_the_frame_kept() {
    let mut config = unpadded();
    config.window.remember_frame = true;
    let mut fake = Fake::default();
    let mut bench = App::new(None, config, None).with_output_bench(quiet(), None);
    fake.start(&mut bench);
    let window = fake.windows().next().unwrap();
    let shown = fake.window(window).unwrap();
    assert!(!shown.remember && !shown.restore);
    // 200 columns of 4 points and 62 rows of 8.
    assert_eq!(
        shown.size.points,
        Size {
            width: 800.0,
            height: 496.0
        }
    );
    assert!(!fake.has_quit());
}

#[test]
fn the_latency_bench_of_the_user_s_keys_tells_them_as_its_last_window_closes() {
    let mut fake = Fake::default();
    let mut bench = app(Config::default(), quiet()).with_latency_bench(0, false);
    fake.start(&mut bench);
    let window = fake.windows().next().unwrap();
    bench.close(&mut fake, window);
    assert!(bench.latency_bench.is_none(), "its report printed");
    assert!(fake.has_quit());
}

#[test]
fn the_benches_give_up_without_a_frame_on_the_display() {
    let start = Instant::now();
    let mut fake = Fake::default();
    let mut typing = app(Config::default(), quiet()).with_latency_bench(10, false);
    fake.start(&mut typing);
    assert!(fake.timers().any(|(_, at)| at >= start + BENCH_DEADLINE));
    typing.give_up_benches(&mut fake);
    assert!(fake.has_quit());
    // The user's own keys go on.
    let mut fake = Fake::default();
    let mut users = app(Config::default(), quiet()).with_latency_bench(0, false);
    fake.start(&mut users);
    users.give_up_benches(&mut fake);
    assert!(!fake.has_quit());
}

#[test]
fn a_new_font_has_the_layers_draw_their_cells_again() {
    let mut drawing = Drawing::new(FontConfig::default()).unwrap();
    assert_eq!(drawing.fonts, 1);
    drawing.set_font(FontConfig::default()).unwrap();
    assert_eq!(drawing.fonts, 2);
}

#[test]
fn blinking_text_on_its_layer_blinks_without_a_timer_but_its_end() {
    // A cursor that does not blink, and a smooth blink, which the text
    // does not take.
    let mut config = Config::default();
    config.cursor.blink = false;
    config.cursor.smooth_blink = cursor::SmoothBlink::Breathing;
    let mut fake = Fake::default();
    let mut app = app(config, shell("printf '\\033[5mblink\\033[0m'; sleep 10"));
    app.layers = true;
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "blink");
    fake.send(&mut app, window, WindowEvent::Frame);
    // The grid hides it, and its layer switches it at the interval, from
    // the clock's start.
    let (shown, blink) = app.text_blink(&mut fake, window, Instant::now());
    assert!(!shown);
    let blink = blink.expect("a blink");
    let since = app.windows[&window].blink.since();
    assert_eq!(blink.start, since);
    let interval = cursor::BLINK_INTERVAL;
    assert_eq!((blink.visible, blink.hidden), (interval, interval));
    assert_eq!(
        (blink.fade_out, blink.fade_in),
        (Duration::ZERO, Duration::ZERO)
    );
    // One timer alone, of the blinking's end; at its end, a frame, and
    // the text in the grid, steady.
    let (timer, at) = app.blink_timer.expect("the blinking's timer");
    assert_eq!(at, since + cursor::BLINK_TIMEOUT);
    fake.send_app(&mut app, AppEvent::Timer(timer));
    assert!(fake.window(window).unwrap().frame_requested);
    assert_eq!(app.text_blink(&mut fake, window, at), (true, None));
    assert_eq!(app.blink_timer, None);
    // Without the focus, steady.
    app.windows
        .get_mut(&window)
        .unwrap()
        .blink
        .restart(Instant::now());
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let unfocused = app.text_blink(&mut fake, window, Instant::now());
    assert_eq!(unfocused, (true, None));
}

#[test]
fn the_configuration_gives_the_cursor_s_shape_and_blinking() {
    let mut fake = Fake::default();
    let mut config = Config::default();
    config.cursor = CursorConfig {
        shape: cursor::Shape::Bar,
        blink: false,
        ..CursorConfig::default()
    };
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let half = Instant::now() + cursor::BLINK_INTERVAL + cursor::BLINK_INTERVAL / 2;
    assert_eq!(
        app.cursor(&mut fake, window, half).map(|(shape, _)| shape),
        Some(CursorShape::Bar)
    );
    assert_eq!(fake.timers().count(), 0);
}

/// An app started with its config file, in a directory of its own for
/// `test`, which holds `text`.
fn with_file(test: &str, text: &str) -> (Fake, App, PathBuf) {
    let directory = std::env::temp_dir().join(format!("lokyr-app-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, text).unwrap();
    let config = Config::read(&path).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    assert!(app.config_file.as_ref().unwrap().watcher.is_some());
    (fake, app, path)
}

/// Tells the app that its file changed, as its watch does, and lets
/// the changes settle.
fn change(fake: &mut Fake, app: &mut App) {
    let file = app.config_file.as_ref().unwrap();
    file.changed.store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
    // Another change meanwhile reads the file once.
    app.config_file
        .as_ref()
        .unwrap()
        .changed
        .store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
    let reading = app.config_file.as_ref().unwrap().reading;
    assert!(reading.is_some());
    // The one timer, the reading's, as the window has no focus to blink.
    let timers: Vec<_> = fake.timers().collect();
    assert_eq!(timers.len(), 1);
    assert_eq!(Some(timers[0].0), reading);
    fake.fire_timers(app, timers[0].1);
    assert_eq!(app.config_file.as_ref().unwrap().reading, None);
}

#[test]
fn a_changed_config_applies_whole_once_its_changes_settle() {
    let (mut fake, mut app, path) = with_file("valid", "[cursor]\nshape = \"bar\"\n");
    assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(fake.option_as_meta(), (false, false));
    std::fs::write(
        &path,
        "[cursor]\nshape = \"hollow\"\nblink = false\n[keyboard]\noption-as-meta = \"right\"\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Hollow);
    assert!(!app.config.cursor.blink);
    assert!(fake.window(window).unwrap().frame_requested);
    // The platform learns which Option acts as Meta.
    assert_eq!(fake.option_as_meta(), (false, true));
    // A file that goes gives the defaults.
    std::fs::remove_file(&path).unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config, Config::default());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_saved_config_reloads_through_the_watch() {
    let (mut fake, mut app, path) = with_file("watched", "[cursor]\nshape = \"bar\"\n");
    std::fs::write(&path, "[cursor]\nshape = \"underline\"\n").unwrap();
    // The watch's thread wakes the app, and the reading follows.
    run_until(&mut fake, &mut app, |_, app| {
        app.config.cursor.shape == cursor::Shape::Underline
    });
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// A directory of its own for `test`, with two icon's variants: the
/// default one and `laguz`.
fn icons(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-icons-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    for variant in [AppConfig::DEFAULT_ICON, "laguz"] {
        std::fs::write(directory.join(format!("lokyr-{variant}.icns")), "").unwrap();
    }
    directory
}

/// The default config, with the icon's variant `icon`.
fn with_icon(icon: &str) -> Config {
    let mut config = Config::default();
    config.app.icon = icon.to_owned();
    config
}

#[test]
fn the_config_s_icon_shows_as_it_applies() {
    let directory = icons("bundled");
    let mut fake = Fake::default();
    let mut app =
        app(Config::default(), quiet()).with_icons(Some(Icons::new(directory.clone(), true)));
    fake.start(&mut app);
    // The bundle's own icon is the default variant's.
    assert_eq!(fake.app_icons(), []);
    let laguz = Some(directory.join("lokyr-laguz.icns"));
    app.apply(&mut fake, with_icon("laguz"));
    assert_eq!(fake.app_icons(), std::slice::from_ref(&laguz));
    // Another key leaves it.
    let mut other = with_icon("laguz");
    other.cursor.blink = false;
    app.apply(&mut fake, other);
    assert_eq!(fake.app_icons().len(), 1);
    // A name that is no variant's gives the default, the bundle's own.
    app.apply(&mut fake, with_icon("lagus"));
    assert_eq!(fake.app_icons(), [laguz.clone(), None]);
    // The default, which shows already, changes nothing.
    app.apply(&mut fake, with_icon(AppConfig::DEFAULT_ICON));
    assert_eq!(fake.app_icons().len(), 2);
    // A variant whose file cannot be read keeps the icon that shows.
    std::os::unix::fs::symlink(directory.join("gone"), directory.join("lokyr-broken.icns"))
        .unwrap();
    app.apply(&mut fake, with_icon("broken"));
    assert_eq!(fake.app_icons().len(), 2);
    app.apply(&mut fake, with_icon("laguz"));
    assert_eq!(fake.app_icons(), [laguz.clone(), None, laguz]);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn outside_the_bundle_the_icon_shows_from_its_file_at_the_start() {
    let directory = icons("tree");
    for (config, variant) in [
        (Config::default(), AppConfig::DEFAULT_ICON),
        (with_icon("laguz"), "laguz"),
    ] {
        let mut fake = Fake::default();
        let mut app = app(config, quiet()).with_icons(Some(Icons::new(directory.clone(), false)));
        fake.start(&mut app);
        let file = directory.join(format!("lokyr-{variant}.icns"));
        assert_eq!(fake.app_icons(), [Some(file)]);
    }
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_link_s_target_that_goes_and_comes_back_reloads() {
    use std::os::unix::fs::symlink;

    let home = std::env::temp_dir().join(format!("lokyr-app-checkout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let (dotfiles, directory) = (home.join("dotfiles"), home.join("lokyr"));
    std::fs::create_dir_all(&dotfiles).unwrap();
    std::fs::create_dir_all(&directory).unwrap();
    let target = dotfiles.join("lokyr.toml");
    std::fs::write(&target, "[cursor]\nshape = \"bar\"\n").unwrap();
    let path = directory.join("config.toml");
    symlink(&target, &path).unwrap();
    let config = Config::read(&path).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
    // As a checkout of the dotfiles removes the file, and writes it
    // again, changed.
    std::fs::remove_file(&target).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.config.cursor.shape == cursor::Shape::Block
    });
    std::fs::write(&target, "[cursor]\nshape = \"underline\"\n").unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.config.cursor.shape == cursor::Shape::Underline
    });
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_link_pointed_elsewhere_reloads_at_its_new_target() {
    use std::os::unix::fs::symlink;

    let home = std::env::temp_dir().join(format!("lokyr-app-links-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    // The config's link to a file of the dotfiles, pointed to another
    // of the same directory; then its directory's link, as GNU Stow
    // folds one, pointed to another package.
    for folded in [false, true] {
        let root = home.join(if folded { "folded" } else { "file" });
        let (one, two) = if folded {
            (root.join("one"), root.join("two"))
        } else {
            (root.join("dotfiles"), root.join("dotfiles"))
        };
        let target = |package: &std::path::Path, name: &str| {
            if folded {
                package.to_owned()
            } else {
                package.join(name)
            }
        };
        let file = |package: &std::path::Path, name: &str| {
            if folded {
                package.join("config.toml")
            } else {
                package.join(name)
            }
        };
        for package in [&one, &two] {
            std::fs::create_dir_all(package).unwrap();
        }
        std::fs::write(file(&one, "one.toml"), "[cursor]\nshape = \"bar\"\n").unwrap();
        std::fs::write(file(&two, "two.toml"), "[cursor]\nshape = \"underline\"\n").unwrap();
        let directory = root.join("lokyr");
        let path = directory.join("config.toml");
        let linked = if folded {
            directory.clone()
        } else {
            std::fs::create_dir_all(&directory).unwrap();
            path.clone()
        };
        symlink(target(&one, "one.toml"), &linked).unwrap();
        let config = Config::read(&path).unwrap();
        let mut fake = Fake::default();
        let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
        fake.start(&mut app);
        assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
        std::fs::remove_file(&linked).unwrap();
        symlink(target(&two, "two.toml"), &linked).unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Underline
        });
        // A later edit of the new target reloads too.
        std::fs::write(file(&two, "two.toml"), "[cursor]\nshape = \"hollow\"\n").unwrap();
        run_until(&mut fake, &mut app, |_, app| {
            app.config.cursor.shape == cursor::Shape::Hollow
        });
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_config_s_directory_is_made_at_the_start_and_watched() {
    let home = std::env::temp_dir().join(format!("lokyr-app-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // A home without `.config`.
    let path = home.join(".config").join("lokyr").join("config.toml");
    let mut fake = Fake::default();
    let mut app = App::new(None, Config::default(), Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    assert!(path.parent().unwrap().is_dir());
    assert!(app.config_file.as_ref().unwrap().watcher.is_some());
    std::fs::write(&path, "[cursor]\nshape = \"hollow\"\n").unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.config.cursor.shape == cursor::Shape::Hollow
    });
    // A directory that goes gives the defaults.
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.config == Config::default()
    });
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn each_change_puts_the_reading_off() {
    let (mut fake, mut app, path) = with_file("settle", "");
    let changed = |fake: &mut Fake, app: &mut App| {
        let file = app.config_file.as_ref().unwrap();
        file.changed.store(true, Ordering::Release);
        fake.send_app(app, AppEvent::Wake);
        app.config_file.as_ref().unwrap().reading.unwrap()
    };
    let first = changed(&mut fake, &mut app);
    let second = changed(&mut fake, &mut app);
    // The first timer is gone, and the one left counts from the last.
    assert_ne!(first, second);
    let timers: Vec<_> = fake.timers().map(|(timer, _)| timer).collect();
    assert_eq!(timers, [second]);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn an_invalid_config_changes_nothing() {
    let (mut fake, mut app, path) = with_file("invalid", "[cursor]\nshape = \"bar\"\n");
    std::fs::write(&path, "[cursor]\nshape = \"beam\"\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// Runs the app, as the main thread's loop would, handing it the wakes
/// of its programs, the timers that come due and the frames that it asks
/// for, until `done` holds; fails after a while.
fn run_until(fake: &mut Fake, app: &mut App, mut done: impl FnMut(&Fake, &App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(fake, app) {
        assert!(Instant::now() < deadline, "not done in time");
        fake.deliver_wake(app);
        fake.fire_timers(app, Instant::now());
        let asked: Vec<WindowId> = fake
            .windows()
            .filter(|&window| fake.window(window).unwrap().frame_requested)
            .collect();
        for window in asked {
            fake.send(app, window, WindowEvent::Frame);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The text of row `row` of a window's screen.
fn row(app: &App, window: WindowId, row: u16) -> String {
    let snapshot = app.pane(window).snapshot();
    let line = snapshot.line(row);
    line.cells()
        .iter()
        // A wide character's second cell holds nothing of it.
        .filter(|cell| cell.width() != lokyr_vt_core::page::Width::SpacerTail)
        .map(|cell| match cell.content() {
            lokyr_vt_core::page::Content::Char(c) | lokyr_vt_core::page::Content::Grapheme(c) => c,
            _ => ' ',
        })
        .map(|c| if c == '\0' { ' ' } else { c })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn shell(script: &str) -> Program {
    Program::command("/bin/sh", ["-c", script])
}

/// A directory of the test's own, made empty, as the system names it,
/// which `pwd` tells, its links resolved.
fn real_directory(test: &str) -> PathBuf {
    std::fs::canonicalize(directory(test)).unwrap()
}

/// The directory that `window`'s shell told as it started, with `pwd`.
fn started_in(fake: &mut Fake, app: &mut App, window: WindowId) -> String {
    run_until(fake, app, |_, app| !row(app, window, 0).is_empty());
    row(app, window, 0)
}

#[test]
fn the_first_window_starts_where_start_directory_says() {
    let base = real_directory("start-directory");
    let (last, path, state) = (base.join("last"), base.join("path"), base.join("state"));
    for made in [&last, &path] {
        std::fs::create_dir(made).unwrap();
    }
    let home = std::env::home_dir().unwrap().display().to_string();
    let shown = |at: &std::path::Path| at.display().to_string();
    let gone = base.join("gone").display().to_string();
    for (directory, kept, expected) in [
        (StartDirectory::Home, true, home.clone()),
        (StartDirectory::Last, true, shown(&last)),
        (StartDirectory::Last, false, home.clone()),
        (StartDirectory::Path(shown(&path)), true, shown(&path)),
        (StartDirectory::Path(gone), true, home.clone()),
    ] {
        let _ = std::fs::remove_dir_all(&state);
        if kept {
            state::keep_last_directory(&state, &last).unwrap();
        }
        let mut config = Config::default();
        config.window.start_directory = directory.clone();
        let mut fake = Fake::default();
        let mut app = app(config, shell("pwd; exec sleep 10")).with_state(Some(state.clone()));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        assert_eq!(
            started_in(&mut fake, &mut app, window),
            expected,
            "{directory:?}"
        );
    }
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_new_window_starts_where_new_directory_says_and_the_last_is_kept() {
    let base = real_directory("new-directory");
    let (inherited, path, state) = (base.join("inherit"), base.join("path"), base.join("state"));
    for made in [&inherited, &path] {
        std::fs::create_dir(made).unwrap();
    }
    let shown = |at: &std::path::Path| at.display().to_string();
    let home = std::env::home_dir().unwrap().display().to_string();
    // Each window's shell tells where it started, then goes on in its
    // own directory, or the first one's, of its program, `sleep`.
    let script = format!(
        "pwd; [ \"$PWD\" = {home:?} ] && cd {:?}; exec sleep 10",
        shown(&inherited)
    );
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell(&script)).with_state(Some(state.clone()));
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    assert_eq!(started_in(&mut fake, &mut app, first), home);
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(first).working_directory().as_deref() == Some(inherited.as_path())
    });
    // The window that had the focus last, as the app is not active.
    fake.send(&mut app, first, WindowEvent::Focused(true));
    fake.send(&mut app, first, WindowEvent::Focused(false));
    let opened = |fake: &mut Fake, app: &mut App| {
        let before: Vec<WindowId> = fake.windows().collect();
        fake.send_app(app, AppEvent::Command(NEW_WINDOW.to_owned()));
        let window = fake
            .windows()
            .find(|window| !before.contains(window))
            .unwrap();
        (window, started_in(fake, app, window))
    };
    // Of the program in the window that had the focus, by default.
    let (second, directory) = opened(&mut fake, &mut app);
    assert_eq!(directory, shown(&inherited));
    app.config.window.new_directory = NewDirectory::Home;
    assert_eq!(opened(&mut fake, &mut app).1, home);
    app.config.window.new_directory = NewDirectory::Path(shown(&path));
    let (third, directory) = opened(&mut fake, &mut app);
    assert_eq!(directory, shown(&path));
    // A window that closes keeps its directory as the last one's; the
    // one in front, as the app quits, though no window has the focus,
    // as the app is not active.
    app.close(&mut fake, second);
    assert_eq!(state::last_directory(&state), Some(inherited.clone()));
    fake.send(&mut app, third, WindowEvent::Focused(true));
    fake.send(&mut app, third, WindowEvent::Focused(false));
    assert_eq!(fake.focused_window(), None);
    app.quit(&mut fake);
    assert!(fake.has_quit());
    assert_eq!(state::last_directory(&state), Some(path));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_window_whose_program_ended_keeps_the_directory_that_its_title_saw() {
    let base = real_directory("ended-directory");
    let state = base.join("state");
    let script = format!("cd {:?}; echo here; sleep 0.5; exit 1", base.display());
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell(&script)).with_state(Some(state.clone()));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].directory.as_deref() == Some(base.as_path())
    });
    run_until(&mut fake, &mut app, |_, app| app.pane(window).is_kept());
    assert_eq!(app.pane(window).working_directory(), None);
    app.close(&mut fake, window);
    assert_eq!(state::last_directory(&state), Some(base.clone()));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn secure_keyboard_entry_follows_a_password_s_prompt_and_the_menu() {
    let script = "stty -echo; echo ready; read a; stty echo; stty -echo; printf 'Password:'; \
        read secret; stty echo; echo done; read b; stty -echo; read c; stty echo; read d";
    let (mut fake, mut app, window) = started_with(shell(script));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "ready");
    let on = |fake: &Fake| fake.secure_input().0;
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    // Before the prompt, `read` with its echo off, in canonical mode, is
    // a password's too.
    assert!(on(&fake));
    // At the prompt, which its output tells, on, of a lock in the title.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 1).starts_with("Password:")
    });
    assert!(on(&fake));
    assert!(title(&fake).starts_with(SECURE_MARK), "{}", title(&fake));
    // Without the focus, off, and the lock goes; back, on again.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    assert!(!on(&fake) && !title(&fake).starts_with(SECURE_MARK));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert!(on(&fake) && title(&fake).starts_with(SECURE_MARK));
    // After it, as its output tells, off.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |fake, app| {
        (0..4).any(|r| row(app, window, r).ends_with("done")) && !on(fake)
    });
    assert!(!title(&fake).starts_with(SECURE_MARK));
    // A prompt that no output tells, as after a silent `stty -echo`, is
    // seen before the next key.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| app.pane(window).hides_input());
    assert!(!on(&fake));
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    assert!(on(&fake));
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, WindowEvent::Focused(false));
    // The menu's item turns it on for good, ticked, and off again.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        !app.pane(window).hides_input()
    });
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    assert!(!on(&fake));
    assert!(!fake.command_checked(SECURE_INPUT));
    fake.send_app(&mut app, AppEvent::Command(SECURE_INPUT.to_owned()));
    assert!(on(&fake) && fake.command_checked(SECURE_INPUT));
    // Never while another app is active, as no window has the focus.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    assert!(!on(&fake));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert!(on(&fake));
    fake.send_app(&mut app, AppEvent::Command(SECURE_INPUT.to_owned()));
    assert!(!on(&fake) && !fake.command_checked(SECURE_INPUT));
    // Each change once.
    assert_eq!(fake.secure_input().1, 14);
}

#[test]
fn secure_keyboard_entry_goes_with_the_focused_window_that_closes() {
    let config = Config::parse(
        "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send_app(&mut app, AppEvent::Command(SECURE_INPUT.to_owned()));
    assert!(fake.secure_input().0);
    // The last window, which a global key keeps the app open without:
    // no window takes the focus.
    app.close(&mut fake, window);
    assert!(!fake.has_quit());
    assert_eq!(fake.secure_input(), (false, 2));
}

#[test]
fn a_program_that_asks_learns_of_the_focus() {
    let (mut fake, mut app, window) = started_with(shell(
        "stty raw -echo; printf '\\033[?1004h'; echo ready; cat -v",
    ));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, WindowEvent::Focused(false));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[I^[[O"));
}

#[test]
fn a_window_that_does_not_show_draws_nothing_until_it_shows() {
    let (mut fake, mut app, window) = started_with(shell(
        "stty raw -echo; echo ready; head -c 1 >/dev/null; echo more; cat",
    ));
    fake.send(&mut app, window, WindowEvent::Frame);
    // Hidden and shown again without a frame asked meanwhile, it draws
    // anew, as it let its drawables go.
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    fake.send(&mut app, window, WindowEvent::Occluded(false));
    assert!(fake.send(&mut app, window, WindowEvent::Frame));
    assert!(!fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    fake.send(&mut app, window, WindowEvent::Occluded(false));
    assert!(fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Frame);
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    assert!(app.windows[&window].occluded);
    // Output asks for a frame, which waits.
    fake.send(&mut app, window, key());
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let shown = fake.window(window).unwrap();
    assert!(!shown.frame_requested && shown.frame_held);
    // Shown again, it draws.
    fake.send(&mut app, window, WindowEvent::Occluded(false));
    assert!(!app.windows[&window].occluded);
    assert!(fake.window(window).unwrap().frame_requested);
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "more"));
}

#[test]
fn a_window_opens_of_its_cells_and_the_first_where_the_last_was() {
    let mut config = unpadded();
    config.window.columns = 80.try_into().unwrap();
    config.window.rows = 24.try_into().unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    let shown = fake.window(first).unwrap();
    assert_eq!(
        shown.size.points,
        Size {
            width: 320.0,
            height: 192.0
        }
    );
    assert!(shown.remember && shown.restore && !shown.restore_size);
    // The next keeps its frame, and opens of the kept size, or of the
    // cells without one.
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let second = fake.windows().find(|&window| window != first).unwrap();
    let shown = fake.window(second).unwrap();
    assert!(shown.remember && !shown.restore && shown.restore_size);
    // Without the key, none keeps it.
    let mut config = unpadded();
    config.window.remember_frame = false;
    let mut fake = Fake::default();
    let mut forgetful = App::new(None, config, None).with_program(quiet());
    fake.start(&mut forgetful);
    let first = fake.windows().next().unwrap();
    let shown = fake.window(first).unwrap();
    assert!(!shown.remember && !shown.restore);
    fake.send_app(&mut forgetful, AppEvent::Command(NEW_WINDOW.to_owned()));
    let second = fake.windows().find(|&window| window != first).unwrap();
    assert!(!fake.window(second).unwrap().restore_size);
    // The key, turned on, reaches the window open.
    let mut config = forgetful.config.clone();
    config.window.remember_frame = true;
    forgetful.apply(&mut fake, config);
    assert!(fake.window(first).unwrap().remember);
}

#[test]
fn a_window_shows_its_program_s_output() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell("printf 'hello'; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "hello");
    // Of the config's columns and rows, within the padding.
    let snapshot = app.pane(window).snapshot();
    assert_eq!((snapshot.cols(), snapshot.rows()), (100, 30));
}

#[test]
fn a_program_finds_the_bundle_s_resources_and_the_user_s_locale() {
    let resources =
        std::env::temp_dir().join(format!("lokyr-app-resources-{}", std::process::id()));
    std::fs::create_dir_all(&resources).unwrap();
    let mut fake = Fake::default();
    // A language that the system has no locale of, then one that it has.
    fake.set_locales(&["xx_XX", "it_CH"]);
    let script = "printf '%s' \"$LOKYR_RESOURCES_DIR\"; sleep 10";
    let mut app = app(Config::default(), shell(script)).with_resources(Some(resources.clone()));
    fake.start(&mut app);
    assert_eq!(app.environment.locale, "it_CH.UTF-8");
    let window = fake.windows().next().unwrap();
    let shown = resources.display().to_string();
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == shown);
    let _ = std::fs::remove_dir_all(resources);
}

/// The point inside the cell at `col` and `row` of a window without
/// padding, whose columns are 4 points wide and rows 8 high.
fn cell(col: u16, row: u16) -> Point {
    Point {
        x: 4.0 * f64::from(col) + 1.0,
        y: 8.0 * f64::from(row) + 1.0,
    }
}

fn press(button: MouseButton, pressed: bool, at: Point, mods: Modifiers) -> WindowEvent {
    WindowEvent::MouseButton {
        button,
        pressed,
        position: at,
        mods,
        clicks: 1,
    }
}

/// The left button's press, or release, at `at`, with `mods`, of
/// `clicks` in a row.
fn click(pressed: bool, at: Point, mods: Modifiers, clicks: u32) -> WindowEvent {
    WindowEvent::MouseButton {
        button: MouseButton::Left,
        pressed,
        position: at,
        mods,
        clicks,
    }
}

/// The pointer's move to `at`, with `mods`.
fn moved(at: Point, mods: Modifiers) -> WindowEvent {
    WindowEvent::MouseMoved { position: at, mods }
}

/// A drag of the left button from `from` to `to`, with `mods`.
fn select_drag(
    fake: &mut Fake,
    app: &mut App,
    window: WindowId,
    (from, to): (Point, Point),
    mods: Modifiers,
) {
    fake.send(app, window, click(true, from, mods, 1));
    fake.send(app, window, moved(to, mods));
    fake.send(app, window, click(false, to, mods, 1));
}

/// The text that `window`'s pane selects, as the terminal has it.
fn selected_text(app: &App, window: WindowId) -> Option<String> {
    app.pane(window).selection_text()
}

/// The app without padding, its window running a program that shows two
/// lines, `hello world` and `second line`, then `ready`, of `config`'s
/// `[clipboard]`.
fn selecting(clipboard: &str, modes: &str) -> (Fake, App, WindowId) {
    let mut config = Config::parse(
        &format!("[clipboard]\n{clipboard}"),
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    config.window = unpadded().window;
    let program = shell(&format!(
        "stty raw -echo; printf 'hello world\\r\\nsecond line\\r\\n{modes}'; echo ready; cat -v"
    ));
    let mut fake = Fake::default();
    let mut app = app(config, program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window)
}

/// The last context menu that the app opened, over `window`: where, its
/// items' titles, `-` for a line, and its dimmed commands.
fn context_menu(fake: &Fake, window: WindowId) -> (Point, Vec<String>, Vec<String>) {
    let (over, position, menu) = fake.context_menus().last().expect("a context menu");
    assert_eq!(*over, window);
    let items = menu
        .items
        .iter()
        .map(|item| match item {
            MenuItem::Command {
                title, shortcut, ..
            } => {
                assert_eq!(*shortcut, None, "{title}");
                title.clone()
            }
            MenuItem::Separator => "-".to_owned(),
            other => panic!("{other:?}"),
        })
        .collect();
    (*position, items, menu.dimmed.clone())
}

/// A press and a release of the right button at `at`, with `mods`.
fn right_click(fake: &mut Fake, app: &mut App, window: WindowId, at: Point, mods: Modifiers) {
    fake.send(app, window, press(MouseButton::Right, true, at, mods));
    fake.send(app, window, press(MouseButton::Right, false, at, mods));
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

/// The app without padding, its window running `seq 1 100`, which
/// prints `101` to `103` at each Return: and the screen's row of `90`.
fn numbers() -> (Fake, App, WindowId, u16) {
    // The echo off before `ready`, so that no Return typed after it
    // echoes a line of its own.
    let program = shell(concat!(
        "seq 1 100; stty -echo; echo ready; ",
        "read line; seq 101 103; read line; seq 104 106; read line"
    ));
    let (fake, app, window) = started_with(program);
    let rows = app.pane(window).grid().rows;
    let ninety = (0..rows)
        .find(|&r| row(&app, window, r) == "90")
        .expect("90 on the screen");
    (fake, app, window, ninety)
}

const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::NONE
};

/// A program that turns the mouse's tracking on with `modes`, says
/// `ready`, and shows what it reads, in raw mode.
fn reader(modes: &str) -> Program {
    shell(&format!(
        "stty raw -echo; printf '{modes}'; echo ready; cat -v"
    ))
}

/// The app without padding, its window running `program`, once it
/// shows `ready`.
fn started_with(program: Program) -> (Fake, App, WindowId) {
    let mut fake = Fake::default();
    let mut app = app(unpadded(), program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window)
            .iter()
            .any(|line| line.contains("ready"))
    });
    (fake, app, window)
}

fn shows(app: &App, window: WindowId, text: &str) -> bool {
    screen(app, window).iter().any(|line| line.contains(text))
}

/// The app without padding, of a config whose `[clipboard]` reads
/// `read`, its window running `program`, once it shows `ready`.
fn with_clipboard(read: &str, program: Program) -> (Fake, App, WindowId) {
    let mut config = Config::parse(
        &format!("[clipboard]\nread = \"{read}\"\n"),
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = app(config, program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window)
}

#[test]
fn a_program_s_write_reaches_the_clipboard() {
    // `hello` to the clipboard, then `other` to the primary selection
    // alone, which macOS lacks.
    let program = shell(
        "printf '\\033]52;c;aGVsbG8=\\a'; printf '\\033]52;p;b3RoZXI=\\a'; echo ready; sleep 10",
    );
    let (mut fake, mut app, _) = with_clipboard("never", program);
    run_until(&mut fake, &mut app, |fake, _| {
        fake.clipboard_text().as_deref() == Some("hello")
    });
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
}

/// A program that reads the clipboard once it reads a byte, and shows
/// the answer.
fn clipboard_reader() -> Program {
    shell(
        "stty raw -echo; echo ready; while :; do head -c 1 >/dev/null; printf '\\033]52;c;?\\a'; done & cat -v",
    )
}

#[test]
fn a_read_gets_no_answer_unless_the_user_allows_it() {
    // Off by default: no alert, no answer.
    let (mut fake, mut app, window) = with_clipboard("never", clipboard_reader());
    fake.set_clipboard_text("secret");
    fake.send(&mut app, window, key());
    let pause = Instant::now() + Duration::from_millis(400);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(fake.alerts().is_empty());
    assert!(!shows(&app, window, "52;"), "{:?}", screen(&app, window));
    // Asked: refused, then allowed; a read while the alert is open asks
    // nothing more.
    let (mut fake, mut app, window) = with_clipboard("ask", clipboard_reader());
    fake.set_clipboard_text("secret");
    for allowed in [false, true] {
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| !fake.alerts().is_empty());
        fake.send(&mut app, window, key());
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(fake.alerts().len(), 1);
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        assert_eq!(
            open.alert.buttons,
            [
                app.texts.text("clipboard-read-deny"),
                app.texts.text("clipboard-read-allow")
            ]
        );
        let id = open.id;
        fake.answer_alert(&mut app, id, usize::from(allowed));
        if !allowed {
            let pause = Instant::now() + Duration::from_millis(300);
            run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
            assert!(!shows(&app, window, "52;"), "{:?}", screen(&app, window));
        }
    }
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[]52;c;c2VjcmV0^G")
    });
    let shown = screen(&app, window).concat();
    assert_eq!(shown.matches("52;").count(), 1, "{shown:?}");
}

#[test]
fn a_config_that_asks_applies_to_the_open_windows() {
    let (mut fake, mut app, window) = with_clipboard("never", clipboard_reader());
    let mut asking = app.config.clone();
    asking.clipboard.read = ClipboardRead::Ask;
    app.apply(&mut fake, asking);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
    // A window that closes takes its ask with it, once its program's
    // question, as `cat` runs, is answered.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert!(app.clipboard_asks.is_empty());
}

/// The app without padding, of the config `text`, its window running
/// `program`, once it shows `ready`.
fn configured(text: &str, program: Program) -> (Fake, App, WindowId) {
    let mut config = Config::parse(text, std::path::Path::new("config.toml")).unwrap();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = app(config, program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window)
}

#[test]
fn the_grid_starts_below_the_titlebar() {
    let mut fake = Fake::default();
    fake.set_bar(14.0);
    let mut app = app(unpadded(), selecting_program());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    // The size asked for is below the bar, whose height the grid leaves.
    let size = fake.window(window).unwrap().size;
    assert_eq!(size.points.height, 510.0);
    let grid = app.pane(window).grid();
    assert_eq!((grid.columns, grid.rows), (200, 62));
    // The cells start below it.
    let below = |col: u16, row: u16| Point {
        y: cell(col, row).y + 14.0,
        ..cell(col, row)
    };
    fake.send(
        &mut app,
        window,
        click(true, below(8, 0), Modifiers::NONE, 2),
    );
    fake.send(
        &mut app,
        window,
        click(false, below(8, 0), Modifiers::NONE, 2),
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    fake.send(
        &mut app,
        window,
        click(true, below(3, 1), Modifiers::NONE, 2),
    );
    fake.send(
        &mut app,
        window,
        click(false, below(3, 1), Modifiers::NONE, 2),
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("second"));
    // Over the bar, the first row.
    fake.send(
        &mut app,
        window,
        click(true, cell(8, 0), Modifiers::NONE, 2),
    );
    fake.send(
        &mut app,
        window,
        click(false, cell(8, 0), Modifiers::NONE, 2),
    );
    assert_eq!(selected_text(&app, window).as_deref(), Some("world"));
    // An opaque bar covers the bar's pixels, of the background.
    let background = Color::from_rgb8(1, 2, 3);
    let Some(Item::Rect { rect, color }) = bar_item(size, background) else {
        panic!("a bar");
    };
    assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 0, 1600, 28));
    assert_eq!(color, background.with_alpha(1.0));
    assert!(bar_item(points(400.0, 300.0), background).is_none());
}

/// A program that shows `hello world` and `second line`, then `ready`.
fn selecting_program() -> Program {
    shell("stty raw -echo; printf 'hello world\\r\\nsecond line\\r\\n'; echo ready; cat -v")
}

#[test]
fn the_title_tells_the_directory_the_program_and_the_size() {
    // A program that starts without a word: the last look after the
    // output's frames finds it.
    let mut fake = Fake::default();
    let mut app = app(
        unpadded(),
        shell("cd /usr; stty raw -echo; echo ready; sleep 0.1; exec cat"),
    );
    app.title_look = Some(TITLE_LOOK);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    run_until(&mut fake, &mut app, |fake, _| {
        title(fake) == "usr — cat — 200×62"
    });
    // Without the size.
    app.config.window.title_size = false;
    app.show_title(&mut fake, window);
    assert_eq!(title(&fake), "usr — cat");
    // A title of the program's, as a shell's prompt sets, and the
    // program in the foreground after it; once, when they are one.
    app.config.window.title_size = true;
    app.pane(window).text("\u{1b}]2;prompt\u{7}");
    run_until(&mut fake, &mut app, |fake, _| {
        title(fake) == "prompt — cat — 200×62"
    });
    app.pane(window).text("\u{1b}]2;cat\u{7}");
    run_until(&mut fake, &mut app, |fake, _| title(fake) == "cat — 200×62");
    // The directory's names.
    let home = Some(std::path::Path::new("/Users/me"));
    let named = |directory: &str| directory_name(std::path::Path::new(directory), home);
    assert_eq!(named("/Users/me/src/lokyr"), "lokyr");
    assert_eq!(named("/Users/me"), "~");
    assert_eq!(named("/"), "/");
    assert_eq!(directory_name(std::path::Path::new("/tmp"), None), "tmp");
}

#[test]
fn the_cursor_s_blinking_takes_no_look_at_the_title() {
    let (mut fake, mut app, window) = focused();
    app.title_look = Some(TITLE_LOOK);
    // The blink's frames, which no output brings: its timer alone.
    for _ in 0..4 {
        let (_, at) = app.blink_timer.unwrap();
        fake.fire_timers(&mut app, at);
        assert!(fake.send(&mut app, window, WindowEvent::Frame));
        assert_eq!(fake.timers().count(), 1);
        assert!(app.title_timer.is_none());
    }
    // A frame of output starts the titles' timer.
    app.pane(window).text("x");
    run_until(&mut fake, &mut app, |_, app| app.title_timer.is_some());
}

#[test]
fn the_title_s_last_look_comes_after_the_window_s_last_output() {
    let (mut fake, mut app, window) = focused();
    app.title_look = Some(TITLE_LOOK);
    app.pane(window).text("x");
    run_until(&mut fake, &mut app, |_, app| app.title_timer.is_some());
    let (_, at) = app.title_timer.unwrap();
    // A frame of output came after the timer started, and the program
    // changed since.
    let later = at - TITLE_LOOK / 2;
    let state = app.windows.get_mut(&window).unwrap();
    state.output_at = Some(later);
    state.place.1 = "stale".to_owned();
    // The timer looks not yet, and starts again for that frame.
    fake.fire_timers(&mut app, at);
    assert_eq!(app.windows[&window].place.1, "stale");
    assert_eq!(app.title_timer.map(|(_, at)| at), Some(later + TITLE_LOOK));
    fake.fire_timers(&mut app, later + TITLE_LOOK);
    assert_eq!(app.windows[&window].place.1, "cat");
    assert!(app.title_timer.is_none());
    assert_eq!(app.windows[&window].output_at, None);
}

#[test]
fn a_directory_that_cannot_be_read_keeps_the_title_s_place() {
    // A program of another user's, as `login` runs, whose directory the
    // system does not tell, waiting for a name.
    let mut fake = Fake::default();
    let mut app = app(
        unpadded(),
        shell("cd /usr; stty -echo; echo ready; read line; echo gone; exec /usr/bin/login"),
    );
    app.title_look = Some(TITLE_LOOK);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    run_until(&mut fake, &mut app, |fake, _| {
        title(fake).starts_with("usr — ")
    });
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |fake, _| {
        title(fake).ends_with(" — login — 200×62")
    });
    assert_eq!(title(&fake), "usr — login — 200×62");
}

#[test]
fn a_kept_screen_keeps_its_title_s_place() {
    let mut config = unpadded();
    config.window.close_on_exit = CloseOnExit::Never;
    let mut fake = Fake::default();
    let mut app = app(
        config,
        shell("cd /usr; stty raw -echo; echo ready; head -c 1 >/dev/null; exit 3"),
    );
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    run_until(&mut fake, &mut app, |fake, _| {
        title(fake).starts_with("usr — ")
    });
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| app.pane(window).is_kept());
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(title(&fake).starts_with("usr — "), "{}", title(&fake));
}

#[test]
fn a_window_takes_the_appearance_of_its_background_where_it_is_not_the_app_s() {
    let gray = |level: u8| Color::from_rgb8(level, level, level);
    assert_eq!(appearance_over(gray(250), Appearance::Light), None);
    assert_eq!(
        appearance_over(gray(250), Appearance::Dark),
        Some(Appearance::Light)
    );
    assert_eq!(
        appearance_over(gray(30), Appearance::Light),
        Some(Appearance::Dark)
    );
    assert_eq!(appearance_over(gray(30), Appearance::Dark), None);
    // Past the luminance where black and white contrast alike, about
    // 118 of 255.
    assert_eq!(
        appearance_over(gray(124), Appearance::Dark),
        Some(Appearance::Light)
    );
    assert_eq!(
        appearance_over(gray(112), Appearance::Light),
        Some(Appearance::Dark)
    );
}

#[test]
fn a_title_of_the_user_s_takes_the_place_of_the_window_s_own() {
    let program = shell("stty raw -echo; echo ready; cat");
    let (mut fake, mut app, window) = configured("", program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    let own = app.title_of(window);
    let ask = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, command_key_shift(KeyCode::KeyI, "i"));
        let open = fake.alerts().last().expect("the question").clone();
        assert_eq!(open.window, Some(window));
        open
    };
    // The question holds nothing at first; its title, trimmed.
    let open = ask(&mut fake, &mut app);
    assert_eq!(open.alert.input.as_deref(), Some(""));
    assert_eq!(open.alert.buttons, ["Set", "Cancel"]);
    // A second ask waits for the first's answer.
    fake.send_app(&mut app, AppEvent::Command(SET_TITLE.to_owned()));
    assert_eq!(fake.alerts().len(), 1);
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some("  build  "));
    assert_eq!(title(&fake), "build — 200×62");
    // A program's title stays behind it.
    app.pane(window).text("\u{1b}]2;prompt\u{7}");
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(title(&fake), "build — 200×62");
    // Asked again, it holds the user's; cancelled, it stays.
    let open = ask(&mut fake, &mut app);
    assert_eq!(open.alert.input.as_deref(), Some("build"));
    fake.answer_alert_typed(&mut app, open.id, (1, false), Some("other"));
    assert_eq!(title(&fake), "build — 200×62");
    // Emptied, the window's own comes back, of the program's title.
    let open = ask(&mut fake, &mut app);
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some(" "));
    assert_eq!(title(&fake), "prompt — bash — 200×62");
    assert_ne!(own, title(&fake));
}

/// The press of Command and Shift and the key of `letter`.
fn command_key_shift(code: KeyCode, letter: &str) -> WindowEvent {
    let mods = Modifiers { shift: true, ..CMD };
    key_event(code, letter, mods, true)
}

#[test]
fn a_window_takes_its_program_s_title() {
    let program = shell(
        "stty raw -echo; printf '\\033]2;x\\a'; echo ready; head -c 1 >/dev/null; printf '\\033]2;\\a'; sleep 10",
    );
    let (mut fake, mut app, window) = configured("", program);
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    let size = |app: &App| {
        let grid = app.pane(window).grid();
        format!("{TITLE_DASH}{}×{}", grid.columns, grid.rows)
    };
    // The program's title, then the program in the foreground: the
    // shell that runs the script.
    run_until(&mut fake, &mut app, |fake, app| {
        title(fake) == format!("x{TITLE_DASH}bash{}", size(app))
    });
    // A title of nothing gives the window's own.
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, app| {
        !title(fake).starts_with('x') && title(fake) == app.title_of(window)
    });
}

/// A program that rings its bell at each key, twice the second time
/// within a fifth of a second.
fn ringer() -> Program {
    shell(
        "stty raw -echo; echo ready; while :; do head -c 1 >/dev/null; printf '\\a'; sleep 0.05; printf '\\a'; done",
    )
}

#[test]
fn a_bell_flashes_a_window_with_the_focus_and_marks_one_without() {
    let (mut fake, mut app, window) = configured("", ringer());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].flash_until.is_some()
    });
    // The flash ends with its timer.
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].flash_until.is_none()
    });
    assert_eq!((fake.alert_sounds(), fake.attention_requests()), (0, 0));
    assert!(!fake.window(window).unwrap().title.starts_with(BELL_MARK));
    // Without the focus: a bounce and the mark, once for the two bells,
    // until the window takes the focus. The bells of the first key are
    // a fifth of a second behind.
    let pause = Instant::now() + Duration::from_millis(250);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, app| {
        fake.window(window).unwrap().title == format!("🔔 {}", app.title_of(window))
    });
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(fake.attention_requests(), 1);
    assert!(app.windows[&window].flash_until.is_none());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert_eq!(fake.window(window).unwrap().title, app.title_of(window));
}

#[test]
fn the_bell_s_config_chooses_what_it_does() {
    let config = "[bell]\nsound = true\nflash = false\nbounce = false\nmark-title = false\n";
    let (mut fake, mut app, window) = configured(config, ringer());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| fake.alert_sounds() == 1);
    assert!(app.windows[&window].flash_until.is_none());
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let pause = Instant::now() + Duration::from_millis(250);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| fake.alert_sounds() == 2);
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(fake.alert_sounds(), 2, "one sound for the two bells");
    assert_eq!(fake.attention_requests(), 0);
    assert!(!fake.window(window).unwrap().title.starts_with(BELL_MARK));
    assert!(app.windows[&window].flash_until.is_none());
}

/// A program that sends a notification of OSC 9 and one of OSC 777 at
/// each key.
fn notifier() -> Program {
    shell(
        "stty raw -echo; printf '\\033]2;agent\\a'; echo ready; while :; do head -c 1 >/dev/null; printf '\\033]9;waiting\\a'; printf '\\033]777;notify;Done;all tests pass\\a'; done",
    )
}

#[test]
fn a_notification_shows_for_a_window_without_the_focus() {
    let (mut fake, mut app, window) = configured("", notifier());
    // Not while the window has the focus.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(fake.notifications().is_empty());
    // Without it, the first of the two, its title the window's, as
    // OSC 9 gives none; the second within the pause shows nothing.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let shown = fake.notifications();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(
        (
            shown[0].window,
            shown[0].title.as_str(),
            shown[0].body.as_str()
        ),
        (window, "agent", "waiting")
    );
    // A click brings the window forward.
    fake.send_app(&mut app, AppEvent::NotificationClicked(window));
    assert_eq!(fake.focus_requests(), [window]);
    // A window that closed is not asked for.
    fake.send_app(&mut app, AppEvent::NotificationClicked(WindowId::new(999)));
    assert_eq!(fake.focus_requests(), [window]);
}

#[test]
fn a_notification_without_a_title_takes_the_user_s() {
    let (mut fake, mut app, window) = configured("", notifier());
    app.windows.get_mut(&window).unwrap().user_title = Some("build".to_owned());
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    assert_eq!(fake.notifications()[0].title, "build");
}

#[test]
fn a_notification_s_title_and_text_show_their_start() {
    let long = "x".repeat(5000);
    let program = shell(&format!(
        "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033]777;notify;{long};{long}\\a'; sleep 10"
    ));
    let (mut fake, mut app, window) = configured("", program);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    let shown = &fake.notifications()[0];
    assert_eq!(
        (shown.title.chars().count(), shown.body.chars().count()),
        (NOTIFICATION_TITLE, NOTIFICATION_BODY)
    );
}

/// The characters of the view's row `row` of `window`, each by its
/// column, and whether it is a cluster of several codepoints.
fn cells_of(app: &App, window: WindowId, row: u16) -> Vec<(u16, char, bool)> {
    use lokyr_vt_core::page::Content;

    let snapshot = app.pane(window).snapshot();
    (0..)
        .zip(snapshot.line(row).cells())
        .filter_map(|(col, cell)| match cell.content() {
            Content::Char(character) if character != '\0' => Some((col, character, false)),
            Content::Grapheme(character) => Some((col, character, true)),
            _ => None,
        })
        .collect()
}

/// A program that prints its `TERM`, then an emoji, an ambiguous `α`
/// and an accent that combines, each before a letter, at the start and
/// at each key, a row each time.
fn printer() -> Program {
    shell(
        "stty raw -echo; printf '%s\\r\\nready\\r\\n' \"$TERM\"; while :; do printf '\\360\\237\\230\\200x\\316\\261ye\\314\\201z\\r\\n'; head -c 1 >/dev/null; done",
    )
}

/// The columns of `x`, `y` and `z` on the view's row `row`, and whether
/// the accent's character before `z` is a cluster.
fn placed(app: &App, window: WindowId, row: u16) -> Option<(u16, u16, u16, bool)> {
    let cells = cells_of(app, window, row);
    let at = |wanted: char| {
        cells
            .iter()
            .find(|&&(_, c, _)| c == wanted)
            .map(|&(col, ..)| col)
    };
    let z = at('z')?;
    let accent = cells.iter().rev().find(|&&(col, ..)| col < z)?.2;
    Some((at('x')?, at('y')?, z, accent))
}

#[test]
fn a_new_window_takes_the_terminal_s_settings() {
    let (mut fake, mut app, window) = configured("", printer());
    run_until(&mut fake, &mut app, |_, app| {
        placed(app, window, 2).is_some()
    });
    assert_eq!(row(&app, window, 0).trim(), "xterm-256color");
    // An emoji of two cells, `α` of one, and the accent kept apart.
    assert_eq!(placed(&app, window, 2), Some((2, 4, 6, true)));
    let config = "[terminal]\nterm = \"vt220\"\nunicode-widths = \"8\"\nambiguous-width = 2\nnormalization = \"nfc\"\n";
    let (mut fake, mut app, window) = configured(config, printer());
    run_until(&mut fake, &mut app, |_, app| {
        placed(app, window, 2).is_some()
    });
    assert_eq!(row(&app, window, 0).trim(), "vt220");
    // The emoji of one cell, `α` of two, and `é` composed, of two too,
    // as it is ambiguous.
    assert_eq!(placed(&app, window, 2), Some((1, 4, 7, false)));
    // XTGETTCAP tells the same name, in hex.
    let asking =
        shell("stty raw -echo; printf '\\033P+q544E\\033\\\\'; printf 'ready\\r\\n'; cat -v");
    let (mut fake, mut app, window) = configured(config, asking);
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "+r544E=7674323230")
    });
}

#[test]
fn the_terminal_s_own_term_needs_its_entry() {
    let program = || shell("printf '[%s] [%s]\\n' \"$TERM\" \"$TERMINFO\"; echo ready; sleep 10");
    let config = || {
        let mut config = Config::parse(
            "[terminal]\nterm = \"xterm-lokyr\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap();
        config.window = unpadded().window;
        config
    };
    for (terminfo, shown) in [
        (None, "[xterm-256color] []"),
        (
            Some("/tmp/lokyr-terminfo"),
            "[xterm-lokyr] [/tmp/lokyr-terminfo]",
        ),
    ] {
        let mut fake = Fake::default();
        let mut app = app(config(), program());
        app.environment.terminfo = terminfo.map(PathBuf::from);
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
        assert_eq!(row(&app, window, 0).trim(), shown);
    }
}

#[test]
fn a_reloaded_config_s_settings_apply_to_the_text_to_come() {
    let (mut fake, mut app, window) = configured("", printer());
    let mut changed = app.config.clone();
    changed.terminal = Config::parse(
        "[terminal]\nterm = \"vt220\"\nunicode-widths = \"8\"\nambiguous-width = 2\nnormalization = \"nfc\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap()
    .terminal;
    app.apply(&mut fake, changed);
    run_until(&mut fake, &mut app, |_, app| {
        placed(app, window, 2).is_some()
    });
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| {
        placed(app, window, 3).is_some()
    });
    // The row before keeps its cells; the new one takes the settings,
    // and the program keeps its `TERM`.
    assert_eq!(placed(&app, window, 2), Some((2, 4, 6, true)));
    assert_eq!(placed(&app, window, 3), Some((1, 4, 7, false)));
    assert_eq!(row(&app, window, 0).trim(), "xterm-256color");
}

#[test]
fn the_scrollback_keeps_within_its_limit_and_the_budget_follows_the_config() {
    let program = shell(
        "stty raw -echo; seq 1 20000; printf 'ready\\r\\n'; head -c 1 >/dev/null; seq 1 20000; printf 'done\\r\\n'; sleep 10",
    );
    let (mut fake, mut app, window) = configured("[scrollback]\nlimit = 2\nbudget = 50\n", program);
    assert_eq!(app.budget.limit(), 50 * 1024 * 1024);
    let kept = |fake: &mut Fake, app: &mut App| {
        app.pane(window).scroll(Scroll::Top);
        fake.send(app, window, WindowEvent::Frame);
        app.pane(window).snapshot().offset()
    };
    // Two MiB keep a thousand rows or so of these columns, short of the
    // 20,000.
    let two = kept(&mut fake, &mut app);
    assert!((500..4000).contains(&two), "{two} rows kept");
    // Smaller, for the output to come, and a larger budget.
    let changed = Config::parse(
        "[scrollback]\nlimit = 1\nbudget = 200\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let mut config = app.config.clone();
    config.scrollback = changed.scrollback;
    app.apply(&mut fake, config);
    assert_eq!(app.budget.limit(), 200 * 1024 * 1024);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "done"));
    let one = kept(&mut fake, &mut app);
    assert!(one < two * 3 / 4, "{one} rows kept, from {two}");
}

#[test]
fn the_config_s_scroll_on_output_reaches_the_panes() {
    let (mut fake, mut app, path) = with_file("output", "[scrollback]\nscroll-on-output = true\n");
    let window = fake.windows().next().unwrap();
    assert!(app.pane(window).scroll_on_output());
    std::fs::write(&path, "").unwrap();
    change(&mut fake, &mut app);
    assert!(!app.pane(window).scroll_on_output());
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

/// The config of a window without padding.
/// The config of no padding, of windows of 200 columns and 62 rows:
/// 800 points by 496, of cells 4 points wide and 8 high.
fn unpadded() -> Config {
    let mut config = Config::default();
    config.window.padding_x = 0.0.try_into().unwrap();
    config.window.padding_y = 0.0.try_into().unwrap();
    config.window.columns = 200.try_into().unwrap();
    config.window.rows = 62.try_into().unwrap();
    config
}

/// A size of a window's content, in points, at a scale of 2: cells of 8
/// by 16 pixels make a column of 4 points and a row of 8.
fn points(width: f64, height: f64) -> WindowSize {
    WindowSize::new(Size { width, height }, 2.0)
}

/// The text of every row of a window's screen.
fn screen(app: &App, window: WindowId) -> Vec<String> {
    let rows = app.pane(window).snapshot().rows();
    (0..rows).map(|at| row(app, window, at)).collect()
}

#[test]
fn the_grid_fits_the_window_less_its_padding() {
    let (mut fake, mut app, path) = with_file("padding", "[window]\npadding-x = 10\n");
    // 1600 by 1000 pixels, less 20 at the left and the right and 8 at
    // the top and the bottom.
    let grid = app.grid(points(800.0, 500.0));
    assert_eq!((grid.columns, grid.rows), (195, 61));
    // A window smaller than its padding keeps a cell.
    let grid = app.grid(points(15.0, 5.0));
    assert_eq!((grid.columns, grid.rows), (1, 1));
    // The input method's candidates go at the cursor's cell, within the
    // padding.
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    let (origin, _) = fake.window(window).unwrap().text_area.unwrap();
    assert_eq!((origin.x, origin.y), (10.0, 4.0));
    // A reloaded padding gives the grid that it leaves: of a window of
    // 100 columns and 30 rows with their padding, 420 points by 248.
    std::fs::write(&path, "[window]\npadding-x = 0\npadding-y = 0\n").unwrap();
    change(&mut fake, &mut app);
    assert!(fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Frame);
    let snapshot = app.pane(window).snapshot();
    assert_eq!((snapshot.cols(), snapshot.rows()), (105, 31));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_translucent_background_makes_the_window_translucent() {
    let (mut fake, mut reloaded, path) = with_file("translucent", "");
    let window = fake.windows().next().unwrap();
    let backdrop = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::Frame);
        fake.window(window)
            .unwrap()
            .translucent
            .map(|backdrop| (backdrop.color, backdrop.blur))
    };
    assert_eq!(backdrop(&mut fake, &mut reloaded), None);
    std::fs::write(
        &path,
        "[window]\nbackground-opacity = 0.9\nbackground-blur = 20\n",
    )
    .unwrap();
    change(&mut fake, &mut reloaded);
    // The terminal's default background, of the light variant, with the
    // opacity.
    let light = reloaded.colors(Appearance::Light).background;
    let rgb = |rgb: lokyr_vt_core::page::Rgb| {
        [rgb.r, rgb.g, rgb.b].map(|component| f64::from(f32::from(component) / 255.0))
    };
    let [red, green, blue] = rgb(light);
    let opacity = f64::from(0.9_f32);
    assert_eq!(
        backdrop(&mut fake, &mut reloaded),
        Some(([red, green, blue, opacity], 20.0))
    );
    // The dark variant's background takes its place.
    fake.send_app(&mut reloaded, AppEvent::AppearanceChanged(Appearance::Dark));
    let [red, green, blue] = rgb(reloaded.colors(Appearance::Dark).background);
    assert_eq!(
        backdrop(&mut fake, &mut reloaded),
        Some(([red, green, blue, opacity], 20.0))
    );
    // A blur alone, of an opaque background, leaves the window opaque.
    std::fs::write(&path, "[window]\nbackground-blur = 20\n").unwrap();
    change(&mut fake, &mut reloaded);
    assert_eq!(backdrop(&mut fake, &mut reloaded), None);
    std::fs::write(&path, "[window]\nbackground-opacity = 0.5\n").unwrap();
    change(&mut fake, &mut reloaded);
    let blur = backdrop(&mut fake, &mut reloaded).map(|(_, blur)| blur);
    assert_eq!(blur, Some(0.0));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
    // A window opens as the config says.
    let mut config = Config::default();
    config.window.background_opacity = 0.8.try_into().unwrap();
    config.window.background_blur = 12.0.try_into().unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let opened = fake.window(window).unwrap().translucent;
    assert_eq!(opened.map(|backdrop| backdrop.blur), Some(12.0));
}

#[test]
fn a_resize_storm_tells_the_program_its_size_at_its_pace() {
    let mut fake = Fake::default();
    // The program prints its size at each SIGWINCH.
    let script = "trap 'stty size' WINCH; echo ready; while :; do sleep 0.02; done";
    let mut app = app(unpadded(), shell(script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "ready");
    // A live resize's steps, as fast as they come.
    let start = Instant::now();
    let mut told = Vec::new();
    let grids: Vec<Grid> = (0..50)
        .map(|step| {
            let size = points(400.0 + f64::from(step) * 4.0, 300.0);
            assert!(fake.send(&mut app, window, WindowEvent::Resized(size)));
            told.push(app.pane(window).told());
            app.grid(size)
        })
        .collect();
    let elapsed = start.elapsed();
    let last = grids[49];
    assert_eq!((last.columns, last.rows), (149, 37));
    // The terminal takes each step at once, and the program the first,
    // then one every 50 ms at most.
    let pane = app.pane(window);
    assert_eq!(pane.snapshot().cols(), last.columns);
    assert_eq!(told[0], grids[0]);
    told.dedup();
    let paced = elapsed.as_millis() / RESIZE_PACE.as_millis() + 1;
    assert!(
        told.len() as u128 <= paced,
        "{} sizes in {elapsed:?}",
        told.len()
    );
    // Then the last step, once, as its timer comes: the timer of a size
    // told since it started comes first, and starts it again.
    for _ in 0..2 {
        let Some((timer, at)) = app.resize_timer else {
            break;
        };
        // The label of the grid's size has its own, a second later.
        let timers: Vec<_> = fake.timers().collect();
        assert!(timers.contains(&(timer, at)), "{timers:?}");
        assert_eq!(fake.fire_timers(&mut app, at), 1);
    }
    assert_eq!(app.resize_timer, None);
    assert_eq!(app.pane(window).told(), last);
    // The program saw few sizes, and the last one.
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window).iter().any(|line| line == "37 149")
    });
    let sizes: Vec<String> = screen(&app, window)
        .into_iter()
        .skip(1)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(sizes.len() <= told.len() + 1, "{sizes:?}");
    assert_eq!(sizes.last().map(String::as_str), Some("37 149"));
}

#[test]
fn a_resize_shows_the_grid_s_size_until_a_moment_after_it_ends() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    let label = |app: &App| app.windows[&window].size_label;
    // The first frame shows none.
    assert_eq!(label(&app), None);
    // A live resize that changes the grid shows it, while it lasts.
    fake.send(&mut app, window, WindowEvent::LiveResize(true));
    assert!(app.windows[&window].live_resize);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    let until = label(&app).expect("the label");
    let (timer, at) = app.label_timer.expect("its timer");
    assert_eq!(at, until);
    // Its timer, within the live resize, leaves it.
    fake.send_app(&mut app, AppEvent::Timer(timer));
    assert_eq!(label(&app), Some(until));
    // Its end keeps it a moment more, then takes it away, with a frame.
    let ended = Instant::now();
    fake.send(&mut app, window, WindowEvent::LiveResize(false));
    let until = label(&app).expect("the label");
    assert!(until >= ended + size_label::HOLD);
    let (_, at) = app.label_timer.expect("its timer");
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!fake.window(window).unwrap().frame_requested);
    // The cursor's blinking has a timer of its own before it.
    assert!(fake.fire_timers(&mut app, at) >= 1);
    assert_eq!(label(&app), None);
    assert_eq!(app.label_timer, None);
    assert!(fake.window(window).unwrap().frame_requested);
    // A resize of the same grid shows none.
    fake.send(&mut app, window, WindowEvent::Resized(points(401.0, 301.0)));
    assert_eq!(label(&app), None);
}

#[test]
fn a_live_resize_draws_once_a_refresh() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::LiveResize(true));
    // A frame drawn within this refresh: the step waits for the next.
    app.windows.get_mut(&window).unwrap().drawn_at = Some(Instant::now());
    let before = app.pane(window).grid();
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!fake.window(window).unwrap().frame_requested);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    assert_eq!(app.pane(window).grid(), before);
    assert!(fake.window(window).unwrap().frame_requested);
    // A refresh later, it draws at once.
    let earlier = Instant::now() - Duration::from_millis(50);
    app.windows.get_mut(&window).unwrap().drawn_at = Some(earlier);
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    assert_ne!(app.pane(window).grid(), before);
}

#[test]
fn the_screen_and_its_scrollback_reflow_with_the_window() {
    let mut fake = Fake::default();
    let script = "seq 1 3000; printf '%150s' '' | tr ' ' x; sleep 10";
    let mut app = app(unpadded(), shell(script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let line = "x".repeat(150);
    run_until(&mut fake, &mut app, |_, app| row(app, window, 61) == line);
    let before = screen(&app, window);
    assert_eq!(before[60], "3000");
    // Half as wide, the line takes two rows, and the screen keeps its
    // height.
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 500.0)));
    let narrow = screen(&app, window);
    assert_eq!(narrow.len(), 62);
    assert_eq!(narrow[59..], ["3000", &line[..100], &line[100..]]);
    // Through other sizes, taller and shorter too, and back, the screen
    // is as it was: no row lost, none doubled.
    for (width, height) in [
        (52.0, 500.0),
        (244.0, 200.0),
        (1000.0, 900.0),
        (148.0, 500.0),
    ] {
        fake.send(
            &mut app,
            window,
            WindowEvent::Resized(points(width, height)),
        );
    }
    fake.send(&mut app, window, WindowEvent::Resized(points(800.0, 500.0)));
    assert_eq!(screen(&app, window), before);
}

#[test]
fn a_synchronized_update_shows_whole() {
    let mut fake = Fake::default();
    let script = "printf 'before\\r\\n'; sleep 0.3; printf '\\033[?2026hhalf'; sleep 0.5; \
                  printf ' done\\033[?2026l'; sleep 10";
    let mut app = app(Config::default(), shell(script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let mut seen = Vec::new();
    run_until(&mut fake, &mut app, |_, app| {
        let shown = row(app, window, 1);
        if seen.last() != Some(&shown) {
            seen.push(shown.clone());
        }
        shown == "half done"
    });
    assert!(!seen.iter().any(|shown| shown == "half"), "{seen:?}");
    assert_eq!(row(&app, window, 0), "before");
}

/// The app with `close-on-exit` as `close`, whose window runs `script`,
/// once its program has ended and the window has done what `close`
/// says.
fn ended(close: CloseOnExit, script: &str) -> (Fake, App, WindowId) {
    let mut config = Config::default();
    config.window.close_on_exit = close;
    let mut fake = Fake::default();
    let mut app = app(config, shell(script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |fake, app| {
        fake.windows().count() == 0 || app.pane(window).is_kept()
    });
    (fake, app, window)
}

fn key() -> WindowEvent {
    typed(lokyr_platform_core::KeyCode::KeyA, "a", false)
}

/// A key, of `code`, that types `text`, with Command held or not.
fn typed(code: lokyr_platform_core::KeyCode, text: &str, command: bool) -> WindowEvent {
    WindowEvent::Key(lokyr_platform_core::KeyEvent {
        code,
        state: KeyState::Pressed,
        mods: lokyr_platform_core::Modifiers {
            super_key: command,
            ..lokyr_platform_core::Modifiers::default()
        },
        text: text.to_owned(),
        unmodified: text.to_owned(),
        plain: text.to_owned(),
        time: None,
    })
}

/// The press, or the release, of the key at `code`, which types `plain`
/// without modifiers, with `mods`.
fn key_event(code: KeyCode, plain: &str, mods: Modifiers, pressed: bool) -> WindowEvent {
    let control = mods.control && plain.len() == 1;
    let text = if control {
        // Control types the letter's control code.
        let byte = plain.as_bytes()[0].to_ascii_uppercase() & 0x1f;
        char::from(byte).to_string()
    } else {
        plain.to_owned()
    };
    WindowEvent::Key(KeyEvent {
        code,
        state: if pressed {
            KeyState::Pressed
        } else {
            KeyState::Released
        },
        mods,
        text,
        unmodified: plain.to_owned(),
        plain: plain.to_owned(),
        time: None,
    })
}

const CMD: Modifiers = Modifiers {
    super_key: true,
    ..Modifiers::NONE
};

const CTRL: Modifiers = Modifiers {
    control: true,
    ..Modifiers::NONE
};

/// The app without padding, of a config whose `[keys]` are `keys`, its
/// window running `program`, once it shows `ready`.
fn bound(keys: &str, program: Program) -> (Fake, App, WindowId) {
    let mut config = Config::parse(
        &format!("[keys]\n{keys}"),
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = app(config, program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window)
}

#[test]
fn a_binding_of_the_config_sends_its_text_as_it_is() {
    // A shell's line editing: the word before, and the line's start.
    let keys = concat!(
        "\"alt+backspace\" = { command = \"pane.send-text\", args = { text = \"\\u0017\" } }\n",
        "\"cmd+left\" = { command = \"pane.send-text\", args = { text = \"\\u0001\" } }\n",
    );
    // More lines than the window's rows, for a scrollback.
    let program = shell("seq 1 200; echo ready; stty raw -echo; cat -v");
    let (mut fake, mut app, window) = bound(keys, program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    // A view up the scrollback comes back down, as typing brings it.
    let offset = |app: &App| app.pane(window).snapshot().offset();
    fake.send(&mut app, window, key_event(KeyCode::ArrowUp, "", CMD, true));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 1);
    let alt = Modifiers {
        left_alt: true,
        ..Modifiers::NONE
    };
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Backspace, "", alt, true),
    );
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowLeft, "", CMD, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        offset(app) == 0 && shows(app, window, "^W^A")
    });
    // Its text is required.
    let error = app
        .commands
        .check("pane.send-text", Args::new())
        .unwrap_err();
    assert_eq!(error.code, Code::MissingArgument);
}

/// A plugin of the tests: `test.copy`, on Control-T, puts the first
/// line of the focused pane's text on the clipboard, and opens a
/// window through `window.new`; `test.boom`, on Control-B, panics;
/// `test.size` gives the text 18 points; `test.close` closes the
/// focused window; and `test.again` runs itself, counting its runs.
struct Tester {
    runs: std::rc::Rc<std::cell::Cell<usize>>,
}

impl plugins::Plugin for Tester {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        let names = [
            "test.copy",
            "test.boom",
            "test.size",
            "test.close",
            "test.again",
            "test.bind",
        ];
        for name in names {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        for (keys, command) in [("ctrl+t", "test.copy"), ("ctrl+b", "test.boom")] {
            cx.bind(plugins::host::Binding {
                keys: keys.to_owned(),
                command: command.to_owned(),
                args: Args::new(),
                when: Some("pane.focused".to_owned()),
            })?;
        }
        Ok(())
    }

    fn on_command(
        &mut self,
        cx: &mut plugins::Cx<'_>,
        name: &str,
        _args: Args,
    ) -> Result<(), Error> {
        match name {
            "test.boom" => panic!("on purpose"),
            "test.size" => cx.set_font_size(Some(18.0)),
            "test.close" => {
                let window = cx.focused_window().expect("a window");
                cx.close_window(window)
            }
            "test.again" => {
                self.runs.set(self.runs.get() + 1);
                cx.run_command("test.again", Args::new())
            }
            // A binding that takes a key of the app's own.
            "test.bind" => cx.bind(plugins::host::Binding {
                keys: "cmd+n".to_owned(),
                command: "test.size".to_owned(),
                args: Args::new(),
                when: None,
            }),
            _ => {
                let pane = cx.focused_pane().expect("a focused pane");
                cx.select_all(pane)?;
                let text = cx.selection_text(pane)?.unwrap_or_default();
                cx.set_clipboard_text(text.lines().next().unwrap_or_default())?;
                cx.run_command(NEW_WINDOW, Args::new())
            }
        }
    }
}

/// The app with the tests' plugin, whose runs of `test.again` the
/// counter counts, its window running `reader`, focused, once it shows
/// `ready`.
fn with_tester() -> (Fake, App, WindowId, std::rc::Rc<std::cell::Cell<usize>>) {
    let runs = std::rc::Rc::default();
    let tester = Tester {
        runs: std::rc::Rc::clone(&runs),
    };
    let mut fake = Fake::default();
    let mut app = app(unpadded(), reader("")).with_plugin("tester", Box::new(tester));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window, runs)
}

#[test]
fn a_plugin_s_binding_runs_its_command_and_what_it_asks() {
    let (mut fake, mut app, window, _) = with_tester();
    // The plugins started with the app, the built-in one too.
    assert_eq!(
        app.plugins.status("core-actions"),
        Some(plugins::Status::Active)
    );
    assert!(app.commands.get("test.copy").is_some());
    fake.send(&mut app, window, key_event(KeyCode::KeyT, "t", CTRL, true));
    assert_eq!(fake.clipboard_text().as_deref(), Some("ready"));
    // The window that its command asked for, once the call returned.
    assert_eq!(fake.windows().count(), 2);
    // The key went nowhere else.
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(!shows(&app, window, "^T"));
    // Another language makes the registry anew, with the plugins'
    // commands.
    let mut config = app.config.clone();
    config.app.language = "it".to_owned();
    app.apply(&mut fake, config);
    assert!(app.commands.get("test.copy").is_some());
    fake.send(&mut app, window, key_event(KeyCode::KeyT, "t", CTRL, true));
    assert_eq!(fake.windows().count(), 3);
}

#[test]
fn a_plugin_that_panics_is_disabled_and_its_keys_go_to_the_program() {
    let (mut fake, mut app, window, _) = with_tester();
    fake.send(&mut app, window, key_event(KeyCode::KeyB, "b", CTRL, true));
    assert_eq!(
        app.plugins.status("tester"),
        Some(plugins::Status::Disabled)
    );
    assert!(app.commands.get("test.copy").is_none());
    // Its bindings went with it: its keys are the program's, and the
    // app goes on.
    fake.send(&mut app, window, key_event(KeyCode::KeyT, "t", CTRL, true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^T"));
    assert_eq!(fake.clipboard_text(), None);
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn a_plugin_s_requests_change_the_app_after_its_call() {
    let (mut fake, mut app, window, runs) = with_tester();
    app.run(&mut fake, "test.size", Args::new());
    assert_eq!(app.font_size, Some(18.0));
    assert_eq!(sized(&app.config.font, app.font_size).size.points(), 18.0);
    assert_eq!(sized(&app.config.font, None).size.points(), 13.0);
    // The focused window closes; the other stays.
    app.run(&mut fake, NEW_WINDOW, Args::new());
    assert_eq!(fake.windows().count(), 2);
    app.run(&mut fake, "test.close", Args::new());
    // Its program, `cat`, runs: the question, answered.
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert_eq!(fake.windows().collect::<Vec<_>>().len(), 1);
    assert!(!fake.windows().any(|shown| shown == window));
    // A command that runs itself stops, its first run and as many
    // again as the depth allows.
    app.run(&mut fake, "test.again", Args::new());
    assert_eq!(runs.get(), COMMANDS_DEEP + 1);
}

/// The press of Command and the key of `letter`.
fn command_key(code: KeyCode, letter: &str) -> WindowEvent {
    key_event(code, letter, CMD, true)
}

/// The app without padding, its window running `program`, focused, once
/// it shows `ready`.
fn focused_with(program: Program) -> (Fake, App, WindowId) {
    let (mut fake, mut app, window) = started_with(program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    (fake, app, window)
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

/// Opens with Command-click what `window` shows at `col` of its first
/// row.
fn cmd_click(fake: &mut Fake, app: &mut App, window: WindowId, col: u16) {
    fake.send(app, window, click(true, cell(col, 0), CMD, 1));
    fake.send(app, window, click(false, cell(col, 0), CMD, 1));
}

/// A program that shows a URL at column 0, a file at 20, a program at
/// 31 and a directory at 45.
fn links() -> Program {
    shell(concat!(
        "stty raw -echo; ",
        "printf 'https://lokyr.dev/x /etc/hosts /usr/bin/true /usr\\r\\n'; ",
        "echo ready; cat -v"
    ))
}

#[test]
fn a_link_asks_first_and_the_config_keeps_the_answer() {
    let (mut fake, mut app, window, path) = pasting("links", "# mine\n", links());
    let shown = |fake: &Fake| {
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        (
            open.alert.message.clone(),
            open.alert.detail.clone(),
            open.alert.buttons.clone(),
            open.alert.suppression.clone().unwrap(),
        )
    };
    // A URL: Open or Cancel.
    cmd_click(&mut fake, &mut app, window, 2);
    let url = (
        "Open this link?".to_owned(),
        "https://lokyr.dev/x".to_owned(),
        vec!["Open".to_owned(), "Cancel".to_owned()],
        "Don't ask again for links".to_owned(),
    );
    assert_eq!(shown(&fake), url);
    // Ticked, but cancelled: nothing changes.
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 1, true);
    // A file: Open, Show in Finder, or Cancel.
    cmd_click(&mut fake, &mut app, window, 22);
    let file = (
        "Open this file?".to_owned(),
        "/etc/hosts".to_owned(),
        vec![
            "Open".to_owned(),
            "Show in Finder".to_owned(),
            "Cancel".to_owned(),
        ],
        "Don't ask again for files".to_owned(),
    );
    assert_eq!(shown(&fake), file);
    fake.answer_alert(&mut app, fake.alerts()[0].id, 1);
    // A program shows in the Finder, and a directory opens, without a
    // question.
    cmd_click(&mut fake, &mut app, window, 33);
    cmd_click(&mut fake, &mut app, window, 46);
    assert!(fake.alerts().is_empty());
    assert_eq!(fake.opened(), ["/usr"]);
    assert_eq!(fake.revealed(), ["/etc/hosts", "/usr/bin/true"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");
    // Ticked, the answer goes into the config, and the next one opens
    // without a question, before the file's reload too.
    cmd_click(&mut fake, &mut app, window, 2);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    cmd_click(&mut fake, &mut app, window, 22);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 1, true);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("# mine\n"), "{written}");
    let links = Config::read(&path).unwrap().links;
    assert_eq!((links.urls, links.files), (OpenUrl::Open, OpenFile::Reveal));
    cmd_click(&mut fake, &mut app, window, 2);
    cmd_click(&mut fake, &mut app, window, 22);
    assert!(fake.alerts().is_empty());
    assert_eq!(
        fake.opened()[1..],
        ["https://lokyr.dev/x", "https://lokyr.dev/x"]
    );
    assert_eq!(fake.revealed()[2..], ["/etc/hosts", "/etc/hosts"]);
    // Opening a file, ticked, keeps `open`.
    std::fs::write(&path, "").unwrap();
    app.apply(&mut fake, Config::default());
    cmd_click(&mut fake, &mut app, window, 22);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    assert_eq!(fake.opened().last().map(String::as_str), Some("/etc/hosts"));
    assert_eq!(Config::read(&path).unwrap().links.files, OpenFile::Open);
    assert!(app.links_chosen == (None, None));
}

#[test]
fn a_window_that_closes_drops_its_link_s_question() {
    let (mut fake, mut app, window, _) = pasting("link-close", "", links());
    cmd_click(&mut fake, &mut app, window, 2);
    assert_eq!(app.open_asks.len(), 1);
    app.run(&mut fake, NEW_WINDOW, Args::new());
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert!(app.open_asks.is_empty());
}

#[test]
fn a_link_s_question_shows_its_invisible_characters_escaped() {
    assert_eq!(
        shown("https://lokyr.dev/\u{202E}gpj.exe\u{200B}x\u{7}é"),
        "https://lokyr.dev/\\u{202E}gpj.exe\\u{200B}x\\u{7}é"
    );
    assert_eq!(shown("/Users/x/日本 文書.txt"), "/Users/x/日本 文書.txt");
    for c in [
        '\u{2066}',
        '\u{2069}',
        '\u{2028}',
        '\u{FEFF}',
        '\u{E0041}',
        '\u{FE0F}',
    ] {
        assert!(invisible(c), "{c:?}");
    }
    for c in ['a', ' ', '/', 'é', '日', '😀'] {
        assert!(!invisible(c), "{c:?}");
    }
}

#[test]
fn a_link_s_answer_lasts_until_the_app_quits_where_the_config_cannot_keep_it() {
    let text = "[font]\nsise = 14\n";
    let (mut fake, mut app, window, path) = pasting("bad-links", text, links());
    cmd_click(&mut fake, &mut app, window, 2);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    cmd_click(&mut fake, &mut app, window, 22);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert_eq!(
        app.links_chosen,
        (Some(OpenUrl::Open), Some(OpenFile::Open))
    );
    // A reload of the file does not bring the questions back.
    app.apply(&mut fake, Config::default());
    cmd_click(&mut fake, &mut app, window, 2);
    cmd_click(&mut fake, &mut app, window, 22);
    assert!(fake.alerts().is_empty());
    assert_eq!(fake.opened().len(), 4);
}

/// The app without padding, of the config file `text`, its window
/// running `program`, focused, once it shows `ready`; a file that does
/// not read gives the defaults.
fn pasting(test: &str, text: &str, program: Program) -> (Fake, App, WindowId, PathBuf) {
    let directory = std::env::temp_dir().join(format!("lokyr-paste-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, text).unwrap();
    let mut config = Config::read(&path).unwrap_or_default();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = App::new(None, config, Some(path.clone())).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    (fake, app, window, path)
}

/// Pastes `text` with Command-V into `window`.
fn paste_key(fake: &mut Fake, app: &mut App, window: WindowId, text: &str) {
    fake.set_clipboard_text(text);
    fake.send(app, window, command_key(KeyCode::KeyV, "v"));
}

#[test]
fn copy_and_paste_go_through_the_clipboard_and_several_lines_ask() {
    let (mut fake, mut app, window) = focused_with(reader(""));
    // The pane's text, to its cursor's row.
    fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
    fake.send(&mut app, window, command_key(KeyCode::KeyC, "c"));
    assert_eq!(fake.clipboard_text().as_deref(), Some("ready\n"));
    // A line pastes at once, and a line with its line break without it.
    paste_key(&mut fake, &mut app, window, "hello");
    paste_key(&mut fake, &mut app, window, "ls\n");
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "hellols"));
    assert!(fake.alerts().is_empty());
    // Several lines ask first, once at a time: cancelled, pasted, and
    // pasted as one line.
    for button in [2, 0, 1] {
        paste_key(&mut fake, &mut app, window, "a\nb\n");
        paste_key(&mut fake, &mut app, window, "a\nb\n");
        assert_eq!(fake.alerts().len(), 1);
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        assert_eq!(
            open.alert.message,
            app.texts
                .text_with("paste-lines-message", &[("lines", Arg::Number(2))])
        );
        assert!(
            open.alert.detail.ends_with("\n\na\nb"),
            "{}",
            open.alert.detail
        );
        assert_eq!(
            open.alert.buttons,
            [
                app.texts.text("paste-lines-paste"),
                app.texts.text("paste-lines-one-line"),
                app.texts.text("paste-lines-cancel")
            ]
        );
        assert_eq!(
            open.alert.suppression,
            Some(app.texts.text("paste-lines-dont-ask"))
        );
        let id = open.id;
        fake.answer_alert(&mut app, id, button);
        if button == 2 {
            let pause = Instant::now() + Duration::from_millis(300);
            run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
            assert!(
                !shows(&app, window, "hellolsa"),
                "{:?}",
                screen(&app, window)
            );
        }
    }
    // The line breaks reach the program as Return; as one line, a
    // space for each, but the last.
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "hellolsa^Mb^Ma b")
    });
    // The question asked nothing of the config.
    assert_eq!(app.config.clipboard.confirm_paste, ConfirmPaste::Auto);
}

#[test]
fn a_program_that_asks_for_bracketed_paste_takes_line_breaks_at_once() {
    let (mut fake, mut app, window) = focused_with(reader("\\033[?2004h"));
    paste_key(&mut fake, &mut app, window, "a\nb");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[200~a"));
    // Always asks, there too, and for a line with its line break.
    let (mut fake, mut app, window, _) = pasting(
        "always",
        "[clipboard]\nconfirm-paste = \"always\"\n",
        reader("\\033[?2004h"),
    );
    paste_key(&mut fake, &mut app, window, "ls\n");
    assert_eq!(fake.alerts().len(), 1);
    // Never asks nothing.
    let (mut fake, mut app, window, _) = pasting(
        "never",
        "[clipboard]\nconfirm-paste = \"never\"\n",
        reader(""),
    );
    paste_key(&mut fake, &mut app, window, "a\nb");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "a^Mb"));
}

#[test]
fn don_t_ask_again_writes_the_config() {
    let (mut fake, mut app, window, path) =
        pasting("again", "# mine\n[font]\nsize = 14\n", reader(""));
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    // Ticked, but cancelled: nothing changes.
    fake.answer_alert_with(&mut app, id, 2, true);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\n[font]\nsize = 14\n"
    );
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    fake.answer_alert_with(&mut app, id, 0, true);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.starts_with("# mine\n[font]\nsize = 14\n"),
        "{written}"
    );
    assert_eq!(
        Config::read(&path).unwrap().clipboard.confirm_paste,
        ConfirmPaste::Never
    );
    // The next paste pastes, before the file's reload too.
    paste_key(&mut fake, &mut app, window, "c\nd");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "a^Mbc^Md"));
    assert!(!app.pastes_unconfirmed);
}

#[test]
fn don_t_ask_again_lasts_until_the_app_quits_where_the_config_cannot_keep_it() {
    let text = "[font]\nsise = 14\n";
    let (mut fake, mut app, window, path) = pasting("bad", text, reader(""));
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    fake.answer_alert_with(&mut app, id, 1, true);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert!(app.pastes_unconfirmed);
    // A reload of the file does not bring the question back.
    app.apply(&mut fake, Config::default());
    paste_key(&mut fake, &mut app, window, "c\nd");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "a bc^Md"));
}

#[test]
fn a_window_that_closes_drops_its_paste_s_question() {
    let (mut fake, mut app, window) = focused_with(reader(""));
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    app.run(&mut fake, NEW_WINDOW, Args::new());
    // Its program, `cat`, runs: the question, answered.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert!(app.paste_asks.is_empty());
    // A late answer reaches nothing.
    fake.answer_alert(&mut app, id, 0);
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn the_font_s_size_and_the_view_take_their_keys() {
    // More lines than the window's rows, for a scrollback.
    let (mut fake, mut app, window) =
        focused_with(shell("seq 1 200; echo ready; stty raw -echo; cat -v"));
    let named = |code, plain: &str, mods| key_event(code, plain, mods, true);
    fake.send(&mut app, window, named(KeyCode::Equal, "=", CMD));
    assert_eq!(app.font_size, Some(14.0));
    fake.send(&mut app, window, named(KeyCode::Minus, "-", CMD));
    fake.send(&mut app, window, named(KeyCode::Minus, "-", CMD));
    assert_eq!(app.font_size, Some(12.0));
    fake.send(&mut app, window, named(KeyCode::Digit0, "0", CMD));
    assert_eq!(app.font_size, None);
    // A line up, a page down, to the top.
    let offset = |app: &App| app.pane(window).snapshot().offset();
    fake.send(&mut app, window, named(KeyCode::ArrowUp, "", CMD));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 1);
    fake.send(&mut app, window, named(KeyCode::PageDown, "", SHIFT));
    run_until(&mut fake, &mut app, |_, app| offset(app) == 0);
    fake.send(&mut app, window, named(KeyCode::Home, "", CMD));
    run_until(&mut fake, &mut app, |_, app| offset(app) > 1);
}

#[test]
fn clearing_keeps_the_prompt_at_the_top() {
    let (mut fake, mut app, window) =
        focused_with(shell("seq 1 60; printf 'ready$'; stty raw -echo; cat -v"));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready$"));
    fake.send(&mut app, window, command_key(KeyCode::KeyK, "k"));
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 0) == "ready$"
    });
    let shown = screen(&app, window);
    assert!(shown[1..].iter().all(String::is_empty), "{shown:?}");
    // Nothing is left above it either, and typing goes on at the prompt.
    fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
    fake.send(&mut app, window, command_key(KeyCode::KeyC, "c"));
    assert_eq!(fake.clipboard_text().as_deref(), Some("ready$"));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyX, "x", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 0) == "ready$x"
    });
}

#[test]
fn a_binding_holds_in_its_context() {
    let keys = "\"ctrl+o\" = { command = \"window.new\", when = \"alternate-screen\" }\n";
    let program = shell(
        "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033[?1049h'; echo alt; cat -v",
    );
    let (mut fake, mut app, window) = bound(keys, program);
    // On the primary screen, the key is the program's.
    fake.send(&mut app, window, key_event(KeyCode::KeyO, "o", CTRL, true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "alt"));
    assert_eq!(fake.windows().count(), 1);
    fake.send(&mut app, window, key_event(KeyCode::KeyO, "o", CTRL, true));
    assert_eq!(fake.windows().count(), 2);
}

#[test]
fn a_reloaded_config_binds_its_keys_anew() {
    let (mut fake, mut app, path) = with_file("keys", "[keys]\n\"cmd+n\" = false\n");
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, key_event(KeyCode::KeyN, "n", CMD, true));
    assert_eq!(fake.windows().count(), 1, "the default taken away");
    std::fs::write(&path, "").unwrap();
    change(&mut fake, &mut app);
    fake.send(&mut app, window, key_event(KeyCode::KeyN, "n", CMD, true));
    assert_eq!(fake.windows().count(), 2);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn an_invalid_config_shows_its_notice_until_one_that_reads() {
    let (mut fake, mut app, path) = with_file("notice", "[font]\nsize = 14\n");
    let window = fake.windows().next().unwrap();
    assert_eq!(app.diagnostics.shown(), None);
    // The error, of its line, and nothing of the file applies.
    std::fs::write(&path, "[font]\nsize = 15\nbogus = 1\n").unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    change(&mut fake, &mut app);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(
        (shown.code, &shown.about),
        (Code::InvalidConfig, &About::Config)
    );
    assert!(shown.text.contains("config.toml:3:1: "), "{}", shown.text);
    assert!(shown.text.ends_with("and the one in force stays"));
    assert_eq!(app.config.font.size.points(), 14.0);
    assert!(fake.window(window).unwrap().frame_requested);
    // Another error takes its place; a config that reads takes it away.
    std::fs::write(&path, "[font]\nsize = 1000\n").unwrap();
    change(&mut fake, &mut app);
    let again = app.diagnostics.shown().cloned().expect("a notice");
    assert_ne!(again.text, shown.text);
    std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    assert_eq!(app.config.font.size.points(), 16.0);
    // Back to the config in force, as after an error, takes it away too.
    std::fs::write(&path, "bogus = 1\n").unwrap();
    change(&mut fake, &mut app);
    std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_font_that_is_not_installed_shows_its_notice_until_one_that_is() {
    let family = |name: &str| Some(name.to_owned().try_into().unwrap());
    let mut config = Config::default();
    config.font.family = family("No Such Family 42");
    let drawing = Drawing::new(config.font.clone()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), config, None).with_program(quiet());
    fake.start(&mut app);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(
        (shown.code, &shown.about),
        (Code::Unavailable, &About::Font)
    );
    assert!(
        shown
            .text
            .starts_with("the font family `No Such Family 42` is not installed; the text takes `")
    );
    // A family that is there takes it away.
    let mut config = app.config.clone();
    config.font.family = family("Menlo");
    app.apply(&mut fake, config);
    assert_eq!(app.diagnostics.shown(), None);
}

#[test]
fn a_failure_shows_its_notice_and_a_click_on_the_bar_closes_it() {
    let (mut fake, mut app, window, _) = with_tester();
    // A command that the caller named wrongly goes to the log alone.
    app.run(&mut fake, "test.nothing", Args::new());
    assert_eq!(app.diagnostics.shown(), None);
    // A plugin that panics: a fault, which disables it.
    app.run(&mut fake, "test.boom", Args::new());
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(shown.code, Code::Fault);
    assert!(
        shown.text.contains("panicked: on purpose"),
        "{}",
        shown.text
    );
    // Where the bar was drawn, a press on its close closes it; on the
    // rest of it, nothing; neither reaches the program, nor starts a
    // selection.
    let rect = |x, width| Rect {
        x,
        y: 56,
        width,
        height: 40,
    };
    let rects = notice::Rects {
        bar: rect(0, 800),
        close: rect(780, 20),
    };
    app.windows.get_mut(&window).unwrap().notice = Some(rects);
    let scale = fake.window(window).unwrap().size.scale;
    let at = |x: f64, y: f64| Point {
        x: x / scale,
        y: y / scale,
    };
    let press = |position| WindowEvent::MouseButton {
        button: lokyr_platform_core::MouseButton::Left,
        pressed: true,
        clicks: 1,
        position,
        mods: Modifiers::NONE,
    };
    fake.send(&mut app, window, press(at(10.0, 70.0)));
    assert!(app.diagnostics.shown().is_some());
    assert!(app.windows[&window].selecting.is_none());
    fake.send(&mut app, window, press(at(790.0, 70.0)));
    assert_eq!(app.diagnostics.shown(), None);
    assert!(app.windows[&window].selecting.is_none());
    // A failure of a command shows too; a press below the bar leaves it.
    let failed = Diagnostic::new(Code::Failed, "it failed", About::Other);
    app.report(&mut fake, failed);
    fake.send(&mut app, window, press(at(10.0, 120.0)));
    assert!(app.diagnostics.shown().is_some());
    assert!(app.windows[&window].selecting.is_some());
}

#[test]
fn a_clean_exit_closes_the_window_and_another_keeps_it_until_a_key() {
    let (fake, _, _) = ended(CloseOnExit::Clean, "exit 0");
    assert_eq!(fake.windows().count(), 0);
    assert!(fake.has_quit());

    let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "printf 'oops'; exit 3");
    assert_eq!(fake.windows().count(), 1);
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
    });
    assert_eq!(row(&app, window, 0), "oops");
    fake.send(&mut app, window, key());
    assert_eq!(fake.windows().count(), 0);
    assert!(fake.has_quit());
}

#[test]
fn a_program_that_cannot_start_leaves_a_window_that_a_key_closes() {
    let mut fake = Fake::default();
    let missing = Program::command("/nonexistent/lokyr-program", [] as [&str; 0]);
    let mut app = app(Config::default(), missing);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    assert!(row(&app, window, 0).starts_with("[the program cannot start"));
    fake.send(&mut app, window, key());
    assert_eq!(fake.windows().count(), 0);
}

#[test]
fn the_line_of_a_program_that_ended_reads_whatever_it_left_set() {
    // The line drawing character set, insert mode and no autowrap.
    let script = "printf '\\033(0\\033[4h\\033[?7l'; exit 3";
    let (mut fake, mut app, window) = ended(CloseOnExit::Clean, script);
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
    });
}

#[test]
fn always_closes_and_never_keeps() {
    let (fake, _, _) = ended(CloseOnExit::Always, "exit 3");
    assert_eq!(fake.windows().count(), 0);
    let (fake, app, window) = ended(CloseOnExit::Never, "exit 0");
    assert_eq!(fake.windows().count(), 1);
    assert!(app.pane(window).is_kept());
    // A program that a signal ends.
    let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "kill -9 $$");
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 1) == "[the program ended by signal 9; a key closes the window]"
    });
}

#[test]
fn the_input_method_s_text_shows_then_reaches_the_program() {
    use lokyr_platform_core::KeyCode;

    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    // The candidates' place: the cursor's cell, at the top left within
    // the padding of 4 points, in the points of cells of 8 by 16 pixels
    // at a scale of 2.
    let (origin, size) = fake.window(window).unwrap().text_area.unwrap();
    assert_eq!((origin.x, origin.y), (4.0, 4.0));
    assert_eq!((size.width, size.height), (4.0, 8.0));
    let composing = Ime::Preedit {
        text: "にほ".to_owned(),
        selection: 6..6,
    };
    fake.send(&mut app, window, WindowEvent::Ime(composing));
    assert_eq!(
        app.windows[&window].preedit,
        Some(Preedit {
            text: "にほ".to_owned(),
            cursor: 6
        })
    );
    assert!(fake.window(window).unwrap().frame_requested);
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("日本".to_owned())),
    );
    assert_eq!(app.windows[&window].preedit, None);
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    // `cat` writes back the line.
    run_until(&mut fake, &mut app, |_, app| row(app, window, 1) == "日本");
    // An empty preedit ends the composition with nothing typed.
    let composing = Ime::Preedit {
        text: "k".to_owned(),
        selection: 1..1,
    };
    fake.send(&mut app, window, WindowEvent::Ime(composing));
    let ended = Ime::Preedit {
        text: String::new(),
        selection: 0..0,
    };
    fake.send(&mut app, window, WindowEvent::Ime(ended));
    assert_eq!(app.windows[&window].preedit, None);
}

#[test]
fn output_starts_the_blinking_again() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell("sleep 0.3; printf x; sleep 10"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let focused = Instant::now();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "x");
    // The output came at least 0.3 s after the focus: 15.1 s after the
    // focus, the cursor still blinks, as its timeout counts from it.
    let later = focused + cursor::BLINK_TIMEOUT + Duration::from_millis(100);
    app.cursor(&mut fake, window, later).map(|(shape, _)| shape);
    assert!(app.windows[&window].blinking);
}

/// A directory of the test's own, made empty.
fn directory(test: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lokyr-app-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

/// The startup lines of a shell whose prompt is as oh-my-posh draws it
/// with its shell integration: three lines of output above it, then a
/// first line computed once, as wide as the terminal, `left` and
/// `right` at its edges, and the input line, marked with OSC 133.
const PROMPT_OUTPUT: &str = "for n in 1 2 3; do echo \"output $n\"; done\n\
    cols=$(stty size | cut -d' ' -f2)\n\
    first=\"left$(printf '%*s' $((cols - 9)) '')right\"\n";

/// Drags the window of `app` through `widths`, in points, a step every
/// 8 ms, as a live resize does.
fn drag(fake: &mut Fake, app: &mut App, window: WindowId, widths: &[f64]) {
    for pair in widths.windows(2) {
        for step in 0..=20 {
            let width = pair[0] + (pair[1] - pair[0]) * f64::from(step) / 20.0;
            fake.send(app, window, WindowEvent::Resized(points(width, 500.0)));
            let until = Instant::now() + Duration::from_millis(8);
            run_until(fake, app, |_, _| Instant::now() > until);
        }
    }
}

/// Runs the shell that `command` starts through a drag from 200
/// columns to 75, 175 and 100, and waits for its screen to show
/// `expected`, its rows without their spaces at either end, and to keep
/// it a while.
fn prompt_through_a_drag(command: &str, expected: &[&str]) {
    let mut fake = Fake::default();
    let mut app = app(unpadded(), shell(command));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let trimmed = |app: &App| -> Vec<String> {
        screen(app, window)
            .iter()
            .take(expected.len())
            .map(|line| line.trim().to_owned())
            .collect()
    };
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window).iter().any(|line| line.starts_with('>'))
    });
    drag(&mut fake, &mut app, window, &[800.0, 300.0, 700.0, 400.0]);
    run_until(&mut fake, &mut app, |_, app| trimmed(app) == expected);
    let until = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > until);
    assert_eq!(trimmed(&app), expected);
}

#[test]
fn a_zsh_prompt_draws_once_through_a_drag() {
    let directory = directory("zsh-prompt");
    let rc = format!(
        r#"{PROMPT_OUTPUT}PROMPT=$'%{{\e]133;D;0\a\e]133;A\a%}}'"$first"$'\n> %{{\e]133;B\a%}}'
preexec() {{ print -n '\e]133;C\a' }}
print -z 'echo typed'
"#
    );
    std::fs::write(directory.join(".zshrc"), rc).unwrap();
    let zsh = format!("ZDOTDIR={} exec /bin/zsh -d -i", directory.display());
    // The first line, of 200 columns, takes two rows at 100; zsh draws
    // the prompt again, with what was typed, where the cleared one was.
    let expected = [
        "output 1",
        "output 2",
        "output 3",
        "left",
        "right",
        "> echo typed",
        "",
    ];
    prompt_through_a_drag(&zsh, &expected);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_bash_prompt_keeps_the_lines_that_bash_does_not_draw_again() {
    let directory = directory("bash-prompt");
    let rc = directory.join("rc");
    let prompt = format!(
        r#"{PROMPT_OUTPUT}PS1='\[\e]133;D;0\a\e]133;A;redraw=last\a\]'"$first"'\n> \[\e]133;B\a\]'
"#
    );
    std::fs::write(&rc, prompt).unwrap();
    let bash = format!(
        "BASH_SILENCE_DEPRECATION_WARNING=1 exec /bin/bash --noprofile --rcfile {} -i",
        rc.display()
    );
    // bash draws the last line again, and the first reflows.
    let expected = ["output 1", "output 2", "output 3", "left", "right", ">", ""];
    prompt_through_a_drag(&bash, &expected);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn blinking_text_keeps_the_blinking_s_timer_in_the_focused_window() {
    // A cursor that does not blink, over text that does.
    let config = |text_blink: bool| {
        let mut config = Config::default();
        config.cursor.blink = false;
        config.text.blink = text_blink;
        config
    };
    let script = "printf '\\033[5mblink\\033[0m'; sleep 10";
    for (text_blink, focused, timer) in [
        (true, true, true),
        (false, true, false),
        (true, false, false),
    ] {
        let mut fake = Fake::default();
        let mut app = app(config(text_blink), shell(script));
        fake.start(&mut app);
        let window = fake.windows().next().unwrap();
        fake.send(&mut app, window, WindowEvent::Focused(focused));
        run_until(&mut fake, &mut app, |_, app| row(app, window, 0) == "blink");
        fake.send(&mut app, window, WindowEvent::Frame);
        assert_eq!(app.blink_timer.is_some(), timer, "{text_blink} {focused}");
        assert_eq!(app.windows[&window].blinking, timer);
    }
}

#[test]
fn other_commands_do_nothing() {
    let (mut fake, mut app) = started();
    fake.send_app(&mut app, AppEvent::Command("unknown".to_owned()));
    assert!(!fake.has_quit());
    assert_eq!(fake.windows().count(), 1);
}

/// The quick terminal's window, if the app opened it.
fn quick_window(fake: &Fake) -> Option<WindowId> {
    fake.windows().find(|&window| {
        fake.window(window)
            .is_some_and(|state| state.quick.is_some())
    })
}

fn toggle(fake: &mut Fake, app: &mut App) {
    fake.send_app(app, AppEvent::Command(TOGGLE_QUICK.to_owned()));
}

#[test]
fn the_quick_terminal_shows_and_hides_keeping_its_pane() {
    let (mut fake, mut app) = started();
    assert_eq!(quick_window(&fake), None);
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    let shown = |fake: &Fake| fake.window(quick).unwrap().quick.unwrap();
    // At the top, of half the area under the menu bar, its pane of
    // that size from the start.
    assert_eq!(
        shown(&fake),
        Some(lokyr_platform_core::QuickPlace::default())
    );
    let size = fake.window(quick).unwrap().size;
    assert_eq!((size.points.width, size.points.height), (1440.0, 438.0));
    let pane = app.windows[&quick].pane;
    assert_eq!(app.grid(size), app.pane(quick).grid());
    // With the focus, it hides; its pane stays.
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    toggle(&mut fake, &mut app);
    assert_eq!(shown(&fake), None);
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert_eq!(app.windows[&quick].pane, pane);
    // It shows again; without the focus, it takes it rather than
    // hiding.
    toggle(&mut fake, &mut app);
    assert!(shown(&fake).is_some());
    toggle(&mut fake, &mut app);
    assert!(shown(&fake).is_some());
    assert_eq!(quick_window(&fake), Some(quick));
    // Losing the focus hides it, as `autohide` says.
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert_eq!(shown(&fake), None);
    // Closed, the next shows a new one.
    app.close(&mut fake, quick);
    assert_eq!(quick_window(&fake), None);
    assert!(!fake.has_quit());
    toggle(&mut fake, &mut app);
    assert_ne!(quick_window(&fake), Some(quick));
}

#[test]
fn the_quick_terminal_stays_as_it_loses_the_focus_without_autohide_or_to_a_question() {
    let mut config = Config::default();
    config.quick_terminal.autohide = false;
    config.quick_terminal.position = config::QuickPosition::Left;
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    let place = fake.window(quick).unwrap().quick.unwrap().unwrap();
    assert_eq!(place.edge, lokyr_platform_core::QuickEdge::Left);
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
    // With autohide, a question on it, which takes the focus, keeps it.
    app.config.quick_terminal.autohide = true;
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    app.ask_title(&mut fake, quick);
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
}

#[test]
fn a_global_key_runs_its_command_from_every_app_and_keeps_the_app_open() {
    let config = Config::parse(
        "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n\"global:ctrl+f24\" = \"window.new\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let key = GlobalKey {
        code: KeyCode::Backquote,
        command: false,
        control: true,
        alt: false,
        shift: false,
    };
    let f24 = GlobalKey {
        code: KeyCode::F(24),
        ..key
    };
    assert_eq!(fake.global_keys(), [key, f24]);
    // The one that the system refused, of a key that the Mac lacks, is
    // told.
    let shown = app.diagnostics.shown().unwrap();
    assert_eq!(shown.about, About::GlobalKeys);
    assert!(shown.text.contains("`global:ctrl+f24`"), "{}", shown.text);
    fake.send_app(&mut app, AppEvent::GlobalKey(0));
    assert!(quick_window(&fake).is_some());
    // Without its windows, the app waits for the key, and opens a
    // window as the Dock's icon asks.
    let quick = quick_window(&fake).unwrap();
    app.close(&mut fake, quick);
    app.close(&mut fake, window);
    assert_eq!(fake.windows().count(), 0);
    assert!(!fake.has_quit());
    fake.send_app(&mut app, AppEvent::Reopen);
    assert_eq!(fake.windows().count(), 1);
    // A key that another app takes is told too; once the config takes
    // it away, the notice goes, and the last window quits again.
    fake.take_global_key(key);
    app.apply(
        &mut fake,
        Config::parse(
            "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap(),
    );
    let shown = app.diagnostics.shown().unwrap();
    assert!(
        shown.text.contains("`global:ctrl+`` cannot"),
        "{}",
        shown.text
    );
    app.apply(&mut fake, Config::default());
    assert!(app.diagnostics.shown().is_none());
    assert!(fake.global_keys().is_empty());
    // A key that the system refused keeps nothing open.
    let refused = Config::parse(
        "[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    app.apply(&mut fake, refused);
    assert_eq!(fake.global_keys(), [key]);
    let window = fake.windows().next().unwrap();
    app.close(&mut fake, window);
    assert!(fake.has_quit());
}

#[test]
fn without_a_global_key_the_last_window_quits_with_the_hidden_quick_terminal() {
    // Its program asked about first.
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    toggle(&mut fake, &mut app);
    assert_eq!(fake.window(quick).unwrap().quick, Some(None));
    // A notification's click shows it, rather than focusing it hidden.
    fake.send_app(&mut app, AppEvent::NotificationClicked(quick));
    assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
    assert!(fake.focus_requests().is_empty());
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert_eq!(fake.window(quick).unwrap().quick, Some(None));
    app.close(&mut fake, window);
    assert!(!fake.has_quit());
    let [question] = fake.alerts() else {
        panic!("the question before quitting: {:?}", fake.alerts());
    };
    let question = question.id;
    fake.answer_alert(&mut app, question, 0);
    assert!(fake.has_quit());
    // A shell at its prompt quits at once; a quick terminal that shows
    // keeps the app.
    let (mut fake, mut app, window) = started_running(idle());
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, quick).iter().any(|line| line.contains("ready"))
    });
    app.close(&mut fake, window);
    assert!(!fake.has_quit());
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    toggle(&mut fake, &mut app);
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let window = fake.windows().find(|&opened| opened != quick).unwrap();
    app.close(&mut fake, window);
    assert!(fake.has_quit());
}

#[test]
fn a_pane_s_output_asks_for_its_own_window_s_frame_alone() {
    let (mut fake, mut app, first) = started_running(idle());
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let second = fake.windows().find(|&window| window != first).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, second)
            .iter()
            .any(|line| line.contains("ready"))
    });
    for window in [first, second] {
        if fake.window(window).unwrap().frame_requested {
            fake.send(&mut app, window, WindowEvent::Frame);
        }
    }
    // The second's shell echoes a key: its window's frame, not the
    // first's.
    fake.send(&mut app, second, typed(KeyCode::KeyX, "x", false));
    let start = Instant::now();
    while !screen(&app, second).iter().any(|line| line.ends_with('x')) {
        assert!(start.elapsed() < Duration::from_secs(10), "no echo");
        fake.deliver_wake(&mut app);
        assert!(!fake.window(first).unwrap().frame_requested);
        if fake.window(second).unwrap().frame_requested {
            fake.send(&mut app, second, WindowEvent::Frame);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn a_stream_of_bells_wakes_the_main_thread_at_the_frame_s_pace() {
    let script = "echo ready; read line; while :; do printf '\\a\\a\\a\\a'; done";
    let (mut fake, mut app, window) = started_running(shell(script));
    let pane = app.windows[&window].pane;
    let wake = Arc::clone(&app.pane_wakes[&pane]);
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    // The main thread as it runs, which takes each wake at once, and a
    // display of 60 Hz, which draws the window while it shows.
    let run = |fake: &mut Fake, app: &mut App, drawn: bool| {
        let before = wake.wakes.load(Ordering::Relaxed);
        let start = Instant::now();
        let mut frames = 0;
        let mut frame_at = start;
        while start.elapsed() < Duration::from_millis(500) {
            fake.deliver_wake(app);
            fake.fire_timers(app, Instant::now());
            if drawn
                && frame_at.elapsed() >= TOLD_PACE
                && fake.window(window).unwrap().frame_requested
            {
                fake.send(app, window, WindowEvent::Frame);
                frames += 1;
                frame_at = Instant::now();
            }
        }
        (wake.wakes.load(Ordering::Relaxed) - before, frames)
    };
    // Drawn: a wake a frame for the grid, and one a pace for the bells,
    // at most.
    let (wakes, frames) = run(&mut fake, &mut app, true);
    assert!(app.windows[&window].rang_at.is_some(), "the bell rang");
    assert!(wakes <= frames + 35, "{wakes} wakes for {frames} frames");
    // Not drawn: one a pace for the bells.
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    let (wakes, _) = run(&mut fake, &mut app, false);
    assert!(wakes <= 35, "{wakes} wakes hidden");
}

#[test]
fn a_frame_with_nothing_new_presents_nothing() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(idle());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    run_until(&mut fake, &mut app, |_, app| {
        screen(app, window)
            .iter()
            .any(|line| line.contains("ready"))
    });
    let drawn = |app: &App| app.windows[&window].drawn_at;
    fake.send(&mut app, window, WindowEvent::Frame);
    let first = drawn(&app);
    assert!(first.is_some());
    // The same again: nothing presents, and the surface keeps it.
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(drawn(&app), first);
    // The focus's cursor is new.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_ne!(drawn(&app), first);
}

#[test]
fn a_frame_and_a_key_lock_the_pane_once_each() {
    let (mut fake, mut app, window) = started_running(idle());
    let locks = |app: &App| app.pane(window).locks();
    // A frame copies the screen, and reads the rest from the copy.
    let before = locks(&app);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(locks(&app) - before, 1);
    // A key resolves in the frame's contexts, and locks to write.
    let before = locks(&app);
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    assert_eq!(locks(&app) - before, 1);
    let before = locks(&app);
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", true));
    assert_eq!(
        locks(&app) - before,
        0,
        "Command's key, which the program does not take"
    );
}

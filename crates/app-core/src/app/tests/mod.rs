use lokyr_kernel::l10n::Arg;
use lokyr_kernel::workspace::Grid;
use lokyr_platform_core::fake::{Fake, FakeDefinition};
use lokyr_platform_core::{
    Dropped, Ime, KeyCode, KeyEvent, KeyState, MenuItem, MenuRole, Modifiers, MouseButton,
    ScrollDelta, Shortcut, ShortcutKey,
};
use lokyr_platform_core::{Easing, Point, PointerShape};
use lokyr_render_core::{Color, CursorShape, Item, Rect};
use lokyr_ui_core::cursor;
use lokyr_ui_core::cursor::CursorConfig;
use lokyr_ui_core::pane::Preedit;

use super::asks::{invisible, shown};
use super::frame::{appearance_over, bar_item};
use super::menus::{main_menu, man_keywords, man_page, man_program, shortened, unique_titles};
use super::*;
use crate::{fakes, notice};

mod access;
mod asks;
mod chrome;
mod configs;
mod consent;
mod copy;
mod folders;
mod frame;
mod input;
mod installed;
mod marks;
mod menus;
mod notices;
mod numbers;
mod opens;
mod panels;
mod panes;
mod parts;
mod reloads;
mod restore;
mod scrollbars;
mod surfaces;
#[cfg(feature = "plugin-tabs")]
mod tabs;
mod themes;
mod tools;
mod workers;

/// The app of the tests, which draws with the tests' drawing, if at all.
type App = super::App<fakes::Gpu>;
type Drawing = super::Drawing<fakes::Gpu>;

/// Lets `window`'s screen show its terminal past the wait for its
/// program's drawing at a new size, which the tests' programs never do,
/// with a frame.
fn settle(fake: &mut Fake, app: &mut App, window: WindowId) {
    std::thread::sleep(lokyr_kernel::workspace::ANSWER_WAIT);
    fake.send(app, window, WindowEvent::Frame);
}

/// A program that runs until its terminal hangs up, and writes nothing.
/// What a test that makes VMs of Luau, or that counts them, holds, so that
/// the counts of one are not another's.
static LUAU: Mutex<()> = Mutex::new(());

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
fn settings_opens_the_config_in_the_text_editor_made_first() {
    let (mut fake, mut app, path) = with_file("settings", "");
    std::fs::remove_file(&path).unwrap();
    // The application's menu has it first after About and the updates, of
    // its key.
    assert_eq!(
        menu_items(&fake, 0)[3..8],
        [
            "Settings… cmd+,",
            "Secure Keyboard Entry",
            "Make Lokyr the Default Terminal",
            "Install Command Line Tools…",
            "-"
        ]
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
    let builtins: Vec<Box<dyn plugins::Plugin + Send>> = super::registrations()
        .iter()
        .map(|registration| (registration.make)())
        .collect();
    assert_eq!(builtins.len(), 11);
    for plugin in &builtins {
        let catalogs = plugin.catalogs();
        // A plugin of no commands, as the keymap's, has no titles.
        if catalogs.is_empty() {
            continue;
        }
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
        .map(|(command, _, _)| (command.name, command.title))
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
fn the_updates_start_with_the_config_s_checks_and_the_menu_checks_now() {
    let directory = std::env::temp_dir().join(format!("lokyr-app-updates-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[updates]\ncheck = false\n").unwrap();
    let mut fake = Fake::default();
    fake.set_can_update(true);
    let mut app =
        App::new(None, Config::read(&path).unwrap(), Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    // Started once the window opened, the checks off as the file says.
    assert_eq!(fake.windows().count(), 1);
    assert_eq!(fake.update_checks(), Some(false));
    assert!(fake.command_enabled(CHECK_UPDATES));
    // The menu's item checks now, whatever the key says.
    fake.send_app(&mut app, AppEvent::Command(CHECK_UPDATES.to_owned()));
    assert_eq!(fake.updates_checked(), 1);
    // The key changed turns the checks on, at once.
    std::fs::write(&path, "[updates]\ncheck = true\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(fake.update_checks(), Some(true));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn about_opens_the_panel_of_the_license_the_repository_and_the_notices() {
    let (mut fake, mut app) = started();
    fake.send_app(&mut app, AppEvent::Command(ABOUT.to_owned()));
    let [about] = fake.abouts() else {
        panic!("one panel: {:?}", fake.abouts());
    };
    let lines: Vec<(&str, Option<TargetKind>)> = about
        .credits
        .iter()
        .map(|line| {
            (
                line.text.as_str(),
                line.link.as_ref().map(|target| target.kind),
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("Released under the MIT License.", None),
            ("Source code on GitHub", Some(TargetKind::Url)),
        ]
    );
    assert_eq!(about.credits[1].link.as_ref().unwrap().location, REPOSITORY);
    // In a bundle, the notices of the licenses of what it includes, the
    // directory of its resources, which a click opens.
    let directory = std::env::temp_dir().join(format!("lokyr-about-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("licenses")).unwrap();
    app.environment.resources = Some(directory.clone());
    fake.send_app(&mut app, AppEvent::Command(ABOUT.to_owned()));
    let licenses = &fake.abouts()[1].credits[2];
    assert_eq!(licenses.text, "Licenses of the included software");
    let target = licenses.link.as_ref().unwrap();
    assert_eq!(target.kind, TargetKind::Directory);
    assert_eq!(
        std::path::Path::new(&target.location),
        directory.join("licenses").canonicalize().unwrap()
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn report_an_issue_opens_a_new_issue_of_the_version_and_the_system() {
    let (mut fake, mut app) = started();
    fake.send_app(&mut app, AppEvent::Command(REPORT_ISSUE.to_owned()));
    let [url] = fake.opened() else {
        panic!("one page opened: {:?}", fake.opened());
    };
    assert!(url.starts_with("https://github.com/sephiroth74/lokyr/issues/new?body="));
    let version = query_encoded(&format!("**Lokyr**: {}", app.environment.version));
    assert!(url.contains(&version), "{url}");
    assert!(
        url.contains(&query_encoded("**System**: Fake 1.0")),
        "{url}"
    );
}

#[test]
fn a_query_encodes_all_but_the_unreserved() {
    assert_eq!(
        query_encoded("a-b.c_d~e f\n**x**: 0.3.0/é"),
        "a-b.c_d~e%20f%0A%2A%2Ax%2A%2A%3A%200.3.0%2F%C3%A9"
    );
}

#[test]
fn the_checks_start_off_of_a_config_that_fell_back_and_follow_it_once_it_applies() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-fell-back-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    // The user turned the checks off, in a file of an error, which gives
    // the defaults, as the app starts.
    std::fs::write(&path, "[updates]\ncheck = false\nnot-a-key = 1\n").unwrap();
    let error = Config::read(&path).unwrap_err();
    let mut diagnostics = lokyr_kernel::diagnostics::Diagnostics::default();
    diagnostics.report(Diagnostic::new(
        Code::InvalidConfig,
        error.to_string(),
        About::Config,
    ));
    let mut fake = Fake::default();
    fake.set_can_update(true);
    let mut app = App::new(None, Config::default(), Some(path.clone()))
        .with_program(quiet())
        .with_diagnostics(diagnostics);
    fake.start(&mut app);
    assert!(app.config.updates.check);
    assert_eq!(fake.update_checks(), Some(false));
    // Fixed to the defaults, which check, the file applies, and so do they.
    std::fs::write(&path, "").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(fake.update_checks(), Some(true));
    // Turned off, as the file says.
    std::fs::write(&path, "[updates]\ncheck = false\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(fake.update_checks(), Some(false));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn an_app_that_cannot_update_itself_dims_the_menu_s_item() {
    let (mut fake, mut app) = started();
    assert!(!fake.command_enabled(CHECK_UPDATES));
    assert_eq!(fake.update_checks(), None);
    fake.send_app(&mut app, AppEvent::Command(CHECK_UPDATES.to_owned()));
    assert_eq!(fake.updates_checked(), 0);
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
    let (opened, pane) = (state.window, app.focused(window).unwrap());
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
    // The resize draws at once, of the new size; the frame after it draws
    // again only if what shows changed meanwhile, as the program's output
    // that comes when it comes, or the glyphs that the first frame's budget
    // left, which a loaded machine meets.
    fake.send(&mut app, window, WindowEvent::Resized(points(400.0, 300.0)));
    assert_eq!(frames(&app), 2);
    assert_eq!(shown(&app), ((800, 600), 2.0));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(shown(&app), ((800, 600), 2.0));
    // Closed, with its state and its workspace's window.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert!(app.windows.is_empty());
    assert!(app.workspace.window(opened).is_none());
    assert!(app.workspace.pane(pane).is_none());
}

#[test]
fn closing_the_last_window_leaves_the_app_open_unless_the_config_quits() {
    let (mut fake, mut app, window) = started_running(idle());
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().count(), 0);
    assert!(!fake.has_quit());
    // The Dock's icon opens a window, as a command does.
    fake.send_app(&mut app, AppEvent::Reopen);
    assert_eq!(fake.windows().count(), 1);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    assert_eq!(fake.windows().count(), 1);
    assert!(!fake.has_quit());
    // The config's quit, as it reloads.
    let quitting = Config::parse(
        "[app]\nquit-after-last-window-closed = true\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    app.apply(&mut fake, quitting);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().count(), 0);
    assert!(fake.has_quit());
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

/// The name of the program in the foreground that [`shell`] runs, as a
/// title tells it: macOS's `/bin/sh` runs bash, Linux's is dash, of the
/// name `sh`.
fn shell_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "bash"
    } else {
        "sh"
    }
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
fn a_split_starts_in_the_directory_that_its_pane_s_shell_told() {
    let base = real_directory("told-directory");
    let told = base.join("told");
    std::fs::create_dir(&told).unwrap();
    let home = std::env::home_dir().unwrap().display().to_string();
    // The first pane's shell tells, by OSC 7, a directory that its program,
    // `sleep`, does not work in; each pane tells where it started.
    let script = format!(
        "pwd; [ \"$PWD\" = {home:?} ] && printf '\\033]7;file://localhost%s\\007' {:?}; exec sleep 10",
        told.display().to_string()
    );
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell(&script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    assert_eq!(started_in(&mut fake, &mut app, window), home);
    let first = app
        .workspace
        .active_pane(app.windows[&window].window)
        .unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(first)
            .and_then(|pane| pane.told_directory(&lokyr_pty::host_name()))
            .is_some()
    });
    let mut right = Args::new();
    right.insert(
        "direction".to_owned(),
        lokyr_kernel::commands::Value::String("right".to_owned()),
    );
    fake.send(&mut app, window, WindowEvent::Focused(true));
    app.run(&mut fake, "splits.split", right);
    // The new pane, of the keyboard, started there, its program working
    // there.
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.active_pane(app.windows[&window].window) != Some(first)
            && app.pane(window).working_directory().as_deref() == Some(told.as_path())
    });
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
    // Its tab is marked of it, for a tab bar to show.
    let tab = app
        .workspace
        .window(app.windows[&window].window)
        .unwrap()
        .active_tab();
    let marked = |app: &App| app.workspace.tab(tab).unwrap().marks().secure;
    assert!(marked(&app));
    // Without the focus, off, and the lock goes; back, on again.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    assert!(!on(&fake) && !title(&fake).starts_with(SECURE_MARK));
    assert!(!marked(&app));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert!(on(&fake) && title(&fake).starts_with(SECURE_MARK));
    assert!(marked(&app));
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
    // A language that the system has no locale of, then one that macOS
    // has; elsewhere, what the system's locales give, as a Linux of
    // English's alone gives `en_US.UTF-8`.
    fake.set_locales(&["xx_XX", "it_CH"]);
    let script = "printf '%s' \"$LOKYR_RESOURCES_DIR\"; sleep 10";
    let mut app = app(Config::default(), shell(script)).with_resources(Some(resources.clone()));
    fake.start(&mut app);
    let expected = if cfg!(target_os = "macos") {
        "it_CH.UTF-8".to_owned()
    } else {
        crate::environment::utf8_locale(
            &["xx_XX".to_owned(), "it_CH".to_owned()],
            &crate::environment::system_locales(),
        )
    };
    assert_eq!(app.environment.locale, expected);
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
#[cfg_attr(
    not(target_os = "macos"),
    ignore = "macOS's `login` alone is set-user-ID root, of a directory that the system does not tell"
)]
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
        title(fake) == format!("x{TITLE_DASH}{}{}", shell_name(), size(app))
    });
    // A title of nothing gives the window's own.
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, app| {
        !title(fake).starts_with('x') && title(fake) == app.title_of(window)
    });
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
    config.window.gutter = false;
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
    // The program learns of the first at once, then of one a pace at most,
    // the terminal taking each with its signal.
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
    assert_eq!(app.pane(window).grid(), last);
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
    settle(&mut fake, &mut app, window);
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
    settle(&mut fake, &mut app, window);
    assert_eq!(screen(&app, window), before);
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
    // More lines than the window's rows, for a scrollback; `ready` once
    // the terminal is raw, so that no key meets the line's editing.
    let program = shell("seq 1 200; stty raw -echo; echo ready; cat -v");
    let (mut fake, mut app, window) = bound(keys, program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    // A view up the scrollback comes back down, as typing brings it.
    let offset = |app: &App| app.pane(window).snapshot().offset();
    let shift_cmd = Modifiers { shift: true, ..CMD };
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowUp, "", shift_cmd, true),
    );
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
/// focused window; `test.again` runs itself, and `test.twice` runs itself
/// twice, counting their runs.
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
            "test.twice",
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
            "test.twice" => {
                self.runs.set(self.runs.get() + 1);
                cx.run_command("test.twice", Args::new())?;
                cx.run_command("test.twice", Args::new())
            }
            // A binding that takes a key of the app's own.
            "test.bind" => cx.bind(plugins::host::Binding {
                keys: "cmd+n".to_owned(),
                command: "test.size".to_owned(),
                args: Args::new(),
                when: None,
            }),
            _ => {
                // The first row's text, which a selection, applied as the
                // call returns, would not give in the same call.
                let pane = cx.focused_pane().expect("a focused pane");
                let line = cx.line_text(pane, 0, 0)?.unwrap_or_default();
                cx.set_clipboard_text(line.text.trim_end())?;
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
        app.plugins.status("lokyr/core-actions"),
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
    // One that runs itself twice, at each step, stops as its chain ran as
    // many as the app lets one chain run, not 2^16 times; and the next
    // chain, the user's next run, runs as many again.
    runs.set(0);
    app.run(&mut fake, "test.twice", Args::new());
    assert_eq!(runs.get(), COMMANDS_WIDE + 1);
    runs.set(0);
    app.run(&mut fake, "test.twice", Args::new());
    assert_eq!(runs.get(), COMMANDS_WIDE + 1);
}

/// A plugin whose activation runs its command `fan.twice` 8 times, which
/// runs itself twice, its runs counted.
struct Fanner {
    runs: std::rc::Rc<std::cell::Cell<usize>>,
}

impl plugins::Plugin for Fanner {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "fan.twice".to_owned(),
            title: "Twice".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        for _ in 0..8 {
            cx.run_command("fan.twice", Args::new())?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        self.runs.set(self.runs.get() + 1);
        cx.run_command("fan.twice", Args::new())?;
        cx.run_command("fan.twice", Args::new())
    }
}

#[test]
fn a_call_s_runs_are_one_chain_however_many_it_asks() {
    // An activation's 8 runs run as many as one chain runs, not 8 times
    // as many.
    let runs = std::rc::Rc::default();
    let fanner = Fanner {
        runs: std::rc::Rc::clone(&runs),
    };
    let mut fake = Fake::default();
    let mut app = app(unpadded(), reader("")).with_plugin("fanner", Box::new(fanner));
    fake.start(&mut app);
    assert_eq!(runs.get(), COMMANDS_WIDE);
}

#[test]
fn a_chain_under_way_keeps_its_runs_however_many_chains_come_after_it() {
    let (_fake, mut app, _, _) = with_tester();
    let under_way = plugins::new_chain();
    app.chains.1.push(under_way.id);
    assert!(app.may_run(under_way, "test.twice"));
    // Many chains after it, as calls within its commands' would start.
    for _ in 0..4 * CHAINS_KEPT {
        assert!(app.may_run(plugins::new_chain(), "test.twice"));
    }
    assert!(app.chains.0.len() <= CHAINS_KEPT);
    // Its count went on: the rest of its runs, and no more.
    for _ in 1..COMMANDS_WIDE {
        assert!(app.may_run(under_way, "test.twice"));
    }
    assert!(!app.may_run(under_way, "test.twice"));
}

/// A plugin that binds its key again each time that it hears that the
/// keys changed, as one that binds a key where it is free would, and
/// registers a command of no key, the times counted.
struct Rebinder {
    heard: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Rebinder {
    fn bind(cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.bind(plugins::host::Binding {
            keys: "ctrl+r".to_owned(),
            command: "rebind.go".to_owned(),
            args: Args::new(),
            when: None,
        })
    }
}

impl plugins::Plugin for Rebinder {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "rebind.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        Self::bind(cx)?;
        cx.subscribe(&["keys-changed"])
    }

    fn on_event(
        &mut self,
        cx: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for _ in events.iter().filter(|event| event.name() == "keys-changed") {
            self.heard.set(self.heard.get() + 1);
            Self::bind(cx)?;
            cx.register_command(Command {
                name: format!("rebind.n{}", self.heard.get()),
                title: "N".to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }
}

#[test]
fn a_plugin_that_binds_as_the_keys_change_hears_it_as_they_change_alone() {
    let heard = std::rc::Rc::default();
    let rebinder = Rebinder {
        heard: std::rc::Rc::clone(&heard),
    };
    let mut fake = Fake::default();
    let mut app = app(unpadded(), reader("")).with_plugin("rebinder", Box::new(rebinder));
    fake.start(&mut app);
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(heard.get(), 1);
    assert_eq!(
        app.keymap
            .bindings()
            .iter()
            .filter(|binding| binding.invocation.command == "rebind.go")
            .count(),
        1
    );
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
    let shift_cmd = Modifiers { shift: true, ..CMD };
    fake.send(&mut app, window, named(KeyCode::ArrowUp, "", shift_cmd));
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
fn a_clean_exit_closes_the_window_and_another_keeps_it_until_a_key() {
    let (fake, _, _) = ended(CloseOnExit::Clean, "exit 0");
    assert_eq!(fake.windows().count(), 0);
    assert!(!fake.has_quit());

    let (mut fake, mut app, window) = ended(CloseOnExit::Clean, "printf 'oops'; exit 3");
    assert_eq!(fake.windows().count(), 1);
    run_until(&mut fake, &mut app, |_, app| {
        row(app, window, 1) == "[the program exited with code 3; a key closes the window]"
    });
    assert_eq!(row(&app, window, 0), "oops");
    fake.send(&mut app, window, key());
    assert_eq!(fake.windows().count(), 0);
    assert!(!fake.has_quit());
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
    // A system without zsh, as some Linux's.
    if !std::path::Path::new("/bin/zsh").exists() {
        eprintln!("zsh is not installed: the test checks nothing");
        return;
    }
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

/// The quick terminal's command, of its plugin.
const TOGGLE_QUICK: &str = "window.toggle-quick-terminal";

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
    let pane = app.focused(quick).unwrap();
    assert_eq!(app.grid(size), app.pane(quick).grid());
    // With the focus, it hides; its pane stays.
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    toggle(&mut fake, &mut app);
    assert_eq!(shown(&fake), None);
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert_eq!(app.focused(quick).unwrap(), pane);
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
fn a_quick_window_closes_as_its_plugin_stops_and_comes_back_alone() {
    let (mut fake, mut app) = started();
    let config = |text: &str| Config::parse(text, std::path::Path::new("config.toml")).unwrap();
    let quick_windows = |fake: &Fake| {
        fake.windows()
            .filter(|&window| {
                fake.window(window)
                    .is_some_and(|state| state.quick.is_some())
            })
            .count()
    };
    // Shown, then hidden.
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    toggle(&mut fake, &mut app);
    assert_eq!(fake.window(quick).unwrap().quick, Some(None));
    // Turned off, its window closes, shown first for the close's question
    // of a program that runs, which the user answers.
    app.apply(
        &mut fake,
        config("[plugins.quick-terminal]\nenabled = false\n"),
    );
    if let Some(alert) = fake.alerts().first() {
        assert_eq!(alert.window, Some(quick));
        assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
        let alert = alert.id;
        fake.send_app(
            &mut app,
            AppEvent::AlertClosed {
                alert,
                button: 0,
                suppressed: false,
                text: None,
            },
        );
    }
    assert!(fake.window(quick).is_none(), "the hidden window stayed");
    // On again, one quick window.
    app.apply(
        &mut fake,
        config("[plugins.quick-terminal]\nenabled = true\n"),
    );
    toggle(&mut fake, &mut app);
    assert_eq!(quick_windows(&fake), 1);
}

#[test]
fn the_quick_terminal_s_old_section_tells_its_new_place_and_its_plugin_s_applies() {
    let text = "[quick-terminal]\nposition = \"left\"\n\n[plugins.quick-terminal]\nposition = \"bottom\"\n";
    let config = Config::parse(text, std::path::Path::new("config.toml")).unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    let shown = app.diagnostics.shown().unwrap();
    assert_eq!(
        shown.about,
        About::PluginConfig("quick-terminal".to_owned())
    );
    assert!(
        shown.text.starts_with("config.toml:1:")
            && shown
                .text
                .contains("`[quick-terminal]` is now `[plugins.quick-terminal]`"),
        "{}",
        shown.text
    );
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    let place = fake.window(quick).unwrap().quick.unwrap().unwrap();
    assert_eq!(place.edge, lokyr_platform_core::QuickEdge::Bottom);
    // Moved, the notice goes.
    let text = "[plugins.quick-terminal]\nposition = \"bottom\"\n";
    app.apply(
        &mut fake,
        Config::parse(text, std::path::Path::new("config.toml")).unwrap(),
    );
    assert!(app.diagnostics.shown().is_none());
}

#[test]
fn the_quick_terminal_stays_as_it_loses_the_focus_without_autohide_or_to_a_question() {
    let section = |autohide: bool| {
        let text =
            format!("[plugins.quick-terminal]\nautohide = {autohide}\nposition = \"left\"\n");
        Config::parse(&text, std::path::Path::new("config.toml")).unwrap()
    };
    let config = section(false);
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
    app.apply(&mut fake, section(true));
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
    // it away, the notice goes.
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
    // The config's quit with the last window quits, though the system
    // gave the key.
    let quitting = Config::parse(
        "[app]\nquit-after-last-window-closed = true\n[keys]\n\"global:ctrl+`\" = \"window.toggle-quick-terminal\"\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    app.apply(&mut fake, quitting);
    assert_eq!(fake.global_keys(), [key]);
    let window = fake.windows().next().unwrap();
    app.close(&mut fake, window);
    assert!(fake.has_quit());
}

#[test]
fn the_config_s_quit_takes_the_hidden_quick_terminal_with_the_last_window() {
    // Its program asked about first.
    let (mut fake, mut app) = started();
    app.config.app.quit_after_last_window_closed = true;
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
    app.config.app.quit_after_last_window_closed = true;
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
    // By default, the app stays with the hidden quick terminal, and asks
    // nothing.
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    toggle(&mut fake, &mut app);
    app.close(&mut fake, window);
    assert!(!fake.has_quit());
    assert!(fake.alerts().is_empty());
    assert_eq!(fake.windows().collect::<Vec<_>>(), [quick]);
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

/// Opens with Command-click what `window` shows at `col` of its first
/// row.
fn cmd_click(fake: &mut Fake, app: &mut App, window: WindowId, col: u16) {
    fake.send(app, window, click(true, cell(col, 0), CMD, 1));
    fake.send(app, window, click(false, cell(col, 0), CMD, 1));
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
    // A plugin's item by its title, as its id is its place in a list.
    let dimmed = menu
        .dimmed
        .iter()
        .map(|id| {
            let title = menu.items.iter().find_map(|item| match item {
                MenuItem::Command { title, command, .. } if command == id => Some(title),
                _ => None,
            });
            match title {
                Some(title) if id.starts_with("menu-item:") => title.clone(),
                _ => id.clone(),
            }
        })
        .collect();
    (*position, items, dimmed)
}

/// A press and a release of the right button at `at`, with `mods`.
fn right_click(fake: &mut Fake, app: &mut App, window: WindowId, at: Point, mods: Modifiers) {
    fake.send(app, window, press(MouseButton::Right, true, at, mods));
    fake.send(app, window, press(MouseButton::Right, false, at, mods));
}

/// The text that `window`'s pane selects, as the terminal has it.
fn selected_text(app: &App, window: WindowId) -> Option<String> {
    app.pane(window).selection_text()
}

/// A plugin that subscribes to the windows' and the panes' events, and
/// records their names, a batch a line.
struct Watcher(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

impl plugins::Plugin for Watcher {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.subscribe(&[
            "window-opened",
            "window-closed",
            "pane-created",
            "pane-closed",
        ])
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        let names: Vec<&str> = events.iter().map(plugins::Event::name).collect();
        self.0.borrow_mut().push(names.join(" "));
        Ok(())
    }
}

#[test]
fn a_plugin_takes_the_workspace_s_changes_as_each_turn_ends() {
    let log = std::rc::Rc::default();
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell("read line"))
        .with_plugin("watcher", Box::new(Watcher(std::rc::Rc::clone(&log))));
    fake.start(&mut app);
    // The first window, as the start's turn ends.
    assert_eq!(*log.borrow(), ["window-opened pane-created"]);
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(log.borrow()[1], "window-opened pane-created");
    // Closed, with its pane, as its close's turn ends.
    let second = fake.windows().last().unwrap();
    fake.send(&mut app, second, WindowEvent::CloseRequested);
    // A question, if the shell's program is not the shell yet.
    if let Some(asked) = fake.alerts().first().map(|alert| alert.id) {
        fake.answer_alert(&mut app, asked, 0);
    }
    assert_eq!(log.borrow().len(), 3);
    assert_eq!(log.borrow()[2], "pane-closed window-closed");
    // A turn without changes delivers nothing.
    let first = fake.windows().next().unwrap();
    fake.send(&mut app, first, WindowEvent::Frame);
    assert_eq!(log.borrow().len(), 3);
}

/// The ids that [`Arranger`]'s commands got: the tab and its pane, and the
/// split's pane.
type Arranged = std::rc::Rc<
    std::cell::RefCell<
        Option<(
            plugins::host::TabId,
            plugins::host::PaneId,
            plugins::host::PaneId,
        )>,
    >,
>;

/// A plugin that opens a tab and a split, moves the split's pane out to a
/// window of its own and closes it, a command each, and records the names
/// of the workspace's events, a batch a line.
struct Arranger(Arranged, std::rc::Rc<std::cell::RefCell<Vec<String>>>);

impl plugins::Plugin for Arranger {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["ws.open", "ws.out", "ws.close"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::Windows,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        cx.subscribe(&plugins::WorkspaceEvent::NAMES)
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        use plugins::host::{Direction, Side};

        match name {
            "ws.open" => {
                let window = cx.focused_window().unwrap();
                let pane = cx.focused_pane().unwrap();
                let (tab, tab_pane) = cx.new_tab(window, None)?;
                let split = cx.split(pane, Direction::Horizontal, Side::Second)?;
                *self.0.borrow_mut() = Some((tab, tab_pane, split));
                Ok(())
            }
            "ws.out" => cx.pane_to_new_window(self.0.borrow().unwrap().2),
            _ => cx.close_pane(self.0.borrow().unwrap().2, false),
        }
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        // The workspace's shape, of no tab's label.
        let names: Vec<&str> = events
            .iter()
            .map(plugins::Event::name)
            .filter(|&name| name != "tab-changed")
            .collect();
        if !names.is_empty() {
            self.1.borrow_mut().push(names.join(" "));
        }
        Ok(())
    }
}

#[test]
fn a_plugin_opens_a_tab_and_a_split_moves_the_pane_out_and_closes_it() {
    let (ids, log) = (Arranged::default(), std::rc::Rc::default());
    let mut fake = Fake::default();
    let arranger = Arranger(std::rc::Rc::clone(&ids), std::rc::Rc::clone(&log));
    let mut app = app(Config::default(), shell("read line")).with_plugin("ws", Box::new(arranger));
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    fake.send(&mut app, first, WindowEvent::Focused(true));
    let shown = app.windows[&first].window;
    log.borrow_mut().clear();
    // A tab and a split, of the ids that the call got, whose programs
    // started as a window's.
    fake.send_app(&mut app, AppEvent::Command("ws.open".to_owned()));
    let (tab, tab_pane, split) = ids.borrow().unwrap();
    let (tab_pane, split) = (PaneId::of(tab_pane.0), PaneId::of(split.0));
    let tab = app.workspace.tab(workspace::TabId::of(tab.0)).unwrap();
    assert_eq!(tab.window(), shown);
    assert_eq!(tab.layout().panes(), [tab_pane]);
    assert_eq!(app.workspace.panes_of(shown).len(), 3);
    assert!(app.pane_wakes.contains_key(&tab_pane) && app.pane_wakes.contains_key(&split));
    assert_eq!(fake.windows().count(), 1);
    assert_eq!(
        *log.borrow(),
        ["tab-created pane-created pane-created layout-changed"]
    );
    // Out to a window of its own, which the platform shows.
    fake.send_app(&mut app, AppEvent::Command("ws.out".to_owned()));
    assert_eq!(fake.windows().count(), 2);
    let out = fake.windows().find(|&window| window != first).unwrap();
    assert_eq!(
        app.windows[&out].window,
        app.workspace
            .tab(app.workspace.tab_of(split).unwrap())
            .unwrap()
            .window()
    );
    assert_eq!(app.workspace.panes_of(app.windows[&out].window), [split]);
    assert_eq!(
        log.borrow()[1],
        "layout-changed window-opened tab-created pane-moved"
    );
    // Closed, its window with it, and the app forgets it.
    fake.send_app(&mut app, AppEvent::Command("ws.close".to_owned()));
    assert_eq!(fake.windows().collect::<Vec<_>>(), [first]);
    assert!(app.workspace.pane(split).is_none());
    assert!(!app.pane_wakes.contains_key(&split));
    assert_eq!(log.borrow()[2], "pane-closed tab-closed window-closed");
    assert!(!fake.has_quit());
}

/// A plugin of a quick window of its own, at the bottom, which its
/// global key toggles, and the windows' focus that it hears.
#[derive(Default)]
struct Slider {
    window: Option<lokyr_kernel::plugins::host::WindowId>,
    heard: std::rc::Rc<std::cell::RefCell<Vec<(lokyr_kernel::plugins::host::WindowId, bool)>>>,
}

impl Slider {
    const PLACE: lokyr_kernel::plugins::host::QuickPlace =
        lokyr_kernel::plugins::host::QuickPlace {
            edge: lokyr_kernel::plugins::host::QuickEdge::Bottom,
            primary: lokyr_kernel::plugins::host::QuickLength::Points(300.0),
            secondary: None,
            screen: lokyr_kernel::plugins::host::QuickScreen::Main,
            animation: 0,
            every_space: false,
        };
}

impl plugins::Plugin for Slider {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["slider.toggle", "slider.hide"] {
            cx.register_command(lokyr_kernel::commands::Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: lokyr_kernel::commands::Category::Windows,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        cx.bind(lokyr_kernel::plugins::host::Binding {
            keys: "global:ctrl+f1".to_owned(),
            command: "slider.toggle".to_owned(),
            args: Args::new(),
            when: None,
        })?;
        cx.subscribe(&[lokyr_kernel::plugins::WINDOW_FOCUS])
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        match (name, self.window) {
            ("slider.toggle", None) => {
                self.window = Some(cx.open_quick_window(Self::PLACE, Some("/"))?);
                Ok(())
            }
            ("slider.toggle", Some(window)) => cx.show_quick_window(window, Some(Self::PLACE)),
            ("slider.hide", Some(window)) => cx.show_quick_window(window, None),
            _ => Ok(()),
        }
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for event in events {
            if let plugins::Event::WindowFocus { window, focused } = event {
                self.heard.borrow_mut().push((*window, *focused));
            }
        }
        Ok(())
    }
}

#[test]
fn a_plugin_s_quick_window_shows_and_hides_and_its_global_key_is_taken() {
    let slider = Slider::default();
    let heard = std::rc::Rc::clone(&slider.heard);
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet()).with_plugin("slider", Box::new(slider));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let f1 = GlobalKey {
        code: KeyCode::F(1),
        command: false,
        control: true,
        alt: false,
        shift: false,
    };
    assert_eq!(fake.global_keys(), [f1]);
    // Its key opens it, at its place, in the folder that it asked for.
    let key = app
        .global_keys
        .iter()
        .position(|(_, invocation)| invocation.command == "slider.toggle")
        .unwrap();
    fake.send_app(&mut app, AppEvent::GlobalKey(key));
    let quick = quick_window(&fake).unwrap();
    let place = fake.window(quick).unwrap().quick.unwrap().unwrap();
    assert_eq!(
        (place.edge, place.primary),
        (
            lokyr_platform_core::QuickEdge::Bottom,
            lokyr_platform_core::Length::Points(300.0)
        )
    );
    let shown = app.windows[&quick].window;
    let state = |app: &App| {
        let found = app.workspace.window(shown).unwrap();
        (found.quick(), found.hidden())
    };
    assert_eq!(state(&app), (true, false));
    assert!(
        !app.workspace
            .window(app.windows[&window].window)
            .unwrap()
            .quick()
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(quick).working_directory().as_deref() == Some(std::path::Path::new("/"))
    });
    // It hears the focus, but of a question on the window.
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    fake.send(&mut app, quick, WindowEvent::Focused(true));
    app.ask_title(&mut fake, quick);
    fake.send(&mut app, quick, WindowEvent::Focused(false));
    assert_eq!(
        *heard.borrow(),
        [
            (shown.into(), true),
            (shown.into(), false),
            (shown.into(), true)
        ]
    );
    // Hidden, and shown again by a notification's click at its place.
    app.run(&mut fake, "slider.hide", Args::new());
    assert_eq!(fake.window(quick).unwrap().quick, Some(None));
    assert_eq!(state(&app), (true, true));
    fake.send_app(&mut app, AppEvent::NotificationClicked(quick));
    let place = fake.window(quick).unwrap().quick.unwrap().unwrap();
    assert_eq!(place.edge, lokyr_platform_core::QuickEdge::Bottom);
    assert_eq!(state(&app), (true, false));
}

/// A plugin that tries to hide every quick window, another plugin's too,
/// and keeps the codes of its refusals.
#[derive(Default)]
struct Thief {
    refused: std::rc::Rc<std::cell::RefCell<Vec<lokyr_kernel::commands::Code>>>,
}

impl plugins::Plugin for Thief {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(lokyr_kernel::commands::Command {
            name: "thief.hide".to_owned(),
            title: "thief.hide".to_owned(),
            category: lokyr_kernel::commands::Category::Windows,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        for window in cx.windows()? {
            if window.quick
                && let Err(error) = cx.show_quick_window(window.id, None)
            {
                self.refused.borrow_mut().push(error.code);
            }
        }
        Ok(())
    }
}

#[test]
fn a_plugin_cannot_show_or_hide_another_plugin_s_quick_window() {
    let thief = Thief::default();
    let refused = std::rc::Rc::clone(&thief.refused);
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet())
        .with_plugin("slider", Box::new(Slider::default()))
        .with_plugin("thief", Box::new(thief));
    fake.start(&mut app);
    app.run(&mut fake, "slider.toggle", Args::new());
    let quick = quick_window(&fake).unwrap();
    app.run(&mut fake, "thief.hide", Args::new());
    assert_eq!(*refused.borrow(), [lokyr_kernel::commands::Code::NotFound]);
    assert!(fake.window(quick).unwrap().quick.unwrap().is_some());
    // Its own plugin hides it.
    app.run(&mut fake, "slider.hide", Args::new());
    assert_eq!(fake.window(quick).unwrap().quick, Some(None));
}

#[test]
fn command_up_and_down_jump_to_the_prompts_that_the_shell_marks() {
    // Three commands of the shell's marks, of 30 lines each, then the
    // terminal raw for the keys; the last command's prompt on the screen.
    let program = shell(concat!(
        "for n in 1 2 3; do printf '\\033]133;A\\007$ \\033]133;B\\007run %s\\r\\n\\033]133;C\\007' $n; ",
        "seq 1 30; printf '\\033]133;D;0\\007'; done; stty raw -echo; echo ready; cat -v"
    ));
    let (mut fake, mut app, window) = bound("", program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let first_row = |app: &App| screen(app, window)[0].clone();
    let key = |code| key_event(code, "", CMD, true);
    // Up past the prompt that the view shows, to the one above its top, at
    // its top, then the first.
    fake.send(&mut app, window, key(KeyCode::ArrowUp));
    run_until(&mut fake, &mut app, |_, app| {
        first_row(app).starts_with("$ run 2")
    });
    fake.send(&mut app, window, key(KeyCode::ArrowUp));
    run_until(&mut fake, &mut app, |_, app| {
        first_row(app).starts_with("$ run 1")
    });
    // Down again, then to the screen, past the scrollback's last prompt.
    fake.send(&mut app, window, key(KeyCode::ArrowDown));
    run_until(&mut fake, &mut app, |_, app| {
        first_row(app).starts_with("$ run 2")
    });
    fake.send(&mut app, window, key(KeyCode::ArrowDown));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).snapshot().offset() == 0
    });
    // The marks, as the terminal keeps them, of their statuses.
    let pane = app.pane(window);
    let commands = pane
        .terminal()
        .map(|terminal| terminal.commands())
        .unwrap_or_default();
    let statuses: Vec<Option<i32>> = commands.iter().map(|command| command.status).collect();
    assert_eq!(statuses, [Some(0), Some(0), Some(0)]);
}

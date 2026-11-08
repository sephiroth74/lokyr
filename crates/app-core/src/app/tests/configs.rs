//! The plugins' sections of the config file in the app: a plugin takes its
//! section as the app starts and as the file reloads, and the notice tells
//! a section refused at its line, or what it ignored, while the rest of the
//! file applies.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::{Cx, Data, Plugin};

use super::*;

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/keyed\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nconfig-schema = \"schema.json\"\n",
);

const SCHEMA: &str = r#"{ "type": "object", "properties": { "count": { "type": "integer", "minimum": 1, "default": 1 } } }"#;

/// A plugin of a key, `count`, whose config it reads as it activates and
/// as it changes, into `heard`.
struct Keyed {
    heard: Rc<RefCell<Vec<Data>>>,
}

impl Plugin for Keyed {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.heard.borrow_mut().push(cx.config()?);
        Ok(())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[plugins::Event]) -> Result<(), Error> {
        if events.contains(&plugins::Event::ConfigChanged) {
            self.heard.borrow_mut().push(cx.config()?);
        }
        Ok(())
    }

    fn config_schema(&self) -> Option<&'static str> {
        Some(SCHEMA)
    }
}

/// `keyed`'s config of `count`.
fn count(count: i64) -> Data {
    Data::Map([("count".to_owned(), Data::Integer(count))].into())
}

#[test]
fn a_plugin_s_section_applies_as_the_file_reloads_and_a_wrong_one_tells_its_line() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-plugin-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[plugins.keyed]\ncount = 2\n").unwrap();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut fake = Fake::default();
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_builtin(
            MANIFEST,
            Box::new(Keyed {
                heard: Rc::clone(&heard),
            }),
        );
    fake.start(&mut app);
    assert_eq!(*heard.borrow(), [count(2)]);
    assert_eq!(app.diagnostics.shown(), None);
    // A wrong value refuses its section alone, as the notice tells at its
    // line; the rest of the file applies.
    std::fs::write(
        &path,
        "[cursor]\nshape = \"bar\"\n\n[plugins.keyed]\ncount = 0\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Bar);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(shown.about, About::PluginConfig("keyed".to_owned()));
    assert!(
        shown
            .text
            .contains("config.toml:5:1: `count` is at least 1, not 0")
            && shown.text.ends_with("keeps its config in force"),
        "{}",
        shown.text
    );
    assert_eq!(*heard.borrow(), [count(2)]);
    // A section that it takes takes the notice away, and the plugin hears
    // of its change.
    std::fs::write(
        &path,
        "[cursor]\nshape = \"bar\"\n\n[plugins.keyed]\ncount = 5\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    assert_eq!(*heard.borrow(), [count(2), count(5)]);
    // A key of another version, and a plugin that is not there, are
    // ignored, as the notices tell.
    std::fs::write(
        &path,
        "[plugins.keyed]\ncount = 5\nnewer = true\n\n[plugins.absent]\nx = 1\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.config.cursor.shape, cursor::Shape::Block);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(shown.about, About::PluginConfig("absent".to_owned()));
    assert!(
        app.diagnostics
            .tells(&About::PluginConfig("keyed".to_owned()))
    );
    assert_eq!(heard.borrow().len(), 2);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_refused_section_s_notice_follows_its_line_and_a_whole_number_is_an_integer() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-plugin-lines-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[plugins.keyed]\ncount = 0\n").unwrap();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut fake = Fake::default();
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_builtin(
            MANIFEST,
            Box::new(Keyed {
                heard: Rc::clone(&heard),
            }),
        );
    fake.start(&mut app);
    let text = |app: &App| app.diagnostics.shown().map(|shown| shown.text.clone());
    assert!(text(&app).is_some_and(|text| text.contains("config.toml:2:1:")));
    // Lines above it, of the same values, move its notice with it.
    std::fs::write(&path, "# mine\n\n[plugins.keyed]\ncount = 0\n").unwrap();
    change(&mut fake, &mut app);
    assert!(
        text(&app).is_some_and(|text| text.contains("config.toml:4:1:")),
        "{:?}",
        text(&app)
    );
    // A whole number of a fraction is the integer that the schema asks.
    std::fs::write(&path, "[plugins.keyed]\ncount = 5.0\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(text(&app), None);
    assert_eq!(heard.borrow().last(), Some(&count(5)));
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_command_header_s_section_is_checked_against_its_schema() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-header-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(
        &path,
        "[plugins.command-header]\nborder = \"accent\"\nalign = \"middle\"\n",
    )
    .unwrap();
    let mut fake = Fake::default();
    let mut app =
        App::new(None, Config::read(&path).unwrap(), Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(
        shown.about,
        About::PluginConfig("command-header".to_owned())
    );
    assert!(shown.text.contains("middle"), "{}", shown.text);
    // Of its values, it takes the section.
    std::fs::write(
        &path,
        "[plugins.command-header]\nborder = \"accent\"\nfont-weight = \"bold\"\npadding-x = 10\n",
    )
    .unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    let _ = std::fs::remove_dir_all(&directory);
}

/// The `Barred` plugins that the test made.
static BARRED_MADE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A plugin of a status bar of a text, and of a command, `barred.go`.
struct Barred;

impl Plugin for Barred {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(lokyr_kernel::commands::Command {
            name: "barred.go".to_owned(),
            title: "Go".to_owned(),
            category: lokyr_kernel::commands::Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }

    fn render(
        &mut self,
        _: &mut Cx<'_>,
        _: &lokyr_kernel::plugins::ui::ViewId,
    ) -> Option<lokyr_kernel::plugins::view::Node> {
        let mut text = lokyr_kernel::plugins::view::Node::text("barred");
        text.layout.width = lokyr_kernel::plugins::view::Length::Points(60.0);
        text.layout.height = lokyr_kernel::plugins::view::Length::Points(20.0);
        Some(lokyr_kernel::plugins::view::Node::container(vec![text]))
    }
}

/// Tells the app that its file changed, as its watch does, and fires the
/// reading's timer alone.
fn reload(fake: &mut Fake, app: &mut App) {
    let file = app.config_file.as_ref().unwrap();
    file.changed.store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
    let reading = app
        .config_file
        .as_ref()
        .unwrap()
        .reading
        .expect("a reading");
    let at = fake
        .timers()
        .find(|(timer, _)| *timer == reading)
        .map(|(_, at)| at)
        .expect("the reading's timer");
    fake.fire_timers(app, at);
    assert_eq!(app.config_file.as_ref().unwrap().reading, None);
}

fn make_barred() -> Box<dyn Plugin + Send> {
    BARRED_MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Box::new(Barred)
}

#[test]
fn a_plugin_turned_off_by_its_section_leaves_the_windows_and_comes_back_anew() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-plugin-off-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "").unwrap();
    let mut fake = Fake::default();
    let drawing = Drawing::new(lokyr_text_core::FontConfig::default()).unwrap();
    let mut app = App::new(
        Some(drawing),
        Config::read(&path).unwrap(),
        Some(path.clone()),
    )
    .with_program(quiet())
    .with_registration(plugins::Registration {
        manifest: concat!(
            "[plugin]\nname = \"lokyr/barred\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
            "[contributes]\nslots = [\"status-bar\"]\n",
        ),
        make: make_barred,
    });
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(crate::fakes::Surface::default());
    let status_bar = |app: &App| {
        app.windows[&window]
            .chrome
            .iter()
            .any(|view| view.view.slot() == Some(lokyr_kernel::plugins::ui::Slot::StatusBar))
    };
    let frames = |fake: &mut Fake, app: &mut App| {
        for _ in 0..3 {
            fake.send(app, window, WindowEvent::Frame);
        }
    };
    frames(&mut fake, &mut app);
    assert!(status_bar(&app));
    assert!(app.commands.get("barred.go").is_some());
    let made = BARRED_MADE.load(Ordering::Relaxed);
    // Turned off: its bar and its command go.
    std::fs::write(&path, "[plugins.barred]\nenabled = false\n").unwrap();
    reload(&mut fake, &mut app);
    assert!(fake.window(window).unwrap().frame_requested);
    frames(&mut fake, &mut app);
    assert!(!status_bar(&app));
    assert!(app.commands.get("barred.go").is_none());
    assert_eq!(app.diagnostics.shown(), None);
    // Turned on again: a new one, its bar back.
    std::fs::write(&path, "[plugins.barred]\nenabled = true\n").unwrap();
    reload(&mut fake, &mut app);
    frames(&mut fake, &mut app);
    assert!(status_bar(&app));
    assert!(app.commands.get("barred.go").is_some());
    assert_eq!(BARRED_MADE.load(Ordering::Relaxed), made + 1);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_tab_bar_turned_off_and_on_again_takes_its_place_afresh() {
    use lokyr_kernel::plugins::host::BarPlace;
    use lokyr_kernel::workspace::{Environment, Grid, Pane, Settings};
    let directory = std::env::temp_dir().join(format!("lokyr-app-tabs-off-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "").unwrap();
    let mut fake = Fake::default();
    let drawing = Drawing::new(lokyr_text_core::FontConfig::default()).unwrap();
    let mut app = App::new(
        Some(drawing),
        Config::read(&path).unwrap(),
        Some(path.clone()),
    )
    .with_program(quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(crate::fakes::Surface::default());
    let shown = app.windows[&window].window;
    let place = |app: &App| app.plugins.window_tab_bar_place("lokyr/tabs", shown.into());
    let frames = |fake: &mut Fake, app: &mut App| {
        for _ in 0..3 {
            fake.send(app, window, WindowEvent::Frame);
        }
    };
    // A single tab, of its title in the titlebar.
    run_until(&mut fake, &mut app, |_, app| place(app) == BarPlace::Title);
    // Turned off, the window takes a second tab.
    std::fs::write(&path, "[plugins.tabs]\nenabled = false\n").unwrap();
    reload(&mut fake, &mut app);
    frames(&mut fake, &mut app);
    let second = Pane::spawn(
        &lokyr_pty::Program::command("/bin/sh", ["-c", "read line"]),
        &Environment::default(),
        &Settings::default(),
        Grid {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        },
        &lokyr_vt_core::terminal::Colors::default(),
        &lokyr_kernel::l10n::Texts::new("en", &lokyr_kernel::l10n::KERNEL),
        || {},
    );
    assert!(app.workspace.open_tab(shown, second, 1).is_some());
    fake.send_app(&mut app, AppEvent::Wake);
    frames(&mut fake, &mut app);
    // On again: a bar of two tabs, at the titlebar, not the title of the
    // single tab that it left.
    std::fs::write(&path, "[plugins.tabs]\nenabled = true\n").unwrap();
    reload(&mut fake, &mut app);
    run_until(&mut fake, &mut app, |_, app| {
        place(app) == BarPlace::Titlebar
            && app.windows[&window]
                .chrome
                .iter()
                .any(|view| view.view.slot() == Some(lokyr_kernel::plugins::ui::Slot::TabBar))
    });
    assert_eq!(app.workspace.window(shown).unwrap().tabs().len(), 2);
    let _ = std::fs::remove_dir_all(&directory);
}

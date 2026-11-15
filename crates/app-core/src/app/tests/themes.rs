//! The themes of `[theme]`: of the themes' folder, applied as the app
//! starts, as their files change, and their errors told.

use std::path::Path;

use lokyr_vt_core::page::Rgb;
use lokyr_vt_core::terminal::ColorScheme;

use super::*;

/// The 16 colors of a theme's file.
const PALETTE: &str = r##"palette = ["#000000", "#110000", "#001100", "#111100", "#000011", "#110011", "#001111", "#111111",
           "#222222", "#330000", "#003300", "#333300", "#000033", "#330033", "#003333", "#ffffff"]"##;

/// A theme's file of `background`, dark.
fn theme(background: &str) -> String {
    format!("[terminal]\nbackground = \"{background}\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n")
}

/// An app of a config file of `config`, started, beside a themes' folder
/// of `files`, each a name and its text, which `test` names.
fn with_themes(test: &str, config: &str, files: &[(&str, &str)]) -> (Fake, App, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-themes-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("themes")).unwrap();
    for (name, text) in files {
        std::fs::write(directory.join("themes").join(format!("{name}.toml")), text).unwrap();
    }
    let path = directory.join("config.toml");
    std::fs::write(&path, config).unwrap();
    let config = Config::read(&path).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(None, config, Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    (fake, app, directory)
}

/// Tells the app that a theme's file changed, as its watch does, and fires
/// the loading's timer.
fn changed(fake: &mut Fake, app: &mut App) {
    app.theme_files.changed.store(true, Ordering::Release);
    fake.send_app(app, AppEvent::Wake);
    let reading = app.theme_files.reading.expect("a loading");
    let at = fake
        .timers()
        .find(|(timer, _)| *timer == reading)
        .map(|(_, at)| at)
        .expect("the loading's timer");
    fake.fire_timers(app, at);
    assert_eq!(app.theme_files.reading, None);
}

/// The background of `window`'s pane, as its terminal has it.
fn pane_background(app: &App, window: WindowId) -> Rgb {
    let reader = app.pane(window).reader().unwrap();
    reader.lock().colors().background
}

fn rgb(hex: u32) -> Rgb {
    let [_, r, g, b] = hex.to_be_bytes();
    Rgb { r, g, b }
}

fn write(directory: &Path, name: &str, text: &str) {
    std::fs::write(directory.join("themes").join(format!("{name}.toml")), text).unwrap();
}

#[test]
fn a_theme_of_the_folder_colors_the_panes_and_its_saved_file_applies_at_once() {
    let config = "[theme]\nlight = \"Nightfall\"\ndark = \"Nightfall\"\n";
    let (mut fake, mut app, directory) =
        with_themes("folder", config, &[("Nightfall", &theme("#101012"))]);
    let window = fake.windows().next().unwrap();
    assert_eq!(app.theme.dark.colors.background, rgb(0x101012));
    assert_eq!(app.theme.light.colors.background, rgb(0x101012));
    assert_eq!(pane_background(&app, window), rgb(0x101012));
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    // A preview of it shown, as the Themes page's.
    app.previews.insert(
        "Nightfall".to_owned(),
        Some(lokyr_ui_core::paint::PreviewColors::of(
            &ui_theme::dark_spec().terminal,
        )),
    );
    // Saved anew, of another background: the pane takes it, and its
    // preview.
    write(&directory, "Nightfall", &theme("#202024"));
    changed(&mut fake, &mut app);
    assert_eq!(pane_background(&app, window), rgb(0x202024));
    // Of another accent alone, the chrome's tokens take it.
    app.windows.get_mut(&window).unwrap().chrome_tokens = Some((
        (
            true,
            lokyr_render_core::Color::BLACK,
            lokyr_render_core::Color::WHITE,
            1.0,
        ),
        app.theme.dark.tokens.clone(),
    ));
    write(
        &directory,
        "Nightfall",
        &format!(
            "{}[interface]\n\"accent\" = \"#ff79c6\"\n",
            theme("#202024")
        ),
    );
    changed(&mut fake, &mut app);
    let kept = app.windows[&window].chrome_tokens.as_ref();
    assert!(
        kept.is_none_or(|(_, tokens)| tokens.get("accent")
            == Some(lokyr_render_core::Color::from_rgb8(0xff, 0x79, 0xc6))),
        "{kept:?}"
    );
    let preview = app.previews["Nightfall"].as_ref().unwrap();
    assert_eq!(
        preview.background,
        lokyr_render_core::Color::from_rgb8(0x20, 0x20, 0x24)
    );
    // The built-in themes again, as `[theme]` names them.
    app.apply(&mut fake, Config::default());
    assert_eq!(app.theme, Theme::builtin());
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_theme_s_error_tells_its_line_and_the_themes_in_force_stay_until_its_fix() {
    let config = "[theme]\ndark = \"Broken\"\nlight = \"Broken\"\n";
    let wrong = "[terminal]\nbackground = \"#10101\"\n";
    let (mut fake, mut app, directory) = with_themes("broken", config, &[("Broken", wrong)]);
    let shown = app
        .diagnostics
        .shown_diagnostic()
        .cloned()
        .expect("a notice");
    assert_eq!(shown.about, About::Theme);
    assert!(shown.text.contains("Broken.toml:2:14:"), "{}", shown.text);
    assert!(
        shown.text.ends_with("the themes in force stay"),
        "{}",
        shown.text
    );
    assert_eq!(app.theme, Theme::builtin());
    // Fixed, it applies, and its notice goes.
    write(&directory, "Broken", &theme("#303030"));
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x303030));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn what_a_theme_colors_nothing_with_tells_its_line_and_the_rest_applies() {
    let config = "[theme]\ndark = \"Told\"\nlight = \"Told\"\n";
    let told = format!(
        "{}\n[interface]\n\"accent\" = \"#c678dd\"\n\"focus.ring\" = \"nothing\"\n\"separator\" = \"mix(text\"\n",
        theme("#101012")
    );
    let (mut fake, mut app, directory) = with_themes("told", config, &[("Told", &told)]);
    let shown = app
        .diagnostics
        .shown_diagnostic()
        .cloned()
        .expect("a notice");
    assert_eq!(shown.about, About::Theme);
    assert!(shown.text.contains("Told.toml:"), "{}", shown.text);
    assert!(
        shown.text.contains("and 1 more in the log"),
        "{}",
        shown.text
    );
    assert!(
        shown.text.ends_with("the rest of the theme is in force"),
        "{}",
        shown.text
    );
    // The theme applies, its accent too, which the focus ring takes of
    // its rule.
    assert_eq!(app.theme.dark.colors.background, rgb(0x101012));
    let tokens = &app.theme.dark.tokens;
    let accent = tokens.get("accent").unwrap();
    assert_eq!(
        accent,
        lokyr_render_core::Color::from_rgb8(0xc6, 0x78, 0xdd)
    );
    assert_eq!(tokens.get("focus.ring"), Some(accent.with_alpha(0.6)));
    // Fixed, its notice goes.
    write(&directory, "Told", &theme("#101012"));
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_name_of_no_theme_tells_itself_and_its_file_applies_as_it_comes() {
    let config = "[theme]\ndark = \"Later\"\n";
    let (mut fake, mut app, directory) = with_themes("later", config, &[]);
    let shown = app
        .diagnostics
        .shown_diagnostic()
        .cloned()
        .expect("a notice");
    assert!(
        shown.text.contains("there is no theme `Later`"),
        "{}",
        shown.text
    );
    // Its file is watched, there or not.
    let file = directory.join("themes").join("Later.toml");
    assert!(
        app.theme_files.paths.contains(&file),
        "{:?}",
        app.theme_files.paths
    );
    write(&directory, "Later", &theme("#404040"));
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x404040));
    // The light one stays the built-in one.
    assert_eq!(app.theme.light, Theme::builtin().light);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_theme_that_the_app_holds_applies_by_its_name_after_the_folder_s() {
    let config = "[theme]\ndark = \"Held Night\"\n";
    let (mut fake, mut app, directory) = with_themes("held", config, &[]);
    assert!(
        app.diagnostics.shown_diagnostic().is_some(),
        "none held yet"
    );
    let held = directory.join("held");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("Held Night.toml"), theme("#123456")).unwrap();
    app.held_themes = Some(held.clone());
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x123456));
    // The held folder is not watched; the themes' folder's file of its
    // name is, which takes its place.
    assert!(
        app.theme_files
            .paths
            .iter()
            .all(|path| !path.starts_with(&held))
    );
    write(&directory, "Held Night", &theme("#654321"));
    changed(&mut fake, &mut app);
    assert_eq!(app.theme.dark.colors.background, rgb(0x654321));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_theme_s_file_saved_reloads_through_its_watch() {
    let config = "[theme]\ndark = \"Watched\"\nlight = \"Watched\"\n";
    let (mut fake, mut app, directory) =
        with_themes("watched", config, &[("Watched", &theme("#101010"))]);
    write(&directory, "Watched", &theme("#505050"));
    run_until(&mut fake, &mut app, |_, app| {
        app.theme.dark.colors.background == rgb(0x505050)
    });
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn the_built_in_themes_names_are_the_kernel_s_and_ui_core_s() {
    assert_eq!(lokyr_kernel::themes::LIGHT, ui_theme::LIGHT_NAME);
    assert_eq!(lokyr_kernel::themes::DARK, ui_theme::DARK_NAME);
    assert_eq!(ui_theme::light_spec().name, lokyr_kernel::themes::LIGHT);
}

/// An `.itermcolors` file of grays, dark: its background `#333333`.
fn grays() -> String {
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
    body.push_str(&format!("<key>Background Color</key>{}", entry(0.2)));
    body.push_str(&format!("<key>Foreground Color</key>{}", entry(0.9)));
    format!("<plist version=\"1.0\"><dict>{body}</dict></plist>")
}

#[test]
fn an_itermcolors_file_of_the_folder_applies_as_it_is() {
    let config = "[theme]\ndark = \"Grays\"\nlight = \"Grays\"\n";
    let (mut fake, mut app, directory) = with_themes("itermcolors", config, &[]);
    assert!(
        app.diagnostics.shown_diagnostic().is_some(),
        "no theme `Grays` yet"
    );
    let file = directory.join("themes").join("Grays.itermcolors");
    assert!(app.theme_files.paths.contains(&file));
    std::fs::write(&file, grays()).unwrap();
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x333333));
    // Never converted: the folder holds the file as it was.
    let names: Vec<_> = std::fs::read_dir(directory.join("themes"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["Grays.itermcolors"]);
    let _ = std::fs::remove_dir_all(directory);
}

/// A theme's file `name` of `text`, outside the themes' folder of the
/// app's `directory`, as a download is.
fn downloaded(directory: &Path, name: &str, text: &str) -> PathBuf {
    let downloads = directory.join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    let path = downloads.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

/// Opens `paths` as the system opens them with the app, and gives the
/// question that the last asks.
fn open(
    fake: &mut Fake,
    app: &mut App,
    paths: Vec<PathBuf>,
) -> lokyr_platform_core::fake::FakeAlert {
    fake.send_app(app, AppEvent::OpenFiles(paths));
    fake.alerts().last().cloned().expect("a question")
}

/// Answers `alert`'s question with its `button`, its checkbox `ticked`.
fn answer_ticked(fake: &mut Fake, app: &mut App, alert: AlertId, button: usize, ticked: bool) {
    fake.send_app(
        app,
        AppEvent::AlertClosed {
            alert,
            button,
            suppressed: ticked,
            text: None,
        },
    );
}

/// Answers `alert`'s question with its `button`.
fn answer(fake: &mut Fake, app: &mut App, alert: AlertId, button: usize) {
    answer_ticked(fake, app, alert, button, false);
}

#[test]
fn a_theme_s_file_opened_installs_on_its_answer_and_install_and_use_names_it() {
    let (mut fake, mut app, directory) = with_themes("install-use", "", &[]);
    let source = downloaded(&directory, "Grays.itermcolors", &grays());
    let asked = open(&mut fake, &mut app, vec![source.clone()]);
    assert!(asked.alert.message.contains("Grays"), "{:?}", asked.alert);
    assert!(asked.alert.detail.contains("themes"), "{:?}", asked.alert);
    assert_eq!(asked.alert.buttons.len(), 3);
    // Dark, in the light appearance: the question tells it, and offers the
    // dark one, which the user leaves.
    let variant = app
        .texts
        .text_with("theme-install-variant", &[("variant", Arg::Text("dark"))]);
    assert!(asked.alert.detail.ends_with(&variant), "{:?}", asked.alert);
    assert!(asked.alert.suppression.is_some());
    answer(&mut fake, &mut app, asked.id, 0);
    assert_eq!(app.config.app.appearance, AppearanceMode::System);
    let target = directory.join("themes/Grays.itermcolors");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        std::fs::read(&source).unwrap()
    );
    // Dark, of its colors, it names the dark appearance's theme, at once.
    let config = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(config.contains("[theme]\ndark = \"Grays\""), "{config}");
    assert_eq!(app.config.theme.dark, "Grays");
    assert_eq!(app.theme.dark.colors.background, rgb(0x333333));
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    assert!(app.theme_asks.is_empty());
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn install_alone_copies_the_file_and_cancel_copies_nothing() {
    let (mut fake, mut app, directory) = with_themes("install-only", "", &[]);
    let paper = downloaded(&directory, "Paper.toml", &theme("#ffffff"));
    let grays = downloaded(&directory, "Grays.itermcolors", &grays());
    let asked = open(&mut fake, &mut app, vec![paper]);
    answer(&mut fake, &mut app, asked.id, 1);
    let asked = open(&mut fake, &mut app, vec![grays]);
    answer(&mut fake, &mut app, asked.id, 2);
    let names: Vec<_> = std::fs::read_dir(directory.join("themes"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["Paper.toml"]);
    let config = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert_eq!(config, "");
    assert_eq!(app.config.theme.light, lokyr_kernel::themes::LIGHT);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_light_theme_installed_and_used_names_the_light_appearance_s_and_replaces_its_file() {
    let config = "[theme]\nlight = \"Paper\"\n";
    let (mut fake, mut app, directory) =
        with_themes("install-replace", config, &[("Paper", &theme("#eeeeee"))]);
    assert_eq!(app.theme.light.colors.background, rgb(0xeeeeee));
    let source = downloaded(&directory, "Paper.toml", &theme("#ffffff"));
    let asked = open(&mut fake, &mut app, vec![source]);
    // A theme of its name there already, which it replaces, as the question
    // tells.
    let replace = app.texts.text("theme-install-replace");
    assert!(asked.alert.detail.ends_with(&replace), "{:?}", asked.alert);
    // Light, in the light appearance: nothing to switch.
    assert_eq!(asked.alert.suppression, None);
    answer(&mut fake, &mut app, asked.id, 0);
    let config = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(config.contains("light = \"Paper\""), "{config}");
    assert!(!config.contains("dark"), "{config}");
    // The same name, of new colors, which apply.
    assert_eq!(app.theme.light.colors.background, rgb(0xffffff));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_file_of_no_theme_tells_why_and_asks_nothing() {
    let (mut fake, mut app, directory) = with_themes("install-none", "", &[]);
    let notes = downloaded(&directory, "notes.txt", "hello");
    let broken = downloaded(
        &directory,
        "Broken.itermcolors",
        "<plist><dict></dict></plist>",
    );
    for path in [notes, broken] {
        let told = open(&mut fake, &mut app, vec![path]);
        assert_eq!(told.alert.style, AlertStyle::Warning);
        assert!(told.alert.buttons.is_empty());
        assert_eq!(told.alert.message, app.texts.text("theme-install-failed"));
    }
    assert!(app.theme_asks.is_empty());
    assert_eq!(
        std::fs::read_dir(directory.join("themes")).unwrap().count(),
        0
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn the_question_s_checkbox_switches_to_the_theme_s_appearance() {
    let (mut fake, mut app, directory) = with_themes("install-switch", "", &[]);
    let window = fake.windows().next().unwrap();
    let source = downloaded(&directory, "Grays.itermcolors", &grays());
    let asked = open(&mut fake, &mut app, vec![source]);
    let switch = app
        .texts
        .text_with("theme-install-switch", &[("variant", Arg::Text("dark"))]);
    assert_eq!(asked.alert.suppression.as_deref(), Some(switch.as_str()));
    answer_ticked(&mut fake, &mut app, asked.id, 0, true);
    let config = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(config.contains("appearance = \"dark\""), "{config}");
    assert!(config.contains("dark = \"Grays\""), "{config}");
    assert_eq!(app.config.app.appearance, AppearanceMode::Dark);
    assert_eq!(fake.appearance(), Appearance::Dark);
    // The theme shows at once.
    assert_eq!(pane_background(&app, window), rgb(0x333333));
    // A theme of the appearance in force now asks nothing of it.
    let source = downloaded(&directory, "Grays.itermcolors", &grays());
    let asked = open(&mut fake, &mut app, vec![source]);
    assert_eq!(asked.alert.suppression, None);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_theme_opened_by_another_path_of_its_own_file_is_installed_whole() {
    let (mut fake, mut app, directory) = with_themes("install-self", "", &[]);
    let file = directory.join("themes/Grays.itermcolors");
    std::fs::write(&file, grays()).unwrap();
    let length = std::fs::metadata(&file).unwrap().len();
    // The config's folder reached through a link, as GNU Stow makes it.
    let linked =
        std::env::temp_dir().join(format!("lokyr-app-themes-linked-{}", std::process::id()));
    let _ = std::fs::remove_file(&linked);
    std::os::unix::fs::symlink(directory.join("themes"), &linked).unwrap();
    let asked = open(&mut fake, &mut app, vec![linked.join("Grays.itermcolors")]);
    // Its own file, which it does not replace.
    let replace = app.texts.text("theme-install-replace");
    assert!(!asked.alert.detail.contains(&replace), "{:?}", asked.alert);
    answer(&mut fake, &mut app, asked.id, 0);
    assert_eq!(std::fs::metadata(&file).unwrap().len(), length);
    assert_eq!(app.theme.dark.colors.background, rgb(0x333333));
    let _ = std::fs::remove_file(&linked);
    let _ = std::fs::remove_dir_all(directory);
}

/// Waits until a watch of the themes' files tells a change, for five
/// seconds at most.
fn watched(app: &App) {
    let start = std::time::Instant::now();
    while !app.theme_files.changed.load(Ordering::Acquire) {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "no change told"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Takes the changes that the watch told until it tells none for a while,
/// as Linux's inotify can tell a folder's making twice, the second after
/// the first is taken.
fn settled(fake: &mut Fake, app: &mut App) {
    let start = std::time::Instant::now();
    loop {
        changed(fake, app);
        std::thread::sleep(std::time::Duration::from_millis(200));
        if !app.theme_files.changed.load(Ordering::Acquire) {
            return;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "the watch tells changes without end"
        );
    }
}

#[test]
fn a_themes_folder_made_after_the_start_is_watched_for_its_files() {
    let directory = std::env::temp_dir().join(format!(
        "lokyr-app-themes-folder-made-later-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[theme]\ndark = \"Later\"\n").unwrap();
    let config = Config::read(&path).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(None, config, Some(path)).with_program(quiet());
    fake.start(&mut app);
    assert!(
        app.diagnostics.shown_diagnostic().is_some(),
        "no theme `Later` yet"
    );
    // The folder comes, then the theme's file in it.
    std::fs::create_dir(directory.join("themes")).unwrap();
    watched(&app);
    settled(&mut fake, &mut app);
    write(&directory, "Later", &theme("#404040"));
    watched(&app);
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown_diagnostic(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x404040));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_name_of_no_file_of_the_folder_is_watched_nowhere() {
    let config = "[theme]\ndark = \"/etc/x\"\nlight = \"../y\"\n";
    let (_fake, app, directory) = with_themes("names", config, &[]);
    assert!(app.diagnostics.shown_diagnostic().is_some());
    let themes = directory.join("themes");
    assert!(
        app.theme_files
            .paths
            .iter()
            .all(|path| path.starts_with(&themes)),
        "{:?}",
        app.theme_files.paths
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_picker_lists_the_themes_previews_one_and_names_the_chosen_one() {
    use lokyr_kernel::plugins::host::{ThemeFormat, ThemeOrigin};
    use lokyr_kernel::themes::ThemeVariant;
    let config = "[theme]\nlight = \"Lokyr Light\"\n";
    let files = [("Paper", theme("#ffffff")), ("Night", theme("#101010"))];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let (mut fake, mut app, directory) = with_themes("picker", config, &files);
    let held = directory.join("held");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("Held Dawn.toml"), theme("#fafafa")).unwrap();
    // A file of the folder of a held theme's name, the folder's.
    std::fs::write(held.join("Paper.toml"), theme("#eeeeee")).unwrap();
    // An `.itermcolors` file of the folder, of its format.
    std::fs::write(directory.join("themes/Grays.itermcolors"), grays()).unwrap();
    app.held_themes = Some(held);
    let listed = |app: &App, fake: &Fake| {
        pickers::theme_list(
            &app.theme_catalog,
            (&app.theme_places(), app.waker.as_ref()),
            fake.appearance(),
            &app.config.theme,
        )
    };
    // The built-in ones alone, as loading, while the read runs off the
    // main thread; then every one, as it took the read's list.
    let first = listed(&app, &fake);
    assert!(first.loading);
    assert_eq!(first.themes.len(), 2);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.theme_catalog.borrow().listed.is_none() && std::time::Instant::now() < deadline {
        app.take_themes(&mut fake);
        std::thread::sleep(Duration::from_millis(5));
    }
    let list = listed(&app, &fake);
    assert!(!list.loading);
    assert_eq!(list.appearance, ThemeVariant::Light);
    assert_eq!(list.current, "Lokyr Light");
    let names: Vec<(&str, ThemeVariant, ThemeOrigin, ThemeFormat)> = list
        .themes
        .iter()
        .map(|theme| {
            (
                theme.name.as_str(),
                theme.variant,
                theme.origin,
                theme.format,
            )
        })
        .collect();
    let lokyr = ThemeFormat::Lokyr;
    assert_eq!(
        names,
        [
            (
                "Grays",
                ThemeVariant::Dark,
                ThemeOrigin::Folder,
                ThemeFormat::Itermcolors
            ),
            ("Held Dawn", ThemeVariant::Light, ThemeOrigin::Held, lokyr),
            (
                "Lokyr Dark",
                ThemeVariant::Dark,
                ThemeOrigin::BuiltIn,
                lokyr
            ),
            (
                "Lokyr Light",
                ThemeVariant::Light,
                ThemeOrigin::BuiltIn,
                lokyr
            ),
            ("Night", ThemeVariant::Dark, ThemeOrigin::Folder, lokyr),
            ("Paper", ThemeVariant::Light, ThemeOrigin::Folder, lokyr),
        ]
    );
    // Previewed in place of the light appearance's, then the config's.
    app.preview_theme(&mut fake, Some("Paper".to_owned()));
    assert_eq!(app.theme.light.colors.background, rgb(0xffffff));
    app.preview_theme(&mut fake, None);
    assert_eq!(app.theme.light, Theme::builtin().light);
    // Chosen: named in the config, shown at once.
    app.preview_theme(&mut fake, Some("Night".to_owned()));
    app.set_theme(&mut fake, ThemeVariant::Light, "Paper");
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(written.contains("light = \"Paper\""), "{written}");
    assert_eq!(app.theme_preview, None);
    assert_eq!(app.theme.light.colors.background, rgb(0xffffff));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_family_chosen_after_a_preview_shows_and_its_reset_hides_the_styles_that_the_default_lacks() {
    use lokyr_kernel::plugins::host::FontFace;
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-family-written-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[font]\nfamily = \"Menlo\"\n").unwrap();
    let config = Config::read(&path).unwrap();
    let drawing = Drawing::new(config.font.clone()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), config, Some(path.clone())).with_program(quiet());
    fake.start(&mut app);
    let shown = |app: &App| app.drawing.as_ref().unwrap().font.family().to_owned();
    assert_eq!(shown(&app), "Menlo");
    // Mono previewed, then Menlo, the config's own, chosen: Menlo shows,
    // though the config changes not.
    app.preview_font(&mut fake, Some("Mono".to_owned()));
    assert_eq!(shown(&app), "Mono");
    app.set_font(&mut fake, "Menlo");
    assert_eq!(app.font_preview, None);
    assert_eq!(shown(&app), "Menlo");
    // A style previewed of the family shown, the previewed one: one that
    // it lacks is not taken, one that it has is.
    app.preview_font(&mut fake, Some("Mono".to_owned()));
    app.preview_font_style(&mut fake, FontFace::Bold, "Heavy".to_owned());
    assert_eq!(app.font_in_force().bold_style, None);
    app.preview_font_style(&mut fake, FontFace::Regular, "Regular".to_owned());
    assert_eq!(
        app.font_in_force()
            .style
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("Regular")
    );
    app.preview_font(&mut fake, None);
    // Reset: the default family, the config's styles that it lacks
    // commented out where they were.
    std::fs::write(&path, "[font]\nfamily = \"Menlo\"\nbold-style = \"Bold\"\n").unwrap();
    app.reload(&mut fake);
    assert_eq!(
        app.config
            .font
            .bold_style
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some("Bold")
    );
    app.set_setting(&mut fake, &["font".to_owned()], "family", None);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(!written.contains("family"), "{written}");
    assert!(written.contains("# bold-style = \"Bold\""), "{written}");
    assert_eq!(app.config.font.bold_style, None);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_picker_lists_the_monospaced_families_previews_one_and_names_the_chosen_one() {
    let (mut fake, mut app, directory) = with_themes("fonts", "[font]\nsize = 13\n", &[]);
    let list = |app: &App, configured: Option<&str>| {
        pickers::font_list::<fakes::Gpu>(&app.font_list, configured)
    };
    // The system's monospaced families, in order, the default one in force
    // of a config of none.
    let fonts = list(&app, None);
    assert_eq!(fonts.families, ["Menlo", "Mono"]);
    assert_eq!(fonts.current, "Mono");
    // Of the config's family, whatever its case; of one that the system
    // lacks, the default one.
    assert_eq!(list(&app, Some("menlo")).current, "Menlo");
    assert_eq!(list(&app, Some("Helvetica")).current, "Mono");
    // Previewed in every pane, then the config's again.
    app.preview_font(&mut fake, Some("Menlo".to_owned()));
    assert_eq!(app.font_in_force().family.unwrap().as_str(), "Menlo");
    app.preview_font(&mut fake, None);
    assert_eq!(app.font_in_force().family, None);
    // Chosen: named in the config, in place, which a restart keeps.
    app.preview_font(&mut fake, Some("Mono".to_owned()));
    app.set_font(&mut fake, "Menlo");
    assert_eq!(app.font_preview, None);
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(written.contains("family = \"Menlo\""), "{written}");
    assert!(written.contains("size = 13"), "{written}");
    assert_eq!(app.config.font.family.as_ref().unwrap().as_str(), "Menlo");
    let again = Config::read(&directory.join("config.toml")).unwrap();
    assert_eq!(again.font.family.unwrap().as_str(), "Menlo");
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_family_that_lacks_the_config_s_style_is_previewed_and_chosen_of_its_own() {
    // The families of the tests have a Regular face alone.
    let config = "[font]\nfamily = \"Mono\"\nstyle = \"Medium\"\nsize = 13\n";
    let (mut fake, mut app, directory) = with_themes("font-style", config, &[]);
    app.preview_font(&mut fake, Some("Menlo".to_owned()));
    let shown = app.font_in_force();
    assert_eq!(shown.family.unwrap().as_str(), "Menlo");
    assert_eq!(shown.style, None, "its own regular face");
    app.set_font(&mut fake, "Menlo");
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert_eq!(
        written,
        "[font]\nfamily = \"Menlo\"\n# style = \"Medium\"\nsize = 13\n"
    );
    assert_eq!(app.config.font.family.as_ref().unwrap().as_str(), "Menlo");
    assert_eq!(app.config.font.style, None);
    assert_eq!(
        app.diagnostics.shown_diagnostic(),
        None,
        "no notice of a style"
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_program_of_mode_2031_learns_of_the_appearance_s_scheme_as_it_switches() {
    // In raw mode, the reports of the color scheme on, and the report read
    // back, with ESC as E, once the program tells that it waits.
    let script = r#"stty raw -echo; printf '\033[?2031hwaiting\r\n'; r=$(dd bs=1 count=9 2>/dev/null); stty sane; printf 'got:%s\n' "$r" | tr '\033' E; exec sleep 10"#;
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell(script));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let shown = |app: &App, text: &str| (0..4).any(|line| row(app, window, line) == text);
    run_until(&mut fake, &mut app, |_, app| shown(app, "waiting"));
    // The light theme's to the dark one's.
    assert_eq!(app.colors(Appearance::Dark).scheme(), ColorScheme::Dark);
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    run_until(&mut fake, &mut app, |_, app| shown(app, "got:E[?997;1n"));
}

#[test]
fn open_themes_folder_opens_the_folder_made_first_in_the_file_manager() {
    let (mut fake, mut app, directory) = with_themes("open-folder", "", &[]);
    let themes = directory.join("themes");
    std::fs::remove_dir_all(&themes).unwrap();
    // The view's menu has it right after the palette's theme picker.
    let view = menu_items(&fake, 3);
    let picker = view
        .iter()
        .position(|item| item == "Choose Theme…")
        .unwrap();
    assert_eq!(view[picker + 3], "Open Themes Folder");
    // A folder that is not there is made, then opens.
    fake.send_app(&mut app, AppEvent::Command(OPEN_THEMES.to_owned()));
    assert!(themes.is_dir());
    let opened = std::fs::canonicalize(&themes).unwrap();
    assert_eq!(fake.opened(), [opened.display().to_string()]);
    // One that is there opens as it is, its files kept.
    write(&directory, "Kept", &theme("#101010"));
    fake.send_app(&mut app, AppEvent::Command(OPEN_THEMES.to_owned()));
    assert_eq!(fake.opened().len(), 2);
    assert!(themes.join("Kept.toml").is_file());
    assert!(fake.alerts().is_empty());
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_new_theme_of_an_existing_one_is_written_whole_named_for_its_variant_and_opened() {
    // The light appearance in force, of a dark theme of the folder.
    let night = format!(
        "[theme]\nauthor = \"Ada\"\nlicense = \"MIT\"\n{}",
        theme("#101010")
    );
    let (mut fake, mut app, directory) = with_themes(
        "new-theme",
        "[theme]\nlight = \"Night\"\n",
        &[("Night", &night)],
    );
    let held = directory.join("held");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("Held Dawn.toml"), theme("#fafafa")).unwrap();
    app.held_themes = Some(held);
    // The view's menu has it after the palette's theme picker, before the
    // themes' folder.
    let view = menu_items(&fake, 3);
    let picker = view
        .iter()
        .position(|item| item == "Choose Theme…")
        .unwrap();
    assert_eq!(
        view[picker + 1..picker + 4],
        [
            "New Theme from Current…",
            "Import Themes…",
            "Open Themes Folder"
        ]
    );
    let answered = |fake: &mut Fake, app: &mut App, text: &str| {
        let asked = fake.alerts().last().cloned().expect("a question");
        fake.send_app(
            app,
            AppEvent::AlertClosed {
                alert: asked.id,
                button: 0,
                suppressed: false,
                text: Some(text.to_owned()),
            },
        );
        fake.alerts().last().cloned().expect("an alert")
    };
    // Of the one in force: its name asked, of a first one of it.
    fake.send_app(&mut app, AppEvent::Command(NEW_THEME.to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.input.as_deref(), Some("Night Copy"));
    assert!(asked.alert.message.contains("Night"), "{:?}", asked.alert);
    // A name taken, of a held theme, or that names no file: asked again,
    // why told, the text kept.
    let again = answered(&mut fake, &mut app, "Held Dawn");
    assert_ne!(again.id, asked.id);
    assert!(
        again.alert.detail.contains("is there already"),
        "{:?}",
        again.alert
    );
    assert_eq!(again.alert.input.as_deref(), Some("Held Dawn"));
    let again = answered(&mut fake, &mut app, ".hidden");
    assert!(
        again.alert.detail.contains("cannot name a theme"),
        "{:?}",
        again.alert
    );
    // A name of its own: written whole, its credits kept, named for its
    // variant, the dark one, shown and opened.
    let alerts = fake.alerts().len();
    answered(&mut fake, &mut app, "  Midnight  ");
    assert_eq!(fake.alerts().len(), alerts, "no question more");
    let file = directory.join("themes/Midnight.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("name = \"Midnight\""), "{text}");
    assert!(text.contains("author = \"Ada\""), "{text}");
    assert!(text.contains("variant = \"dark\""), "{text}");
    assert!(text.contains("palette = ["), "{text}");
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(written.contains("dark = \"Midnight\""), "{written}");
    assert_eq!(app.theme.dark.colors.background, rgb(0x101010));
    let opened = std::fs::canonicalize(&file).unwrap();
    assert_eq!(fake.opened().last(), Some(&opened.display().to_string()));
    // Of the theme that its argument names, a held one, light.
    app.run(
        &mut fake,
        NEW_THEME,
        Args::from([("theme".to_owned(), Value::String("Held Dawn".to_owned()))]),
    );
    answered(&mut fake, &mut app, "Dawn Mine");
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(written.contains("light = \"Dawn Mine\""), "{written}");
    assert!(directory.join("themes/Dawn Mine.toml").is_file());
    // A new one of Lokyr's own dark values, of every key, its optional
    // ones as comments, used for the dark appearance; a second one of the
    // first name that no theme has.
    let fresh = Args::from([("variant".to_owned(), Value::String("dark".to_owned()))]);
    app.run(&mut fake, NEW_THEME, fresh.clone());
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.message, "New dark theme");
    assert_eq!(asked.alert.input.as_deref(), Some("New Dark Theme"));
    answered(&mut fake, &mut app, "New Dark Theme");
    let text = std::fs::read_to_string(directory.join("themes/New Dark Theme.toml")).unwrap();
    for line in [
        "name = \"New Dark Theme\"",
        "variant = \"dark\"",
        "background = \"#1e1e20\"",
        "# cursor = \"#e6e6e6\"",
        "# underline = ",
        "\"panel.bg\" = ",
        "[search]",
    ] {
        assert!(text.contains(line), "{line} in {text}");
    }
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(written.contains("dark = \"New Dark Theme\""), "{written}");
    app.run(&mut fake, NEW_THEME, fresh);
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.input.as_deref(), Some("New Dark Theme 2"));
    answered(&mut fake, &mut app, "New Dark Theme 2");
    let light = Args::from([("variant".to_owned(), Value::String("light".to_owned()))]);
    app.run(&mut fake, NEW_THEME, light);
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.message, "New light theme");
    assert_eq!(asked.alert.input.as_deref(), Some("New Light Theme"));
    answered(&mut fake, &mut app, "New Light Theme");
    let written = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(written.contains("light = \"New Light Theme\""), "{written}");
    // A copy of a name taken, of a number after it.
    write(&directory, "Held Dawn Copy", &theme("#fafafa"));
    app.run(
        &mut fake,
        NEW_THEME,
        Args::from([("theme".to_owned(), Value::String("Held Dawn".to_owned()))]),
    );
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.input.as_deref(), Some("Held Dawn Copy 2"));
    // Of a name of no theme: told, nothing asked.
    app.run(
        &mut fake,
        NEW_THEME,
        Args::from([("theme".to_owned(), Value::String("Nowhere".to_owned()))]),
    );
    let told = fake.alerts().last().cloned().unwrap();
    assert_eq!(told.alert.message, "The new theme cannot be made.");
    assert!(
        told.alert.detail.contains("there is no theme `Nowhere`"),
        "{:?}",
        told.alert
    );
    assert_eq!(told.alert.input, None);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn themes_imported_of_the_open_panel_are_copied_one_question_for_those_there() {
    let paper_text = theme("#ffffff");
    let (mut fake, mut app, directory) = with_themes("import", "", &[("Paper", &paper_text)]);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    // The open panel, on the window in front, of both formats.
    fake.send_app(&mut app, AppEvent::Command(IMPORT_THEMES.to_owned()));
    let (shown, extensions, message) = fake.open_panels().last().cloned().expect("the panel");
    assert_eq!(shown, window);
    assert_eq!(extensions, ["toml", "itermcolors"]);
    assert!(message.contains(".itermcolors"), "{message}");
    // One panel at a time.
    fake.send_app(&mut app, AppEvent::Command(IMPORT_THEMES.to_owned()));
    assert_eq!(fake.open_panels().len(), 1);
    // A new theme, one of a name there, and a file of no theme.
    let gray = downloaded(&directory, "Grays.itermcolors", &grays());
    let paper = downloaded(&directory, "Paper.toml", &theme("#eeeeee"));
    let notes = downloaded(&directory, "Notes.toml", "[terminal]\nbackground = 12\n");
    let alerts = fake.alerts().len();
    fake.choose(&mut app, vec![gray, paper.clone(), notes]);
    // Asked once of the one there, nothing copied yet.
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(fake.alerts().len(), alerts + 1);
    assert_eq!(
        asked.alert.message,
        "Replace the theme that is there already?"
    );
    assert!(
        asked.alert.detail.contains("“Paper.toml”"),
        "{:?}",
        asked.alert
    );
    assert_eq!(asked.alert.buttons, ["Replace", "Skip"]);
    assert!(!directory.join("themes/Grays.itermcolors").exists());
    // Replace: both copied as they are, the file of no theme told.
    answer(&mut fake, &mut app, asked.id, 0);
    let themes = directory.join("themes");
    assert_eq!(
        std::fs::read_to_string(themes.join("Grays.itermcolors")).unwrap(),
        grays()
    );
    assert_eq!(
        std::fs::read_to_string(themes.join("Paper.toml")).unwrap(),
        theme("#eeeeee")
    );
    assert!(!themes.join("Notes.toml").exists());
    // One alert of the themes imported, by their names, and of the file
    // not imported, of why.
    let told = fake.alerts().last().cloned().unwrap();
    assert_eq!(told.alert.message, "2 themes imported");
    assert_eq!(told.alert.style, lokyr_platform_core::AlertStyle::Warning);
    let detail = &told.alert.detail;
    assert!(
        detail.starts_with("Grays, Paper: in the themes' folder"),
        "{detail}"
    );
    assert!(
        detail.contains("\n\nNot imported:\nNotes.toml: "),
        "{detail}"
    );
    // Skip: the one there kept, the new ones copied.
    fake.send_app(&mut app, AppEvent::Command(IMPORT_THEMES.to_owned()));
    std::fs::write(&paper, theme("#dddddd")).unwrap();
    let night = downloaded(&directory, "Night.toml", &theme("#101010"));
    fake.choose(&mut app, vec![paper, night]);
    let asked = fake.alerts().last().cloned().unwrap();
    answer(&mut fake, &mut app, asked.id, 1);
    assert_eq!(
        std::fs::read_to_string(themes.join("Paper.toml")).unwrap(),
        theme("#eeeeee")
    );
    assert!(themes.join("Night.toml").is_file());
    let told = fake.alerts().last().cloned().unwrap();
    assert_eq!(told.alert.message, "One theme imported");
    assert_eq!(
        told.alert.style,
        lokyr_platform_core::AlertStyle::Informational
    );
    assert!(told.alert.detail.starts_with("Night: "), "{:?}", told.alert);
    // Every one skipped, none refused: nothing told.
    fake.send_app(&mut app, AppEvent::Command(IMPORT_THEMES.to_owned()));
    let night = directory.join("Downloads/Night.toml");
    fake.choose(&mut app, vec![night]);
    let asked = fake.alerts().last().cloned().unwrap();
    let alerts = fake.alerts().len();
    answer(&mut fake, &mut app, asked.id, 1);
    assert_eq!(fake.alerts().len(), alerts);
    // Files of no theme alone: told as not imported.
    fake.send_app(&mut app, AppEvent::Command(IMPORT_THEMES.to_owned()));
    let notes = directory.join("Downloads/Notes.toml");
    fake.choose(&mut app, vec![notes]);
    let told = fake.alerts().last().cloned().unwrap();
    assert_eq!(told.alert.message, "A file is not imported.");
    assert!(
        told.alert.detail.starts_with("Notes.toml: "),
        "{:?}",
        told.alert
    );
    // A panel cancelled imports nothing, and asks nothing.
    fake.send_app(&mut app, AppEvent::Command(IMPORT_THEMES.to_owned()));
    let alerts = fake.alerts().len();
    fake.choose(&mut app, Vec::new());
    assert_eq!(fake.alerts().len(), alerts);
    assert_eq!(fake.open_panels().len(), 5);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn open_themes_folder_tells_why_it_opens_nothing() {
    let told = |fake: &Fake| {
        fake.alerts()
            .iter()
            .map(|shown| (shown.alert.message.clone(), shown.alert.detail.clone()))
            .collect::<Vec<_>>()
    };
    let message = "Lokyr cannot open its themes' folder".to_owned();
    // A folder that cannot be made: a link to one that is gone.
    let (mut fake, mut app, directory) = with_themes("open-folder-failed", "", &[]);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let themes = directory.join("themes");
    std::fs::remove_dir_all(&themes).unwrap();
    std::os::unix::fs::symlink(directory.join("gone"), &themes).unwrap();
    fake.send_app(&mut app, AppEvent::Command(OPEN_THEMES.to_owned()));
    let shown = told(&fake);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].0, message);
    assert!(
        shown[0].1.contains(&themes.display().to_string()),
        "{shown:?}"
    );
    assert!(fake.opened().is_empty());
    // On the window that has the focus.
    assert_eq!(fake.alerts()[0].window, Some(window));
    let _ = std::fs::remove_dir_all(&directory);
    // Without a config file, there is no folder.
    let (mut fake, mut app) = started();
    fake.send_app(&mut app, AppEvent::Command(OPEN_THEMES.to_owned()));
    let shown = told(&fake);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].0, message);
    assert!(fake.opened().is_empty());
}

#[test]
fn the_themes_list_and_colors_come_off_the_main_thread_a_part_at_a_time() {
    let (mut fake, mut app, directory) = with_themes("catalog", "", &[]);
    // Thirty held themes, past a part of colors.
    let held = directory.join("held");
    std::fs::create_dir_all(&held).unwrap();
    for at in 0..30 {
        std::fs::write(held.join(format!("Held {at:02}.toml")), theme("#202020")).unwrap();
    }
    app.held_themes = Some(held);
    // A preview's need starts the read; its list, then every one's colors,
    // come as the main thread takes them, none of them here before.
    let window = fake.windows().next().unwrap();
    let asked = fake.window(window).unwrap().frames_asked;
    app.keep_previews(vec!["Held 00".to_owned(), "Held 29".to_owned()]);
    assert!(!app.previews.contains_key("Held 00"));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !app.previews.contains_key("Held 29") && std::time::Instant::now() < deadline {
        app.take_themes(&mut fake);
        std::thread::sleep(Duration::from_millis(5));
    }
    let listed = app
        .theme_catalog
        .borrow()
        .listed
        .clone()
        .unwrap_or_default();
    assert_eq!(listed.len(), 30);
    // The read ends, every one colored, of its parts that may come in
    // turns apart; its colors drew no window, as none shows a preview.
    while !app.theme_catalog.borrow().finished() && std::time::Instant::now() < deadline {
        app.take_themes(&mut fake);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.theme_catalog.borrow().finished());
    assert!((0..30).all(|at| app.previews.contains_key(&format!("Held {at:02}"))));
    assert_eq!(
        app.previews["Held 00"].unwrap().background,
        lokyr_render_core::Color::from_rgb8(0x20, 0x20, 0x20)
    );
    assert_eq!(fake.window(window).unwrap().frames_asked, asked);
    // A name that the read colored not is of no theme, and starts no read
    // again, however often its preview shows, until the list changes.
    app.keep_previews(vec!["Nothing".to_owned()]);
    assert_eq!(app.previews.get("Nothing"), Some(&None));
    assert!(app.theme_catalog.borrow().finished(), "no read again");
    app.keep_previews(vec!["Nothing".to_owned()]);
    assert!(app.theme_catalog.borrow().finished());
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn the_cursor_guide_shows_as_its_item_toggles_it_until_the_config_s_key_changes() {
    let (mut fake, mut app, directory) = with_themes("guide", "", &[]);
    let toggle = |fake: &mut Fake, app: &mut App| {
        fake.send_app(app, AppEvent::Command(CURSOR_GUIDE.to_owned()));
    };
    // Off by default, its item not ticked, in the View menu.
    assert!(!app.cursor_guide && !fake.command_checked(CURSOR_GUIDE));
    toggle(&mut fake, &mut app);
    assert!(app.cursor_guide && fake.command_checked(CURSOR_GUIDE));
    toggle(&mut fake, &mut app);
    assert!(!app.cursor_guide && !fake.command_checked(CURSOR_GUIDE));
    // The config's key turns it on, and off again as it changes; the same
    // key again keeps the item's choice.
    let mut config = Config::default();
    config.cursor.guide = true;
    app.apply(&mut fake, config.clone());
    assert!(app.cursor_guide && fake.command_checked(CURSOR_GUIDE));
    toggle(&mut fake, &mut app);
    config.text.bold = !config.text.bold;
    app.apply(&mut fake, config.clone());
    assert!(!app.cursor_guide);
    config.cursor.guide = false;
    app.apply(&mut fake, config);
    assert!(!app.cursor_guide && !fake.command_checked(CURSOR_GUIDE));
    // Its color the theme's token.
    assert!(app.theme.dark.tokens.get("cursor.guide").is_some());
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_theme_s_tokens_tell_their_colors_and_origins_of_its_file_its_base_and_rules() {
    use lokyr_kernel::plugins::host::TokenOrigin;
    use lokyr_kernel::themes::ThemeColor;
    let mine = format!(
        "[theme]\nextends = \"Lokyr Dark\"\n\n[terminal]\nbackground = \"#101012\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n\n[interface]\n\"accent\" = \"#c678dd\"\n[command-header]\npill = \"panel.bg\"\n"
    );
    let config = "[theme]\ndark = \"Mine\"\nlight = \"Mine\"\n";
    let (fake, app, directory) = with_themes("tokens", config, &[("Mine", &mine)]);
    let places = app.theme_places();
    let tokens = theme_catalog::theme_tokens(
        &places,
        None,
        (fake.appearance(), &app.config.theme),
        &BTreeMap::new(),
    )
    .expect("the theme in force's tokens");
    assert_eq!(tokens.theme, "Mine");
    let token = |name: &str| {
        tokens
            .tokens
            .iter()
            .find(|token| token.name == name)
            .unwrap()
    };
    // Of its own file, at its line.
    let accent = token("accent");
    assert_eq!(accent.color, ThemeColor::hex(0xc678dd));
    let TokenOrigin::File { path, line, theme } = &accent.origin else {
        panic!("{:?}", accent.origin);
    };
    assert!(path.ends_with("Mine.toml"), "{path}");
    assert_eq!((*line, theme.as_str()), (11, "Mine"));
    // Of the built-in theme that it extends, of no file.
    assert_eq!(
        token("panel.bg").origin,
        TokenOrigin::Theme {
            theme: "Lokyr Dark".to_owned()
        }
    );
    // Of its rule, its color the accent's at 60%.
    assert_eq!(
        token("focus.ring").origin,
        TokenOrigin::Rule {
            text: "accent / 60%".to_owned()
        }
    );
    assert_eq!(
        token("focus.ring").color,
        ThemeColor::hex(0xc678dd).with_alpha(153)
    );
    // The terminal's colors: its keys, then the palette's 16, each of its
    // file's line, of the theme that it extends, or of its default.
    let terminal = |name: &str| {
        tokens
            .terminal
            .iter()
            .find(|token| token.name == name)
            .unwrap()
    };
    assert_eq!(tokens.terminal.len(), 9 + 16);
    assert_eq!(tokens.terminal[0].name, "background");
    assert_eq!(tokens.terminal[9].name, "black");
    let line_of = |origin: &TokenOrigin| match origin {
        TokenOrigin::File { line, .. } => Some(*line),
        _ => None,
    };
    assert_eq!(terminal("background").color, ThemeColor::hex(0x101012));
    assert_eq!(line_of(&terminal("background").origin), Some(5));
    assert_eq!(line_of(&terminal("bright-white").origin), Some(8));
    assert_eq!(
        terminal("selection").origin,
        TokenOrigin::Theme {
            theme: "Lokyr Dark".to_owned()
        }
    );
    assert_eq!(
        terminal("cursor").origin,
        TokenOrigin::Rule {
            text: "foreground".to_owned()
        }
    );
    assert_eq!(terminal("cursor").color, ThemeColor::hex(0xe0e0e0));
    // Lokyr's in their rules' order, then a plugin's.
    assert_eq!(tokens.tokens[0].name, "text");
    assert_eq!(tokens.tokens.last().unwrap().name, "command-header.pill");
    assert_eq!(
        tokens.tokens.len(),
        lokyr_kernel::themes::rules::tokens().count() + 1
    );
    // A name of no theme, none; a built-in one's, of its own.
    assert!(
        theme_catalog::theme_tokens(
            &places,
            Some("Nothing"),
            (fake.appearance(), &app.config.theme),
            &BTreeMap::new(),
        )
        .is_none()
    );
    let light = theme_catalog::theme_tokens(
        &places,
        Some("Lokyr Light"),
        (fake.appearance(), &app.config.theme),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(light.variant, lokyr_kernel::themes::ThemeVariant::Light);
    let _ = std::fs::remove_dir_all(directory);
}

/// The answers that the theme editor's plugin heard, of its questions'
/// ids, and its questions' ids.
type EditorAnswers = std::rc::Rc<std::cell::RefCell<(Vec<(u64, Option<String>)>, Vec<u64>)>>;

/// A plugin that copies the held theme Held Dawn and converts the folder's
/// `.itermcolors` Grays, as the Theme Editor asks, keeping the answers that
/// it hears.
struct Editing(EditorAnswers);

impl plugins::Plugin for Editing {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in [
            "editing.copy",
            "editing.convert",
            "editing.convert-pair",
            "editing.new",
        ] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let id = match name {
            "editing.copy" => cx.copy_theme("Held Dawn")?,
            "editing.convert-pair" => cx.convert_theme("Pair (Dark)")?,
            "editing.new" => cx.new_theme(lokyr_kernel::themes::ThemeVariant::Dark)?,
            _ => cx.convert_theme("Grays")?,
        };
        self.0.borrow_mut().1.push(id);
        Ok(())
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for event in events {
            if let plugins::Event::Answered { id, text } = event {
                self.0.borrow_mut().0.push((*id, text.clone()));
            }
        }
        Ok(())
    }
}

#[test]
fn the_theme_editor_s_copy_and_conversion_answer_its_plugin_of_no_editor_opened() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-themes-editing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let themes = directory.join("themes");
    std::fs::create_dir_all(&themes).unwrap();
    std::fs::write(themes.join("Grays.itermcolors"), grays()).unwrap();
    // A file of both appearances: Pair (Light), white, and Pair (Dark).
    let pair = grays().replace(
        "</dict></plist>",
        "<key>Background Color (Light)</key><dict><key>Red Component</key><real>1</real>\
         <key>Green Component</key><real>1</real><key>Blue Component</key><real>1</real>\
         <key>Color Space</key><string>sRGB</string></dict>\
         <key>Foreground Color (Light)</key><dict><key>Red Component</key><real>0</real>\
         <key>Green Component</key><real>0</real><key>Blue Component</key><real>0</real>\
         <key>Color Space</key><string>sRGB</string></dict></dict></plist>",
    );
    std::fs::write(themes.join("Pair.itermcolors"), pair).unwrap();
    let held = directory.join("held");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("Held Dawn.toml"), theme("#fafafa")).unwrap();
    let path = directory.join("config.toml");
    // The light appearance in force, of the dark Grays.
    std::fs::write(&path, "[theme]\nlight = \"Grays\"\n").unwrap();
    let config = Config::read(&path).unwrap();
    let answers = EditorAnswers::default();
    let mut app = App::new(None, config, Some(path.clone()))
        .with_program(quiet())
        .with_plugin("editing", Box::new(Editing(std::rc::Rc::clone(&answers))));
    app.held_themes = Some(held);
    let mut fake = Fake::default();
    fake.start(&mut app);
    // The themes' list read, which the requests are checked against.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        pickers::theme_list(
            &app.theme_catalog,
            (&app.theme_places(), app.waker.as_ref()),
            fake.appearance(),
            &app.config.theme,
        );
        app.take_themes(&mut fake);
        if app.theme_catalog.borrow().listed.is_some() || std::time::Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let heard = |fake: &mut Fake, app: &mut App, count: usize| {
        run_until(fake, app, |_, _| answers.borrow().0.len() == count);
        answers.borrow().0[count - 1].clone()
    };
    // A copy: its name asked, of no editor's words; written, named for
    // its variant, its name answered, no editor opened.
    fake.send_app(&mut app, AppEvent::Command("editing.copy".to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.input.as_deref(), Some("Held Dawn Copy"));
    assert!(
        asked.alert.detail.ends_with("and uses it."),
        "{:?}",
        asked.alert
    );
    fake.answer_alert_typed(&mut app, asked.id, (0, false), Some("Dawn Mine"));
    let id = answers.borrow().1[0];
    assert_eq!(
        heard(&mut fake, &mut app, 1),
        (id, Some("Dawn Mine".to_owned()))
    );
    assert!(themes.join("Dawn Mine.toml").is_file());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("light = \"Dawn Mine\""), "{written}");
    assert!(fake.opened().is_empty(), "{:?}", fake.opened());
    // Cancelled: none.
    fake.send_app(&mut app, AppEvent::Command("editing.copy".to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    fake.answer_alert(&mut app, asked.id, 1);
    assert_eq!(heard(&mut fake, &mut app, 2).1, None);
    // A conversion cancelled: none, the folder as it was.
    fake.send_app(&mut app, AppEvent::Command("editing.convert".to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.message, "Convert “Grays” to Lokyr's format?");
    assert!(asked.alert.detail.contains("Trash"), "{:?}", asked.alert);
    assert_eq!(asked.alert.input, None);
    fake.answer_alert(&mut app, asked.id, 1);
    assert_eq!(heard(&mut fake, &mut app, 3).1, None);
    assert!(themes.join("Grays.itermcolors").is_file());
    assert!(!themes.join("Grays.toml").exists());
    // The Trash failing: told, none answered, its new file taken away.
    fake.fail_trash(Some("the disk is locked"));
    fake.send_app(&mut app, AppEvent::Command("editing.convert".to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    fake.answer_alert(&mut app, asked.id, 0);
    assert_eq!(heard(&mut fake, &mut app, 4).1, None);
    let told = fake.alerts().last().cloned().expect("an alert");
    assert_eq!(told.alert.message, "The theme cannot be converted.");
    assert!(
        told.alert.detail.contains("the disk is locked"),
        "{:?}",
        told.alert
    );
    assert!(themes.join("Grays.itermcolors").is_file());
    assert!(!themes.join("Grays.toml").exists());
    // Converted: of Lokyr's format, of its name and colors, its file in
    // the Trash, the theme in force of its new file, as its old one is
    // gone, its name answered.
    app.set_theme(
        &mut fake,
        lokyr_kernel::themes::ThemeVariant::Light,
        "Grays",
    );
    assert_eq!(app.theme.light.colors.background, rgb(0x333333));
    fake.fail_trash(None);
    fake.send_app(&mut app, AppEvent::Command("editing.convert".to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    fake.answer_alert(&mut app, asked.id, 0);
    assert_eq!(heard(&mut fake, &mut app, 5).1, Some("Grays".to_owned()));
    let text = std::fs::read_to_string(themes.join("Grays.toml")).unwrap();
    assert!(text.contains("name = \"Grays\""), "{text}");
    assert!(text.contains("background = \"#333333\""), "{text}");
    assert_eq!(fake.trashed(), [themes.join("Grays.itermcolors")]);
    assert!(!themes.join("Grays.itermcolors").exists());
    assert_eq!(app.theme.light.colors.background, rgb(0x333333));
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("light = \"Grays\""), "{written}");
    // A theme of a file of both appearances: both converted, of their
    // names, the question telling the other, the file in the Trash.
    fake.send_app(
        &mut app,
        AppEvent::Command("editing.convert-pair".to_owned()),
    );
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(
        asked.alert.message,
        "Convert “Pair (Dark)” to Lokyr's format?"
    );
    assert!(
        asked.alert.detail.contains("“Pair (Light)” too"),
        "{:?}",
        asked.alert
    );
    fake.answer_alert(&mut app, asked.id, 0);
    assert_eq!(
        heard(&mut fake, &mut app, 6).1,
        Some("Pair (Dark)".to_owned())
    );
    let light = std::fs::read_to_string(themes.join("Pair (Light).toml")).unwrap();
    assert!(light.contains("background = \"#ffffff\""), "{light}");
    assert!(light.contains("variant = \"light\""), "{light}");
    let dark = std::fs::read_to_string(themes.join("Pair (Dark).toml")).unwrap();
    assert!(dark.contains("background = \"#333333\""), "{dark}");
    assert!(!themes.join("Pair.itermcolors").exists());
    // A new theme of Lokyr's own dark values: its name asked, written of
    // every key, named for the dark appearance, its name answered, no
    // editor opened.
    fake.send_app(&mut app, AppEvent::Command("editing.new".to_owned()));
    let asked = fake.alerts().last().cloned().expect("a question");
    assert_eq!(asked.alert.input.as_deref(), Some("New Dark Theme"));
    fake.answer_alert_typed(&mut app, asked.id, (0, false), Some("New Dark Theme"));
    assert_eq!(
        heard(&mut fake, &mut app, 7).1,
        Some("New Dark Theme".to_owned())
    );
    let text = std::fs::read_to_string(themes.join("New Dark Theme.toml")).unwrap();
    assert!(text.contains("variant = \"dark\""), "{text}");
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("dark = \"New Dark Theme\""), "{written}");
    assert!(fake.opened().is_empty(), "{:?}", fake.opened());
    // A question whose window closes has no answer.
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send_app(&mut app, AppEvent::Command("editing.copy".to_owned()));
    assert_eq!(
        fake.alerts().last().map(|asked| asked.window),
        Some(Some(window))
    );
    app.close(&mut fake, window);
    assert_eq!(heard(&mut fake, &mut app, 8).1, None);
    let _ = std::fs::remove_dir_all(&directory);
}

/// A plugin that sets the colors of the theme Mine, or takes one away, as
/// its commands say, keeping each call's error, or none.
struct Values(std::rc::Rc<std::cell::RefCell<Vec<Option<String>>>>);

impl plugins::Plugin for Values {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for (name, ..) in VALUES {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let Some(&(_, color, value)) = VALUES.iter().find(|(command, ..)| *command == name) else {
            return Ok(());
        };
        let told = cx.set_theme_value("Mine", color, value).err();
        self.0.borrow_mut().push(told.map(|error| error.message));
        Ok(())
    }
}

/// The commands of [`Values`]: each a color of Mine and its value, or none.
const VALUES: [(&str, &str, Option<&str>); 6] = [
    ("values.background", "background", Some("#202020")),
    ("values.accent", "accent", Some("#00ff00")),
    ("values.unknown", "accent", Some("nowhere / 50%")),
    ("values.ring", "focus.ring", Some("field.focus")),
    ("values.cycle", "field.focus", Some("focus.ring")),
    ("values.reset", "accent", None),
];

#[test]
fn a_theme_s_color_set_of_a_plugin_is_written_into_its_file_and_shows_at_once() {
    let mine = format!(
        "[theme]\nname = \"Mine\"\n\n[terminal]\nbackground = \"#101010\"   # mine\nforeground = \"#e0e0e0\"\n{PALETTE}\n"
    );
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-themes-values-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("themes")).unwrap();
    let file = directory.join("themes/Mine.toml");
    std::fs::write(&file, &mine).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[theme]\nlight = \"Mine\"\n").unwrap();
    let told = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_plugin("values", Box::new(Values(std::rc::Rc::clone(&told))));
    let mut fake = Fake::default();
    fake.start(&mut app);
    // The themes' list read, which the requests are checked against.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.theme_catalog.borrow().listed.is_none() && std::time::Instant::now() < deadline {
        pickers::theme_list(
            &app.theme_catalog,
            (&app.theme_places(), app.waker.as_ref()),
            fake.appearance(),
            &app.config.theme,
        );
        app.take_themes(&mut fake);
        std::thread::sleep(Duration::from_millis(5));
    }
    let run = |fake: &mut Fake, app: &mut App, command: &str| {
        fake.send_app(app, AppEvent::Command(command.to_owned()));
        told.borrow().last().cloned().expect("a call")
    };
    // A terminal's color: written, its comment kept, shown at once.
    assert_eq!(run(&mut fake, &mut app, "values.background"), None);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("background = \"#202020\"   # mine"), "{text}");
    assert_eq!(app.theme.light.colors.background, rgb(0x202020));
    // A token: into `[interface]`.
    assert_eq!(run(&mut fake, &mut app, "values.accent"), None);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("[interface]\n\"accent\" = \"#00ff00\""),
        "{text}"
    );
    // A name of no color, and a cycle, refused, why told, the file as it
    // was.
    let why = run(&mut fake, &mut app, "values.unknown").expect("refused");
    assert!(why.contains("`nowhere` names no color"), "{why}");
    assert_eq!(run(&mut fake, &mut app, "values.ring"), None);
    let why = run(&mut fake, &mut app, "values.cycle").expect("refused");
    assert!(why.contains("name each other"), "{why}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("\"field.focus\" ="), "{text}");
    // Taken away.
    assert_eq!(run(&mut fake, &mut app, "values.reset"), None);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("\"accent\""), "{text}");
    let _ = std::fs::remove_dir_all(&directory);
}

/// What a plugin of the color panel heard: its colors, then `closed`.
type Heard = std::rc::Rc<std::cell::RefCell<Vec<String>>>;

/// A plugin that shows the color panel, closes it, and shows Mine's
/// background before it writes it, keeping what it hears of the panel.
struct Panel(Heard);

impl plugins::Plugin for Panel {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["panel.show", "panel.close", "panel.preview", "panel.write"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let gray = lokyr_kernel::themes::ThemeColor::rgb(16, 16, 16);
        match name {
            "panel.show" => cx.show_color_panel(gray, true),
            "panel.close" => cx.close_color_panel(),
            "panel.preview" => cx.preview_theme_value("Mine", "background", Some("#303030")),
            _ => cx.set_theme_value("Mine", "background", Some("#404040")),
        }
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for event in events {
            match event {
                plugins::Event::ColorPicked { color } => {
                    self.0.borrow_mut().push(color.to_string());
                }
                plugins::Event::ColorPanelClosed => self.0.borrow_mut().push("closed".to_owned()),
                _ => {}
            }
        }
        Ok(())
    }
}

#[test]
fn the_color_panel_tells_its_plugin_and_a_color_shows_before_it_is_written() {
    let mine = format!(
        "[theme]\nname = \"Mine\"\n\n[terminal]\nbackground = \"#101010\"\nforeground = \"#e0e0e0\"\n{PALETTE}\n"
    );
    let directory =
        std::env::temp_dir().join(format!("lokyr-app-themes-panel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("themes")).unwrap();
    let file = directory.join("themes/Mine.toml");
    std::fs::write(&file, &mine).unwrap();
    let path = directory.join("config.toml");
    std::fs::write(&path, "[theme]\nlight = \"Mine\"\n").unwrap();
    let heard = Heard::default();
    let mut app = App::new(None, Config::read(&path).unwrap(), Some(path.clone()))
        .with_program(quiet())
        .with_plugin("panel", Box::new(Panel(std::rc::Rc::clone(&heard))));
    let mut fake = Fake::default();
    fake.start(&mut app);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while app.theme_catalog.borrow().listed.is_none() && std::time::Instant::now() < deadline {
        pickers::theme_list(
            &app.theme_catalog,
            (&app.theme_places(), app.waker.as_ref()),
            fake.appearance(),
            &app.config.theme,
        );
        app.take_themes(&mut fake);
        std::thread::sleep(Duration::from_millis(5));
    }
    let run = |fake: &mut Fake, app: &mut App, command: &str| {
        fake.send_app(app, AppEvent::Command(command.to_owned()));
    };
    let color = |red, green, blue, alpha| lokyr_platform_core::PanelColor {
        red,
        green,
        blue,
        alpha,
    };
    // Shown of its color and its opacity's slider; its colors and its
    // closing to the plugin.
    run(&mut fake, &mut app, "panel.show");
    assert_eq!(fake.color_panel(), Some((color(16, 16, 16, 255), true)));
    fake.send_app(&mut app, AppEvent::ColorPicked(color(255, 0, 0, 128)));
    fake.send_app(&mut app, AppEvent::ColorPanelClosed);
    run_until(&mut fake, &mut app, |_, _| heard.borrow().len() == 2);
    assert_eq!(*heard.borrow(), ["#ff000080", "closed"]);
    // Closed by the plugin: none told, a color of no panel to none.
    run(&mut fake, &mut app, "panel.show");
    run(&mut fake, &mut app, "panel.close");
    assert_eq!(fake.color_panel(), None);
    fake.send_app(&mut app, AppEvent::ColorPicked(color(0, 0, 0, 255)));
    assert_eq!(heard.borrow().len(), 2);
    // A color shown before it is written: in force, its file as it was.
    run(&mut fake, &mut app, "panel.preview");
    assert_eq!(app.theme.light.colors.background, rgb(0x303030));
    assert!(std::fs::read_to_string(&file).unwrap().contains("#101010"));
    // Written: the file's, no longer one before it.
    run(&mut fake, &mut app, "panel.write");
    assert!(std::fs::read_to_string(&file).unwrap().contains("#404040"));
    assert_eq!(app.theme.light.colors.background, rgb(0x404040));
    assert!(app.theme_values.is_empty());
    let _ = std::fs::remove_dir_all(&directory);
}

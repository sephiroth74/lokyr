//! The themes of `[theme]`: of the themes' folder, applied as the app
//! starts, as their files change, and their errors told.

use std::path::Path;

use lokyr_vt_core::page::Rgb;

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
    assert_eq!(app.diagnostics.shown(), None);
    // Saved anew, of another background: the pane takes it.
    write(&directory, "Nightfall", &theme("#202024"));
    changed(&mut fake, &mut app);
    assert_eq!(pane_background(&app, window), rgb(0x202024));
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
    let shown = app.diagnostics.shown().cloned().expect("a notice");
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
    assert_eq!(app.diagnostics.shown(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x303030));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_name_of_no_theme_tells_itself_and_its_file_applies_as_it_comes() {
    let config = "[theme]\ndark = \"Later\"\n";
    let (mut fake, mut app, directory) = with_themes("later", config, &[]);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
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
    assert_eq!(app.diagnostics.shown(), None);
    assert_eq!(app.theme.dark.colors.background, rgb(0x404040));
    // The light one stays the built-in one.
    assert_eq!(app.theme.light, Theme::builtin().light);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_theme_that_the_app_holds_applies_by_its_name_after_the_folder_s() {
    let config = "[theme]\ndark = \"Held Night\"\n";
    let (mut fake, mut app, directory) = with_themes("held", config, &[]);
    assert!(app.diagnostics.shown().is_some(), "none held yet");
    let held = directory.join("held");
    std::fs::create_dir_all(&held).unwrap();
    std::fs::write(held.join("Held Night.toml"), theme("#123456")).unwrap();
    app.held_themes = Some(held.clone());
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
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
    assert!(app.diagnostics.shown().is_some(), "no theme `Grays` yet");
    let file = directory.join("themes").join("Grays.itermcolors");
    assert!(app.theme_files.paths.contains(&file));
    std::fs::write(&file, grays()).unwrap();
    changed(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
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
    assert_eq!(app.diagnostics.shown(), None);
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

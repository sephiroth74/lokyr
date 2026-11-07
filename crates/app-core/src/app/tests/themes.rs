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

//! The app as the system's default terminal: the menu's item, the scripts
//! that the system opens with it, and the `ssh` and man pages' links.

use super::*;

/// An executable script at `path` of `text`.
fn script(path: &std::path::Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The windows that opened since `before`.
fn opened(fake: &Fake, before: &[WindowId]) -> Vec<WindowId> {
    fake.windows()
        .filter(|window| !before.contains(window))
        .collect()
}

#[test]
fn the_default_terminal_s_item_shows_the_system_s_default_and_asks_for_it() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet());
    fake.start(&mut app);
    // In the app's menu, after Secure Keyboard Entry.
    let items = menu_items(&fake, 0);
    let at = |title: &str| items.iter().position(|item| item == title);
    assert_eq!(
        at("Make Lokyr the Default Terminal"),
        at("Secure Keyboard Entry").map(|secure| secure + 1),
        "{items:?}"
    );
    // Not the default: of no tick, and chosen, it asks the system.
    assert!(!fake.command_checked(DEFAULT_TERMINAL));
    assert!(fake.command_enabled(DEFAULT_TERMINAL));
    fake.send_app(&mut app, AppEvent::Command(DEFAULT_TERMINAL.to_owned()));
    assert_eq!(fake.default_terminal_asks(), 1);
    // The system's answers: ticked and dimmed once it is the default.
    fake.set_default_terminal(true);
    fake.send_app(
        &mut app,
        AppEvent::DefaultTerminalMade {
            failed: vec!["the ssh URLs: declined".to_owned()],
        },
    );
    assert!(fake.command_checked(DEFAULT_TERMINAL));
    assert!(!fake.command_enabled(DEFAULT_TERMINAL));
    // As the system tells it, after the start, which does not wait for
    // its answer.
    let mut fake = Fake::default();
    fake.set_default_terminal(true);
    let mut app = app_of(Config::default(), quiet());
    fake.start(&mut app);
    assert!(!fake.command_checked(DEFAULT_TERMINAL));
    fake.send_app(&mut app, AppEvent::DefaultTerminalKnown);
    assert!(fake.command_checked(DEFAULT_TERMINAL));
    assert!(!fake.command_enabled(DEFAULT_TERMINAL));
}

#[test]
fn a_script_runs_in_a_window_of_its_own_which_keeps_what_it_printed() {
    let base = real_directory("opened-script");
    let path = base.join("run me.command");
    script(&path, "#!/bin/sh\necho \"ran in $PWD\"\n");
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet());
    fake.start(&mut app);
    let before: Vec<WindowId> = fake.windows().collect();
    fake.send_app(&mut app, AppEvent::OpenFiles(vec![path.clone()]));
    let new = opened(&fake, &before);
    let [window] = new[..] else {
        panic!("one window: {new:?}");
    };
    assert_eq!(fake.focus_requests().last(), Some(&window));
    // It ran in its folder, and ended of 0, which the default
    // `close-on-exit` closes a window of: its window stays.
    let ran = format!("ran in {}", base.display());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows.contains_key(&window)
            && app.pane(window).is_kept()
            && (0..4).any(|line| row(app, window, line) == ran)
    });
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(fake.windows().any(|open| open == window));
    // A script of its extension runs, executable or not, as the system
    // opens it; a file of no script's extension, not executable, does not.
    let plain = base.join("plain.sh");
    std::fs::write(&plain, "echo plain\n").unwrap();
    let notes = base.join("notes");
    std::fs::write(&notes, "hello\n").unwrap();
    let before: Vec<WindowId> = fake.windows().collect();
    fake.send_app(&mut app, AppEvent::OpenFiles(vec![plain, notes]));
    assert_eq!(opened(&fake, &before).len(), 1);
    // A theme's file still asks to install it, and opens no window.
    let theme = base.join("Theme.itermcolors");
    std::fs::write(&theme, "not a plist").unwrap();
    let before: Vec<WindowId> = fake.windows().collect();
    fake.send_app(&mut app, AppEvent::OpenFiles(vec![theme]));
    assert!(opened(&fake, &before).is_empty());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn an_ssh_link_runs_ssh_to_its_host_and_a_man_page_s_link_its_page() {
    let base = real_directory("opened-links");
    // An `ssh` and a `man` that tell their arguments.
    let ssh = base.join("ssh");
    script(&ssh, "#!/bin/sh\necho \"ssh $*\"\nexec sleep 30\n");
    let man = base.join("man");
    script(&man, "#!/bin/sh\necho \"man $*\"\n");
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet());
    app.ssh = ssh;
    app.man = Some(man);
    fake.start(&mut app);
    let before: Vec<WindowId> = fake.windows().collect();
    fake.send_app(
        &mut app,
        AppEvent::OpenUrls(vec![
            "ssh://ada@example.com:2222/".to_owned(),
            "x-man-page://1/open".to_owned(),
            "ssh://-oProxyCommand=open".to_owned(),
            "https://example.com".to_owned(),
        ]),
    );
    let new = opened(&fake, &before);
    let [connecting, page] = new[..] else {
        panic!("two windows: {new:?}");
    };
    run_until(&mut fake, &mut app, |_, app| {
        row(app, connecting, 0) == "ssh -p 2222 -- ada@example.com"
    });
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(page).is_kept() && row(app, page, 0) == "man -P ul -- 1 open"
    });
    assert_eq!(fake.window(page).unwrap().title, "man open(1) — 80×48");
    assert_eq!(fake.focus_requests().last(), Some(&page));
    let _ = std::fs::remove_dir_all(base);
}

/// The app, of `config`, whose windows run `program`, as [`app`] makes
/// it, where a local of that name hides it.
fn app_of(config: Config, program: Program) -> App {
    app(config, program)
}

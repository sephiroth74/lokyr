//! The windows and the tabs at folders that the system asks for: dropped
//! on the app's icon, and the file manager's services.

use super::*;

/// A program that waits in the directory that it started in.
fn waiting() -> Program {
    shell("exec sleep 30")
}

/// The folders that the panes of `window`'s tabs work in, in their order,
/// once each works in one.
fn tab_folders(fake: &mut Fake, app: &mut App, window: WindowId) -> Vec<PathBuf> {
    let panes = |app: &App| -> Vec<PaneId> {
        let shown = app.workspace.window(app.windows[&window].window).unwrap();
        shown
            .tabs()
            .iter()
            .map(|&tab| app.workspace.tab(tab).unwrap().active_pane())
            .collect()
    };
    run_until(fake, app, |_, app| {
        panes(app).iter().all(|&pane| {
            app.workspace
                .pane(pane)
                .is_some_and(|pane| pane.working_directory().is_some())
        })
    });
    panes(app)
        .iter()
        .filter_map(|&pane| app.workspace.pane(pane)?.working_directory())
        .collect()
}

/// The windows that opened since `before`.
fn opened(fake: &Fake, before: &[WindowId]) -> Vec<WindowId> {
    fake.windows()
        .filter(|window| !before.contains(window))
        .collect()
}

#[test]
fn folders_dropped_on_the_icon_open_a_window_each_there() {
    let base = real_directory("dropped-folders");
    let (a, b) = (base.join("a"), base.join("b"));
    for made in [&a, &b] {
        std::fs::create_dir(made).unwrap();
    }
    let mut fake = Fake::default();
    let mut app = app(Config::default(), waiting());
    fake.start(&mut app);
    let before: Vec<WindowId> = fake.windows().collect();
    fake.send_app(
        &mut app,
        AppEvent::OpenFiles(vec![a.clone(), b.clone(), base.join("gone")]),
    );
    let new = opened(&fake, &before);
    assert_eq!(new.len(), 2);
    assert_eq!(tab_folders(&mut fake, &mut app, new[0]), [a]);
    assert_eq!(tab_folders(&mut fake, &mut app, new[1]), [b]);
    // The last comes forward, with the app.
    assert_eq!(fake.focus_requests().last(), Some(&new[1]));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_services_open_a_window_or_a_tab_at_each_folder() {
    let base = real_directory("service-folders");
    let (a, b, c) = (base.join("a"), base.join("b"), base.join("c"));
    for made in [&a, &b, &c] {
        std::fs::create_dir(made).unwrap();
    }
    let file = c.join("notes.txt");
    std::fs::write(&file, "").unwrap();
    let mut fake = Fake::default();
    let mut app = app(Config::default(), waiting());
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    // A window at a folder.
    fake.send_app(
        &mut app,
        AppEvent::OpenFolders {
            folders: vec![a.clone()],
            tabs: false,
        },
    );
    let new = opened(&fake, &[first]);
    assert_eq!(new.len(), 1);
    assert_eq!(
        tab_folders(&mut fake, &mut app, new[0]),
        std::slice::from_ref(&a)
    );
    assert_eq!(fake.focus_requests().last(), Some(&new[0]));
    // Tabs, in the window in front, the one that had the focus last: a
    // folder's, and a file's folder's, the last shown.
    fake.send(&mut app, first, WindowEvent::Focused(true));
    fake.send(&mut app, first, WindowEvent::Focused(false));
    let home = std::env::home_dir().unwrap();
    fake.send_app(
        &mut app,
        AppEvent::OpenFolders {
            folders: vec![b.clone(), file, base.join("gone")],
            tabs: true,
        },
    );
    assert_eq!(fake.windows().count(), 2);
    let folders = tab_folders(&mut fake, &mut app, first);
    assert_eq!(folders, [home, b, c.clone()]);
    let shown = app.workspace.window(app.windows[&first].window).unwrap();
    assert_eq!(shown.active_tab(), shown.tabs()[2]);
    assert_eq!(fake.focus_requests().last(), Some(&first));
    // Without a window, a tab's folder opens a window.
    for window in fake.windows().collect::<Vec<_>>() {
        app.close(&mut fake, window);
    }
    fake.send_app(
        &mut app,
        AppEvent::OpenFolders {
            folders: vec![c.clone()],
            tabs: true,
        },
    );
    let windows: Vec<WindowId> = fake.windows().collect();
    assert_eq!(windows.len(), 1);
    assert_eq!(tab_folders(&mut fake, &mut app, windows[0]), [c]);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_start_to_open_folders_opens_no_new_window_of_its_own() {
    let base = real_directory("launch-folders");
    let mut fake = Fake::default();
    fake.set_default_launch(false);
    let mut app = app(Config::default(), waiting());
    fake.start(&mut app);
    // The folder's window alone, the wait past.
    assert_eq!(fake.windows().count(), 0);
    fake.send_app(&mut app, AppEvent::OpenFiles(vec![base.clone()]));
    fake.fire_timers(&mut app, Instant::now() + UNTITLED_WAIT * 2);
    let windows: Vec<WindowId> = fake.windows().collect();
    assert_eq!(windows.len(), 1);
    assert_eq!(
        tab_folders(&mut fake, &mut app, windows[0]),
        std::slice::from_ref(&base)
    );
    // What opened no window, as a theme's file, leaves a new window to
    // open once the wait is past.
    let mut fake = Fake::default();
    fake.set_default_launch(false);
    let mut app = app_of(Config::default(), quiet());
    fake.start(&mut app);
    fake.fire_timers(&mut app, Instant::now() + UNTITLED_WAIT / 2);
    assert_eq!(fake.windows().count(), 0);
    fake.fire_timers(&mut app, Instant::now() + UNTITLED_WAIT * 2);
    assert_eq!(fake.windows().count(), 1);
    let _ = std::fs::remove_dir_all(base);
}

/// The app, of `config`, whose windows run `program`, as [`app`] makes
/// it, where a local of that name hides it.
fn app_of(config: Config, program: Program) -> App {
    app(config, program)
}

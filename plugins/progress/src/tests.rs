//! The plugin against the SDK's fake host, whose window's tabs tell their
//! programs' progress as the test gives it.

use lokyr_plugin_sdk::fake::{FakeHost, TabMarks};
use lokyr_plugin_sdk::host::{PaneId, TabId};

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/progress";

/// The window of the tests.
const WINDOW: WindowId = WindowId(1);

/// Its view.
const VIEW: ViewId = ViewId::WindowStrip(WINDOW);

/// A host of one window of two tabs, of panes 1 and 2, the first shown,
/// and the plugin, activated, its bar rendered once.
fn host() -> (FakeHost, Strip) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1)).tab = TabId(1);
    host.pane(PaneId(2)).tab = TabId(2);
    host.focus(Some(PaneId(1)));
    let mut plugin = Strip::default();
    host.activate(&mut plugin).unwrap();
    assert_eq!(host.render(&mut plugin, &VIEW), None);
    (host, plugin)
}

/// Gives tab `tab` the progress `progress`, and delivers the events that
/// it owes: whether the plugin asked for its bar's render again.
fn tell(host: &mut FakeHost, plugin: &mut Strip, tab: u64, progress: Option<TabProgress>) -> bool {
    host.set_tab_marks(
        TabId(tab),
        TabMarks {
            progress,
            ..TabMarks::default()
        },
    );
    host.deliver(plugin).unwrap();
    let renders = host.take_renders();
    assert!(renders.iter().all(|view| *view == VIEW), "{renders:?}");
    !renders.is_empty()
}

fn progress(state: ProgressState, value: Option<f32>) -> Option<TabProgress> {
    Some(TabProgress { state, value })
}

/// The bar of the window's render: its state, how far it fills, its shape
/// and its height.
fn bar(host: &mut FakeHost, plugin: &mut Strip) -> Option<(ProgressState, f32)> {
    let strip = host.render(plugin, &VIEW)?;
    let [bar] = &strip.children[..] else {
        panic!("{strip:?}");
    };
    let Kind::Progress(drawn) = &bar.kind else {
        panic!("{bar:?}");
    };
    assert_eq!(
        (drawn.shape, drawn.radius, bar.layout.height),
        (ProgressShape::Bar, Some(0.0), Length::Points(HEIGHT))
    );
    assert_eq!(bar.label.as_deref(), Some("strip-label"));
    Some((drawn.state, drawn.value))
}

#[test]
fn the_bar_shows_the_shown_tab_s_progress_of_each_state() {
    let (mut host, mut plugin) = host();
    for (told, drawn) in [
        (
            progress(ProgressState::Normal, Some(0.4)),
            (ProgressState::Normal, 0.4),
        ),
        // A failed task, and a paused one, of no value fill it all.
        (
            progress(ProgressState::Error, None),
            (ProgressState::Error, 1.0),
        ),
        (
            progress(ProgressState::Paused, Some(0.7)),
            (ProgressState::Paused, 0.7),
        ),
        (
            progress(ProgressState::Indeterminate, None),
            (ProgressState::Indeterminate, 0.0),
        ),
        (
            progress(ProgressState::Normal, Some(1.5)),
            (ProgressState::Normal, 1.0),
        ),
    ] {
        assert!(tell(&mut host, &mut plugin, 1, told), "{told:?}");
        assert_eq!(bar(&mut host, &mut plugin), Some(drawn), "{told:?}");
    }
    // None takes the bar away.
    assert!(tell(&mut host, &mut plugin, 1, None));
    assert_eq!(bar(&mut host, &mut plugin), None);
}

#[test]
fn a_hidden_tab_s_progress_shows_as_its_tab_shows() {
    let (mut host, mut plugin) = host();
    // The second tab's, which the window does not show, asks nothing.
    assert!(!tell(
        &mut host,
        &mut plugin,
        2,
        progress(ProgressState::Normal, Some(0.5))
    ));
    // Shown, it shows; the first again, it goes.
    let shown = |host: &mut FakeHost, tab: u64| {
        host.focus(Some(PaneId(tab)));
        host.post(Event::Workspace(WorkspaceEvent::TabActivated {
            tab: TabId(tab),
            window: WINDOW,
        }));
    };
    shown(&mut host, 2);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(host.take_renders(), [VIEW]);
    assert_eq!(
        bar(&mut host, &mut plugin),
        Some((ProgressState::Normal, 0.5))
    );
    shown(&mut host, 1);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(host.take_renders(), [VIEW]);
    assert_eq!(bar(&mut host, &mut plugin), None);
    // The same progress again asks no render.
    assert!(!tell(&mut host, &mut plugin, 1, None));
}

#[test]
fn a_closed_window_s_bar_goes() {
    let (mut host, mut plugin) = host();
    tell(
        &mut host,
        &mut plugin,
        1,
        progress(ProgressState::Normal, Some(0.5)),
    );
    host.close_window(WINDOW);
    host.deliver(&mut plugin).unwrap();
    assert!(plugin.shown.is_empty());
    assert!(host.take_renders().is_empty());
}

#[test]
fn the_label_is_in_both_languages() {
    for (_, catalog) in CATALOGS {
        assert!(catalog.contains("strip-label = "));
    }
}

//! The side of the status bar of a plugin's view: its manifest's, or the
//! one that it asked for, until it stops.

use lokyr_pkg_model::manifest::Manifest;
use lokyr_plugin_sdk::host::StatusBarSide;

use super::*;

#[test]
fn a_plugin_s_status_bar_side_is_its_manifest_s_then_its_ask_until_it_stops() {
    let mut app = App::default();
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"lokyr/segment\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"status-bar\"]\n[contributes.status-bar]\nside = \"end\"\n",
    ))
    .unwrap();
    let mut plugins = Plugins::new();
    let segment = scripted(
        |cx| cx.register_command(command("segment.center")),
        |cx, _, _| cx.set_status_bar_side(StatusBarSide::Center),
    );
    plugins.add_declared(manifest, segment).unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    // Its manifest's, then the one that it asks for.
    assert_eq!(plugins.status_bar_side("lokyr/segment"), StatusBarSide::End);
    plugins
        .run("segment.center", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        plugins.status_bar_side("lokyr/segment"),
        StatusBarSide::Center
    );
    // Stopped, as its section turns it off, its manifest's again.
    let off = crate::config::Config::parse(
        "[plugins.segment]\nenabled = false\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    plugins.configure(&off.plugins);
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("lokyr/segment"), Some(Status::Off));
    assert_eq!(plugins.status_bar_side("lokyr/segment"), StatusBarSide::End);
    // A plugin not there: the start.
    assert_eq!(plugins.status_bar_side("lokyr/none"), StatusBarSide::Start);
}

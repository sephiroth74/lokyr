//! Built-in plugin of the programs' progress, as OSC 9;4 tells it, as
//! Gradle's: a thin bar across each window's top, under the titlebar or the
//! tab bar, over the panes, of the progress of the tab that the window
//! shows.
//!
//! The bar fills from the left as far as the percentage, in the accent's
//! color; a failed task's is red, a paused one's yellow, and one of no known
//! end sweeps across, as the window server moves it; none takes the bar
//! away. Like every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::Error;
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{TabProgress, WindowId};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::ViewId;
use lokyr_plugin_sdk::view::{Kind, Layout, Length, Node, Progress, ProgressShape, ProgressState};

lokyr_plugin_sdk::export_plugin!(Strip);

static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/progress.ftl")),
    ("it", include_str!("../locales/it/progress.ftl")),
];

/// The bar's height, in points: a line, two pixels on a Retina display.
const HEIGHT: f32 = 1.0;

/// The plugin of the progress's bar, and the progress that each window's
/// bar showed at its last render, which a change renders again.
#[derive(Debug, Default)]
pub struct Strip {
    shown: BTreeMap<WindowId, Option<TabProgress>>,
}

impl Plugin for Strip {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.subscribe(&WorkspaceEvent::NAMES)
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    /// A window's bar: the progress of the tab that it shows, or none.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::WindowStrip(window) = view else {
            return None;
        };
        let progress = shown_progress(cx, *window);
        self.shown.insert(*window, progress);
        let label = cx.message("strip-label", &[]).unwrap_or_default();
        progress.map(|progress| strip(progress, &label))
    }

    /// A change of the tabs renders again each window's bar whose progress
    /// it changed: a tab's progress, another tab shown, a tab moved; a
    /// closed window's bar goes.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if let Event::Workspace(WorkspaceEvent::WindowClosed { window }) = event {
                self.shown.remove(window);
            }
        }
        let windows: Vec<(WindowId, Option<TabProgress>)> = self
            .shown
            .iter()
            .map(|(&window, &last)| (window, last))
            .collect();
        for (window, last) in windows {
            if shown_progress(cx, window) != last {
                cx.request_render(ViewId::WindowStrip(window))?;
            }
        }
        Ok(())
    }
}

/// The progress of the tab that `window` shows, if its program tells one.
fn shown_progress(cx: &mut Cx<'_>, window: WindowId) -> Option<TabProgress> {
    cx.tabs(window)
        .ok()?
        .into_iter()
        .find(|tab| tab.active)?
        .progress
}

/// The bar of `progress`, of `label` to accessibility, across the strip's
/// width, of square ends at the window's edges.
fn strip(progress: TabProgress, label: &str) -> Node {
    let bar = Node::new(Kind::Progress(Progress {
        state: progress.state,
        value: filled(progress),
        shape: ProgressShape::Bar,
        radius: Some(0.0),
        ..Progress::default()
    }))
    .label(label)
    .layout(Layout {
        grow: 1.0,
        height: Length::Points(HEIGHT),
        ..Layout::default()
    });
    Node::container(vec![bar]).layout(Layout {
        grow: 1.0,
        ..Layout::default()
    })
}

/// How far `progress` fills, from 0 to 1: its value, or, of none, all of a
/// failed or a paused one, and none of an indeterminate one, which a part
/// that sweeps across shows.
fn filled(progress: TabProgress) -> f32 {
    progress
        .value
        .unwrap_or(match progress.state {
            ProgressState::Indeterminate => 0.0,
            _ => 1.0,
        })
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;

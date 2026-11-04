//! A plugin of a view in every slot, for a build of tests, as `--features
//! sample-slots` builds it: the window's tabs in the tab bar, an
//! indeterminate progress in the strip, the tabs' count in the status bar,
//! and each pane's id in its header. Each change of the workspace renders
//! its views again.

use lokyr_kernel::commands::Error;
use lokyr_kernel::plugins::ui::ViewId;
use lokyr_kernel::plugins::view::{
    Align, Edges, Length, Node, ProgressShape, ProgressState, Style, Token,
};
use lokyr_kernel::plugins::{Cx, Event, Plugin, WorkspaceEvent};

/// Its manifest, of every slot.
pub(crate) const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/sample-slots\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"tab-bar\", \"window-strip\", \"status-bar\", \"pane-header\"]\n",
);

/// The plugin, and the views that it rendered, which it renders again.
#[derive(Default)]
pub(crate) struct SampleSlots {
    views: Vec<ViewId>,
}

/// A bar of `children`, of a token's background, its texts in the middle
/// of its height, past a padding.
fn bar(children: Vec<Node>, background: &str) -> Node {
    let mut node = Node::container(children);
    node.layout.grow = 1.0;
    node.layout.align = Align::Center;
    node.layout.gap = 12.0;
    node.layout.padding = Edges::all(6.0);
    node.style = Style {
        background: Some(Token::from(background)),
        ..Style::default()
    };
    node
}

/// A text of a token's color.
fn text(text: String, color: &str) -> Node {
    let mut node = Node::text(text);
    node.style.color = Some(Token::from(color));
    node
}

impl Plugin for SampleSlots {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.subscribe(&WorkspaceEvent::NAMES)
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, _: &[Event]) -> Result<(), Error> {
        for view in &self.views {
            // A view of a window or a pane that closed is no longer one.
            let _ = cx.request_render(view.clone());
        }
        Ok(())
    }

    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if !self.views.contains(view) {
            self.views.push(view.clone());
        }
        match view {
            ViewId::TabBar(window) => {
                let tabs = cx.tabs(*window).ok()?;
                let shown = tabs
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        let title = if tab.title.is_empty() {
                            format!("Tab {}", index + 1)
                        } else {
                            tab.title.clone()
                        };
                        let color = if tab.active {
                            "tab.active.fg"
                        } else {
                            "tab.inactive.fg"
                        };
                        text(title, color)
                    })
                    .collect();
                Some(bar(shown, "tab.bar.bg"))
            }
            ViewId::WindowStrip(_) => {
                let mut progress =
                    Node::progress(ProgressShape::Bar, ProgressState::Indeterminate, 0.0);
                progress.layout.grow = 1.0;
                progress.layout.height = Length::Points(3.0);
                let mut strip = Node::container(vec![progress]);
                strip.layout.grow = 1.0;
                Some(strip)
            }
            ViewId::StatusBar(window) => {
                let count = cx.tabs(*window).map_or(0, |tabs| tabs.len());
                Some(bar(
                    vec![text(
                        format!("sample-slots: {count} tab(s)"),
                        "text.secondary",
                    )],
                    "tab.bar.bg",
                ))
            }
            ViewId::PaneHeader(pane) => Some(bar(
                vec![text(format!("pane {}", pane.0), "text.secondary")],
                "tab.inactive.bg",
            )),
            ViewId::Panel(_) => None,
        }
    }
}

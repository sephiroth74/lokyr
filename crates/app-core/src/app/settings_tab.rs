//! The settings tab: a tab of no pane, one at most in the app, which shows
//! the settings' pages in the room that a tab's panes take, as a view of
//! the app's own under the window's panels; opened in the window in front,
//! or the one open shown, and closed as a tab closes.

use lokyr_kernel::plugins::ui::{
    Anchor as PanelAnchor, Corner as PanelCorner, PanelId, PanelOptions, Role as PanelRole, ViewId,
};
use lokyr_kernel::plugins::view::{
    Align, Direction, Edges, FontWeight, Justify, Layout, Length, Node, Style, TextSize, Token,
};
use lokyr_platform_core::{Platform, WindowId};
use lokyr_ui_core::panel::{Anchor, Dismiss, Options, Role};

use super::App;
use super::chrome::{OVERFLOW, PanelPlace, SlotView, free_access};
use crate::drawing::Backend;

/// The id of the settings tab's view among the app's own views.
pub(super) const SETTINGS: &str = "settings-tab";

/// The width of the settings tab's sidebar, in points.
const SIDEBAR: f32 = 200.0;

/// The settings tab's view, the app's own.
pub(super) fn settings_view() -> ViewId {
    ViewId::Panel(PanelId::from(SETTINGS))
}

impl<B: Backend> App<B> {
    /// Opens the settings tab in the window that the commands run in, or
    /// the one that has the focus, right after the tab that it shows, or a
    /// new window's; or shows the one open, wherever it is.
    pub(super) fn open_settings_tab(&mut self, platform: &mut dyn Platform) {
        let title = self.texts.text("settings-tab-title");
        if let Some((tab, shown)) = self.workspace.settings_tab() {
            self.workspace.activate_tab(tab);
            let window = self
                .windows
                .iter()
                .find(|(_, state)| state.window == shown)
                .map(|(&window, _)| window);
            if let Some(window) = window {
                platform.focus_window(window);
            }
            return;
        }
        let window = match self.acting.or_else(|| platform.focused_window()) {
            Some(window) if self.windows.contains_key(&window) => Some(window),
            _ => {
                let program = self.program.clone();
                self.open_window_running(platform, &program, None, None)
            }
        };
        let Some(shown) = window.and_then(|window| self.windows.get(&window)) else {
            return;
        };
        let shown = shown.window;
        self.workspace.open_settings(shown, &title);
        if let Some(window) = window {
            platform.focus_window(window);
        }
    }

    /// Opens `window`'s view of the settings tab, under its panels, as the
    /// tab shows, or closes it as the tab hides or closes.
    pub(super) fn settle_settings(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let shows = self
            .workspace
            .window(state.window)
            .is_some_and(|shown| self.workspace.is_settings(shown.active_tab()));
        let open = state
            .chrome
            .iter()
            .find(|view| view.plugin == OVERFLOW && view.view == settings_view())
            .map(|view| view.id);
        match (shows, open) {
            (true, None) => self.open_settings_view(platform, window),
            (false, Some(view)) => self.close_panel(platform, window, view, false),
            _ => {}
        }
    }

    /// Opens `window`'s view of the settings tab, under its other panels,
    /// of no keyboard yet.
    fn open_settings_view(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let scale = platform.window_size(window).map_or(1.0, |size| size.scale) as f32;
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        state.next_view += 1;
        let view = state.next_view;
        let access = free_access(state.chrome.iter());
        state.panels.open_under(
            view,
            Options {
                anchor: Anchor::At((0.0, 0.0)),
                role: Role::Overlay,
                dismiss: Dismiss {
                    outside: false,
                    escape: false,
                },
            },
        );
        let options = PanelOptions {
            anchor: PanelAnchor::WindowCorner {
                window: state.window.into(),
                corner: PanelCorner::TopLeft,
            },
            role: PanelRole::Overlay,
            close_outside: false,
            close_escape: false,
            takes_keys: false,
            close_typing: false,
        };
        state.chrome.push(SlotView::panel(
            (view, access),
            (OVERFLOW.to_owned(), PanelId::from(SETTINGS)),
            scale,
            PanelPlace {
                options,
                pointer: (0.0, 0.0),
            },
        ));
    }

    /// The settings tab's tree: its sidebar, and the chosen page beside
    /// it; a page that tells that none is there yet, until the plugins'
    /// pages come.
    pub(super) fn settings_tree(&self) -> Node {
        let sidebar = Node::container(vec![
            Node::text(self.texts.text("settings-tab-title")).style(Style {
                font_weight: Some(FontWeight::Semibold),
                ..Style::default()
            }),
        ])
        .key("sidebar")
        .layout(Layout {
            direction: Direction::Column,
            width: Length::Points(SIDEBAR),
            height: Length::Fraction(1.0),
            padding: Edges {
                top: 16.0,
                right: 12.0,
                bottom: 16.0,
                left: 16.0,
            },
            ..Layout::default()
        })
        .style(Style {
            background: Some(Token::from("panel.bg")),
            ..Style::default()
        });
        let page = Node::container(vec![
            Node::text(self.texts.text("settings-tab-empty")).style(Style {
                text_size: Some(TextSize::Small),
                color: Some(Token::from("text.secondary")),
                ..Style::default()
            }),
        ])
        .key("page")
        .layout(Layout {
            grow: 1.0,
            justify: Justify::Center,
            align: Align::Center,
            ..Layout::default()
        });
        Node::container(vec![sidebar, page])
            .key("settings")
            .layout(Layout {
                direction: Direction::Row,
                width: Length::Fraction(1.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            })
    }
}

//! The settings tab: a tab of no pane, one at most in the app, which shows
//! the settings' pages in the room that a tab's panes take, as a view of
//! the app's own under the window's panels; opened in the window in front,
//! or the one open shown, and closed as a tab closes.

use lokyr_kernel::plugins::ui::{
    Anchor as PanelAnchor, Corner as PanelCorner, PanelId, PanelOptions, Role as PanelRole,
    UiEvent, ViewId,
};
use lokyr_kernel::plugins::view::{
    Align, Direction, Edges, Justify, Kind, Layout, Length, ListItem, Node, Role as NodeRole,
    Style, TextSize, Token, UiEventKind,
};
use lokyr_platform_core::{Platform, WindowId};
use lokyr_ui_core::panel::{Anchor, Dismiss, Options, Role};

use super::App;
use super::chrome::{OVERFLOW, PanelPlace, SlotView, free_access};
use crate::drawing::Backend;

/// The id of the settings tab's view among the app's own views.
pub(super) const SETTINGS: &str = "settings-tab";

/// The width of the settings tab's sidebar, in points.
const SIDEBAR: f32 = 220.0;

/// The height of a row of the sidebar, in points, as macOS's sidebars'.
const ROW: f32 = 28.0;

/// The key of the sidebar's list of pages.
pub(super) const PAGES: &str = "pages";

/// A page of the settings tab: its id, its title in the language shown,
/// and its icon, of the host's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Page {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) icon: String,
}

/// What the app keeps of the settings tab: its pages, in the sidebar's
/// order, the chosen one, and the revision of its tree, which a change of
/// either renews.
#[derive(Debug, Default)]
pub(crate) struct SettingsTab {
    pub(crate) pages: Vec<Page>,
    pub(crate) chosen: usize,
    pub(crate) revision: u64,
}

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
                role: Role::Dialog,
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
            role: PanelRole::Dialog,
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

    /// An event of the settings tab's nodes: a page chosen in the sidebar,
    /// by a click or the keys, shows beside it.
    pub(super) fn settings_event(&mut self, platform: &mut dyn Platform, event: &UiEvent) {
        let item = match (&event.kind, event.key.0.as_str()) {
            (UiEventKind::Select { item } | UiEventKind::Activate { item }, PAGES) => *item,
            _ => return,
        };
        if item < self.settings.pages.len() && item != self.settings.chosen {
            self.settings.chosen = item;
            self.settings.revision += 1;
            self.request_frames(platform);
        }
    }

    /// The settings tab's tree: its sidebar of pages, each a row of its
    /// icon and its title, the chosen one selected, and the chosen page
    /// beside it; of no page, a text that tells that none is there yet.
    pub(super) fn settings_tree(&self) -> Node {
        let pages = &self.settings.pages;
        let row = Node::container(vec![
            Node::icon("").style(Style {
                color: Some(Token::from("accent")),
                ..Style::default()
            }),
            Node::text(""),
        ])
        .layout(Layout {
            direction: Direction::Row,
            gap: 8.0,
            align: Align::Center,
            height: Length::Points(ROW),
            padding: Edges {
                top: 0.0,
                right: 8.0,
                bottom: 0.0,
                left: 8.0,
            },
            ..Layout::default()
        });
        let items = pages
            .iter()
            .map(|page| ListItem {
                texts: vec![page.icon.clone(), page.title.clone()],
            })
            .collect();
        let mut list = Node::list(row, pages.len(), (0, items))
            .key(PAGES)
            .label(self.texts.text("settings-tab-title"))
            .layout(Layout {
                width: Length::Fraction(1.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            });
        if let Kind::List(list) = &mut list.kind {
            list.row_height = Some(ROW);
            list.menu = true;
        }
        let sidebar = Node::container(vec![list])
            .key("sidebar")
            .role(NodeRole::Group)
            .layout(Layout {
                direction: Direction::Column,
                width: Length::Points(SIDEBAR),
                height: Length::Fraction(1.0),
                padding: Edges {
                    top: 10.0,
                    right: 10.0,
                    bottom: 10.0,
                    left: 10.0,
                },
                ..Layout::default()
            })
            .style(Style {
                background: Some(Token::from("panel.bg")),
                ..Style::default()
            });
        let shown = match pages.get(self.settings.chosen) {
            Some(page) => Node::text(page.title.clone()).style(Style {
                text_size: Some(TextSize::Large),
                ..Style::default()
            }),
            None => Node::text(self.texts.text("settings-tab-empty")).style(Style {
                text_size: Some(TextSize::Small),
                color: Some(Token::from("text.secondary")),
                ..Style::default()
            }),
        };
        let page = Node::container(vec![shown])
            .key("page")
            .role(NodeRole::Group)
            .layout(Layout {
                grow: 1.0,
                justify: Justify::Center,
                align: Align::Center,
                ..Layout::default()
            });
        Node::container(vec![sidebar, Node::separator(), page])
            .key("settings")
            .layout(Layout {
                direction: Direction::Row,
                width: Length::Fraction(1.0),
                height: Length::Fraction(1.0),
                ..Layout::default()
            })
    }
}

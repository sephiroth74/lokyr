//! The settings tab: a tab of no pane, one at most in the app, which shows
//! the settings' pages in the room that a tab's panes take, as a view of
//! the app's own under the window's panels, its sidebar of the pages that
//! Lokyr's own plugins' manifests have, and the chosen page beside it, its
//! plugin's view over the app's; opened in the window in front, or the one
//! open shown, and closed as a tab closes. The plugin of the page shown
//! hears it shown, and hidden as another page, another tab or none shows.

use std::sync::Arc;

use lokyr_kernel::plugins::host::PluginInfo;
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

/// The key of the room beside the sidebar that the chosen page's view
/// takes.
pub(super) const PAGE: &str = "page";

/// A page of the settings tab: its plugin, its id, its title in the
/// language shown, and its icon, of the host's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Page {
    pub(crate) plugin: String,
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) icon: String,
}

/// What the app keeps of the settings tab: its pages, in the sidebar's
/// order, the chosen one, the revision of its tree, which a change of
/// either renews, the plugins' list and the language that the pages were
/// read of, and the page that its plugin was told shows.
#[derive(Debug, Default)]
pub(crate) struct SettingsTab {
    pub(crate) pages: Vec<Page>,
    pub(crate) chosen: usize,
    pub(crate) revision: u64,
    read: Option<(Arc<[PluginInfo]>, String)>,
    told: Option<(String, String)>,
}

impl SettingsTab {
    /// The chosen page's plugin and view, if the tab has a page.
    pub(super) fn chosen_page(&self) -> Option<(&str, ViewId)> {
        self.pages
            .get(self.chosen)
            .map(|page| (page.plugin.as_str(), ViewId::Page(page.id.clone())))
    }
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
        self.refresh_pages();
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

    /// Shows the settings tab at the page `page` of the plugin `plugin`, in
    /// the window in front, or where the tab shows, as its plugin asks.
    pub(super) fn show_page(&mut self, platform: &mut dyn Platform, plugin: &str, page: &str) {
        self.refresh_pages();
        let found = self
            .settings
            .pages
            .iter()
            .position(|shown| shown.plugin == plugin && shown.id == page);
        if let Some(at) = found
            && at != self.settings.chosen
        {
            self.settings.chosen = at;
            self.settings.revision += 1;
        }
        self.open_settings_tab(platform);
        self.request_frames(platform);
    }

    /// Reads the pages of the plugins anew, as the plugins' list or the
    /// language changed since: the chosen page stays, or the first is.
    pub(super) fn refresh_pages(&mut self) {
        let infos = self.plugins.infos();
        let language = self.texts.language();
        if self
            .settings
            .read
            .as_ref()
            .is_some_and(|(read, of)| Arc::ptr_eq(read, &infos) && of == language)
        {
            return;
        }
        let pages: Vec<Page> = self
            .plugins
            .pages(language)
            .into_iter()
            .map(|page| Page {
                plugin: page.plugin,
                id: page.id,
                title: page.title,
                icon: page.icon,
            })
            .collect();
        let chosen = self
            .settings
            .pages
            .get(self.settings.chosen)
            .and_then(|was| {
                pages
                    .iter()
                    .position(|page| page.plugin == was.plugin && page.id == was.id)
            })
            .unwrap_or(0);
        if pages != self.settings.pages || chosen != self.settings.chosen {
            self.settings.pages = pages;
            self.settings.chosen = chosen;
            self.settings.revision += 1;
        }
        self.settings.read = Some((infos, language.to_owned()));
    }

    /// Tells the plugin of the page that shows now, the settings tab shown
    /// in its window at it, that it does, and the one of the page that
    /// showed before that it no longer does.
    pub(super) fn settle_page(&mut self) {
        let shows = self.workspace.settings_tab().is_some_and(|(tab, shown)| {
            self.workspace
                .window(shown)
                .is_some_and(|shown| shown.active_tab() == tab)
        });
        let page = self
            .settings
            .pages
            .get(self.settings.chosen)
            .filter(|_| shows);
        let same = match (&self.settings.told, page) {
            (Some((plugin, id)), Some(page)) => *plugin == page.plugin && *id == page.id,
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some((plugin, id)) = self.settings.told.take() {
            self.plugins.page_shown(&plugin, &id, false);
        }
        if let Some(page) = page {
            self.plugins.page_shown(&page.plugin, &page.id, true);
            self.settings.told = Some((page.plugin.clone(), page.id.clone()));
        }
    }

    /// Opens `window`'s view of the settings tab, under its panels, as the
    /// tab shows, its pages read anew if the plugins changed, or closes it
    /// as the tab hides or closes.
    pub(super) fn settle_settings(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let shows = self
            .workspace
            .window(state.window)
            .is_some_and(|shown| self.workspace.is_settings(shown.active_tab()));
        if shows {
            self.refresh_pages();
        }
        let Some(state) = self.windows.get(&window) else {
            return;
        };
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
            self.settle_page();
            self.request_frames(platform);
        }
    }

    /// The settings tab's tree: its sidebar of pages, each a row of its
    /// icon and its title, the chosen one selected, and the room beside it
    /// that the chosen page's view takes; of no page, a text there that
    /// tells that none is there yet.
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
        let empty = pages.is_empty().then(|| {
            Node::text(self.texts.text("settings-tab-empty")).style(Style {
                text_size: Some(TextSize::Small),
                color: Some(Token::from("text.secondary")),
                ..Style::default()
            })
        });
        let page = Node::container(empty.into_iter().collect())
            .key(PAGE)
            .role(NodeRole::Group)
            .layout(Layout {
                grow: 1.0,
                height: Length::Fraction(1.0),
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

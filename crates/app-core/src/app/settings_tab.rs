//! The settings: a window of their own, one at most in the app, of a tab
//! of no pane, which shows the settings' pages in the room that a tab's
//! panes take, as a view of the app's own under the window's panels: the
//! pages that Lokyr's own plugins' manifests have as tabs across the top,
//! and the chosen page below them, its plugin's view over the app's, of a
//! sidebar of its own where it needs one; opened by the commands, or the
//! window open brought forward, and closed as a window closes. The plugin
//! of the page shown hears it shown, and hidden as another page shows or
//! the window closes.

use std::sync::Arc;

use lokyr_kernel::plugins::host::PluginInfo;
use lokyr_kernel::plugins::ui::{
    Anchor as PanelAnchor, Corner as PanelCorner, PanelId, PanelOptions, Role as PanelRole,
    UiEvent, ViewId,
};
use lokyr_kernel::plugins::view::{
    Align, Direction, Edges, Justify, Layout, Length, Node, Role as NodeRole, Style, Tab, TextSize,
    Token, UiEventKind,
};
use lokyr_platform_core::{Platform, Size, WindowAttributes, WindowId};
use lokyr_ui_core::panel::{Anchor, Dismiss, Options, Role};

use super::App;
use super::chrome::{OVERFLOW, PanelPlace, SlotView, free_access};
use crate::drawing::Backend;

/// The id of the settings tab's view among the app's own views.
pub(super) const SETTINGS: &str = "settings-tab";

/// The size of the settings' window of their own, in points, as macOS's
/// System Settings opens.
const WINDOW: Size = Size {
    width: 780.0,
    height: 560.0,
};

/// The room about the settings' tabs' box, in points.
const PADDING: f32 = 12.0;

/// The key of the view of tabs of the pages.
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
/// read of, and the page that its plugin was told shows, of the run of
/// the plugin told.
#[derive(Debug, Default)]
pub(crate) struct SettingsTab {
    pub(crate) pages: Vec<Page>,
    pub(crate) chosen: usize,
    pub(crate) revision: u64,
    read: Option<(Arc<[PluginInfo]>, String)>,
    told: Option<(String, String, u64)>,
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
    /// Opens the settings in a window of their own, of the size of macOS's
    /// System Settings, which neither a number nor the restore takes; or
    /// brings the one open forward.
    pub(super) fn open_settings_tab(&mut self, platform: &mut dyn Platform) {
        self.refresh_pages();
        let title = self.texts.text("settings-tab-title");
        if let Some((_, shown)) = self.workspace.settings_tab() {
            self.workspace.open_settings_window(&title);
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
        let attributes = WindowAttributes {
            title: title.clone(),
            size: WINDOW,
            remember: false,
            restore: false,
            restore_size: false,
            frame: None,
        };
        match platform.create_window(&attributes) {
            Ok(window) => {
                let shown = self.workspace.open_settings_window(&title);
                self.attach(platform, window, shown, None);
                platform.focus_window(window);
            }
            Err(error) => eprintln!("lokyr: {error}"),
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
    /// in its window at it, that it does, once the plugin is active, and
    /// the one of the page that showed before that it no longer does.
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
        // Told to the run of the plugin that is active: one started again
        // after a fault hears it anew.
        let same = match (&self.settings.told, page) {
            (Some((plugin, id, run)), Some(page)) => {
                *plugin == page.plugin
                    && *id == page.id
                    && self.plugins.activated_run(plugin) == Some(*run)
            }
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some((plugin, id, _)) = self.settings.told.take() {
            self.plugins.page_shown(&plugin, &id, false);
        }
        // A plugin that its page's first render activates hears it shown
        // once it is active, at a later turn, as it would hear nothing
        // before.
        if let Some(page) = page
            && let Some(run) = self.plugins.activated_run(&page.plugin)
        {
            self.plugins.page_shown(&page.plugin, &page.id, true);
            self.settings.told = Some((page.plugin.clone(), page.id.clone(), run));
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

    /// An event of the settings' nodes: a page chosen among their tabs, by
    /// a click or the keys, shows below them.
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

    /// The settings' tree: their pages as tabs across the top, of their
    /// icons and titles, the chosen one's, and below them, in the tabs'
    /// box, the room that the chosen page's view takes, where its plugin
    /// gives the page a sidebar of its own if it needs one; of no page, a
    /// text that tells that none is there yet.
    pub(super) fn settings_tree(&self) -> Node {
        let pages = &self.settings.pages;
        let whole = Layout {
            width: Length::Fraction(1.0),
            height: Length::Fraction(1.0),
            ..Layout::default()
        };
        if pages.is_empty() {
            let empty = Node::text(self.texts.text("settings-tab-empty")).style(Style {
                text_size: Some(TextSize::Small),
                color: Some(Token::from("text.secondary")),
                ..Style::default()
            });
            return Node::container(vec![empty])
                .key(PAGE)
                .role(NodeRole::Group)
                .layout(Layout {
                    justify: Justify::Center,
                    align: Align::Center,
                    ..whole
                });
        }
        let tabs = pages
            .iter()
            .map(|page| Tab {
                label: page.title.clone(),
                icon: page.icon.clone(),
                disabled: false,
            })
            .collect();
        let page = Node::container(Vec::new())
            .key(PAGE)
            .role(NodeRole::Group)
            .layout(Layout {
                grow: 1.0,
                width: Length::Fraction(1.0),
                ..Layout::default()
            });
        let control = Node::tabs(tabs, self.settings.chosen, page)
            .key(PAGES)
            .label(self.texts.text("settings-tab-title"))
            .layout(whole);
        Node::container(vec![control])
            .key("settings")
            .layout(Layout {
                direction: Direction::Column,
                padding: Edges::all(PADDING),
                ..whole
            })
    }
}

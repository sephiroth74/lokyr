//! The windows and the tabs that the system asks for at folders: a folder
//! dropped on the app's icon, and the file manager's services on the
//! folders that it selected, a window or a tab at each.

use std::path::{Path, PathBuf};

use lokyr_kernel::workspace::Environment;
use lokyr_platform_core::{Platform, WindowId, WindowSize};

use super::{App, WINDOW_SIZE};
use crate::drawing::Backend;

impl<B: Backend> App<B> {
    /// Opens a window, or a tab of the window in front, at each of
    /// `folders`, or at the folder that holds a file of them; the last
    /// window that one opened in comes forward, with the app, as the file
    /// manager that asked keeps the focus otherwise. A path that is not
    /// there opens nothing, as the log tells.
    pub(super) fn open_folders(
        &mut self,
        platform: &mut dyn Platform,
        folders: Vec<PathBuf>,
        tabs: bool,
    ) {
        let mut shown = None;
        for path in folders {
            let Some(folder) = folder_of(&path) else {
                eprintln!(
                    "lokyr: nothing opens at {}, as it is no folder, nor a file in one",
                    path.display()
                );
                continue;
            };
            let opened = if tabs {
                self.open_tab_at(platform, folder)
            } else {
                let program = self.program.clone();
                self.open_window_running(platform, &program, None, Some(folder))
            };
            shown = opened.or(shown);
        }
        if let Some(window) = shown {
            platform.focus_window(window);
        }
    }

    /// Opens a tab at `folder` after the tabs of the window in front, or of
    /// another, the quick terminal's and the man pages' aside, which shows
    /// it; or a window, without one. The window that it opened in.
    fn open_tab_at(&mut self, platform: &mut dyn Platform, folder: PathBuf) -> Option<WindowId> {
        let takes = |app: &Self, window: &WindowId| {
            app.windows
                .get(window)
                .is_some_and(|open| open.page.is_none())
                && !app.quick.contains_key(window)
        };
        let front = self
            .front
            .or_else(|| platform.focused_window())
            .filter(|window| takes(self, window))
            .or_else(|| {
                self.windows
                    .keys()
                    .copied()
                    .find(|window| takes(self, window))
            });
        let program = self.program.clone();
        let Some(window) = front else {
            return self.open_window_running(platform, &program, None, Some(folder));
        };
        let shown = self.windows[&window].window;
        let size = platform
            .window_size(window)
            .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
        let grid = self.grid(size);
        let colors = self.colors(platform.appearance());
        let environment = Environment {
            directory: Some(folder),
            ..self.environment.clone()
        };
        let pane = self.spawn_pane(platform, &program, &environment, grid, &colors);
        let index = self
            .workspace
            .window(shown)
            .map_or(0, |open| open.tabs().len());
        let tab = self.workspace.open_tab(shown, pane, index)?;
        self.workspace.activate_tab(tab);
        Some(window)
    }
}

/// The folder at `path`: itself, or the one that holds the file there;
/// `None` for a path that is not there.
fn folder_of(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        Some(path.to_path_buf())
    } else if path.is_file() {
        path.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

//! The windows kept as the app quits, which open again as it starts, as
//! `[app] restore-windows` says: their frames, tabs, splits and the
//! directories of their panes, whose programs start anew.

use std::path::Path;

use lokyr_kernel::commands::Code;
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::state::windows::{self, Kept};
use lokyr_kernel::workspace::{self, Environment, PaneId};
use lokyr_platform_core::{Platform, ScreenRect, WindowAttributes, WindowId, WindowSize};

use super::{App, TITLE, WINDOW_SIZE, spawn};
use crate::drawing::Backend;

impl<B: Backend> App<B> {
    /// Whether the windows open again as the app starts, and are kept as
    /// it quits: as the config says, of the system's setting; never for a
    /// bench, whose window is its own.
    fn restores_windows(&self, platform: &dyn Platform) -> bool {
        self.startup_bench.is_none()
            && self.latency_bench.is_none()
            && self.output_bench.is_none()
            && self
                .config
                .app
                .restore_windows
                .restores(platform.keeps_windows())
    }

    /// Keeps the windows open now, in the order of their numbers, for the
    /// next start, or, as the config keeps none, forgets those kept. A
    /// quick window and a man page's window are not kept.
    pub(super) fn keep_windows(&self, platform: &dyn Platform) {
        let Some(state) = &self.state else {
            return;
        };
        if self.restores_windows(platform) {
            let mut shown: Vec<(u32, workspace::WindowId, Option<windows::Frame>)> = self
                .windows
                .iter()
                .filter(|(id, open)| open.page.is_none() && !self.quick.contains_key(id))
                .filter_map(|(&id, open)| {
                    let number = self.workspace.window(open.window)?.number();
                    let frame = platform.windowed_frame(id).map(|frame| windows::Frame {
                        x: frame.x,
                        y: frame.y,
                        width: frame.width,
                        height: frame.height,
                    });
                    Some((number.unwrap_or(u32::MAX), open.window, frame))
                })
                .collect();
            shown.sort_by_key(|&(number, ..)| number);
            let shown: Vec<_> = shown
                .into_iter()
                .map(|(_, window, frame)| (window, frame))
                .collect();
            let front = self
                .front
                .or_else(|| platform.focused_window())
                .and_then(|front| self.windows.get(&front))
                .map(|open| open.window);
            let host = lokyr_pty::host_name();
            // The directory that its shell told last, of this machine, as a
            // new pane's; its program's otherwise.
            let kept = Kept::of(&self.workspace, &shown, front, |pane| {
                pane.told_directory(&host)
                    .or_else(|| pane.working_directory())
            });
            if !kept.windows.is_empty() {
                if let Err(error) = windows::write(state, &kept) {
                    eprintln!("lokyr: the windows cannot be kept for the next start: {error}");
                }
                return;
            }
        }
        if let Err(error) = windows::forget(state) {
            eprintln!("lokyr: the windows kept for the next start cannot be forgotten: {error}");
        }
    }

    /// Opens the windows kept as the app last quit, as the config says,
    /// the one that was in front taking the focus; whether it opened one.
    /// A file of the windows that is of a newer version, or that does not
    /// read, is left aside, as a notice tells.
    pub(super) fn restore_windows(&mut self, platform: &mut dyn Platform) -> bool {
        let Some(state) = self.state.clone() else {
            return false;
        };
        if !self.restores_windows(platform) {
            return false;
        }
        let kept = match windows::read(&state) {
            Ok(Some(kept)) => kept,
            Ok(None) => return false,
            Err(unread) => {
                let text = format!(
                    "the windows of the last run do not open again, as their file in {} is left aside: {unread}",
                    state.display()
                );
                self.report(platform, Diagnostic::new(Code::Failed, text, About::Other));
                return false;
            }
        };
        let mut front = None;
        for (index, window) in kept.windows.iter().enumerate() {
            let opened = self.restore_window(platform, window);
            if kept.front == Some(index) {
                front = opened;
            }
        }
        if self.windows.is_empty() {
            return false;
        }
        self.opened_one = true;
        if let Some(front) = front {
            platform.focus_window(front);
        }
        true
    }

    /// Opens the window `kept`, at its frame, on a screen that is there, of
    /// its tabs and splits, each pane in its directory, or, where that is
    /// not one, in the home directory.
    fn restore_window(
        &mut self,
        platform: &mut dyn Platform,
        kept: &windows::Window,
    ) -> Option<WindowId> {
        let window_config = &self.config.window;
        let cells = (
            u32::from(window_config.columns.count()),
            u32::from(window_config.rows.count()),
        );
        let remember = window_config.remember_frame;
        let frame = kept.frame.and_then(usable);
        let size = self.size_of(platform.screen_scale(), cells);
        let attributes = WindowAttributes {
            title: TITLE.to_owned(),
            size,
            remember,
            restore: false,
            restore_size: false,
            frame,
        };
        let window = match platform.create_window(&attributes) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("lokyr: {error}");
                return None;
            }
        };
        let size = platform
            .window_size(window)
            .unwrap_or_else(|| WindowSize::new(WINDOW_SIZE, 1.0));
        let grid = self.grid(size);
        let colors = self.colors(platform.appearance());
        let settings = self.settings();
        let waker = platform.waker();
        let program = self.program.clone();
        let (environment, texts, config) = (&self.environment, &self.texts, &self.config);
        let wakes = &mut self.pane_wakes;
        let shown = kept.open(&mut self.workspace, |directory| {
            let directory = directory.filter(|directory| {
                let there = directory.is_dir();
                if !there {
                    eprintln!(
                        "lokyr: a pane kept in {} opens in the home directory, as that is not a directory that it can enter",
                        directory.display()
                    );
                }
                there
            });
            let environment = Environment {
                directory: directory.map(Path::to_path_buf),
                ..environment.clone()
            };
            spawn::Start {
                program: &program,
                environment: &environment,
                settings: &settings,
                colors: &colors,
                texts,
                config,
                waker: waker.clone(),
                wakes,
            }
            .pane(PaneId::new(), grid)
        });
        let Some(shown) = shown else {
            platform.close_window(window);
            return None;
        };
        self.attach(platform, window, shown, None);
        self.show_title(platform, window);
        Some(window)
    }
}

/// The frame of `kept` on the screens, if it is one that a window can
/// take: of finite numbers, and of a size.
fn usable(kept: windows::Frame) -> Option<ScreenRect> {
    let frame = ScreenRect {
        x: kept.x,
        y: kept.y,
        width: kept.width,
        height: kept.height,
    };
    let finite = [frame.x, frame.y, frame.width, frame.height]
        .iter()
        .all(|number| number.is_finite());
    (finite && frame.width >= 1.0 && frame.height >= 1.0).then_some(frame)
}

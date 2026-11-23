//! The windows kept as the app quits, which open again as it starts, as
//! `[app] restore-windows` says: their frames, tabs, splits and the
//! directories of their panes, whose programs start anew; and, as
//! `[app] restore-scrollback` says, what each pane showed, its scrollback
//! within `[scrollback] limit`, which it shows again. Every session comes
//! back, the active one shown: the windows of the others open as each first
//! shows, so that the start pays for the windows that show alone.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use lokyr_kernel::commands::Code;
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::{Arg, Texts};
use lokyr_kernel::plugins::host::LocalTime;
use lokyr_kernel::state::windows::{self, Kept};
use lokyr_kernel::state::{self, scrollback};
use lokyr_kernel::workspace::{self, Environment, PaneId, Restored};
use lokyr_platform_core::{Platform, ScreenRect, WindowAttributes, WindowId, WindowSize};

use super::{App, TITLE, WINDOW_SIZE, spawn};
use crate::drawing::Backend;

impl<B: Backend> App<B> {
    /// Whether the windows open again as the app starts, and are kept as
    /// it quits: as the config says, of the system's setting; never for a
    /// bench, whose window is its own.
    fn restores_windows(&self, platform: &dyn Platform) -> bool {
        !self.benched
            && self
                .config
                .app
                .restore_windows
                .restores(platform.keeps_windows())
    }

    /// Keeps the windows open now, in the order of their numbers, for the
    /// next start, and, as the config says, each pane's scrollback beside
    /// them, of no file for a pane that shows nothing; or, as the config
    /// keeps no window, forgets those kept and their panes' scrollback. A
    /// quick window and a man page's window are not kept. A bench's app,
    /// of the user's state, neither keeps nor forgets the user's windows.
    pub(super) fn keep_windows(&self, platform: &dyn Platform) {
        let Some(state) = &self.state else {
            return;
        };
        if self.benched {
            return;
        }
        // The folders that the shells told, which the sessions' switcher
        // lists at the next start too.
        if let Err(error) = state::keep_recent_folders(state, self.workspace.recent_folders()) {
            eprintln!("lokyr: the recent folders cannot be kept for the next start: {error}");
        }
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
            let keeps_scrollback = self.config.app.restore_scrollback;
            let bound = self.config.scrollback.limit.bytes();
            let mut archives: Vec<(String, Vec<u8>)> = Vec::new();
            // The directory that its shell told last, of this machine, as a
            // new pane's; its program's otherwise.
            let mut kept = Kept::of(
                &self.workspace,
                &shown,
                front,
                |pane| {
                    pane.told_directory(&host)
                        .or_else(|| pane.working_directory())
                },
                |pane, index| {
                    if !keeps_scrollback {
                        return None;
                    }
                    let bytes = pane.terminal()?.archive(bound);
                    if bytes.is_empty() {
                        return None;
                    }
                    let name = scrollback::name(index);
                    archives.push((name.clone(), bytes));
                    Some(name)
                },
            );
            self.keep_pending(state, &mut kept, &mut archives);
            if !kept.windows.is_empty() {
                // The panes' files first, so that the windows name none that
                // is not there.
                if let Err(error) = scrollback::write(state, &archives) {
                    eprintln!(
                        "lokyr: the panes' scrollback cannot be kept for the next start: {error}"
                    );
                }
                if let Err(error) = windows::write(state, &kept) {
                    eprintln!("lokyr: the windows cannot be kept for the next start: {error}");
                }
                return;
            }
        }
        if let Err(error) = windows::forget(state) {
            eprintln!("lokyr: the windows kept for the next start cannot be forgotten: {error}");
        }
        if let Err(error) = scrollback::forget(state) {
            eprintln!(
                "lokyr: the panes' scrollback kept for the next start cannot be forgotten: {error}"
            );
        }
    }

    /// Appends to `kept` the windows of the sessions that have not shown
    /// since the app started, as they were kept, of the files of their
    /// panes' scrollback, read for `archives` of their new names, after
    /// those of the panes kept before them, as the config keeps any.
    fn keep_pending(&self, state: &Path, kept: &mut Kept, archives: &mut Vec<(String, Vec<u8>)>) {
        let keeps_scrollback = self.config.app.restore_scrollback;
        let mut index = kept.panes();
        let mut pending = Vec::new();
        for session in self.workspace.sessions() {
            let Some(windows) = self.pending_windows.get(&session.id()) else {
                continue;
            };
            for window in windows {
                let mut window = window.clone();
                window.rename_scrollbacks(|name| {
                    let at = index;
                    index += 1;
                    let rows = name
                        .filter(|_| keeps_scrollback)
                        .and_then(|name| scrollback::take(state, &name).ok().flatten())?;
                    let name = scrollback::name(at);
                    archives.push((name.clone(), rows.rows));
                    Some(name)
                });
                pending.push((session.id(), window));
            }
        }
        if !pending.is_empty() {
            kept.append(&self.workspace, pending);
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
        // The folders that the shells told in the runs before.
        if !self.benched {
            self.workspace
                .set_recent_folders(state::recent_folders(&state));
        }
        // What the panes showed, which no pane shows again, goes at once.
        if !self.benched
            && !(self.restores_windows(platform) && self.config.app.restore_scrollback)
            && let Err(error) = scrollback::forget(&state)
        {
            eprintln!("lokyr: the panes' scrollback kept cannot be forgotten: {error}");
        }
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
                self.report(
                    platform,
                    Diagnostic::new(Code::Failed, text, About::KeptWindows),
                );
                return false;
            }
        };
        // The sessions, the first the one that the app starts in, the
        // others after it in the order of their use.
        let first = self.workspace.active_session();
        let sessions: Vec<workspace::SessionId> = kept
            .sessions_to_open()
            .into_iter()
            .enumerate()
            .map(|(index, session)| {
                let id = if index == 0 {
                    self.workspace.rename_session(first, &session.name);
                    first
                } else {
                    self.workspace.add_session(&session.name)
                };
                self.workspace.set_session_folder(id, session.folder);
                id
            })
            .collect();
        let mut front = None;
        for (index, window) in kept.windows.iter().enumerate() {
            // A hidden session's windows open as it first shows.
            let session = sessions[window.session_index(sessions.len())];
            if session != first {
                self.pending_windows
                    .entry(session)
                    .or_default()
                    .push(window.clone());
                continue;
            }
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
    pub(super) fn restore_window(
        &mut self,
        platform: &mut dyn Platform,
        kept: &windows::Window,
    ) -> Option<WindowId> {
        // A window of no tab opens nothing, not even a window to close.
        if kept.tabs.is_empty() {
            return None;
        }
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
        // The panes' scrollback kept, as the config shows it again.
        let state = self
            .state
            .as_deref()
            .filter(|_| config.app.restore_scrollback);
        let offset = crate::clock::local_time().offset;
        // The commands of a layout's panes whose folders are not there,
        // which run nowhere else.
        let mut unrun: Vec<(String, std::path::PathBuf)> = Vec::new();
        let shown = kept.open(&mut self.workspace, |pane| {
            let restored = state
                .zip(pane.scrollback)
                .and_then(|(state, name)| restored(state, name, grid.columns, texts, offset));
            let directory = pane.directory.filter(|directory| {
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
                restored,
                ..environment.clone()
            };
            let new = spawn::Start {
                program: &program,
                environment: &environment,
                settings: &settings,
                colors: &colors,
                texts,
                config,
                waker: waker.clone(),
                wakes,
            }
            .pane(PaneId::new(), grid);
            // A layout's pane's command, as though typed, its shell staying
            // once it ends; in its folder alone, as a command of a folder
            // runs nowhere else.
            match (pane.command, pane.directory) {
                (Some(command), Some(folder)) if directory.is_none() => {
                    unrun.push((command.to_owned(), folder.to_owned()));
                }
                (Some(command), _) => new.send_text(&format!("{command}\r")),
                (None, _) => {}
            }
            new
        });
        if !unrun.is_empty() {
            let told: Vec<String> = unrun
                .iter()
                .map(|(command, folder)| format!("`{command}` in {}", folder.display()))
                .collect();
            let text = format!(
                "a layout's commands do not run, as their folders are not there: {}",
                told.join(", ")
            );
            self.report(platform, Diagnostic::new(Code::Failed, text, About::Other));
        }
        let Some(shown) = shown else {
            platform.close_window(window);
            return None;
        };
        self.attach(platform, window, shown, None);
        self.show_title(platform, window);
        Some(window)
    }
}

/// What the pane of the scrollback's file `name` in `state` shows again, as
/// it opens: its rows, which the pane's own thread reads, as reading a
/// file of a full pane's takes milliseconds that the first frame need not
/// wait for, and the line under them of when they were kept, across
/// `columns`, of the local time zone of `offset`; none of no such file. A
/// file left aside shows nothing, as the log tells.
fn restored(
    state: &Path,
    name: &str,
    columns: u16,
    texts: &Texts,
    offset: i32,
) -> Option<Restored> {
    let time = scrollback::time(state, name)?;
    let (state, name) = (state.to_path_buf(), name.to_owned());
    Some(Restored {
        rows: Arc::new(move || match scrollback::take(&state, &name) {
            Ok(kept) => kept.map(|kept| kept.rows),
            Err(unread) => {
                eprintln!(
                    "lokyr: a pane's scrollback does not show again, as its file is left aside: {unread}"
                );
                None
            }
        }),
        line: restored_line(texts, time, offset, columns),
    })
}

/// The line under a pane's restored scrollback: `── Restored from <date and
/// time>`, of `time` in the local time zone of `offset`, as 2026-11-22
/// 09:31, then a rule to `columns`.
fn restored_line(texts: &Texts, time: SystemTime, offset: i32, columns: u16) -> String {
    let millis = time.duration_since(UNIX_EPOCH).map_or(0, |since| {
        u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
    });
    let at = LocalTime {
        time: millis,
        offset,
    }
    .civil(millis);
    let when = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        at.year, at.month, at.day, at.hour, at.minute
    );
    let mut line = format!(
        "── {} ",
        texts.text_with("scrollback-restored", &[("when", Arg::Text(&when))])
    );
    let shown = line.chars().count();
    line.extend(std::iter::repeat_n(
        '─',
        usize::from(columns).saturating_sub(shown),
    ));
    line
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

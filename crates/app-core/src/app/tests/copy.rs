//! A mode of the keyboard's over the scrollback in the app: a plugin's
//! cursor in place of the terminal's, moved, its selection of lines from
//! where it started, a scroll to its row, and the terminal's cursor back.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::commands::{Category, Command};
use lokyr_kernel::plugins::host::{PaneId, Scroll, SelectionKind};
use lokyr_kernel::plugins::{Cx, Plugin};

use super::*;

/// Where the plugin's cursor started and where it is.
#[derive(Default)]
struct Copying {
    pane: Option<PaneId>,
    start: Option<(u16, i64)>,
    at: Option<(u16, i64)>,
}

/// A plugin of a cursor of its own: `copier.start` shows it at the
/// terminal's cursor, `copier.up` moves it up a row and selects the lines
/// from where it started, `copier.top` moves it 100 rows up, into the
/// scrollback, its row at the view's top, and `copier.end` gives the
/// terminal's cursor back.
struct Copier(Rc<RefCell<Copying>>);

impl Plugin for Copier {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["copier.start", "copier.up", "copier.top", "copier.end"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let mut copying = self.0.borrow_mut();
        if name == "copier.start" {
            let pane = cx.focused_pane().expect("a focused pane");
            let at = cx.cursor_cell(pane)?;
            copying.pane = Some(pane);
            (copying.start, copying.at) = (at, at);
            return cx.show_cursor(pane, at);
        }
        let (Some(pane), Some(start), Some((column, row))) =
            (copying.pane, copying.start, copying.at)
        else {
            return Ok(());
        };
        match name {
            "copier.up" => {
                let at = (column, row - 1);
                copying.at = Some(at);
                cx.show_cursor(pane, Some(at))?;
                cx.select_range(pane, start, at, SelectionKind::Lines)
            }
            "copier.top" => {
                let at = (column, row - 100);
                copying.at = Some(at);
                cx.show_cursor(pane, Some(at))?;
                cx.scroll(pane, Scroll::Row(at.1))
            }
            _ => cx.show_cursor(pane, None),
        }
    }
}

/// The cursor that the window's last frame shows: its column and its row of
/// the view, if it shows.
fn shown_cursor(app: &App, window: WindowId) -> Option<(u16, u16)> {
    let cursor = app.pane(window).snapshot().cursor();
    cursor
        .visible
        .then_some((cursor.position.x, cursor.position.y))
}

#[test]
fn a_plugin_s_cursor_moves_selects_lines_scrolls_and_gives_the_terminal_s_back() {
    // Lines past the screen, into the scrollback, then a prompt that waits.
    let program =
        shell("for n in $(seq 1 200); do echo \"line $n\"; done; printf 'ready $ '; sleep 30");
    let copying = Rc::new(RefCell::new(Copying::default()));
    let mut fake = Fake::default();
    let mut app =
        app(unpadded(), program).with_plugin("lokyr/copier", Box::new(Copier(Rc::clone(&copying))));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready $"));
    let command = |fake: &mut Fake, app: &mut App, name: &str| {
        fake.send_app(app, AppEvent::Command(name.to_owned()));
    };
    let terminal = shown_cursor(&app, window).expect("the terminal's cursor");
    // At the terminal's cursor, its own in place, as the terminal's is.
    command(&mut fake, &mut app, "copier.start");
    run_until(&mut fake, &mut app, |_, app| {
        !app.pane(window).snapshot().cursor().blink && shown_cursor(app, window) == Some(terminal)
    });
    // Up three rows, of the lines from the prompt's selected whole.
    for _ in 0..3 {
        command(&mut fake, &mut app, "copier.up");
    }
    let up = (terminal.0, terminal.1 - 3);
    run_until(&mut fake, &mut app, |_, app| {
        shown_cursor(app, window) == Some(up)
    });
    let selected = app
        .workspace
        .pane(app.windows[&window].views[0].pane)
        .unwrap()
        .selection_text()
        .expect("a selection");
    assert_eq!(
        selected, "line 198\nline 199\nline 200\nready $",
        "{selected:?}"
    );
    // 100 rows up, in the scrollback, its row at the view's top.
    command(&mut fake, &mut app, "copier.top");
    run_until(&mut fake, &mut app, |_, app| {
        shown_cursor(app, window) == Some((up.0, 0))
    });
    let at = copying.borrow().at.unwrap();
    assert_eq!(
        app.pane(window)
            .snapshot()
            .origin()
            .map(|origin| origin.top),
        Some(at.1)
    );
    // The terminal's cursor back, below the view, which hides it.
    command(&mut fake, &mut app, "copier.end");
    run_until(&mut fake, &mut app, |_, app| {
        shown_cursor(app, window).is_none()
    });
}

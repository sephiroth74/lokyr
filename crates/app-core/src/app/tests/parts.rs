//! The tests of what a plugin asks of the app for its tabs and panes: a
//! close that asks the user first, a title of a tab's own, and a text that
//! the user is asked for, whose answer comes to the plugin.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

/// The answers that the plugin heard, of its questions' ids.
type Answers = Rc<RefCell<Vec<(u64, Option<String>)>>>;

/// A plugin that opens a tab, closes the focused tab or pane asking
/// first, names the focused tab, and asks for a text, keeping the answers
/// that it hears and its questions' ids.
struct Parts(Answers, Rc<RefCell<Vec<u64>>>);

impl plugins::Plugin for Parts {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in [
            "parts.tab",
            "parts.close-tab",
            "parts.close-pane",
            "parts.name",
            "parts.ask",
        ] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::Tabs,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let window = cx.focused_window().unwrap();
        let tab = cx.tabs(window)?.into_iter().find(|tab| tab.active).unwrap();
        match name {
            "parts.tab" => {
                let (tab, _) = cx.new_tab(window, None)?;
                cx.activate_tab(tab)
            }
            "parts.close-tab" => cx.close_tab(tab.id, true),
            "parts.close-pane" => cx.close_pane(tab.focused, true),
            "parts.name" => cx.set_tab_title(tab.id, Some("named")),
            _ => {
                let id = cx.ask_text(None, "The tab's title", "", "old")?;
                self.1.borrow_mut().push(id);
                Ok(())
            }
        }
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for event in events {
            if let plugins::Event::Answered { id, text } = event {
                self.0.borrow_mut().push((*id, text.clone()));
            }
        }
        Ok(())
    }
}

/// The app of the plugin, each pane running `program`, its window
/// focused once it shows `ready`; the answers that the plugin hears, and
/// its questions' ids.
fn with_parts(program: Program) -> (Fake, App, WindowId, Answers, Rc<RefCell<Vec<u64>>>) {
    let answers = Answers::default();
    let asked = Rc::new(RefCell::new(Vec::new()));
    let mut fake = Fake::default();
    let plugin = Parts(Rc::clone(&answers), Rc::clone(&asked));
    let mut app = app(unpadded(), program).with_plugin("parts", Box::new(plugin));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window, answers, asked)
}

/// Runs `command` of the plugin's.
fn run(fake: &mut Fake, app: &mut App, command: &str) {
    fake.send_app(app, AppEvent::Command(command.to_owned()));
}

/// The tabs of `window`, as the workspace has them.
fn tabs(app: &App, window: WindowId) -> usize {
    let shown = app.windows[&window].window;
    app.workspace
        .window(shown)
        .map_or(0, |shown| shown.tabs().len())
}

#[test]
fn a_plugin_s_close_of_a_running_tab_asks_first_as_a_window_s_does() {
    let (mut fake, mut app, window, ..) = with_parts(shell("echo ready; exec sleep 30"));
    run(&mut fake, &mut app, "parts.tab");
    assert_eq!(tabs(&app, window), 2);
    let pane = app.focused(window).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(pane).and_then(Pane::running).as_deref() == Some("sleep")
    });
    // Cancelled, the tab stays; asked again meanwhile, one question.
    run(&mut fake, &mut app, "parts.close-tab");
    run(&mut fake, &mut app, "parts.close-tab");
    let [question] = fake.alerts() else {
        panic!("one question: {:?}", fake.alerts());
    };
    assert_eq!(
        question.alert.message,
        app.texts.text("close-tab-running-message")
    );
    assert_eq!(question.window, Some(window));
    let id = question.id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(tabs(&app, window), 2);
    // Confirmed, it closes, and the window shows the other.
    run(&mut fake, &mut app, "parts.close-tab");
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    assert_eq!(tabs(&app, window), 1);
    assert!(app.workspace.pane(pane).is_none());
    assert_eq!(fake.windows().count(), 1);
    // The last pane's close closes the window, which asks as its own
    // close does.
    run(&mut fake, &mut app, "parts.close-pane");
    assert_eq!(
        fake.alerts()[0].alert.message,
        app.texts.text("close-running-message")
    );
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    assert_eq!(fake.windows().count(), 0);
}

#[test]
fn a_shell_at_its_prompt_closes_at_once_and_a_tab_takes_a_title_of_its_own() {
    let (mut fake, mut app, window, ..) = with_parts(idle());
    run(&mut fake, &mut app, "parts.tab");
    run(&mut fake, &mut app, "parts.name");
    // The window's title is the tab's own.
    assert!(
        fake.window(window).unwrap().title.starts_with("named"),
        "{}",
        fake.window(window).unwrap().title
    );
    run(&mut fake, &mut app, "parts.close-tab");
    assert!(fake.alerts().is_empty());
    assert_eq!(tabs(&app, window), 1);
}

#[test]
fn a_text_that_a_plugin_asks_for_comes_to_it_or_none() {
    let (mut fake, mut app, window, answers, asked) = with_parts(idle());
    run(&mut fake, &mut app, "parts.ask");
    let [question] = fake.alerts() else {
        panic!("one question: {:?}", fake.alerts());
    };
    assert_eq!(question.alert.message, "The tab's title");
    assert_eq!(question.alert.input.as_deref(), Some("old"));
    assert_eq!(question.window, Some(window));
    let id = question.id;
    fake.answer_alert_typed(&mut app, id, (0, false), Some("new"));
    let first = asked.borrow()[0];
    run_until(&mut fake, &mut app, |_, _| !answers.borrow().is_empty());
    assert_eq!(answers.borrow()[0], (first, Some("new".to_owned())));
    // Cancelled, none.
    run(&mut fake, &mut app, "parts.ask");
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 1);
    run_until(&mut fake, &mut app, |_, _| answers.borrow().len() == 2);
    assert_eq!(answers.borrow()[1], (asked.borrow()[1], None));
    // A question whose window closes has no answer.
    run(&mut fake, &mut app, "parts.ask");
    app.close(&mut fake, window);
    run_until(&mut fake, &mut app, |_, _| answers.borrow().len() == 3);
    assert_eq!(answers.borrow()[2], (asked.borrow()[2], None));
}

#[test]
fn the_tabs_keys_open_select_move_and_close_tabs() {
    let (mut fake, mut app, window, ..) = with_parts(idle());
    let active = |app: &App| {
        let shown = app.workspace.window(app.windows[&window].window).unwrap();
        let at = shown
            .tabs()
            .iter()
            .position(|&tab| tab == shown.active_tab());
        (shown.tabs().to_vec(), at)
    };
    let press = |fake: &mut Fake, app: &mut App, code, text: &str, mods| {
        fake.send(app, window, key_event(code, text, mods, true));
    };
    press(&mut fake, &mut app, KeyCode::KeyT, "t", CMD);
    press(&mut fake, &mut app, KeyCode::KeyT, "t", CMD);
    let (opened, at) = active(&app);
    assert_eq!((opened.len(), at), (3, Some(2)));
    // By their numbers, then round.
    press(&mut fake, &mut app, KeyCode::Digit1, "1", CMD);
    assert_eq!(active(&app).1, Some(0));
    let shifted = Modifiers { shift: true, ..CMD };
    press(&mut fake, &mut app, KeyCode::BracketLeft, "[", shifted);
    assert_eq!(active(&app).1, Some(2));
    press(&mut fake, &mut app, KeyCode::Tab, "\t", CTRL);
    assert_eq!(active(&app).1, Some(0));
    // The first moves a place right.
    press(&mut fake, &mut app, KeyCode::ArrowRight, "", shifted);
    let (moved, at) = active(&app);
    assert_eq!((moved[1], at), (opened[0], Some(1)));
    // Command-W closes the tab, of a shell at its prompt, and the window
    // with its last.
    press(&mut fake, &mut app, KeyCode::KeyW, "w", CMD);
    assert_eq!(active(&app).0.len(), 2);
    press(&mut fake, &mut app, KeyCode::KeyW, "w", CMD);
    press(&mut fake, &mut app, KeyCode::KeyW, "w", CMD);
    assert_eq!(fake.windows().count(), 0);
}

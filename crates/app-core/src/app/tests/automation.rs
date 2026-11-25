//! The automation's requests of scripts and shortcuts, as the platform's
//! bridges hand them, through the automation's plugin, and their answers
//! back to the platform.

use std::time::Duration;

use lokyr_platform_core::automation as platform;

use super::*;

/// The app that asks, in the tests.
const CLIENT: &str = "com.acme.script";

/// Hands the app the request `request` of `id`, of [`CLIENT`], and waits
/// for its answer.
fn ask(fake: &mut Fake, app: &mut App, id: u64, request: platform::Request) -> platform::Answer {
    send(fake, app, id, request);
    answered(fake, app, id)
}

/// Hands the app the request `request` of `id`, of [`CLIENT`].
fn send(fake: &mut Fake, app: &mut App, id: u64, request: platform::Request) {
    fake.send_app(
        app,
        AppEvent::Automation {
            id,
            client: CLIENT.to_owned(),
            request,
        },
    );
}

/// The answer of `id`, once it comes.
fn answered(fake: &mut Fake, app: &mut App, id: u64) -> platform::Answer {
    let answer = |fake: &Fake| {
        fake.automation_answers()
            .iter()
            .find(|(answered, _)| *answered == id)
            .map(|(_, answer)| answer.clone())
    };
    run_until(fake, app, |fake, _| answer(fake).is_some());
    answer(fake).unwrap()
}

#[test]
fn a_script_s_request_opens_a_tab_whose_ids_its_answer_tells() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    let request = platform::Request::NewTab {
        window: None,
        folder: None,
        command: None,
    };
    let platform::Answer::Opened { tab, pane, .. } = ask(&mut fake, &mut app, 1, request) else {
        panic!("{:?}", fake.automation_answers());
    };
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    assert_eq!(shown.tabs().len(), 2);
    assert!(shown.tabs().iter().any(|id| id.get() == tab));
    assert!(app.workspace.pane(PaneId::of(pane)).is_some());
}

#[test]
fn a_request_of_a_command_that_is_not_there_is_refused() {
    let (mut fake, mut app) = started();
    let request = platform::Request::Run {
        command: "acme.none".to_owned(),
        args: Vec::new(),
    };
    assert!(
        matches!(
            ask(&mut fake, &mut app, 2, request),
            platform::Answer::Error { code, .. } if code == Code::NotFound.number()
        ),
        "{:?}",
        fake.automation_answers()
    );
}

#[test]
fn the_ids_stay_the_same_from_a_request_to_the_next() {
    let (mut fake, mut app, window) = focused();
    let first = ask(&mut fake, &mut app, 3, platform::Request::Workspace);
    let second = ask(&mut fake, &mut app, 4, platform::Request::Workspace);
    assert_eq!(first, second);
    let platform::Answer::Workspace(windows) = first else {
        panic!("{first:?}");
    };
    let id = app.windows[&window].window.get();
    assert_eq!(
        windows.iter().map(|shown| shown.id).collect::<Vec<_>>(),
        [id]
    );
    assert!(windows[0].focused);
}

#[test]
fn a_pane_s_text_comes_once_the_user_allows_its_app() {
    let (mut fake, mut app) = started();
    let request = || platform::Request::PaneText {
        pane: None,
        selection: false,
    };
    send(&mut fake, &mut app, 5, request());
    run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
    assert!(fake.alerts()[0].alert.message.contains(CLIENT));
    assert!(fake.automation_answers().is_empty());
    // The second button allows it.
    let question = fake.alerts()[0].id;
    fake.answer_alert(&mut app, question, 1);
    let answer = answered(&mut fake, &mut app, 5);
    assert!(matches!(answer, platform::Answer::Text(_)), "{answer:?}");
    // The next read asks nothing.
    assert!(matches!(
        ask(&mut fake, &mut app, 6, request()),
        platform::Answer::Text(_)
    ));
    assert!(fake.alerts().is_empty());
}

#[test]
fn a_request_that_waits_past_its_deadline_fails() {
    let (mut fake, mut app) = started();
    let request = platform::Request::PaneText {
        pane: None,
        selection: true,
    };
    send(&mut fake, &mut app, 7, request);
    run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(11));
    assert_eq!(
        answered(&mut fake, &mut app, 7),
        platform::Answer::Error {
            code: Code::Failed.number(),
            message: super::super::automation::LATE.to_owned(),
        }
    );
    // The answer that comes after is the plugin's alone: the platform
    // took the request's.
    let question = fake.alerts()[0].id;
    fake.answer_alert(&mut app, question, 1);
    assert_eq!(fake.automation_answers().len(), 1);
}

#[test]
fn the_automation_turned_off_answers_at_once() {
    let config = Config::parse(
        "[plugins.automation]\nenabled = false\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    assert_eq!(
        ask(&mut fake, &mut app, 8, platform::Request::Workspace),
        platform::Answer::Error {
            code: Code::Unavailable.number(),
            message: super::super::automation::OFF.to_owned(),
        }
    );
}

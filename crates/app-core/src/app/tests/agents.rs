//! The tests of the AI agents in the panes: found by their programs'
//! names, their notifications and the Dock's badge.

use super::super::INSTALL_CLAUDE_CODE;
use super::*;

/// An agent of the program `perl`, in raw mode: ready; at a key, it works a
/// while after the key's echo; at the next, it asks for the user by a
/// notification of OSC 9; then it waits.
fn agent() -> Program {
    shell(concat!(
        "exec perl -e '$| = 1; system(\"stty raw -echo\"); print \"ready\\n\"; ",
        "sysread(STDIN, $c, 1); select(undef, undef, undef, 0.8); print \"working\\n\"; ",
        "sysread(STDIN, $c, 1); print \"\\e]9;need you\\a\"; ",
        "sysread(STDIN, $c, 1); sleep 30;'",
    ))
}

/// The state of the agent of `window`'s pane, of its name.
fn agent_of(
    app: &App,
    window: WindowId,
) -> Option<(String, Option<lokyr_kernel::plugins::host::AgentState>)> {
    let shown = app.windows.get(&window)?.window;
    let pane = app.workspace.focused(shown)?;
    let agent = pane.agent()?;
    Some((agent.name().to_owned(), agent.state()))
}

#[test]
fn an_agent_tells_its_end_and_its_requests_while_its_window_has_no_focus() {
    use lokyr_kernel::plugins::host::AgentState::{Done, Waiting};
    let (mut fake, mut app, window) = configured(
        "[agents]\nprograms = [\"perl\"]\ndone-after = 0.5\n",
        agent(),
    );
    // Found by its program's name as its output shows.
    run_until(&mut fake, &mut app, |_, app| {
        agent_of(app, window).is_some_and(|(name, _)| name == "perl")
    });
    fake.send(&mut app, window, WindowEvent::Focused(false));
    // Asked, it works, then is done: a notification tells it.
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    assert_eq!(agent_of(&app, window).unwrap().1, Some(Done));
    let shown = fake.notifications();
    assert_eq!(
        (shown[0].window, shown[0].body.as_str()),
        (window, "perl is done")
    );
    assert_eq!(fake.badge(), None);
    // Its request: waiting, which the Dock's badge counts, until a key.
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| {
        agent_of(app, window).is_some_and(|(_, state)| state == Some(Waiting))
    });
    assert_eq!(fake.badge(), Some("1"));
    fake.send(&mut app, window, key());
    assert_eq!(agent_of(&app, window).unwrap().1, None);
    assert_eq!(fake.badge(), None);
}

#[test]
fn the_agents_programs_and_badge_follow_the_config() {
    let (mut fake, mut app, window) = configured(
        "[agents]\nprograms = [\"perl\"]\ndock-badge = false\n",
        agent(),
    );
    run_until(&mut fake, &mut app, |_, app| {
        agent_of(app, window).is_some()
    });
    // No program of the list: no agent, at once.
    let mut config = app.config.clone();
    config.agents.programs = vec!["claude".to_owned()];
    app.apply(&mut fake, config.clone());
    assert_eq!(agent_of(&app, window), None);
    // An empty list turns the agents off; the list again finds it.
    config.agents.programs = Vec::new();
    app.apply(&mut fake, config.clone());
    assert_eq!(agent_of(&app, window), None);
    config.agents.programs = vec!["perl".to_owned()];
    app.apply(&mut fake, config);
    assert!(agent_of(&app, window).is_some());
}

/// Opens a second tab behind `window`'s, of an agent of the program `perl`
/// that runs `script` and asks for the user at once, and collects its
/// output until it waits, as its session wakes no one: the tab and the
/// pane.
fn waiting_behind(
    fake: &mut Fake,
    app: &mut App,
    window: WindowId,
    script: &str,
) -> (lokyr_kernel::workspace::TabId, PaneId) {
    waiting_behind_of(
        fake,
        app,
        window,
        &format!("exec perl -e '$| = 1; {script}; sleep 30'"),
    )
}

/// [`waiting_behind`], of the shell's `command`.
fn waiting_behind_of(
    fake: &mut Fake,
    app: &mut App,
    window: WindowId,
    command: &str,
) -> (lokyr_kernel::workspace::TabId, PaneId) {
    let (tab, agent) = behind(app, window, command);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app
        .workspace
        .pane(agent)
        .and_then(Pane::agent)
        .and_then(lokyr_kernel::workspace::Agent::state)
        != Some(lokyr_kernel::plugins::host::AgentState::Waiting)
    {
        assert!(Instant::now() < deadline, "no agent waits");
        app.collect(fake, &[agent]);
        std::thread::sleep(Duration::from_millis(10));
    }
    (tab, agent)
}

/// Opens a second tab behind `window`'s, of the shell's `command`: the
/// tab and the pane.
fn behind(
    app: &mut App,
    window: WindowId,
    command: &str,
) -> (lokyr_kernel::workspace::TabId, PaneId) {
    let shown = app.windows[&window].window;
    let waiting = Pane::spawn(
        &shell(command),
        &Environment::default(),
        &Settings::default(),
        Grid {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        },
        &lokyr_vt_core::terminal::Colors::default(),
        &lokyr_kernel::l10n::Texts::new("en", &lokyr_kernel::l10n::KERNEL),
        || {},
    );
    let agent = waiting.id();
    let tab = app.workspace.open_tab(shown, waiting, 1).unwrap();
    (tab, agent)
}

#[test]
fn an_agent_that_a_shell_s_exec_starts_counts_its_first_request() {
    // The shell looked at first, as it sleeps; then the agent of the same
    // group, whose first output asks for the user.
    let (mut fake, mut app, window) = configured(
        "[agents]\nprograms = [\"perl\"]\n",
        shell("echo ready; read line"),
    );
    let (_, agent) = behind(
        &mut app,
        window,
        r#"sleep 1; exec perl -e '$| = 1; print "\e]9;need you\a"; sleep 30'"#,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while app
        .workspace
        .pane(agent)
        .and_then(Pane::foreground_group)
        .is_none()
    {
        assert!(Instant::now() < deadline, "the shell does not run");
        std::thread::sleep(Duration::from_millis(10));
    }
    app.collect(&mut fake, &[agent]);
    // The agent started and asked, between two looks.
    std::thread::sleep(Duration::from_secs(2));
    app.collect(&mut fake, &[agent]);
    assert_eq!(
        app.workspace
            .pane(agent)
            .and_then(Pane::agent)
            .and_then(lokyr_kernel::workspace::Agent::state),
        Some(lokyr_kernel::plugins::host::AgentState::Waiting)
    );
}

#[test]
fn the_next_agent_is_the_one_that_waits_in_another_tab() {
    let (mut fake, mut app, window) = configured(
        "[agents]\nprograms = [\"perl\"]\n",
        shell("echo ready; read line"),
    );
    // Nothing to go to.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    app.next_agent(&mut fake);
    assert!(fake.focus_requests().is_empty());
    // An agent behind, in a second tab, that asks for the user at once.
    let shown = app.windows[&window].window;
    let first = app.workspace.window(shown).unwrap().active_tab();
    let (second, _) = waiting_behind(&mut fake, &mut app, window, r#"print "\e]9;need you\a""#);
    assert_eq!(app.workspace.window(shown).unwrap().active_tab(), first);
    // Its tab shows, its window forward.
    app.next_agent(&mut fake);
    assert_eq!(app.workspace.window(shown).unwrap().active_tab(), second);
    assert_eq!(fake.focus_requests(), [window]);
    // It runs as a command too, which its menu and its keys name.
    assert!(app.commands.get("window.next-agent").is_some());
}

#[test]
fn an_agent_s_request_shows_one_notification_whose_click_shows_its_pane() {
    // Of its own notification, the program's alone; of its bell, the
    // app's.
    for (script, body) in [
        (r#"print "\e]9;need you\a""#, "need you"),
        (
            r#"print "\e]777;notify;Claude Code;need you\a""#,
            "need you",
        ),
        (r#"print "\a""#, "perl is waiting for you"),
    ] {
        let (mut fake, mut app, window) = configured(
            "[agents]\nprograms = [\"perl\"]\n",
            shell("echo ready; read line"),
        );
        fake.send(&mut app, window, WindowEvent::Focused(false));
        let shown = app.windows[&window].window;
        let first = app.workspace.window(shown).unwrap().active_tab();
        let (second, agent) = waiting_behind(&mut fake, &mut app, window, script);
        let told: Vec<&str> = fake
            .notifications()
            .iter()
            .map(|told| told.body.as_str())
            .collect();
        assert_eq!(told, [body], "{script}");
        assert_eq!(app.workspace.window(shown).unwrap().active_tab(), first);
        // Its click brings the agent's tab and pane forward.
        let id = fake.notifications()[0].id;
        fake.send_app(&mut app, AppEvent::NotificationClicked(window, id));
        assert_eq!(app.workspace.window(shown).unwrap().active_tab(), second);
        assert_eq!(app.workspace.focused(shown).map(Pane::id), Some(agent));
        assert_eq!(fake.focus_requests(), [window]);
    }
}

#[test]
fn claude_code_s_integration_asks_of_each_configuration_and_adds_the_hooks() {
    let home = real_directory("claude-home");
    let work = home.join(".claude-work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("settings.json"), "{\"model\": \"opus\"}").unwrap();
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet());
    app.claude_home = Some(home.clone());
    fake.start(&mut app);
    // In the app's menu, after the command line tools.
    let items = menu_items(&fake, 0);
    let at = |title: &str| items.iter().position(|item| item == title);
    assert_eq!(
        at("Install Claude Code Integration…"),
        at("Install Command Line Tools…").map(|tools| tools + 1),
        "{items:?}"
    );
    let answer = |fake: &mut Fake, app: &mut App, button: usize| {
        let alert = fake.alerts().last().expect("a question").id;
        fake.send_app(
            app,
            AppEvent::AlertClosed {
                alert,
                button,
                suppressed: false,
                text: None,
            },
        );
    };
    let message = |fake: &Fake| fake.alerts().last().unwrap().alert.message.clone();
    fake.send_app(&mut app, AppEvent::Command(INSTALL_CLAUDE_CODE.to_owned()));
    // The default configuration's first, which is not there: skipped.
    assert!(
        message(&fake).ends_with(".claude/settings.json?"),
        "{}",
        message(&fake)
    );
    answer(&mut fake, &mut app, 1);
    assert!(!home.join(".claude").exists());
    // Then the other one's, whose hooks go in, its file copied.
    assert!(
        message(&fake).ends_with(".claude-work/settings.json?"),
        "{}",
        message(&fake)
    );
    answer(&mut fake, &mut app, 0);
    let text = std::fs::read_to_string(work.join("settings.json")).unwrap();
    assert!(lokyr_kernel::claude_code::has_hooks(&text), "{text}");
    assert!(text.contains("\"model\": \"opus\""));
    assert!(work.join("settings.json.lokyr-backup").is_file());
    // What was done, told; asked again, nothing to add there.
    assert_eq!(message(&fake), "Claude Code's integration is installed");
    let before = fake.alerts().len();
    fake.send_app(&mut app, AppEvent::Command(INSTALL_CLAUDE_CODE.to_owned()));
    assert!(message(&fake).ends_with(".claude/settings.json?"));
    answer(&mut fake, &mut app, 1);
    assert_eq!(message(&fake), "No settings changed");
    assert_eq!(fake.alerts().len(), before + 2);
}

#[test]
fn an_agent_s_hooks_tell_its_state_and_show_no_notification() {
    use lokyr_kernel::plugins::host::AgentState::{Done, Waiting, Working};
    // An agent whose hooks tell its request, a permission, then its end.
    let agent = shell(concat!(
        "exec perl -e '$| = 1; system(\"stty raw -echo\"); print \"ready\\n\"; ",
        "sysread(STDIN, $c, 1); print \"\\e]777;notify;lokyr-agent:working;\\a\"; ",
        "sysread(STDIN, $c, 1); print \"\\e]777;notify;lokyr-agent:waiting;Claude needs your permission\\a\"; ",
        "sysread(STDIN, $c, 1); print \"\\e]777;notify;lokyr-agent:done;\\a\"; sleep 30'",
    ));
    let (mut fake, mut app, window) = configured("[agents]\nprograms = [\"perl\"]\n", agent);
    run_until(&mut fake, &mut app, |_, app| {
        agent_of(app, window).is_some_and(|(name, _)| name == "perl")
    });
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let state = |app: &App| agent_of(app, window).unwrap().1;
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| state(app) == Some(Working));
    // A permission: waiting, answered by the third key; then done at once,
    // of no quiet.
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| state(app) == Some(Waiting));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| state(app) == Some(Done));
    // The app's own notifications alone: none of the hooks' titles.
    let titles: Vec<&str> = fake
        .notifications()
        .iter()
        .map(|told| told.title.as_str())
        .collect();
    assert!(
        titles
            .iter()
            .all(|title| !title.starts_with("lokyr-agent:")),
        "{titles:?}"
    );
    let bodies: Vec<&str> = fake
        .notifications()
        .iter()
        .map(|told| told.body.as_str())
        .collect();
    assert!(bodies.contains(&"perl is waiting for you"), "{bodies:?}");
}

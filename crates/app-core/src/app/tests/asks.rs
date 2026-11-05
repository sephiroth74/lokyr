//! The tests of the questions before the app acts, and of their answers.

use super::*;

/// The app without padding, of a config whose `[clipboard]` reads
/// `read`, its window running `program`, once it shows `ready`.
fn with_clipboard(read: &str, program: Program) -> (Fake, App, WindowId) {
    let mut config = Config::parse(
        &format!("[clipboard]\nread = \"{read}\"\n"),
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    config.window = unpadded().window;
    let mut fake = Fake::default();
    let mut app = app(config, program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    (fake, app, window)
}

/// A program that reads the clipboard once it reads a byte, and shows
/// the answer.
fn clipboard_reader() -> Program {
    shell(
        "stty raw -echo; echo ready; while :; do head -c 1 >/dev/null; printf '\\033]52;c;?\\a'; done & cat -v",
    )
}

/// The press of Command and Shift and the key of `letter`.
fn command_key_shift(code: KeyCode, letter: &str) -> WindowEvent {
    let mods = Modifiers { shift: true, ..CMD };
    key_event(code, letter, mods, true)
}

/// Pastes `text` with Command-V into `window`.
fn paste_key(fake: &mut Fake, app: &mut App, window: WindowId, text: &str) {
    fake.set_clipboard_text(text);
    fake.send(app, window, command_key(KeyCode::KeyV, "v"));
}

#[test]
fn closing_a_window_whose_program_runs_asks_first() {
    let (mut fake, mut app, window) = started_running(shell("echo ready; exec sleep 30"));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).running().as_deref() == Some("sleep")
    });
    // The close button, then ⌘W: one question, cancelled.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    assert_eq!(fake.alerts().len(), 1);
    let open = &fake.alerts()[0];
    assert_eq!(open.window, Some(window));
    assert_eq!(open.alert.message, app.texts.text("close-running-message"));
    assert_eq!(
        open.alert.detail,
        app.texts
            .text_with("close-running-detail", &[("program", Arg::Text("sleep"))])
    );
    assert_eq!(
        open.alert.buttons,
        [
            app.texts.text("close-running-close"),
            app.texts.text("close-running-cancel")
        ]
    );
    let id = open.id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(fake.windows().count(), 1);
    // Confirmed, it closes, the app staying open without windows.
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    assert_eq!(fake.windows().count(), 0);
    assert!(!fake.has_quit());
}

#[test]
fn closing_a_window_asks_for_a_job_behind_the_shell() {
    // A job in the background, behind a shell that reads, as at its
    // prompt.
    let (mut fake, mut app, window) = started_running(shell("sleep 30 & echo ready; read line"));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(window).running().as_deref() == Some("sleep")
    });
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    assert_eq!(fake.alerts().len(), 1);
    assert_eq!(
        fake.alerts()[0].alert.detail,
        app.texts
            .text_with("close-running-detail", &[("program", Arg::Text("sleep"))])
    );
}

#[test]
fn quitting_while_programs_run_asks_and_the_system_waits_for_the_answer() {
    let (mut fake, mut app, busy) = started_running(shell("echo ready; exec sleep 30"));
    run_until(&mut fake, &mut app, |_, app| {
        app.pane(busy).running().as_deref() == Some("sleep")
    });
    // Another window, whose shell waits: it runs nothing.
    app.program = idle();
    app.run(&mut fake, NEW_WINDOW, Args::new());
    // ⌘Q: a question without a window, cancelled.
    fake.send(&mut app, busy, command_key(KeyCode::KeyQ, "q"));
    assert_eq!(fake.alerts().len(), 1);
    let open = &fake.alerts()[0];
    assert_eq!(open.window, None);
    assert_eq!(
        open.alert.detail,
        app.texts.text_with(
            "quit-running-detail",
            &[
                ("windows", Arg::Number(1)),
                ("programs", Arg::Text("sleep"))
            ]
        )
    );
    let id = open.id;
    fake.answer_alert(&mut app, id, 1);
    assert!(!fake.has_quit());
    assert_eq!(fake.quit_refusals(), 0);
    // The system's request: cancelled, it is refused; a second one
    // while the question is open waits for the same answer.
    fake.send_app(&mut app, AppEvent::QuitRequested);
    fake.send_app(&mut app, AppEvent::QuitRequested);
    assert_eq!(fake.alerts().len(), 1);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(fake.quit_refusals(), 1);
    assert!(!fake.has_quit());
    // The system's request while ⌘Q's question is open takes its
    // answer.
    fake.send(&mut app, busy, command_key(KeyCode::KeyQ, "q"));
    fake.send_app(&mut app, AppEvent::QuitRequested);
    assert_eq!(fake.alerts().len(), 1);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 1);
    assert_eq!(fake.quit_refusals(), 2);
    // Confirmed, the app quits.
    fake.send_app(&mut app, AppEvent::QuitRequested);
    let id = fake.alerts()[0].id;
    fake.answer_alert(&mut app, id, 0);
    assert!(fake.has_quit());
    assert_eq!(fake.quit_refusals(), 2);
}

#[test]
fn programs_of_several_windows_take_one_question() {
    let (mut fake, mut app, first) = started_running(shell("echo ready; exec sleep 30"));
    app.run(&mut fake, NEW_WINDOW, Args::new());
    app.program = shell("echo ready; exec cat");
    app.run(&mut fake, NEW_WINDOW, Args::new());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows
            .values()
            .filter(|state| {
                app.workspace
                    .focused(state.window)
                    .and_then(Pane::running)
                    .is_some()
            })
            .count()
            == 3
    });
    fake.send(&mut app, first, command_key(KeyCode::KeyQ, "q"));
    assert_eq!(
        fake.alerts()[0].alert.detail,
        app.texts.text_with(
            "quit-running-detail",
            &[
                ("windows", Arg::Number(3)),
                ("programs", Arg::Text("sleep, cat"))
            ]
        )
    );
    // A program whose name is not known is "a program".
    assert_eq!(
        app.program_name(String::new()),
        app.texts.text("program-unknown")
    );
}

#[test]
fn a_read_gets_no_answer_unless_the_user_allows_it() {
    // Off by default: no alert, no answer.
    let (mut fake, mut app, window) = with_clipboard("never", clipboard_reader());
    fake.set_clipboard_text("secret");
    fake.send(&mut app, window, key());
    let pause = Instant::now() + Duration::from_millis(400);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(fake.alerts().is_empty());
    assert!(!shows(&app, window, "52;"), "{:?}", screen(&app, window));
    // Asked: refused, then allowed; a read while the alert is open asks
    // nothing more.
    let (mut fake, mut app, window) = with_clipboard("ask", clipboard_reader());
    fake.set_clipboard_text("secret");
    for allowed in [false, true] {
        fake.send(&mut app, window, key());
        run_until(&mut fake, &mut app, |fake, _| !fake.alerts().is_empty());
        fake.send(&mut app, window, key());
        let pause = Instant::now() + Duration::from_millis(300);
        run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
        assert_eq!(fake.alerts().len(), 1);
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        assert_eq!(
            open.alert.buttons,
            [
                app.texts.text("clipboard-read-deny"),
                app.texts.text("clipboard-read-allow")
            ]
        );
        let id = open.id;
        fake.answer_alert(&mut app, id, usize::from(allowed));
        if !allowed {
            let pause = Instant::now() + Duration::from_millis(300);
            run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
            assert!(!shows(&app, window, "52;"), "{:?}", screen(&app, window));
        }
    }
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "^[]52;c;c2VjcmV0^G")
    });
    let shown = screen(&app, window).concat();
    assert_eq!(shown.matches("52;").count(), 1, "{shown:?}");
}

#[test]
fn a_program_s_write_reaches_the_clipboard() {
    // `hello` to the clipboard, then `other` to the primary selection
    // alone, which macOS lacks.
    let program = shell(
        "printf '\\033]52;c;aGVsbG8=\\a'; printf '\\033]52;p;b3RoZXI=\\a'; echo ready; sleep 10",
    );
    let (mut fake, mut app, _) = with_clipboard("never", program);
    run_until(&mut fake, &mut app, |fake, _| {
        fake.clipboard_text().as_deref() == Some("hello")
    });
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(fake.clipboard_text().as_deref(), Some("hello"));
}

#[test]
fn a_link_asks_first_and_the_config_keeps_the_answer() {
    let (mut fake, mut app, window, path) = pasting("links", "# mine\n", links());
    let shown = |fake: &Fake| {
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        (
            open.alert.message.clone(),
            open.alert.detail.clone(),
            open.alert.buttons.clone(),
            open.alert.suppression.clone().unwrap(),
        )
    };
    // A URL: Open or Cancel.
    cmd_click(&mut fake, &mut app, window, 2);
    let url = (
        "Open this link?".to_owned(),
        "https://lokyr.dev/x".to_owned(),
        vec!["Open".to_owned(), "Cancel".to_owned()],
        "Don't ask again for links".to_owned(),
    );
    assert_eq!(shown(&fake), url);
    // Ticked, but cancelled: nothing changes.
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 1, true);
    // A file: Open, Show in Finder, or Cancel.
    cmd_click(&mut fake, &mut app, window, 22);
    let file = (
        "Open this file?".to_owned(),
        "/etc/hosts".to_owned(),
        vec![
            "Open".to_owned(),
            "Show in Finder".to_owned(),
            "Cancel".to_owned(),
        ],
        "Don't ask again for files".to_owned(),
    );
    assert_eq!(shown(&fake), file);
    fake.answer_alert(&mut app, fake.alerts()[0].id, 1);
    // A program shows in the Finder, and a directory opens, without a
    // question, each at the place that its links lead to.
    cmd_click(&mut fake, &mut app, window, 33);
    cmd_click(&mut fake, &mut app, window, 46);
    assert!(fake.alerts().is_empty());
    assert_eq!(fake.opened(), [real("/usr")]);
    assert_eq!(fake.revealed(), [real("/etc/hosts"), real("/usr/bin/true")]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");
    // Ticked, the answer goes into the config, and the next one opens
    // without a question, before the file's reload too.
    cmd_click(&mut fake, &mut app, window, 2);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    cmd_click(&mut fake, &mut app, window, 22);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 1, true);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.starts_with("# mine\n"), "{written}");
    let links = Config::read(&path).unwrap().links;
    assert_eq!((links.urls, links.files), (OpenUrl::Open, OpenFile::Reveal));
    cmd_click(&mut fake, &mut app, window, 2);
    cmd_click(&mut fake, &mut app, window, 22);
    assert!(fake.alerts().is_empty());
    assert_eq!(
        fake.opened()[1..],
        ["https://lokyr.dev/x", "https://lokyr.dev/x"]
    );
    assert_eq!(
        fake.revealed()[2..],
        [real("/etc/hosts"), real("/etc/hosts")]
    );
    // Opening a file, ticked, keeps `open`.
    std::fs::write(&path, "").unwrap();
    app.apply(&mut fake, Config::default());
    cmd_click(&mut fake, &mut app, window, 22);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    assert_eq!(fake.opened().last(), Some(&real("/etc/hosts")));
    assert_eq!(Config::read(&path).unwrap().links.files, OpenFile::Open);
    assert!(app.links_chosen == (None, None));
}

/// The path that `path`'s links lead to.
fn real(path: &str) -> String {
    std::fs::canonicalize(path).unwrap().display().to_string()
}

#[test]
fn a_program_never_opens_and_the_rest_do_as_the_config_says() {
    use crate::app::asks::{Opening, opening};

    for urls in [OpenUrl::Ask, OpenUrl::Open] {
        for files in [OpenFile::Ask, OpenFile::Open, OpenFile::Reveal] {
            let links = lokyr_kernel::config::LinksConfig { urls, files };
            let of = |kind| opening(kind, links);
            assert_eq!(of(TargetKind::Program), Opening::Reveal, "{links:?}");
            assert_eq!(of(TargetKind::Directory), Opening::Open, "{links:?}");
            let url = if urls == OpenUrl::Ask {
                Opening::Ask
            } else {
                Opening::Open
            };
            assert_eq!(of(TargetKind::Url), url, "{links:?}");
            let file = match files {
                OpenFile::Ask => Opening::Ask,
                OpenFile::Open => Opening::Open,
                OpenFile::Reveal => Opening::Reveal,
            };
            assert_eq!(of(TargetKind::File), file, "{links:?}");
        }
    }
}

#[test]
fn a_file_that_becomes_a_program_while_its_question_waits_shows_in_the_finder() {
    let directory = real_directory("becomes-program");
    let file = directory.join("notes.txt");
    std::fs::write(&file, "notes").unwrap();
    let program = shell(&format!(
        "stty raw -echo; printf '{}\\r\\n'; echo ready; cat -v",
        file.display()
    ));
    let (mut fake, mut app, window, _) = pasting("becomes-program", "", program);
    cmd_click(&mut fake, &mut app, window, 2);
    assert_eq!(fake.alerts()[0].alert.message, "Open this file?");
    // The file can run before the answer, which opens it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    fake.answer_alert(&mut app, fake.alerts()[0].id, 0);
    #[cfg(unix)]
    {
        assert!(fake.opened().is_empty());
        assert_eq!(fake.revealed(), [file.display().to_string()]);
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_link_to_a_file_url_without_a_path_opens_nothing() {
    let program = shell(concat!(
        "stty raw -echo; ",
        "printf '\\033]8;;file://localhost\\033\\\\here\\033]8;;\\033\\\\\\r\\n'; ",
        "echo ready; cat -v"
    ));
    let (mut fake, mut app, window, _) = pasting("no-path", "[links]\nfiles = \"open\"\n", program);
    cmd_click(&mut fake, &mut app, window, 1);
    assert!(fake.opened().is_empty() && fake.revealed().is_empty());
    assert!(fake.alerts().is_empty());
    // The app goes on: the window takes its keys.
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "x"));
}

#[test]
fn a_window_that_closes_drops_its_link_s_question() {
    let (mut fake, mut app, window, _) = pasting("link-close", "", links());
    cmd_click(&mut fake, &mut app, window, 2);
    assert_eq!(app.open_asks.len(), 1);
    app.run(&mut fake, NEW_WINDOW, Args::new());
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert!(app.open_asks.is_empty());
}

#[test]
fn a_link_s_question_shows_its_invisible_characters_escaped() {
    assert_eq!(
        shown("https://lokyr.dev/\u{202E}gpj.exe\u{200B}x\u{7}é"),
        "https://lokyr.dev/\\u{202E}gpj.exe\\u{200B}x\\u{7}é"
    );
    assert_eq!(shown("/Users/x/日本 文書.txt"), "/Users/x/日本 文書.txt");
    for c in [
        '\u{2066}',
        '\u{2069}',
        '\u{2028}',
        '\u{FEFF}',
        '\u{E0041}',
        '\u{FE0F}',
    ] {
        assert!(invisible(c), "{c:?}");
    }
    for c in ['a', ' ', '/', 'é', '日', '😀'] {
        assert!(!invisible(c), "{c:?}");
    }
}

#[test]
fn a_link_s_answer_lasts_until_the_app_quits_where_the_config_cannot_keep_it() {
    let text = "[font]\nsise = 14\n";
    let (mut fake, mut app, window, path) = pasting("bad-links", text, links());
    cmd_click(&mut fake, &mut app, window, 2);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    cmd_click(&mut fake, &mut app, window, 22);
    fake.answer_alert_with(&mut app, fake.alerts()[0].id, 0, true);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert_eq!(
        app.links_chosen,
        (Some(OpenUrl::Open), Some(OpenFile::Open))
    );
    // A reload of the file does not bring the questions back.
    app.apply(&mut fake, Config::default());
    cmd_click(&mut fake, &mut app, window, 2);
    cmd_click(&mut fake, &mut app, window, 22);
    assert!(fake.alerts().is_empty());
    assert_eq!(fake.opened().len(), 4);
}

#[test]
fn copy_and_paste_go_through_the_clipboard_and_several_lines_ask() {
    let (mut fake, mut app, window) = focused_with(reader(""));
    // The pane's text, to its cursor's row.
    fake.send(&mut app, window, command_key(KeyCode::KeyA, "a"));
    fake.send(&mut app, window, command_key(KeyCode::KeyC, "c"));
    assert_eq!(fake.clipboard_text().as_deref(), Some("ready\n"));
    // A line pastes at once, and a line with its line break without it.
    paste_key(&mut fake, &mut app, window, "hello");
    paste_key(&mut fake, &mut app, window, "ls\n");
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "hellols"));
    assert!(fake.alerts().is_empty());
    // Several lines ask first, once at a time: cancelled, pasted, and
    // pasted as one line.
    for button in [2, 0, 1] {
        paste_key(&mut fake, &mut app, window, "a\nb\n");
        paste_key(&mut fake, &mut app, window, "a\nb\n");
        assert_eq!(fake.alerts().len(), 1);
        let open = &fake.alerts()[0];
        assert_eq!(open.window, Some(window));
        assert_eq!(
            open.alert.message,
            app.texts
                .text_with("paste-lines-message", &[("lines", Arg::Number(2))])
        );
        assert!(
            open.alert.detail.ends_with("\n\na\nb"),
            "{}",
            open.alert.detail
        );
        assert_eq!(
            open.alert.buttons,
            [
                app.texts.text("paste-lines-paste"),
                app.texts.text("paste-lines-one-line"),
                app.texts.text("paste-lines-cancel")
            ]
        );
        assert_eq!(
            open.alert.suppression,
            Some(app.texts.text("paste-lines-dont-ask"))
        );
        let id = open.id;
        fake.answer_alert(&mut app, id, button);
        if button == 2 {
            let pause = Instant::now() + Duration::from_millis(300);
            run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
            assert!(
                !shows(&app, window, "hellolsa"),
                "{:?}",
                screen(&app, window)
            );
        }
    }
    // The line breaks reach the program as Return; as one line, a
    // space for each, but the last.
    run_until(&mut fake, &mut app, |_, app| {
        shows(app, window, "hellolsa^Mb^Ma b")
    });
    // The question asked nothing of the config.
    assert_eq!(app.config.clipboard.confirm_paste, ConfirmPaste::Auto);
}

#[test]
fn a_program_that_asks_for_bracketed_paste_takes_line_breaks_at_once() {
    let (mut fake, mut app, window) = focused_with(reader("\\033[?2004h"));
    paste_key(&mut fake, &mut app, window, "a\nb");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "^[[200~a"));
    // Always asks, there too, and for a line with its line break.
    let (mut fake, mut app, window, _) = pasting(
        "always",
        "[clipboard]\nconfirm-paste = \"always\"\n",
        reader("\\033[?2004h"),
    );
    paste_key(&mut fake, &mut app, window, "ls\n");
    assert_eq!(fake.alerts().len(), 1);
    // Never asks nothing.
    let (mut fake, mut app, window, _) = pasting(
        "never",
        "[clipboard]\nconfirm-paste = \"never\"\n",
        reader(""),
    );
    paste_key(&mut fake, &mut app, window, "a\nb");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "a^Mb"));
}

#[test]
fn don_t_ask_again_writes_the_config() {
    let (mut fake, mut app, window, path) =
        pasting("again", "# mine\n[font]\nsize = 14\n", reader(""));
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    // Ticked, but cancelled: nothing changes.
    fake.answer_alert_with(&mut app, id, 2, true);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\n[font]\nsize = 14\n"
    );
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    fake.answer_alert_with(&mut app, id, 0, true);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.starts_with("# mine\n[font]\nsize = 14\n"),
        "{written}"
    );
    assert_eq!(
        Config::read(&path).unwrap().clipboard.confirm_paste,
        ConfirmPaste::Never
    );
    // The next paste pastes, before the file's reload too.
    paste_key(&mut fake, &mut app, window, "c\nd");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "a^Mbc^Md"));
    assert!(!app.pastes_unconfirmed);
}

#[test]
fn don_t_ask_again_lasts_until_the_app_quits_where_the_config_cannot_keep_it() {
    let text = "[font]\nsise = 14\n";
    let (mut fake, mut app, window, path) = pasting("bad", text, reader(""));
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    fake.answer_alert_with(&mut app, id, 1, true);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert!(app.pastes_unconfirmed);
    // A reload of the file does not bring the question back.
    app.apply(&mut fake, Config::default());
    paste_key(&mut fake, &mut app, window, "c\nd");
    assert!(fake.alerts().is_empty());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "a bc^Md"));
}

#[test]
fn a_window_that_closes_drops_its_paste_s_question() {
    let (mut fake, mut app, window) = focused_with(reader(""));
    paste_key(&mut fake, &mut app, window, "a\nb");
    let id = fake.alerts()[0].id;
    app.run(&mut fake, NEW_WINDOW, Args::new());
    // Its program, `cat`, runs: the question, answered.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert!(app.paste_asks.is_empty());
    // A late answer reaches nothing.
    fake.answer_alert(&mut app, id, 0);
    assert_eq!(fake.windows().count(), 1);
}

#[test]
fn a_title_of_the_user_s_takes_the_place_of_the_window_s_own() {
    let program = shell("stty raw -echo; echo ready; cat");
    let (mut fake, mut app, window) = configured("", program);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let title = |fake: &Fake| fake.window(window).unwrap().title.clone();
    let own = app.title_of(window);
    let ask = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, command_key_shift(KeyCode::KeyI, "i"));
        let open = fake.alerts().last().expect("the question").clone();
        assert_eq!(open.window, Some(window));
        open
    };
    // The question holds nothing at first; its title, trimmed.
    let open = ask(&mut fake, &mut app);
    assert_eq!(open.alert.input.as_deref(), Some(""));
    assert_eq!(open.alert.buttons, ["Set", "Cancel"]);
    // A second ask waits for the first's answer.
    fake.send_app(&mut app, AppEvent::Command(SET_TITLE.to_owned()));
    assert_eq!(fake.alerts().len(), 1);
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some("  build  "));
    assert_eq!(title(&fake), "build — 200×62");
    // A program's title stays behind it.
    app.pane(window).text("\u{1b}]2;prompt\u{7}");
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(title(&fake), "build — 200×62");
    // Asked again, it holds the user's; cancelled, it stays.
    let open = ask(&mut fake, &mut app);
    assert_eq!(open.alert.input.as_deref(), Some("build"));
    fake.answer_alert_typed(&mut app, open.id, (1, false), Some("other"));
    assert_eq!(title(&fake), "build — 200×62");
    // Emptied, the window's own comes back, of the program's title.
    let open = ask(&mut fake, &mut app);
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some(" "));
    assert_eq!(title(&fake), format!("prompt — {} — 200×62", shell_name()));
    assert_ne!(own, title(&fake));
}

#[test]
fn a_config_that_asks_applies_to_the_open_windows() {
    let (mut fake, mut app, window) = with_clipboard("never", clipboard_reader());
    let mut asking = app.config.clone();
    asking.clipboard.read = ClipboardRead::Ask;
    app.apply(&mut fake, asking);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
    // A window that closes takes its ask with it, once its program's
    // question, as `cat` runs, is answered.
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    let close = fake.alerts().last().unwrap().id;
    fake.answer_alert(&mut app, close, 0);
    assert!(app.clipboard_asks.is_empty());
}

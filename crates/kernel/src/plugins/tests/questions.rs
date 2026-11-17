//! A plugin's questions, one at a time, its next once the user answered the
//! last; the color panel and the themes' colors shown before they are
//! written, which its stop takes away; and what a plugin of another's than
//! Lokyr's asks of the app and reads of a theme.

use lokyr_plugin_sdk::host::{ThemeFormat, ThemeInfo, ThemeOrigin, TokenOrigin};
use lokyr_plugin_sdk::theme::{ThemeColor, ThemeVariant};

use super::*;

/// The codes of a plugin's calls' errors, each `None` of a call that did
/// not fail.
type Codes = Rc<RefCell<Vec<Option<Code>>>>;

/// A plugin whose commands ask a question of buttons, `<name>.ask`; ask for
/// a text and fail, `<name>.fails`; show the color panel, `<name>.panel`;
/// show Mine's accent before it is written, `<name>.preview`; read the
/// tokens of the theme in force, `<name>.tokens`, into `tokens`; and fault,
/// `<name>.fault`: each call's error's code kept in `codes`.
fn asker(
    name: &'static str,
    codes: &Codes,
    tokens: &Rc<RefCell<Option<host::ThemeTokens>>>,
) -> Box<dyn Plugin> {
    let (codes, tokens) = (Rc::clone(codes), Rc::clone(tokens));
    scripted(
        move |cx| {
            for action in ["ask", "fails", "panel", "preview", "tokens", "fault"] {
                cx.register_command(command(&format!("{name}.{action}")))?;
            }
            Ok(())
        },
        move |cx, run, _| {
            let done = match run.rsplit('.').next() {
                Some("ask") => cx
                    .ask_choice(None, ("Use it?", ""), &["Use", "Cancel"])
                    .map(|_| ()),
                Some("fails") => {
                    cx.ask_text(None, "Its name", "", "")?;
                    return Err(Error::new(Code::Failed, "on purpose"));
                }
                Some("panel") => cx.show_color_panel(ThemeColor::hex(0x4c9bff), true),
                Some("preview") => cx.preview_theme_value("Mine", "accent", Some("#ff0000")),
                Some("tokens") => cx
                    .theme_tokens(None)
                    .map(|read| *tokens.borrow_mut() = read),
                _ => return Err(Error::new(Code::Fault, "on purpose")),
            };
            codes.borrow_mut().push(done.err().map(|error| error.code));
            Ok(())
        },
    )
}

/// The themes of an app of a theme of the user's folder, Mine, in force.
fn mine() -> host::Themes {
    host::Themes {
        appearance: ThemeVariant::Dark,
        current: "Mine".to_owned(),
        themes: vec![ThemeInfo {
            name: "Mine".to_owned(),
            variant: ThemeVariant::Dark,
            origin: ThemeOrigin::Folder,
            format: ThemeFormat::Lokyr,
        }],
        loading: false,
    }
}

fn run(plugins: &mut Plugins, app: &mut App, command: &str) {
    let _ = plugins.run(command, Args::new(), &mut app.context());
}

#[test]
fn a_plugin_asks_one_question_at_a_time_its_next_once_its_answer_came() {
    let mut app = App::default();
    let (codes, tokens) = (Codes::default(), Rc::default());
    let mut plugins = Plugins::new();
    plugins
        .add("asker", asker("asker", &codes, &tokens))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let asked = |plugins: &mut Plugins| -> Vec<u64> {
        plugins
            .take_requests()
            .into_iter()
            .filter_map(|request| match request {
                Request::AskChoice { id, .. } => Some(id),
                _ => None,
            })
            .collect()
    };
    // The second waits for the first's answer.
    run(&mut plugins, &mut app, "asker.ask");
    run(&mut plugins, &mut app, "asker.ask");
    assert_eq!(*codes.borrow(), [None, Some(Code::Unavailable)]);
    let first = asked(&mut plugins);
    assert_eq!(first.len(), 1);
    // Another question's answer leaves it waiting; its own closes it.
    plugins.chosen("asker", first[0] + 1, Some(0));
    run(&mut plugins, &mut app, "asker.ask");
    assert_eq!(codes.borrow().last(), Some(&Some(Code::Unavailable)));
    plugins.chosen("asker", first[0], Some(0));
    run(&mut plugins, &mut app, "asker.ask");
    assert_eq!(codes.borrow().last(), Some(&None));
    let second = asked(&mut plugins);
    assert_eq!(second.len(), 1);
    // A question of a call that fails goes with its request, as if it had
    // not been asked: the next waits for nothing more.
    plugins.answer("asker", second[0], None);
    run(&mut plugins, &mut app, "asker.fails");
    assert!(plugins.take_requests().is_empty());
    run(&mut plugins, &mut app, "asker.ask");
    assert_eq!(codes.borrow().last(), Some(&None));
}

#[test]
fn a_stopped_plugin_s_color_panel_closes_and_its_colors_shown_before_they_are_written_go() {
    let mut app = App {
        themes: Some(Box::new(mine)),
        ..App::default()
    };
    let (codes, tokens) = (Codes::default(), Rc::default());
    let mut plugins = Plugins::new();
    for name in ["shows", "closed", "idle"] {
        plugins.add(name, asker(name, &codes, &tokens)).unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    // One shows the panel and a color; one shows the panel that the user
    // closes; one shows neither.
    run(&mut plugins, &mut app, "shows.panel");
    run(&mut plugins, &mut app, "shows.preview");
    run(&mut plugins, &mut app, "closed.panel");
    assert_eq!(*codes.borrow(), [None; 3]);
    plugins.color_panel_closed("closed");
    let shown = plugins.take_requests();
    assert!(
        shown.iter().any(|request| matches!(
            request,
            Request::ShowColorPanel { plugin, settings: true, .. } if plugin == "shows"
        )),
        "{shown:?}"
    );
    assert!(
        shown.iter().any(|request| matches!(
            request,
            Request::PreviewThemeValue { plugin, value: Some(_), .. } if plugin == "shows"
        )),
        "{shown:?}"
    );
    // Each faults: the first's panel closes and its color goes, as the app
    // hears; of the others, nothing.
    for name in ["shows", "closed", "idle"] {
        run(&mut plugins, &mut app, &format!("{name}.fault"));
    }
    let stopped: Vec<Request> = plugins
        .take_requests()
        .into_iter()
        .filter(|request| {
            matches!(
                request,
                Request::CloseColorPanel { .. } | Request::EndPreviews { .. }
            )
        })
        .collect();
    assert!(
        matches!(
            stopped.as_slice(),
            [
                Request::CloseColorPanel { plugin: closing },
                Request::EndPreviews { plugin: ending },
            ] if closing == "shows" && ending == "shows"
        ),
        "{stopped:?}"
    );
}

#[test]
fn another_s_plugin_reads_a_theme_s_files_by_name_and_is_told_of_no_config_to_the_app() {
    let mut app = App::default();
    let (codes, tokens) = (Codes::default(), Rc::default());
    let manifest = Manifest::parse(concat!(
        "[plugin]\nname = \"acme/asker\"\nversion = \"1.0.0\"\nkind = \"luau\"\n",
        "api = \"^0.1\"\nlicense = \"MIT\"\n",
    ))
    .unwrap();
    let mut plugins = Plugins::new();
    plugins
        .add_declared(manifest, asker("asker", &codes, &tokens))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    run(&mut plugins, &mut app, "asker.tokens");
    run(&mut plugins, &mut app, "asker.panel");
    run(&mut plugins, &mut app, "asker.ask");
    assert_eq!(*codes.borrow(), [None; 3]);
    // A file's name, of no folder.
    let read = tokens.borrow().clone().expect("the tokens");
    let files: Vec<&str> = read
        .terminal
        .iter()
        .chain(&read.tokens)
        .filter_map(|token| match &token.origin {
            TokenOrigin::File { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert!(!files.is_empty());
    assert!(
        files.iter().all(|path| *path == "Lokyr Dark.toml"),
        "{files:?}"
    );
    // Its panel and its question of no `config`, which the app tells apart.
    let asked = plugins.take_requests();
    assert!(
        asked.iter().any(|request| matches!(
            request,
            Request::ShowColorPanel {
                settings: false,
                ..
            }
        )),
        "{asked:?}"
    );
    assert!(
        asked.iter().any(|request| matches!(
            request,
            Request::AskChoice {
                settings: false,
                ..
            }
        )),
        "{asked:?}"
    );
}

/// A plugin of a thread of its own whose commands ask a question of
/// buttons, `stepper.ask`, and read the windows, then wait for `go` and
/// fail, `stepper.slow`, `read` set once the read came back.
struct Stepper {
    read: Arc<AtomicBool>,
    go: Arc<AtomicBool>,
}

impl Plugin for Stepper {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["ask", "slow"] {
            cx.register_command(command(&format!("stepper.{name}")))?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        if name == "stepper.ask" {
            return cx
                .ask_choice(None, ("Use it?", ""), &["Use", "Cancel"])
                .map(|_| ());
        }
        cx.windows()?;
        self.read.store(true, Ordering::SeqCst);
        let start = Instant::now();
        while !self.go.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(Error::new(Code::Failed, "on purpose"))
    }
}

#[test]
fn a_call_on_a_plugin_s_thread_that_fails_across_an_answer_leaves_its_plugin_free_to_ask() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (read, go) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let (made_read, made_go) = (Arc::clone(&read), Arc::clone(&go));
    let manifest = Manifest::parse(
        "[plugin]\nname = \"acme/stepper\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    plugins
        .add_installed(
            manifest,
            Box::new(move || {
                Box::new(Stepper {
                    read: Arc::clone(&made_read),
                    go: Arc::clone(&made_go),
                })
            }),
            (&[], &[]),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let serve_until = |plugins: &mut Plugins, app: &mut App, done: &dyn Fn(&Plugins) -> bool| {
        let start = Instant::now();
        while !done(plugins) {
            plugins.serve(&mut app.context());
            assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    serve_until(&mut plugins, &mut app, &|plugins| !plugins.waiting());
    let asked = |plugins: &mut Plugins| -> Vec<u64> {
        plugins
            .take_requests()
            .into_iter()
            .filter_map(|request| match request {
                Request::AskChoice { id, .. } => Some(id),
                _ => None,
            })
            .collect()
    };
    let name = plugins.entries[0].state.name.clone();
    plugins
        .run("stepper.ask", Args::new(), &mut app.context())
        .unwrap();
    serve_until(&mut plugins, &mut app, &|plugins| !plugins.waiting());
    let first = asked(&mut plugins);
    assert_eq!(first.len(), 1);
    // A call reads, the user answers the question, and the call fails.
    plugins
        .run("stepper.slow", Args::new(), &mut app.context())
        .unwrap();
    serve_until(&mut plugins, &mut app, &|_| read.load(Ordering::SeqCst));
    plugins.chosen(&name, first[0], Some(0));
    go.store(true, Ordering::SeqCst);
    serve_until(&mut plugins, &mut app, &|plugins| !plugins.waiting());
    // The answered question stays answered: the plugin asks again.
    assert_eq!(plugins.entries[0].state.question, None);
    plugins
        .run("stepper.ask", Args::new(), &mut app.context())
        .unwrap();
    serve_until(&mut plugins, &mut app, &|plugins| !plugins.waiting());
    assert_eq!(asked(&mut plugins).len(), 1);
}

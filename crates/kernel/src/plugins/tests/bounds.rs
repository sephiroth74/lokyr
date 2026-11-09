//! A call's bounds: its runs of commands one chain, its requests of the
//! app's counted with its writes, and its searches.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

/// The chains of the runs of `requests`.
fn chains(requests: Vec<Request>) -> Vec<Chain> {
    requests
        .into_iter()
        .filter_map(|request| match request {
            Request::Run { chain, .. } => Some(chain),
            _ => None,
        })
        .collect()
}

#[test]
fn a_call_s_runs_are_of_one_chain_and_its_requests_of_the_app_count_with_its_writes() {
    let mut app = App::default();
    window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    plugins
        .add(
            "asker",
            scripted(
                |cx| {
                    cx.register_command(command("asker.go"))?;
                    cx.register_command(command("asker.again"))?;
                    cx.register_command(command("asker.once"))?;
                    // An activation's runs, of no command's chain.
                    cx.run_command("asker.again", Args::new())?;
                    cx.run_command("asker.again", Args::new())
                },
                move |cx, name, _| {
                    match name {
                        "asker.once" => return cx.run_command("asker.again", Args::new()),
                        "asker.go" => {}
                        _ => return Ok(()),
                    }
                    let pane = cx.focused_pane().unwrap();
                    let mut seen = seen.borrow_mut();
                    for at in 0..=MOST_WRITES {
                        let asked = if at % 2 == 0 {
                            cx.scroll(pane, host::Scroll::Top)
                        } else {
                            cx.run_command("asker.again", Args::new())
                        };
                        seen.push(asked.err().map(|error| error.code));
                    }
                    Ok(())
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let activated = chains(plugins.take_requests());
    assert_eq!(activated.len(), 2);
    assert_ne!(activated[0].id, 0);
    assert_eq!(activated[0], activated[1]);
    // The user's run, of no chain yet: its runs of one chain, a new one,
    // and its writes and its requests of the app bounded together.
    plugins
        .run("asker.go", Args::new(), &mut app.context())
        .unwrap();
    let codes = codes.borrow();
    assert!(codes[..MOST_WRITES].iter().all(Option::is_none));
    assert_eq!(codes[MOST_WRITES], Some(Code::Failed));
    let ran = chains(plugins.take_requests());
    assert_eq!(ran.len(), MOST_WRITES / 2);
    assert!(ran.iter().all(|chain| *chain == ran[0]));
    assert!(ran[0].id != 0 && ran[0].id != activated[0].id);
    // A command's call of a chain gives its runs that chain.
    let chain = Chain {
        depth: 3,
        ..new_chain()
    };
    plugins
        .run_deep("asker.once", Args::new(), chain, &mut app.context())
        .unwrap();
    assert_eq!(chains(plugins.take_requests()), vec![chain]);
}

/// A plugin whose `searcher.many` searches its pane `count` times, whose
/// `searcher.fails` searches it and fails, and whose look searches it.
struct Searcher {
    count: usize,
    codes: Rc<RefCell<Vec<Option<Code>>>>,
}

impl Searcher {
    /// A search of `pane`, its code if it fails.
    fn search(&self, cx: &mut Cx<'_>, pane: host::PaneId) -> Option<Code> {
        let query = host::SearchQuery {
            text: "x".to_owned(),
            ..host::SearchQuery::default()
        };
        cx.search(pane, query).err().map(|error| error.code)
    }
}

impl Plugin for Searcher {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("searcher.many"))?;
        cx.register_command(command("searcher.fails"))
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let pane = cx.focused_pane().unwrap();
        if name == "searcher.fails" {
            let code = self.search(cx, pane);
            self.codes.borrow_mut().push(code);
            return Err(Error::new(Code::Failed, "on purpose"));
        }
        for _ in 0..self.count {
            let code = self.search(cx, pane);
            self.codes.borrow_mut().push(code);
        }
        Ok(())
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        let pane = cx.focused_pane().unwrap();
        let code = self.search(cx, pane);
        self.codes.borrow_mut().push(code);
        true
    }
}

#[test]
fn a_call_s_searches_are_bounded_a_look_starts_none_and_a_failed_call_s_end() {
    let mut app = App::default();
    window(&mut app, "echo x; sleep 10");
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    plugins
        .add(
            "searcher",
            Box::new(Searcher {
                count: MOST_SEARCHES + 1,
                codes: Rc::clone(&codes),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let searches = |plugins: &Plugins| plugins.entries[0].state.searches.len();
    // Past the bound, refused; each of the pane's ends the one before.
    plugins
        .run("searcher.many", Args::new(), &mut app.context())
        .unwrap();
    let taken: Vec<Option<Code>> = codes.borrow_mut().drain(..).collect();
    assert!(taken[..MOST_SEARCHES].iter().all(Option::is_none));
    assert_eq!(taken[MOST_SEARCHES], Some(Code::Failed));
    assert_eq!(searches(&plugins), 1);
    // A look starts none, and the search under way goes on.
    assert!(
        plugins
            .applies("searcher.many", &Args::new(), &mut app.context())
            .unwrap()
    );
    assert_eq!(codes.borrow_mut().pop(), Some(Some(Code::Failed)));
    assert_eq!(searches(&plugins), 1);
    // A failed call's search ends, as its id names nothing.
    let failed = plugins.run("searcher.fails", Args::new(), &mut app.context());
    assert_eq!(failed.unwrap_err().code, Code::Failed);
    assert_eq!(codes.borrow_mut().pop(), Some(None));
    assert_eq!(searches(&plugins), 0);
}

#[test]
fn a_declaration_made_already_changes_nothing_and_a_plugin_s_are_bounded() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let codes = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&codes);
    plugins
        .add(
            "binder",
            scripted(
                |cx| {
                    cx.register_command(Command {
                        params: vec![Param {
                            name: "n".to_owned(),
                            kind: Kind::Integer,
                            default: Some(Value::Integer(0)),
                        }],
                        ..command("binder.go")
                    })?;
                    cx.register_command(command("binder.same"))?;
                    cx.register_command(command("binder.many"))
                },
                move |cx, name, _| {
                    let n = |n: i64| Args::from([("n".to_owned(), Value::Integer(n))]);
                    if name == "binder.many" {
                        let mut seen = seen.borrow_mut();
                        for at in 0..MOST_DECLARED {
                            let bound = cx.bind(PluginBinding {
                                keys: "ctrl+t".to_owned(),
                                command: "binder.go".to_owned(),
                                args: n(i64::try_from(at).unwrap() + 1),
                                when: None,
                            });
                            seen.push(bound.err().map(|error| error.code));
                        }
                        return Ok(());
                    }
                    cx.bind(PluginBinding {
                        keys: "ctrl+t".to_owned(),
                        command: "binder.go".to_owned(),
                        args: n(0),
                        when: None,
                    })?;
                    cx.bind(PluginBinding {
                        keys: "global:ctrl+alt+k".to_owned(),
                        command: "binder.go".to_owned(),
                        args: Args::new(),
                        when: None,
                    })?;
                    cx.add_menu_item(host::MenuItem {
                        place: host::MenuPlace::PaneContext,
                        command: "binder.go".to_owned(),
                        args: Args::new(),
                        title: None,
                    })
                },
            ),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(plugins.take_declarations_changed());
    let state = |plugins: &Plugins| {
        let state = &plugins.entries[0].state;
        (
            state.bindings.len(),
            state.global_bindings.len(),
            state.menu_items.len(),
        )
    };
    plugins
        .run("binder.same", Args::new(), &mut app.context())
        .unwrap();
    assert!(plugins.take_declarations_changed());
    assert_eq!(state(&plugins), (1, 1, 1));
    // The same again: nothing changes, and the keymap is not made anew.
    plugins
        .run("binder.same", Args::new(), &mut app.context())
        .unwrap();
    assert!(!plugins.take_declarations_changed());
    assert_eq!(state(&plugins), (1, 1, 1));
    // Past the bound, of its 3 commands and its 3 keys and items, refused.
    plugins
        .run("binder.many", Args::new(), &mut app.context())
        .unwrap();
    let codes = codes.borrow();
    let taken = MOST_DECLARED - 6;
    assert!(codes[..taken].iter().all(Option::is_none));
    assert!(
        codes[taken..]
            .iter()
            .all(|code| *code == Some(Code::Failed))
    );
    assert_eq!(state(&plugins).0, 1 + taken);
}

/// A plugin whose `panel.open` opens its panel `p`, whose `panel.close`
/// closes it, whose `panel.fails` closes it and fails, and whose look
/// closes it; its panels' closing logged.
struct Panelist {
    closed: Rc<RefCell<usize>>,
}

impl Plugin for Panelist {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["panel.open", "panel.close", "panel.fails"] {
            cx.register_command(command(name))?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        match name {
            "panel.open" => {
                let window = cx.focused_window().unwrap();
                cx.open_panel(
                    "p",
                    PanelOptions {
                        anchor: Anchor::Pointer(window),
                        role: lokyr_plugin_sdk::ui::Role::Dialog,
                        close_outside: false,
                        close_escape: true,
                        takes_keys: false,
                    },
                )
            }
            "panel.close" => cx.close_panel("p"),
            _ => {
                cx.close_panel("p")?;
                Err(Error::new(Code::Failed, "on purpose"))
            }
        }
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        cx.close_panel("p").is_ok()
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let closed = events
            .iter()
            .filter(|event| matches!(event, Event::PanelClosed(_)))
            .count();
        *self.closed.borrow_mut() += closed;
        Ok(())
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<Node> {
        Some(Node::text("p"))
    }
}

#[test]
fn a_failed_call_or_a_look_that_closes_a_panel_leaves_it_open_of_its_tree() {
    let mut app = App::default();
    let shown = window(&mut app, "sleep 30");
    app.focused = Some(shown);
    let mut plugins = Plugins::new();
    let closed = Rc::new(RefCell::new(0));
    plugins
        .add(
            "panelist",
            Box::new(Panelist {
                closed: Rc::clone(&closed),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("panel.open", Args::new(), &mut app.context())
        .unwrap();
    plugins.take_requests();
    let panel = ViewId::Panel(PanelId::from("p"));
    plugins
        .render("panelist", &panel, &mut app.context())
        .unwrap();
    assert!(plugins.tree("panelist", &panel).is_some());
    // A failed call's close, and a look's: the panel open still, of its
    // tree, and no close told.
    let failed = plugins.run("panel.fails", Args::new(), &mut app.context());
    assert_eq!(failed.unwrap_err().code, Code::Failed);
    assert!(
        plugins
            .applies("panel.close", &Args::new(), &mut app.context())
            .unwrap()
    );
    assert!(plugins.take_requests().is_empty());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*closed.borrow(), 0);
    assert!(plugins.tree("panelist", &panel).is_some());
    let again = plugins.run("panel.open", Args::new(), &mut app.context());
    assert_eq!(again.unwrap_err().code, Code::AlreadyRegistered);
    // A call that closes it and returns: its tree goes, and it is told.
    plugins
        .run("panel.close", Args::new(), &mut app.context())
        .unwrap();
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*closed.borrow(), 1);
    assert!(plugins.tree("panelist", &panel).is_none());
}

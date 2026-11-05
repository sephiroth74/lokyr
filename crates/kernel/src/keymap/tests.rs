//! The keymap's bindings, their contexts and sequences, as keys resolve.

use lokyr_vt_core::input::ModifierKey;

use super::*;

fn mods(cmd: bool, ctrl: bool, alt: bool, shift: bool) -> Mods {
    Mods {
        cmd,
        ctrl,
        alt,
        shift,
    }
}

fn combo(text: &str) -> Combo {
    Combo::parse(text).unwrap()
}

fn binding(keys: &str, command: &str, when: Option<&str>) -> Binding {
    Binding {
        keys: parse_keys(keys).unwrap(),
        invocation: Invocation {
            command: command.to_owned(),
            args: Args::new(),
        },
        when: when.map(|when| When::parse(when).unwrap()),
    }
}

fn run(command: &str) -> Resolved {
    Resolved::Run(Invocation {
        command: command.to_owned(),
        args: Args::new(),
    })
}

/// The key that types `character` with `mods`, on the US layout.
fn key(character: char, mods: Mods) -> Pressed {
    Pressed::key(Key::Char(character), mods, Some(character)).unwrap()
}

const NONE: Mods = Mods {
    cmd: false,
    ctrl: false,
    alt: false,
    shift: false,
};

const CMD: Mods = Mods {
    cmd: true,
    ctrl: false,
    alt: false,
    shift: false,
};

const CTRL: Mods = Mods {
    cmd: false,
    ctrl: true,
    alt: false,
    shift: false,
};

#[test]
fn a_combination_names_its_modifiers_and_its_key() {
    let cases = [
        (
            "cmd+shift+]",
            mods(true, false, false, true),
            Press::Char(']'),
        ),
        ("ctrl++", mods(false, true, false, false), Press::Char('+')),
        ("+", NONE, Press::Char('+')),
        ("opt+K", mods(false, false, true, false), Press::Char('k')),
        (
            "command+option+control+x",
            mods(true, true, true, false),
            Press::Char('x'),
        ),
        ("enter", NONE, Press::Named(Named::Enter)),
        ("esc", NONE, Press::Named(Named::Escape)),
        (
            "shift+F5",
            mods(false, false, false, true),
            Press::Named(Named::F(5)),
        ),
        (
            "alt+pageup",
            mods(false, false, true, false),
            Press::Named(Named::PageUp),
        ),
        ("cmd+wheel-up", CMD, Press::Wheel(Direction::Up)),
    ];
    for (text, mods, press) in cases {
        assert_eq!(Combo::parse(text), Ok(Combo { mods, press }), "{text}");
    }
    assert_eq!(combo("cmd+shift+]").to_string(), "cmd+shift+]");
    assert_eq!(combo("ctrl+esc").to_string(), "ctrl+escape");
    assert_eq!(combo("f12").to_string(), "f12");
    assert_eq!(combo("cmd+wheel-down").to_string(), "cmd+wheel-down");
    for text in [
        "hyper+a",
        "cmd+cmd+a",
        "cmd+",
        "cmd+foo",
        "f36",
        "f0",
        "",
        "Cmd+a",
        "cmd+ ",
    ] {
        assert!(Combo::parse(text).is_err(), "{text}");
    }
}

#[test]
fn keys_make_a_sequence_with_the_wheel_alone() {
    assert_eq!(
        parse_keys("ctrl+a>c"),
        Ok(vec![combo("ctrl+a"), combo("c")])
    );
    // `>` as a key, after a `+` or a `>`.
    assert_eq!(parse_keys("shift+>"), Ok(vec![combo("shift+>")]));
    assert_eq!(
        parse_keys("ctrl+a>>"),
        Ok(vec![combo("ctrl+a"), combo(">")])
    );
    assert_eq!(parse_keys(">"), Ok(vec![combo(">")]));
    assert!(parse_keys("cmd+wheel-up").is_ok());
    assert!(parse_keys("ctrl+a>wheel-up").is_err());
    assert!(parse_keys("ctrl+a>").is_err());
}

#[test]
fn a_when_holds_in_its_contexts() {
    let context = |names: &[&str]| Context::new(names.iter().copied());
    let when = When::parse("pane.focused && !selection").unwrap();
    assert!((context(&["pane.focused"])).holds_when(&when));
    assert!(!(context(&["pane.focused", "selection"])).holds_when(&when));
    let when = When::parse("selection || pane.focused && alternate-screen").unwrap();
    assert!((context(&["selection"])).holds_when(&when));
    assert!(
        !(context(&["pane.focused"])).holds_when(&when),
        "`&&` binds tighter"
    );
    let when = When::parse("!(selection || alternate-screen)").unwrap();
    assert!((context(&[])).holds_when(&when));
    assert!(!(context(&["alternate-screen"])).holds_when(&when));
    // The plugins' panels' contexts, of their namespaces.
    let when = When::parse("pane.focused && !search.open || search-plus.focused").unwrap();
    assert!((context(&["pane.focused"])).holds_when(&when));
    assert!(!(context(&["pane.focused", "search.open"])).holds_when(&when));
    assert!((context(&["search.open", "search-plus.focused"])).holds_when(&when));
    for text in [
        "focus",
        "pane.open",
        "pane.focsued",
        "search.shown",
        "search.open.now",
        "search",
        "9search.open",
        "selection.open",
        "selection & pane.focused",
        "(selection",
        "selection &&",
        "selection selection",
        "",
        "Selection",
    ] {
        assert!(When::parse(text).is_err(), "{text}");
    }
}

#[test]
fn a_when_nests_32_levels_at_most() {
    let nested = |levels: usize, open: &str, close: &str| {
        format!("{}selection{}", open.repeat(levels), close.repeat(levels))
    };
    for (open, close) in [("!", ""), ("(", ")"), ("!(", ")")] {
        assert!(When::parse(&nested(16, open, close)).is_ok(), "{open}");
        let error = When::parse(&nested(100_000, open, close)).unwrap_err();
        assert!(error.contains("deeper than 32"), "{error}");
    }
    assert!(When::parse(&nested(32, "!", "")).is_ok());
    assert!(When::parse(&nested(33, "!", "")).is_err());
    // A chain of operands nests too.
    let chain = |count: usize| vec!["selection"; count].join(" && ");
    assert!(When::parse(&chain(32)).is_ok());
    assert!(When::parse(&chain(100_000)).is_err());
    let either = vec!["selection"; 100_000].join(" || ");
    assert!(When::parse(&either).is_err());
}

#[test]
fn the_last_binding_that_holds_wins() {
    let mut keymap = Keymap::new(vec![
        binding("cmd+k", "view.clear", None),
        binding("cmd+c", "pane.copy", Some("selection")),
    ]);
    keymap.bind(binding("cmd+k", "view.reset", Some("alternate-screen")));
    let mut resolver = Resolver::new();
    let plain = Context::new(["pane.focused"]);
    let alternate = Context::new(["pane.focused", "alternate-screen"]);
    assert_eq!(
        resolver.press(&keymap, key('k', CMD), &plain),
        run("view.clear")
    );
    assert_eq!(
        resolver.press(&keymap, key('k', CMD), &alternate),
        run("view.reset")
    );
    // Without a selection, Command-C goes to the program.
    assert_eq!(
        resolver.press(&keymap, key('c', CMD), &plain),
        Resolved::Program { before: 0 }
    );
    let selected = Context::new(["selection"]);
    assert_eq!(
        resolver.press(&keymap, key('c', CMD), &selected),
        run("pane.copy")
    );
    // Another modifier, another combination.
    assert_eq!(
        resolver.press(&keymap, key('k', CTRL), &plain),
        Resolved::Program { before: 0 }
    );
    keymap.unbind(&parse_keys("cmd+k").unwrap());
    assert_eq!(
        resolver.press(&keymap, key('k', CMD), &alternate),
        Resolved::Program { before: 0 }
    );
    assert_eq!(keymap.bindings().len(), 1);
}

#[test]
fn a_sequence_waits_for_its_next_key_or_gives_its_keys_to_the_program() {
    let keymap = Keymap::new(vec![
        binding("ctrl+a>c", "tabs.new", None),
        binding("ctrl+a>ctrl+a", "tabs.last", None),
    ]);
    let mut resolver = Resolver::new();
    let context = Context::default();
    assert_eq!(
        resolver.press(&keymap, key('a', CTRL), &context),
        Resolved::Pending
    );
    assert!(resolver.is_pending());
    assert_eq!(
        resolver.press(&keymap, key('c', NONE), &context),
        run("tabs.new")
    );
    assert!(!resolver.is_pending());
    assert_eq!(
        resolver.press(&keymap, key('a', CTRL), &context),
        Resolved::Pending
    );
    assert_eq!(
        resolver.press(&keymap, key('a', CTRL), &context),
        run("tabs.last")
    );
    // A key that goes on with no sequence: both go to the program.
    resolver.press(&keymap, key('a', CTRL), &context);
    assert_eq!(
        resolver.press(&keymap, key('e', NONE), &context),
        Resolved::Program { before: 1 }
    );
    assert!(!resolver.is_pending());
}

#[test]
fn a_key_takes_the_us_layout_s_binding_when_its_own_has_none() {
    // The Italian layout's `è`, at the US layout's `[`, and its `+`, at `]`.
    let keymap = Keymap::new(vec![
        binding("cmd+[", "tabs.prev", None),
        binding("cmd+]", "tabs.next", None),
        binding("cmd++", "font.increase", None),
    ]);
    let mut resolver = Resolver::new();
    let context = Context::default();
    let e_grave = Pressed::key(Key::Char('è'), CMD, Some('[')).unwrap();
    assert_eq!(resolver.press(&keymap, e_grave, &context), run("tabs.prev"));
    // The layout's own character first.
    let plus = Pressed::key(Key::Char('+'), CMD, Some(']')).unwrap();
    assert_eq!(
        resolver.press(&keymap, plus, &context),
        run("font.increase")
    );
    let unbound = Keymap::new(vec![binding("cmd+]", "tabs.next", None)]);
    assert_eq!(resolver.press(&unbound, plus, &context), run("tabs.next"));
    // Capitals and the space take their bindings' names.
    let capital = Pressed::key(Key::Char('K'), CMD, Some('k')).unwrap();
    assert_eq!(capital.press, Press::Char('k'));
    assert_eq!(capital.us, None);
    let space = Pressed::key(Key::Char(' '), CTRL, Some(' ')).unwrap();
    assert_eq!(space.press, Press::Named(Named::Space));
    // A modifier alone takes no binding.
    assert_eq!(
        Pressed::key(Key::Modifier(ModifierKey::LeftShift), NONE, None),
        None
    );
}

#[test]
fn a_letter_or_a_digit_takes_no_binding_of_the_us_layout_s_key_at_its_place() {
    let keymap = Keymap::new(vec![
        binding("cmd+q", "app.quit", None),
        binding("cmd+w", "window.close", None),
        binding("cmd+1", "tabs.goto", None),
        binding("cmd+[", "tabs.prev", None),
        binding("cmd+]", "tabs.next", None),
        binding("cmd+;", "view.clear", None),
    ]);
    let mut resolver = Resolver::new();
    let context = Context::default();
    let program = Resolved::Program { before: 0 };
    // AZERTY's `a` and `z`, at the US layout's `q` and `w`; Dvorak's `x` at
    // `b`, and its `s` at `;`; Colemak's `f` at `e`; Dvorak's `'` and
    // AZERTY's `&`, punctuation at a letter's and a digit's places.
    for (own, us) in [
        ('a', 'q'),
        ('z', 'w'),
        ('x', 'b'),
        ('s', ';'),
        ('f', 'e'),
        ('\'', 'q'),
        ('&', '1'),
    ] {
        let pressed = Pressed::key(Key::Char(own), CMD, Some(us)).unwrap();
        assert_eq!(pressed.us, None, "{own}");
        assert_eq!(resolver.press(&keymap, pressed, &context), program, "{own}");
    }
    // The Italian layout's `è`, at the US layout's `[`, its `+`, at `]`, and
    // the Russian one's `й`, at `q`, take the US layout's.
    for (own, us, command) in [
        ('è', '[', "tabs.prev"),
        ('+', ']', "tabs.next"),
        ('й', 'q', "app.quit"),
    ] {
        let pressed = Pressed::key(Key::Char(own), CMD, Some(us)).unwrap();
        assert_eq!(
            resolver.press(&keymap, pressed, &context),
            run(command),
            "{own}"
        );
    }
}

#[test]
fn the_wheel_takes_its_bindings() {
    let keymap = Keymap::new(vec![binding("cmd+wheel-up", "font.increase", None)]);
    let mut resolver = Resolver::new();
    let context = Context::default();
    assert_eq!(
        resolver.press(&keymap, Pressed::wheel(Wheel::Up, CMD), &context),
        run("font.increase")
    );
    assert_eq!(
        resolver.press(&keymap, Pressed::wheel(Wheel::Down, CMD), &context),
        Resolved::Program { before: 0 }
    );
    assert_eq!(
        resolver.press(&keymap, Pressed::wheel(Wheel::Up, NONE), &context),
        Resolved::Program { before: 0 }
    );
}

#[test]
fn a_click_binds_alone_with_its_modifiers() {
    let combos = parse_keys("cmd+click").unwrap();
    assert_eq!(combos.len(), 1);
    assert_eq!(combos[0].press, Press::Click);
    assert_eq!(combos[0].to_string(), "cmd+click");
    assert_eq!(parse_keys("CLICK").unwrap()[0].press, Press::Click);
    assert!(
        parse_keys("ctrl+a>click")
            .unwrap_err()
            .contains("a click takes no sequence")
    );
    assert!(parse_keys("cmd+click>a").is_err());
    // It runs from the keymap, with its modifiers only.
    let keymap = Keymap::new(vec![Binding {
        keys: parse_keys("cmd+click").unwrap(),
        invocation: Invocation {
            command: "links.open".to_owned(),
            args: Args::new(),
        },
        when: None,
    }]);
    let mut resolver = Resolver::new();
    let cmd = Mods {
        cmd: true,
        ..Mods::default()
    };
    let context = Context::new([]);
    assert!(matches!(
        resolver.press(&keymap, Pressed::click(cmd), &context),
        Resolved::Run(invocation) if invocation.command == "links.open"
    ));
    assert!(matches!(
        resolver.press(&keymap, Pressed::click(Mods::default()), &context),
        Resolved::Program { .. }
    ));
}

#[test]
fn a_menu_shows_a_command_s_first_key_that_no_later_binding_takes() {
    let binding = |keys: &str, command: &str, when: Option<&str>| Binding {
        keys: parse_keys(keys).unwrap(),
        invocation: Invocation {
            command: command.to_owned(),
            args: Args::new(),
        },
        when: when.map(|when| When::parse(when).unwrap()),
    };
    let keymap = Keymap::new(vec![
        binding("cmd+n", "window.new", None),
        binding("cmd+c", "pane.copy", Some("selection")),
        binding("cmd+shift+c", "pane.copy", None),
        binding("cmd+w", "window.close", None),
        binding("ctrl+a>c", "tabs.new", None),
        binding("cmd+wheel-up", "font.bigger", None),
        binding("cmd+=", "font.bigger", None),
        binding("cmd+w", "pane.clear", None),
        binding("cmd+k", "pane.clear", None),
        binding("ctrl+a", "pane.select-all", None),
        binding("ctrl+a>s", "panes.split", None),
    ]);
    let shown = |command| keymap.shortcut(command).map(|combo| combo.to_string());
    assert_eq!(shown("window.new").as_deref(), Some("cmd+n"));
    // Not the key of a context alone.
    assert_eq!(shown("pane.copy").as_deref(), Some("cmd+shift+c"));
    // Taken by a later binding.
    assert_eq!(shown("window.close"), None);
    assert_eq!(shown("pane.clear").as_deref(), Some("cmd+w"));
    // Neither a sequence, nor a step of the wheel.
    assert_eq!(shown("tabs.new"), None);
    assert_eq!(shown("font.bigger").as_deref(), Some("cmd+="));
    // Nor the first key of a later sequence.
    assert_eq!(shown("pane.select-all"), None);
    // A binding with arguments shows on no item.
    let mut args = Args::new();
    args.insert("text".to_owned(), Value::String("x".to_owned()));
    let keymap = Keymap::new(vec![Binding {
        invocation: Invocation {
            command: "pane.send-text".to_owned(),
            args,
        },
        ..binding("cmd+e", "pane.send-text", None)
    }]);
    assert_eq!(keymap.shortcut("pane.send-text"), None);
}

#[test]
fn a_global_key_binds_one_key_with_a_modifier_outside_the_windows_keymap() {
    assert_eq!(parse_global("ctrl+`"), Ok(combo("ctrl+`")));
    assert_eq!(parse_global("f12"), Ok(combo("f12")));
    assert_eq!(
        parse_global("cmd+shift+space"),
        Ok(combo("cmd+shift+space"))
    );
    for wrong in [
        "a",
        "shift+a",
        "enter",
        "ctrl+a>b",
        "ctrl+wheel-up",
        "cmd+click",
    ] {
        assert!(parse_global(wrong).is_err(), "{wrong}");
    }
    // A character by its key of the US layout, without Shift.
    assert_eq!(parse_global("ctrl+shift+`"), Ok(combo("ctrl+shift+`")));
    assert_eq!(parse_global("cmd+shift+P"), Ok(combo("cmd+shift+p")));
    let shifted = parse_global("ctrl+~").unwrap_err();
    assert!(shifted.contains("write `ctrl+shift+``"), "{shifted}");
    let plus = parse_global("ctrl++").unwrap_err();
    assert!(plus.contains("write `ctrl+shift+=`"), "{plus}");
    let other = parse_global("ctrl+è").unwrap_err();
    assert!(other.contains("no key of the US layout"), "{other}");
    let config = KeysConfig {
        bindings: [
            (
                "global:ctrl+`".to_owned(),
                KeyValue::Command("window.toggle-quick-terminal".to_owned()),
            ),
            (
                "global:f12".to_owned(),
                KeyValue::Binding(KeyBinding {
                    command: "pane.send-text".to_owned(),
                    args: [("text".to_owned(), ArgValue::String("ls".to_owned()))].into(),
                    when: None,
                }),
            ),
            (
                "cmd+n".to_owned(),
                KeyValue::Command("window.new".to_owned()),
            ),
        ]
        .into(),
    };
    let global = global_bindings(&config);
    assert_eq!(
        global
            .iter()
            .map(|(combo, invocation)| (combo.to_string(), invocation.command.as_str()))
            .collect::<Vec<_>>(),
        [
            ("ctrl+`".to_owned(), "window.toggle-quick-terminal"),
            ("f12".to_owned(), "pane.send-text"),
        ]
    );
    assert_eq!(
        global[1].1.args.get("text"),
        Some(&Value::String("ls".to_owned()))
    );
    // The windows' keymap has the others alone.
    let keymap = Keymap::with_config(Vec::new(), &config);
    assert_eq!(keymap.bindings(), [binding("cmd+n", "window.new", None)]);
}

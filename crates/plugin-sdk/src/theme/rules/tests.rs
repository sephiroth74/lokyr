use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::theme::{TokenValue, builtin};

/// The terminal's colors of a dark theme of a blue that reads.
fn dark_terminal() -> TerminalColors {
    builtin::dark().terminal
}

/// The terminal's colors of a light theme.
fn light_terminal() -> TerminalColors {
    builtin::light().terminal
}

#[test]
fn the_rules_read_and_are_those_of_every_token_once_each_of_what_it_colors() {
    let names: Vec<&str> = tokens().collect();
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), names.len());
    let sdk: BTreeSet<&str> = crate::theme::TOKENS.into_iter().collect();
    assert_eq!(unique, sdk);
    for (name, rule) in rules() {
        assert!(
            rule.description.ends_with('.'),
            "{name}: {}",
            rule.description
        );
    }
}

#[test]
fn every_rule_resolves_of_the_tokens_that_it_names_in_both_variants() {
    for terminal in [dark_terminal(), light_terminal()] {
        let given: BTreeMap<String, Given> = BTreeMap::new();
        let mut resolver = Resolver::new(rules(), &terminal, &given);
        for name in tokens() {
            assert!(resolver.token(name).is_ok(), "{name}");
        }
    }
    // And each token that a rule names has a rule.
    for (name, rule) in rules() {
        for dark in [false, true] {
            for token in rule.expr(dark).tokens() {
                assert!(super::rule(token).is_some(), "{name}: {token}");
            }
        }
    }
    assert!(is_dark(&dark_terminal()) && !is_dark(&light_terminal()));
}

#[test]
fn expressions_read_of_their_terms_and_functions() {
    let parsed = |text: &str| text.parse::<Expr>().unwrap();
    let token = |name: &str| Box::new(Expr::Token(name.to_owned()));
    assert_eq!(
        parsed("#ff000080"),
        Expr::Color(Rgba::of(ThemeColor {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 128
        }))
    );
    assert_eq!(parsed("transparent"), Expr::Color(Rgba::CLEAR));
    assert_eq!(parsed("background"), Expr::Terminal(Terminal::Background));
    assert_eq!(parsed("selection"), Expr::Terminal(Terminal::Selection));
    assert_eq!(parsed("bright-blue"), Expr::Terminal(Terminal::Palette(12)));
    assert_eq!(parsed("panel.bg"), *token("panel.bg"));
    assert_eq!(
        parsed("accent / 60% / 50%"),
        Expr::Opacity(Box::new(Expr::Opacity(token("accent"), 0.6)), 0.5)
    );
    assert_eq!(
        parsed("mix( text , background,38%)"),
        Expr::Mix(
            token("text"),
            Box::new(Expr::Terminal(Terminal::Background)),
            0.38
        )
    );
    assert_eq!(
        parsed("readable(green, on: panel.bg)"),
        Expr::Readable(
            Box::new(Expr::Terminal(Terminal::Palette(2))),
            token("panel.bg")
        )
    );
    assert_eq!(
        parsed("contrasting(accent)"),
        Expr::Contrasting(token("accent"))
    );
    assert_eq!(
        parsed("first(blue, foreground, on: background, at: 4.5)"),
        Expr::First(
            vec![
                Expr::Terminal(Terminal::Palette(4)),
                Expr::Terminal(Terminal::Foreground)
            ],
            Box::new(Expr::Terminal(Terminal::Background)),
            4.5
        )
    );
    // A share of a percent as its literal's.
    assert_eq!(parsed("text / 38%"), Expr::Opacity(token("text"), 0.38));
}

#[test]
fn an_expression_that_does_not_read_says_why() {
    let wrong = |text: &str| text.parse::<Expr>().unwrap_err().to_string();
    for (text, why) in [
        ("", "missing"),
        ("accent /", "opacity"),
        ("accent / 50", "opacity"),
        ("accent / 150%", "from 0% to 100%"),
        ("#12345", "#rrggbb"),
        ("#", "#rrggbb"),
        ("50%", "no color"),
        ("blend(a, b)", "no function"),
        ("mix(a, b)", "mix(X, Y, N%)"),
        ("mix(a, b, 50)", "mix(X, Y, N%)"),
        ("readable(a)", "readable(X, on: Y)"),
        ("contrasting(a, b)", "contrasting(X)"),
        ("first(a, on: b)", "first("),
        ("first(a, on: b, at: 30)", "first("),
        ("first(on: b, at: 3)", "first("),
        ("mix(a, b, 5%", "`,` or `)`"),
        ("accent accent", "goes on"),
        ("Accent", "no place"),
        ("accent?", "no place"),
        ("1.2.3", "no number"),
    ] {
        let told = wrong(text);
        assert!(told.contains(why), "{text}: {told}");
    }
}

#[test]
fn a_token_given_reaches_the_tokens_whose_rules_name_it_and_one_given_stays_its_own() {
    let terminal = dark_terminal();
    let pink = ThemeColor::hex(0xc678dd);
    let given = BTreeMap::from([("accent".to_owned(), TokenValue::Color(pink))]);
    let resolved = resolve(&terminal, &given);
    let accent = Rgba::of(pink);
    assert_eq!(resolved["accent"], accent);
    assert_eq!(resolved["focus.ring"], accent.with_alpha(0.6));
    assert_eq!(resolved["list.selection.bg"], accent);
    assert_eq!(resolved["list.selection.fg"], contrasting(accent));
    assert_eq!(resolved["gutter.running"], accent);
    // Given too, it stays the theme's.
    let green = ThemeColor::hex(0x00ff00);
    let given = BTreeMap::from([
        ("accent".to_owned(), TokenValue::Color(pink)),
        ("focus.ring".to_owned(), TokenValue::Color(green)),
    ]);
    let resolved = resolve(&terminal, &given);
    assert_eq!(resolved["focus.ring"], Rgba::of(green));
    assert_eq!(resolved["field.focus"], accent.with_alpha(0.6));
    // An opacity is the color's at that opacity, whatever its own.
    let given = BTreeMap::from([(
        "accent".to_owned(),
        TokenValue::Color(pink.with_alpha(0x80)),
    )]);
    let resolved = resolve(&terminal, &given);
    assert_eq!(resolved["focus.ring"], accent.with_alpha(0.6));
    // A plugin's token, of no rule, kept; every token resolved.
    let given = BTreeMap::from([("command-header.pill".to_owned(), TokenValue::Color(green))]);
    let resolved = resolve(&terminal, &given);
    assert_eq!(resolved["command-header.pill"], Rgba::of(green));
    assert_eq!(resolved.len(), crate::theme::TOKENS.len() + 1);
}

#[test]
fn the_terminal_s_names_take_its_colors_and_their_defaults() {
    let mut terminal = dark_terminal();
    terminal.cursor = None;
    terminal.selection = None;
    let given: BTreeMap<String, Given> = BTreeMap::new();
    let mut resolver = Resolver::new(rules(), &terminal, &given);
    let mut eval = |text: &str| resolver.eval(&text.parse().unwrap()).unwrap();
    assert_eq!(eval("cursor"), Rgba::of(terminal.foreground));
    assert_eq!(eval("selection"), eval("field.selection"));
    assert_eq!(eval("bright-white"), Rgba::of(terminal.palette[15]));
    terminal.cursor = Some(ThemeColor::hex(0xff0000));
    terminal.selection = Some(ThemeColor::hex(0x00ff00));
    let mut resolver = Resolver::new(rules(), &terminal, &given);
    let mut eval = |text: &str| resolver.eval(&text.parse().unwrap()).unwrap();
    assert_eq!(eval("cursor"), Rgba::of(ThemeColor::hex(0xff0000)));
    assert_eq!(eval("selection"), Rgba::of(ThemeColor::hex(0x00ff00)));
}

#[test]
fn first_takes_the_first_that_reads_else_the_last() {
    let terminal = dark_terminal();
    let given: BTreeMap<String, Given> = BTreeMap::new();
    let mut resolver = Resolver::new(rules(), &terminal, &given);
    let mut eval = |text: &str| resolver.eval(&text.parse().unwrap()).unwrap();
    let black = Rgba::of(ThemeColor::hex(0x000000));
    let white = Rgba::of(ThemeColor::hex(0xffffff));
    assert_eq!(eval("first(#000000, #ffffff, on: #000000, at: 3)"), white);
    assert_eq!(eval("first(#ffffff, #000000, on: #000000, at: 3)"), white);
    assert_eq!(eval("first(#101010, #000000, on: #000000, at: 3)"), black);
}

#[test]
fn a_name_of_no_color_and_a_cycle_do_not_resolve() {
    let rules = read_rules("\"a\" = \"b / 50%\"\n\"b\" = { light = \"a\", dark = \"mix(a, c, 10%)\" }\n\"c\" = \"nothing\"\n").unwrap();
    let terminal = dark_terminal();
    let given: BTreeMap<String, Given> = BTreeMap::new();
    let mut resolver = Resolver::new(&rules, &terminal, &given);
    assert_eq!(
        resolver.token("a"),
        Err(Unresolved::Cycle("`a` → `b` → `a`".to_owned()))
    );
    assert_eq!(
        resolver.token("c"),
        Err(Unresolved::Unknown("nothing".to_owned()))
    );
    assert_eq!(
        resolver.token("zz"),
        Err(Unresolved::Unknown("zz".to_owned()))
    );
    // A given color breaks the cycle.
    let given = BTreeMap::from([(
        "b".to_owned(),
        Given::Color(Rgba::of(ThemeColor::hex(0x336699))),
    )]);
    let mut resolver = Resolver::new(&rules, &terminal, &given);
    assert_eq!(
        resolver.token("a"),
        Ok(Rgba::of(ThemeColor::hex(0x336699)).with_alpha(0.5))
    );
}

#[test]
fn a_rules_file_that_does_not_read_tells_its_line() {
    for (text, why) in [
        ("a = \"x\"", ":1: a rule is"),
        ("# c\n\"a\" \"x\"", ":2: `=` is missing"),
        ("\"a\" = x", "quoted expression"),
        ("\"a\" = \"x\" \"y\"", "one expression"),
        ("\"a\" = { dark = \"x\", light = \"y\" }", "comes first"),
        ("\"a\" = { light = \"x\" dark = \"y\" }", "`,` is missing"),
        ("\"a\" = { light = \"x\", dark = \"y\"", "`}` is missing"),
        (
            "\"a\" = { light = \"x\", dark = \"y\", z = \"w\" }",
            "alone",
        ),
        ("\"a\" = \"x\"\n\"a\" = \"y\"", "has a rule already"),
        ("\"a\" = \"mix(\"", ":1:"),
    ] {
        let told = read_rules(text).unwrap_err();
        assert!(told.contains(why), "{text}: {told}");
    }
    let read = read_rules(
        "# The header.\n\n# What a\n# colors.\n\"a\" = { light = \"x\", dark = \"y / 5%\" }\n\"b\" = \"x\"\n",
    )
    .unwrap();
    assert_eq!(
        (read[0].1.text(false), read[0].1.text(true)),
        ("x", "y / 5%")
    );
    // The comment right above a rule, its description; none of a header
    // past a blank line, nor of the rule before.
    assert_eq!(read[0].1.description, "What a colors.");
    assert_eq!(read[1].1.description, "");
}

#[test]
fn a_theme_s_expressions_resolve_and_those_that_do_not_take_their_rules_told() {
    let terminal = dark_terminal();
    let value = |text: &str| TokenValue::Expr(text.to_owned());
    let given = BTreeMap::from([
        (
            "accent".to_owned(),
            TokenValue::Color(ThemeColor::hex(0xc678dd)),
        ),
        ("focus.ring".to_owned(), value("accent / 40%")),
        (
            "tooltip.bg".to_owned(),
            value("mix(background, foreground, 8%)"),
        ),
        // A plugin's token of an expression, of no rule.
        ("command-header.pill".to_owned(), value("panel.bg")),
    ]);
    let resolved = resolve(&terminal, &given);
    let accent = Rgba::of(ThemeColor::hex(0xc678dd));
    assert_eq!(resolved["focus.ring"], accent.with_alpha(0.4));
    let (bg, fg) = (Rgba::of(terminal.background), Rgba::of(terminal.foreground));
    assert_eq!(resolved["tooltip.bg"], mix(bg, fg, 0.08));
    assert_eq!(resolved["command-header.pill"], resolved["panel.bg"]);
    assert!(problems(&terminal, &given).is_empty());
    // A name of no color, a cycle through a rule, a cycle of two given,
    // and an expression that does not read: each told, its rule kept.
    let given = BTreeMap::from([
        ("separator".to_owned(), value("nothing / 10%")),
        ("panel.bg".to_owned(), value("text.secondary")),
        ("button.bg".to_owned(), value("button.hover.bg")),
        ("button.hover.bg".to_owned(), value("button.bg / 50%")),
        ("scrollbar".to_owned(), value("mix(text")),
        ("pane.divider".to_owned(), value("separator")),
    ]);
    let told = problems(&terminal, &given);
    let names: BTreeSet<&str> = told.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        BTreeSet::from(["separator", "panel.bg", "button.hover.bg", "scrollbar"]),
        "{told:?}"
    );
    let why = |name: &str| &told.iter().find(|(told, _)| told == name).unwrap().1;
    assert_eq!(why("separator"), &Unresolved::Unknown("nothing".to_owned()));
    assert_eq!(
        why("panel.bg"),
        &Unresolved::Cycle("`panel.bg` → `text.secondary` → `panel.bg`".to_owned())
    );
    assert_eq!(
        why("button.hover.bg"),
        &Unresolved::Cycle("`button.bg` → `button.hover.bg` → `button.bg`".to_owned())
    );
    assert!(matches!(why("scrollbar"), Unresolved::Unreadable(_)));
    let resolved = resolve(&terminal, &given);
    let rules_alone = resolve(&terminal, &BTreeMap::new());
    for name in ["separator", "panel.bg", "button.hover.bg", "scrollbar"] {
        assert_eq!(resolved[name], rules_alone[name], "{name}");
    }
    // The one of the cycle of two that stays, its own, the innermost
    // told; one that names a token told keeps naming it, of its rule.
    assert_eq!(resolved["button.bg"], rules_alone["button.hover.bg"]);
    assert_eq!(resolved["pane.divider"], rules_alone["separator"]);
    assert_eq!(resolved.len(), crate::theme::TOKENS.len());
    for (name, why) in &told {
        assert!(!why.to_string().is_empty(), "{name}");
    }
}

#[test]
fn a_token_s_value_reads_as_a_color_or_else_an_expression() {
    assert_eq!(
        "#336699".parse::<TokenValue>(),
        Ok(TokenValue::Color(ThemeColor::hex(0x336699)))
    );
    assert_eq!(
        " accent / 40% ".parse::<TokenValue>(),
        Ok(TokenValue::Expr("accent / 40%".to_owned()))
    );
    assert!("#33669".parse::<TokenValue>().is_err());
    assert!("mix(a".parse::<TokenValue>().is_err());
    assert_eq!(
        TokenValue::Expr("accent / 40%".to_owned()).to_string(),
        "accent / 40%"
    );
    assert_eq!(
        TokenValue::Color(ThemeColor::hex(0x336699)).to_string(),
        "#336699"
    );
}

#[test]
fn an_expression_and_a_chain_of_tokens_are_bounded_of_no_overflow() {
    // Nested past the bound, or long past it: refused as they read.
    let nested = |depth: usize| {
        format!(
            "{}accent{}",
            "contrasting(".repeat(depth),
            ")".repeat(depth)
        )
    };
    assert!(nested(MOST_EXPR_DEPTH).parse::<Expr>().is_ok());
    let deep = nested(MOST_EXPR_DEPTH + 1).parse::<Expr>().unwrap_err();
    assert!(deep.0.contains("nest"), "{deep}");
    let long = nested(100_000).parse::<Expr>().unwrap_err();
    assert!(long.0.contains("bytes at most"), "{long}");
    // A chain of tokens each naming the next: within the bound, resolved;
    // past it, told, of no overflow, the rest of the theme resolved.
    let terminal = dark_terminal();
    let chain = |length: usize| -> BTreeMap<String, TokenValue> {
        (0..length)
            .map(|at| {
                let next = if at + 1 == length {
                    "accent".to_owned()
                } else {
                    format!("p.{:05}", at + 1)
                };
                (format!("p.{at:05}"), TokenValue::Expr(next))
            })
            .collect()
    };
    let short = chain(MOST_CHAIN - 8);
    assert!(problems(&terminal, &short).is_empty());
    assert_eq!(
        resolve(&terminal, &short)["p.00000"],
        resolve(&terminal, &BTreeMap::new())["accent"]
    );
    let long = chain(4_096);
    let started = std::time::Instant::now();
    let told = problems(&terminal, &long);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    assert!(
        told.iter()
            .any(|(_, why)| matches!(why, Unresolved::Deep(_))),
        "{told:?}"
    );
    assert_eq!(
        resolve(&terminal, &long)["accent"],
        resolve(&terminal, &BTreeMap::new())["accent"]
    );
    // Many tokens of a name of no color: each told, in a few passes.
    let unknown: BTreeMap<String, TokenValue> = (0..4_096)
        .map(|at| (format!("p.{at:05}"), TokenValue::Expr("zz".to_owned())))
        .collect();
    let started = std::time::Instant::now();
    assert_eq!(problems(&terminal, &unknown).len(), 4_096);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

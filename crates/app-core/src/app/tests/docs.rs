//! The commands' reference, `docs/commands.md`, and the keys of the
//! commands' tables of the bundled plugins' pages, made of the registry and
//! the default keymap of an app of the default distribution: written where
//! `LOKYR_WRITE_DOCS` is set, as `cargo xtask docs` sets it, else checked,
//! so that a changed command or binding fails until they are written again.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use lokyr_kernel::commands::{Category, Command, Kind, Value};
use lokyr_kernel::keymap::Binding;
use lokyr_kernel::keymap::{Combo, Direction, Named, Press, When};

use super::*;

/// The repository's `docs/`.
fn docs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

/// The categories, in the reference's order, each of its heading.
const CATEGORIES: [(Category, &str); 9] = [
    (Category::App, "App"),
    (Category::Windows, "Windows"),
    (Category::Tabs, "Tabs"),
    (Category::Panes, "Panes"),
    (Category::Input, "Input"),
    (Category::Clipboard, "Clipboard"),
    (Category::Edit, "Edit"),
    (Category::Find, "Find"),
    (Category::View, "View"),
];

/// The mark before a plugin's table of commands, of its plugin's name, as
/// `<!-- commands: lokyr/tabs -->`, and the mark after it.
const TABLE_START: &str = "<!-- commands: ";
const TABLE_END: &str = "<!-- commands end -->";

/// `text` in a span of code of a table's cell: between two backticks and a
/// space on each side where it holds a backtick, as `` `` cmd+` `` ``, and
/// its bars escaped, which would end the cell.
fn code(text: &str) -> String {
    let text = text.replace('|', "\\|");
    if text.contains('`') {
        format!("`` {text} ``")
    } else {
        format!("`{text}`")
    }
}

/// `character` as Markdown shows it outside a span of code: escaped where
/// it would start one, emphasis or a tag, or end a table's cell.
fn plain(character: char) -> String {
    match character {
        '`' | '|' | '\\' | '*' | '_' | '<' => format!("\\{character}"),
        _ => character.to_string(),
    }
}

/// A key of `combo`, as the pages write keys: macOS's symbols of its
/// modifiers, Control, Option, Shift then Command, and its key's name; a
/// character alone, or a letter of Shift alone, as it types, in a span of
/// code, as `` `g` `` and `` `G` ``; a click as `⌘-click`.
fn key(combo: &Combo) -> String {
    let mods = &combo.mods;
    let symbols: String = [
        (mods.ctrl, '⌃'),
        (mods.alt, '⌥'),
        (mods.shift, '⇧'),
        (mods.cmd, '⌘'),
    ]
    .iter()
    .filter(|(held, _)| *held)
    .map(|(_, symbol)| *symbol)
    .collect();
    let others = mods.ctrl || mods.alt || mods.cmd;
    match combo.press {
        Press::Char(character) if !others && !mods.shift => code(&character.to_string()),
        Press::Char(character) if !others && character.is_ascii_alphabetic() => {
            code(&character.to_ascii_uppercase().to_string())
        }
        Press::Char(character) => {
            let shown: String = character.to_uppercase().map(plain).collect();
            format!("{symbols}{shown}")
        }
        Press::Named(named) => format!("{symbols}{}", named_key(named)),
        Press::Click => format!("{symbols}-click"),
        Press::Wheel(direction) => {
            let way = match direction {
                Direction::Up => "up",
                Direction::Down => "down",
                Direction::Left => "left",
                Direction::Right => "right",
            };
            format!("{symbols} scroll {way}")
        }
    }
}

/// The name of a named key, as the pages write it.
fn named_key(named: Named) -> String {
    match named {
        Named::Enter => "Return".to_owned(),
        Named::Tab => "Tab".to_owned(),
        Named::Backspace => "Delete".to_owned(),
        Named::Escape => "Escape".to_owned(),
        Named::Space => "Space".to_owned(),
        Named::Delete => "Forward Delete".to_owned(),
        Named::Insert => "Help".to_owned(),
        Named::Home => "Home".to_owned(),
        Named::End => "End".to_owned(),
        Named::PageUp => "Page Up".to_owned(),
        Named::PageDown => "Page Down".to_owned(),
        Named::Up => "↑".to_owned(),
        Named::Down => "↓".to_owned(),
        Named::Left => "←".to_owned(),
        Named::Right => "→".to_owned(),
        Named::F(number) => format!("F{number}"),
    }
}

/// The keys of `binding`, as the pages write them, a sequence's apart.
fn keys_shown(binding: &Binding) -> String {
    binding.keys.iter().map(key).collect::<Vec<_>>().join(" ")
}

/// The keys of `binding`, as the config writes them, a sequence's parted
/// by `>`.
fn keys_written(binding: &Binding) -> String {
    binding
        .keys
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(">")
}

/// A value as the config writes it.
fn value(value: &Value) -> String {
    match value {
        Value::String(text) => format!("{text:?}"),
        Value::Integer(number) => number.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Boolean(on) => on.to_string(),
    }
}

/// A run of the bindings of a command: one, or several of the same context
/// whose arguments differ by one whole number alone, rising by one, as
/// ⌘1 to ⌘9 of `index`.
struct Run<'a> {
    bindings: Vec<&'a Binding>,
    /// The argument that rises, of a run of several.
    rising: Option<String>,
}

/// The runs of `bindings`, in their order: three or more whose arguments
/// differ by one whole number alone, rising by one, of the same context,
/// make one.
fn runs<'a>(bindings: &[&'a Binding]) -> Vec<Run<'a>> {
    // The argument that rises from `before` to `after`, if they differ in
    // it alone, by one.
    let rises = |before: &Binding, after: &Binding| -> Option<String> {
        if before.when != after.when || before.invocation.args.len() != after.invocation.args.len()
        {
            return None;
        }
        let mut rising = None;
        for (name, was) in &before.invocation.args {
            match (was, after.invocation.args.get(name)?) {
                (Value::Integer(was), Value::Integer(is)) if *is == was + 1 => {
                    if rising.replace(name.clone()).is_some() {
                        return None;
                    }
                }
                (was, is) if was == is => {}
                _ => return None,
            }
        }
        rising
    };
    let mut found: Vec<Run<'a>> = Vec::new();
    let mut at = 0;
    while at < bindings.len() {
        let mut end = at + 1;
        let mut rising = None;
        while end < bindings.len() {
            match rises(bindings[end - 1], bindings[end]) {
                Some(name) if rising.as_ref().is_none_or(|known| *known == name) => {
                    rising = Some(name);
                    end += 1;
                }
                _ => break,
            }
        }
        if end - at >= 3 {
            found.push(Run {
                bindings: bindings[at..end].to_vec(),
                rising,
            });
            at = end;
        } else {
            found.push(Run {
                bindings: vec![bindings[at]],
                rising: None,
            });
            at += 1;
        }
    }
    found
}

/// The default bindings of `command`, in the keymap's order.
fn bindings_of<'a>(app: &'a App, command: &str) -> Vec<&'a Binding> {
    app.keymap
        .bindings()
        .iter()
        .filter(|binding| binding.invocation.command == command)
        .collect()
}

/// The keys of `command` as the reference shows them: each binding's, as
/// the pages and as the config write them, its arguments and its context.
fn keys_in_full(bindings: &[&Binding]) -> String {
    let mut parts = Vec::new();
    for run in runs(bindings) {
        let first = run.bindings[0];
        let last = run.bindings[run.bindings.len() - 1];
        // The config's form, unless it is what the page shows, as `h`.
        let written = |binding: &Binding| {
            let written = keys_written(binding);
            (keys_shown(binding) != code(&written)).then_some(written)
        };
        let mut part = if run.bindings.len() > 1 {
            let shown = format!("{} to {}", keys_shown(first), keys_shown(last));
            match (written(first), written(last)) {
                (Some(from), Some(to)) => format!("{shown} ({} to {})", code(&from), code(&to)),
                _ => shown,
            }
        } else {
            match written(first) {
                Some(written) => format!("{} ({})", keys_shown(first), code(&written)),
                None => keys_shown(first),
            }
        };
        let args: Vec<String> = first
            .invocation
            .args
            .iter()
            .map(|(name, given)| match &run.rising {
                Some(rising) if rising == name => {
                    let end = last
                        .invocation
                        .args
                        .get(name)
                        .map(value)
                        .unwrap_or_default();
                    format!("`{name}` {} to {end}", value(given))
                }
                _ => format!("`{name} = {}`", value(given)),
            })
            .collect();
        if !args.is_empty() {
            let _ = write!(part, ", {}", args.join(", "));
        }
        if let Some(when) = &first.when {
            let _ = write!(part, ", while `{when}`");
        }
        parts.push(part);
    }
    parts.join("; ")
}

/// The keys of `bindings` as a plugin's page shows them: those of no
/// context, then those of each context, as `Tab, ⇧Tab while
/// `links.focused``, a run as ⌘1 to ⌘9.
fn keys_in_brief(bindings: &[&Binding]) -> String {
    let mut contexts: Vec<Option<String>> = Vec::new();
    for binding in bindings {
        let when = binding.when.as_ref().map(ToString::to_string);
        if !contexts.contains(&when) {
            contexts.push(when);
        }
    }
    contexts.sort_by_key(Option::is_some);
    let mut parts = Vec::new();
    for context in contexts {
        let of: Vec<&Binding> = bindings
            .iter()
            .copied()
            .filter(|binding| binding.when.as_ref().map(ToString::to_string) == context)
            .collect();
        let mut keys: Vec<String> = Vec::new();
        for run in runs(&of) {
            let shown = if run.bindings.len() > 1 {
                format!(
                    "{} to {}",
                    keys_shown(run.bindings[0]),
                    keys_shown(run.bindings[run.bindings.len() - 1])
                )
            } else {
                keys_shown(run.bindings[0])
            };
            if !keys.contains(&shown) {
                keys.push(shown);
            }
        }
        let mut part = keys.join(", ");
        if let Some(when) = context {
            let _ = write!(part, " while `{when}`");
        }
        parts.push(part);
    }
    parts.join("; ")
}

/// A command's parameters, as the reference shows them.
fn params(command: &Command) -> String {
    command
        .params
        .iter()
        .map(|param| {
            let kind = match param.kind {
                Kind::String => "text",
                Kind::Integer => "whole number",
                Kind::Number => "number",
                Kind::Boolean => "`true` or `false`",
            };
            match &param.default {
                Some(given) => format!("`{}`, {kind}, `{}` by default", param.name, value(given)),
                None => format!("`{}`, {kind}", param.name),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The page of the bundled plugin `plugin`, as `lokyr/tabs`, from
/// `docs/`, if it has one.
fn plugin_page(plugin: &str) -> Option<String> {
    let name = plugin.strip_prefix("lokyr/")?;
    let page = format!("plugins/{name}.md");
    docs().join(&page).is_file().then_some(page)
}

/// A table's cell of `text`, its pipes escaped.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The commands' reference of `app`.
fn reference(app: &App) -> String {
    let mut page = String::from(
        "# Commands\n\n<sub>macOS · Lokyr's commands and its bundled plugins'</sub>\n\n<!-- Made by `cargo xtask docs` of the app's commands and its default keys: change those, not this page. -->\n\nEvery action of Lokyr is a command, which a menu, a key, the [command palette](plugins/command-palette.md) or a plugin runs. This page lists every command of Lokyr and of its bundled plugins, with its arguments and the keys that run it by default, so that you can bind keys of your own.\n\n## Bind a command to keys\n\nIn your config, `[keys]` binds keys to a command, and to its arguments:\n\n```toml\n[keys]\n\"cmd+shift+t\" = \"tabs.new\"                                      # a second key for a new tab\n\"ctrl+cmd+1\" = { command = \"tabs.goto\", args = { index = 1 } }  # the first tab\n\"cmd+d\" = false                                                  # no split on ⌘D\n```\n\nYour keys win over the defaults below. The [keys' section of the configuration guide](CONFIGURATION.md#bindings) tells how to write keys, arguments and contexts.\n\nThe keys below show as macOS writes them, then as the config writes them. A key that holds in a context only, such as while the palette is open, says so with `while`.\n",
    );
    for (category, heading) in CATEGORIES {
        let mut names: Vec<&str> = app
            .commands
            .in_order()
            .filter(|command| command.category == category)
            .map(|command| command.name.as_str())
            .collect();
        if names.is_empty() {
            continue;
        }
        names.sort_unstable();
        let _ = write!(
            page,
            "\n## {heading}\n\n| Command | Title | Arguments | Default keys |\n|---|---|---|---|\n"
        );
        for name in names {
            let Some(command) = app.commands.get(name) else {
                continue;
            };
            let shown = match app.commands.owner(name).and_then(plugin_page) {
                Some(page) => format!("[`{name}`]({page})"),
                None => format!("`{name}`"),
            };
            let bindings = bindings_of(app, name);
            let _ = writeln!(
                page,
                "| {shown} | {} | {} | {} |",
                cell(&command.title),
                cell(&params(command)),
                cell(&keys_in_full(&bindings))
            );
        }
    }
    page.push_str("\n## See also\n\n- [The default shortcuts](plugins/keymap-macos.md)\n- [The configuration guide](CONFIGURATION.md#bindings), of the keys' section\n");
    page
}

/// The cells of a table's `row`, between its pipes, as written, of their
/// escaped pipes.
fn cells(row: &str) -> Vec<String> {
    let inner = row.trim().trim_start_matches('|').trim_end_matches('|');
    let mut found = Vec::new();
    let mut current = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\\' if chars.peek() == Some(&'|') => {
                current.push_str("\\|");
                chars.next();
            }
            '|' => found.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    found.push(current);
    found
}

/// The command names of a cell, each in its span of code, as
/// `` `tabs.next`, `tabs.prev` ``.
fn names_in(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .filter(|name| name.contains('.') && !name.contains(' '))
        .map(str::to_owned)
        .collect()
}

/// The page `text`, of `file`, of the keys of each table of commands
/// between its marks made anew of `app`: the column of the default keys of
/// each row, of the keys of the commands of its first cell; or the
/// problems of a table whose rows leave a command of its plugin out, or
/// name one that it has not.
fn plugin_tables(app: &App, file: &str, text: &str) -> Result<String, Vec<String>> {
    let mut made = String::new();
    let mut problems = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(TABLE_START) {
        let Some(close) = rest[start..].find("-->") else {
            break;
        };
        let plugin = rest[start + TABLE_START.len()..start + close]
            .trim()
            .to_owned();
        let Some(end) = rest[start..].find(TABLE_END) else {
            problems.push(format!(
                "{file}: the table of `{plugin}` has no `{TABLE_END}`"
            ));
            break;
        };
        let (before, table, after) = (
            &rest[..start + close + 3],
            &rest[start + close + 3..start + end],
            &rest[start + end..],
        );
        made.push_str(before);
        let mut listed: Vec<String> = Vec::new();
        let mut column = None;
        for (at, line) in table.split_inclusive('\n').enumerate() {
            if !line.trim_start().starts_with('|') {
                made.push_str(line);
                continue;
            }
            let mut row = cells(line);
            if column.is_none() {
                column = row
                    .iter()
                    .position(|cell| matches!(cell.trim(), "Default keys" | "Keys"));
                if column.is_none() {
                    problems.push(format!(
                        "{file}: the table of `{plugin}` has no column of `Default keys`"
                    ));
                }
                made.push_str(line);
                continue;
            }
            // The line of dashes under the header.
            if row
                .iter()
                .all(|cell| cell.trim().chars().all(|c| matches!(c, '-' | ':')))
            {
                made.push_str(line);
                continue;
            }
            let names = names_in(&row[0]);
            let mut keys = Vec::new();
            for name in &names {
                if app.commands.owner(name) != Some(plugin.as_str()) {
                    problems.push(format!(
                        "{file}: the table of `{plugin}` lists `{name}`, which `{plugin}` has not, at its row {at}"
                    ));
                    continue;
                }
                listed.push(name.clone());
                let shown = keys_in_brief(&bindings_of(app, name));
                if !shown.is_empty() {
                    keys.push(shown);
                }
            }
            if let Some(column) = column.filter(|&column| column < row.len()) {
                row[column] = if keys.is_empty() {
                    " ".to_owned()
                } else {
                    format!(" {} ", cell(&keys.join(", ")))
                };
            }
            let ending = if line.ends_with('\n') { "\n" } else { "" };
            let _ = write!(made, "|{}|{ending}", row.join("|"));
        }
        let mut owned: Vec<&str> = app
            .commands
            .in_order()
            .map(|command| command.name.as_str())
            .filter(|name| app.commands.owner(name) == Some(plugin.as_str()))
            .collect();
        owned.sort_unstable();
        for name in owned {
            if !listed.iter().any(|listed| listed == name) {
                problems.push(format!(
                    "{file}: the table of `{plugin}` has no row of `{name}`: add one, of what it does"
                ));
            }
        }
        rest = after;
    }
    made.push_str(rest);
    if problems.is_empty() {
        Ok(made)
    } else {
        Err(problems)
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_commands_reference_and_the_plugins_tables_are_the_app_s() {
    let (_fake, app) = started();
    let write = std::env::var_os("LOKYR_WRITE_DOCS").is_some();
    let mut stale = Vec::new();
    let mut problems = Vec::new();
    let mut check = |file: &str, made: String| {
        let path = docs().join(file);
        let held = std::fs::read_to_string(&path).unwrap_or_default();
        if held == made {
            return;
        }
        if write {
            std::fs::write(&path, made).unwrap();
        } else {
            stale.push(format!("docs/{file}"));
        }
    };
    check("commands.md", reference(&app));
    let mut pages: Vec<PathBuf> = std::fs::read_dir(docs().join("plugins"))
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    pages.sort();
    for path in pages {
        let text = std::fs::read_to_string(&path).unwrap();
        if !text.contains(TABLE_START) {
            continue;
        }
        let file = format!("plugins/{}", path.file_name().unwrap().to_string_lossy());
        match plugin_tables(&app, &file, &text) {
            Ok(made) => check(&file, made),
            Err(found) => problems.extend(found),
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        stale.is_empty(),
        "{} are not the app's commands and keys: `cargo xtask docs` writes them",
        stale.join(", ")
    );
}

#[test]
fn keys_show_as_the_pages_write_them_and_a_run_of_numbers_as_one() {
    let parse = |text: &str| lokyr_kernel::keymap::parse_keys(text).unwrap();
    let shown = |text: &str| parse(text).iter().map(key).collect::<Vec<_>>().join(" ");
    assert_eq!(shown("cmd+shift+p"), "⇧⌘P");
    assert_eq!(shown("ctrl+alt+shift+cmd+a"), "⌃⌥⇧⌘A");
    assert_eq!(shown("cmd+shift+enter"), "⇧⌘Return");
    assert_eq!(shown("cmd+pageup"), "⌘Page Up");
    assert_eq!(shown("h"), "`h`");
    assert_eq!(shown("shift+g"), "`G`");
    assert_eq!(shown("shift+6"), "⇧6");
    assert_eq!(shown("ctrl+v"), "⌃V");
    assert_eq!(shown("g>g"), "`g` `g`");
    assert_eq!(shown("cmd+click"), "⌘-click");
    assert_eq!(shown("shift+tab"), "⇧Tab");
    // What Markdown reads outside a span of code, escaped.
    assert_eq!(shown("cmd+`"), "⌘\\`");
    assert_eq!(shown("cmd+\\"), "⌘\\\\");
    assert_eq!(shown("`"), "`` ` ``");
    let binding = |keys: &str, args: &[(&str, i64)], when: Option<&str>| Binding {
        keys: parse(keys),
        invocation: lokyr_kernel::keymap::Invocation {
            command: "tabs.goto".to_owned(),
            args: args
                .iter()
                .map(|(name, value)| ((*name).to_owned(), Value::Integer(*value)))
                .collect(),
        },
        when: when.map(|when| When::parse(when).unwrap()),
    };
    let numbers: Vec<Binding> = (1..=9)
        .map(|index| binding(&format!("cmd+{index}"), &[("index", index)], None))
        .collect();
    let refs: Vec<&Binding> = numbers.iter().collect();
    assert_eq!(keys_in_brief(&refs), "⌘1 to ⌘9");
    assert_eq!(
        keys_in_full(&refs),
        "⌘1 to ⌘9 (`cmd+1` to `cmd+9`), `index` 1 to 9"
    );
    // Two are no run; a context says so.
    let moves = [
        binding("tab", &[("delta", 1)], Some("links.focused")),
        binding("shift+tab", &[("delta", -1)], Some("links.focused")),
    ];
    let refs: Vec<&Binding> = moves.iter().collect();
    assert_eq!(keys_in_brief(&refs), "Tab, ⇧Tab while `links.focused`");
    assert_eq!(
        keys_in_full(&refs),
        "Tab (`tab`), `delta = 1`, while `links.focused`; ⇧Tab (`shift+tab`), `delta = -1`, while `links.focused`"
    );
    // A backtick in the config's form, between two backticks.
    let next = [binding("cmd+`", &[], None)];
    let refs: Vec<&Binding> = next.iter().collect();
    assert_eq!(keys_in_full(&refs), "⌘\\` (`` cmd+` ``)");
    assert_eq!(code("a|b"), "`a\\|b`");
}

#[test]
fn a_plugin_s_table_takes_its_keys_and_tells_a_row_left_out() {
    let (_fake, app) = started();
    let page = "Intro.\n\n<!-- commands: lokyr/tabs -->\n| Command | Menu | Default keys | What it does |\n|---|---|---|---|\n| `tabs.new` | Shell › New Tab | ⌘N | Opens a tab |\n| `tabs.next`, `tabs.prev` | | | Moves |\n<!-- commands end -->\n\nAfter.\n";
    let problems = plugin_tables(&app, "plugins/tabs.md", page).unwrap_err();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("has no row of `tabs.goto`")),
        "{problems:?}"
    );
    assert!(
        !problems
            .iter()
            .any(|problem| problem.contains("`tabs.new`"))
    );
    // A command of another plugin is told.
    let page = "<!-- commands: lokyr/tabs -->\n| Command | Default keys |\n|---|---|\n| `search.open` | |\n<!-- commands end -->\n";
    let problems = plugin_tables(&app, "x.md", page).unwrap_err();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("lists `search.open`"))
    );
}

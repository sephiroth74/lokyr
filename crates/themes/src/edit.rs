//! A theme's file of Lokyr's format edited in place: one of its terminal's
//! colors or of its tokens set or taken away, of `toml_edit`, the file's
//! comments, its order and its other keys kept.

use std::path::Path;

use lokyr_plugin_sdk::theme::{TERMINAL_KEYS, ThemeColor, TokenValue, rules};
use toml_edit::{Array, DocumentMut, Item, Table, TableLike, Value};

use crate::{Error, Theme, parse, quoted};

/// `text`, the file of Lokyr's format of a theme at `path`, of the color
/// `name` set to `value`, or taken away of none, as a read of the theme's
/// colors names them:
///
/// - a key of `[terminal]`, a color;
/// - a color of its palette, by its name, as `bright-red`, a color, into
///   its `palette`, written whole of `theme`'s 16 where the file has none;
/// - a token, a color or an expression, where the file writes it: in its
///   section, as `active.bg` of `[tabs]`, `[search]`'s `match` and
///   `current`, or `[interface]`; else into `[interface]`, made at the
///   file's end where there is none, its rule's comment there taken away.
///
/// The file's comments and order kept, a value's comment too. A value
/// that does not read, a palette's color taken away alone, a section that
/// is no table, or a text that no longer reads as a theme, refused.
pub fn set_value(
    text: &str,
    path: &Path,
    name: &str,
    value: Option<&str>,
    theme: &Theme,
) -> Result<String, Error> {
    let error = |message: String| Error {
        path: path.to_owned(),
        position: None,
        message,
    };
    if value.is_none() {
        removable(text, path, name)?;
    }
    let mut document: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| error(e.message().to_owned()))?;
    let palette = rules::PALETTE.iter().position(|color| *color == name);
    if TERMINAL_KEYS.contains(&name) || palette.is_some() {
        if let Some(value) = value {
            value
                .parse::<ThemeColor>()
                .map_err(|e| error(format!("`{name}` is a color: {e}")))?;
        }
        let terminal = section(&mut document, "terminal").map_err(error)?;
        match (palette, value) {
            (None, Some(value)) => set(terminal, name, value),
            (None, None) => {
                remove_keeping(&mut document, "terminal", name);
            }
            (Some(at), Some(value)) => set_palette(terminal, at, value, &theme.terminal.palette),
            (Some(_), None) => {
                return Err(error(format!(
                    "`{name}` is a color of the palette, which holds the 16 whole: none is taken away alone"
                )));
            }
        }
    } else {
        if let Some(value) = value {
            value
                .parse::<TokenValue>()
                .map_err(|e| error(format!("`{name}` is left out, as {e}")))?;
        }
        let places = token_places(name);
        let mut found = false;
        for (section, keys) in &places {
            // A key of the section itself taken away, the comments above it
            // kept.
            let joined = keys.join(".");
            let direct = document
                .get(section.as_str())
                .and_then(Item::as_table)
                .and_then(|table| table.get(&joined))
                .is_some_and(|item| item.as_value().is_some());
            if value.is_none() && direct {
                found |= remove_keeping(&mut document, section, &joined);
                continue;
            }
            if let Some(table) = document
                .get_mut(section.as_str())
                .and_then(Item::as_table_like_mut)
            {
                found |= set_at(table, keys, value);
            }
        }
        // A new one in `[interface]`, of its whole name quoted, as the
        // section writes its names.
        if let (false, Some(value)) = (found, value) {
            let interface = section(&mut document, "interface").map_err(error)?;
            let key = toml_edit::Key::parse(&quoted(name))
                .ok()
                .and_then(|keys| keys.into_iter().next())
                .unwrap_or_else(|| toml_edit::Key::new(name));
            interface.insert_formatted(&key, toml_edit::value(value));
        }
    }
    let mut written = document.to_string();
    // A token given: its rule's comment, as a new theme writes it, goes.
    if let (false, Some(_)) = (TERMINAL_KEYS.contains(&name) || palette.is_some(), value) {
        let comment = format!("# {} = ", quoted(name));
        if written.lines().any(|line| line.starts_with(&comment)) {
            written = written
                .split_inclusive('\n')
                .filter(|line| !line.starts_with(&comment))
                .collect();
        }
    }
    parse(&written, path)?;
    Ok(written)
}

/// The keys of `[terminal]` that a theme gives, of its own or of the theme
/// that it extends.
const NEEDED: [&str; 2] = ["background", "foreground"];

/// Whether the color `name` can be taken away of `text`, the file of
/// Lokyr's format of a theme at `path`: one of the colors that a theme
/// gives, its background and its text, of a file that extends no theme,
/// nor a color of its palette, which holds the 16 whole, never; why not.
pub fn removable(text: &str, path: &Path, name: &str) -> Result<(), Error> {
    let error = |message: String| Error {
        path: path.to_owned(),
        position: None,
        message,
    };
    if rules::PALETTE.contains(&name) {
        return Err(error(format!(
            "`{name}` is a color of the palette, which holds the 16 whole: none is taken away alone"
        )));
    }
    if NEEDED.contains(&name) && parse(text, path)?.extends.is_none() {
        return Err(error(format!(
            "`{name}` is a color that a theme gives, of its own or of a theme that it extends, which this one does not: none would take its place"
        )));
    }
    Ok(())
}

/// The places where a file can write the token `name`: a section and the
/// parts of the name within it, `[interface]`'s of the whole name, its own
/// section's of the rest of it, and `[search]`'s two; each part a table of
/// the rest, or the rest one key, as [`set_at`] finds them.
fn token_places(name: &str) -> Vec<(String, Vec<String>)> {
    let parts: Vec<String> = name.split('.').map(str::to_owned).collect();
    let mut places = vec![("interface".to_owned(), parts)];
    let search = match name {
        "search.match" => Some("match"),
        "search.match.current" => Some("current"),
        _ => None,
    };
    if let Some(key) = search {
        places.push(("search".to_owned(), vec![key.to_owned()]));
        return places;
    }
    let mut parts = name.split('.');
    let section = parts.next().unwrap_or_default();
    let keys: Vec<String> = parts.map(str::to_owned).collect();
    // The file's other sections hold no tokens.
    let other = ["theme", "terminal", "interface", "search"].contains(&section);
    if !keys.is_empty() && !other {
        places.push((section.to_owned(), keys));
    }
    places
}

/// Sets the value at `keys` of `table`, or takes it away of none: the keys
/// joined as one key of `table`, as `"mark.current"`, else the first a table
/// of the rest, as a dotted key's, the tables that a removal empties gone;
/// whether the table held it.
fn set_at(table: &mut dyn TableLike, keys: &[String], value: Option<&str>) -> bool {
    let joined = keys.join(".");
    if table
        .get(&joined)
        .is_some_and(|item| item.as_value().is_some())
    {
        match value {
            Some(value) => set_like(table, &joined, value),
            None => {
                table.remove(&joined);
            }
        }
        return true;
    }
    let Some((first, rest)) = keys.split_first() else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let Some(inner) = table.get_mut(first).and_then(Item::as_table_like_mut) else {
        return false;
    };
    let held = set_at(inner, rest, value);
    if held && value.is_none() && inner.is_empty() {
        table.remove(first);
    }
    held
}

/// Takes the key `key` away of the section `section` of `document`, the
/// lines of comments above it kept: before the next key of the section, or
/// past the section, before the next one or at the file's end; whether the
/// section held it.
fn remove_keeping(document: &mut DocumentMut, section: &str, key: &str) -> bool {
    let Some(table) = document.get_mut(section).and_then(Item::as_table_mut) else {
        return false;
    };
    if table.get(key).is_none() {
        return false;
    }
    let above = table
        .key(key)
        .and_then(|own| own.leaf_decor().prefix())
        .and_then(|prefix| prefix.as_str())
        .unwrap_or_default()
        .to_owned();
    let keys: Vec<String> = table.iter().map(|(own, _)| own.to_owned()).collect();
    let next = keys
        .iter()
        .position(|own| own == key)
        .and_then(|at| keys.get(at + 1))
        .cloned();
    table.remove(key);
    if !above.contains('#') {
        return true;
    }
    let joined = |before: Option<&toml_edit::RawString>| {
        format!(
            "{above}{}",
            before.and_then(|raw| raw.as_str()).unwrap_or_default()
        )
    };
    if let Some(mut next) = next.and_then(|next| table.key_mut(&next)) {
        let prefix = joined(next.leaf_decor().prefix());
        next.leaf_decor_mut().set_prefix(prefix);
        return true;
    }
    // The section's last key: the next section's, by their places.
    let place = table.position();
    let following = document
        .iter()
        .filter_map(|(name, item)| Some((name.to_owned(), item.as_table()?.position()?)))
        .filter(|(_, at)| place.is_some_and(|place| *at > place))
        .min_by_key(|(_, at)| *at)
        .map(|(name, _)| name);
    match following.and_then(|name| document.get_mut(&name)?.as_table_mut()) {
        Some(table) => {
            let prefix = joined(table.decor().prefix());
            table.decor_mut().set_prefix(prefix);
        }
        None => {
            let trailing = format!(
                "{above}{}",
                document.trailing().as_str().unwrap_or_default()
            );
            document.set_trailing(trailing);
        }
    }
    true
}

/// The table of the section `name` of `document`, made at its end where
/// there is none; refused of a key of that name that is no table.
fn section<'d>(document: &'d mut DocumentMut, name: &str) -> Result<&'d mut Table, String> {
    document
        .entry(name)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("`{name}` is no section of the file"))
}

/// Sets `key` of `table` to the string `value`, its old value's comment and
/// spaces kept.
fn set(table: &mut Table, key: &str, value: &str) {
    set_like(table, key, value);
}

/// [`set`] of any table, inline too.
fn set_like(table: &mut dyn TableLike, key: &str, value: &str) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            let decor = old.decor().clone();
            *old = Value::from(value);
            *old.decor_mut() = decor;
        }
        None => {
            table.insert(key, toml_edit::value(value));
        }
    }
}

/// Sets the color `at` of `[terminal]`'s palette to `value`: in the file's
/// palette of 16, its comment and its layout kept, or in one written whole
/// of `colors`.
fn set_palette(terminal: &mut Table, at: usize, value: &str, colors: &[ThemeColor; 16]) {
    if let Some(array) = terminal
        .get_mut("palette")
        .and_then(Item::as_array_mut)
        .filter(|array| array.len() == colors.len())
    {
        array.replace(at, value);
        return;
    }
    let mut array: Array = colors.iter().map(ToString::to_string).collect();
    array.replace(at, value);
    terminal.insert("palette", toml_edit::value(array));
}

/// `theme` of the color `name` of `value` in place of its own, as a value
/// shown before it is written: a key of `[terminal]` or a color of its
/// palette, a color, or a token, a color or an expression; why not of a
/// value that does not read.
pub fn with_value(theme: &mut Theme, name: &str, value: &str) -> Result<(), String> {
    if !lokyr_plugin_sdk::theme::is_terminal_color(name) {
        let value = value
            .parse::<TokenValue>()
            .map_err(|e| format!("`{name}` is left out, as {e}"))?;
        theme.tokens.insert(name.to_owned(), value);
        return Ok(());
    }
    let color = value
        .parse::<ThemeColor>()
        .map_err(|e| format!("`{name}` is a color: {e}"))?;
    let terminal = &mut theme.terminal;
    match name {
        "background" => terminal.background = color,
        "foreground" => terminal.foreground = color,
        "cursor" => terminal.cursor = Some(color),
        "cursor-text" => terminal.cursor_text = Some(color),
        "selection" => terminal.selection = Some(color),
        "selection-text" => terminal.selection_text = Some(color),
        "bold" => terminal.bold = Some(color),
        "link" => terminal.link = Some(color),
        "underline" => terminal.underline = Some(color),
        _ => {
            if let Some(at) = rules::PALETTE.iter().position(|each| *each == name) {
                terminal.palette[at] = color;
            }
        }
    }
    Ok(())
}

//! Themes of the native format: TOML files in the themes' folder beside the
//! config, `themes/<name>.toml`, each a variant of the light appearance or
//! the dark one; iTerm2's `.itermcolors` files there, read as they are; the
//! themes that the app holds, native files of a folder of its own; and the
//! built-in themes. A file of the themes' folder takes the place of a held
//! or built-in theme of its name, a native one before an `.itermcolors`
//! one. A file's `[theme]` names it and the theme that it extends;
//! `[terminal]` gives the terminal's colors; `[search]` the search's
//! highlights; `[interface]` the interface's tokens by their names; and
//! each other section the tokens of its name, as `[tabs]` the tab bar's,
//! `tabs.*`.
//!
//! ```toml
//! [theme]
//! name = "Nightfall"
//! variant = "dark"
//! extends = "Lokyr Dark"
//!
//! [terminal]
//! background = "#1e1e20"
//! foreground = "#e6e6e6"
//!
//! [tabs]
//! bar.bg = "#18181a"
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

pub use lokyr_plugin_sdk::theme::{TerminalColors, Theme, ThemeColor, ThemeVariant};
use toml_edit::{Document, Item, TableLike, Value};

use crate::config::{Error, Position};

pub mod itermcolors;

/// The built-in theme of the light appearance's name.
pub const LIGHT: &str = "Lokyr Light";

/// The built-in theme of the dark appearance's name.
pub const DARK: &str = "Lokyr Dark";

/// The themes' folder, beside the config file.
pub const DIRECTORY: &str = "themes";

/// How deep a theme's `extends` goes, past which it is an error, as a
/// theme's chain may loop through files that change.
const MOST_EXTENDED: usize = 8;

/// The themes' folder of the config file at `config`.
pub fn directory(config: &Path) -> Option<PathBuf> {
    Some(config.parent()?.join(DIRECTORY))
}

/// A theme as it resolved: the theme, whole, and the files that its name
/// and those that it extends were looked for at, there or not, which a
/// watch of them tells the changes of.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// The theme.
    pub theme: Theme,
    /// The files looked for.
    pub files: Vec<PathBuf>,
}

/// The theme of `name`: its file of the themes' folder `directory`, if
/// there is one, or else the native file of that name of `held`, the
/// folder of the themes that the app holds, or else the built-in one of
/// that name among `builtins`; of the theme that it extends, whose values
/// its own override, as deep as it goes. The error of a file tells its
/// place in it.
pub fn resolve(
    name: &str,
    directory: Option<&Path>,
    held: Option<&Path>,
    builtins: &[Theme],
) -> Result<Resolved, Error> {
    let sources = Sources {
        directory,
        held,
        builtins,
    };
    let mut files = Vec::new();
    let mut chain = Vec::new();
    let partial = load(name, &sources, &mut files, &mut chain)?;
    let theme = partial.finish(name, files.first().map(PathBuf::as_path))?;
    Ok(Resolved { theme, files })
}

/// Whether `name` can name a theme: a file's name of the themes' folder,
/// not empty, of no separator of paths, and not hidden.
pub fn is_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.')
}

/// Where a theme's name is looked for: the themes' folder, whose files a
/// watch follows, the folder of the themes that the app holds, which never
/// change, and the built-in themes.
struct Sources<'a> {
    directory: Option<&'a Path>,
    held: Option<&'a Path>,
    builtins: &'a [Theme],
}

/// The partial theme of `name` and those that it extends, the files of the
/// themes' folder looked for added to `files`, the names of its chain so
/// far in `chain`.
fn load(
    name: &str,
    sources: &Sources<'_>,
    files: &mut Vec<PathBuf>,
    chain: &mut Vec<String>,
) -> Result<Partial, Error> {
    let not_found = |path: Option<&Path>| Error {
        path: path.map_or_else(|| PathBuf::from(name), Path::to_path_buf),
        position: None,
        message: format!(
            "there is no theme `{name}`, of a file of the themes' folder, of Lokyr's own or built in"
        ),
    };
    if !is_name(name) {
        return Err(Error {
            path: PathBuf::from(name),
            position: None,
            message: format!(
                "`{name}` is no theme's name, which names a file of the themes' folder"
            ),
        });
    }
    chain.push(name.to_owned());
    let path = sources
        .directory
        .map(|directory| directory.join(format!("{name}.toml")));
    if let Some(path) = &path {
        files.push(path.clone());
        if let Some(partial) = load_native(path, sources, files, chain)? {
            return Ok(partial);
        }
    }
    if let Some(directory) = sources.directory
        && let Some(theme) = load_itermcolors(name, directory, files)?
    {
        return Ok(Partial::of(&theme));
    }
    if let Some(held) = sources.held
        && let Some(partial) =
            load_native(&held.join(format!("{name}.toml")), sources, files, chain)?
    {
        return Ok(partial);
    }
    sources
        .builtins
        .iter()
        .find(|theme| theme.name == name)
        .map(Partial::of)
        .ok_or_else(|| not_found(path.as_deref()))
}

/// The partial theme of the native file at `path` and those that it
/// extends, as [`load`] finds them; `None` of no file there.
fn load_native(
    path: &Path,
    sources: &Sources<'_>,
    files: &mut Vec<PathBuf>,
    chain: &mut Vec<String>,
) -> Result<Option<Partial>, Error> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Error {
                path: path.to_owned(),
                position: None,
                message: format!("it cannot be read: {error}"),
            });
        }
    };
    let file = parse(&text, path)?;
    let base = match &file.extends {
        Some((parent, span)) => {
            let at = || Some(Position::at(&text, span.start));
            if chain.iter().any(|known| known == parent) {
                return Err(Error {
                    path: path.to_owned(),
                    position: at(),
                    message: format!("`{parent}` extends this theme, which extends it"),
                });
            }
            if chain.len() >= MOST_EXTENDED {
                return Err(Error {
                    path: path.to_owned(),
                    position: at(),
                    message: format!(
                        "a theme extends {MOST_EXTENDED} others at most, one after another"
                    ),
                });
            }
            Some(load(parent, sources, files, chain)?)
        }
        None => None,
    };
    Ok(Some(file.partial.over(base)))
}

/// The theme of `name` of an `.itermcolors` file of `directory`, read as
/// it is: `<name>.itermcolors`, or the light or the dark variant of
/// `<base>.itermcolors` of a name `<base> (Light)` or `<base> (Dark)`;
/// `None` of no such file. The files looked for go into `files`.
fn load_itermcolors(
    name: &str,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<Option<Theme>, Error> {
    let base = [" (Light)", " (Dark)"]
        .into_iter()
        .find_map(|suffix| name.strip_suffix(suffix));
    for stem in std::iter::once(name).chain(base) {
        let path = directory.join(format!("{stem}.itermcolors"));
        files.push(path.clone());
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(Error {
                    path,
                    position: None,
                    message: format!("it cannot be read: {error}"),
                });
            }
        };
        let themes = itermcolors::read(&bytes, stem, &path)?;
        if let Some(theme) = themes.iter().find(|theme| theme.name == name) {
            return Ok(Some(theme.clone()));
        }
        if stem != name {
            continue;
        }
        // A file of both appearances' keys, named without one.
        let names: Vec<String> = themes
            .iter()
            .map(|theme| format!("`{}`", theme.name))
            .collect();
        return Err(Error {
            path,
            position: None,
            message: format!(
                "the file holds the themes {}, of the light and the dark appearance: name one of them",
                names.join(" and ")
            ),
        });
    }
    Ok(None)
}

/// What a theme tells people of itself, which its file of the native
/// format writes in `[theme]`, and which changes nothing that it colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Credits<'a> {
    /// Who made it.
    pub author: Option<&'a str>,
    /// Its license, an SPDX expression.
    pub license: Option<&'a str>,
    /// Where it comes from.
    pub source: Option<&'a str>,
}

/// The text of a file of the native format of `theme`, whole, with its
/// `credits`, which reads back as the same theme: the search's tokens in
/// `[search]`, and any other in `[interface]`, by its whole name.
pub fn native(theme: &Theme, credits: Credits<'_>) -> String {
    let mut text = String::from("[theme]\n");
    let mut line = |key: &str, value: &str| {
        text.push_str(&format!("{key} = {}\n", quoted(value)));
    };
    line("name", &theme.name);
    for (key, value) in [
        ("author", credits.author),
        ("license", credits.license),
        ("source", credits.source),
    ] {
        if let Some(value) = value {
            line(key, value);
        }
    }
    line(
        "variant",
        match theme.variant {
            ThemeVariant::Light => "light",
            ThemeVariant::Dark => "dark",
        },
    );
    let terminal = &theme.terminal;
    text.push_str(&format!(
        "\n[terminal]\nbackground = \"{}\"\nforeground = \"{}\"\npalette = [\n",
        terminal.background, terminal.foreground
    ));
    for row in terminal.palette.chunks(8) {
        let colors: Vec<String> = row.iter().map(|color| format!("\"{color}\"")).collect();
        text.push_str(&format!("    {},\n", colors.join(", ")));
    }
    text.push_str("]\n");
    for (key, color) in [
        ("cursor", terminal.cursor),
        ("cursor-text", terminal.cursor_text),
        ("selection", terminal.selection),
        ("selection-text", terminal.selection_text),
        ("bold", terminal.bold),
        ("link", terminal.link),
        ("underline", terminal.underline),
    ] {
        if let Some(color) = color {
            text.push_str(&format!("{key} = \"{color}\"\n"));
        }
    }
    let search: Vec<(&str, ThemeColor)> = [
        ("match", "search.match"),
        ("current", "search.match.current"),
    ]
    .into_iter()
    .filter_map(|(key, token)| theme.tokens.get(token).map(|color| (key, *color)))
    .collect();
    if !search.is_empty() {
        text.push_str("\n[search]\n");
        for (key, color) in search {
            text.push_str(&format!("{key} = \"{color}\"\n"));
        }
    }
    let others: Vec<(&String, &ThemeColor)> = theme
        .tokens
        .iter()
        .filter(|(token, _)| !matches!(token.as_str(), "search.match" | "search.match.current"))
        .collect();
    if !others.is_empty() {
        text.push_str("\n[interface]\n");
        for (token, color) in others {
            text.push_str(&format!("{} = \"{color}\"\n", quoted(token)));
        }
    }
    text
}

/// `text` as a basic string of TOML, of its escapes.
fn quoted(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => quoted.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// The extensions of the themes' files: the native format's and
/// `.itermcolors`.
pub const EXTENSIONS: [&str; 2] = ["toml", "itermcolors"];

/// The themes of the file at `path`, of either format, as a theme's file
/// to install: each one's name, as `[theme]` would name it once the file
/// is in the themes' folder, and its variant. A native file is one theme,
/// of its file's name, of the variant that it says, or of its colors, or
/// dark of neither, as the theme that it extends may give them; a file of
/// another extension, or that is no theme, is an error.
pub fn themes_of(path: &Path) -> Result<Vec<(String, ThemeVariant)>, Error> {
    let error = |message: String| Error {
        path: path.to_owned(),
        position: None,
        message,
    };
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| error("its name is no theme's".to_owned()))?;
    let extension = path.extension().and_then(|extension| extension.to_str());
    let read_error = |e: io::Error| error(format!("it cannot be read: {e}"));
    match extension {
        Some("itermcolors") => {
            let bytes = fs::read(path).map_err(read_error)?;
            Ok(itermcolors::read(&bytes, stem, path)?
                .into_iter()
                .map(|theme| (theme.name, theme.variant))
                .collect())
        }
        Some("toml") => {
            let text = fs::read_to_string(path).map_err(read_error)?;
            let file = parse(&text, path)?;
            let partial = file.partial;
            let variant =
                partial
                    .variant
                    .unwrap_or(match (partial.background, partial.foreground) {
                        (Some(background), Some(foreground))
                            if lightness(background) > lightness(foreground) =>
                        {
                            ThemeVariant::Light
                        }
                        _ => ThemeVariant::Dark,
                    });
            Ok(vec![(stem.to_owned(), variant)])
        }
        _ => Err(error(format!(
            "it is no theme's file, of {}",
            EXTENSIONS
                .map(|extension| format!(".{extension}"))
                .join(" or ")
        ))),
    }
}

/// What a file of the native format gives: its partial theme, and the
/// theme that it extends, with the place of its name.
#[derive(Debug)]
struct File {
    partial: Partial,
    extends: Option<(String, Range<usize>)>,
}

/// A theme of what it gives alone, which the themes that it extends fill.
#[derive(Clone, Debug, Default, PartialEq)]
struct Partial {
    name: Option<String>,
    variant: Option<ThemeVariant>,
    background: Option<ThemeColor>,
    foreground: Option<ThemeColor>,
    palette: Option<[ThemeColor; 16]>,
    cursor: Option<ThemeColor>,
    cursor_text: Option<ThemeColor>,
    selection: Option<ThemeColor>,
    selection_text: Option<ThemeColor>,
    bold: Option<ThemeColor>,
    link: Option<ThemeColor>,
    underline: Option<ThemeColor>,
    tokens: BTreeMap<String, ThemeColor>,
}

impl Partial {
    /// The partial theme of a whole one.
    fn of(theme: &Theme) -> Self {
        let terminal = &theme.terminal;
        Self {
            name: Some(theme.name.clone()),
            variant: Some(theme.variant),
            background: Some(terminal.background),
            foreground: Some(terminal.foreground),
            palette: Some(terminal.palette),
            cursor: terminal.cursor,
            cursor_text: terminal.cursor_text,
            selection: terminal.selection,
            selection_text: terminal.selection_text,
            bold: terminal.bold,
            link: terminal.link,
            underline: terminal.underline,
            tokens: theme.tokens.clone(),
        }
    }

    /// This theme over `base`, the one that it extends: its values where it
    /// gives them, the base's otherwise, its tokens over the base's.
    fn over(self, base: Option<Self>) -> Self {
        let Some(base) = base else {
            return self;
        };
        let mut tokens = base.tokens;
        tokens.extend(self.tokens);
        Self {
            name: self.name.or(base.name),
            variant: self.variant.or(base.variant),
            background: self.background.or(base.background),
            foreground: self.foreground.or(base.foreground),
            palette: self.palette.or(base.palette),
            cursor: self.cursor.or(base.cursor),
            cursor_text: self.cursor_text.or(base.cursor_text),
            selection: self.selection.or(base.selection),
            selection_text: self.selection_text.or(base.selection_text),
            bold: self.bold.or(base.bold),
            link: self.link.or(base.link),
            underline: self.underline.or(base.underline),
            tokens,
        }
    }

    /// The whole theme of `name`, as `path`'s file names it, if it has its
    /// terminal's background, foreground and palette; of the light variant
    /// where it says none and its background is the lighter.
    fn finish(self, name: &str, path: Option<&Path>) -> Result<Theme, Error> {
        let missing = |what: &str| Error {
            path: path.map_or_else(|| PathBuf::from(name), Path::to_path_buf),
            position: None,
            message: format!(
                "the theme `{name}` gives no {what} of `[terminal]`, nor does one that it extends"
            ),
        };
        let background = self.background.ok_or_else(|| missing("background"))?;
        let foreground = self.foreground.ok_or_else(|| missing("foreground"))?;
        let palette = self.palette.ok_or_else(|| missing("palette"))?;
        let variant = self
            .variant
            .unwrap_or(if lightness(background) > lightness(foreground) {
                ThemeVariant::Light
            } else {
                ThemeVariant::Dark
            });
        Ok(Theme {
            name: name.to_owned(),
            variant,
            terminal: TerminalColors {
                background,
                foreground,
                palette,
                cursor: self.cursor,
                cursor_text: self.cursor_text,
                selection: self.selection,
                selection_text: self.selection_text,
                bold: self.bold,
                link: self.link,
                underline: self.underline,
            },
            tokens: self.tokens,
        })
    }
}

/// A color's lightness, of its components' sum, which tells the lighter of
/// two colors.
fn lightness(color: ThemeColor) -> u32 {
    u32::from(color.red) + u32::from(color.green) + u32::from(color.blue)
}

/// The keys of `[theme]`.
const THEME_KEYS: [&str; 6] = ["name", "author", "license", "source", "variant", "extends"];

/// The keys of `[terminal]` of one color each.
const TERMINAL_COLORS: [&str; 9] = [
    "background",
    "foreground",
    "cursor",
    "cursor-text",
    "selection",
    "selection-text",
    "bold",
    "link",
    "underline",
];

/// Reads the text of a theme's file, which `path` names in its errors.
fn parse(text: &str, path: &Path) -> Result<File, Error> {
    let error = |span: Option<Range<usize>>, message: String| Error {
        path: path.to_owned(),
        position: span.map(|span| Position::at(text, span.start)),
        message,
    };
    let document = Document::parse(text).map_err(|e| error(e.span(), e.message().to_owned()))?;
    let mut partial = Partial::default();
    let mut extends = None;
    for (section, item) in document.as_table() {
        let key_span = document
            .as_table()
            .key(section)
            .and_then(toml_edit::Key::span);
        let Some(table) = item.as_table_like() else {
            return Err(error(
                key_span,
                format!("`{section}` is a key of no section: a theme's keys are in its sections"),
            ));
        };
        match section {
            "theme" => {
                for (key, value) in table.iter() {
                    let span = value
                        .span()
                        .or_else(|| table.key(key).and_then(toml_edit::Key::span));
                    if !THEME_KEYS.contains(&key) {
                        return Err(error(
                            table.key(key).and_then(toml_edit::Key::span),
                            format!(
                                "unknown key `{key}` in `[theme]`, of {}",
                                THEME_KEYS.join(", ")
                            ),
                        ));
                    }
                    let text = value.as_str().ok_or_else(|| {
                        error(span.clone(), format!("`{key}` of `[theme]` is a string"))
                    })?;
                    match key {
                        "name" => partial.name = Some(text.to_owned()),
                        "variant" => {
                            partial.variant = Some(match text {
                                "light" => ThemeVariant::Light,
                                "dark" => ThemeVariant::Dark,
                                _ => {
                                    return Err(error(
                                        span,
                                        format!(
                                            "`variant` is \"light\" or \"dark\", not \"{text}\""
                                        ),
                                    ));
                                }
                            });
                        }
                        "extends" => extends = Some((text.to_owned(), span.unwrap_or(0..0))),
                        // The author, the license and the source tell people.
                        _ => {}
                    }
                }
            }
            "terminal" => {
                for (key, value) in table.iter() {
                    let span = value.span();
                    if key == "palette" {
                        partial.palette = Some(palette(value, &error)?);
                        continue;
                    }
                    if !TERMINAL_COLORS.contains(&key) {
                        return Err(error(
                            table.key(key).and_then(toml_edit::Key::span),
                            format!(
                                "unknown key `{key}` in `[terminal]`, of {} and palette",
                                TERMINAL_COLORS.join(", ")
                            ),
                        ));
                    }
                    let color = Some(color(value, span, &error)?);
                    match key {
                        "background" => partial.background = color,
                        "foreground" => partial.foreground = color,
                        "cursor" => partial.cursor = color,
                        "cursor-text" => partial.cursor_text = color,
                        "selection" => partial.selection = color,
                        "selection-text" => partial.selection_text = color,
                        "bold" => partial.bold = color,
                        "link" => partial.link = color,
                        _ => partial.underline = color,
                    }
                }
            }
            "search" => {
                for (key, value) in table.iter() {
                    let token = match key {
                        "match" => "search.match",
                        "current" => "search.match.current",
                        _ => {
                            return Err(error(
                                table.key(key).and_then(toml_edit::Key::span),
                                format!("unknown key `{key}` in `[search]`, of match and current"),
                            ));
                        }
                    };
                    let color = color(value, value.span(), &error)?;
                    partial.tokens.insert(token.to_owned(), color);
                }
            }
            // The interface's tokens by their names; each other section the
            // tokens of its own name.
            _ => {
                let prefix = if section == "interface" {
                    String::new()
                } else {
                    format!("{section}.")
                };
                flatten(table, &prefix, &mut partial.tokens, &error)?;
            }
        }
    }
    Ok(File { partial, extends })
}

/// The tokens of `table`, each by its key's path after `prefix`, as
/// `active.bg` of `[tabs]` `tabs.active.bg`, into `tokens`.
fn flatten(
    table: &dyn TableLike,
    prefix: &str,
    tokens: &mut BTreeMap<String, ThemeColor>,
    error: &dyn Fn(Option<Range<usize>>, String) -> Error,
) -> Result<(), Error> {
    for (key, item) in table.iter() {
        let name = format!("{prefix}{key}");
        match item.as_table_like() {
            Some(inner) => flatten(inner, &format!("{name}."), tokens, error)?,
            None => {
                tokens.insert(name, color(item, item.span(), error)?);
            }
        }
    }
    Ok(())
}

/// The color of `item`, a string of `#rrggbb` or `#rrggbbaa`.
fn color(
    item: &Item,
    span: Option<Range<usize>>,
    error: &dyn Fn(Option<Range<usize>>, String) -> Error,
) -> Result<ThemeColor, Error> {
    let text = item.as_str().ok_or_else(|| {
        error(
            span.clone(),
            "a color is a string, as \"#1e1e20\"".to_owned(),
        )
    })?;
    text.parse()
        .map_err(|e: lokyr_plugin_sdk::theme::ColorError| error(span, e.to_string()))
}

/// The 16 colors of `item`, an array of colors.
fn palette(
    item: &Item,
    error: &dyn Fn(Option<Range<usize>>, String) -> Error,
) -> Result<[ThemeColor; 16], Error> {
    let span = item.span();
    let Some(array) = item.as_array() else {
        return Err(error(
            span,
            "`palette` is an array of the 16 colors".to_owned(),
        ));
    };
    if array.len() != 16 {
        return Err(error(
            span,
            format!(
                "`palette` holds the 16 colors, from black to bright white, not {}",
                array.len()
            ),
        ));
    }
    let mut colors = [ThemeColor::rgb(0, 0, 0); 16];
    for (entry, value) in colors.iter_mut().zip(array.iter()) {
        let text = match value {
            Value::String(text) => text.value(),
            other => {
                return Err(error(
                    other.span(),
                    "a color is a string, as \"#1e1e20\"".to_owned(),
                ));
            }
        };
        *entry = text
            .parse()
            .map_err(|e: lokyr_plugin_sdk::theme::ColorError| error(value.span(), e.to_string()))?;
    }
    Ok(colors)
}

#[cfg(test)]
mod tests;

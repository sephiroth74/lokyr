//! The config file: where it is, and its sections, read whole or not at all.
//!
//! ```toml
//! config_version = 1   # the version of the file's format
//!
//! [font]
//! family = "CaskaydiaCove NFM"
//! style = "SemiBold"
//! size = 22
//!
//! [cursor]
//! shape = "bar"
//!
//! [window]
//! close-on-exit = "clean"   # clean | always | never
//!
//! [keyboard]
//! option-as-meta = "none"   # none | left | right | both
//! ```
//!
//! Each section documents its keys: `[font]` is [`FontConfig`], `[cursor]`
//! [`CursorConfig`], `[window]` [`WindowConfig`], and `[keyboard]`
//! [`KeyboardConfig`]. Every key
//! and every section can be left out, for its default. An error, of the
//! syntax, of a key or of a value, tells its place in the file, and a key
//! that no section has tells the nearest that it has.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bolt_text_core::FontConfig;
use bolt_ui_core::cursor::CursorConfig;
use serde::Deserialize;
use serde::de::{self, Visitor};
use toml_edit::{Document, Item, TableLike};

/// The version of the file's format that the app reads.
pub const VERSION: i64 = 1;

/// The app's directory in the user's config directory.
const DIRECTORY: &str = "bolt";

/// The config file's name.
const FILE: &str = "config.toml";

/// The config: its sections, each with the defaults of what the file leaves
/// out.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// The version of the file's format, which can only be the app's.
    #[serde(rename = "config_version")]
    version: Version,
    /// The `[font]` section.
    pub font: FontConfig,
    /// The `[cursor]` section.
    pub cursor: CursorConfig,
    /// The `[window]` section.
    pub window: WindowConfig,
    /// The `[keyboard]` section.
    pub keyboard: KeyboardConfig,
}

/// The `[keyboard]` section: how keys reach programs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct KeyboardConfig {
    /// Which Option keys act as Meta, sending an escape before the key,
    /// rather than typing the characters that the layout gives them.
    pub option_as_meta: OptionAsMeta,
}

/// Which Option keys act as Meta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OptionAsMeta {
    /// Neither, as Option types `@ # [ ] { } ~` on many layouts, the
    /// Italian one among them.
    #[default]
    None,
    /// The left one.
    Left,
    /// The right one.
    Right,
    /// Both.
    Both,
}

impl OptionAsMeta {
    /// Whether the left Option, and the right one, act as Meta.
    pub fn sides(self) -> (bool, bool) {
        match self {
            Self::None => (false, false),
            Self::Left => (true, false),
            Self::Right => (false, true),
            Self::Both => (true, true),
        }
    }
}

/// The `[window]` section: what a window does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowConfig {
    /// What a window does when its program ends.
    pub close_on_exit: CloseOnExit,
}

/// What a window does when its program ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloseOnExit {
    /// It closes when the program exits with 0, and otherwise keeps its
    /// screen, with a line that tells how the program ended, until a key
    /// closes it.
    #[default]
    Clean,
    /// It closes.
    Always,
    /// It keeps its screen, with the line, until a key closes it.
    Never,
}

/// The version of the file's format: [`VERSION`], the only one so far.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(try_from = "i64")]
struct Version;

impl TryFrom<i64> for Version {
    type Error = String;

    fn try_from(version: i64) -> Result<Self, Self::Error> {
        if version == VERSION {
            Ok(Self)
        } else {
            Err(format!(
                "the config's version is {version}, and the app reads version {VERSION}"
            ))
        }
    }
}

impl Config {
    /// Reads the config file at `path`; the defaults when there is none.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(Error {
                    path: path.to_owned(),
                    position: None,
                    message: format!("it cannot be read: {error}"),
                });
            }
        };
        let text = String::from_utf8(bytes).map_err(|error| {
            let valid = &error.as_bytes()[..error.utf8_error().valid_up_to()];
            let valid = std::str::from_utf8(valid).expect("the bytes before the error are valid");
            Error {
                path: path.to_owned(),
                position: Some(Position::at(valid, valid.len())),
                message: "the file is not UTF-8 text".to_owned(),
            }
        })?;
        Self::parse(&text, path)
    }

    /// Reads the text of a config file, which `path` names in its errors.
    pub fn parse(text: &str, path: &Path) -> Result<Self, Error> {
        let error = |span: Option<std::ops::Range<usize>>, message: &str| Error {
            path: path.to_owned(),
            position: span.map(|span| Position::at(text, span.start)),
            message: message.to_owned(),
        };
        let document = Document::parse(text).map_err(|e| error(e.span(), e.message()))?;
        check_keys(document.as_table()).map_err(|(span, message)| error(span, &message))?;
        Self::deserialize(toml_edit::de::Deserializer::from(document))
            .map_err(|e| error(e.span(), e.message()))
    }
}

/// The config file's path: in `$XDG_CONFIG_HOME` when it is set, or else in
/// `~/.config`, on macOS too; `None` without a home directory.
pub fn path() -> Option<PathBuf> {
    path_in(std::env::var_os("XDG_CONFIG_HOME"), std::env::home_dir())
}

fn path_in(config_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    // The XDG Base Directory Specification ignores a path that is not
    // absolute: https://specifications.freedesktop.org/basedir-spec/latest/
    let absolute = |path: &PathBuf| path.is_absolute();
    let base = config_home
        .map(PathBuf::from)
        .filter(absolute)
        .or_else(|| home.filter(absolute).map(|home| home.join(".config")))?;
    Some(base.join(DIRECTORY).join(FILE))
}

/// An error of the config file, at its place in the file when it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    /// The file.
    pub path: PathBuf,
    /// Where the error is in the file.
    pub position: Option<Position>,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path.display())?;
        if let Some(Position { line, column }) = self.position {
            write!(f, ":{line}:{column}")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for Error {}

/// A place in a file: its line and its column, in characters, both from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// The line.
    pub line: usize,
    /// The column, in characters.
    pub column: usize,
}

impl Position {
    /// The place of the byte at `offset` in `text`.
    fn at(text: &str, offset: usize) -> Self {
        let mut offset = offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        let before = &text[..offset];
        let start = before.rfind('\n').map_or(0, |newline| newline + 1);
        Self {
            line: before.matches('\n').count() + 1,
            column: before[start..].chars().count() + 1,
        }
    }
}

/// The sections of the file, with the keys of each.
fn sections() -> [(&'static str, &'static [&'static str]); 4] {
    [
        ("font", keys::<FontConfig>()),
        ("cursor", keys::<CursorConfig>()),
        ("window", keys::<WindowConfig>()),
        ("keyboard", keys::<KeyboardConfig>()),
    ]
}

/// Fails at the first key that the file's sections do not have, with the
/// nearest that they have, or at a section given as a value.
fn check_keys(root: &dyn TableLike) -> Result<(), (Option<std::ops::Range<usize>>, String)> {
    let top = keys::<Config>();
    let sections = sections();
    for (name, item) in root.iter() {
        let key = root.key(name).expect("a key of the table");
        if !top.contains(&name) {
            let message = if matches!(item, Item::Table(_)) {
                format!("unknown section `[{name}]`{}", nearest(name, top, "[", "]"))
            } else {
                format!("unknown key `{name}`{}", nearest(name, top, "", ""))
            };
            return Err((key.span(), message));
        }
        let Some((_, known)) = sections.iter().find(|(section, _)| *section == name) else {
            continue;
        };
        let Some(section) = item.as_table_like() else {
            return Err((item.span(), format!("`{name}` is a section, not a value")));
        };
        for (inner, _) in section.iter() {
            if !known.contains(&inner) {
                let key = section.key(inner).expect("a key of the table");
                let message = format!(
                    "unknown key `{inner}` in `[{name}]`{}",
                    nearest(inner, known, "", "")
                );
                return Err((key.span(), message));
            }
        }
    }
    Ok(())
}

/// `; did you mean …?`, with the known name nearest to `name`, when one is
/// near enough to be a typing error; nothing otherwise.
fn nearest(name: &str, known: &[&str], open: &str, close: &str) -> String {
    let limit = (name.chars().count() / 3).max(2);
    known
        .iter()
        .map(|candidate| (distance(name, candidate), candidate))
        .filter(|&(distance, _)| distance <= limit)
        .min_by_key(|&(distance, _)| distance)
        .map(|(_, candidate)| format!("; did you mean `{open}{candidate}{close}`?"))
        .unwrap_or_default()
}

/// The edit distance of `a` and `b`, in characters: Levenshtein's.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, a) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &b) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (diagonal + usize::from(a != b))
                .min(above + 1)
                .min(row[j] + 1);
            diagonal = above;
        }
    }
    row[b.len()]
}

/// The keys of a section, as serde names them: the fields that a derived
/// `Deserialize` hands to `deserialize_struct`.
fn keys<T: for<'de> Deserialize<'de>>() -> &'static [&'static str] {
    struct Fields<'a>(&'a mut &'static [&'static str]);

    impl<'de> de::Deserializer<'de> for Fields<'_> {
        type Error = de::value::Error;

        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
            Err(de::Error::custom("not a struct"))
        }

        fn deserialize_struct<V: Visitor<'de>>(
            self,
            _: &'static str,
            fields: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            *self.0 = fields;
            Err(de::Error::custom("only its fields"))
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes
            byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map
            enum identifier ignored_any
        }
    }

    let mut fields: &'static [&'static str] = &[];
    let _ = T::deserialize(Fields(&mut fields));
    fields
}

#[cfg(test)]
mod tests {
    use bolt_text_core::{Factor, Name, Size};
    use bolt_ui_core::cursor::{Shape, Thickness};

    use super::*;

    fn parse(text: &str) -> Result<Config, Error> {
        Config::parse(text, Path::new("config.toml"))
    }

    /// The error of `text`, with its line and column, and its message.
    fn error(text: &str) -> (Option<(usize, usize)>, String) {
        let error = parse(text).expect_err("an invalid config");
        let position = error
            .position
            .map(|position| (position.line, position.column));
        (position, error.message)
    }

    fn name(name: &str) -> Option<Name> {
        Some(Name::try_from(name.to_owned()).unwrap())
    }

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(parse(""), Ok(Config::default()));
        assert_eq!(parse("# nothing\n\n[font]\n"), Ok(Config::default()));
    }

    #[test]
    fn a_full_config_reads() {
        let config = parse(
            r#"
            config_version = 1

            [font]
            family = "CaskaydiaCove NFM"
            size = 22
            style = "SemiBold"
            bold-style = "Bold"
            italic-style = "Italic"
            bold-italic-style = "Bold Italic"
            non-ascii-family = "Hiragino Sans"
            line-height = 1.2
            cell-width = 1.05
            ligatures = false

            [cursor]
            shape = "bar"
            blink = false
            thickness = 1.5

            [window]
            close-on-exit = "never"

            [keyboard]
            option-as-meta = "left"
            "#,
        )
        .unwrap();
        assert_eq!(config.window.close_on_exit, CloseOnExit::Never);
        assert_eq!(config.keyboard.option_as_meta.sides(), (true, false));
        assert_eq!(
            config.cursor,
            CursorConfig {
                shape: Shape::Bar,
                blink: false,
                thickness: Some(Thickness::try_from(1.5).unwrap()),
            }
        );
        let font = config.font;
        assert_eq!(font.family, name("CaskaydiaCove NFM"));
        assert_eq!(font.size, Size::try_from(22.0).unwrap());
        assert_eq!(font.style, name("SemiBold"));
        assert_eq!(font.bold_style, name("Bold"));
        assert_eq!(font.italic_style, name("Italic"));
        assert_eq!(font.bold_italic_style, name("Bold Italic"));
        assert_eq!(font.non_ascii_family, name("Hiragino Sans"));
        assert_eq!(font.line_height, Factor::try_from(1.2).unwrap());
        assert_eq!(font.cell_width, Factor::try_from(1.05).unwrap());
        assert!(!font.ligatures);
    }

    #[test]
    fn dotted_keys_and_inline_tables_read_as_sections() {
        let mut expected = Config::default();
        expected.font.size = Size::try_from(15.0).unwrap();
        assert_eq!(parse("font.size = 15"), Ok(expected.clone()));
        assert_eq!(parse("font = { size = 15 }"), Ok(expected));
    }

    #[test]
    fn a_syntax_error_tells_its_place() {
        let (position, _) = error("[font]\nsize = = 3\n");
        assert_eq!(position, Some((2, 8)));
        let (position, _) = error("[font\n");
        assert_eq!(position.map(|(line, _)| line), Some(1));
    }

    #[test]
    fn an_unknown_key_tells_the_nearest_known_one() {
        assert_eq!(
            error("[font]\nfamily = \"Menlo\"\n  sise = 3\n"),
            (
                Some((3, 3)),
                "unknown key `sise` in `[font]`; did you mean `size`?".to_owned()
            )
        );
        assert_eq!(
            error("[font]\nbold_style = \"Bold\"\n").1,
            "unknown key `bold_style` in `[font]`; did you mean `bold-style`?"
        );
        assert_eq!(
            error("[font]\nweight = 600\n").1,
            "unknown key `weight` in `[font]`"
        );
        assert_eq!(
            error("[fonts]\nsize = 3\n"),
            (
                Some((1, 2)),
                "unknown section `[fonts]`; did you mean `[font]`?".to_owned()
            )
        );
        assert_eq!(
            error("config-version = 1\n").1,
            "unknown key `config-version`; did you mean `config_version`?"
        );
        // In a dotted key and in an inline table.
        assert_eq!(
            error("font.sise = 3\n"),
            (
                Some((1, 6)),
                "unknown key `sise` in `[font]`; did you mean `size`?".to_owned()
            )
        );
        assert_eq!(error("font = { sise = 3 }\n").0, Some((1, 10)));
    }

    #[test]
    fn a_wrong_value_tells_its_place() {
        assert_eq!(
            error("[font]\nsize = 3.5\n"),
            (
                Some((2, 8)),
                "the size, in points, is from 4 to 200, not 3.5".to_owned()
            )
        );
        let (position, message) = error("[font]\nsize = \"big\"\n");
        assert_eq!(position, Some((2, 8)));
        assert!(message.contains("string"), "{message}");
        let (position, _) = error("[font]\nfamily = \"  \"\n");
        assert_eq!(position, Some((2, 10)));
        assert_eq!(
            error("font = 3\n"),
            (Some((1, 8)), "`font` is a section, not a value".to_owned())
        );
    }

    #[test]
    fn the_cursor_s_shape_is_one_of_four() {
        for (name, shape) in [
            ("block", Shape::Block),
            ("bar", Shape::Bar),
            ("underline", Shape::Underline),
            ("hollow", Shape::Hollow),
        ] {
            let config = parse(&format!("[cursor]\nshape = \"{name}\"")).unwrap();
            assert_eq!(config.cursor.shape, shape);
        }
        let (position, message) = error("[cursor]\nshape = \"beam\"\n");
        assert_eq!(position, Some((2, 9)));
        assert!(message.contains("`block`, `bar`, `underline`"), "{message}");
        assert_eq!(
            error("[cursor]\nthickness = 30\n"),
            (
                Some((2, 13)),
                "the cursor's thickness, in points, is from 0.5 to 20, not 30".to_owned()
            )
        );
        assert_eq!(
            error("[cursor]\nblnk = true\n").1,
            "unknown key `blnk` in `[cursor]`; did you mean `blink`?"
        );
    }

    #[test]
    fn only_the_app_s_version_reads() {
        assert_eq!(parse("config_version = 1"), Ok(Config::default()));
        assert_eq!(
            error("\nconfig_version = 2\n"),
            (
                Some((2, 18)),
                "the config's version is 2, and the app reads version 1".to_owned()
            )
        );
    }

    #[test]
    fn columns_count_characters() {
        let (position, _) = error("[font]\nfamily = \"Écrire\" size = 3\n");
        assert_eq!(position, Some((2, 19)));
    }

    #[test]
    fn a_missing_file_is_the_defaults_and_an_unreadable_one_an_error() {
        let directory = std::env::temp_dir().join(format!("bolt-config-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        assert_eq!(
            Config::read(&directory.join("missing.toml")),
            Ok(Config::default())
        );
        // A directory cannot be read as a file.
        let error = Config::read(&directory).unwrap_err();
        assert_eq!(error.position, None);
        assert!(error.message.starts_with("it cannot be read"));

        let file = directory.join("config.toml");
        fs::write(&file, b"[font]\nfamily = \"a\xffb\"\n").unwrap();
        let error = Config::read(&file).unwrap_err();
        assert_eq!(error.path, file);
        assert_eq!(
            error.position,
            Some(Position {
                line: 2,
                column: 12
            })
        );
        assert_eq!(error.message, "the file is not UTF-8 text");
        assert_eq!(
            error.to_string(),
            format!("{}:2:12: the file is not UTF-8 text", file.display())
        );

        fs::write(&file, "[font]\nsize = 18\n").unwrap();
        assert_eq!(
            Config::read(&file).unwrap().font.size,
            Size::try_from(18.0).unwrap()
        );
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn the_file_is_in_the_xdg_config_home_or_in_dot_config() {
        let home = Some(PathBuf::from("/Users/me"));
        assert_eq!(
            path_in(None, home.clone()),
            Some(PathBuf::from("/Users/me/.config/bolt/config.toml"))
        );
        assert_eq!(
            path_in(Some("/tmp/xdg".into()), home.clone()),
            Some(PathBuf::from("/tmp/xdg/bolt/config.toml"))
        );
        // A relative or empty one is ignored.
        assert_eq!(
            path_in(Some("xdg".into()), home.clone()),
            path_in(None, home.clone())
        );
        assert_eq!(path_in(Some("".into()), home.clone()), path_in(None, home));
        assert_eq!(path_in(None, None), None);
        assert_eq!(path_in(None, Some(PathBuf::new())), None);
    }

    #[test]
    fn the_sections_keys_are_serde_s() {
        assert_eq!(
            keys::<Config>(),
            ["config_version", "font", "cursor", "window", "keyboard"]
        );
        assert_eq!(keys::<KeyboardConfig>(), ["option-as-meta"]);
        assert_eq!(OptionAsMeta::default().sides(), (false, false));
        assert_eq!(OptionAsMeta::Both.sides(), (true, true));
        assert_eq!(keys::<WindowConfig>(), ["close-on-exit"]);
        assert_eq!(keys::<CursorConfig>(), ["shape", "blink", "thickness"]);
        let font = keys::<FontConfig>();
        assert_eq!(font.len(), 10);
        assert!(font.contains(&"bold-italic-style"));
        assert!(font.contains(&"non-ascii-family"));
    }

    #[test]
    fn the_nearest_name_is_near_enough() {
        assert_eq!(distance("sise", "size"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(
            nearest("fmaily", &["family", "size"], "", ""),
            "; did you mean `family`?"
        );
        assert_eq!(nearest("colour", &["family", "size"], "", ""), "");
    }
}

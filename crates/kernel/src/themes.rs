//! Lokyr's themes, of `lokyr-themes`, of the config's errors: the native
//! format's parser and writer, iTerm2's `.itermcolors` files, and the
//! resolution of a theme's name of the themes' folder, of those that the
//! app holds, of the built-in ones and of the installed packages'.

use std::path::{Path, PathBuf};

pub use lokyr_themes::{
    Credits, DARK, DIRECTORY, EXTENSIONS, LIGHT, MOST_THEME_BYTES, MOST_THEME_FILES, Resolved,
    TerminalColors, Theme, ThemeColor, ThemeCredits, ThemeVariant, copy_of, directory, is_name,
    native, read_file,
};

use crate::config::Error;

/// The theme of `name`, as [`lokyr_themes::resolve`] finds it, of the
/// config's error.
pub fn resolve(
    name: &str,
    directory: Option<&Path>,
    held: Option<&Path>,
    builtins: &[Theme],
    installed: &[PathBuf],
) -> Result<Resolved, Error> {
    lokyr_themes::resolve(name, directory, held, builtins, installed).map_err(Error::from)
}

/// The themes of the file at `path`, as [`lokyr_themes::themes_of`] reads
/// them, of the config's error.
pub fn themes_of(path: &Path) -> Result<Vec<(String, ThemeVariant)>, Error> {
    lokyr_themes::themes_of(path).map_err(Error::from)
}

/// iTerm2's `.itermcolors` files, of the config's error.
pub mod itermcolors {
    use std::path::Path;

    use super::Theme;
    use crate::config::Error;

    /// The themes of the `.itermcolors` file of `bytes`, as
    /// [`lokyr_themes::itermcolors::read`] reads them.
    pub fn read(bytes: &[u8], name: &str, path: &Path) -> Result<Vec<Theme>, Error> {
        lokyr_themes::itermcolors::read(bytes, name, path).map_err(Error::from)
    }
}

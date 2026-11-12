//! The user's decisions on the capabilities that the installed plugins ask
//! for, `permissions.toml`, in the config's folder: of each plugin, by its
//! name, the capabilities granted and those refused, as the user answered
//! the app's questions or edited the file, which dotfiles can keep. A
//! capability of neither list is asked for at the plugin's activation.

use std::collections::BTreeMap;
use std::path::Path;

use lokyr_plugin_sdk::command::Capability;
use serde::Deserialize;

use crate::FileError;
use crate::manifest::{is_name, line_and_column};

/// The file's name, in the config's folder.
pub const FILE: &str = "permissions.toml";

/// The first lines of the file.
const HEADER: &str = "# The capabilities that the installed plugins hold, and those refused, as\n# the user answered Lokyr's questions. Remove one to be asked again.";

/// The decisions of every plugin, by its name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Permissions {
    /// Each plugin's, by its name, `owner/name`.
    pub plugins: BTreeMap<String, Decisions>,
}

/// A plugin's decisions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decisions {
    /// The capabilities granted, in their order, each once.
    pub granted: Vec<Capability>,
    /// The capabilities refused, in their order, each once.
    pub refused: Vec<Capability>,
}

impl Decisions {
    /// Grants `capabilities`, or refuses them, as the user answered, each
    /// of the one list alone.
    pub fn decide(&mut self, capabilities: &[Capability], granted: bool) {
        let (to, from) = if granted {
            (&mut self.granted, &mut self.refused)
        } else {
            (&mut self.refused, &mut self.granted)
        };
        from.retain(|capability| !capabilities.contains(capability));
        to.extend(capabilities.iter().cloned());
        to.sort();
        to.dedup();
    }
}

impl Permissions {
    /// The decisions of `text`, or why they do not read, and where.
    pub fn parse(text: &str) -> Result<Self, FileError> {
        let at = |span: std::ops::Range<usize>| Some(line_and_column(text, span.start));
        let fail = |span: std::ops::Range<usize>, message: String| FileError {
            at: at(span),
            message,
        };
        let file: File = toml::from_str(text).map_err(|error| crate::unread(text, &error))?;
        let mut permissions = Self::default();
        for (name, raw) in file.plugins {
            if !is_name(name.get_ref()) {
                return Err(fail(
                    name.span(),
                    format!("`{}` is not a plugin's name, `owner/name`", name.get_ref()),
                ));
            }
            let list = |texts: &[toml::Spanned<String>]| {
                let mut capabilities = Vec::new();
                for text in texts {
                    let capability = crate::capability_of(text.get_ref()).ok_or_else(|| {
                        fail(
                            text.span(),
                            format!("`{}` is no capability", text.get_ref()),
                        )
                    })?;
                    capabilities.push(capability);
                }
                capabilities.sort();
                capabilities.dedup();
                Ok::<_, FileError>(capabilities)
            };
            let decisions = Decisions {
                granted: list(&raw.granted)?,
                refused: list(&raw.refused)?,
            };
            if let Some(both) = raw.refused.iter().find(|text| {
                crate::capability_of(text.get_ref())
                    .is_some_and(|capability| decisions.granted.contains(&capability))
            }) {
                return Err(fail(
                    both.span(),
                    format!(
                        "`{}` is granted and refused, where it is one",
                        both.get_ref()
                    ),
                ));
            }
            permissions.plugins.insert(name.into_inner(), decisions);
        }
        Ok(permissions)
    }

    /// The decisions of the file at `path`: none of a file that is not
    /// there; or why they do not read, the file's path before its place.
    pub fn read(path: &Path) -> Result<Self, FileError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(FileError {
                    at: None,
                    message: format!("{} cannot be read: {error}", path.display()),
                });
            }
        };
        Self::parse(&text).map_err(|error| FileError {
            message: format!("{}: {}", path.display(), error.message),
            ..error
        })
    }

    /// The decisions as the file writes them, which [`Permissions::parse`]
    /// reads back.
    pub fn text(&self) -> String {
        let quoted = |text: &str| toml::Value::String(text.to_owned()).to_string();
        let list = |capabilities: &[Capability]| {
            let items: Vec<String> = capabilities
                .iter()
                .map(|capability| quoted(&capability.to_string()))
                .collect();
            format!("[{}]", items.join(", "))
        };
        let mut text = format!("{HEADER}\n");
        for (name, decisions) in &self.plugins {
            text.push_str(&format!(
                "\n[plugins.{}]\ngranted = {}\nrefused = {}\n",
                quoted(name),
                list(&decisions.granted),
                list(&decisions.refused)
            ));
        }
        text
    }

    /// Writes the decisions to `path`, whole.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        crate::write_whole(path, &self.text())
    }
}

/// The file as serde reads it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    plugins: BTreeMap<toml::Spanned<String>, RawDecisions>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDecisions {
    #[serde(default)]
    granted: Vec<toml::Spanned<String>>,
    #[serde(default)]
    refused: Vec<toml::Spanned<String>>,
}

#[cfg(test)]
mod tests;

//! The maintainer's variables that stay out of the repository, such as the
//! signing identity: the environment's, or else those of the untracked
//! `private/lokyr.env`, of lines `NAME=value`, `NAME="value"` or
//! `export NAME=value`, and comments of `#`, which `just` reads too.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result};

/// The file of the variables, relative to the workspace root.
const FILE: &str = "private/lokyr.env";

/// The value of the variable `name`: the environment's, or else the one
/// that `private/lokyr.env` gives it, if any.
pub fn variable(name: &str) -> Result<Option<String>> {
    if let Some(value) = std::env::var_os(name) {
        return value
            .into_string()
            .map(Some)
            .map_err(|_| anyhow::anyhow!("{name} is not UTF-8"));
    }
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(FILE);
    match fs::read_to_string(&file) {
        Ok(text) => Ok(given(&text, name)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("reading {}", file.display())),
    }
}

/// The value that the lines of `text` give `name`, the last one's: a
/// comment's key, of its `#`, is no name.
fn given(text: &str, name: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, value) = line.split_once('=')?;
            (key.trim() == name).then(|| unquoted(value.trim()))
        })
        .next_back()
}

/// `value` without the quotes, double or single, around it.
fn unquoted(value: &str) -> String {
    ['"', '\'']
        .iter()
        .find_map(|&quote| value.strip_prefix(quote)?.strip_suffix(quote))
        .unwrap_or(value)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_gives_its_variable_quoted_or_not() {
        let text = "# The identity.\n\
            LOKYR_SIGN_IDENTITY=\"Developer ID Application: The Maintainer (ABCDE12345)\"\n\
            export LOKYR_NOTARY_PROFILE='a-profile'\n\
            OTHER = plain \n";
        assert_eq!(
            given(text, "LOKYR_SIGN_IDENTITY").as_deref(),
            Some("Developer ID Application: The Maintainer (ABCDE12345)")
        );
        assert_eq!(
            given(text, "LOKYR_NOTARY_PROFILE").as_deref(),
            Some("a-profile")
        );
        assert_eq!(given(text, "OTHER").as_deref(), Some("plain"));
        assert_eq!(given(text, "MISSING"), None);
    }

    #[test]
    fn a_comment_gives_nothing_and_the_last_line_wins() {
        let text =
            "#LOKYR_SIGN_IDENTITY=commented\nLOKYR_SIGN_IDENTITY=first\nLOKYR_SIGN_IDENTITY=last\n";
        assert_eq!(given(text, "LOKYR_SIGN_IDENTITY").as_deref(), Some("last"));
        assert_eq!(
            given("# LOKYR_SIGN_IDENTITY=x\n", "LOKYR_SIGN_IDENTITY"),
            None
        );
    }
}

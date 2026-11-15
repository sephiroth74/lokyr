//! `cargo xtask theme-guide`: writes the table of the themes' tokens in the
//! authors' guide, `docs/themes/authoring.md`, of the rules of
//! `plugin-sdk`'s `theme/rules.toml`: each token, what it colors and its
//! rule, between the guide's two marks; with `--check`, tells whether the
//! guide's table is the rules', without writing it.

use std::fs;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use lokyr_plugin_sdk::theme::rules;

use crate::luau_types::workspace_root;

/// The guide, relative to the workspace root.
const GUIDE: &str = "docs/themes/authoring.md";

/// The mark before the table.
const START: &str =
    "<!-- The tokens' table, of the rules: `cargo xtask theme-guide` writes it. -->";

/// The mark after the table.
const END: &str = "<!-- The tokens' table's end. -->";

/// Runs the task with the remaining command-line arguments.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => bail!("unknown argument `{other}`"),
        }
    }
    let path = workspace_root().join(GUIDE);
    let held = fs::read_to_string(&path).with_context(|| format!("reading {GUIDE}"))?;
    let made = guide(&held)?;
    if check {
        if held != made {
            eprintln!(
                "{GUIDE}'s table of the tokens is not the rules': run `cargo xtask theme-guide`"
            );
            return Ok(ExitCode::FAILURE);
        }
        println!("{GUIDE}'s table of the tokens is the rules'");
        return Ok(ExitCode::SUCCESS);
    }
    fs::write(&path, made).with_context(|| format!("writing {GUIDE}"))?;
    println!("wrote {GUIDE}'s table of the tokens");
    Ok(ExitCode::SUCCESS)
}

/// The guide `held`, its table between the marks made anew of the rules.
fn guide(held: &str) -> Result<String> {
    let (Some(start), Some(end)) = (held.find(START), held.find(END)) else {
        bail!("{GUIDE} has not the marks of the tokens' table");
    };
    if end < start {
        bail!("{GUIDE}'s marks of the tokens' table are out of their order");
    }
    Ok(format!(
        "{}{START}\n\n{}\n{}",
        &held[..start],
        table(),
        &held[end..]
    ))
}

/// The table of the tokens, a row each, in the rules' order.
fn table() -> String {
    let code = |text: &str| format!("`{}`", text.replace('|', "\\|"));
    let mut out = String::from("| Token | What it colors | Its rule |\n|---|---|---|\n");
    for token in rules::tokens() {
        let Some(rule) = rules::rule(token) else {
            continue;
        };
        let (light, dark) = (rule.text(false), rule.text(true));
        let made = if light == dark {
            code(light)
        } else {
            format!("light {}, dark {}", code(light), code(dark))
        };
        let what = rule.description.trim_end_matches('.').replace('|', "\\|");
        out.push_str(&format!("| {} | {what} | {made} |\n", code(token)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_goes_between_the_marks_and_the_rest_stays() {
        let held = format!("# Guide\n\nBefore.\n\n{START}\n\nold\n{END}\n\nAfter.\n");
        let made = guide(&held).unwrap();
        assert!(made.starts_with("# Guide\n\nBefore.\n\n"));
        assert!(made.ends_with(&format!("{END}\n\nAfter.\n")));
        assert!(
            made.contains("| `text` | The interface's text | `foreground` |"),
            "{made}"
        );
        assert!(
            made.contains("| `focus.ring` | The ring of the keyboard's focus | light `accent / 50%`, dark `accent / 60%` |"),
            "{made}"
        );
        assert!(!made.contains("old"));
        // Made again, the same.
        assert_eq!(guide(&made).unwrap(), made);
        assert!(guide("no marks").is_err());
    }

    #[test]
    fn the_guide_s_table_is_the_rules() {
        let held = fs::read_to_string(workspace_root().join(GUIDE)).unwrap();
        assert_eq!(guide(&held).unwrap(), held, "run `cargo xtask theme-guide`");
    }
}

//! What the tasks do on GitHub's runners: the values that a log must not
//! tell, such as the signing identity, its team and a notarization's id,
//! masked by GitHub's `add-mask` command, before anything tells them.

/// Whether the task runs on GitHub's runners.
pub(crate) fn on_runner() -> bool {
    std::env::var_os("GITHUB_ACTIONS").is_some_and(|value| value == "true")
}

/// Has GitHub mask `value` in the rest of the job's log, on its runners;
/// does nothing elsewhere, nor of an empty value.
pub(crate) fn mask(value: &str) {
    for command in mask_commands(on_runner(), value) {
        println!("{command}");
    }
}

/// The commands that mask `value`, on GitHub's runners, `on_runner`: one a
/// line of it, as the command takes one line.
fn mask_commands(on_runner: bool, value: &str) -> Vec<String> {
    if !on_runner {
        return Vec::new();
    }
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("::add-mask::{line}"))
        .collect()
}

/// The team of a signing identity, as `Developer ID Application: Name
/// (TEAM)`, its last parentheses' text.
pub(crate) fn team_of(identity: &str) -> Option<&str> {
    let (_, team) = identity.trim_end().strip_suffix(')')?.rsplit_once('(')?;
    (!team.is_empty()).then_some(team)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_masked_on_a_runner_alone() {
        assert_eq!(mask_commands(true, "secret"), ["::add-mask::secret"]);
        assert_eq!(
            mask_commands(true, "one\n\ntwo"),
            ["::add-mask::one", "::add-mask::two"]
        );
        assert!(mask_commands(false, "secret").is_empty());
        assert!(mask_commands(true, "  ").is_empty());
    }

    #[test]
    fn an_identity_tells_its_team() {
        assert_eq!(
            team_of("Developer ID Application: The Maintainer (ABCDE12345)"),
            Some("ABCDE12345")
        );
        assert_eq!(team_of("-"), None);
    }
}
